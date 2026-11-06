use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;

use crate::agent::llm_client::ToolDefinition;
use crate::agent::llm_client::ToolFunction;
use crate::models::report::AggregatedReport;
use crate::replay::parser::ReplayParser;
use crate::replay::scanner::{ReplayScanner, ScanFilter};
use crate::wargaming::api_client::WgApiClient;
use crate::wargaming::blitzkit;
use crate::wargaming::penetration::{self, ArmorHit, ArmorSection, PenetrationRequest};
use crate::wargaming::tank_resolver::{norm_name, strip_ws, TankInfo, TankResolver};

// Agent 工具集：注册给 LLM 的工具定义 + 执行逻辑；Agent Loop 调用 execute() 并把结果回填为 tool 消息。

/// Agent 工具执行器：持有 WG API 客户端、可选坦克解析器、回放目录。
// Clone 供 chat_async 在 spawn_blocking 上执行工具（阻塞 IO 不得占用 async worker）。
#[derive(Clone)]
pub struct AgentTools {
    pub wg_client: WgApiClient,
    pub tank_resolver: Option<TankResolver>,
    pub replay_dir: String,
}

impl AgentTools {
    /// 构造工具执行器；`tank_cache` 存在则加载为坦克解析器（tank_id → 名称翻译）。
    pub fn new(app_id: &str, server: &str, replay_dir: &str, tank_cache: Option<&Path>) -> Self {
        let tank_resolver = tank_cache
            .filter(|p| p.exists())
            .and_then(|p| TankResolver::load_from_json_file(p).ok());
        Self {
            wg_client: WgApiClient::new(app_id, server),
            tank_resolver,
            replay_dir: replay_dir.to_string(),
        }
    }

