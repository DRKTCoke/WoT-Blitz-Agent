use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// =====================================================================
//  坦克数据解析器：从本地数据文件（BlitzKit pb 解析结果 + 游戏提取数据）
//  构建 ID → 完整信息映射，无需联网、无需 WG API。
// =====================================================================

/// 一辆坦克的完整信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TankInfo {
    pub name: String,
    pub tier: u8,
    #[serde(rename = "type")]
    pub tank_type: String,
    pub nation: String,
    pub is_premium: bool,
    /// 收藏车（tanks.pb field13 == 2）。与 `is_premium`（== 1）互斥——同一枚举字段的两档。
    #[serde(default)]
    pub is_collector: bool,
    #[serde(default)]
    pub armor: Option<ArmorData>,
    #[serde(default)]
    pub shells: Vec<ShellData>,
    #[serde(default)]
    pub hp: Option<u32>,
    #[serde(default)]
    pub speed_forward: Option<u32>,
    #[serde(default)]
    pub speed_reverse: Option<u32>,
    /// 车体旋转速度（deg/s）
    #[serde(default)]
    pub hull_traverse: Option<f32>,
    /// 视野（m）
    #[serde(default)]
    pub view_range: Option<f32>,
    /// 炮塔旋转速度（deg/s）
    #[serde(default)]
    pub turret_traverse_speed: Option<f32>,
    #[serde(default)]
    pub gun_depression: Option<f32>,
    #[serde(default)]
    pub gun_elevation: Option<f32>,
    #[serde(default)]
    pub turret_traverse_left: Option<f32>,
    #[serde(default)]
    pub turret_traverse_right: Option<f32>,
}

/// 装甲摘要（前/侧/后，单位 mm），用于信息面板显示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmorData {
    pub turret_front: u32,
    pub turret_sides: u32,
    pub turret_rear: u32,
    pub hull_front: u32,
    pub hull_sides: u32,
    pub hull_rear: u32,
}

/// 一种弹（弹药）的数据：类型、穿深、血量伤害、模块伤害，以及等效厚度判定所需的
/// 弹径 / 转正角 / 跳弹角。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellData {
    pub shell_type: String,
    /// tanks.pb field9 的弹种枚举（权威，0=AP/1=APCR/2=HEAT/3=HE）；透出给消费方，
    /// 使其可走权威路径而不必依赖 `shell_type` 的 icon 词表。
    #[serde(default)]
    pub shell_type_id: Option<u32>,
    pub penetration: u32,
    /// HP（血量）伤害——穿透主装甲盒时对敌人血量造成的伤害
    pub damage: u32,
    /// 模块伤害——仅命中履带/炮管等外部模块时的伤害
    pub module_damage: u32,
    /// HE 爆炸半径（m）；缓存缺失该字段时为 0。
    #[serde(default)]
    pub explosion_radius: f64,
    /// 弹径（mm）——三倍口径 overmatch 与两倍口径转正放大两项判定的输入；
    /// 逐弹自带（同车不同炮口径不同，不能拿车级口径代替）。
    #[serde(default)]
    pub caliber: f64,
    /// 转正角（度）——穿深判定的等效厚度修正（`effective = t / cos(angle - norm)`）。
    /// 缺失（旧缓存）时为 0 = 转正不生效，属降级值而非真值。
    #[serde(default)]
    pub normalization: f64,
    /// 跳弹临界角（度）；HEAT 数据为 85°、HE 为 0（爆炸弹判定侧另有 90° 覆盖）。
    #[serde(default)]
    pub ricochet: f64,
}

/// 归一化名称索引项：坦克名预归一结果，避免每次模糊查询对全部坦克重新归一。
#[derive(Debug, Clone)]
pub(crate) struct NameIndexEntry {
    /// 归一名（'-'/'·'/'.'/'_' → 空格 + 小写）
    pub(crate) norm: String,
    /// 归一名去空白形态（"e100"↔"E 100"）
    pub(crate) norm_ns: String,
    pub(crate) id: u32,
}

#[derive(Debug, Clone)]
pub struct TankResolver {
    cache: HashMap<u32, TankInfo>,
    /// 与 cache 同步维护的归一化名称索引（模糊匹配用）
    name_index: Vec<NameIndexEntry>,
}

/// 名称归一：'-'/'·'/'.'/'_' 全部视为空格并转小写，使 "E 100" 与 "E-100"、"IS-7" 与 "IS 7" 等可互相命中。
pub(crate) fn norm_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '-' || c == '·' || c == '.' || c == '_' {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .to_lowercase()
}

