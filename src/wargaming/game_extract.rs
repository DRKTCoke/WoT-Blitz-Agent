use crate::wargaming::dvpl::{ArmorModel, CollisionData, DvplFile};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

// =====================================================================
//  游戏数据批量提取：从本机 WoTB 安装目录的 DVPL 文件解析每辆坦克的
//  装甲模型与碰撞数据，导出为 game_data/{tank_id}.json（其他设备免装游戏）。
// =====================================================================

/// 常见游戏安装目录（WSL / Windows 各一份，供自动探测）。
pub const DEFAULT_GAME_DIRS: &[&str] = &[
    "/mnt/d/SteamLibrary/steamapps/common/World of Tanks Blitz/Data",
    "D:/SteamLibrary/steamapps/common/World of Tanks Blitz/Data",
    "/mnt/c/Program Files (x86)/Steam/steamapps/common/World of Tanks Blitz/Data",
    "C:/Program Files (x86)/Steam/steamapps/common/World of Tanks Blitz/Data",
];

/// 坦克条目（只取 tank_id/model_name/nation，源自 tanks.pb 唯一数据源）。
#[derive(Debug, Deserialize)]
struct PbTankEntry {
    tank_id: u32,
    /// 游戏模型名（tanks.pb field32，如 "Sherman_Jumbo"）——定位源文件的唯一依据。
    /// BlitzKit 的 field2 是 slug（如 "m4a3e2"），与游戏文件名对不上。
    #[serde(default)]
    model_name: Option<String>,
    /// BlitzKit slug（tanks.pb field2）——只写进产物供人工比对，不参与文件定位。
    #[serde(default)]
    dev_name: String,
    #[serde(default)]
    nation: Option<String>,
    /// 顶级炮塔模块 id（tanks.pb turrets 末位；0 = 无数据）。
    #[serde(default)]
    top_turret_module: u32,
    /// 顶级主炮模块 id（顶级炮塔下 guns 末位；0 = 无数据）。
    #[serde(default)]
    top_gun_module: u32,
}

/// 一辆坦克的提取结果（可移植的游戏数据替代品）。
#[derive(Debug, Serialize, Deserialize)]
pub struct TankGameData {
    pub tank_id: u32,
    pub dev_name: String,
    pub nation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armor_model: Option<ArmorModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision: Option<CollisionData>,
}

/// 读取之前提取好的某辆坦克数据；3D 查看器优先用便携数据，缺失时才回退游戏目录。
pub fn load_game_data(tank_id: u32, dir: &Path) -> Option<TankGameData> {
    let path = dir.join(format!("{}.json", tank_id));
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 从 item_defs XML 提取 `<hullPosition>x y z</hullPosition>`（车体相对底盘位置）。
fn parse_hull_position(xml: &str) -> Option<[f32; 3]> {
    let start = xml.find("<hullPosition>")? + "<hullPosition>".len();
    let end = xml[start..].find("</hullPosition>")? + start;
    let nums: Vec<f32> = xml[start..end]
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    if nums.len() == 3 {
        Some([nums[0], nums[1], nums[2]])
    } else {
        None
    }
}

/// 确定游戏数据目录：优先用显式路径，否则在常见目录里自动探测。
pub fn resolve_game_dir(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(d) = explicit {
        if d.exists() {
            return Ok(d.to_path_buf());
        }
        anyhow::bail!("Game directory not found: {}", d.display());
    }
    for d in DEFAULT_GAME_DIRS {
        let p = Path::new(d);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }
    anyhow::bail!("No game data directory found. Pass --game-dir explicitly.")
}

/// DLC 覆盖层目录：`%LOCALAPPDATA%\wotblitz\packs`。
///
/// 客户端把 DLC 微更新写在这里，**以相同的相对路径覆盖**游戏 `Data/` 下的同名文件
/// （第三方 mod 工具文档明写 "its files override the base ones"）。本机实测：
/// packs 下 45 个 `.dvpl` 中 **5 个与 Data 同名不同内容**——`3d/Tanks/German/Ferdinand.sc2`
/// （10458 vs 7832 B）、同车 `.scg`（922191 vs 791385 B）、`XML/item_defs/vehicles/common/camouflages.xml`、
/// `camouflages.yaml`、`3d/Customization.yaml`；另 **40 个 Data 里根本没有**（全是
/// `G37_Ferdinand_skin` 皮肤资产，含 `Customization/*.sc2/.scg`、`images_pbr/*.dds`、
/// `Gfx/UI/BigTankIcons/germany-G37_Ferdinand_skin.packed.webp`）。
///
/// 非 Windows / 无该目录（如 WSL、纯净检出）时返回 None，行为退回只读 `Data/`。
pub fn packs_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    let p = PathBuf::from(base).join("wotblitz").join("packs");
    p.is_dir().then_some(p)
}