    /// 返回注册给 LLM 的工具定义（OpenAI tools 格式）。
    pub fn definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "search_player".to_string(),
                    description: "Search for a WoTB player by nickname. Returns account_id and nickname.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "nickname": {"type": "string", "description": "Player nickname to search"}
                        },
                        "required": ["nickname"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "get_player_stats".to_string(),
                    description: "Get player cumulative stats from WG API, including random and rating battle stats, win rate, avg damage, hit rate, rating mm_rating.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "account_id": {"type": "integer", "description": "Player account ID"}
                        },
                        "required": ["account_id"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "scan_replays".to_string(),
                    description: "Scan replay files in the replay directory and generate an aggregated report. Can filter by mode (rating/regular/all) and days. Returns battle count, win rate, avg damage, tank usage, map stats, rating changes.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "mode": {"type": "string", "description": "Filter by game mode: rating, regular, or all", "default": "all"},
                            "days": {"type": "integer", "description": "Only include replays from last N days. Omit for all."}
                        }
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "parse_replay".to_string(),
                    description: "Parse a single replay file and return detailed battle results including all 14 players' stats, author stats, map, mode, duration, winner.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "file_path": {"type": "string", "description": "Path to the .wotbreplay file"}
                        },
                        "required": ["file_path"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "compare_replay_vs_api".to_string(),
                    description: "Compare recent replay stats vs WG API cumulative stats. Shows how the player is performing recently vs their all-time average.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "nickname": {"type": "string", "description": "Player nickname"},
                            "mode": {"type": "string", "description": "Filter: rating, regular, or all", "default": "rating"}
                        },
                        "required": ["nickname"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "view_tank".to_string(),
                    description: "Open the 3D armor viewer for one or two tanks so the user can inspect armor, rotate the turret/gun, and simulate shell penetration. Use when the user asks to see/visualize a tank's armor, compare two tanks, or simulate a specific shooter (attacking) tank vs a target (defending) tank. Pass the tank name or numeric tank ID for `target` (the tank being viewed/inspected) and optionally `shooter` (the tank firing the shell). Both are fuzzy-matched: if a name matches multiple tanks, a numbered candidate list is returned for the user to disambiguate.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "target": {"type": "string", "description": "Tank to view/inspect (defending side). Name (e.g. 'E 100') or numeric ID (e.g. 7169). Fuzzy match."},
                            "shooter": {"type": "string", "description": "Shooting/attacking tank whose shell & caliber are used for penetration (optional). Name or numeric ID, fuzzy match. Omit to default to the target."},
                            "port": {"type": "integer", "description": "Preferred port (optional, default 0 = auto-assign)"}
                        },
                        "required": ["target"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "get_tank_armor".to_string(),
                    description: "Get a tank's full armor breakdown: hull/turret front-side-rear summary, per-plate thickness list with spaced-armor classification (spaced plates are angle-effective layers like primary armor - only external modules such as tracks/gun barrel are flat; a shot that crosses nothing but spaced plates/external modules is not a tank penetration in BlitzKit's armor inspector), and the top-config gun's shells with formal types (AP/APCR/HEAT/HE), penetration near/far, damage, module damage and HE explosion radius. Call this before simulate_penetration to inspect available plates.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "target": {"type": "string", "description": "Tank name (e.g. 'E 100', 'Jg.Pz. E 100') or numeric ID. Fuzzy match."}
                        },
                        "required": ["target"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "simulate_penetration".to_string(),
                    description: "Simulate a shell penetration: shooter tank's shell vs target tank's armor, matching BlitzKit's shoot() judgment step by step (ricochet, normalization, overmatch, spaced plates, HEAT gap decay, HE splash, calibrated shells/enhanced armor; near penetration value only - no distance decay). Result follows the LAST layer's status: the shot is a PENETRATION (full armor damage) iff the final layer is penetrated - so a stack of only tracks/spaced plates yields PENETRATION (per BlitzKit's shoot() semantics; note BlitzKit's armor inspector UI only starts a judgment when the ray reaches main armor - hull/turret/gun - so it never displays that shot). A ricochet keeps 75% penetration and the reflected ray is re-judged. Provide `aim` preset OR explicit `hits` (see get_tank_armor for available plates).".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "target": {"type": "string", "description": "Target (defending) tank name or ID. Fuzzy match."},
                            "shooter": {"type": "string", "description": "Shooter (attacking) tank name or ID. Fuzzy match."},
                            "shell": {"type": "string", "description": "Shell type filter: AP, APCR, HEAT or HE (optional, default = the gun's first shell)"},
                            "aim": {"type": "string", "description": "Aim preset: hull_front, hull_side, hull_rear, turret_front, turret_side, turret_rear. Builds a single-plate hit from the armor summary.", "enum": ["hull_front","hull_side","hull_rear","turret_front","turret_side","turret_rear"]},
                            "hits": {"type": "array", "description": "Explicit layer stack (advanced, overrides aim). Order = the order the shell crosses them. Items: {section: hull|turret|gun|spaced|chassis|gunBarrel, thickness_mm: number}. hull/turret/gun are angle-effective primary armor (can ricochet/normalize, blitzkit Primary); spaced is angle-effective spaced armor; chassis/gunBarrel are flat external modules (blitzkit External).", "items": {"type": "object", "properties": {"section": {"type": "string", "enum": ["hull","turret","gun","spaced","chassis","gunBarrel"]}, "thickness_mm": {"type": "number"}}, "required": ["section","thickness_mm"]}},
                            "angle_deg": {"type": "number", "description": "Impact angle in degrees. 0 = perpendicular to the plate. Default 0."},
                            "distance_m": {"type": "number", "description": "Engagement distance in meters (default 100). Penetration decays linearly from near to far over the shell's range."},
                            "calibrated_shells": {"type": "boolean", "description": "Calibrated Shells equipment: penetration +6% (AP/APCR) / +7% (HEAT/HE). Default false."},
                            "enhanced_armor": {"type": "boolean", "description": "Enhanced Armor equipment on the target: armor thickness +4%. Default false."}
                        },
                        "required": ["target", "shooter"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "replay_shot".to_string(),
                    description: "Replay a specific shot from a .wotbreplay file in the 3D armor viewer: extracts the recorded shooter/target positions and orientations, places the camera at the shooter's viewpoint aiming at the target, overlays the penetration heat map, and saves a screenshot PNG. Use after analyzing which shot to inspect. shot_no is 1-based over the detected hits.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "replay_file": {"type": "string", "description": "Path to the .wotbreplay file"},
                            "shot_no": {"type": "integer", "description": "1-based shot (hit) index to reproduce", "default": 1}
                        },
                        "required": ["replay_file"]
                    }),
                },
            },
            ToolDefinition {
                def_type: "function".to_string(),
                function: ToolFunction {
                    name: "render_heatmap".to_string(),
                    description: "Render a penetration heat-map screenshot of the target tank's 3D armor viewer and save it as a PNG. The heat map colors armor faces by penetration chance for the shooter's shell (green=likely penetration, red=blocked, magenta=ricochet, orange=HE splash). Supports turret yaw, gun pitch and camera view. Requires Chrome/Chromium (auto-detected; WSL can use the Windows install, or set the CHROME_PATH env var). ALWAYS embed the returned image in your final reply as a markdown image: ![heatmap](/screenshots/<file>.png) - the user sees images only if you embed them.".to_string(),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "target": {"type": "string", "description": "Target (defending) tank name or ID. Fuzzy match."},
                            "shooter": {"type": "string", "description": "Shooter (attacking) tank whose shell is visualized. Optional, default = the target."},
                            "shell": {"type": "string", "description": "Shell type filter: AP, APCR, HEAT or HE (optional, default = the gun's first shell)"},
                            "yaw_deg": {"type": "number", "description": "Turret rotation in degrees (optional, default 0)"},
                            "pitch_deg": {"type": "number", "description": "Gun elevation in degrees (optional, default 0)"},
                            "view": {"type": "string", "description": "Camera view preset. front/rear/left/right = LEVEL view at gun line height (default front). hull_down = low camera looking up at the turret over the hull (卖头视角). top = from above. front_left/front_right/rear_left/rear_right = level 3/4 views.", "enum": ["front","rear","left","right","top","hull_down","front_left","front_right","rear_left","rear_right"]},
                            "width": {"type": "integer", "description": "Screenshot width in px (default 1280)"},
                            "height": {"type": "integer", "description": "Screenshot height in px (default 800)"}
                        },
                        "required": ["target"]
                    }),
                },
            },
        ]
    }

    /// 按工具名分发执行，返回人类可读的结果文本（会回填为 tool 消息）。
    pub fn execute(&self, tool_name: &str, args: &Value) -> Result<String> {
        match tool_name {
            "search_player" => {
                let nickname = args["nickname"].as_str().unwrap_or("");
                let results = self.wg_client.search_player(nickname, false)?;
                if results.is_empty() {
                    Ok("No players found.".to_string())
                } else {
                    let lines: Vec<String> = results
                        .iter()
                        .map(|(n, id)| format!("  {} (account_id={})", n, id))
                        .collect();
                    Ok(format!(
                        "Found {} players:\n{}",
                        results.len(),
                        lines.join("\n")
                    ))
                }
            }
            "get_player_stats" => {
                let account_id = args["account_id"].as_u64().unwrap_or(0) as u32;
                let stats = self.wg_client.get_player_stats(account_id)?;
                let n_r = stats.random_battles.max(1) as f64;
                let n_t = stats.rating_battles.max(1) as f64;
                Ok(format!(
                    "Player: {} (id={})\n\
                     \n--- Random Battles ---\n\
                     Battles: {}, WR: {:.1}%, Avg Dmg: {:.0}, Avg Frags: {:.2}, Hit Rate: {:.1}%\n\
                     \n--- Rating Battles ---\n\
                     Battles: {}, WR: {:.1}%, Avg Dmg: {:.0}, Avg Frags: {:.2}, Hit Rate: {:.1}%\n\
                     mm_rating: {:.2}, Display Rating: {}, Season: {}",
                    stats.nickname,
                    stats.account_id,
                    stats.random_battles,
                    stats.random_wins as f64 / n_r * 100.0,
                    stats.random_damage_dealt as f64 / n_r,
                    stats.random_frags as f64 / n_r,
                    if stats.random_shots > 0 {
                        stats.random_hits as f64 / stats.random_shots as f64 * 100.0
                    } else {
                        0.0
                    },
                    stats.rating_battles,
                    stats.rating_wins as f64 / n_t * 100.0,
                    stats.rating_damage_dealt as f64 / n_t,
                    stats.rating_frags as f64 / n_t,
                    if stats.rating_shots > 0 {
                        stats.rating_hits as f64 / stats.rating_shots as f64 * 100.0
                    } else {
                        0.0
                    },
                    stats.rating_mm_rating.unwrap_or(0.0),
                    stats.rating_display_rating.unwrap_or(0),
                    stats.rating_season.unwrap_or(0),
                ))
            }
            "scan_replays" => {
                let mode = args["mode"].as_str().unwrap_or("all");
                let days = args["days"].as_i64();

                let scanner = self.scanner();

                let filter = ScanFilter::from_mode(mode, days);

                let dir = Path::new(&self.replay_dir);
                let battles = scanner.scan_dir(dir, &filter, |_| {})?;

                if battles.is_empty() {
                    return Ok("No replays found matching the filter.".to_string());
                }

                let room_type = battles
                    .first()
                    .map(|b| b.room_type.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let report = AggregatedReport::from_battles(battles, &room_type);

                let mut result = format!(
                    "Replay Scan Results:\n\
                     Player: {}, Mode: {}, Date: {}\n\
                     Battles: {}, WR: {:.1}%, Avg Dmg: {:.0}, Avg Frags: {:.2}\n\
                     Hit Rate: {:.1}%, Avg Block: {:.0}, Avg Assist: {:.0}\n\
                     Rating: {:.2} -> {:.2} ({:+.2})\n\
                     \nTop Tanks:\n",
                    report.author_name,
                    report.room_type,
                    report.date_range,
                    report.total_battles,
                    report.win_rate,
                    report.avg_damage,
                    report.avg_frags,
                    report.hit_rate,
                    report.avg_damage_blocked,
                    report.avg_assisted,
                    report.rating_start.unwrap_or(0.0),
                    report.rating_end.unwrap_or(0.0),
                    report.rating_delta.unwrap_or(0.0),
                );

                for t in report.tank_usage.iter().take(5) {
                    result.push_str(&format!(
                        "  {} - {}b, WR {:.0}%, avg_dmg {:.0}\n",
                        t.tank_name, t.battles, t.win_rate, t.avg_damage
                    ));
                }

                Ok(result)
            }
            "parse_replay" => {
                let file_path = args["file_path"].as_str().unwrap_or("");
                let parser = self.parser();
                let summary = parser.parse_file(Path::new(file_path))?;

                Ok(format!(
                    "Replay: {}\nPlayer: {} (tank: {})\nMap: {}, Mode: {}, Duration: {:.0}s\n\
                     Winner: Team {}\nAuthor: Team {} ({})\n\
                     Shots: {}/{}, Pens: {}, Damage: {}, Kills: {}",
                    summary.file_name,
                    summary.author_nickname,
                    summary.author_tank_name,
                    summary.map_name,
                    summary.room_type,
                    summary.battle_duration_secs,
                    summary.winner_team,
                    summary.author_team,
                    if summary.author_won { "WON" } else { "LOST" },
                    summary.author.n_shots,
                    summary.author.n_hits,
                    summary.author.n_penetrations,
                    summary.author.damage_dealt,
                    summary
                        .players
                        .iter()
                        .find(|p| p.account_id == summary.author_account_id)
                        .map(|p| p.n_enemies_destroyed)
                        .unwrap_or(0),
                ))
            }
            "compare_replay_vs_api" => {
                let nickname = args["nickname"].as_str().unwrap_or("");
                let mode = args["mode"].as_str().unwrap_or("rating");
                let results = self.wg_client.search_player(nickname, true)?;
                if results.is_empty() {
                    return Ok(format!("Player not found: {}", nickname));
                }
                let account_id = results[0].1;
                let api_stats = self.wg_client.get_player_stats(account_id)?;

                let scanner = self.scanner();

                let filter = ScanFilter::from_mode(mode, None);

                let battles = scanner.scan_dir(Path::new(&self.replay_dir), &filter, |_| {})?;
                if battles.is_empty() {
                    return Ok("No replays found.".to_string());
                }

                let room_type = battles
                    .first()
                    .map(|b| b.room_type.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let report = AggregatedReport::from_battles(battles, &room_type);

                let is_rating = mode == "rating";
                let (api_battles, api_wins, api_dmg, api_frags, api_shots, api_hits) = if is_rating
                {
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

                let an = api_battles.max(1) as f64;
                let api_hit_rate = if api_shots > 0 {
                    api_hits as f64 / api_shots as f64 * 100.0
                } else {
                    0.0
                };
                Ok(format!(
                    "Comparison: {} ({} mode)\n\
                     \n  Metric          Replay     API Total   Difference\n\
                     Battles:         {}          {}          \n\
                     Win Rate:        {:.1}%      {:.1}%      {:+.1}%\n\
                     Avg Damage:      {:.0}        {:.0}        {:+.0}\n\
                     Avg Frags:       {:.2}        {:.2}        {:+.2}\n\
                     Hit Rate:        {:.1}%      {:.1}%      {:+.1}%\n\
                     \nReplay Rating: {:.2} -> {:.2} ({:+.2})\n\
                     API Rating: {:.2} (display {})\n\
                     Coverage: {:.1}%",
                    nickname,
                    mode,
                    report.total_battles,
                    api_battles,
                    report.win_rate,
                    api_wins as f64 / an * 100.0,
                    report.win_rate - api_wins as f64 / an * 100.0,
                    report.avg_damage,
                    api_dmg as f64 / an,
                    report.avg_damage - api_dmg as f64 / an,
                    report.avg_frags,
                    api_frags as f64 / an,
                    report.avg_frags - api_frags as f64 / an,
                    report.hit_rate,
                    api_hit_rate,
                    report.hit_rate - api_hit_rate,
                    report.rating_start.unwrap_or(0.0),
                    report.rating_end.unwrap_or(0.0),
                    report.rating_delta.unwrap_or(0.0),
                    api_stats.rating_mm_rating.unwrap_or(0.0),
                    api_stats.rating_display_rating.unwrap_or(0),
                    report.total_battles as f64 / api_battles.max(1) as f64 * 100.0,
                ))
            }
            "view_tank" => self.execute_view_tank(args),
            "get_tank_armor" => self.execute_get_tank_armor(args),
            "simulate_penetration" => self.execute_simulate_penetration(args),
            "render_heatmap" => self.execute_render_heatmap(args),
            "replay_shot" => self.execute_replay_shot(args),
            _ => Ok(format!("Unknown tool: {}", tool_name)),
        }
    }

    /// view_tank：target/shooter 模糊匹配（数字 ID 直用；名称优先精确、其次最短子串；
    /// 多候选返回编号列表由用户选择）。viewer::serve 为阻塞服务，在独立线程 + tokio
    /// runtime 中启动，不卡 Agent 循环。
    fn execute_view_tank(&self, args: &Value) -> Result<String> {
        let target_ref = args["target"].as_str().unwrap_or("").trim();
        let shooter_ref = args["shooter"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        if target_ref.is_empty() {
            return Ok(
                "Please provide a tank name or ID to view (e.g. target: 'E 100').".to_string(),
            );
        }

        let resolver = self.load_resolver()?;

        let target = match self.resolve_tank_or_message(&resolver, target_ref, "name", "a tank", true) {
            Ok(id) => id,
            Err(msg) => return Ok(msg),
        };

        let shooter = match shooter_ref {
            Some(sref) => match self.resolve_tank_or_message(&resolver, sref, "shooter name", "a shooter tank", true) {
                Ok(id) => Some(id),
                Err(msg) => return Ok(msg),
            },
            None => None,
        };

        let target_name = resolver
            .resolve(target)
            .unwrap_or_else(|| format!("tank_{}", target));
        let shooter_name = shooter.map(|id| {
            resolver
                .resolve(id)
                .unwrap_or_else(|| format!("tank_{}", id))
        });

        // 服务器线程与旧标签页共存是既定行为（每次 view_tank 新端口新页签，
        // 旧页签靠旧服务器存活）；泄漏有界，见架构债文档次级清单
        std::thread::spawn(move || {
            let _ =
                tool_runtime().block_on(crate::wargaming::viewer::serve(resolver, target, shooter));
        });

        let desc = match (&shooter_name, shooter) {
            (Some(sname), Some(_)) => format!(
                "**{}** (attacking) → **{}** (defending)",
                sname, target_name
            ),
            (_, _) => format!("**{}**", target_name),
        };
        Ok(format!(
            "Opened the 3D armor viewer for {}. A browser window should have appeared showing the tank model, armor plates, and penetration analysis. The user can rotate the view, select a shell, switch shooter/target via the tank picker, and click on armor to test penetration.",
            desc
        ))
    }

    /// 弹种正式显示名（AP/APCR/HEAT/HE）。
    fn shell_label(shell_type: &str) -> String {
        match shell_type.to_lowercase().as_str() {
            "ap" | "ap_premium" => "AP".to_string(),
            "apcr" | "ap_cr" | "ap_cr_premium" => "APCR".to_string(),
            "heat" | "hc" | "hc_premium" => "HEAT".to_string(),
            "he" | "he_premium" => "HE".to_string(),
            other => other.to_uppercase(),
        }
    }

    /// 加载坦克解析器（tank_cache.json，与 execute_view_tank 相同）。
    fn load_resolver(&self) -> Result<TankResolver> {
        TankResolver::load_from_json_file(&crate::data::data_path("tank_cache.json")).or_else(
            |_| {
                self.tank_resolver
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("no resolver"))
            },
        )
    }

    /// 构建 ReplayScanner：有坦克解析器则带车名翻译，否则退化为 ID 显示。
    fn scanner(&self) -> ReplayScanner<'_> {
        if let Some(ref r) = self.tank_resolver {
            ReplayScanner::with_resolver(r)
        } else {
            ReplayScanner::new()
        }
    }

    /// 构建 ReplayParser：有坦克解析器则带车名翻译，否则退化为 ID 显示。
    fn parser(&self) -> ReplayParser<'_> {
        if let Some(ref r) = self.tank_resolver {
            ReplayParser::with_resolver(r)
        } else {
            ReplayParser::new()
        }
    }

    /// get_tank_armor 工具：装甲汇总 + 逐板明细（含 spaced 分类）+ 顶配弹种。
    fn execute_get_tank_armor(&self, args: &Value) -> Result<String> {
        let target_ref = args["target"].as_str().unwrap_or("").trim();
        if target_ref.is_empty() {
            return Ok("Please provide a tank name or ID (e.g. target: 'E 100').".to_string());
        }
        let resolver = self.load_resolver()?;
        let target = match self.resolve_tank_or_message(&resolver, target_ref, "name", "a tank", false) {
            Ok(id) => id,
            Err(msg) => return Ok(msg),
        };
        let name = resolver
            .resolve(target)
            .unwrap_or_else(|| format!("tank_{}", target));

        // 装甲汇总（前/侧/后，mm）与血量（一次 resolve_info 复用）
        let info = resolver.resolve_info(target);
        let summary = info.and_then(|i| i.armor.clone());
        // 逐板厚度（models.pb 唯一来源）
        let plates: Value = crate::wargaming::tank_configs::synth_armor_model(target)
            .and_then(|m| serde_json::to_value(m).ok())
            .unwrap_or(json!(null));

        // spaced 分类 + 顶配弹种（models.pb；顶配 = turrets.at(-1).guns.at(-1)）
        let mut sp: Value = json!(null);
        let mut shells_json: Vec<Value> = Vec::new();
        let mut gun_name = String::new();
        let hp = info.and_then(|i| i.hp);
        if let Some(tank) = blitzkit::tank_full(target) {
            let mi = blitzkit::model_info(target);
            let mut sp_map = serde_json::Map::new();
            sp_map.insert(
                "hull".into(),
                json!(mi
                    .as_ref()
                    .map(|m| m.hull_spaced.clone())
                    .unwrap_or_default()),
            );
            if let Some(t) = tank.turrets.last() {
                // 同一炮塔只查一次，turret/gun 的 spaced 分类复用
                let ti = mi
                    .as_ref()
                    .and_then(|m| m.turrets.iter().find(|x| x.module_id == t.module_id));
                if let Some(ti) = ti {
                    sp_map.insert("turret".into(), json!(ti.turret_spaced.clone()));
                }
                if let Some(g) = t.guns.last() {
                    gun_name = g.name.clone();
                    if let Some(gi) =
                        ti.and_then(|x| x.guns.iter().find(|y| y.gun_module_id == g.module_id))
                    {
                        sp_map.insert("gun".into(), json!(gi.gun_spaced.clone()));
                    }
                    for s in &g.shells {
                        shells_json.push(json!({
                            "label": Self::shell_label(&s.shell_type),
                            "type": s.shell_type,
                            "penetration_mm": s.penetration,
                            "penetration_far_mm": if s.penetration_far > 0.0 { json!(s.penetration_far) } else { Value::Null },
                            "damage_hp": s.damage,
                            "module_damage": s.module_damage,
                            "explosion_radius_m": if s.explosion_radius > 0.0 { json!(s.explosion_radius) } else { Value::Null },
                            "caliber_mm": s.caliber,
                        }));
                    }
                }
            }
            sp = Value::Object(sp_map);
        }

        Ok(json!({
            "tank": name,
            "hp_total": hp,
            "hp_note": "total = hull health + turret health",
            "armor_summary_mm": summary,
            "plates": plates,
            "spaced": sp,
            "top_gun": gun_name,
            "shells": shells_json,
        })
        .to_string())
    }

    /// simulate_penetration 工具：对齐 BlitzKit 的击穿模拟（penetration.rs）。
    fn execute_simulate_penetration(&self, args: &Value) -> Result<String> {
        let target_ref = args["target"].as_str().unwrap_or("").trim();
        let shooter_ref = args["shooter"].as_str().unwrap_or("").trim();
        if target_ref.is_empty() || shooter_ref.is_empty() {
            return Ok("Please provide both `target` (defending) and `shooter` (attacking) tank names or IDs.".to_string());
        }
        let resolver = self.load_resolver()?;
        let target = match self.resolve_tank_or_message(&resolver, target_ref, "target name", "a target tank", false) {
            Ok(id) => id,
            Err(msg) => return Ok(msg),
        };
        let shooter = match self.resolve_tank_or_message(&resolver, shooter_ref, "shooter name", "a shooter tank", false) {
            Ok(id) => id,
            Err(msg) => return Ok(msg),
        };
        let target_name = resolver
            .resolve(target)
            .unwrap_or_else(|| format!("tank_{}", target));
        let shooter_name = resolver
            .resolve(shooter)
            .unwrap_or_else(|| format!("tank_{}", shooter));

        let Some(tank) = blitzkit::tank_full(shooter) else {
            return Ok(format!("No shell data for shooter {}.", shooter_name));
        };
        let Some(turret) = tank.turrets.last() else {
            return Ok(format!("No turret data for shooter {}.", shooter_name));
        };
        let Some(gun) = turret.guns.last() else {
            return Ok(format!("No gun data for shooter {}.", shooter_name));
        };
        if gun.shells.is_empty() {
            return Ok(format!("No shells for shooter {}.", shooter_name));
        }
        let shell = match args["shell"].as_str() {
            Some(filt) => gun
                .shells
                .iter()
                .find(|s| {
                    Self::shell_label(&s.shell_type).eq_ignore_ascii_case(filt)
                        || s.shell_type.eq_ignore_ascii_case(filt)
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "shell '{}' not found; available: {}",
                        filt,
                        gun.shells
                            .iter()
                            .map(|s| Self::shell_label(&s.shell_type))
                            .collect::<Vec<_>>()
                            .join("/")
                    )
                })?,
            None => &gun.shells[0],
        };

        let angle = args["angle_deg"].as_f64().unwrap_or(0.0);
        let rad = (angle as f32).to_radians();
        let view = [0.0f32, 1.0, 0.0];
        let normal = [0.0f32, rad.cos(), rad.sin()];

        // 命中层：显式 hits 优先，否则 aim 预设（装甲汇总单板）
        let mut hits: Vec<ArmorHit> = Vec::new();
        let mut aim_desc = String::from("custom layers");
        if let Some(hits_arr) = args["hits"].as_array() {
            for (i, h) in hits_arr.iter().enumerate() {
                let section: ArmorSection =
                    serde_json::from_value(h["section"].clone()).map_err(|_| {
                        anyhow::anyhow!(
                            "invalid section '{}' (use hull/turret/gun/spaced/chassis/gunBarrel)",
                            h["section"].as_str().unwrap_or("?")
                        )
                    })?;
                let thickness = h["thickness_mm"].as_f64().unwrap_or(0.0) as f32;
                hits.push(ArmorHit {
                    section,
                    plate_id: format!("h{}", i + 1),
                    thickness,
                    normal,
                    point: [0.0, 0.0, 0.0],
                    part_name: format!(
                        "Layer {} ({})",
                        i + 1,
                        h["section"].as_str().unwrap_or("?")
                    ),
                });
            }
        } else if let Some(aim) = args["aim"].as_str() {
            let Some(summary) = resolver.resolve_info(target).and_then(|i| i.armor.clone()) else {
                return Ok(format!("No armor summary for {}. Use get_tank_armor to inspect, or provide explicit `hits`.", target_name));
            };
            let (section, thickness, part) = match aim {
                "hull_front" => (ArmorSection::Hull, summary.hull_front, "Hull Front"),
                "hull_side" => (ArmorSection::Hull, summary.hull_sides, "Hull Side"),
                "hull_rear" => (ArmorSection::Hull, summary.hull_rear, "Hull Rear"),
                "turret_front" => (ArmorSection::Turret, summary.turret_front, "Turret Front"),
                "turret_side" => (ArmorSection::Turret, summary.turret_sides, "Turret Side"),
                "turret_rear" => (ArmorSection::Turret, summary.turret_rear, "Turret Rear"),
                other => return Ok(format!("Unknown aim '{}'. Use hull_front/hull_side/hull_rear/turret_front/turret_side/turret_rear, or provide explicit `hits`.", other)),
            };
            if thickness == 0 {
                return Ok(format!("No armor data for {} on {}.", aim, target_name));
            }
            aim_desc = part.to_string();
            hits.push(ArmorHit {
                section,
                plate_id: "aim".into(),
                thickness: thickness as f32,
                normal,
                point: [0.0, 0.0, 0.0],
                part_name: part.into(),
            });
        }
        if hits.is_empty() {
            return Ok(
                "Provide `aim` (e.g. hull_front) or explicit `hits` for the simulation."
                    .to_string(),
            );
        }

        let distance = args["distance_m"].as_f64().unwrap_or(100.0) as f32;
        let req = PenetrationRequest {
            shell_type: shell.shell_type.clone(),
            shell_type_id: shell.shell_type_id, // field9 权威；词表仅作回退
            penetration: shell.penetration as f32,
            // 远距穿深：>0 才传入（数据缺失 → None → 判定不启用距离衰减，fail-safe）
            penetration_far: (shell.penetration_far > 0.0)
                .then_some(shell.penetration_far as f32),
            caliber: shell.caliber as f32,
            view_dir: view,
            hits,
            damage: shell.damage as f32,
            explosion_radius: shell.explosion_radius as f32,
            calibrated_shells: args["calibrated_shells"].as_bool().unwrap_or(false),
            enhanced_armor: args["enhanced_armor"].as_bool().unwrap_or(false),
            // blitzkit：normalization ?? 0；ricochet 仅非 explosive 弹使用（HEAT/HE 强制 90°）
            normalization_deg: Some(shell.normalization as f32),
            ricochet_deg: if shell.ricochet > 0.0 {
                Some(shell.ricochet as f32)
            } else {
                None
            },
            allow_ricochet: true,
            require_primary: false,
            // 距离衰减：d>100 且 far 存在时启用（对齐客户端指示器 0x719740）
            distance_m: (distance > 0.0).then_some(distance),
        };
        let res = penetration::calculate(&req);

        let mut out = format!(
            "Result: **{}**\n{} (shell {} {:.0}mm) vs {}\nAim: {} · Angle {:.0}° · Distance {:.0}m\nLayers:\n",
            res.result, shooter_name, Self::shell_label(&shell.shell_type), shell.caliber, target_name, aim_desc, angle, distance
        );
        for (i, l) in res.layers.iter().enumerate() {
            let status = if l.ricochet {
                "RICOCHET".to_string()
            } else if l.penetrated {
                format!(
                    "penetrated, remaining pen {:.0}mm",
                    (l.remaining_before - l.effective).max(0.0)
                )
            } else {
                "BLOCKED here".to_string()
            };
            out.push_str(&format!(
                "  {}. {} — nominal {:.0}mm / effective {:.0}mm — {}\n",
                i + 1,
                l.part_name,
                l.thickness,
                l.effective,
                status
            ));
        }
        out.push_str(&format!("Total effective: {:.0}mm\n", res.total_effective));
        if res.damage > 0.0 {
            out.push_str(&format!("Damage: {:.0}\n", res.damage));
        }
        Ok(out)
    }

    /// render_heatmap：启动独立查看器服务器（随机端口）→ 带参热力图 URL → Chrome 无头截图。
    /// WSL 环境自动探测 Windows 侧 Chrome 并转换输出路径。
    fn execute_render_heatmap(&self, args: &Value) -> Result<String> {
        let target_ref = args["target"].as_str().unwrap_or("").trim();
        if target_ref.is_empty() {
            return Ok(
                "Please provide a target tank name or ID (e.g. target: 'E 100').".to_string(),
            );
        }
        let shooter_ref = args["shooter"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let resolver = self.load_resolver()?;
        let target = match self.resolve_tank_or_message(&resolver, target_ref, "target name", "a target tank", false) {
            Ok(id) => id,
            Err(msg) => return Ok(msg),
        };
        let shooter = match shooter_ref {
            Some(sref) => match self.resolve_tank_or_message(&resolver, sref, "shooter name", "a shooter tank", false) {
                Ok(id) => Some(id),
                Err(msg) => return Ok(msg),
            },
            None => None,
        };
        let shooter_id = shooter.unwrap_or(target);
        let target_name = resolver
            .resolve(target)
            .unwrap_or_else(|| format!("tank_{}", target));
        let shooter_name = resolver
            .resolve(shooter_id)
            .unwrap_or_else(|| format!("tank_{}", shooter_id));

        // 弹种索引（查看器弹种选择器 = 射手**顶级炮塔 × 顶级主炮**的弹种列表，按正式名过滤）。
        // 必须与 `/api/shells/{tank_id}`（shells_handler）取同一档炮，否则与同屏渲染的
        // 顶级炮塔装甲、以及列表/详情的穿深对不上。
        let shell_filter = args["shell"].as_str().map(|s| s.trim().to_string());
        let shooter_tank = blitzkit::tank_full(shooter_id);
        let shell_idx = shell_filter.as_ref().and_then(|f| {
            shooter_tank.as_ref().and_then(|t| {
                t.turrets
                    .last()
                    .and_then(|tu| tu.guns.last())
                    .and_then(|g| {
                        g.shells.iter().position(|s| {
                            Self::shell_label(&s.shell_type).eq_ignore_ascii_case(f)
                                || s.shell_type.eq_ignore_ascii_case(f)
                        })
                    })
            })
        });
        if let Some(f) = &shell_filter {
            if shell_idx.is_none() {
                let avail = shooter_tank
                    .as_ref()
                    .and_then(|t| {
                        t.turrets.last().and_then(|tu| tu.guns.last()).map(|g| {
                            g.shells
                                .iter()
                                .map(|s| Self::shell_label(&s.shell_type))
                                .collect::<Vec<_>>()
                                .join("/")
                        })
                    })
                    .unwrap_or_default();
                return Ok(format!(
                    "Shell '{}' not found for {}. Available: {}",
                    f, shooter_name, avail
                ));
            }
        }

        // 视角预设由前端按炮线高度解析（水平/卖头/俯视），工具只传语义名称；azimuth_deg 可覆盖方位角
        let view_raw = args["view"].as_str().unwrap_or("front").trim().to_string();
        let view: String = {
            let safe: String = view_raw
                .chars()
                .filter(|c| c.is_ascii_lowercase() || *c == '_')
                .collect();
            if safe.is_empty() {
                "front".into()
            } else {
                safe
            }
        };
        let yaw = args["yaw_deg"].as_f64().unwrap_or(0.0);
        let pitch = args["pitch_deg"].as_f64().unwrap_or(0.0);
        let width = args["width"].as_u64().unwrap_or(1280).clamp(320, 3840) as u32;
        let height = args["height"].as_u64().unwrap_or(800).clamp(240, 2160) as u32;

        let Some(chrome) = find_chrome() else {
            return Ok("Chrome/Chromium not found (required for headless screenshots). Install Google Chrome, or set the CHROME_PATH environment variable to the browser executable.".to_string());
        };

        std::fs::create_dir_all(crate::data::data_path("cache/screenshots"))?;
        let shell_txt = shell_filter.as_deref().unwrap_or("default").to_uppercase();
        let fname = crate::data::data_path("cache/screenshots")
            .join(format!(
                "heatmap_{}_vs_{}_{}_{}.png",
                sanitize_name(&target_name),
                sanitize_name(&shooter_name),
                sanitize_name(&shell_txt),
                view
            ))
            .to_string_lossy()
            .to_string();

        // 独立线程 + 独立 runtime：启动服务器（阻塞）→ Chrome 无头截图 → 线程结束即关闭服务器
        let resolver2 = resolver.clone();
        let chrome2 = chrome.clone();
        let fname2 = fname.clone();
        let url = {
            let mut u = format!(
                "http://127.0.0.1:PORT/?headless=1&heatmap=1&clean=1&shell={}&yaw={}&pitch={}&view={}",
                shell_idx.unwrap_or(0), yaw, pitch, view
            );
            if let Some(v) = args["azimuth_deg"].as_f64() {
                u += &format!("&az={}", v as i64);
            }
            u
        };
        let view2 = view.clone();
        let handle = std::thread::spawn(move || -> Result<String> {
            let port = tool_runtime().block_on(crate::wargaming::viewer::start_viewer_server(
                resolver2, target, shooter_id,
            ))?;
            let url = url.replace("PORT", &port.to_string());
            // WSL 调 Windows 侧浏览器：--screenshot 输出路径转 Windows 形式（/mnt/d/x → D:\x）
            let is_win_browser = chrome2.contains("/mnt/");
            // 相对路径必须先转绝对（Windows Edge 无法解析 WSL 相对路径——否则不写文件）
            let fname_abs = std::env::current_dir()
                .map(|d| d.join(&fname2).to_string_lossy().to_string())
                .unwrap_or_else(|_| fname2.clone());
            let shot_arg = if is_win_browser {
                format!(
                    "--screenshot={}",
                    wsl_to_windows_path(&fname_abs).unwrap_or_else(|| fname_abs.clone())
                )
            } else {
                format!("--screenshot={}", fname_abs)
            };
            let out = std::process::Command::new(&chrome2)
                .args([
                    "--headless=new",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                    "--hide-scrollbars",
                    &format!("--window-size={},{}", width, height),
                    "--virtual-time-budget=20000",
                    &shot_arg,
                    &url,
                ])
                .output()
                .map_err(|e| anyhow::anyhow!("failed to launch browser '{}': {}", chrome2, e))?;
            // 完成信号 = 浏览器 stderr 的 "N bytes written to file ..."（WSL 对 Windows 侧
            // 写入文件的 stat 存在 drvfs 元数据缓存延迟，size/存在性都不可靠，不能用作判据）
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() {
                return Err(anyhow::anyhow!(
                    "browser screenshot failed (status {:?}): {}",
                    out.status,
                    stderr.chars().take(400).collect::<String>()
                ));
            }
            let written = stderr
                .lines()
                .rev()
                .find(|l| l.contains("bytes written to file"))
                .map(|l| l.trim().trim_start_matches('[').to_string())
                .unwrap_or_default();
            if written.is_empty() {
                return Err(anyhow::anyhow!(
                    "browser did not report a screenshot (status {:?}): {}",
                    out.status,
                    stderr.chars().take(400).collect::<String>()
                ));
            }
            let url_name = fname2.rsplit('/').next().unwrap_or(&fname2);
            Ok(format!(
                "Heatmap screenshot saved: {} ({}x{}, browser: {})\n[{}]\nImage URL: /screenshots/{}",
                fname2, width, height, chrome2, written, url_name
            ))
        });

        let detail = handle
            .join()
            .map_err(|_| anyhow::anyhow!("screenshot thread panicked"))??;
        Ok(format!(
            "Heatmap screenshot saved: {}\nTarget: {} · Shooter: {} · View: {} · Turret yaw {}° · Gun pitch {}°\nThe image colors armor faces by penetration chance (green=likely penetration, red=blocked, magenta=ricochet, orange=HE splash). The temporary viewer server has been stopped; use view_tank for an interactive session.",
            detail, target_name, shooter_name, view2, yaw, pitch
        ))
    }

    /// replay_shot：回放解析（extract_shot_replays_auto_with_limits）→ 带复现数据的查看器 → 射手 POV 无头截图。
    fn execute_replay_shot(&self, args: &Value) -> Result<String> {
        let file = args["replay_file"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        if file.is_empty() {
            return Ok("Please provide replay_file (path to the .wotbreplay file).".to_string());
        }
        let shot_no = args["shot_no"].as_i64().unwrap_or(1).max(1) as usize;
        let Some(chrome) = find_chrome() else {
            return Ok("Chrome/Chromium not found (required for headless screenshots). Install Google Chrome or set CHROME_PATH.".to_string());
        };

        let resolver = self.load_resolver()?;
        let fname2 = crate::data::data_path("cache/screenshots")
            .join(format!("replay_shot_{:02}.png", shot_no))
            .to_string_lossy()
            .to_string();

        let handle = std::thread::spawn(move || -> Result<String> {
            let (port, shell_slot, target_cfg, shooter_cfg) = tool_runtime().block_on(
                crate::wargaming::viewer::start_viewer_server_for_replay(
                    std::path::Path::new(&file),
                    resolver,
                    shot_no,
                ),
            )?;
            let is_win = chrome.contains("/mnt/");
            let fname_abs = std::env::current_dir()
                .map(|d| d.join(&fname2).to_string_lossy().to_string())
                .unwrap_or_else(|_| fname2.clone());
            let shot_arg = if is_win {
                format!(
                    "--screenshot={}",
                    wsl_to_windows_path(&fname_abs).unwrap_or_else(|| fname_abs.clone())
                )
            } else {
                format!("--screenshot={}", fname_abs)
            };
            // 实际搭载配置（comp blob/弹种/血量证据链）：目标模型按其选炮塔/主炮变体；
            // 射手配置（scfg）= 射手弹表域，shell 下标与 3D 端下拉同域对齐；
            // 弹种链解析失败 → 无 &shell= 参数（3D 端按默认弹渲染，不猜槽位）
            let cfg_arg = target_cfg
                .map(|c| format!("&config={}", c))
                .unwrap_or_default();
            let scfg_arg = shooter_cfg
                .map(|c| format!("&scfg={}", c))
                .unwrap_or_default();
            let shell_arg = shell_slot
                .map(|s| format!("&shell={}", s))
                .unwrap_or_default();
            let url = format!(
                "http://127.0.0.1:{}/?headless=1&heatmap=1&clean=1&shot={}{}{}{}&dist=9",
                port, shot_no, shell_arg, cfg_arg, scfg_arg
            );
            let out = std::process::Command::new(&chrome)
                .args([
                    "--headless=new",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                    "--hide-scrollbars",
                    "--window-size=1280,800",
                    "--virtual-time-budget=25000",
                    &shot_arg,
                    &url,
                ])
                .output()
                .map_err(|e| anyhow::anyhow!("failed to launch browser: {}", e))?;
            if !out.status.success() {
                return Err(anyhow::anyhow!(
                    "browser failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                        .chars()
                        .take(300)
                        .collect::<String>()
                ));
            }
            let written = String::from_utf8_lossy(&out.stderr)
                .lines()
                .rev()
                .find(|l| l.contains("bytes written to file"))
                .map(|l| l.trim().trim_start_matches('[').to_string())
                .unwrap_or_default();
            Ok(format!("{} — [{}]", fname2, written))
        });
        let result = handle
            .join()
            .map_err(|_| anyhow::anyhow!("screenshot thread panicked"))??;
        Ok(format!(
            "Shot #{} replay view saved (世界模式双车视角): {}\n相机置于双车侧后 3/4 视角，弹着点按服务器部件约束；◎渲染位/双 tick 下拉可对照渲染滞后。",
            shot_no, result
        ))
    }

    /// resolve_tank + 应答折叠：命中即 id；歧义/未命中返回给模型的选择/纠正话术（逐字
    /// 保持各调用点原文案——label/noun 按 target/shooter 角色变化，hint 补未命中的示例提示）。
    fn resolve_tank_or_message(
        &self,
        resolver: &TankResolver,
        tank_ref: &str,
        label: &str,
        noun: &str,
        hint: bool,
    ) -> Result<u32, String> {
        match self.resolve_tank(resolver, tank_ref) {
            ResolveTank::One(id) => Ok(id),
            ResolveTank::Ambiguous(cands) => Err(format!(
                "The {} '{}' matches multiple tanks. Please pick one:\n{}",
                label,
                tank_ref,
                format_candidates(&cands)
            )),
            ResolveTank::None => Err(format!(
                "Could not find {} matching '{}'.{}",
                noun,
                tank_ref,
                if hint {
                    " Ask for a specific tank name (e.g. IS-7, E 100) or numeric ID."
                } else {
                    ""
                }
            )),
        }
    }

    /// 模糊解析坦克：One 唯一命中 / Ambiguous 多候选 / None 未命中。
    fn resolve_tank(&self, resolver: &TankResolver, tank_ref: &str) -> ResolveTank {
        // 数字 ID 直接用（即使 tank_cache 里没有该 ID，也透传给查看器）
        if let Ok(id) = tank_ref.parse::<u32>() {
            return ResolveTank::One(id);
        }

        // 查询词只归一化一次；候选名在 TankResolver 加载时已预归一（name_index），零分配匹配。
        // 归一规则（'-'/'·'/'.'/'_' → 空格 + 小写；再剥空白）见 tank_resolver::norm_name/strip_ws。
        let needle = norm_name(tank_ref);
        let needle_ns = strip_ws(&needle);
        if needle.is_empty() {
            return ResolveTank::None;
        }
        let mut exacts: Vec<(u32, &TankInfo)> = Vec::new();
        let mut subs: Vec<(u32, &TankInfo, usize)> = Vec::new(); // (id, info, score)
        for e in resolver.name_index() {
            let exact = e.norm == needle || (!needle_ns.is_empty() && e.norm_ns == needle_ns);
            if exact {
                if let Some(info) = resolver.resolve_info(e.id) {
                    exacts.push((e.id, info));
                }
            } else if let Some(pos) = e.norm.find(&needle).or_else(|| {
                if needle_ns.is_empty() {
                    None
                } else {
                    e.norm_ns.find(&needle_ns)
                }
            }) {
                // 命中位置越靠前越好；同名不同短名长度带来惩罚
                let score = pos + e.norm.len().saturating_sub(needle.len());
                if let Some(info) = resolver.resolve_info(e.id) {
                    subs.push((e.id, info, score));
                }
            }
        }

        let base: Vec<(u32, &TankInfo)> = if !exacts.is_empty() {
            exacts
        } else {
            subs.sort_by_key(|(_, _, s)| *s);
            subs.into_iter().map(|(id, info, _)| (id, info)).collect()
        };

        match base.as_slice() {
            [] => ResolveTank::None,
            [single] => ResolveTank::One(single.0),
            many => {
                let candidates: Vec<TankCandidate> = many
                    .iter()
                    .map(|(id, info)| TankCandidate {
                        id: *id,
                        name: info.name.clone(),
                        tier: info.tier as u32,
                        nation: info.nation.clone(),
                        tank_type: info.tank_type.clone(),
                    })
                    .collect();
                ResolveTank::Ambiguous(candidates)
            }
        }
    }
}

/// 一次性坦克解析结果。
#[derive(Debug)]
enum ResolveTank {
    One(u32),
    Ambiguous(Vec<TankCandidate>),
    None,
}

/// 供消歧列表展示的坦克候选。
#[derive(Debug)]
struct TankCandidate {
    id: u32,
    name: String,
    tier: u32,
    nation: String,
    tank_type: String,
}

/// 把候选列表格式化为带编号的文本（LLM 据此让用户选择）。
fn format_candidates(cands: &[TankCandidate]) -> String {
    let mut out = String::new();
    for (i, c) in cands.iter().enumerate() {
        out.push_str(&format!(
            "  {}. {} (id={}) — Tier {}, {}, {}\n",
            i + 1,
            c.name,
            c.id,
            c.tier,
            c.nation,
            c.tank_type
        ));
    }
    out
}

/// 工具线程共享的 Tokio Runtime（block_on 可多线程并发；复用单例，避免每次调用重建线程池+reactor）。
fn tool_runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tool runtime"))
}