/// 去空格形态：归一后再剥掉全部空白——"hori"↔"Ho-Ri"、"e100"↔"E 100"，
/// 否则此类名无法命中。
pub(crate) fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 核心库的坦克名最小接口：核心库不绑定 IO，本实现读 tank_cache.json。
impl wotb_replay_core::replay::TankNames for TankResolver {
    fn resolve(&self, tank_id: u32) -> Option<String> {
        TankResolver::resolve(self, tank_id)
    }
}

impl TankResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            name_index: Vec::new(),
        }
    }

    pub fn resolve(&self, tank_id: u32) -> Option<String> {
        self.cache.get(&tank_id).map(|info| info.name.clone())
    }

    pub fn resolve_info(&self, tank_id: u32) -> Option<&TankInfo> {
        self.cache.get(&tank_id)
    }

    /// 昵称 → 炮管俯仰限制锚定表：prop2 frac 解码用（combat::decode_prop2_gun_pitch，
    /// 扇区化——俯仰范围随炮塔朝向 front/back 分段）。
    /// 数据源（优先级）：models.pb 按**实际搭载**（`comps` = ARENA_INFO comp blob 的
    /// 炮塔/主炮局部 id，`module_id>>8` 对号；多炮车非顶级主炮俯仰范围不同）>
    /// models.pb 顶级配置（最后炮塔×最后炮，comp 缺失或对号失败时回退）。
    /// **本表是基础极限**：实体实际携带的俯仰配件（122 改进型垂直稳定器，+4°仰/+3°俯）
    /// 由 replay-core 按同 eid 的 Type5 `0B` 配件串在解码时叠加
    /// （`combat::effective_pitch_limits`），调用方不得在此预加，否则双重计算。
    /// models.pb 无数据的玩家不入选（其俯仰走提取链回退路径并打质量标记）。注意匿名玩家
    /// 共用显示名 "Anonyme"，同场多个匿名玩家会互相覆盖（按昵称连接的固有歧义）。
    pub fn pitch_limits_from_battle_results(
        &self,
        br: &wotbreplay_parser::models::battle_results::BattleResults,
        comps: &HashMap<String, crate::replay::playback::CompDescriptor>,
    ) -> HashMap<String, crate::replay::combat::GunPitchRange> {
        let mut m: HashMap<String, crate::replay::combat::GunPitchRange> = HashMap::new();
        for p in &br.players {
            let tank_id = br
                .player_results
                .iter()
                .find(|pr| pr.info.account_id == p.account_id)
                .map(|pr| pr.info.tank_id);
            let Some(tid) = tank_id else { continue };
            // 实际搭载（comp blob，确定性）：**account_id 主键**（条目 field7，P3 探针
            // 定案——匿名/重名昵称免疫），退回昵称键；tank 低 16 位对号后取该炮塔/主炮局部 id
            let comp = comps
                .values()
                .find(|c| c.account_id != 0 && c.account_id == p.account_id as u64)
                .or_else(|| comps.get(p.info.nickname.as_str()))
                .filter(|c| (c.tank_id & 0xFFFF) == (tid & 0xFFFF))
                .map(|c| (c.turret_local, c.gun_local));
            if let Some(r) = Self::models_pitch_limits(tid, comp) {
                m.insert(p.info.nickname.clone(), r);
            }
        }
        m
    }

    /// 单车俯仰锚定（含扇区）：实际搭载（comp blob 局部 id 对号）优先，顶级配置回退；
    /// models.pb 无该车或两条路都取不到 pitch → None。
    fn models_pitch_limits(
        tank_id: u32,
        comp: Option<(u16, u16)>,
    ) -> Option<crate::replay::combat::GunPitchRange> {
        let mi = crate::wargaming::blitzkit::model_info(tank_id)?;
        let pitch_of = |turret_local: u32, gun_local: u32| {
            mi.turrets
                .iter()
                .find(|t| (t.module_id >> 8) == turret_local)
                .and_then(|t| {
                    t.guns
                        .iter()
                        .find(|gm| (gm.gun_module_id >> 8) == gun_local)
                })
                .and_then(|gm| gm.pitch_limits.clone())
        };
        let pl = comp
            .and_then(|(tl, gl)| pitch_of(tl as u32, gl as u32))
            .or_else(|| {
                let top_gun_module =
                    crate::wargaming::blitzkit::tank_full(tank_id).and_then(|tank| {
                        tank.turrets
                            .last()
                            .and_then(|t| t.guns.last())
                            .map(|g| g.module_id)
                    })?;
                mi.turrets
                    .iter()
                    .flat_map(|t| t.guns.iter())
                    .find(|gm| gm.gun_module_id == top_gun_module)
                    .and_then(|gm| gm.pitch_limits.clone())
            })?;
        Some(crate::replay::combat::GunPitchRange {
            dep: pl.max,
            ele: -pl.min,
            front: pl.front.map(|f| crate::replay::combat::SectorLimits {
                min: f.min,
                max: f.max,
                range: f.range,
            }),
            back: pl.back.map(|b| crate::replay::combat::SectorLimits {
                min: b.min,
                max: b.max,
                range: b.range,
            }),
            transition: pl.transition,
        })
    }

    pub fn add(&mut self, tank_id: u32, info: TankInfo) {
        let norm = norm_name(&info.name);
        self.name_index.push(NameIndexEntry {
            norm_ns: strip_ws(&norm),
            norm,
            id: tank_id,
        });
        self.cache.insert(tank_id, info);
    }

    /// 归一化名称索引（模糊匹配用，与 cache 同步维护）。
    pub(crate) fn name_index(&self) -> &[NameIndexEntry] {
        &self.name_index
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// 从 JSON 文件加载坦克缓存（`tank_cache.json`）。
    pub fn load_from_json_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read tank cache: {}", path.display()))?;
        let cache: HashMap<u32, TankInfo> = serde_json::from_str(&content)?;
        // 预计算归一化名称索引（按 cache 迭代序构建，与直接遍历 cache 的顺序一致）
        let name_index = cache
            .iter()
            .map(|(id, info)| {
                let norm = norm_name(&info.name);
                NameIndexEntry {
                    norm_ns: strip_ws(&norm),
                    norm,
                    id: *id,
                }
            })
            .collect();
        Ok(Self { cache, name_index })
    }

    /// 把坦克缓存写为 JSON 文件（`fetch-tanks` 命令用）。
    ///
    /// 按 tank_id 排序输出：cache 是 HashMap，迭代序随进程随机——不排序时同一份
    /// 数据每次重建都会整文件重排（diff 无法评审，资产包每次全量重传）。
    pub fn save_to_json_file(&self, path: &Path) -> Result<()> {
        let sorted: std::collections::BTreeMap<u32, &TankInfo> =
            self.cache.iter().map(|(id, info)| (*id, info)).collect();
        let content = serde_json::to_string_pretty(&sorted)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// 从本地 BlitzKit 数据文件构建完整解析器（无需 WG API）。
    /// 数据源：tanks.pb（唯一数据源，运行时解析）、models.pb（俯仰角）、
    /// game_data/{id}.json（装甲模型）；装甲摘要回退 BlitzKit models.pb。
    pub fn from_blitzkit() -> Result<Self> {
        let mut resolver = Self::new();

        let tanks = crate::wargaming::blitzkit::load_tanks();

        for (id, tank) in tanks.iter() {
            let name = tank.name.clone();
            let nation = tank.nation.clone();
            let tank_type = tank.tank_type.clone();
            let tier = tank.tier as u8;
            // 血量 = 车体 health（TankDefinition.health）+ 炮塔 health（TurretDefinition.health）
            // ——取顶级炮塔（turrets.at(-1)，对齐 BlitzKit 默认配置），百科显示的总血量
            let hp = Some(tank.hp + tank.turrets.last().map(|t| t.health).unwrap_or(0));
            let is_premium = tank.is_premium;
            let is_collector = tank.is_collector;
            let speed_forward = if tank.speed_forward > 0.0 {
                Some(tank.speed_forward as u32)
            } else {
                None
            };
            let speed_reverse = if tank.speed_reverse > 0.0 {
                Some(tank.speed_reverse as u32)
            } else {
                None
            };
            // 车体转向速度（deg/s）：取**顶级履带**的 traverse_speed（与下方 turrets.last()
            // 同口径）。旧实现读 tanks.pb field27 并 ×180/π——field27 实为 camouflage_still
            // （静止迷彩系数 0~1 的分数），被误当弧度换算成"转速"透出给消费方；
            // BlitzKit `tank_definitions.proto:37` + 生成器（取 XML `invisibility.still`）
            // 双重确认，已更正为履带真值（T-34 顶级履带 = 46 deg/s）。
            let hull_traverse = tank.tracks.last().map(|t| t.traverse_speed as f32);

            // 弹种：取**顶级炮塔的顶级主炮**的 shells——与详情页 `configs[]`（按炮逐项展开）
            // 和 models.pb 的 `turrets.last() × guns.last()` 同档。
            // 口径/转正角/跳弹角随弹透出：消费方（装甲查看器）按弹算等效厚度与击穿判定，
            // 缺这三项时转正按 0° 兜底——同一发弹的判定会明显偏保守（曾致"切换射击方后
            // 同一点由可击穿变挡弹"，2026-10-10）。
            let mut shells = Vec::new();
            if let Some(gun) = tank.turrets.last().and_then(|t| t.guns.last()) {
                for s in &gun.shells {
                    shells.push(ShellData {
                        shell_type: s.shell_type.clone(),
                        shell_type_id: s.shell_type_id,
                        penetration: s.penetration.round() as u32,
                        damage: s.damage.round() as u32,
                        module_damage: s.module_damage.round() as u32,
                        explosion_radius: s.explosion_radius,
                        caliber: s.caliber,
                        normalization: s.normalization,
                        ricochet: s.ricochet,
                    });
                }
            }

            // 视野 / 炮塔旋转速度：同样取**顶级炮塔**。
            let top_turret = tank.turrets.last();
            let view_range = top_turret.map(|t| t.view_range as f32);
            let turret_traverse_speed = top_turret.map(|t| t.traverse_speed as f32);

            // 俯仰角：models.pb 顶级配置全局极值
            let (gun_depression, gun_elevation) = Self::models_pitch_limits(*id, None)
                .map(|r| (Some(r.dep), Some(r.ele)))
                .unwrap_or((None, None));

            let armor = extract_armor_summary(*id);

            resolver.add(
                *id,
                TankInfo {
                    name,
                    tier,
                    tank_type,
                    nation,
                    is_premium,
                    is_collector,
                    armor,
                    shells,
                    hp,
                    speed_forward,
                    speed_reverse,
                    hull_traverse,
                    view_range,
                    turret_traverse_speed,
                    gun_depression,
                    gun_elevation,
                    turret_traverse_left: None,
                    turret_traverse_right: None,
                },
            );
        }

        Ok(resolver)
    }
}