/// 客户端资源路径解析：**优先 DLC 覆盖层 `packs/<rel>`，缺失才回退 `Data/<rel>`**。
///
/// 所有对客户端资源的读取都应经此函数。直接 `data_dir.join(rel)` 会读到 DLC 应用前的
/// 旧版本（tank 侧实测：Ferdinand 的 `.scg` 旧版 791385 B vs 覆盖版 922191 B，差 13 万字节）。
pub fn resolve_client_path(data_dir: &Path, rel: &str) -> PathBuf {
    if let Some(packs) = packs_dir() {
        let cand = packs.join(rel);
        if cand.exists() {
            return cand;
        }
    }
    data_dir.join(rel)
}

/// 归一化用于文件名比较：去掉所有非字母数字并转小写（如 "GB91_Super_Conqueror" → "gb91superconqueror"）。
fn norm_file_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}
/// 在民族目录内解析某坦克的车辆 DVPL 文件。tanks.pb 的 dev_name 与游戏文件名常
/// 不一致（如 "super-conqueror" → "GB91_Super_Conqueror"），匹配优先级：
/// ① 精确/归一化相等；
/// ② 文件名归一化后【词缀锚定包含】dev_name（前缀或后缀，最短优先，通常是基准车；
///    纯内嵌子串拒绝——部分根下 "m6" 会内嵌命中 "A183_XM69_Hacker"（xm69）、
///    "t43" 命中 "R230_Object_432U"（object432u），装上别人的装甲）；
/// ③ dev_name 以文件名归一化为【前缀或后缀】（最长优先）。反向匹配同样必须锚定词缀
///    边界、不允许纯内嵌子串——否则 "xm66f" 会被 "M6"（内嵌子串且更短）抢走匹配。
fn resolve_vehicle_file(
    game_dir: &Path,
    rel: &str,
    nation: &str,
    dev_name: &str,
    ext: &str,
) -> Option<PathBuf> {
    // DLC 覆盖层优先参与匹配（见 `packs_dir` / `resolve_client_path` 的说明）
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(packs) = packs_dir() {
        dirs.push(packs.join(rel).join(nation));
    }
    dirs.push(game_dir.join(rel).join(nation));

    for dir in &dirs {
        let exact = dir.join(format!("{}{}", dev_name, ext));
        if exact.exists() {
            return Some(exact);
        }
    }
    let target = norm_file_name(dev_name);
    if target.is_empty() {
        return None;
    }
    // (dir_rank, rank, tie, path)：dir_rank 让 packs 整体优先于 Data；同目录内保持原语义
    // （rank 越小越优先；tie 在 rank 内部决定次序）
    let mut cands: Vec<(u8, u8, usize, PathBuf)> = Vec::new();
    for (dir_rank, dir) in dirs.iter().enumerate() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.ends_with(ext) {
                continue;
            }
            let base = &name[..name.len() - ext.len()];
            // 排除 tutorial/bot 等衍生变体
            let base_lower = base.to_lowercase();
            if base_lower.contains("tutorial") || base_lower.contains("bot") {
                continue;
            }
            let norm = norm_file_name(base);
            let (rank, tie) = if norm == target {
                (0u8, 0usize)
            } else if let Some(pos) = norm.find(&target) {
                if pos == 0 || pos + target.len() == norm.len() {
                    // ② 正向：词缀锚定（前缀或后缀），最短（最接近 dev_name）优先
                    (1, base.len())
                } else {
                    // 纯内嵌子串：拒绝（见函数 docstring ②）
                    continue;
                }
            } else if target.starts_with(&norm) || target.ends_with(&norm) {
                (2, usize::MAX - norm.len()) // ③ 反向：最长（最具体）优先
            } else {
                continue;
            };
            cands.push((dir_rank as u8, rank, tie, e.path()));
        }
    }
    cands.sort_by_key(|&(dir_rank, rank, tie, _)| (dir_rank, rank, tie));
    cands.into_iter().next().map(|(_, _, _, p)| p)
}