/// 探测 Chrome/Chromium 可执行文件：CHROME_PATH 环境变量 → 常见命令 → Windows/WSL 安装路径。
/// 结果进程内缓存（环境不变，首次探测即最终结果）。
fn find_chrome() -> Option<String> {
    static CACHE: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(find_chrome_uncached).clone()
}

fn find_chrome_uncached() -> Option<String> {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        let p = p.trim().to_string();
        if !p.is_empty() && Path::new(&p).exists() {
            return Some(p);
        }
    }
    for c in [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
        "msedge",
    ] {
        if let Ok(out) = std::process::Command::new(c).arg("--version").output() {
            if out.status.success() {
                return Some(c.to_string());
            }
        }
    }
    for p in [
        "/mnt/c/Program Files/Google/Chrome/Application/chrome.exe",
        "/mnt/c/Program Files (x86)/Google/Chrome/Application/chrome.exe",
        "/mnt/c/Program Files/Microsoft/Edge/Application/msedge.exe",
        "/mnt/c/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
    ] {
        if Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    // Windows 原生路径（非 WSL）
    for p in [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Core\msedge.exe",
    ] {
        if Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    None
}

/// WSL 路径 → Windows 路径（/mnt/d/Class/x → D:\Class\x）；非 /mnt/ 路径返回 None。
fn wsl_to_windows_path(p: &str) -> Option<String> {
    let rest = p.strip_prefix("/mnt/")?;
    let (drive, path) = rest.split_once('/')?;

    if drive.len() != 1 || path.is_empty() {
        return None;
    }
    Some(format!(
        "{}:\\{}",
        drive.to_uppercase(),
        path.replace('/', "\\")
    ))
}

/// 文件名清理：非字母数字字符替换为下划线。
fn sanitize_name(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    s.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wargaming::tank_resolver::TankInfo;

    fn mk_resolver(tanks: &[(u32, &str, u8, &str, &str)]) -> TankResolver {
        let mut r = TankResolver::new();
        for (id, name, tier, nation, ttype) in tanks {
            r.add(
                *id,
                TankInfo {
                    name: name.to_string(),
                    tier: *tier,
                    tank_type: ttype.to_string(),
                    nation: nation.to_string(),
                    is_premium: false,
                    is_collector: false,
                    armor: None,
                    shells: Vec::new(),
                    hp: None,
                    hp_top: None,
                    speed_forward: None,
                    speed_reverse: None,
                    hull_traverse: None,
                    view_range: None,
                    turret_traverse_speed: None,
                    gun_depression: None,
                    gun_elevation: None,
                    turret_traverse_left: None,
                    turret_traverse_right: None,
                },
            );
        }
        r
    }

    fn tools() -> AgentTools {
        AgentTools::new("app", "asia", "", None)
    }

    fn resolve_ok(res: ResolveTank) -> u32 {
        match res {
            ResolveTank::One(id) => id,
            _ => panic!("expected One, got {:?}", res),
        }
    }

    #[test]
    fn exact_match_beats_substring() {
        let r = mk_resolver(&[
            (9489, "E 100", 10, "germany", "heavyTank"),
            (12049, "Jg.Pz. E 100", 10, "germany", "AT-SPG"),
        ]);
        let res = tools().resolve_tank(&r, "e 100");
        assert_eq!(resolve_ok(res), 9489);
    }

    #[test]
    fn numeric_id_passthrough() {
        let r = mk_resolver(&[(9489, "E 100", 10, "germany", "heavyTank")]);
        let res = tools().resolve_tank(&r, "7169");
        assert_eq!(resolve_ok(res), 7169);
    }

    #[test]
    fn ambiguous_returns_candidates() {
        let r = mk_resolver(&[
            (529, "Tiger I", 7, "germany", "heavyTank"),
            (5137, "Tiger II", 8, "germany", "heavyTank"),
            (10769, "Tiger (P)", 7, "germany", "heavyTank"),
        ]);
        let res = tools().resolve_tank(&r, "Tiger");
        match res {
            ResolveTank::Ambiguous(cands) => {
                assert_eq!(cands.len(), 3);
                let txt = format_candidates(&cands);
                assert!(txt.contains("Tiger I"));
                assert!(txt.contains("Tiger II"));
                assert!(txt.contains("Tiger (P)"));
            }
            _ => panic!("expected Ambiguous"),
        }
    }

    #[test]
    fn none_when_not_found() {
        let r = mk_resolver(&[(1, "T-34", 5, "ussr", "mediumTank")]);
        assert!(matches!(
            tools().resolve_tank(&r, "Maus"),
            ResolveTank::None
        ));
    }

    // 用真实 tank_cache.json 验证模糊查询；文件缺失则跳过。
    #[test]
    fn simulate_penetration_smoke() {
        let tools = tools();
        // IS-7 顶配炮（AP 250mm）打 E 100 车体正面（200mm @0°）→ PENETRATION
        let mut args = json!({
            "target": "E 100", "shooter": "IS-7",
            "aim": "hull_front", "angle_deg": 0, "distance_m": 100
        });
        let out = tools.execute_simulate_penetration(&args).unwrap();
        assert!(out.contains("Result:"), "missing result header: {}", out);
        assert!(out.contains("Layers:"), "missing layer breakdown: {}", out);
        assert!(
            out.contains("PENETRATION"),
            "250mm AP vs 200mm@0° should penetrate: {}",
            out
        );
        // 显式厚板（1000mm 车体）→ BLOCKED
        args["hits"] = json!([{"section": "hull", "thickness_mm": 1000}]);
        let out2 = tools.execute_simulate_penetration(&args).unwrap();
        assert!(
            out2.contains("BLOCKED"),
            "expected BLOCKED vs 1000mm: {}",
            out2
        );
        // HE 弹路径（溅射公式）不 panic
        args["shell"] = json!("HE");
        args["hits"] = json!([{"section": "hull", "thickness_mm": 80}]);
        let out3 = tools.execute_simulate_penetration(&args).unwrap();
        assert!(out3.contains("Result:"), "{}", out3);
        // get_tank_armor 冒烟：plates/shells/spaced 字段齐全
        let out4 = tools
            .execute_get_tank_armor(&json!({"target": "E 100"}))
            .unwrap();
        assert!(
            out4.contains("plates") && out4.contains("shells") && out4.contains("spaced"),
            "{}",
            out4
        );
    }

    #[test]
    fn replay_shot_smoke() {
        let tools = tools();
        let f =
            "data/replay_samples/20260902_2104__Anonyme_A116_XM551_Exp_3355505117896350.wotbreplay";
        if !std::path::Path::new(f).exists() {
            eprintln!("replay sample missing, skip");
            return;
        }
        let args = json!({"replay_file": f, "shot_no": 1});
        match tools.execute_replay_shot(&args) {
            Ok(out) => {
                eprintln!("replay_shot: {}", out);
                assert!(out.contains("replay view saved"), "{}", out);
            }
            Err(e) => {
                // 无 Chrome 环境时允许跳过（服务器启动/浏览器缺失），但数据抽取错误仍算失败
                let msg = format!("{}", e);
                if msg.contains("Chrome") || msg.contains("browser") {
                    eprintln!("skip (no browser): {}", msg);
                } else {
                    panic!("{}", msg);
                }
            }
        }
    }

    #[test]
    fn render_heatmap_smoke() {
        let tools = tools();
        // Kranvagn(4481, GLB 已缓存) vs IS-7 正面热力图
        let args = json!({
            "target": "Kranvagn", "shooter": "IS-7",
            "view": "front", "width": 640, "height": 400
        });
        let out = tools.execute_render_heatmap(&args).unwrap();
        eprintln!("render_heatmap: {}", out);
        assert!(out.contains("Heatmap screenshot saved:"), "{}", out);
        let _path = out
            .split("saved: ")
            .nth(1)
            .and_then(|s| s.split('\n').next())
            .unwrap()
            .trim()
            .to_string();
        // 完成判据 = 浏览器 stderr "bytes written to file"（WSL stat 缓存不可靠）
        assert!(
            out.contains("bytes written to file"),
            "no written confirmation: {}",
            out
        );
    }

    #[test]
    fn real_cache_fuzzy_search() {
        let Ok(r) = TankResolver::load_from_json_file(&crate::data::data_path("tank_cache.json"))
        else {
            eprintln!("tank_cache.json missing, skipping real_cache test");
            return;
        };
        match tools().resolve_tank(&r, "IS-7") {
            ResolveTank::One(id) => assert_eq!(id, 7169),
            other => panic!("IS-7 should be unique, got {:?}", other),
        }
        match tools().resolve_tank(&r, "E 100") {
            ResolveTank::One(id) => assert_eq!(id, 9489),
            other => panic!("E 100 should be exact match, got {:?}", other),
        }
        assert!(matches!(
            tools().resolve_tank(&r, "Tiger"),
            ResolveTank::Ambiguous(_)
        ));
        match tools().resolve_tank(&r, "Maus") {
            ResolveTank::One(id) => assert_eq!(id, 6929),
            other => panic!("Maus should be unique, got {:?}", other),
        }
    }

    #[test]
    fn view_tank_tool_definition_uses_target_shooter() {
        let defs = AgentTools::definitions();
        let vt = defs
            .iter()
            .find(|d| d.function.name == "view_tank")
            .expect("view_tank tool must exist");
        let props = vt
            .function
            .parameters
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("parameters.properties");
        assert!(props.contains_key("target"), "must have target param");
        assert!(props.contains_key("shooter"), "must have shooter param");
        assert!(
            !props.contains_key("tank"),
            "old 'tank' param should be gone"
        );
        let required = vt
            .function
            .parameters
            .get("required")
            .and_then(|r| r.as_array())
            .expect("required array");
        assert!(
            required.iter().any(|r| r == "target"),
            "target must be required"
        );
    }
}