impl Default for TankResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// 提取装甲摘要（前/侧/后，mm）：优先用游戏提取的精确装甲模型（game_data/{id}.json，
/// 按 primary 板 ID 定位各板块厚度）；缺失时回退 BlitzKit models.pb（取每组最大厚度近似）。
fn extract_armor_summary(tank_id: u32) -> Option<ArmorData> {
    let game_path = crate::data::data_path(&format!("game_data/{}.json", tank_id));
    if let Ok(content) = std::fs::read_to_string(&game_path) {
        let am = serde_json::from_str::<serde_json::Value>(&content).ok()?;
        let armor_model = am.get("armor_model")?;
        if let Some(data) = armor_from_model(armor_model) {
            return Some(data);
        }
    }

    let mi = crate::wargaming::blitzkit::model_info(tank_id)?;
    let p = |plates: &std::collections::BTreeMap<u32, f32>| -> u32 {
        plates.values().fold(0.0f64, |a, b| a.max(*b as f64)) as u32
    };
    let hull_max = p(&mi.hull_plates);
    let turret_max = mi
        .turrets
        .iter()
        .map(|t| p(&t.turret_plates))
        .max()
        .unwrap_or(0);
    Some(ArmorData {
        turret_front: turret_max,
        turret_sides: turret_max,
        turret_rear: turret_max,
        hull_front: hull_max,
        hull_sides: hull_max,
        hull_rear: hull_max,
    })
}

