// 业务模块在 lib.rs（wotb_agent::），本文件只保留 CLI 定义与命令分发。

use anyhow::Result;
use clap::{Parser as ClapParser, Subcommand};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use wotb_agent::agent::Agent;
use wotb_agent::models::config::{Config, TokenUsage};
use wotb_agent::models::report::AggregatedReport;
use wotb_agent::replay::combat::CombatTimeline;
use wotb_agent::replay::scanner::{ReplayScanner, ScanFilter};
use wotb_agent::wargaming::api_client::WgApiClient;
use wotb_agent::wargaming::snapshot::SnapshotStore;
use wotb_agent::wargaming::tank_resolver::TankResolver;

#[derive(ClapParser)]
#[command(
    name = "wotb-agent",
    version = "0.1.0",
    about = "WoTB Replay Analysis Agent"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// 字节序列的空格分隔 hex（RE 打印用）。
fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{:02x}", x))
        .collect::<Vec<_>>()
        .join(" ")
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// CombatTimeline 的 CLI 打印（表现层；核心库不绑定 IO，字段均 pub）。
fn print_combat_timeline(tl: &CombatTimeline) {
    println!();
    println!("--- Combat Event Timeline ---");
    println!("  Entities: {}", tl.entity_count);
    println!("  Deaths: {}", tl.death_count);
    println!("  Total damage tracked: {}", tl.total_damage_tracked);
    println!();

    if !tl.entity_names.is_empty() {
        println!("  Player mapping ({}):", tl.entity_names.len());
        for (eid, name) in tl.entity_names.iter() {
            println!("    0x{:08x} = {}", eid, name);
        }
        println!();
    }

    let deaths = tl.death_events();
    if !deaths.is_empty() {
        println!("  Deaths:");
        for (t, _, name) in &deaths {
            println!("    t={:.1}s  {}", t, name);
        }
        println!();
    }

    let health = tl.health_timeline();
    if !health.is_empty() {
        println!("  Health changes (damage > 100):");
        println!(
            "    {:>8} {:<25} {:>10} {:>8}",
            "Time", "Player", "Damage", "HP left"
        );
        println!("    {}", "-".repeat(55));
        for (t, _, name, hp, dmg) in health.iter().filter(|(_, _, _, _, d)| *d > 100) {
            println!("    {:>7.1}s {:<25} -{:>8} {:>8}", t, name, dmg, hp);
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Parse a single replay file
    Single {
        /// Path to the .wotbreplay file
        file: PathBuf,
        /// Output JSON instead of text
        #[arg(short, long)]
        json: bool,
        /// Tank cache file path (JSON)
        #[arg(long)]
        tank_cache: Option<PathBuf>,
    },
    /// Export authoritative ReplayDataset (metadata/settlement/diagnostics) as JSON
    Dataset {
        /// Path to the .wotbreplay file
        file: PathBuf,
        /// Tank cache file path (JSON)
        #[arg(long)]
        tank_cache: Option<PathBuf>,
    },
    /// Dump the map registry (id → key/space/display) as JSON for asset packing
    DumpMapIndex,
    /// Export per-tank data JSONs (armor/configs/origins) for static asset packing
    DumpTankData {
        /// Output directory for {tank_id}.json files
        #[arg(long, default_value = "data/tank_data")]
        out: PathBuf,
    },
    /// Export the global shell-id → shell-type table as JSON（射击复现弹种反解表；
    /// tanks.pb 全量展开，与回放 shell_id 同域——静态资产面前端常量生成入口）
    DumpShellKinds {
        /// Write JSON to file instead of stdout（资产打包器的取数入口）
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Export consumer facets for one replay (playback / ai-review JSON；HoF 是消费方投影，不导出)
    Facets {
        /// Path to the .wotbreplay file
        file: PathBuf,
        /// Comma-separated parts: playback,ai
        #[arg(long, default_value = "playback,ai")]
        parts: String,
        /// Output directory (default: alongside the replay file)
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Tank cache file path (JSON)
        #[arg(long)]
        tank_cache: Option<PathBuf>,
    },
    /// Scan a directory for replays and aggregate stats
    Scan {
        /// Directory containing .wotbreplay files
        dir: PathBuf,
        /// Filter by room type: all, rating, regular, training
        #[arg(short, long, default_value = "all")]
        mode: String,
        /// Only include replays from the last N days
        #[arg(short, long)]
        days: Option<i64>,
        /// Output JSON report to file
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Tank cache file path (JSON)
        #[arg(long)]
        tank_cache: Option<PathBuf>,
        /// Fetch tank data from WG API (requires application_id)
        #[arg(long)]
        fetch_tanks: bool,
        /// WG API application_id
        #[arg(long, env = "WG_APPLICATION_ID")]
        app_id: Option<String>,
        /// WG API server: asia, eu, na
        #[arg(long, default_value = "asia")]
        server: String,
    },
    /// Build tank resolver from local BlitzKit pb data (no WG API needed)
    FetchTanks {
        /// Output file path
        #[arg(short, long, default_value = "data/tank_cache.json")]
        output: PathBuf,
    },
    /// Analyze lineup strength of players for pre-match planning
    Prematch {
        /// Comma-separated player nicknames, or a .wotbreplay path to extract both teams
        nicknames: Option<String>,
        /// File containing player nicknames (one per line)
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Parse a .wotbreplay replay and analyze both teams (same as nicknames path)
        #[arg(short, long)]
        replay: Option<PathBuf>,
        /// WG API application_id
        #[arg(long, env = "WG_APPLICATION_ID")]
        app_id: String,
        /// WG API server
        #[arg(long, default_value = "asia")]
        server: String,
    },
    /// Take or view API data snapshots for time-series analysis
    Snapshot {
        /// WG API application_id
        #[arg(long, env = "WG_APPLICATION_ID")]
        app_id: String,
        /// WG API server
        #[arg(long, default_value = "asia")]
        server: String,
        /// Player nickname
        nickname: String,
        /// Action: take (new snapshot), list (show all), diff (compare oldest vs latest)
        #[arg(short, long, default_value = "take")]
        action: String,
        /// Snapshot storage directory
        #[arg(long, default_value = "data/snapshots")]
        dir: PathBuf,
    },
    /// Parse game data files (DVPL format) for armor/collision data
    ParseGame {
        /// Tank dev name (e.g. R132_T100LT)
        dev_name: String,
        /// Game data directory (auto-detected if omitted)
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Batch-extract armor/collision data from game DVPL files into game_data/ (portable)
    ExtractGame {
        /// Game data directory (auto-detected if omitted)
        #[arg(long)]
        game_dir: Option<PathBuf>,
        /// Output directory for extracted per-tank JSON files
        #[arg(long, default_value = "data/game_data")]
        output: PathBuf,
        /// Re-extract even if output file already exists
        #[arg(long)]
        force: bool,
    },
    /// Download BlitzKit tanks.pb (唯一坦克数据源) to data/
    FetchBlitzkit {
        /// Output file path（tanks.pb）
        #[arg(long, default_value = "data/tanks.pb")]
        output: PathBuf,
    },
    /// Batch-download all tank preview icons from BlitzKit into data/cache/tank_images/
    FetchIcons {
        /// Output directory for tank icons
        #[arg(long, default_value = "data/cache/tank_images")]
        dir: PathBuf,
        /// Re-download icons even if already present
        #[arg(long)]
        force: bool,
    },
    /// Batch-download ALL tank GLB models (model.glb + collision.glb) into
    /// data/cache/models/ so the viewer/playback work fully offline (~2 GB total)
    FetchModels {
        /// Re-download models even if already cached
        #[arg(long)]
        force: bool,
        /// Concurrent download tasks
        #[arg(long, default_value = "6")]
        concurrency: usize,
    },
    /// Pre-extract heightmap terrain cache for all maps into data/maps/_cache/
    FetchTerrain {
        /// Re-extract even if already cached
        #[arg(long)]
        force: bool,
    },
    /// Pre-extract minimap ground textures (low-quality playback ground) for all maps
    /// into data/cache/maps/
    FetchMinimaps {
        /// Re-extract even if already cached
        #[arg(long)]
        force: bool,
    },
    /// One-click update of all derived game data after a game patch
    /// (BlitzKit pb -> tank_cache.json -> game_data/), game-version aware
    UpdateData {
        /// Game data directory (auto-detected if omitted)
        #[arg(long)]
        game_dir: Option<PathBuf>,
        /// tanks.pb output path (models.pb is written alongside)
        #[arg(long, default_value = "data/tanks.pb")]
        output: PathBuf,
        /// tank_cache.json output path
        #[arg(long, default_value = "data/tank_cache.json")]
        tank_cache: PathBuf,
        /// game_data output directory
        #[arg(long, default_value = "data/game_data")]
        game_data: PathBuf,
        /// Skip BlitzKit download (rebuild from existing pb)
        #[arg(long)]
        offline: bool,
        /// Force full game_data re-extraction even if game version unchanged
        #[arg(long)]
        force: bool,
        /// Only report version status and planned actions, change nothing
        #[arg(long)]
        check: bool,
        /// Also download missing tank icons into data/cache/tank_images/
        #[arg(long)]
        icons: bool,
        /// Also pre-download missing tank GLB models into data/cache/models/ (~2 GB)
        #[arg(long)]
        models: bool,
    },
    /// View 3D tank model in browser
    View {
        /// Tank ID
        tank_id: u32,
        /// Tank cache file
        #[arg(long)]
        tank_cache: Option<PathBuf>,
    },
    /// Launch the Web Agent GUI (independent web app)
    Web {
        /// Config file path
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
        /// Tank cache file
        #[arg(long)]
        tank_cache: Option<PathBuf>,
    },
    /// Start interactive Agent chat
    Chat {
        /// Config file path
        #[arg(short, long, default_value = "config.toml")]
        config: PathBuf,
        /// Save session to JSON on exit
        #[arg(long)]
        save: Option<PathBuf>,
        /// Load session from JSON on start
        #[arg(long)]
        load: Option<PathBuf>,
    },
    /// Show or create config file
    Config {
        /// Config file path
        #[arg(short, long, default_value = "config.toml")]
        file: PathBuf,
        /// Show current config
        #[arg(long)]
        show: bool,
    },
    /// Show token usage statistics
    Usage {
        /// Usage file path
        #[arg(short, long, default_value = "data/token_usage.json")]
        file: PathBuf,
    },
    /// Query player stats from WG API
    Player {
        /// Player nickname (exact match)
        nickname: String,
        /// WG API application_id
        #[arg(long, env = "WG_APPLICATION_ID")]
        app_id: String,
        /// WG API server
        #[arg(long, default_value = "asia")]
        server: String,
    },
    /// Compare replay stats vs WG API cumulative stats
    Compare {
        /// Player nickname
        nickname: String,
        /// Replay directory
        dir: PathBuf,
        /// WG API application_id
        #[arg(long, env = "WG_APPLICATION_ID")]
        app_id: String,
        /// WG API server
        #[arg(long, default_value = "asia")]
        server: String,
        /// Tank cache file
        #[arg(long)]
        tank_cache: Option<PathBuf>,
        /// Filter by mode: rating, regular, all
        #[arg(short, long, default_value = "rating")]
        mode: String,
    },
    /// Analyze combat events from a replay
    Combat {
        /// Path to the .wotbreplay file (Windows `C:\...` and WSL `/mnt/c/...` styles both accepted)
        file: PathBuf,
        /// Output JSON instead of text
        #[arg(short, long)]
        json: bool,
        /// Write per-shot replay data (positions/orientations) as JSON to this path
        #[arg(long)]
        shots_json: Option<PathBuf>,
        /// Write raw entity streams (type=10/prop2/launches/endpoints/direct-hits) as JSON for WI alignment scans
        #[arg(long)]
        streams_json: Option<PathBuf>,
    },
    /// 解析回放文件玩家开局配置（队伍/坦克/初始血量/耐久加成/弹种表）
    Loadout {
        /// Path to the .wotbreplay file (Windows `C:\...` and WSL `/mnt/c/...` styles both accepted)
        file: PathBuf,
        /// Output JSON instead of text
        #[arg(short, long)]
        json: bool,
    },
    /// 全场实时回放：解析整场战斗并在浏览器中连续播放（14 车位姿/炮塔/弹道/血量）
    Playback {
        /// Path to the .wotbreplay file
        file: PathBuf,
    },
    /// Dump raw method38 / method8 / type=32 packet bytes (RE tool, wotinspector alignment)
    DumpMethods {
        /// Path to the .wotbreplay file
        file: PathBuf,
    },
    /// Dump all packets of one entity in a time window (RE tool: gun pitch hunt)
    DumpEntity {
        /// Path to the .wotbreplay file
        file: PathBuf,
        /// Entity id (decimal)
        eid: u32,
        /// Time window start (s)
        #[arg(long, default_value = "0")]
        t0: f32,
        /// Time window end (s)
        #[arg(long, default_value = "1e9")]
        t1: f32,
        /// Hex byte pattern to search inside payloads (e.g. "4dc52237")
        #[arg(long)]
        pat: Option<String>,
    },
}

fn main() -> Result<()> {
    #[cfg(feature = "bundle")]
    wotb_agent::bundle::bootstrap()?;

    let cli = Cli::parse();

    if let Commands::View {
        tank_id,
        tank_cache,
    } = &cli.command
    {
        let tank_id = *tank_id;
        let tank_cache = tank_cache.clone();
        return tokio::runtime::Runtime::new()?.block_on(async {
            let resolver_path = tank_cache.unwrap_or_else(|| wotb_agent::data::data_path("tank_cache.json"));
            let resolver = TankResolver::load_from_json_file(&resolver_path)
                .map_err(|e| {
                    eprintln!("Failed to load tank cache: {}. Run `fetch-tanks` first.", e);
                    e
                })?;

            let name = resolver.resolve(tank_id).unwrap_or_else(|| format!("tank_{}", tank_id));
            eprintln!("Starting 3D viewer for {} (id={})...", name, tank_id);
            eprintln!("Models served via local cache proxy (/glb/{}/...), first fetch cached to data/cache/models/.", tank_id);
            eprintln!("Tip: run `fetch-models` once to pre-download ALL tank models for full offline use.");

            wotb_agent::wargaming::viewer::serve(resolver, tank_id, None).await
        });
    }

    if let Commands::Web { config, .. } = &cli.command {
        let config_path = config.clone();
        return tokio::runtime::Runtime::new()?
            .block_on(async { wotb_agent::web::serve(config_path).await });
    }

    if let Commands::Playback { file } = &cli.command {
        let file = file.clone();
        return tokio::runtime::Runtime::new()?.block_on(async {
            wotb_agent::wargaming::playback_viewer::serve_standalone(&file).await
        });
    }

    match cli.command {
        Commands::ParseGame { dev_name, game_dir } => {
            use wotb_agent::wargaming::dvpl::{CollisionData, DvplFile};

            let game_dir =
                wotb_agent::wargaming::game_extract::resolve_game_dir(game_dir.as_deref())?;
            let nations = [
                "ussr", "usa", "germany", "uk", "japan", "china", "france", "european", "other",
            ];
            let mut found = false;

            for nation in &nations {
                let filepath = wotb_agent::wargaming::game_extract::resolve_client_path(
                    &game_dir,
                    &format!("3d/Tanks/Parameters/{}/{}.yaml.dvpl", nation, dev_name),
                );
                if filepath.exists() {
                    eprintln!("Found: {}", filepath.display());
                    let dvpl = DvplFile::read(&filepath)?;
                    let text = String::from_utf8_lossy(&dvpl.data);
                    eprintln!(
                        "Decompressed: {} bytes (compression type {})",
                        dvpl.data.len(),
                        dvpl.compression_type
                    );

                    let collision = CollisionData::parse_from_yaml(&text);
                    println!("\n=== {} Collision Data ===\n", dev_name);
                    if let Some(ref c) = collision {
                        if let Some(ref hull) = c.hull_bbox {
                            println!(
                                "  Hull bbox:   min({:.2}, {:.2}, {:.2}) max({:.2}, {:.2}, {:.2})",
                                hull.min[0],
                                hull.min[1],
                                hull.min[2],
                                hull.max[0],
                                hull.max[1],
                                hull.max[2]
                            );
                        }
                        if let Some(ref turret) = c.turret_bbox {
                            println!(
                                "  Turret bbox: min({:.2}, {:.2}, {:.2}) max({:.2}, {:.2}, {:.2})",
                                turret.min[0],
                                turret.min[1],
                                turret.min[2],
                                turret.max[0],
                                turret.max[1],
                                turret.max[2]
                            );
                        }
                        if let Some(ref gun) = c.gun_bbox {
                            println!(
                                "  Gun bbox:    min({:.2}, {:.2}, {:.2}) max({:.2}, {:.2}, {:.2})",
                                gun.min[0],
                                gun.min[1],
                                gun.min[2],
                                gun.max[0],
                                gun.max[1],
                                gun.max[2]
                            );
                        }
                        if let Some(ref chassis) = c.chassis_bbox {
                            println!(
                                "  Chassis bbox: min({:.2}, {:.2}, {:.2}) max({:.2}, {:.2}, {:.2})",
                                chassis.min[0],
                                chassis.min[1],
                                chassis.min[2],
                                chassis.max[0],
                                chassis.max[1],
                                chassis.max[2]
                            );
                        }
                        if let Some(th) = c.average_thickness_hull {
                            println!("  Avg thickness hull:   {:.1}mm", th);
                        }
                        if let Some(th) = c.average_thickness_turret {
                            println!("  Avg thickness turret: {:.1}mm", th);
                        }
                        if let Some(ref p) = c.hull_points {
                            println!("  Hull points:   [{:.2}, {:.2}, {:.2}]", p[0], p[1], p[2]);
                        }
                        if let Some(ref p) = c.turret_points {
                            println!("  Turret points: [{:.2}, {:.2}, {:.2}]", p[0], p[1], p[2]);
                        }
                        if let Some(ref p) = c.gun_points {
                            println!("  Gun points:    [{:.2}, {:.2}, {:.2}]", p[0], p[1], p[2]);
                        }
                    }

                    found = true;
                    break;
                }
            }

            if !found {
                eprintln!("YAML file not found in any nation directory.");
            }

            for nation in &nations {
                let xml_path = wotb_agent::wargaming::game_extract::resolve_client_path(
                    &game_dir,
                    &format!("XML/item_defs/vehicles/{}/{}.xml.dvpl", nation, dev_name),
                );
                if xml_path.exists() {
                    eprintln!("\nFound XML: {}", xml_path.display());
                    let dvpl = DvplFile::read(&xml_path)?;
                    let text = String::from_utf8_lossy(&dvpl.data);
                    let armor = wotb_agent::wargaming::dvpl::ArmorModel::parse_from_xml(&text);
                    if let Some(ref am) = armor {
                        println!("\n=== Armor Model ===");
                        println!("  Hull plates: {:?}", am.hull.plates);
                        if let Some(ref t) = am.turret {
                            println!("  Turret plates: {:?}", t.plates);
                        }
                        if let Some(ref g) = am.gun {
                            println!("  Gun plates: {:?}", g.plates);
                        }
                        if let Some(ref c) = am.chassis {
                            println!("  Chassis: left={} right={}", c.left_track, c.right_track);
                        }
                    }
                    break;
                }
            }
            return Ok(());
        }
        Commands::ExtractGame {
            game_dir,
            output,
            force,
        } => {
            let stats = wotb_agent::wargaming::game_extract::extract_all(
                game_dir.as_deref(),
                &output,
                force,
            )?;
            println!("\n=== Game Data Extraction ===");
            println!("  Output dir:   {}", output.display());
            println!("  Extracted:    {}", stats.extracted);
            println!(
                "  Cached:       {} (already present, use --force to re-extract)",
                stats.cached
            );
            println!(
                "  Missing src:  {} (file absent in game dir)",
                stats.missing_files
            );
            println!("  Parse failed: {}", stats.parse_failed);
            println!("  No dev_name:  {}", stats.skipped_no_name);
            println!("  Write failed: {}", stats.write_failed);
            return Ok(());
        }
        Commands::FetchBlitzkit { output } => {
            let n = tokio::runtime::Runtime::new()?
                .block_on(wotb_agent::wargaming::blitzkit::fetch_and_save(&output))?;
            println!(
                "Saved tanks.pb ({}) — parsed {} tanks",
                output.display(),
                n
            );
            return Ok(());
        }
        Commands::FetchIcons { dir, force } => {
            let (downloaded, cached, failed) =
                wotb_agent::wargaming::blitzkit::download_all_icons(&dir, force)?;
            println!(
                "Tank icons downloaded={} cached={} failed={} -> {}",
                downloaded,
                cached,
                failed,
                dir.display()
            );
            return Ok(());
        }
        Commands::FetchModels { force, concurrency } => {
            let (downloaded, cached, failed, bytes) = tokio::runtime::Runtime::new()?.block_on(
                wotb_agent::wargaming::model_fetch::fetch_all_models(force, concurrency),
            )?;
            println!("Tank models ready: downloaded={} cached={} failed={} ({:.2} GB) -> data/cache/models/",
                downloaded, cached, failed, bytes as f64 / 1024.0 / 1024.0 / 1024.0);
            if failed > 0 {
                eprintln!("Warning: {} model files failed (rerun `fetch-models` to retry only the failures).", failed);
            }
            return Ok(());
        }
        Commands::FetchTerrain { force } => {
            let (total, extracted, cached, failed) =
                wotb_agent::wargaming::map_assets::cache_all_terrain(force);
            println!("=== Terrain cache (data/cache/terrain) ===");
            println!("  Maps in registry: {}", total);
            println!("  Extracted:        {}", extracted);
            println!("  Already cached:   {} (use --force to re-extract)", cached);
            if !failed.is_empty() {
                eprintln!("  Failed ({}): {:?}", failed.len(), failed);
            }
            return Ok(());
        }
        Commands::FetchMinimaps { force } => {
            let (entries, spaces, extracted, cached, failed) =
                wotb_agent::wargaming::map_assets::cache_all_minimaps(force);
            println!("=== Minimap cache (data/cache/maps/<space>.minimap.webp) ===");
            println!("  Registry entries: {entries}");
            println!("  Spaces:           {spaces} (first entry per space)");
            println!("  Extracted:        {extracted}");
            println!("  Already cached:   {cached} (use --force to re-extract)");
            if !failed.is_empty() {
                eprintln!("  Failed ({}): {:?}", failed.len(), failed);
            }
            return Ok(());
        }
        Commands::UpdateData {
            game_dir,
            output,
            tank_cache,
            game_data,
            offline,
            force,
            check,
            icons,
            models,
        } => {
            cmd_update_data(
                game_dir.as_deref(),
                &output,
                &tank_cache,
                &game_data,
                offline,
                force,
                check,
                icons,
                models,
            )?;
            return Ok(());
        }
        Commands::View { .. } => unreachable!(),
        Commands::Web { .. } => unreachable!(),
        Commands::Playback { .. } => unreachable!(),
        Commands::Single {
            file,
            json,
            tank_cache,
        } => {
            let resolver = tank_cache
                .filter(|p| p.exists())
                .and_then(|p| TankResolver::load_from_json_file(&p).ok());
            let parser = if let Some(ref r) = resolver {
                wotb_agent::replay::parser::ReplayParser::with_resolver(r)
            } else {
                wotb_agent::replay::parser::ReplayParser::new()
            };
            let summary = parser.parse_file(&file)?;

            if json {
                let json = serde_json::to_string_pretty(&summary)?;
                println!("{}", json);
            } else {
                print_single_replay(&summary);
            }
        }
        Commands::Dataset { file, tank_cache } => {
            let resolver = tank_cache
                .filter(|p| p.exists())
                .and_then(|p| TankResolver::load_from_json_file(&p).ok());
            let parser = if let Some(ref r) = resolver {
                wotb_agent::replay::parser::ReplayParser::with_resolver(r)
            } else {
                wotb_agent::replay::parser::ReplayParser::new()
            };
            let summary = parser.parse_file(&file)?;

            // 诊断层：包流直方图 + 未消费数据段 + 质量降级聚合
            let mut replay = wotbreplay_parser::replay::Replay::open(std::fs::File::open(&file)?)?;
            let data = replay.read_data()?;
            let raw_packets = wotb_agent::replay::raw_packet_tuples(&data, 0);

            let mut dataset =
                wotb_agent::models::replay_dataset::ReplayDataset::from_summary(&summary);

            let mut hist: std::collections::BTreeMap<u32, u64> = Default::default();
            for (t, _, _) in &raw_packets {
                *hist.entry(*t).or_insert(0) += 1;
            }
            dataset.diagnostics.packet_types = hist.into_iter().collect();

            // 未消费数据段（type=8 按 method、type=7 按 prop、type=32 按帧型；其余按 type）
            let mut un: std::collections::BTreeMap<String, u64> = Default::default();
            let consumed8 = [
                0x00u32, 0x01, 0x07, 0x08, 0x0c, 0x0d, 0x14, 0x1b, 0x1d, 0x23, 0x24, 0x26, 0x30,
            ];
            let consumed7 = [0u32, 1, 2, 3, 4, 9, 10, 11];
            for (t, _, p) in &raw_packets {
                let key = match t {
                    8 if p.len() >= 8 => {
                        let m = u32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                        if consumed8.contains(&m) {
                            continue;
                        }
                        format!("m0x{:02x}", m)
                    }
                    7 if p.len() >= 8 => {
                        let sub = u32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                        if consumed7.contains(&sub) {
                            continue;
                        }
                        format!("prop{}", sub)
                    }
                    10 | 5 | 4 | 26 | 28 | 31 | 33 | 35 | 36 | 39 | 0 => continue,
                    32 => {
                        if p.len() >= 5 && p[4] == 0x01 && (p.len() == 26 || p.len() == 27) {
                            continue;
                        }
                        "type32_other".to_string()
                    }
                    other => format!("type{}", other),
                };
                *un.entry(key).or_insert(0) += 1;
            }
            dataset.diagnostics.unsupported = un.into_iter().collect();

            // 质量降级聚合（作者 + 他人路径全部 ShotReplayData.quality）
            // 作者实体 = comps 条目 account_id 精确匹配（P3 定案），退回昵称匹配
            let valid_tanks: Vec<u32> = summary
                .players
                .iter()
                .map(|p| p.tank_id)
                .collect();
            let comps = wotb_agent::replay::playback::collect_comp_descriptors(
                &raw_packets,
                &valid_tanks,
            );
            let author_eid = comps
                .values()
                .find(|c| c.account_id != 0 && c.account_id == summary.author_account_id as u64)
                .map(|c| c.eid)
                .filter(|e| *e != 0)
                .unwrap_or_else(|| {
                    wotb_agent::replay::combat::resolve_author_player_eid_by_nick(
                        &raw_packets,
                        &summary.author_nickname,
                    )
                });
            let pitch_limits = Default::default();
            // 一次共享扫描完成作者严格 + 他人宽松两路（作者 Err → 空表降级，诊断上抛语义不变）
            let bundle = wotb_agent::replay::combat::extract_shot_bundle_auto_with_limits(
                &raw_packets,
                &summary.author_nickname,
                author_eid,
                &pitch_limits,
            );
            let mut author_shots = bundle.author.unwrap_or_default();
            let mut others = bundle.others?;
            // 动态装甲弹着态注入（additive：仅 6 辆声明车非缺省）
            wotb_agent::replay::combat::attach_shot_armor_states(&mut author_shots, &raw_packets);
            wotb_agent::replay::combat::attach_shot_armor_states(&mut others.shots, &raw_packets);
            dataset.diagnostics.shots_author = author_shots.len();
            dataset.diagnostics.shots_others = others.shots.len();
            let mut deg: std::collections::BTreeMap<String, u64> = Default::default();
            let bump = |q: &Option<wotb_agent::replay::combat::ShotQuality>,
                        deg: &mut std::collections::BTreeMap<String, u64>| {
                let Some(q) = q else { return };
                if q.shooter_pos_from_muzzle {
                    *deg.entry("shooter_pos_from_muzzle".into()).or_insert(0) += 1;
                }
                if q.shooter_pitch_from_velocity {
                    *deg.entry("shooter_pitch_from_velocity".into()).or_insert(0) += 1;
                }
                if q.shooter_pitch_from_method36 {
                    *deg.entry("shooter_pitch_from_method36".into()).or_insert(0) += 1;
                }
                if q.dmg_unattributed {
                    *deg.entry("dmg_unattributed".into()).or_insert(0) += 1;
                }
                if q.shell_from_broadcast {
                    *deg.entry("shell_from_broadcast".into()).or_insert(0) += 1;
                }
                if q.shell_from_terrain {
                    *deg.entry("shell_from_terrain".into()).or_insert(0) += 1;
                }
                if q.shell_from_launch_code {
                    *deg.entry("shell_from_launch_code".into()).or_insert(0) += 1;
                }
                if q.shell_from_velocity {
                    *deg.entry("shell_from_velocity".into()).or_insert(0) += 1;
                }
                for k in q
                    .gun_pitch_degraded
                    .iter()
                    .chain(q.pitch_frozen.iter())
                    .chain(q.turret_degraded.iter())
                {
                    *deg.entry(k.clone()).or_insert(0) += 1;
                }
            };
            for s in author_shots.iter().chain(others.shots.iter()) {
                bump(&s.quality, &mut deg);
            }
            dataset.diagnostics.degradation = deg.into_iter().collect();

            println!("{}", serde_json::to_string_pretty(&dataset)?);
        }
        Commands::Facets {
            file,
            parts,
            out,
            tank_cache,
        } => {
            wotb_agent::facets::export_cli(&file, &parts, out.as_deref(), tank_cache.as_deref())?;
        }
        Commands::DumpMapIndex => {
            println!("{}", wotb_agent::wargaming::map_assets::dump_map_index());
        }
        Commands::DumpTankData { out } => {
            let total = wotb_agent::wargaming::tank_configs::export_tank_data(&out)?;
            println!("导出 {total} 份 per-tank JSON → {}", out.display());
        }
        Commands::DumpShellKinds { out } => {
            let table = wotb_agent::replay::loadout::ShellKindTable::from_tanks_pb();
            let json = table.to_json();
            match out {
                Some(path) => {
                    if let Some(dir) = path.parent() {
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(&path, json)?;
                    eprintln!("shell kinds table -> {}", path.display());
                }
                None => print!("{}", json),
            }
        }
        Commands::Scan {
            dir,
            mode,
            days,
            output,
            tank_cache,
            fetch_tanks,
            app_id: _,
            server: _,
        } => {
            let resolver = if fetch_tanks {
                eprintln!("Building tank resolver from local BlitzKit data...");
                match TankResolver::from_blitzkit() {
                    Ok(r) => {
                        eprintln!("Loaded {} tanks from BlitzKit", r.len());
                        Some(r)
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to build BlitzKit resolver: {}", e);
                        None
                    }
                }
            } else if let Some(ref cache_path) = tank_cache {
                if cache_path.exists() {
                    eprintln!("Loading tank cache from {}...", cache_path.display());
                    match TankResolver::load_from_json_file(cache_path) {
                        Ok(r) => {
                            eprintln!("Loaded {} tanks", r.len());
                            Some(r)
                        }
                        Err(e) => {
                            eprintln!("Warning: failed to load tank cache: {}", e);
                            None
                        }
                    }
                } else {
                    eprintln!("Tank cache not found: {}", cache_path.display());
                    None
                }
            } else {
                None
            };

            let scanner = if let Some(ref r) = resolver {
                ReplayScanner::with_resolver(r)
            } else {
                ReplayScanner::new()
            };

            let filter = ScanFilter::from_mode(&mode, days);

            eprintln!("Scanning: {}", dir.display());
            eprintln!("Filter: mode={}, days={:?}", mode, days);
            eprintln!();

            let battles = scanner.scan_dir(&dir, &filter, |p| {
                let status = if p.ok { "OK" } else { "ERR" };
                let detail = p
                    .error
                    .as_deref()
                    .map(|e| format!(": {}", e))
                    .unwrap_or_default();
                eprint!(
                    "\r[{}/{}] {} {}{}",
                    p.current, p.total, status, p.file_name, detail
                );
                if p.current == p.total {
                    eprintln!();
                }
            })?;

            if battles.is_empty() {
                eprintln!("No replays matched the filter.");
                return Ok(());
            }

            let room_type = battles
                .first()
                .map(|b| b.room_type.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let report = AggregatedReport::from_battles(battles, &room_type);

            if let Some(output) = output {
                let json = serde_json::to_string_pretty(&report)?;
                std::fs::write(&output, json)?;
                eprintln!("Report saved to: {}", output.display());
            }

            report.print_summary();
        }
        Commands::Snapshot {
            app_id,
            server,
            nickname,
            action,
            dir,
        } => {
            let store = SnapshotStore::new(&dir);

            match action.as_str() {
                "take" => {
                    let client = WgApiClient::new(&app_id, &server);
                    eprintln!("Fetching stats for '{}'...", nickname);
                    let results = client.search_player(&nickname, true)?;
                    if results.is_empty() {
                        anyhow::bail!("Player not found: {}", nickname);
                    }
                    let account_id = results[0].1;
                    let stats = client.get_player_stats(account_id)?;
                    let snapshot =
                        wotb_agent::wargaming::snapshot::Snapshot::from_player_stats(stats);
                    let path = store.save(&snapshot)?;
                    eprintln!("Snapshot saved: {}", path.display());
                    eprintln!(
                        "  Player: {} (id={})",
                        snapshot.player.nickname, snapshot.player.account_id
                    );
                    eprintln!(
                        "  Rating battles: {}, WR: {:.1}%",
                        snapshot.player.rating_battles,
                        if snapshot.player.rating_battles > 0 {
                            snapshot.player.rating_wins as f64
                                / snapshot.player.rating_battles as f64
                                * 100.0
                        } else {
                            0.0
                        }
                    );
                    eprintln!(
                        "  mm_rating: {:.2} (display {})",
                        snapshot.player.rating_mm_rating.unwrap_or(0.0),
                        snapshot.player.rating_display_rating.unwrap_or(0)
                    );
                }
                "list" => {
                    let snapshots = store.list()?;
                    store.print_list(&snapshots);
                }
                "diff" => {
                    let snapshots = store.list()?;
                    if snapshots.len() < 2 {
                        eprintln!(
                            "Need at least 2 snapshots to diff (found {}).",
                            snapshots.len()
                        );
                        eprintln!("Take more snapshots first with: snapshot <name> --action take");
                        return Ok(());
                    }
                    let from = &snapshots[0];
                    let to = &snapshots[snapshots.len() - 1];
                    let diff = store.diff(from, to);
                    store.print_diff(&diff);
                }
                _ => {
                    anyhow::bail!("Unknown action: {} (use: take, list, diff)", action);
                }
            }
        }
        Commands::Chat {
            config: config_path,
            save,
            load,
        } => {
            let _ = ctrlc::set_handler(|| {
                wotb_agent::agent::set_interrupted();
                eprintln!("\n[Interrupted] Finishing current step...");
            });

            eprintln!("Loading config from {}...", config_path.display());
            let mut agent = Agent::new(&config_path)?;

            if let Some(ref load_path) = load {
                if load_path.exists() {
                    eprintln!("Loading session from {}...", load_path.display());
                    match agent.load_session(load_path) {
                        Ok(_) => eprintln!("Session loaded."),
                        Err(e) => eprintln!("Failed to load session: {}", e),
                    }
                }
            }

            eprintln!(
                "Agent ready! Model: {} (context: {}, thinking: {})",
                agent.config.llm.model,
                agent.config.llm.context_length,
                agent.config.llm.thinking_mode
            );
            eprintln!("Replay dir: {}", agent.replay_dir);
            if let Some(ref tc) = agent.tank_cache {
                eprintln!("Tank cache: {}", tc.display());
            }
            eprintln!(
                "Type 'exit' to quit, 'history' to view conversation, 'usage' to see token stats."
            );
            eprintln!("Press Ctrl+C to interrupt long-running tasks.");
            eprintln!();

            let stdin = io::stdin();
            loop {
                print!("> ");
                io::stdout().flush()?;

                let mut input = String::new();
                if stdin.lock().read_line(&mut input)? == 0 {
                    break;
                }
                let input = input.trim();

                if input.is_empty() {
                    continue;
                }
                if input == "exit" || input == "quit" {
                    break;
                }
                if input == "history" {
                    agent.print_history();
                    continue;
                }
                if input == "usage" {
                    agent.print_usage();
                    continue;
                }

                match agent.chat(input) {
                    Ok(response) => {
                        println!("\n{}\n", response);
                    }
                    Err(e) => {
                        eprintln!("Error: {}\n", e);
                    }
                }
            }

            if let Some(ref save_path) = save {
                match agent.save_session(save_path) {
                    Ok(_) => eprintln!("Session saved to {}", save_path.display()),
                    Err(e) => eprintln!("Failed to save session: {}", e),
                }
            }
            agent.save_usage();
            eprintln!("Session saved. Goodbye!");
        }
        Commands::Config { file, show } => {
            if show {
                let config = Config::load_or_create(&file)?;
                let toml = toml::to_string_pretty(&config)?;
                println!("# {}\n\n{}", file.display(), toml);
            } else {
                let config = Config::load_or_create(&file)?;
                eprintln!("Config file: {}", file.display());
                eprintln!(
                    "  WG API: server={}, app_id={}",
                    config.wg_api.server,
                    if config.wg_api.application_id.is_empty() {
                        "(empty)"
                    } else {
                        "configured"
                    }
                );
                eprintln!(
                    "  LLM: model={}, endpoint={}",
                    config.llm.model, config.llm.endpoint
                );
                eprintln!("  Replay dir: {}", config.replay.replay_dir);
                eprintln!();
                eprintln!("Edit {} to configure your settings.", file.display());
            }
        }
        Commands::Usage { file } => {
            let usage = TokenUsage::load_from_file(&file)?;
            usage.print_summary();
        }
        Commands::Player {
            nickname,
            app_id,
            server,
        } => {
            let client = WgApiClient::new(&app_id, &server);

            eprintln!("Searching for player '{}' on {}...", nickname, server);
            let results = client.search_player(&nickname, true)?;
            if results.is_empty() {
                eprintln!("Player not found. Trying partial match...");
                let partial = client.search_player(&nickname, false)?;
                if partial.is_empty() {
                    eprintln!("No players found.");
                    return Ok(());
                }
                eprintln!("Found {} partial matches:", partial.len());
                for (nick, id) in &partial {
                    eprintln!("  {} (id={})", nick, id);
                }
                return Ok(());
            }

            let (_, account_id) = &results[0];
            eprintln!("Found: {} (id={})", results[0].0, account_id);
            eprintln!("Fetching stats...");

            let stats = client.get_player_stats(*account_id)?;
            WgApiClient::print_stats(&stats);
        }
        Commands::Compare {
            nickname,
            dir,
            app_id,
            server,
            tank_cache,
            mode,
        } => {
            let client = WgApiClient::new(&app_id, &server);
            eprintln!("Fetching API stats for '{}'...", nickname);
            let results = client.search_player(&nickname, true)?;
            if results.is_empty() {
                anyhow::bail!("Player not found: {}", nickname);
            }
            let account_id = results[0].1;
            let api_stats = client.get_player_stats(account_id)?;

            let resolver = tank_cache
                .filter(|p| p.exists())
                .and_then(|p| TankResolver::load_from_json_file(&p).ok());
            let scanner = if let Some(ref r) = resolver {
                ReplayScanner::with_resolver(r)
            } else {
                ReplayScanner::new()
            };

            let filter = ScanFilter::from_mode(&mode, None);

            eprintln!("Scanning replays in {} (mode={})...", dir.display(), mode);
            let battles = scanner.scan_dir(&dir, &filter, |p| {
                let detail = p
                    .error
                    .as_deref()
                    .map(|e| format!(" ERR: {}", e))
                    .unwrap_or_default();
                eprint!("\r[{}/{}] {}{}", p.current, p.total, p.file_name, detail);
                if p.current == p.total {
                    eprintln!();
                }
            })?;

            if battles.is_empty() {
                eprintln!("No replays matched.");
                return Ok(());
            }

            let room_type = battles
                .first()
                .map(|b| b.room_type.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            let report = AggregatedReport::from_battles(battles, &room_type);

            let is_rating = mode == "rating";
            let (api_battles, api_wins, api_dmg, api_frags, api_shots, api_hits) = if is_rating {
                (
                    api_stats.rating_battles,
                    api_stats.rating_wins,
                    api_stats.rating_damage_dealt,
                    api_stats.rating_frags,
                    api_stats.rating_shots,
                    api_stats.rating_hits,
                )
            } else {
                (
                    api_stats.random_battles,
                    api_stats.random_wins,
                    api_stats.random_damage_dealt,
                    api_stats.random_frags,
                    api_stats.random_shots,
                    api_stats.random_hits,
                )
            };

            println!();
            println!("========================================================");
            println!("  Replay vs API Comparison: {}", nickname);
            println!("========================================================");
            println!();
            println!(
                "  {:<25} {:>12} {:>12} {:>10}",
                "Metric", "Replays", "API Total", "Replay %"
            );
            println!("  {}", "-".repeat(62));

            let an = api_battles.max(1) as f64;

            let rows: Vec<(&str, f64, f64, bool)> = vec![
                (
                    "Battles",
                    report.total_battles as f64,
                    api_battles as f64,
                    false,
                ),
                ("Wins", report.wins as f64, api_wins as f64, false),
                (
                    "Win rate %",
                    report.win_rate,
                    api_wins as f64 / an * 100.0,
                    true,
                ),
                ("Avg damage", report.avg_damage, api_dmg as f64 / an, true),
                ("Avg frags", report.avg_frags, api_frags as f64 / an, true),
                (
                    "Hit rate %",
                    report.hit_rate,
                    if api_shots > 0 {
                        api_hits as f64 / api_shots as f64 * 100.0
                    } else {
                        0.0
                    },
                    true,
                ),
                (
                    "Avg XP",
                    report.avg_xp,
                    if is_rating {
                        api_stats.rating_xp as f64 / an
                    } else {
                        api_stats.random_xp as f64 / an
                    },
                    true,
                ),
                ("Avg block", report.avg_damage_blocked, 0.0, true),
                ("Avg assisted", report.avg_assisted, 0.0, true),
            ];

            for (label, replay_val, api_val, is_avg) in &rows {
                let pct = if *api_val > 0.0 && *is_avg {
                    format!("{:+.1}%", (replay_val - api_val) / api_val * 100.0)
                } else if !*is_avg && *api_val > 0.0 {
                    format!("{:.1}%", replay_val / api_val * 100.0)
                } else {
                    "-".to_string()
                };
                let replay_str = if *label == "Win rate %" || *label == "Hit rate %" {
                    format!("{:.1}%", replay_val)
                } else {
                    format!("{:.0}", replay_val)
                };
                let api_str = if *label == "Win rate %" || *label == "Hit rate %" {
                    format!("{:.1}%", api_val)
                } else {
                    format!("{:.0}", api_val)
                };
                println!(
                    "  {:<25} {:>12} {:>12} {:>10}",
                    label, replay_str, api_str, pct
                );
            }

            println!();
            println!(
                "  Replay battles: {}  |  API total: {}  |  Coverage: {:.1}%",
                report.total_battles,
                api_battles,
                report.total_battles as f64 / api_battles.max(1) as f64 * 100.0
            );

            if is_rating {
                println!();
                println!("  --- Rating ---");
                println!(
                    "  Replay mm_rating: {:.2} -> {:.2} (delta {:+.2})",
                    report.rating_start.unwrap_or(0.0),
                    report.rating_end.unwrap_or(0.0),
                    report.rating_delta.unwrap_or(0.0)
                );
                println!(
                    "  API mm_rating:     {:.2} (display {})",
                    api_stats.rating_mm_rating.unwrap_or(0.0),
                    api_stats.rating_display_rating.unwrap_or(0)
                );
            }

            println!();
            println!("  --- Top 5 Tanks (Replay) ---");
            for t in report.tank_usage.iter().take(5) {
                println!(
                    "    {:<28} {:>3}b  WR={:.0}%  avg_dmg={:.0}",
                    t.tank_name, t.battles, t.win_rate, t.avg_damage
                );
            }

            println!();
            println!("========================================================");
        }
        Commands::Prematch {
            nicknames,
            file,
            replay,
            app_id,
            server,
        } => {
            let client = WgApiClient::new(&app_id, &server);

            if let Some(ref rp) = replay {
                let resolver = TankResolver::load_from_json_file(&wotb_agent::data::data_path(
                    "tank_cache.json",
                ))
                .ok();
                let parser = match &resolver {
                    Some(r) => wotb_agent::replay::parser::ReplayParser::with_resolver(r),
                    None => wotb_agent::replay::parser::ReplayParser::new(),
                };
                let summary = parser.parse_file(rp)?;
                eprintln!("\n=== 回放: {} ===", summary.file_name);

                let (report_a, report_b) = {
                    let pa = query_lineup_stats(
                        &client,
                        summary
                            .players
                            .iter()
                            .filter(|p| p.team == 1)
                            .map(|p| p.nickname.as_str()),
                        "我方",
                    );
                    let pb = query_lineup_stats(
                        &client,
                        summary
                            .players
                            .iter()
                            .filter(|p| p.team != 1)
                            .map(|p| p.nickname.as_str()),
                        "敌方",
                    );
                    (
                        wotb_agent::wargaming::prematch::analyze_lineup(pa)?,
                        wotb_agent::wargaming::prematch::analyze_lineup(pb)?,
                    )
                };

                eprintln!("\n--- 我方阵容 ---");
                wotb_agent::wargaming::prematch::print_report(&report_a);
                eprintln!("\n--- 敌方阵容 ---");
                wotb_agent::wargaming::prematch::print_report(&report_b);

                let diff = report_a.avg_damage - report_b.avg_damage;
                eprintln!("\n=== 阵容对比 ===");
                eprintln!(
                    "我方场均伤害 {:.0} vs 敌方 {:.0} ({:+.0})",
                    report_a.avg_damage, report_b.avg_damage, diff
                );
                eprintln!(
                    "我方平均胜率 {:.1}% vs 敌方 {:.1}%",
                    report_a.avg_win_rate, report_b.avg_win_rate
                );
                if diff > 0.0 {
                    eprintln!("我方伤害占优，可利用火力压制敌方弱点。");
                } else if diff < 0.0 {
                    eprintln!("敌方伤害占优，注意规避其高威胁玩家，优先集火其薄弱点。");
                } else {
                    eprintln!("双方火力相当，比拼战术配合与走位。");
                }
                return Ok(());
            }

            let mut names: Vec<String> = Vec::new();
            if let Some(ref f) = file {
                let text = std::fs::read_to_string(f)?;
                for line in text.lines() {
                    let n = line.trim().trim_matches(|c| c == '"' || c == '\'');
                    if !n.is_empty() {
                        names.push(n.to_string());
                    }
                }
            }
            if let Some(ref inline) = nicknames {
                for s in inline.split(',') {
                    let n = s.trim();
                    if !n.is_empty() {
                        names.push(n.to_string());
                    }
                }
            }
            if names.is_empty() {
                anyhow::bail!(
                    "No nicknames provided. Pass them inline, via --file, or via --replay."
                );
            }

            let mut players = Vec::new();
            for name in &names {
                eprintln!("查询玩家 '{}'...", name);
                match client.search_player(name, true) {
                    Ok(results) if !results.is_empty() => {
                        let account_id = results[0].1;
                        match client.get_player_stats(account_id) {
                            Ok(stats) => players.push(stats),
                            Err(e) => eprintln!("  {} 查询失败: {}", name, e),
                        }
                    }
                    _ => eprintln!("  未找到玩家 '{}'", name),
                }
            }

            let report = wotb_agent::wargaming::prematch::analyze_lineup(players)?;
            wotb_agent::wargaming::prematch::print_report(&report);
        }
        Commands::FetchTanks { output } => {
            eprintln!("Building tank resolver from local BlitzKit data (no WG API)...");
            let resolver = TankResolver::from_blitzkit()?;
            eprintln!("Built {} tanks from BlitzKit pb data", resolver.len());
            resolver.save_to_json_file(&output)?;
            eprintln!("Saved to: {}", output.display());
        }
        Commands::Combat {
            file,
            json,
            shots_json,
            streams_json,
        } => {
            use std::fs::File;
            use wotbreplay_parser::replay::Replay;

            // 路径风格兼容：Windows/WSL 任一风格输入按运行平台自动转换（C:\... ⇄ /mnt/c/...；path_translate=off 可关闭）
            let file = std::path::PathBuf::from(
                wotb_agent::models::config::ReplayConfig::translate_with_mode(
                    &file.to_string_lossy(),
                    "auto",
                ),
            );

            let mut replay = Replay::open(File::open(&file)?)
                .map_err(|e| anyhow::anyhow!("Failed to open replay: {}", e))?;

            let data = replay
                .read_data()
                .map_err(|e| anyhow::anyhow!("Failed to read data: {}", e))?;

            let raw_packets = wotb_agent::replay::raw_packet_tuples(&data, 0);

            let timeline = CombatTimeline::parse_packets(&raw_packets);

            // 双方炮管俯仰的车型极限锚定表：battle_results × tank_cache.json（best-effort，
            // 缓存缺失时俯仰走回退路径并打质量标记）
            let br = replay.read_battle_results().ok();
            let author_nick = br
                .as_ref()
                .map(wotb_agent::replay::combat::author_nick_from_battle_results)
                .or_else(|| replay.read_meta().ok().map(|m| m.player_name.clone()))
                .unwrap_or_default();
            // 实际搭载 comp blob：俯仰锚定按实际搭载的炮对号（多炮车非顶级主炮范围不同）
            let comps = br
                .as_ref()
                .map(|br| {
                    let valid: Vec<u32> =
                        br.player_results.iter().map(|pr| pr.info.tank_id).collect();
                    wotb_agent::replay::playback::collect_comp_descriptors(&raw_packets, &valid)
                })
                .unwrap_or_default();
            let pitch_limits = br
                .as_ref()
                .and_then(|br| {
                    TankResolver::load_from_json_file(std::path::Path::new("data/tank_cache.json"))
                        .ok()
                        .map(|r| r.pitch_limits_from_battle_results(br, &comps))
                })
                .unwrap_or_default();
            let mut shot_replay =
                wotb_agent::replay::combat::extract_shot_replays_auto_with_limits(
                    &raw_packets,
                    &author_nick,
                    &pitch_limits,
                )?;
            // 弹种回填：全局 shell_id → tanks.pb 原始弹种串（兜底链各级来源统一识别）
            wotb_agent::replay::loadout::ShellKindTable::shared().annotate(&mut shot_replay);
            // UpdateArena 竞技场状态流（子类型名表 + PERIOD 战局阶段时间线，报告 §4.5）
            let arena_updates = wotb_agent::replay::combat::collect_arena_updates(&raw_packets);
            let arena_periods = wotb_agent::replay::combat::parse_arena_periods(&arena_updates);

            if json {
                let combined = serde_json::json!({
                    "timeline": timeline,
                    "shot_replay": shot_replay,
                    "arena_updates": arena_updates,
                    "arena_periods": arena_periods,
                });
                println!("{}", serde_json::to_string_pretty(&combined)?);
            } else {
                println!("\n========================================================");
                println!("  Combat Event Analysis: {}", file.display());
                println!("========================================================");
                print_combat_timeline(&timeline);

                println!("\n========================================================");
                if let Some(path) = shots_json {
                    std::fs::write(&path, serde_json::to_string_pretty(&shot_replay)?)?;
                    println!(
                        "Shot replay data written: {} ({} shots)",
                        path.display(),
                        shot_replay.len()
                    );
                }
            }
            if let Some(path) = streams_json {
                std::fs::write(
                    &path,
                    serde_json::to_string(&wotb_agent::replay::combat::dump_replay_streams(
                        &raw_packets,
                    ))?,
                )?;
                eprintln!("Entity streams written: {}", path.display());
            }
        }
        Commands::Loadout { file, json } => {
            use std::fs::File;
            use wotbreplay_parser::replay::Replay;

            // 路径风格兼容：Windows/WSL 任一风格输入按运行平台自动转换（C:\... ⇄ /mnt/c/...；path_translate=off 可关闭）
            let file = std::path::PathBuf::from(
                wotb_agent::models::config::ReplayConfig::translate_with_mode(
                    &file.to_string_lossy(),
                    "auto",
                ),
            );

            let mut replay = Replay::open(File::open(&file)?)
                .map_err(|e| anyhow::anyhow!("Failed to open replay: {}", e))?;
            let data = replay
                .read_data()
                .map_err(|e| anyhow::anyhow!("Failed to read data: {}", e))?;
            let br = replay
                .read_battle_results()
                .map_err(|e| anyhow::anyhow!("Failed to read battle results: {}", e))?;

            let raw_packets = wotb_agent::replay::raw_packet_tuples(&data, 5);

            let loadouts = wotb_agent::replay::loadout::collect_player_loadouts(&raw_packets, &br);
            if json {
                println!("{}", serde_json::to_string_pretty(&loadouts)?);
            } else {
                println!("\n========================================================");
                println!("  Player Loadouts: {}", file.display());
                println!("========================================================");
                for l in &loadouts {
                    println!(
                        "T{} {} {} (eid={:08x})",
                        l.team, l.nickname, l.tank_name, l.entity_id
                    );
                    match (l.hp_base, l.hp_bonus_pct) {
                        (0, _) => println!("  HP {} (基准未知)", l.hp_initial),
                        (base, Some(pct)) => println!(
                            "  HP {}/{} (+{:.1}% {})",
                            l.hp_initial,
                            base,
                            pct,
                            if l.durability_equipment.is_empty() {
                                ""
                            } else {
                                &l.durability_equipment
                            }
                        ),
                        (base, None) => println!("  HP {}/{}", l.hp_initial, base),
                    }
                    for (i, s) in l.shells.iter().enumerate() {
                        println!(
                            "  shell {} 0x{:06x} {} dmg={} pen={}",
                            i, s.global_id, s.kind, s.damage, s.penetration
                        );
                    }
                    if let Some(lo) = &l.loadout {
                        println!(
                            "  equipment {}/9: {:?}",
                            lo.equipment_count, lo.equipment_ids
                        );
                        for (label, items) in [
                            ("consumable", &lo.consumables),
                            ("provision", &lo.provisions),
                            ("ability", &lo.abilities),
                        ] {
                            for it in items {
                                println!(
                                    "  {label}[{}] wire=0x{:02x} state={} id={}",
                                    it.index,
                                    it.wire_code,
                                    it.state,
                                    it.logical_id.as_deref().unwrap_or("UNKNOWN")
                                );
                            }
                        }
                    } else {
                        println!("  loadout: 无 Type5 framing");
                    }
                }
                println!("共 {} 名玩家", loadouts.len());
            }
        }
        Commands::DumpMethods { file } => {
            use std::fs::File;
            use wotbreplay_parser::replay::Replay;

            let mut replay = Replay::open(File::open(&file)?)
                .map_err(|e| anyhow::anyhow!("Failed to open replay: {}", e))?;
            let data = replay
                .read_data()
                .map_err(|e| anyhow::anyhow!("Failed to read data: {}", e))?;

            let raw_packets = wotb_agent::replay::raw_packet_tuples(&data, 0);

            // ---- method38 (0x26) 命中结果：完整 args 解码 ----
            println!("=== Avatar method38 (0x26) hit results ===");
            for (t, clock, p) in &raw_packets {
                if *t != 8 || p.len() < 12 {
                    continue;
                }
                if u32le(p, 4) != 0x26 {
                    continue;
                }
                let alen = u32le(p, 8) as usize;
                if 12 + alen > p.len() {
                    continue;
                }
                let a = &p[12..12 + alen];
                println!(
                    "\nt={:.3} envelope_eid={} args_len={}",
                    clock,
                    u32le(p, 0),
                    alen
                );
                println!("  args: {}", hex(a));
                if alen < 9 {
                    continue;
                }
                println!(
                    "  victim={} flags={:#06x} headerHi={:#06x} resultCount={}",
                    u32le(a, 0),
                    u16le(a, 4),
                    u16le(a, 6),
                    a[8]
                );
                let mut off = 9usize;
                for k in 0..a[8] as usize {
                    if off + 2 <= alen {
                        println!(
                            "    component[{}]: token={} state={}",
                            k,
                            a[off],
                            a[off + 1]
                        );
                        off += 2;
                    }
                }
                if off < alen {
                    let mcount = a[off];
                    off += 1;
                    println!("  modifierCount={} ", mcount);
                    for k in 0..mcount as usize {
                        if off + 4 <= alen {
                            println!("    modifier[{}]: {}", k, u32le(a, off));
                            off += 4;
                        }
                    }
                    if off < alen {
                        println!("  TRAILING {}B: {}", alen - off, hex(&a[off..]));
                    }
                }
            }

            // ---- method8 (0x08) 直击通知：packed 元数据 ----
            println!("\n=== Vehicle method8 (0x08) direct-hit notices ===");
            for (t, clock, p) in &raw_packets {
                if *t != 8 || p.len() < 12 {
                    continue;
                }
                if u32le(p, 4) != 0x08 {
                    continue;
                }
                let alen = u32le(p, 8) as usize;
                if 12 + alen > p.len() {
                    continue;
                }
                let a = &p[12..12 + alen];
                println!(
                    "t={:.3} envelope_eid={} args_len={} args: {}",
                    clock,
                    u32le(p, 0),
                    alen,
                    hex(a)
                );
                if alen >= 10 {
                    println!(
                        "  shooter={} victim={} b8={:#04x} b9={:#04x} b10={:#04x}",
                        u32le(a, 0),
                        u32le(a, 4),
                        a[8],
                        a[9],
                        a[10]
                    );
                    if alen >= 21 {
                        let seg = u64::from_le_bytes([
                            a[11], a[12], a[13], a[14], a[15], a[16], a[17], a[18],
                        ]);
                        println!(
                            "  packed[11..21]: {} | as u64@11: {:#018x} ({})",
                            hex(&a[11..21]),
                            seg,
                            seg
                        );
                        println!("  trailing[21..]: {}", hex(&a[21..]));
                    }
                }
            }

            // ---- type=32 警告包（含 segment u64 布局）----
            println!("\n=== type=32 packets (len>=26) ===");
            let mut len_hist: std::collections::BTreeMap<usize, usize> =
                std::collections::BTreeMap::new();
            for (t, clock, p) in &raw_packets {
                if *t != 32 {
                    continue;
                }
                *len_hist.entry(p.len()).or_insert(0) += 1;
                if p.len() < 26 {
                    continue;
                }
                println!("t={:.3} len={} raw: {}", clock, p.len(), hex(p));
                // [eid u32][01][method u32][u16][flag u8][shell_id u16][yaw u16][pitch u16][segment u64]
                println!("  eid={} b4={:#04x} method={:#010x} u16@9={:#06x} flag@11={:#04x} shell_id@12={} yaw@14={} pitch@16={} seg@18={:#018x}",
                    u32le(p, 0), p[4], u32le(p, 5), u16le(p, 9), p[11],
                    u16le(p, 12), u16le(p, 14), u16le(p, 16),
                    u64::from_le_bytes([p[18],p[19],p[20],p[21],p[22],p[23],p[24],p[25]]));
            }
            println!("  len histogram: {:?}", len_hist);
        }
        Commands::DumpEntity {
            file,
            eid,
            t0,
            t1,
            pat,
        } => {
            use std::fs::File;
            use wotbreplay_parser::replay::Replay;

            let mut replay = Replay::open(File::open(&file)?)
                .map_err(|e| anyhow::anyhow!("Failed to open replay: {}", e))?;
            let data = replay
                .read_data()
                .map_err(|e| anyhow::anyhow!("Failed to read data: {}", e))?;
            let pat_bytes: Vec<Vec<u8>> = pat
                .as_ref()
                .map(|s| {
                    s.split('|')
                        .map(|h| {
                            (0..h.len())
                                .step_by(2)
                                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();
            for pkt in &data.packets {
                if pkt.clock_secs < t0 || pkt.clock_secs > t1 {
                    continue;
                }
                let p = &pkt.raw_payload[..];
                let pat_hit = !pat_bytes.is_empty()
                    && pat_bytes.iter().any(|pb| {
                        (0..=p.len().saturating_sub(pb.len()))
                            .any(|o| &p[o..o + pb.len()] == pb.as_slice())
                    });
                if !pat_bytes.is_empty() {
                    if !pat_hit {
                        continue;
                    }
                } else {
                    // eid 过滤读 4 字节实体前缀；短载荷包（tick/标志）仅在 eid 模式下无意义，模式搜索须保留
                    if p.len() < 4 {
                        continue;
                    }
                    let e = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                    if e != eid {
                        continue;
                    }
                }
                let pkt_type = match &pkt.payload {
                    wotbreplay_parser::models::data::payload::Payload::BasePlayerCreate {
                        ..
                    } => 0u32,
                    wotbreplay_parser::models::data::payload::Payload::EntityMethod(_) => 8,
                    wotbreplay_parser::models::data::payload::Payload::Unknown { packet_type } => {
                        *packet_type
                    }
                };
                // type=10: 附带 f32 解码（位置/姿态 + 尾部）
                let mut extra = String::new();
                if pkt_type == 10 && p.len() >= 48 {
                    let fs: Vec<f32> = (0..((p.len() - 12) / 4))
                        .map(|k| {
                            f32::from_le_bytes([
                                p[12 + k * 4],
                                p[13 + k * 4],
                                p[14 + k * 4],
                                p[15 + k * 4],
                            ])
                        })
                        .collect();
                    extra = format!(
                        "  f32[{}]: {:?}",
                        fs.len(),
                        fs.iter().map(|f| format!("{:.4}", f)).collect::<Vec<_>>()
                    );
                }
                if pkt_type == 7 && p.len() >= 16 {
                    let sub = u32::from_le_bytes([p[4], p[5], p[6], p[7]]);
                    extra = format!("  sub={} body={}", sub, hex(&p[12..]));
                }
                println!(
                    "t={:>8.3} type={:<3} len={:<4} {}",
                    pkt.clock_secs,
                    pkt_type,
                    p.len(),
                    hex(p)
                );
                if !extra.is_empty() {
                    println!("        {}", extra);
                }
            }
        }
    }

    Ok(())
}

/// `update-data` 命令：游戏版本更新后的一键数据更新。
/// 链路：下载 BlitzKit pb（--offline 可跳过）→ 重建 tank_cache.json → 提取 game_data/
/// （版本变化或 --force 全量重提，否则增量补缺失的新坦克）→ 刷新 data_version.json 清单。
/// 注意顺序：必须先写新 tanks.pb 再调 load_tanks（OnceLock 进程内缓存），否则读到旧数据。
#[allow(clippy::too_many_arguments)] // CLI 子命令参数透传，聚合结构反而不透明
fn cmd_update_data(
    game_dir: Option<&Path>,
    output: &Path,
    tank_cache_path: &Path,
    game_data_dir: &Path,
    offline: bool,
    force: bool,
    check: bool,
    icons: bool,
    models: bool,
) -> Result<()> {
    use wotb_agent::wargaming::data_version::{now_rfc3339, DataVersionManifest};
    use wotb_agent::wargaming::game_extract as ge;

    let gdir = ge::resolve_game_dir(game_dir).ok();
    let current_version = gdir.as_deref().and_then(ge::game_version);

    // 版本变化判定：清单已记录 → 直接比对；清单缺失 → version.txt.dvpl 的 mtime
    // 晚于 game_data 最新文件则视为游戏已更新（mtime 兜底启发式）。
    let manifest = DataVersionManifest::load();
    let version_changed = match current_version.as_deref() {
        Some(cur) if manifest.has_version() => cur != manifest.game_version,
        Some(_) => match (&gdir, game_data_dir.exists()) {
            (Some(d), true) => ge::game_dir_newer_than_data(d, game_data_dir),
            _ => false,
        },
        None => false,
    };

    println!("=== Data Update (game-version aware) ===");
    println!(
        "  Game dir:       {}",
        gdir.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<not found>".into())
    );
    println!(
        "  Game version:   {} (manifest: {})",
        current_version.as_deref().unwrap_or("<unknown>"),
        if manifest.has_version() {
            manifest.game_version.as_str()
        } else {
            "<none>"
        }
    );
    println!(
        "  Version change: {}",
        if version_changed { "YES" } else { "no" }
    );

    if check {
        println!(
            "  Plan: {} -> rebuild {} -> {} ({}); refresh data_version.json{}",
            if offline {
                "rebuild from existing pb"
            } else {
                "download BlitzKit pb"
            },
            tank_cache_path.display(),
            game_data_dir.display(),
            if force || version_changed {
                "full re-extract"
            } else {
                "incremental, missing only"
            },
            if models { " + GLB models preload" } else { "" }
        );
        println!("  --check mode: nothing was changed.");
        return Ok(());
    }

    if gdir.is_none() {
        anyhow::bail!("Game directory not found (needed for version detection and extraction). Pass --game-dir explicitly.");
    }

    // 1. BlitzKit pb（tanks.pb + models.pb 一起下载；换源转正（2026-10-15）后本机编码的
    //    pb 是现役来源，此步仅作对照/兜底——日常刷新用 --offline 保留本机 pb）
    if !offline {
        eprintln!("Downloading BlitzKit definitions ...");
        let n = tokio::runtime::Runtime::new()?
            .block_on(wotb_agent::wargaming::blitzkit::fetch_and_save(output))?;
        println!("  BlitzKit: downloaded tanks.pb + models.pb ({} tanks)", n);
    } else {
        println!("  BlitzKit: skipped (--offline)");
    }

    // 2. 提取 game_data（版本变化或 --force 全量重提，否则增量补缺失）
    //    必须先于 tank_cache 重建：装甲摘要的客户端优先链读的就是 game_data；旧顺序
    //    在空目录上重建 cache 会静默落到“取最大板厚”近似（2026-11-06 超测 r2/r3 事故）。
    let full = force || version_changed;
    if full {
        eprintln!("Re-extracting ALL game_data (version changed or --force) ...");
    }
    let stats = ge::extract_all(game_dir, game_data_dir, full)?;
    println!(
        "  Game data: extracted={} cached={} missing={} failed={} ({})",
        stats.extracted,
        stats.cached,
        stats.missing_files,
        stats.parse_failed + stats.write_failed,
        if full { "full" } else { "incremental" }
    );
    let tank_total = wotb_agent::wargaming::blitzkit::load_tanks().len();
    // 以磁盘上的实际覆盖计数（--force 时 stats 只统计本次重提，预置的基线文件不算
    // cached；用 stats 会把 743/743 误报成 5/743，也会写错 data_version）。
    let game_data_files = std::fs::read_dir(game_data_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
                .count()
        })
        .unwrap_or(0);
    if tank_total > 0 && game_data_files == 0 {
        anyhow::bail!(
            "game_data 为空（{} 辆坦克，0 份提取）：拒绝在空装甲模型上重建 tank_cache \
             （否则所有装甲摘要会回退成“取最大板厚”近似）",
            tank_total
        );
    }
    if game_data_files < tank_total {
        eprintln!(
            "  warning: game_data 覆盖 {}/{} 辆（缺 {} 辆）；缺失车的装甲摘要将回退 BlitzKit 近似",
            game_data_files,
            tank_total,
            tank_total - game_data_files
        );
    }

    // 3. 重建 tank_cache.json（与 fetch-tanks 同链路；此时 game_data 已就位）
    let resolver = TankResolver::from_blitzkit()?;
    resolver.save_to_json_file(tank_cache_path)?;
    let tank_count = Some(resolver.len());
    println!(
        "  Tank cache: rebuilt {} ({} tanks)",
        tank_cache_path.display(),
        resolver.len()
    );

    // 4. 可选：补缺失的坦克图标 / GLB 模型预热（只补缺失项，已有缓存自动跳过）
    if icons {
        let dir = wotb_agent::data::data_path("cache/tank_images");
        let (downloaded, cached, failed) =
            wotb_agent::wargaming::blitzkit::download_all_icons(&dir, false)?;
        println!(
            "  Icons: downloaded={} cached={} failed={} -> {}",
            downloaded,
            cached,
            failed,
            dir.display()
        );
    }

    // 4.5 可选：GLB 模型全量预热
    if models {
        let (downloaded, cached, failed, bytes) = tokio::runtime::Runtime::new()?.block_on(
            wotb_agent::wargaming::model_fetch::fetch_all_models(false, 6),
        )?;
        println!(
            "  Models: downloaded={} cached={} failed={} ({:.2} GB) -> data/cache/models/",
            downloaded,
            cached,
            failed,
            bytes as f64 / 1024.0 / 1024.0 / 1024.0
        );
    }

    // 5. 孤立 game_data（版本更新后被移除的坦克）：只提示不删除
    let orphans = ge::orphan_game_data_ids(game_data_dir);
    if !orphans.is_empty() {
        println!(
            "  Orphan game_data (not in current tanks.pb): {} files, e.g. {:?}",
            orphans.len(),
            &orphans[..orphans.len().min(5)]
        );
    }

    // 6. 刷新版本清单
    DataVersionManifest {
        game_version: current_version.unwrap_or_default(),
        tank_cache_updated_at: Some(now_rfc3339()),
        game_data_updated_at: Some(now_rfc3339()),
        tank_count,
        game_data_files: Some(game_data_files),
    }
    .save()?;

    println!("  Manifest: data/data_version.json updated");
    println!(
        "Note: armor summary prefers client game_data/{{id}}.json (primary plates); \
         missing game_data falls back to BlitzKit models.pb max-plate approximation."
    );
    Ok(())
}

fn print_single_replay(summary: &wotb_agent::models::battle::BattleSummary) {
    println!();
    println!("========================================================");
    println!("  Single Replay: {}", summary.file_name);
    println!("========================================================");
    println!();
    println!(
        "  Player:     {} (id={})",
        summary.author_nickname, summary.author_account_id
    );
    println!(
        "  Tank:       {} (id={})",
        summary.author_tank_name, summary.author_tank_id
    );
    println!(
        "  Map:        {} (id={:#06x})",
        summary.map_name, summary.map_id
    );
    println!("  Mode:       {:?}", summary.room_type);
    println!("  Duration:   {:.1}s", summary.battle_duration_secs);
    println!("  Time:       {}", summary.datetime);
    println!("  Winner:     Team {}", summary.winner_team);
    println!(
        "  Author:     Team {} ({})",
        summary.author_team,
        if summary.author_won { "WON" } else { "LOST" }
    );
    println!();

    let a = &summary.author;
    println!("--- Author Stats ---");
    println!(
        "  HP left:       {} {}",
        a.hitpoints_left,
        if a.is_auto_destroyed {
            "(AUTO-DESTROYED)"
        } else {
            ""
        }
    );
    println!("  Credits:       {}", a.total_credits);
    println!("  XP:            {}", a.total_xp);
    println!(
        "  Shots/Hits:    {}/{} ({:.0}%)",
        a.n_shots,
        a.n_hits,
        if a.n_shots > 0 {
            a.n_hits as f64 / a.n_shots as f64 * 100.0
        } else {
            0.0
        }
    );
    println!("  Penetrations:  {}", a.n_penetrations);
    println!("  Splashes:      {}", a.n_splashes);
    println!("  Damage:        {}", a.damage_dealt);
    println!();

    println!("--- All Players ({} total) ---", summary.players.len());
    println!(
        "  {:<3} {:<25} {:<5} {:<7} {:<6} {:>5} {:>5} {:>5} {:>7} {:>7} {:>5} {:>7}",
        "#",
        "Nickname",
        "Team",
        "Tank",
        "XP",
        "Shots",
        "Hits",
        "Pens",
        "Dmg",
        "Block",
        "Kills",
        "mmRat"
    );
    println!("  {}", "-".repeat(95));
    for (i, p) in summary.players.iter().enumerate() {
        let is_author = p.account_id == summary.author_account_id;
        let marker = if is_author { "*" } else { " " };
        println!(
            "{} {:<2} {:<25} {:<5} {:<7} {:<6} {:>5} {:>5} {:>5} {:>7} {:>7} {:>5} {:>7.1}",
            marker,
            i + 1,
            p.nickname,
            p.team,
            p.tank_name,
            p.base_xp,
            p.n_shots,
            p.n_hits_dealt,
            p.n_penetrations_dealt,
            p.damage_dealt,
            p.damage_blocked,
            p.n_enemies_destroyed,
            p.mm_rating.unwrap_or(0.0)
        );
    }
    println!("\n  (* = replay author)");

    // 结算补充字段（battle_results #301 crate 未暴露部分：死因/寿命/点亮/毁灭协助/炮印）
    println!("--- Settlement（存活/死因/寿命/点亮/毁灭协助/炮印） ---");
    for p in &summary.players {
        let fate = match p.death_reason {
            Some(-1) => format!("幸存(寿命 {}s)", p.life_time_secs.unwrap_or(0)),
            Some(1) => format!("火焰(寿命 {}s)", p.life_time_secs.unwrap_or(0)),
            Some(2) => format!("撞车(寿命 {}s)", p.life_time_secs.unwrap_or(0)),
            Some(3) => format!("世界(寿命 {}s)", p.life_time_secs.unwrap_or(0)),
            Some(d) => format!("死因{d}"),
            None => "击毁".to_string(),
        };
        let killer = p
            .killer_id
            .map(|k| format!(" · 击杀者 {k}"))
            .unwrap_or_default();
        println!(
            "  {:<25} {} · 点亮 {} · 毁灭协助 {} · 炮印 {}{}",
            p.nickname,
            fate,
            p.n_enemies_spotted
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
            p.destruction_assistance
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
            p.gun_marks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
            killer
        );
    }
    println!("\n========================================================");
}

/// 逐个查询一队玩家战绩（search + stats），失败/未找到的玩家静默跳过。
/// label 用于日志（"我方"/"敌方"）。
fn query_lineup_stats<'a>(
    client: &WgApiClient,
    names: impl Iterator<Item = &'a str>,
    label: &str,
) -> Vec<wotb_agent::wargaming::api_client::PlayerStats> {
    let mut stats = Vec::new();
    for n in names {
        eprintln!("查询{}玩家 '{}'...", label, n);
        if let Ok(results) = client.search_player(n, true) {
            if !results.is_empty() {
                if let Ok(s) = client.get_player_stats(results[0].1) {
                    stats.push(s);
                }
            }
        }
    }
    stats
}