/// 批量提取全部坦克的装甲/碰撞数据到 `game_data/`（`extract-game` 命令）。
pub fn extract_all(
    game_dir: Option<&Path>,
    output_dir: &Path,
    force: bool,
) -> Result<ExtractStats> {
    let game_dir = resolve_game_dir(game_dir)?;
    let tanks: Vec<PbTankEntry> = crate::wargaming::blitzkit::load_tanks()
        .values()
        .map(|t| {
            let top_turret = t.turrets.last();
            PbTankEntry {
                tank_id: t.tank_id,
                model_name: {
                    // 文件定位取 field32 游戏模型名；缺失时回退 field2。
                    let n = if !t.model_name.is_empty() {
                        &t.model_name
                    } else {
                        &t.dev_name
                    };
                    if n.is_empty() {
                        None
                    } else {
                        Some(n.clone())
                    }
                },
                dev_name: t.dev_name.clone(),
                nation: if t.nation.is_empty() {
                    None
                } else {
                    Some(t.nation.clone())
                },
                top_turret_module: top_turret.map(|x| x.module_id).unwrap_or(0),
                top_gun_module: top_turret
                    .and_then(|x| x.guns.last())
                    .map(|g| g.module_id)
                    .unwrap_or(0),
            }
        })
        .collect();

    std::fs::create_dir_all(output_dir)?;

    let mut stats = ExtractStats::default();
    let total = tanks.len();

    for (i, tank) in tanks.iter().enumerate() {
        let (Some(model_name), Some(nation)) = (tank.model_name.as_deref(), tank.nation.as_deref())
        else {
            stats.skipped_no_name += 1;
            continue;
        };

        let out_path = output_dir.join(format!("{}.json", tank.tank_id));
        if out_path.exists() && !force {
            stats.cached += 1;
            continue;
        }

        // model_name 已是游戏模型名（tanks.pb field32），通常与文件名逐字相等；
        // 模糊匹配仅处理大小写/前缀差异（如 "T-34" ↔ "T-34.xml.dvpl" 的扩展名前缀情形）。
        let xml_path = resolve_vehicle_file(
            &game_dir,
            "XML/item_defs/vehicles",
            nation,
            model_name,
            ".xml.dvpl",
        );
        let yaml_path = resolve_vehicle_file(
            &game_dir,
            "3d/Tanks/Parameters",
            nation,
            model_name,
            ".yaml.dvpl",
        );

        if xml_path.is_none() || yaml_path.is_none() {
            stats.missing_files += 1;
            continue;
        }
        let xml_path = xml_path.unwrap();
        let yaml_path = yaml_path.unwrap();

        // 解码 DVPL 并解析成结构化数据（失败则视为 null，不算致命）
        let xml_text = DvplFile::read(&xml_path)
            .ok()
            .map(|d| String::from_utf8_lossy(&d.data).into_owned());
        let armor_model = xml_text
            .as_ref()
            .and_then(|t| ArmorModel::parse_from_xml(t));
        // hullPosition 在 item_defs XML 里（车体相对底盘的权威位置），补进 collision 数据。
        let mut collision = DvplFile::read(&yaml_path)
            .ok()
            .and_then(|d| CollisionData::parse_from_yaml(&String::from_utf8_lossy(&d.data)));
        if let (Some(t), Some(ref mut c)) = (&xml_text, collision.as_mut()) {
            c.hull_position = parse_hull_position(t);
        }
        // 顶级配置的炮塔/炮管碰撞盒：YAML 段名带模型节点号（turret_02/gun_06…），
        // 经 models.pb 的 模块id→节点号 映射挑出本车顶级配置对应的段。
        if let Some(ref mut c) = collision {
            let mi = crate::wargaming::blitzkit::model_info(tank.tank_id);
            // 顶级炮塔：models.pb 内按模块 id 匹配，缺失时取末位（BlitzKit 库存→顶级序）
            let top_turret = mi.as_ref().and_then(|m| {
                m.turrets
                    .iter()
                    .find(|t| t.module_id == tank.top_turret_module)
                    .or_else(|| m.turrets.last())
            });
            if let Some(tur) = top_turret {
                // 节点号对号段**覆盖**直解值：直解（首次字面量 + 400 字符窗口）会被
                // averageThickness 厚度引用行（turret_01: 186.08）带偏，抓到后续段
                // （常为 chassis）的包围盒
                if let Some(n) = c.turret_bboxes.iter().find(|n| n.node == tur.model_node) {
                    c.turret_bbox = Some(n.bbox.clone());
                }
                // 顶级主炮：同炮塔下按模块 id 匹配，缺失取末位
                let top_gun = tur
                    .guns
                    .iter()
                    .find(|g| g.gun_module_id == tank.top_gun_module)
                    .or_else(|| tur.guns.last());
                if let Some(g) = top_gun {
                    if let Some(n) = c.gun_bboxes.iter().find(|n| n.node == g.model_node) {
                        c.gun_bbox = Some(n.bbox.clone());
                    }
                }
            }
            // 兜底：无 models.pb 映射时取首个节点段（单炮塔车即唯一段）
            if c.turret_bbox.is_none() {
                c.turret_bbox = c.turret_bboxes.first().map(|n| n.bbox.clone());
            }
            if c.gun_bbox.is_none() {
                c.gun_bbox = c.gun_bboxes.first().map(|n| n.bbox.clone());
            }
        }

        if armor_model.is_none() && collision.is_none() {
            stats.parse_failed += 1;
            continue;
        }

        let entry = TankGameData {
            tank_id: tank.tank_id,
            dev_name: tank.dev_name.clone(),
            nation: nation.to_string(),
            armor_model,
            collision,
        };
        let json = serde_json::to_string(&entry)?;
        if std::fs::write(&out_path, json).is_ok() {
            stats.extracted += 1;
        } else {
            stats.write_failed += 1;
        }

        if (i + 1) % 50 == 0 || i + 1 == total {
            print!(
                "\r  [{}/{}] extracted={} cached={} missing={} failed={}",
                i + 1,
                total,
                stats.extracted,
                stats.cached,
                stats.missing_files,
                stats.parse_failed + stats.write_failed
            );
            let _ = std::io::stdout().flush();
        }
    }
    println!();
    Ok(stats)
}