/// 从游戏提取的 armor_model 里按 primary 板引用定位各板块厚度。
fn armor_from_model(armor_model: &serde_json::Value) -> Option<ArmorData> {
    // 每个板块独立提取：某块板缺失（如 TD 炮塔只有 armor_1、或无 sides/rear 板）时
    // 用同 section 的最大有效厚度兜底，而不是让 `?` 级联失败回退到"取最大值"的粗糙近似。
    let plate_of = |section: &str, slot: &str| -> Option<u32> {
        let section_val = armor_model.get(section)?;
        let plates = section_val.get("plates")?.as_object()?;
        let primary = section_val.get("primary")?;
        let plate_ref = primary.get(slot).and_then(|v| v.as_str());
        let key = plate_ref.and_then(|r| r.rsplit('_').next()).unwrap_or("1");
        let v = plates.get(key).and_then(|v| v.as_f64());
        if let Some(v) = v {
            return Some(v.round() as u32);
        }
        let maxth = plates
            .values()
            .filter_map(|v| v.as_f64())
            .fold(0.0f64, f64::max);
        if maxth > 0.0 {
            Some(maxth.round() as u32)
        } else {
            Some(0)
        }
    };

    Some(ArmorData {
        turret_front: plate_of("turret", "front")?,
        turret_sides: plate_of("turret", "sides")?,
        turret_rear: plate_of("turret", "rear")?,
        hull_front: plate_of("hull", "front")?,
        hull_sides: plate_of("hull", "sides")?,
        hull_rear: plate_of("hull", "rear")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 俯仰锚定按实际搭载（comp blob 对号的炮）取，而非一律顶级配置。
    /// 样本 769：顶级炮塔双炮俯仰不同，后者为顶级主炮。
    /// comp 缺失时回退顶级；comp 对号非顶级炮时锚定随之切换。
    #[test]
    fn pitch_limits_follow_comp_mounted_gun() {
        let tank_id = 769;
        let tank = crate::wargaming::blitzkit::tank_full(tank_id).expect("tank 769 in tanks.pb");
        let top_turret = tank.turrets.last().expect("turret");
        assert!(top_turret.guns.len() >= 2, "样本车顶级炮塔应有 ≥2 门炮");
        let top_gun = top_turret.guns.last().unwrap();
        let sub_gun = &top_turret.guns[top_turret.guns.len() - 2];

        let to_local = |module_id: u32| (module_id >> 8) as u16;
        let top = TankResolver::models_pitch_limits(tank_id, None).expect("顶级配置俯仰");
        let sub = TankResolver::models_pitch_limits(
            tank_id,
            Some((to_local(top_turret.module_id), to_local(sub_gun.module_id))),
        )
        .expect("实际搭载（非顶级炮）俯仰");
        // 两门炮俯仰范围确实不同（否则样本无判别力）
        assert_ne!(top.ele, sub.ele, "样本车两炮仰角应不同");

        // comp 对号顶级炮 → 与无 comp 的顶级回退一致
        let via_comp_top = TankResolver::models_pitch_limits(
            tank_id,
            Some((to_local(top_turret.module_id), to_local(top_gun.module_id))),
        )
        .expect("comp 对号顶级炮");
        assert_eq!(via_comp_top.dep, top.dep);
        assert_eq!(via_comp_top.ele, top.ele);

        // comp 对号失败（局部 id 不属于该车）→ 回退顶级
        let fallback = TankResolver::models_pitch_limits(tank_id, Some((999, 999)));
        assert_eq!(
            fallback.as_ref().map(|r| (r.dep, r.ele)),
            Some((top.dep, top.ele))
        );
    }

    /// 弹种摘要必须带等效厚度判定输入（口径/转正角/跳弹角）：装甲查看器按弹算
    /// 转正与三倍/两倍口径规则，缺项即按 0° 兜底——曾致"切换射击方后同一像素由
    /// 可击穿变挡弹"（2026-10-10 回归锚点）。
    #[test]
    fn shell_summary_keeps_penetration_decision_inputs() {
        let resolver = TankResolver::from_blitzkit().expect("tanks.pb present");
        // 锚点：Strv K（bug 报告车）顶级炮 AP = 105mm / 转正 5° / 跳弹 70°
        let strvk = resolver.resolve_info(12161).expect("Strv K in tanks.pb");
        let ap = strvk
            .shells
            .iter()
            .find(|s| s.shell_type == "ap")
            .expect("Strv K 顶级炮有 AP");
        assert_eq!(ap.caliber, 105.0);
        assert_eq!(ap.normalization, 5.0);
        assert_eq!(ap.ricochet, 70.0);
        // 全量不变量：弹径一切弹种 > 0；动能弹（type 含 ap）转正/跳弹角 > 0
        for id in crate::wargaming::blitzkit::load_tanks().keys() {
            let Some(info) = resolver.resolve_info(*id) else {
                continue;
            };
            for s in &info.shells {
                assert!(s.caliber > 0.0, "tank {id} {} 弹径缺失", s.shell_type);
                if s.shell_type.contains("ap") {
                    assert!(
                        s.normalization > 0.0,
                        "tank {id} {} 转正角缺失",
                        s.shell_type
                    );
                    assert!(s.ricochet > 0.0, "tank {id} {} 跳弹角缺失", s.shell_type);
                }
            }
        }
    }
}