/// 提取过程的统计计数（用于进度显示与结果汇总）。
#[derive(Default)]
pub struct ExtractStats {
    pub extracted: usize,
    pub cached: usize,
    pub skipped_no_name: usize,
    pub missing_files: usize,
    pub parse_failed: usize,
    pub write_failed: usize,
}

/// 读取本机游戏客户端版本号（游戏目录根的 version.txt.dvpl，内容形如
/// "10.30.0.903 release/11.20.0 WOTB_Win7-"，优先取 release/X.Y.Z 段）。
/// 读不到或解码失败时返回 None。
pub fn game_version(game_dir: &Path) -> Option<String> {
    let dvpl = DvplFile::read(&resolve_client_path(game_dir, "version.txt.dvpl")).ok()?;
    let content = String::from_utf8_lossy(&dvpl.data);
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    content
        .split_whitespace()
        .find(|t| t.starts_with("release/"))
        .map(str::to_string)
        .or_else(|| Some(content.to_string()))
}

/// game_data/ 中已不在当前 tanks.pb 里的孤立 {tank_id}.json（版本更新后坦克被移除的情形，
/// 调用方只报告不删除）。注意：会触发 blitzkit::load_tanks 的进程内缓存加载。
pub fn orphan_game_data_ids(output_dir: &Path) -> Vec<u32> {
    let tanks = crate::wargaming::blitzkit::load_tanks();
    let mut orphans = Vec::new();
    let Ok(entries) = std::fs::read_dir(output_dir) else {
        return orphans;
    };
    for e in entries.flatten() {
        let file_name = e.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let Some(id) = name
            .strip_suffix(".json")
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if !tanks.contains_key(&id) {
            orphans.push(id);
        }
    }
    orphans.sort_unstable();
    orphans
}

/// mtime 兜底启发式（清单缺失时判断游戏是否在数据提取之后更新过）：
/// version.txt.dvpl 的修改时间晚于 game_data/ 里最新的 json → 视为已更新。
/// game_data 为空/不可读时返回 false（增量提取本来就会补齐全部缺失文件）。
pub fn game_dir_newer_than_data(game_dir: &Path, game_data_dir: &Path) -> bool {
    let Ok(vt_mtime) = game_dir
        .join("version.txt.dvpl")
        .metadata()
        .and_then(|m| m.modified())
    else {
        return false;
    };
    let mut newest_data: Option<std::time::SystemTime> = None;
    for e in std::fs::read_dir(game_data_dir)
        .into_iter()
        .flatten()
        .flatten()
    {
        let Ok(m) = e.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if newest_data.map(|n| m > n).unwrap_or(true) {
            newest_data = Some(m);
        }
    }
    newest_data.is_some_and(|n| vt_mtime > n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 部分根（如 t11 预览 Data 根只含少量车辆 XML）下的模糊匹配护栏：
    /// 短 dev_name 不得内嵌命中无关车辆（历史事故：M6 ⊂ XM69、T-43 ⊂ Object432U）。
    #[test]
    fn resolve_vehicle_file_rejects_infix_and_keeps_affix() {
        let root = std::env::temp_dir().join(format!(
            "wotb-agent-resolve-test-{}",
            std::process::id()
        ));
        let rel = "test-only/resolve"; // 不与真实客户端 packs/<rel> 相撞
        let dir = root.join(rel).join("usa");
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str| {
            std::fs::write(dir.join(name), b"x").unwrap();
        };

        // ① 精确命中优先
        write("A183_XM69_Hacker.xml.dvpl");
        write("M6.xml.dvpl");
        assert_eq!(
            resolve_vehicle_file(&root, rel, "usa", "M6", ".xml.dvpl"),
            Some(dir.join("M6.xml.dvpl"))
        );

        // ② 纯内嵌子串拒绝：根里没有 M6 时，"m6" 不得内嵌命中 XM69_Hacker
        std::fs::remove_file(dir.join("M6.xml.dvpl")).unwrap();
        assert_eq!(resolve_vehicle_file(&root, rel, "usa", "M6", ".xml.dvpl"), None);

        // ② 词缀锚定的正向包含仍可用："super-conqueror" ⊂ "GB91_Super_Conqueror"
        write("GB91_Super_Conqueror.xml.dvpl");
        assert_eq!(
            resolve_vehicle_file(&root, rel, "usa", "super-conqueror", ".xml.dvpl"),
            Some(dir.join("GB91_Super_Conqueror.xml.dvpl"))
        );

        // ③ 反向词缀（文件名是 dev_name 的前缀/后缀）仍可用
        write("T25.xml.dvpl");
        assert_eq!(
            resolve_vehicle_file(&root, rel, "usa", "T25_AT", ".xml.dvpl"),
            Some(dir.join("T25.xml.dvpl"))
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}
