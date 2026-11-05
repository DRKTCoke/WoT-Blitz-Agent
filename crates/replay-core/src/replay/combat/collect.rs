//! 射击提取路径的包收集器：launch/endpoint/direct8/warning32/地形命中/
//! 血量降幅区间/tick 时间线/刷新簇/配件（全库原始流导出 dump 也在本模块）。

use super::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

/// 收集每实体的 type=7 同钟多属性刷新簇时钟（AoI 补发/通道切换签名，逆向文档 6.x：
/// 锚点后多属性 <2ms 同钟成簇 = 批量补发快照）；开火/命中事件触发更新方案切换 + 状态补发，簇时钟即切换点。
pub(crate) fn collect_refresh_clusters(packets: &[(u32, f32, &[u8])]) -> HashMap<u32, Vec<f32>> {
    // 每实体 (clock, sub) 排序后滑窗：窗口内出现 ≥2 个不同 sub（<2ms）→ 簇时钟
    let mut seqs: HashMap<u32, Vec<(f32, u32)>> = HashMap::new();
    for (t, clock, p) in packets {
        if *t != 7 || p.len() < 14 {
            continue;
        }
        seqs.entry(u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
            .or_default()
            .push((*clock, u32::from_le_bytes([p[4], p[5], p[6], p[7]])));
    }
    let mut out: HashMap<u32, Vec<f32>> = HashMap::new();
    for (eid, mut seq) in seqs {
        seq.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        let mut cluster_clocks: Vec<f32> = Vec::new();
        let mut i = 0usize;
        while i < seq.len() {
            let mut j = i + 1;
            let mut subs: std::collections::HashSet<u32> = std::collections::HashSet::new();
            subs.insert(seq[i].1);
            while j < seq.len() && (seq[j].0 - seq[i].0).abs() < 0.002 {
                subs.insert(seq[j].1);
                j += 1;
            }
            if subs.len() >= 2 {
                cluster_clocks.push(seq[i].0);
            }
            i = j;
        }
        if cluster_clocks.is_empty() {
            continue;
        }
        // 相邻簇时钟 <0.05s 合并取首（一次补发可能跨几毫秒的多条包）
        let mut merged: Vec<f32> = Vec::new();
        for c in cluster_clocks {
            match merged.last() {
                Some(lc) if c - *lc < 0.05 => {}
                _ => merged.push(c),
            }
        }
        out.insert(eid, merged);
    }
    out
}

/// 该实体在 (t, t+0.35] 内首个补发簇时钟相对 t 的偏移（无则 None）。
pub(crate) fn refresh_cluster_after(
    clusters: &HashMap<u32, Vec<f32>>,
    eid: u32,
    t: f32,
) -> Option<f32> {
    let v = clusters.get(&eid)?;
    let &c = v.iter().find(|c| **c > t && **c <= t + 0.35)?;
    Some(c - t)
}

/// 全链原始流导出（WI 对齐扫描 / 探针用）：per-entity type=10 状态流 + type=7 prop2 流 +
/// 全 shooter 的 method29 发射 / method20 终点 / method8 直击通知。
pub fn dump_replay_streams(packets: &[(u32, f32, &[u8])]) -> serde_json::Value {
    let (mut st10, prop2) = build_entity_indexes(packets);
    for v in st10.values_mut() {
        v.sort_by(|a, b| a.clock.partial_cmp(&b.clock).unwrap());
    }
    let st10_json: serde_json::Map<String, serde_json::Value> = st10
        .into_iter()
        .map(|(eid, v)| {
            (
                format!("{eid:08x}"),
                serde_json::Value::Array(
                    v.into_iter()
                        .map(|s| {
                            serde_json::json!([
                                s.clock, s.pos[0], s.pos[1], s.pos[2], s.yaw, s.pitch, s.roll
                            ])
                        })
                        .collect(),
                ),
            )
        })
        .collect();
    let prop2_json: serde_json::Map<String, serde_json::Value> = prop2
        .into_iter()
        .map(|(eid, v)| {
            (
                format!("{eid:08x}"),
                serde_json::Value::Array(
                    v.into_iter()
                        .map(|(c, r, fr)| serde_json::json!([c, r, fr]))
                        .collect(),
                ),
            )
        })
        .collect();
    let (launches, _) = collect_launches(packets, |_| true);
    let endpoints = collect_endpoints(packets);
    // 文件序事件序列（state-machine 扫描用）：volatile/launch/endpoint/direct8 四类事件按包内出现顺序
    let mut seq: Vec<serde_json::Value> = Vec::new();
    for (t, clock, p) in packets {
        if *t == 10 && p.len() >= 48 {
            let f = |o: usize| f32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
            seq.push(json!([
                "v",
                clock,
                u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
                f(12),
                f(16),
                f(20)
            ]));
            continue;
        }
        if p.len() < 12 {
            continue;
        }
        let m = u32::from_le_bytes([p[4], p[5], p[6], p[7]]);
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        if p.len() < 12 + args_len {
            continue;
        }
        let a = &p[12..12 + args_len];
        let f = |o: usize| f32::from_le_bytes([a[o], a[o + 1], a[o + 2], a[o + 3]]);
        if *t == 8 && m == 0x1d && args_len >= 37 {
            seq.push(json!([
                "l",
                clock,
                u32::from_le_bytes([a[0], a[1], a[2], a[3]]),
                u32::from_le_bytes([a[4], a[5], a[6], a[7]]),
                f(9),
                f(13),
                f(17)
            ]));
        } else if *t == 8 && m == 0x14 && args_len >= 16 {
            seq.push(json!([
                "e",
                clock,
                u32::from_le_bytes([a[0], a[1], a[2], a[3]]),
                f(4),
                f(8),
                f(12)
            ]));
        } else if *t == 8 && m == 0x08 && args_len >= 10 && a[8] == 1 {
            seq.push(json!([
                "h",
                clock,
                u32::from_le_bytes([a[0], a[1], a[2], a[3]]),
                u32::from_le_bytes([a[4], a[5], a[6], a[7]])
            ]));
        } else if *t == 8 && m == 0x00 {
            // method0x00 开火事件：envelope entityId = 射手车辆实体，args=[01]
            seq.push(json!([
                "f",
                clock,
                u32::from_le_bytes([p[0], p[1], p[2], p[3]])
            ]));
        }
    }
    use serde_json::json;
    json!({
        "st10": st10_json,
        "prop2": prop2_json,
        "sequence": seq,
        "launches": launches.iter().map(|l| json!({
            "t": l.t, "shooter": l.shooter, "shot_id": l.shot_id,
            "point": l.point, "vel": l.vel,
        })).collect::<Vec<_>>(),
        "endpoints": endpoints.iter().map(|(sid, (t, p))|
            json!({"shot_id": sid, "t": t, "p": p})).collect::<Vec<_>>(),
        "direct_hits8": collect_direct_hits8(packets).iter().map(|d| json!({
            "t": d.t, "shooter": d.shooter, "victim": d.victim, "result": d.result,
            "damage": d.damage, "settled": d.settled,
        })).collect::<Vec<_>>(),
        "type33_pairing": collect_entity_cache_pairing(packets),
    })
}

/// Type5 物化尾部 loadout 的配件 ID 常量（WotbTools item-catalog，BlitzKit 生产定义同步）：
/// 103 = CALIBRATED_SHELLS 校准弹（AP/APCR 穿深 +6%、其他 +7%）、110 = ENHANCED_ARMOR 强化装甲（厚度 +4%）、
/// 122 = IMPROVED_PITCH_LIMITS 改进型垂直稳定器（仰角上限 +4°、俯角上限 +3°）。
pub const EQ_CALIBRATED_SHELLS: u8 = 103;
pub const EQ_ENHANCED_ARMOR: u8 = 110;
pub const EQ_IMPROVED_PITCH_LIMITS: u8 = 122;

/// 装备 122 的俯仰加成（度），与 `data/research/devices.json` 的
/// `script.params.bonusValues` 同值；`crates/replay-core/tests/pitch_device_data.rs`
/// 对该数据做锚定（客户端数值变动时测试失败，强制显式复核）。
pub const PITCH_DEVICE_UPPER_DEG: f32 = 4.0;
pub const PITCH_DEVICE_LOWER_DEG: f32 = 3.0;

/// 实体实际配件串 → 有效俯仰极限：含 122 时叠加 +4° 仰角 / +3° 俯角
/// （基础值与 front/back 扇区同增益，见 [`GunPitchRange::with_pitch_bonus`]），
/// 否则借用原值（零拷贝）。配置只来自该实体自己的 Type5 `0B` 配件串，
/// 不按昵称/车型推断。
pub fn effective_pitch_limits<'a>(
    base: &'a GunPitchRange,
    equipment: Option<&[u8; 9]>,
) -> Cow<'a, GunPitchRange> {
    if equipment.is_some_and(|eq| eq.contains(&EQ_IMPROVED_PITCH_LIMITS)) {
        Cow::Owned(base.with_pitch_bonus(PITCH_DEVICE_UPPER_DEG, PITCH_DEVICE_LOWER_DEG))
    } else {
        Cow::Borrowed(base)
    }
}

/// Type5 `0A` item 描述符的 `wireCode`（u8）→ 消耗品逻辑标识。
///
/// 语义源：WotbTools `docs/research/replay/loadout-materialization.md` /
/// `provision-wirecode-mapping.md`（11.19 生产语料行为闭合），以及本仓 11.20/11.21
/// 回放样本与客户端坦克百科 `consumables.json` 的闭合：
/// - `0xA9` = `DYNAMIC_ARMOUR_SYSTEM`（目录 id 73 `armorMover`，cooldown 60 / duration 17；
///   Type32 实测 state2 参数 22.61 = 17×1.33（装备 101 高端消耗品）、state3 参数 60.0，
///   且目录过滤器只包含 Titan Strife 等少数活动车）。
///
/// **未知线码一律返回 `None`，保留 raw 不猜名**。
pub fn consumable_logical_id(wire_code: u8) -> Option<&'static str> {
    Some(match wire_code {
        0x08 => "AUTOMATIC_FIRE_EXTINGUISHER",
        0x09 => "ADRENALINE",
        0x0A => "ENGINE_POWER_BOOST",
        0x0B => "MULTI_PURPOSE_RESTORATION_PACK",
        0x0C => "FIRST_AID_KIT",
        0x0D => "REPAIR_KIT",
        0x3D => "IMPROVED_ENGINE_POWER_BOOST",
        0x3E => "RETICLE_CALIBRATION",
        // 0x3F = 客户端 `newAutoloader`（Shell Reload Boost，dur 20s / cd 65s）。
        // 证据：M-VI-Yoh（弹夹重坦）slot0 激活 20.0s，冷却 55.555 = 65 × 0.8547
        // （装备 101 高-end 消耗品冷却缩短），与目录内唯一 dur20/cd65 条目吻合。
        0x3F => "SHELL_RELOAD_BOOST",
        0x42 => "REACTIVE_ARMOR",
        0x69 => "TUNGSTEN_SHELLS",
        0xA9 => "DYNAMIC_ARMOUR_SYSTEM",
        0xBD => "REDUCED_ENGINE_POWER_BOOST",
        _ => return None,
    })
}

/// Type5 `0A` item 描述符中**模式额外能力**的 `wireCode` → 逻辑标识。
///
/// `0x67` = 坦克百科 `consumables.json` 的 `gravityJump`（mode `lunar`，name "Thruster"，
/// acceleration 4 / cooldown 25）。Type32 实测同一线码的 state2 参数恒 0（瞬发）、state3
/// 冷却 25.0 / 21.368（=25÷1.17，装备 118 配送系统）/22.727/19.685，与 25s 基准及
/// 冷却缩短分支吻合；GravityMode 样本中只出现在非重型车（LT/MT/TD）的第 4 条描述符。
/// 未闭合线码返回 `None`。
pub fn ability_logical_id(wire_code: u8) -> Option<&'static str> {
    match wire_code {
        0x67 => Some("GRAVITY_JUMP"),
        _ => None,
    }
}

/// Type5 `0A` item 描述符的 `wireCode`（u8）→ 给养逻辑标识。
///
/// **线码是游戏内部补给槽 id 的自有命名空间，与客户端目录 id 无统一换算关系**
/// （WotbTools 11.19 文档原话："The replay wireCode is **not** assumed to equal
/// BlitzKit/public provision source ID"）。`103/104 → 0x46/0x47` 的 −33 关系只是两个
/// 观测点上的巧合归纳：瑞典对内 HQ/regular 顺序与欧系对相反（`0x48`=小/`0x49`=大），
/// 波兰 `0x6D/0x6E` 恰等于目录 id 109/110 的十进制原值。因此**每条线码独立取证**，
/// 禁止按数值邻接外推。证据：
/// - 七国连续块（德美苏英日中法）：WotbTools 11.19 结算战斗体（960 份 3+3+9，0 unknown）
///   行为闭合 `0x0E..0x12` 大 / `0x16..0x19` 小；本仓 11.20/11.21 样本补齐 `0x13`(中)
///   `0x14`(法) `0x15`(德) `0x1A`(中)，`0x1B`(法) 为块内位置；
/// - `0x1C/0x1D/0x1E` 标准燃料/改进燃料/防护套装、`0x44/0x45` 沙袋（Maus 同车 HP 控制
///   2987/2900=1.03、3074/2900=1.06 行为闭合）、`0x6A/0x6B/0x6C` 齿轮油（60TP 受控
///   实验：亲装改进型齿轮油 → `0x6B`）/改进火药（VK72.01 弹速 ×1.35 闭合）；
/// - `0x2D/0x2E` 其他系大/小补给、`0x32` 防破片内衬；
/// - 欧系/瑞典：`0x46`=欧系大(Cheesehead)、`0x47`=欧系小(Cinnamon Roll)（WotbTools
///   11.19 生产映射）；`0x48`=瑞典小(Crispbread)、`0x49`=瑞典大(Grilled Salmon)——
///   **受控实验钉死**（2026-10-31 Kranvagn 亲装大补给 → `0x49`），与 WotbTools 11.19
///   生产映射一致；本仓 v0.4.18 曾按「103..110 连续 −33」把 `0x48/0x49` 反标并外推
///   `0x4A..0x4D`，该推断已被受控实验与 `0x6D` 自然观测证伪，全部撤销；
/// - 捷克：**`0x4A` = 红烩牛肉/Goulash(107) 大——受控实验钉死**（2026-10-31 Vz.55 亲装
///   大口粮 → `0x4A`）；`0x4B` = 油炸奶酪/Fried Cheese(108) 小——`0x4A` 钉死后按成对 +
///   24 槽 24 件计数闭合消去（至此 33 件 common 补给 ↔ 33 线码槽**全绑定**）；
/// - 波兰：`0x6D`=茹尔汤/Zurek(109) **受控实验钉死**（2026-10-31 60TP 亲装茹尔汤 →
///   `0x6D`）+ WotbTools 11.18/11.20 自然观测（60TP 仅带 `0x6D`）；`0x6E` 在 53TP
///   上与 `0x6D` 成对出现 → 消去得 Faworki(110)/小补给；
/// - 曾把 `0x4C/0x4D` 当波兰大/小：纯结构外推、从未自然观测，已撤销返回 `None`。
///
/// **未知/未闭合线码一律返回 `None`，raw 保留不猜名。**
pub fn provision_logical_id(wire_code: u8) -> Option<&'static str> {
    Some(match wire_code {
        // 大补给（德国/美国/苏联/英国/日本/中国/法国）
        0x0E..=0x14 => "LARGE_FOOD",
        // 小补给（德国/美国/苏联/英国/日本/中国/法国）
        0x15..=0x1B => "SMALL_FOOD",
        0x1C => "STANDARD_FUEL",
        0x1D => "IMPROVED_FUEL",
        0x1E => "PROTECTIVE_KIT",
        // 其他系（other）大/小补给：样本成对出现（Titan Strife/E-10），按大→小线序
        0x2D => "LARGE_FOOD",
        0x2E => "SMALL_FOOD",
        0x32 => "SPALL_LINER",
        0x44 => "SANDBAG_ARMOR",
        0x45 => "ENHANCED_SANDBAG_ARMOR",
        // 欧系（大→小）/瑞典（小→大，顺序与欧系对相反——0x49 受控实验钉死；0x48 的
        // 族=瑞典为补对推断，置信低于直接观测，见 docs/index.md 2026-10-31 节未决）
        0x46 => "LARGE_FOOD",
        0x47 => "SMALL_FOOD",
        0x48 => "SMALL_FOOD",
        0x49 => "LARGE_FOOD",
        // 捷克：0x4A=Goulash(107) 大（受控钉死）；0x4B=Fried Cheese(108) 小（成对+计数
        // 闭合消去）；曾把 0x4C/0x4D 当捷克——从未自然观测，不猜。
        0x4A => "LARGE_FOOD",
        0x4B => "SMALL_FOOD",
        // 波兰：目录 id 109/110 十进制原值（受控实验 + 11.18/11.20 自然观测）
        0x6D => "LARGE_FOOD",
        0x6E => "SMALL_FOOD",
        0x6A => "GEAR_OIL",
        0x6B => "IMPROVED_GEAR_OIL",
        0x6C => "IMPROVED_GUNPOWDER",
        _ => return None,
    })
}

/// Type5 `0A` 给养描述符的 `wireCode` → 补给系别族（客户端 `provisions.json` 的
/// nation/sub-line 词汇）。仅食物族有系别；燃料/防护包/沙袋/内衬/齿轮油等通用给养
/// 返回 `None`。消费方（WotbTools）用 `(family, logical_id 的大小)` 选择正确系别
/// 的图标与名称——此前只导出 `LARGE_FOOD`/`SMALL_FOOD`，瑞典/捷克/波兰/欧洲车只能
/// 兜底显示「其他系」图标（Kranvagn 显示他系图标的根因）。
///
/// 系别证据 = **车辆可用性消去**（线码绑定唯一目录条目；观测到线码出现在某系坦克上，
/// 该系坦克可装备的食物族即唯一）：`0x46/0x47` 在意大利车（Carro 45t/Minotauro/
/// Progetto 46，仅欧系通用奶酪/肉桂卷可用）→ 欧系；`0x49` 在 Kranvagn（仅瑞典
/// 三文鱼/脆饼可用）→ 瑞典，`0x48` 与其同族成对；`0x4A` 在 Vz.55（仅捷克红烩牛肉/
/// 油炸奶酪可用）→ 捷克（`0x4B` 未观测不归族）；`0x6D/0x6E` 在 60TP/53TP（波兰）。
pub fn provision_family(wire_code: u8) -> Option<&'static str> {
    Some(match wire_code {
        // 大补给（德国/美国/苏联/英国/日本/中国/法国）
        0x0E..=0x14 => [
            "germany", "usa", "ussr", "uk", "japan", "china", "france",
        ][(wire_code - 0x0E) as usize],
        // 小补给（同序）
        0x15..=0x1B => [
            "germany", "usa", "ussr", "uk", "japan", "china", "france",
        ][(wire_code - 0x15) as usize],
        // 其他系（Hybrid Nation）
        0x2D | 0x2E => "other",
        // 欧洲通用（奶酪/肉桂卷；瑞典/捷克/波兰车被客户端 exclude 列表排除）
        0x46 | 0x47 => "european",
        // 瑞典（三文鱼 0x49 受控实验 / 脆饼 0x48）
        0x48 | 0x49 => "swedish",
        // 捷克（0x4A 受控钉死 Goulash；0x4B 成对消去 Fried Cheese）
        0x4A | 0x4B => "czech",
        // 波兰（茹尔汤/丝带脆饼）
        0x6D | 0x6E => "polish",
        _ => return None,
    })
}

/// Type5 item 描述符的语义槽位。`Consumable` 固定前 3 条（位置契约），
/// `Provision` 由静态尾标记 `f32 -1.0` 识别，其余为模式额外能力（如 GravityMode 的
/// 第 4 条描述符）——**不能把能力当给养**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadoutItemKind {
    Consumable,
    Provision,
    Ability,
}

/// 一条 14B Type5 item 描述符的解码视图（raw 恒保留）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadoutItem {
    /// 描述符在 `0A <k>` 块内的位置
    pub index: u8,
    pub kind: LoadoutItemKind,
    /// 原始 wireCode（u8）
    pub wire_code: u8,
    /// 原始 state（u8）
    pub state: u8,
    /// 原始 12B 载荷（计时器/动态状态尚未完全解码 → 不赋内部语义）
    pub payload_raw: [u8; 12],
    /// 已证逻辑标识；`None` = 线码未闭合（raw 保留，不猜名）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_id: Option<String>,
    /// 补给系别族（仅 `kind = Provision` 的食物族有值：germany/usa/ussr/uk/japan/
    /// china/france/european/swedish/czech/polish/other）；消耗品/能力/通用给养 = None。
    /// 消费方按 `(family, logical_id)` 选系别正确的图标与名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
}

/// 射击一方车辆的配件搭载（Type5 物化 `0B <n>` 变长选择串解码）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleEquipment {
    /// 携带校准弹（ID 103）
    pub calibrated_shells: bool,
    /// 携带强化装甲（ID 110）
    pub enhanced_armor: bool,
    /// 携带改进型垂直稳定器（ID 122）：炮管仰角上限 +4°、俯角上限 +3°
    #[serde(default, skip_serializing_if = "is_false")]
    pub improved_pitch_limits: bool,
    /// 实装件数（`0B <n>` 的 n；空槽省略）
    #[serde(default)]
    pub equipment_count: u8,
    /// 实际选择的配件 ID（线序，不含空槽补齐的 0）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equipment_ids: Vec<u8>,
    /// 九槽配件 ID 数组（诊断/未来扩展；未知 ID 不猜名）。线路串是**变长**的
    /// （空槽省略），此处按原序填入、空槽以 0 补齐——`0` 不是合法配件 ID，
    /// 故不可据位置反推槽位（空槽位置信息线上不存在）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub raw: Vec<u8>,
}

impl VehicleEquipment {
    pub(crate) fn from_ids(eq: &[u8; 9]) -> Self {
        let equipment_ids: Vec<u8> = eq.iter().copied().filter(|&id| id != 0).collect();
        VehicleEquipment {
            calibrated_shells: eq.contains(&EQ_CALIBRATED_SHELLS),
            enhanced_armor: eq.contains(&EQ_ENHANCED_ARMOR),
            improved_pitch_limits: eq.contains(&EQ_IMPROVED_PITCH_LIMITS),
            equipment_count: equipment_ids.len() as u8,
            equipment_ids,
            raw: eq.to_vec(),
        }
    }
}

/// 每实体 Type5 物化 → 配件选择串（首条有效物化为准；配件开局固定，敌方再物化
/// Type4→Type33→Type5 重复携带，WotbTools 683/683）。
/// 扫描契约：offset 可变，定位 `0B <n>` + nB 配件串（**n 变长**，1..=9 = 实装件数，
/// 空槽省略；满配 9 件），字节全部落在已知配件 ID 域 100..=123 才采纳（framing 误配
/// 不猜名；2026-10-10 修：此前硬编码 `0B 09`，凡不满 9 件的玩家整条 loadout 被静默
/// 丢弃——60TP 实测 `0B 02 67 6c` = 校准弹+改进型模块，查看器据此判成"未装备校准弹"）。
pub fn collect_vehicle_equipment(packets: &[(u32, f32, &[u8])]) -> HashMap<u32, [u8; 9]> {
    collect_vehicle_loadout(packets)
        .into_iter()
        .map(|(k, v)| (k, v.equipment))
        .collect()
}

/// 采集每实体的完整开局 loadout（Type5 3+3+9；item 描述符可变 3..=7）。
/// 同一实体取首条成功解析者；**首条只有配件串（无 `0A` 块）时，若后续物化包带来
/// item 描述符则以带 items 的版本替换**（配件不变，物品不丢）。观察者/非战斗实体由
/// 调用方按 eid/class/结算花名册过滤；本函数不按 k 值猜身份。
pub fn collect_vehicle_loadout(packets: &[(u32, f32, &[u8])]) -> HashMap<u32, VehicleLoadout> {
    let mut out: HashMap<u32, VehicleLoadout> = HashMap::new();
    for (ptype, _, p) in packets {
        if *ptype != 5 {
            continue;
        }
        let eid = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
        if let Some(prev) = out.get(&eid) {
            if !prev.items.is_empty() {
                continue;
            }
        }
        if let Some(l) = scan_loadout(p) {
            // 已有 equipment-only 版本时，仅当新版本含 item 描述符才替换
            let replace = out
                .get(&eid)
                .map(|prev| prev.items.is_empty() && !l.items.is_empty())
                .unwrap_or(true);
            if replace {
                out.insert(eid, l);
            }
        }
    }
    out
}

/// Avatar method16：车辆模块/乘员状态事件（WotbTools PROVEN 移植）。
///
/// 线格式（`VehicleModuleCrewStateDecoder`，恒定 22B）：
/// `[0..4) avatarEid | [4..8) method=16 | [8..12) argLen=10 | [12..16) vehicleId |
///  [16] stateCode(codeA) | [17] componentCode(codeB) | [18..22) relatedEntityId`。
///
/// codeB → 组件（PROVEN）：31 引擎 / 32 弹药架 / 33 油箱 / 34 右履带 / 35 左履带 /
/// 36 主炮 / 37 炮塔座圈 / 38 观察装置 / 39 车长 / 40 驾驶员 / 41 炮手 / 43 装填手；
/// 其余（含缺失的 42）原样保留为 UNKNOWN，不猜。
/// codeA → 状态：**乘员**（39/40/41/43）：10 震伤 / 22 治愈；**模块**：4 受损降效 /
/// 5 致命失效 / 18 自动修复至受损 / 19 完全修复；其余 UNKNOWN。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleComponent {
    Engine,
    AmmoRack,
    FuelTank,
    RightTrack,
    LeftTrack,
    Gun,
    TurretRotator,
    ObservationDevice,
    Commander,
    Driver,
    Gunner,
    Loader,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleState {
    DamagedDegraded,
    CriticalDisabled,
    AutoRepairedToDamaged,
    FullRepairedClear,
    CrewShellShocked,
    CrewHealed,
    Unknown,
}

impl ModuleComponent {
    fn from_code(code: u8) -> Self {
        match code {
            31 => Self::Engine,
            32 => Self::AmmoRack,
            33 => Self::FuelTank,
            34 => Self::RightTrack,
            35 => Self::LeftTrack,
            36 => Self::Gun,
            37 => Self::TurretRotator,
            38 => Self::ObservationDevice,
            39 => Self::Commander,
            40 => Self::Driver,
            41 => Self::Gunner,
            43 => Self::Loader,
            _ => Self::Unknown,
        }
    }
    fn is_crew(self) -> bool {
        matches!(
            self,
            Self::Commander | Self::Driver | Self::Gunner | Self::Loader
        )
    }
}

/// 模块/乘员状态事件：raw 码与映射并存（raw 保留以便未来语义扩充时不失真）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModuleCrewStateEvent {
    pub clock: f32,
    /// 受损车辆实体
    pub vehicle_eid: u32,
    /// codeA（raw）
    pub state_code: u8,
    /// codeB（raw）
    pub component_code: u8,
    pub component: ModuleComponent,
    pub state: ModuleState,
    pub related_eid: u32,
}

pub fn collect_module_crew_states(packets: &[(u32, f32, &[u8])]) -> Vec<ModuleCrewStateEvent> {
    let mut out = Vec::new();
    for (t, clock, p) in packets {
        if *t != 8 || p.len() != 22 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 16 {
            continue;
        }
        if u32::from_le_bytes([p[8], p[9], p[10], p[11]]) != 10 {
            continue;
        }
        let vehicle_eid = u32::from_le_bytes([p[12], p[13], p[14], p[15]]);
        let state_code = p[16];
        let component_code = p[17];
        let component = ModuleComponent::from_code(component_code);
        let state = if component.is_crew() {
            match state_code {
                10 => ModuleState::CrewShellShocked,
                22 => ModuleState::CrewHealed,
                _ => ModuleState::Unknown,
            }
        } else {
            match state_code {
                4 => ModuleState::DamagedDegraded,
                5 => ModuleState::CriticalDisabled,
                18 => ModuleState::AutoRepairedToDamaged,
                19 => ModuleState::FullRepairedClear,
                _ => ModuleState::Unknown,
            }
        };
        out.push(ModuleCrewStateEvent {
            clock: *clock,
            vehicle_eid,
            state_code,
            component_code,
            component,
            state,
            related_eid: u32::from_le_bytes([p[18], p[19], p[20], p[21]]),
        });
    }
    out.sort_by(|a, b| {
        a.clock
            .partial_cmp(&b.clock)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// 车辆开局 loadout（Type5 3+3+9 结构；WotbTools PROVEN 位置闭合 + 本仓 11.20/11.21 复核）。
///
/// - `equipment`：配件选择串解码为 9 槽数组——**每字节 = 装备数值 ID（= 目录中该件
///   字母的 ASCII 码，如 `g`=103 校准炮弹 / `n`=110 强化装甲）**；线路上串**变长**
///   （`0B <n>`，空槽省略、满配 9），此处空槽补 0；`equipment_count`/`equipment_ids`
///   保留线序实装件（空槽不再以 0 混入）；
/// - `items`：k 条 14 字节 item 描述符原样保留；`consumables`/`provisions`/`abilities`
///   是同一批描述符的解码视图：
///   * 前 3 条 = 消耗品槽（位置契约，WotbTools 1,037/1,037）；
///   * 其余条目中，静态尾 `f32 -1.0`（`00 00 80 BF`）= 给养槽；
///   * 其余无静态尾标记的条目 = 模式额外能力（如 GravityMode 的第 4 条描述符
///     `0x67`），**不是给养**——WotbTools 的 6 条固定布局在 11.20+ 会被这条能力
///     撑成 7 条，按位置硬切会把能力误判成给养；
///   * 描述符内部字段（计时器/动态状态）尚未完全解码 → `payload_raw` 原样保留，
///     不赋内部语义（WotbTools 明确裁决：retain raw until fully decoded）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleLoadout {
    pub equipment: [u8; 9],
    /// 实装配件数（`0B <n>` 的 n）
    #[serde(default)]
    pub equipment_count: u8,
    /// 实装配件 ID（线序，不含空槽补齐的 0）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equipment_ids: Vec<u8>,
    /// 原始 14B item 描述符（恒保留，供未解码字段/回滚诊断）
    pub items: Vec<[u8; 14]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumables: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abilities: Vec<LoadoutItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provisions: Vec<LoadoutItem>,
}

/// 给养描述符的静态尾标记：载荷最后 4 字节 = f32 -1.0（`00 00 80 BF`）。
/// 这是把给养与消耗品/模式能力分开的**格式不变量**，不是内容统计。
const PROVISION_STATIC_TAIL: [u8; 4] = [0x00, 0x00, 0x80, 0xBF];

fn is_provision_descriptor(item: &[u8; 14]) -> bool {
    item[10..14] == PROVISION_STATIC_TAIL
}

/// 把 `0A <k>` 的原始描述符块解码为 (消耗品, 模式能力, 给养)。
///
/// 布局护栏：前 3 条必须都不带给养静态尾（3+3 位置契约）；否则视为未知布局，
/// 只保留 raw（`items`），语义数组为空——绝不按位置硬猜。
fn decode_loadout_items(
    items: &[[u8; 14]],
) -> (Vec<LoadoutItem>, Vec<LoadoutItem>, Vec<LoadoutItem>) {
    let mut consumables = Vec::new();
    let mut abilities = Vec::new();
    let mut provisions = Vec::new();
    if !items.iter().take(3).all(|it| !is_provision_descriptor(it)) {
        return (consumables, abilities, provisions);
    }
    for (i, raw) in items.iter().enumerate() {
        let wire_code = raw[0];
        let state = raw[1];
        let mut payload_raw = [0u8; 12];
        payload_raw.copy_from_slice(&raw[2..14]);
        let (kind, logical_id) = if i < 3 {
            (
                LoadoutItemKind::Consumable,
                consumable_logical_id(wire_code).map(str::to_string),
            )
        } else if is_provision_descriptor(raw) {
            (
                LoadoutItemKind::Provision,
                provision_logical_id(wire_code).map(str::to_string),
            )
        } else {
            // 模式额外能力：已闭合的 0x67 = gravityJump/Thruster；其余仅 raw
            (
                LoadoutItemKind::Ability,
                ability_logical_id(wire_code).map(str::to_string),
            )
        };
        // 系别族仅食物族有值；大小未闭合的线码（如 0x4A 捷克）同样导出族，消费方
        // 据此至少选对系别，大小仍按 unknown 处理。
        let family = if kind == LoadoutItemKind::Provision {
            provision_family(wire_code).map(str::to_string)
        } else {
            None
        };
        let item = LoadoutItem {
            index: i as u8,
            kind,
            wire_code,
            state,
            payload_raw,
            logical_id,
            family,
        };
        match kind {
            LoadoutItemKind::Consumable => consumables.push(item),
            LoadoutItemKind::Provision => provisions.push(item),
            LoadoutItemKind::Ability => abilities.push(item),
        }
    }
    (consumables, abilities, provisions)
}

/// 在单条 Type5 载荷内扫描 loadout 块：先定位 `0B <n>` + nB 配件串（字节域 100..=123
/// 校验）与帧尾 `0C 00 0D`——这是**装备检测的唯一准入门槛**；`0A KK` + KK×14B
/// item 描述符块只用于 items/消耗品/给养，缺失/数量不认识时不再丢弃装备串。
///
/// `KK` 是**变长**的 item 描述符数量：实测 3..=7（常见 3 消耗品 + 3 给养 = 6 条；
/// 少带给养的玩家出现 3/4/5 条；11.20+ 模式额外能力会在消耗品与给养之间插入第 4 条，
/// 形成 7 条）。旧实现把 `0A KK` 当作整个 loadout 的硬门槛（且只接受 6..=10），这类
/// 玩家的整条 loadout 被静默丢弃（射击复现里表现为「射手校准弹未自动识别」）。
/// 观察者/非战斗实体不做 k 值启发式排除——由调用方按 eid/class/结算花名册过滤，
/// 避免把配装较少的真实玩家误判为观察者。
fn scan_loadout(p: &[u8]) -> Option<VehicleLoadout> {
    let n = p.len();
    let mut pos = 0usize;
    while pos + 2 <= n {
        // 配件串：`0B <count>` + count×1B（**变长**，1..=9 = 实装件数，空槽省略）。
        if p[pos] == 0x0B {
            let count = p[pos + 1] as usize;
            if (1..=9).contains(&count) && pos + 5 + count <= n {
                let eq = &p[pos + 2..pos + 2 + count];
                // 帧护栏：字节全在配件 ID 域 100..=123，且串后恒为 `0C 00 0D`
                // （全语料 175/175 成立；变长放宽后靠它把误配面压回原量级）。
                if eq.iter().all(|&b| (100..=123).contains(&b))
                    && p[pos + 2 + count] == 0x0C
                    && p[pos + 3 + count] == 0x00
                    && p[pos + 4 + count] == 0x0D
                {
                    let mut equipment = [0u8; 9];
                    // 变长串按原序填入，空槽补 0（0 不在 ID 域，不是合法件）
                    equipment[..count].copy_from_slice(eq);
                    // 描述符块是可选的：只用于 items，不作为 equipment 的准入门槛。
                    let items = (1..=10usize)
                        .find_map(|k| {
                            if pos < 2 + k * 14 {
                                return None;
                            }
                            let start = pos - 2 - k * 14;
                            if p[start] != 0x0A || p[start + 1] as usize != k {
                                return None;
                            }
                            Some(
                                (0..k)
                                    .map(|i| {
                                        let s = start + 2 + i * 14;
                                        let mut it = [0u8; 14];
                                        it.copy_from_slice(&p[s..s + 14]);
                                        it
                                    })
                                    .collect::<Vec<[u8; 14]>>(),
                            )
                        })
                        .unwrap_or_default();
                    let (consumables, abilities, provisions) = decode_loadout_items(&items);
                    return Some(VehicleLoadout {
                        equipment,
                        equipment_count: count as u8,
                        equipment_ids: eq.to_vec(),
                        items,
                        consumables,
                        abilities,
                        provisions,
                    });
                }
            }
        }
        pos += 1;
    }
    None
}

/// method29 (0x1d) 发射事件。args 布局（alen=37）：
/// [shooterEntityId u32][shotId u32][rawFlag u8][launchPoint 3×f32][launchVelocity 3×f32][terminalRaw f32]
#[derive(Clone)]
pub(crate) struct LaunchEntry {
    pub(crate) t: f32,
    pub(crate) shooter: u32,
    pub(crate) shot_id: u32,
    pub(crate) point: [f32; 3],
    pub(crate) vel: [f32; 3],
    /// args[8]（旧称 rawFlag）= **弹种编码**：全局每个 shells.xml 弹种条目恒定的 1 字节码
    /// （全语料 2440 发逐发携、137 个弹种零冲突；同一炮同弹种跨玩家一致）。
    /// 结构：低 2 位 = 弹种类别（0=AP / 1=HE / 2=HEAT / 3=APCR，常规弹全数吻合；"现代脱壳弹族"
    /// （T-100 LT「3VBM」/ LT-432 / Rhm. Pzw. 的 APFSDS 与其 HEAT）独立成组 0x17，低 2 位不表类别）；
    /// 高位 = 弹种家族/口径档（观测 1/2/3 档 + 0x17 组），命名未闭合（不猜）。用途：命中通知段包
    /// 缺失（cmpIndex=0 底盘/履带命中）时的弹种回填指纹——同场自校准 + 弹速去重，见 shots.rs。
    pub(crate) shell_code: u8,
    /// method29 包处理时刻（**流序**）射手的最后已知 prop2 原始 u16——WI 解析器同构快照。
    /// 与时钟序"≤t 最后采样"的差异仅在同 tick 内包序：method29 包之前到达的 prop2 才计入。
    pub(crate) shooter_prop2: Option<(f32, u16)>, // (采样钟, 原始 u16)
}

/// method20 (0x14) 弹道终点（shotId 配对）。
/// method8 直击通知（全局广播，envelope eid = 受击者）；
/// args = [shooterEntityId u32][victimEntityId u32][01][result u8][extra u8][hash6][tail...]；result 枚举与 type=32 同域，hash6 与同事件 type=32 完全一致。
/// victim_state = 该通知包处理时刻（文件序）受击者的最后已知 type=10 姿态——
/// wotinspector distance 的精确取值基准，受击方锚点。
pub(crate) struct DirectHit8 {
    pub(crate) t: f32,
    pub(crate) shooter: u32,
    pub(crate) victim: u32,
    pub(crate) result: u8,
    /// args[10] = **服务器下发的受击部件索引 cmpIndex**（showDamageFromShot
    /// 的 8 字节 segment 描述符元素 byte1，客户端 BWUtils::DecodeShotSegment 以 bboxes[4]/
    /// partMatrixes[4] 按此部件放置着弹点——见《回放与射击逆向总集》第三篇 §二）。
    /// 部件对应（命中离地高度分层实证）：**0=底盘/履带、1=车体、2=炮塔、3=炮管（直射弹未观测）**。
    pub(crate) component_index: Option<u8>,
    pub(crate) hash6: [u8; 6],
    /// **服务器结算的该发伤害**（args 尾部 `[…][u16 伤害][u8 状态]` 的 u16，2026-10-15 全语料实证）：
    /// 状态字节 ≠0（观测 0x01/0x03）= 已结算，u16 = 实际掉血值（**overkill 已封顶**——剩 124 HP
    /// 被击穿记 124；复活模式 HP 重置吞并的伤害也如实给出）；状态字节 =0 = 未结算（u16 可能是
    /// 弹种标称占位值，**不可当伤害用**）。
    pub(crate) damage: u16,
    /// 尾部状态字节 ≠0（见 [`DirectHit8::damage`]）
    pub(crate) settled: bool,
    pub(crate) victim_state: Option<([f32; 3], [f32; 3], f32)>, // (pos, ang[yaw,pitch,roll], 状态采样时钟)
    /// method8 包处理时刻（**流序**）受击者的最后已知 prop2 原始 u16——WI 解析器同构快照
    /// （battle.json turret_yaw/gun_pitch 的取样基准：炮塔 coarse10 与流序快照逐位相等——
    /// 同 tick 内 prop2 与 method8 的包序决定取值）。(采样钟, 原始 u16)
    pub(crate) victim_prop2: Option<(f32, u16)>,
}

/// type=32 来袭炮弹警告/命中通知（eid = 受击者，AoI 广播含他人命中）。
/// 帧结构（WotbTools 16,850/16,850）：[eid u32][flag u8][bodyLength u32][body]，bodyLength == payloadLen − 9。
/// len=26 (bodyLen=17): [eid u32][01][bodyLen u32][u16@9][flag@11][hash6@12..18][segment u64@18..26]
/// len=27 (bodyLen=18): [eid u32][01][bodyLen u32][u16@9][flag@11][01@12][hash6@13..19][segment u64@19..27]
/// hash6 6B = [shell u16][来向 yaw u16][抵达 pitch u16]：
///   yaw = (u16−32768)/32768×π = 受击者指向射手的方位角；pitch = (u16−32768)/32768×(π/2)
///   = 抵达垂直角。非命中的路过炮弹警告会被 yaw 校验弃用（decoded_target_gun_pitch）。
#[derive(Clone)]
pub(crate) struct ArenaWarning32 {
    pub(crate) t: f32,
    pub(crate) eid: u32,
    pub(crate) result: u8,
    pub(crate) segment: u64,
    pub(crate) hash6: [u8; 6],
    pub(crate) inc_yaw: f32,
    pub(crate) inc_pitch: f32,
}

/// 血量链降幅区间（参考 WotbTools PlaybackCombatReconstruction.deriveLosses）。
pub(crate) struct DmgLoss {
    pub(crate) victim: u32,
    pub(crate) source: u32,
    pub(crate) t_cur: f32,
    pub(crate) dmg: u32,
    pub(crate) hp_cur: u16,
}

/// method29 (0x1d) 发射事件收集（作者/他人路径共用）：clock ≥5s，按 shooter 过滤。
///
/// **一次开火 = (shooter, shotId) 一条链**：同键的后续 method29 是**同一发炮弹的后续弹道段**
/// （服务器在命中/跳弹时刻重播该发续段：`point` = 跳弹/穿透出射点、`vel` = 续段方向——实测
/// 跳弹样本：首发在炮口仰角 −1.5°，续段起点在受击方装甲处、方向 +38.6°，与该方法8 命中通知
/// 同刻）。因此这里**不做去重**：全部包按 (clock, 流序) 稳定排序返回，由组装方按
/// (shooter, shotId) 分组——首条 = 发射段，其余 = 续段（见 [`shot_segments`]）。
///
/// 按发射时刻排序（稳定排序 → 同刻保流序）。返回 (发射列表, 每射手首个 args<37 包的 args_len)
/// ——作者路径据此对**自己**的短包 fail-fast，他人路径跳过（宽松）。
pub(crate) fn collect_launches(
    packets: &[(u32, f32, &[u8])],
    keep: impl Fn(u32) -> bool,
) -> (Vec<LaunchEntry>, HashMap<u32, usize>) {
    let mut out: Vec<LaunchEntry> = Vec::new();
    let mut short_args: HashMap<u32, usize> = HashMap::new();
    // 流序当前 prop2（全实体维护——keep 过滤只作用于发射事件本身）
    let mut ang2: std::collections::HashMap<u32, (f32, u16)> = Default::default();
    for (t2, clock, p) in packets {
        if *clock < 5.0 || p.len() < 12 {
            continue;
        }
        if *t2 == 7 && p.len() >= 14 && u32::from_le_bytes([p[4], p[5], p[6], p[7]]) == 2 {
            ang2.insert(
                u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
                (*clock, u16::from_le_bytes([p[12], p[13]])),
            );
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x1d {
            continue;
        }
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        if args_len < 4 || 12 + args_len > p.len() {
            continue;
        } // 连 shooter 都读不出：无法归属，跳过
        let a = &p[12..12 + args_len];
        let shooter = u32::from_le_bytes([a[0], a[1], a[2], a[3]]);
        if !keep(shooter) {
            continue;
        }
        if args_len < 37 {
            short_args.entry(shooter).or_insert(args_len);
            continue;
        }
        let shot_id = u32::from_le_bytes([a[4], a[5], a[6], a[7]]);
        let f = |o: usize| f32::from_le_bytes([a[o], a[o + 1], a[o + 2], a[o + 3]]);
        out.push(LaunchEntry {
            t: *clock,
            shooter,
            shot_id,
            point: [f(9), f(13), f(17)],
            vel: [f(21), f(25), f(29)],
            shell_code: a[8],
            shooter_prop2: ang2.get(&shooter).copied(),
        });
    }
    out.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap());
    (out, short_args)
}

/// 一条弹道链的**续段序列**：(时刻, 起点, 速度)，按飞行顺序（见 [`shot_segments`]）
pub(crate) type ShotSegmentChain = Vec<(f32, [f32; 3], [f32; 3])>;
/// (shooter, shotId) → 该发的续段序列（无续段 = 无键）
pub(crate) type ShotSegments = HashMap<(u32, u32), ShotSegmentChain>;

/// (shooter, shotId) 链 → **续段表**：每条链的首条为发射段，其余为**同一发的续段**
/// （命中/跳弹后继续飞：起点 = 装甲接触点 / 穿透出射点，速度 = 续段方向，见 `collect_launches`）。
///
/// 返回 `(续段标记, 续段表)`：`is_continuation[i]` 为真表示第 i 条是某发的续段（不是独立射击，
/// 组装时跳过）；`segments[(shooter, shotId)]` = 该发**全部续段的 (时刻, 起点, 速度)**，按飞行
/// 顺序（= launches 的 clock/流序）。渲染层用它把弹道画成折线（`from → 续段起点… → method20 终点`），
/// 命中归属窗口用首个续段时刻。无续段时表内无该键（直线弹道，终点用 method20）。
pub(crate) fn shot_segments(launches: &[LaunchEntry]) -> (Vec<bool>, ShotSegments) {
    let mut seen: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut is_continuation = vec![false; launches.len()];
    let mut segments: ShotSegments = HashMap::new();
    for (i, l) in launches.iter().enumerate() {
        let key = (l.shooter, l.shot_id);
        if seen.insert(key) {
            continue;
        }
        is_continuation[i] = true;
        segments.entry(key).or_default().push((l.t, l.point, l.vel));
    }
    (is_continuation, segments)
}

/// 弹道折线：`(中间点 via, 各段时长 leg_secs)`。
///
/// 时长按 **几何 / 段速度** 计算（段 k 的速度 = 该段起点处速度：发射段用发射速度，续段用其自带
/// 速度）——**不用包时钟**：包钟是 10Hz 量化的，跳弹常与发射同刻（样本里 71.219 vs 71.22），
/// 用钟会得到零长段（炮弹瞬间闪到跳弹点）。末段速度为最后一个续段速度。
/// `via` 与 `leg_secs` 满足 `leg_secs.len() == via.len() + 1`。
pub(crate) fn shot_legs(
    from: [f32; 3],
    launch_vel: [f32; 3],
    chain: Option<&ShotSegmentChain>,
    to: [f32; 3],
) -> (Vec<[f32; 3]>, Vec<f32>) {
    let empty: ShotSegmentChain = Vec::new();
    let chain = chain.unwrap_or(&empty);
    let mut pts: Vec<[f32; 3]> = Vec::with_capacity(chain.len() + 2);
    pts.push(from);
    for (_, p, _) in chain.iter() {
        pts.push(*p);
    }
    pts.push(to);
    let speed_at = |k: usize| -> f32 {
        let v = if k == 0 {
            launch_vel
        } else {
            chain.get(k - 1).map(|(_, _, v)| *v).unwrap_or(launch_vel)
        };
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    };
    let mut legs: Vec<f32> = Vec::with_capacity(pts.len() - 1);
    for k in 0..pts.len() - 1 {
        let d = [
            pts[k + 1][0] - pts[k][0],
            pts[k + 1][1] - pts[k][1],
            pts[k + 1][2] - pts[k][2],
        ];
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let s = speed_at(k);
        legs.push(if s > 1.0 { dist / s } else { 0.5 });
    }
    let via = pts[1..pts.len() - 1].to_vec();
    (via, legs)
}

/// method20 (0x14) 弹道终点收集（作者/他人路径共用）：shotId 配对（含 miss 的空地终点），重复 shotId 保留首条。
pub(crate) fn collect_endpoints(packets: &[(u32, f32, &[u8])]) -> HashMap<u32, (f32, [f32; 3])> {
    let mut endpoints: HashMap<u32, (f32, [f32; 3])> = HashMap::new();
    for (_, clock, p) in packets {
        if p.len() < 28 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x14 {
            continue;
        }
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        if args_len < 16 || 12 + args_len > p.len() {
            continue;
        }
        let shot_id = u32::from_le_bytes([p[12], p[13], p[14], p[15]]);
        let a = &p[16..];
        endpoints.entry(shot_id).or_insert((
            *clock,
            [
                f32::from_le_bytes([a[0], a[1], a[2], a[3]]),
                f32::from_le_bytes([a[4], a[5], a[6], a[7]]),
                f32::from_le_bytes([a[8], a[9], a[10], a[11]]),
            ],
        ));
    }
    endpoints
}

/// method8 直击通知收集（作者/他人路径共用），按时钟排序。
pub(crate) fn collect_direct_hits8(packets: &[(u32, f32, &[u8])]) -> Vec<DirectHit8> {
    // 文件序状态机：按包出现顺序维护每实体最后已知 type=10 姿态，method8 到达时快照受击者。
    // （wi 对齐验证：distance = |state[shooter] − state[victim]|@method8，99/99 发 median 残差 2μm）
    let mut pose: HashMap<u32, ([f32; 3], [f32; 3], f32)> = HashMap::new();
    // 同一状态机的 prop2 通道：method8 到达时快照受击者最后已知 prop2（WI turret_yaw 同基准）
    let mut ang2: HashMap<u32, (f32, u16)> = HashMap::new();
    let mut direct_hits8: Vec<DirectHit8> = Vec::new();
    for (t2, clock, p) in packets {
        if *t2 == 10 && p.len() >= 48 {
            let f = |o: usize| f32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
            pose.insert(
                u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
                ([f(12), f(16), f(20)], [f(36), f(40), f(44)], *clock),
            );
            continue;
        }
        if *t2 == 7 && p.len() >= 14 && u32::from_le_bytes([p[4], p[5], p[6], p[7]]) == 2 {
            ang2.insert(
                u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
                (*clock, u16::from_le_bytes([p[12], p[13]])),
            );
            continue;
        }
        if p.len() < 12 + 10 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x08 {
            continue;
        }
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        // 直击元素形态需要 hash6 六字节（a[11..17]）——args_len < 17 不是该形态，丢弃。
        if args_len < 17 || 12 + args_len > p.len() {
            continue;
        }
        let a = &p[12..12 + args_len];
        if a[8] != 0x01 {
            continue;
        }
        let victim = u32::from_le_bytes([a[4], a[5], a[6], a[7]]);
        // args = [shooter u32][victim u32][count u8][element 8B = result|cmpIndex|hash6 u48][tail 4B]
        // a[10] = 服务器下发的受击部件索引 cmpIndex（showDamageFromShot/DecodeShotSegment）
        let component_index = if args_len >= 11 { Some(a[10]) } else { None };
        // 尾部结算伤害（args_len=21 观测形态：a[18..20] = u16 LE 伤害，a[20] = 结算状态字节）。
        // 更短/更长的其它收敛形态不带该字段 → damage=0/settled=false（fail-closed）。
        let (damage, settled) = if args_len >= 21 {
            (
                u16::from_le_bytes([a[args_len - 3], a[args_len - 2]]),
                a[args_len - 1] != 0x00,
            )
        } else {
            (0, false)
        };
        direct_hits8.push(DirectHit8 {
            t: *clock,
            shooter: u32::from_le_bytes([a[0], a[1], a[2], a[3]]),
            victim,
            result: a[9],
            component_index,
            hash6: [a[11], a[12], a[13], a[14], a[15], a[16]],
            damage,
            settled,
            victim_state: pose.get(&victim).map(|(pos, ang, c)| (*pos, *ang, *c)),
            victim_prop2: ang2.get(&victim).copied(),
        });
    }
    direct_hits8.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap());
    direct_hits8
}

/// method8（0x08）伤害/命中反馈通知的**原始**形态：全变体、不分类（[`collect_direct_hits8`]
/// 只收 `args[8]==1` 的直击元素并做射击配对，本收集器保留每一包供消费方自行分类）。
/// envelope eid = 方法调用目标实体（通常 = 受击者）；args =
/// `[shooter u32][victim u32][count u8][result u8][cmpIndex u8][hash6][tail…]`。
/// 字段缺失（载荷不足）一律 None——不臆测；分类口径（直击 / 未解码变体 / 短体）属于消费方。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HitNotice {
    pub clock: f32,
    /// envelope 实体（方法调用目标）
    pub eid: u32,
    /// 包载荷总长（字节；含 12 字节 envelope 头）
    pub payload_len: u32,
    pub shooter_eid: Option<u32>,
    pub victim_eid: Option<u32>,
    /// args[9]：游戏命中结果枚举（与 type=32 segment 低字节同域）
    pub result: Option<u8>,
    /// args[10]：受击部件索引 cmpIndex
    pub secondary: Option<u8>,
}

/// method8 原始通知收集（仅 type=8 实体方法包；按时钟排序，稳定保留包序）。
pub fn collect_hit_notices(packets: &[(u32, f32, &[u8])]) -> Vec<HitNotice> {
    let mut out: Vec<HitNotice> = Vec::new();
    for (ptype, clock, p) in packets {
        if *ptype != 8 || p.len() < 12 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x08 {
            continue;
        }
        let a = &p[12..];
        let u32_at = |o: usize| {
            (a.len() >= o + 4).then(|| u32::from_le_bytes([a[o], a[o + 1], a[o + 2], a[o + 3]]))
        };
        out.push(HitNotice {
            clock: *clock,
            eid: u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
            payload_len: p.len() as u32,
            shooter_eid: u32_at(0),
            victim_eid: u32_at(4),
            result: a.get(9).copied(),
            secondary: a.get(10).copied(),
        });
    }
    out.sort_by(|x, y| x.clock.partial_cmp(&y.clock).unwrap());
    out
}

/// Type32 consumable lifecycle state（与 WotbTools 的 canonical 名称逐字一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsumableState {
    /// state 1：注册/可用（开局未使用，或冷却结束后回到可用）
    Initialized,
    /// state 2：激活（param = 有效持续时长；瞬发为 0）
    Activated,
    /// state 3：持续结束/冷却转换（param = 有效冷却时长）
    ActiveEndedOrCooldown,
    /// state 255：实体/控制拆除（离场、死亡、对局结束）
    Teardown,
    /// 未识别 state 码
    Unknown,
}

impl ConsumableState {
    fn from_wire_state(state: u8) -> Self {
        match state {
            1 => Self::Initialized,
            2 => Self::Activated,
            3 => Self::ActiveEndedOrCooldown,
            255 => Self::Teardown,
            _ => Self::Unknown,
        }
    }
}

/// Type32 mobile `flag=0` 长体的**消耗品生命周期**事件（WotbTools PROVEN 移植）。
///
/// 数据链（docs/research/replay/consumable-lifecycle.md，Blitz 11.19 国服 34 场）：
/// Type32 `entityTypeId=2` / `flag=0` / 16 字节体（body 自 p[9] 起）：
/// `wireCode = body[2]`、`state = body[3]`、`bodyClock = f64 LE body[4..12)`、
/// `param = f32 LE body[12..16)`。
///
/// state 语义（该族已闭合）：1 注册/可用、2 激活（param = 有效持续时长，瞬发为 0）、
/// 3 持续结束/冷却转换（param = 有效冷却配置）、255 实体/控制拆除。
///
/// **状态/剩余时间**：`state_name` 是语义状态；state 2/3 的 `duration_s` = param、
/// `ends_at` = clock + param（激活结束 / 冷却结束）。消费方在播放时刻 t 的剩余时间 =
/// `max(0, ends_at - t)`；state 1/255 无倒计时。`ever_activated` 区分「从未使用」与
/// 「用过后已就绪」——两者 raw state 都是 1。`slot` 是 loadout 消耗品槽位（0..2，未知
/// 线码/模式能力为 None）。**state 2/3 的实际结束仍以服务器后续 state3/state1 为准**
/// （`ends_at` 是派生值，容差约一个包钟；实测 state1 可提前到达，例如复活重置）。
///
/// **Type32 flag=0 流同时携带给养注册/拆除**（state1/255，param=-1）：本函数原样透传，
/// 不给给养线码伪造 consumable slot；消费方按 `slot`/`logical_item_id` 过滤（HUD 只显示
/// loadout 槽位 0..2 且 duration>0 的条目）。
///
/// 语义红线：**wireCode 未闭合者原样保留、不猜产品身份**；已证线码通过
/// [`consumable_logical_id`] 给出稳定逻辑标识（与 Type5 消耗品槽共用同一张表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumableTransition {
    /// 包时钟（回放时钟，秒；时间轴以此为准）
    pub clock: f32,
    pub eid: u32,
    /// body[2] 原始线码（未闭合值不赋身份）
    pub wire_code: u8,
    /// 已证逻辑标识；`None` = 线码未闭合（raw 保留，不猜名）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_item_id: Option<String>,
    /// body[3] 状态（1/2/3/255；其他值原样透传）
    pub state: u8,
    /// state 的语义名（与 WotbTools canonical 名称一致）
    pub state_name: ConsumableState,
    /// state=2 有效持续时长 / state=3 有效冷却时长（秒）；state 1/255/未知 = None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_s: Option<f32>,
    /// clock + duration_s（激活结束 / 冷却结束）；state 1/255/未知 = None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<f32>,
    /// 该 (eid, wire) 在此事件之前是否已经激活过：区分「未使用」与「冷却结束已就绪」
    #[serde(default)]
    pub ever_activated: bool,
    /// loadout 消耗品槽位 0..2（未知线码/模式能力 = None；由 model 并表 loadout 后设置）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u8>,
    /// body[4..12) 原始 f64（与包时钟**不同源**——注册态恒 0、有事件时可达 10s 偏差，
    /// 精确语义未闭合 → 原样保留、不作时间轴依据，时间轴一律用 `clock`）
    pub body_clock: f64,
    /// body[12..16)：state=2 为有效持续时长、state=3 为有效冷却（其余无定义）
    pub param: f32,
}

impl ConsumableTransition {
    /// 播放时刻 t 的剩余秒数：state 2/3 为 `clamp(ends_at - t, 0, duration_s)`；
    /// state 1/255/未知 = None。t 早于事件时按完整时长返回，t 晚于结束时返回 0。
    pub fn remaining_s_at(&self, t: f32) -> Option<f32> {
        let end = self.ends_at?;
        let duration = self.duration_s?;
        Some((end - t).clamp(0.0, duration))
    }
}

/// 消耗品线码在开局 loadout 中的槽位（0..2）；给养/模式能力/未知线码 = None。
pub fn consumable_slot_of(loadout: &VehicleLoadout, wire_code: u8) -> Option<u8> {
    loadout
        .consumables
        .iter()
        .position(|i| i.wire_code == wire_code)
        .map(|i| i as u8)
}

pub fn collect_consumable_transitions(packets: &[(u32, f32, &[u8])]) -> Vec<ConsumableTransition> {
    let mut out = Vec::new();
    for (t, clock, p) in packets {
        if *t != 32 || p.len() < 25 {
            continue;
        }
        if p[4] != 0x00 {
            continue;
        } // flag=0 族（flag=1 为炮弹警告，另一解析器）
        let body_len = u32::from_le_bytes([p[5], p[6], p[7], p[8]]) as usize;
        if body_len != 16 || 9 + body_len != p.len() {
            continue;
        } // 该族恒 16B 体
        let body = &p[9..];
        let wire_code = body[2];
        let state = body[3];
        let param = f32::from_le_bytes(body[12..16].try_into().unwrap());
        let (duration_s, ends_at) = if (state == 2 || state == 3)
            && param.is_finite()
            && param >= 0.0
        {
            (Some(param), Some(*clock + param))
        } else {
            (None, None)
        };
        out.push(ConsumableTransition {
            clock: *clock,
            eid: u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
            wire_code,
            logical_item_id: consumable_logical_id(wire_code)
                .or_else(|| ability_logical_id(wire_code))
                .map(str::to_string),
            state,
            state_name: ConsumableState::from_wire_state(state),
            duration_s,
            ends_at,
            ever_activated: false,
            slot: None,
            body_clock: f64::from_le_bytes(body[4..12].try_into().unwrap()),
            param,
        });
    }
    out.sort_by(|a, b| {
        a.clock
            .partial_cmp(&b.clock)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // 第二遍：按 (eid, wire) 的时序标记「此前是否激活过」。state2 本身算已激活，
    // 其后 state3/state1/255 为 true；开局 state1 为 false（未使用）。
    let mut used: HashMap<(u32, u8), bool> = HashMap::new();
    for c in &mut out {
        let key = (c.eid, c.wire_code);
        if c.state == 2 {
            used.insert(key, true);
        }
        c.ever_activated = used.get(&key).copied().unwrap_or(false);
    }
    out
}

/// type=32 警告/命中通知收集（作者/他人路径共用）。不排序：作者路径取窗口内最早一条（取后自排），
/// 他人路径按 hash6 令牌精确配对与顺序无关——各自保持原语义。
pub(crate) fn collect_warnings32(packets: &[(u32, f32, &[u8])]) -> Vec<ArenaWarning32> {
    let mut warnings32: Vec<ArenaWarning32> = Vec::new();
    for (t, clock, p) in packets {
        if *t != 32 || p.len() < 26 {
            continue;
        }
        if p[4] != 0x01 {
            continue;
        }
        // p[5..9] = bodyLength（WotbTools PROVEN：恒 == payloadLen−9，作帧完整性断言）
        let body_len = u32::from_le_bytes([p[5], p[6], p[7], p[8]]) as usize;
        if body_len + 9 != p.len() {
            continue;
        }
        // 尾段 6B = [shell u16][来向 yaw u16][抵达 pitch u16]（原始解码恢复）：
        // off = len≥27 ? 13 : 12（27B 在 hash6 前多一个 01 字节）；yaw@+2 pitch@+4
        let off = if p.len() >= 27 { 13 } else { 12 };
        let u = |k: usize| u16::from_le_bytes([p[off + k], p[off + k + 1]]) as f32;
        let inc_yaw = (u(2) - 32768.0) / 32768.0 * std::f32::consts::PI;
        let inc_pitch = (u(4) - 32768.0) / 32768.0 * (std::f32::consts::FRAC_PI_2);
        let (hash6, seg_bytes) = if p.len() == 26 {
            ([p[12], p[13], p[14], p[15], p[16], p[17]], &p[18..26])
        } else if p.len() == 27 {
            ([p[13], p[14], p[15], p[16], p[17], p[18]], &p[19..27])
        } else {
            continue;
        };
        warnings32.push(ArenaWarning32 {
            t: *clock,
            eid: u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
            result: seg_bytes[0],
            segment: u64::from_le_bytes(seg_bytes.try_into().unwrap()),
            hash6,
            inc_yaw,
            inc_pitch,
        });
    }
    warnings32
}

/// Avatar method 0x1b 地形命中包（仅无坦克命中时广播）：shotId 配对，args(34) 布局见 TerrainImpactData。
/// 全局广播含所有玩家脱靶弹——作者/他人路径共用，args[4..8] 的
/// shell_global_id 同时是弹种兜底链第三级的数据源。gid 掩码 & 0xFFFFFF：
/// byte3（bits24-31）为噪声——不掩码时 u32 值跳出 24 位弹种表键域；
/// 但 bits16-23 承载局部 id 高位（如 IS-7 AP=0x8250a），掩码窄于 24 位会把这类 id 截断成查不中。
/// 0x1b [21..33) = 末段速度方向向量。
pub(crate) fn collect_terrain_impacts(
    packets: &[(u32, f32, &[u8])],
) -> HashMap<u32, (u32, TerrainImpactData)> {
    let mut terrain_impacts: HashMap<u32, (u32, TerrainImpactData)> = HashMap::new();
    for (_, _, p) in packets {
        if p.len() < 46 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x1b {
            continue;
        }
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        if args_len < 34 || 12 + args_len > p.len() {
            continue;
        }
        let a = &p[12..12 + args_len];
        let f = |o: usize| f32::from_le_bytes([a[o], a[o + 1], a[o + 2], a[o + 3]]);
        terrain_impacts
            .entry(u32::from_le_bytes([a[0], a[1], a[2], a[3]]))
            .or_insert((
                u32::from_le_bytes([a[4], a[5], a[6], a[7]]) & 0xFFFFFF,
                TerrainImpactData {
                    material: a[8],
                    impact_point: [f(9), f(13), f(17)],
                    terminal_dir: [f(21), f(25), f(29)],
                },
            ));
    }
    terrain_impacts
}

/// type=32 警告包抵达成角解码与校验：
/// 候选 = 受击者 eid 的警告、命中 ±3s 窗口（来袭警告覆盖未命中弹，含路过炮弹）；
/// `prefer` = method8↔type32 hash6 令牌已精确绑定的警告（命中归属的副产品）——
/// 通过同样的双重校验则直接采用，免去 ±3s 窗内多警告择优（2026-10 确定性收口）；
/// 选取 = 解码来向方位角与位置推算方位角（受击者→射手）偏差最小者；偏差 >15° 弃用；
/// |pitch|>30°（非直射抵达角）弃用。通过 = 该警告确属命中本车的炮弹，
/// pitch 即"受击者反向瞄准射手的俯仰"（渲染炮口指向射手的炮管俯角）。
pub(crate) fn decoded_target_gun_pitch(
    warnings32: &[ArenaWarning32],
    victim: u32,
    end_time: f32,
    bearing: Option<f32>,
    prefer: Option<&ArenaWarning32>,
) -> Option<(f32, f32)> {
    let bearing = bearing?;
    if let Some(w) = prefer {
        if w.eid == victim {
            let err = ((w.inc_yaw - bearing + std::f32::consts::PI).rem_euclid(
                std::f32::consts::TAU,
            ) - std::f32::consts::PI)
                .abs();
            if err <= super::shots::INC_YAW_TOL && w.inc_pitch.to_degrees().abs() <= 30.0 {
                return Some((w.inc_pitch, w.inc_yaw));
            }
        }
    }
    let mut best: Option<(f32, f32, f32)> = None; // (yaw 偏差, pitch, yaw)
    for w in warnings32 {
        if w.eid != victim {
            continue;
        }
        if w.t > end_time + 0.1 || w.t <= end_time - 3.0 {
            continue;
        }
        let err = ((w.inc_yaw - bearing + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI)
            .abs();
        if err > super::shots::INC_YAW_TOL {
            continue;
        }
        if best.as_ref().map(|(be, _, _)| err < *be).unwrap_or(true) {
            best = Some((err, w.inc_pitch, w.inc_yaw));
        }
    }
    let (_, pitch, yaw) = best?;
    if pitch.to_degrees().abs() > 30.0 {
        return None;
    }
    Some((pitch, yaw))
}

/// 血量链降幅区间推导（作者/他人路径共用）：**逐事件采样**——method1 每条 = 该次
/// 伤害后的绝对血量快照，同钟多条（同 tick 多发命中同一受害者）按**包序**即施加
/// 顺序，全部保留；相邻下降对即逐发真实伤害，归属看后一条的 source。
/// 旧"同钟异值整组丢弃"规则在双命中同 tick 时会把中间采样吞掉，使区间吞并多发
/// （实测：两发 357+341 与下一发 389 合成 1087 的假伤害）。cause=0 炮弹直击；
/// author_filter = Some(eid) 仅保留该射手造成的降幅（作者路径），None 保留全部
/// source（他人路径）。
/// initial_hp（type=5 满血锚点）：早于受害者首个 method1 采样时前插 seed，使首刀
/// 降幅可推导；seed 时钟回退 1ms 保持先序；seed 晚于首个 method1（AoI 迟到全量包）
/// 时放弃，保持原行为。
pub(crate) fn derive_dmg_losses(
    hp_events: &[HpEvent],
    author_filter: Option<u32>,
    initial_hp: &HashMap<u32, (f32, u16)>,
) -> Vec<DmgLoss> {
    let mut dmg_losses: Vec<DmgLoss> = Vec::new();
    let mut by_victim: std::collections::HashMap<u32, Vec<&HpEvent>> =
        std::collections::HashMap::new();
    for e in hp_events {
        by_victim.entry(e.victim).or_default().push(e);
    }
    for (victim, evs) in &by_victim {
        let mut samples: Vec<(f32, u16, u32, u8)> = Vec::new();
        if let Some(&(ts, hp)) = initial_hp.get(victim) {
            if evs.first().is_none_or(|e| ts <= e.clock) {
                samples.push(((ts - 1e-3).max(0.0), hp, 0, 0));
            }
        }
        for e in evs {
            samples.push((e.clock, hp_terminal_normalized(e.hp), e.source, e.cause));
        }
        for w in 1..samples.len() {
            let (_, hpp, _, _) = samples[w - 1];
            let (t_cur, hpc, srcc, causec) = samples[w];
            if hpc < hpp && causec == 0 && author_filter.is_none_or(|a| srcc == a) {
                dmg_losses.push(DmgLoss {
                    victim: *victim,
                    source: srcc,
                    t_cur,
                    dmg: (hpp - hpc) as u32,
                    hp_cur: hpc,
                });
            }
        }
    }
    dmg_losses.sort_by(|a, b| a.t_cur.partial_cmp(&b.t_cur).unwrap());
    dmg_losses
}

/// 血量降幅 → 发射的**确定性时钟配对**：method1 血量事件与 method8 命中通知共享
/// 同 tick 包时钟（逐位相等），每段降幅 = 一条 method1 相对前采样的下降，属于
/// `source` 的、命中时钟与降幅事件时钟（t_cur）**相等**的那一发；同射手同受害者
/// 同钟多发（弹夹双击同 tick，无法再细分）按发射序取未消费的第一发，每发至多
/// 认领一段。无时间窗、无就近匹配——配不上的降幅直接丢弃，该发走 dc 计数器兜底。
/// 返回 发射序号 → (dmg, hp_cur)。
pub(crate) fn assign_dmg_losses(
    shots: &[Option<(f32, u32, u32)>], // (end_time, shooter, victim)；None = 无终点/脱靶
    losses: &[DmgLoss],
) -> HashMap<usize, (u32, u16)> {
    let mut out: HashMap<usize, (u32, u16)> = HashMap::new();
    for lo in losses {
        let mut best: Option<usize> = None; // 发射序号（同钟多发取发射序最小者）
        for (si, s) in shots.iter().enumerate() {
            let Some((end_time, shooter, victim)) = *s else {
                continue;
            };
            if victim != lo.victim || shooter != lo.source {
                continue;
            }
            if (end_time - lo.t_cur).abs() > 1e-6 {
                continue;
            }
            if out.contains_key(&si) {
                continue;
            }
            if best.is_none_or(|b| si < b) {
                best = Some(si);
            }
        }
        if let Some(si) = best {
            out.insert(si, (lo.dmg, lo.hp_cur));
        }
    }
    out
}

/// type=35 tick 时间线收集（作者/他人路径共用）。
pub(crate) fn collect_tick_timeline(packets: &[(u32, f32, &[u8])]) -> Vec<(f32, u8)> {
    packets
        .iter()
        .filter(|(t, _, p)| *t == 35 && !p.is_empty())
        .map(|(_, clock, p)| (*clock, p[0]))
        .collect()
}

/// type=36 会话十分秒计数**全值**（每场至多 1 条，载荷恒 4B u32）：(clock, 全值)。
/// 与 type=35 同源——全语料 31/31 场（含 2 场全值=0 边界）低字节与该场 t35 起始值一致
/// （investigation/client-usage-vs-agent-implementation.md §2.2 P1-1）。
/// 用途：[`tick_at_anchored`] 的绝对刻度锚。
pub(crate) fn collect_session_tick(packets: &[(u32, f32, &[u8])]) -> Option<(f32, u32)> {
    packets.iter().find_map(|(t, clock, p)| {
        (*t == 36 && p.len() == 4).then(|| (*clock, u32::from_le_bytes([p[0], p[1], p[2], p[3]])))
    })
}

/// type=33（实体缓存校验，`MSG_ON_ENTITY_CACHE_TEST`）与 type=5（实体创建）的配对诊断。
///
/// 协议不变式（2026-10-13 全语料 31/31 场验证，二进制枚举名
/// `MSG_ON_ENTITY_CACHE_TEST,33`）：每条实体创建必伴随**同 eid** 的一条缓存校验记录
/// （载荷恒 `[eid u32][u32 0]`）。count 相等且 eid 集合一致 = 流完整；失配 = 截断 /
/// 损坏 / 异常版本——**fail-closed 信号，不猜成因**。
/// （investigation/client-usage-vs-agent-implementation.md §2.2 P3-2）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCachePairing {
    /// type=5 实体创建条数
    pub created: usize,
    /// type=33 缓存校验条数
    pub tested: usize,
    /// 有创建无校验的 eid（升序）
    pub created_without_test: Vec<u32>,
    /// 有校验无创建的 eid（升序）
    pub tested_without_create: Vec<u32>,
}

impl EntityCachePairing {
    /// 配对一致（count 相等且双向 eid 集合为空）。
    pub fn is_consistent(&self) -> bool {
        self.created == self.tested
            && self.created_without_test.is_empty()
            && self.tested_without_create.is_empty()
    }
}

/// 收集 type=5 / type=33 配对诊断（全库原始流导出与语料质量扫描消费）。
pub fn collect_entity_cache_pairing(packets: &[(u32, f32, &[u8])]) -> EntityCachePairing {
    let mut created: Vec<u32> = Vec::new();
    let mut tested: Vec<u32> = Vec::new();
    for (t, _, p) in packets {
        if p.len() < 4 {
            continue;
        }
        if *t == 5 {
            created.push(u32::from_le_bytes([p[0], p[1], p[2], p[3]]));
        } else if *t == 33 {
            tested.push(u32::from_le_bytes([p[0], p[1], p[2], p[3]]));
        }
    }
    let created_set: std::collections::HashSet<u32> = created.iter().copied().collect();
    let tested_set: std::collections::HashSet<u32> = tested.iter().copied().collect();
    let mut created_without_test: Vec<u32> = created_set.difference(&tested_set).copied().collect();
    let mut tested_without_create: Vec<u32> =
        tested_set.difference(&created_set).copied().collect();
    created_without_test.sort_unstable();
    tested_without_create.sort_unstable();
    EntityCachePairing {
        created: created.len(),
        tested: tested.len(),
        created_without_test,
        tested_without_create,
    }
}

/// type=35 tick 计数器插值（作者/他人路径共用）：timeline 按 clock 排序 → 二分定位首个 clock ≥ t 的相邻段线性内插；
/// u8 回绕展开（差值掩 0xFF，>128 视为回退取负）；t 早于首包按首段向后外推（与原线性实现一致），t 晚于末包取末值，样本 <2 条取末值。
pub(crate) fn tick_at(tick_timeline: &[(f32, u8)], t: f32) -> f32 {
    if tick_timeline.len() < 2 {
        return tick_timeline.first().map(|(_, v)| *v as f32).unwrap_or(0.0);
    }
    let j = tick_timeline.partition_point(|(c, _)| *c < t);
    if j == tick_timeline.len() {
        return tick_timeline.last().unwrap().1 as f32;
    }
    let i = j.max(1);
    let (t0, v0) = tick_timeline[i - 1];
    let (t1, v1) = tick_timeline[i];
    if t1 <= t0 {
        return v0 as f32;
    }
    let dv = ((v1 as i32 - v0 as i32) & 0xFF) as f32;
    let dv = if dv > 128.0 { dv - 256.0 } else { dv };
    let dt = t1 - t0;
    if dt <= 0.0 {
        return v0 as f32;
    }
    v0 as f32 + dv * (t - t0) / dt
}

/// [`tick_at`] 的锚定变体：type=36 全值（clock, value）存在时，把启发式展开的相对值
/// 整体平移到绝对会话刻度——**锚点样本精确等于全值**，其余样本随整体平移（样本间距
/// 不变；锚点之前的段内启发式误计不因此消除，协议上 tick 连续广播、该形态未观测）。
/// 无锚（缺记录/形状异常）行为与 [`tick_at`] 完全一致。平移不改变相对单调性与窗口内
/// 一致性——fire_tick 的窗口键语义不受影响。
pub(crate) fn tick_at_anchored(
    tick_timeline: &[(f32, u8)],
    session_tick: Option<(f32, u32)>,
    t: f32,
) -> f32 {
    let base = tick_at(tick_timeline, t);
    match session_tick {
        Some((anchor_clock, anchor_value)) if !tick_timeline.is_empty() => {
            base - (tick_at(tick_timeline, anchor_clock) - anchor_value as f32)
        }
        _ => base,
    }
}

/// 单实体动态装甲状态轨道（`armorsStates` = Vehicle propID 16，列式）
///
/// 行为学定案（2026-10-13 两回放交叉实证：0954/1615 Oth71 作者车 + 1615 Oth44 受击车）：
/// prop16 载荷 = `[u8 n][n × u32 LE 状态描述符]`——n = 该车 XML
/// `<extras><armorsStatesController>` 声明的状态数（无该控制器者值域为空，n=0）。
/// 描述符仅允许 bit0/bit16 两位（实测全集 {0, 1, 0x10000, 0x10001}）：**bit16 = 该状态
/// 当前激活**（恒恰一组置位）；bit0 不随切换变化（疑似 XML default 标记，未闭合不消费）。
/// 解码 = bit16 置位组的下标 = XML 状态 id。初始态经 ON_ENTITY_CREATE(type=5) 属性表
/// materialize（含"入视野前已切换"的正确快照）；此后每次 `armorMover` 使用
/// （type=32 flag=0 槽位事件）伴随恰一条 prop16 更新。
///
/// 反例存档：propID 0（1B 交替布尔，全车皆有的 6 万+ 级事件流）与装甲状态**无关**——
/// 早期误判已由交叉实证证伪（0954 作者车 prop0 翻 12 次而 armorMover 仅用 1 次；
/// 1615 受击车 prop0 停在 0 而 prop16 如实切换）。本收集器只信 prop16。
///
/// 消费方按声明文件门控：值 ∉ 该车状态集（`data/armor_states.json`）的样本丢弃。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawArmorStateTrack {
    pub eid: u32,
    pub t: Vec<f32>,
    pub state: Vec<u8>,
}

impl RawArmorStateTrack {
    /// 任一时刻的状态号：≤ t 的最近样本。**早于首样本 → `None`**——首样本通常是
    /// 入视野 create 表快照；在此之前消费方以声明文件默认态为初值（fail-closed）。
    pub fn state_at(&self, t: f32) -> Option<u8> {
        let i = self.t.partition_point(|&c| c <= t);
        if i == 0 {
            None
        } else {
            Some(self.state[i - 1])
        }
    }
}

/// prop16 载荷解码：`[u8 n][n × u32 LE 描述符]` → 唯一 bit16 置位组的下标。
/// n=0（无状态控制器）/ 长度不符 / 出现未知位 / 多位或多组置位 → None（fail-closed）。
fn decode_armors_states(v: &[u8]) -> Option<u8> {
    let n = usize::from(*v.first()?);
    if v.len() != 1 + 4 * n {
        return None;
    }
    let mut active: Option<u8> = None;
    for i in 0..n {
        let g = u32::from_le_bytes(v[1 + 4 * i..5 + 4 * i].try_into().ok()?);
        if g & !0x0001_0001 != 0 {
            return None; // 未知位：形状未闭合，不猜
        }
        if g & 0x0001_0000 != 0 {
            if active.is_some() {
                return None; // 多组置位非已知形状
            }
            active = Some(i as u8);
        }
    }
    active
}

/// ON_ENTITY_CREATE(type=5) 属性表的 prop16 条目解析：锚定签名
/// `10 <n∈1..=4> <n×4B 描述符> 11`，任何一步不符或整体解码失败 → None。
/// `10 00`（n=0 空表，静态车）不产生快照点。
fn parse_create_armors_states(p: &[u8]) -> Option<u8> {
    for i in 0..p.len() {
        if p[i] != 0x10 {
            continue;
        }
        let n = usize::from(*p.get(i + 1)?);
        if !(1..=4).contains(&n) {
            continue;
        }
        let vlen = 1 + 4 * n;
        if p.get(i + 1 + vlen) != Some(&0x11) {
            continue;
        }
        if let Some(state) = decode_armors_states(&p[i + 1..i + 1 + vlen]) {
            return Some(state);
        }
    }
    None
}

fn record_armor_state_point(
    tracks: &mut HashMap<u32, RawArmorStateTrack>,
    order: &mut Vec<u32>,
    eid: u32,
    clock: f32,
    state: u8,
) {
    let tr = tracks.entry(eid).or_insert_with(|| {
        order.push(eid);
        RawArmorStateTrack {
            eid,
            t: Vec::new(),
            state: Vec::new(),
        }
    });
    tr.t.push(clock);
    tr.state.push(state);
}

/// 逐车动态装甲状态轨道（按 eid 升序；列式，wire 序 = 时钟序）。
///
/// 数据源：type=5 create 表 materialize 快照 + type=7 propID 16 更新。
/// **全零轨道（全程默认态）丢弃**——与消费方"缺轨道 = 默认态"语义等价，
/// 不给每辆静态车发单点噪声轨道。
pub fn collect_armor_state_tracks(packets: &[(u32, f32, &[u8])]) -> Vec<RawArmorStateTrack> {
    let mut tracks: HashMap<u32, RawArmorStateTrack> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for (ptype, clock, p) in packets {
        match *ptype {
            5 if p.len() > 40 => {
                if let Some(state) = parse_create_armors_states(p) {
                    let eid = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                    record_armor_state_point(&mut tracks, &mut order, eid, *clock, state);
                }
            }
            7 if p.len() >= 13 && u32::from_le_bytes([p[4], p[5], p[6], p[7]]) == 16 => {
                let vlen = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
                if vlen == 0 || 12 + vlen > p.len() {
                    continue; // 截断帧：无法读出完整载荷，丢弃（形状未闭合不猜）
                }
                if let Some(state) = decode_armors_states(&p[12..12 + vlen]) {
                    let eid = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                    record_armor_state_point(&mut tracks, &mut order, eid, *clock, state);
                }
            }
            _ => {}
        }
    }
    order.sort_unstable();
    order
        .into_iter()
        .filter_map(|eid| tracks.remove(&eid))
        .filter(|tr| tr.state.iter().any(|&s| s != 0))
        .collect()
}

#[cfg(test)]
mod tick_anchor_tests {
    use super::*;

    fn mk35(clock: f32, v: u8) -> (u32, f32, Vec<u8>) {
        (35, clock, vec![v])
    }
    fn mk36(clock: f32, v: u32) -> (u32, f32, Vec<u8>) {
        (36, clock, v.to_le_bytes().to_vec())
    }

    /// type=36 收集：恰取唯一一条 4B 记录；缺失/形状异常 → None。
    #[test]
    fn collects_session_tick_once() {
        let packets: Vec<(u32, f32, Vec<u8>)> =
            vec![mk35(0.1, 244), mk36(0.2, 0x0582F4), mk35(0.2, 245)];
        let refs: Vec<(u32, f32, &[u8])> = packets
            .iter()
            .map(|(t, c, p)| (*t, *c, p.as_slice()))
            .collect();
        assert_eq!(collect_session_tick(&refs), Some((0.2, 0x0582F4)));

        let no36: Vec<(u32, f32, Vec<u8>)> = vec![mk35(0.1, 5), mk35(0.2, 6)];
        let refs: Vec<(u32, f32, &[u8])> = no36
            .iter()
            .map(|(t, c, p)| (*t, *c, p.as_slice()))
            .collect();
        assert_eq!(collect_session_tick(&refs), None);

        let bad_shape: Vec<(u32, f32, Vec<u8>)> = vec![(36, 1.0, vec![1, 2])]; // 2B ≠ 4B
        let refs: Vec<(u32, f32, &[u8])> = bad_shape
            .iter()
            .map(|(t, c, p)| (*t, *c, p.as_slice()))
            .collect();
        assert_eq!(collect_session_tick(&refs), None);
    }

    /// 无锚（缺 type=36）时 tick_at_anchored 与 tick_at 完全一致。
    #[test]
    fn no_anchor_matches_tick_at() {
        let tl = vec![(0.0f32, 0u8), (0.1, 1), (0.2, 2)];
        for t in [0.05f32, 0.1, 0.15, 0.2] {
            assert_eq!(tick_at_anchored(&tl, None, t), tick_at(&tl, t));
        }
    }

    /// 锚定修正：样本间隔 >128 tick（12.8s+ 丢包）时启发式把 +200 误读为 −56——
    /// 锚定后锚点样本精确等于 type=36 全值（绝对刻度正确），整体平移保持样本间距
    /// （单锚无法修正锚点之前的段内误计；协议上 tick 连续广播，>12.8s 空档未观测）。
    #[test]
    fn anchor_corrects_heuristic_wrap_miscount() {
        // 0.0s 计数 0 → 20.0s 计数 200（丢 199 个样本：启发式 200 > 128 误判为 −56）
        let tl = vec![(0.0f32, 0u8), (20.0, 200)];
        let session = Some((20.0f32, 200u32));
        // 启发式：dv = 200 & 0xFF = 200 > 128 → 视为 −56
        assert_eq!(tick_at(&tl, 20.0), -56.0);
        // 锚定后：锚点处精确 = 200（绝对刻度正确）；整体平移保持相对间距
        assert_eq!(tick_at_anchored(&tl, session, 20.0), 200.0);
        assert_eq!(tick_at_anchored(&tl, session, 0.0), 256.0);
    }

    /// 锚点不落首样本：锚定值 = 锚点样本精确对齐全值（真实语料形态：t36 常在开局附近）。
    #[test]
    fn anchor_shifts_uniformly() {
        // 从 250 起始（会话中段）：250, 251, 252（+1 小步，启发式无歧义）
        let tl = vec![(0.0f32, 250u8), (0.1, 251), (0.2, 252)];
        // 全值取低字节 == 锚点样本 u8（251）的数，符合协议不变式（361211 & 0xFF == 251）
        let session = Some((0.1f32, 361_211u32));
        // 无锚展开：0.1s 处 = 251；锚定后整体平移到绝对会话刻度
        let base = tick_at(&tl, 0.1);
        let anchored = tick_at_anchored(&tl, session, 0.1);
        assert_eq!(base, 251.0);
        assert_eq!(anchored, 361_211.0, "锚点样本精确等于全值");
        assert_eq!(
            tick_at_anchored(&tl, session, 0.0),
            361_210.0,
            "整体平移，不改变样本间距"
        );
    }
}

#[cfg(test)]
mod entity_cache_pairing_tests {
    use super::*;

    fn mk(t: u32, eid: u32) -> (u32, f32, Vec<u8>) {
        (t, 1.0, eid.to_le_bytes().to_vec())
    }
    fn refs(p: &[(u32, f32, Vec<u8>)]) -> Vec<(u32, f32, &[u8])> {
        p.iter().map(|(t, c, b)| (*t, *c, b.as_slice())).collect()
    }

    /// 配对一致：count 相等且 eid 集合一致（真实语料 31/31 场形态）。
    #[test]
    fn matched_creates_and_tests_are_consistent() {
        let packets = vec![
            mk(5, 100),
            mk(33, 100),
            mk(10, 100), // 无关类型不参与
            mk(5, 200),
            mk(33, 200),
        ];
        let d = collect_entity_cache_pairing(&refs(&packets));
        assert_eq!((d.created, d.tested), (2, 2));
        assert!(d.is_consistent());
    }

    /// 有校验无创建（流截断/损坏形态）→ 失配，多出的 eid 原样列出。
    #[test]
    fn test_without_create_is_inconsistent() {
        let packets = vec![mk(5, 100), mk(33, 100), mk(33, 300)];
        let d = collect_entity_cache_pairing(&refs(&packets));
        assert_eq!((d.created, d.tested), (1, 2));
        assert!(!d.is_consistent());
        assert!(d.created_without_test.is_empty());
        assert_eq!(d.tested_without_create, vec![300]);
    }

    /// 双向失配：eid 集合不相交 → 两侧各自列出（升序）。
    #[test]
    fn disjoint_eid_sets_report_both_sides() {
        let packets = vec![mk(5, 30), mk(5, 10), mk(33, 20), mk(33, 40)];
        let d = collect_entity_cache_pairing(&refs(&packets));
        assert_eq!((d.created, d.tested), (2, 2));
        assert!(!d.is_consistent(), "count 相等但集合不同也算失配");
        assert_eq!(d.created_without_test, vec![10, 30]);
        assert_eq!(d.tested_without_create, vec![20, 40]);
    }

    /// 短载荷（<4B）不参与配对（防御：畸形包不计入任何一侧）。
    #[test]
    fn malformed_short_payloads_are_ignored() {
        let packets = vec![(5, 1.0, vec![1, 2]), (33, 1.0, vec![]), mk(5, 7), mk(33, 7)];
        let d = collect_entity_cache_pairing(&refs(&packets));
        assert_eq!((d.created, d.tested), (1, 1));
        assert!(d.is_consistent());
    }
}

#[cfg(test)]
mod consumable_tests {
    use super::*;

    /// 合成 Type32 flag=0 帧：[eid u32][flag=0][bodyLen u32=16][body 16B]
    fn mk32(eid: u32, wire: u8, state: u8, body_clock: f64, param: f32) -> Vec<u8> {
        let mut body = vec![0u8; 16];
        body[2] = wire;
        body[3] = state;
        body[4..12].copy_from_slice(&body_clock.to_le_bytes());
        body[12..16].copy_from_slice(&param.to_le_bytes());
        let mut p = vec![0u8; 9];
        p[0..4].copy_from_slice(&eid.to_le_bytes());
        p[4] = 0x00; // flag=0（消耗品族）
        p[5..9].copy_from_slice(&16u32.to_le_bytes());
        p.extend_from_slice(&body);
        p
    }

    #[test]
    fn parses_flag0_consumable_family() {
        let f0 = mk32(0x21, 0x08, 1, 1.0, 0.0); // 开局注册/可用
        let f1 = mk32(0x21, 0x08, 2, 42.5, 6.0); // 激活（持续 6s）
        let f2 = mk32(0x21, 0x08, 3, 48.5, 90.0); // 冷却
        let packets: Vec<(u32, f32, &[u8])> =
            vec![(32, 48.5, &f2), (32, 1.0, &f0), (32, 42.5, &f1)];
        let t = collect_consumable_transitions(&packets);
        assert_eq!(t.len(), 3);
        assert_eq!(
            (t[0].clock, t[0].eid, t[0].wire_code, t[0].state),
            (1.0, 0x21, 0x08, 1)
        );
        assert_eq!(t[0].state_name, ConsumableState::Initialized);
        assert_eq!((t[0].duration_s, t[0].ends_at), (None, None));
        assert!(!t[0].ever_activated, "开局 state1 = 未使用");
        assert_eq!(
            t[0].logical_item_id.as_deref(),
            Some("AUTOMATIC_FIRE_EXTINGUISHER")
        );
        assert_eq!(
            (t[1].clock, t[1].state, t[1].state_name),
            (42.5, 2, ConsumableState::Activated)
        );
        assert!((t[1].body_clock - 42.5).abs() < 1e-9);
        assert!((t[1].param - 6.0).abs() < 1e-6);
        assert_eq!(t[1].duration_s, Some(6.0));
        assert_eq!(t[1].ends_at, Some(48.5));
        assert!(t[1].ever_activated);
        assert_eq!(
            t[1].remaining_s_at(45.0),
            Some(3.5),
            "激活剩余 = ends_at - t"
        );
        assert_eq!(t[1].remaining_s_at(40.0), Some(6.0), "t 早于事件按完整时长");
        assert_eq!(t[1].remaining_s_at(100.0), Some(0.0), "t 晚于结束返回 0");
        assert_eq!(
            (t[2].clock, t[2].state, t[2].state_name),
            (48.5, 3, ConsumableState::ActiveEndedOrCooldown)
        );
        assert_eq!(t[2].duration_s, Some(90.0));
        assert_eq!(t[2].ends_at, Some(138.5));
        assert_eq!(t[2].remaining_s_at(100.0), Some(38.5));
        assert!(t[2].ever_activated);
    }

    #[test]
    fn maps_shell_reload_boost_wire_0x3f() {
        // 0x3F：M-VI-Yoh 样本 slot0 激活 20s、冷却 55.555（65 × 0.8547）——
        // 客户端目录唯一 dur=20/cd=65 的条目是 newAutoloader（Shell Reload Boost）。
        let act = mk32(0x21, 0x3F, 2, 98.0, 20.0);
        let packets: Vec<(u32, f32, &[u8])> = vec![(32, 98.0, &act)];
        let t = collect_consumable_transitions(&packets);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].logical_item_id.as_deref(), Some("SHELL_RELOAD_BOOST"));
        assert_eq!(t[0].state_name, ConsumableState::Activated);
        assert_eq!(t[0].duration_s, Some(20.0));
        assert_eq!(t[0].ends_at, Some(118.0));
    }

    #[test]
    fn keeps_provision_registration_without_consumable_slot_identity() {
        // Type32 flag=0 流同时携带给养注册（state1/param=-1）；原样透传，但 logical/slot
        // 都不伪造——WotbTools 运行时按 slot 过滤，HUD 不会把给养当消耗品。
        let prov = mk32(0x21, 0x1D, 1, 1.0, -1.0);
        let cons = mk32(0x21, 0x09, 1, 1.0, 0.0);
        let packets: Vec<(u32, f32, &[u8])> = vec![(32, 1.0, &prov), (32, 1.0, &cons)];
        let t = collect_consumable_transitions(&packets);
        assert_eq!(t.len(), 2);
        let p = t.iter().find(|c| c.wire_code == 0x1D).unwrap();
        assert_eq!(p.state_name, ConsumableState::Initialized);
        assert_eq!(p.logical_item_id, None);
        assert_eq!(p.slot, None);
        assert_eq!((p.duration_s, p.ends_at), (None, None));
        let c = t.iter().find(|c| c.wire_code == 0x09).unwrap();
        assert_eq!(c.logical_item_id.as_deref(), Some("ADRENALINE"));
    }

    #[test]
    fn instant_activation_has_zero_active_duration() {
        // 瞬发消耗品：state2/state3 同钟，param=0 的激活段长度为 0，冷却从同钟开始
        let f1 = mk32(0x21, 0x0D, 2, 10.0, 0.0);
        let f2 = mk32(0x21, 0x0D, 3, 10.0, 60.0);
        let f3 = mk32(0x21, 0x0D, 1, 70.0, 0.0);
        let packets: Vec<(u32, f32, &[u8])> = vec![(32, 10.0, &f1), (32, 10.0, &f2), (32, 70.0, &f3)];
        let t = collect_consumable_transitions(&packets);
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].duration_s, t[0].ends_at), (Some(0.0), Some(10.0)));
        assert_eq!(t[0].remaining_s_at(10.0), Some(0.0));
        assert_eq!((t[1].duration_s, t[1].ends_at), (Some(60.0), Some(70.0)));
        assert_eq!(t[1].remaining_s_at(25.0), Some(45.0));
        assert_eq!((t[2].state_name, t[2].ends_at), (ConsumableState::Initialized, None));
        assert!(t.iter().all(|c| c.ever_activated), "state2 起均为已使用");
    }

    #[test]
    fn teardown_and_unknown_state_have_no_countdown() {
        let f1 = mk32(0x21, 0x0B, 255, 10.0, 0.0);
        let f2 = mk32(0x21, 0x0B, 9, 11.0, 5.0);
        let packets: Vec<(u32, f32, &[u8])> = vec![(32, 10.0, &f1), (32, 11.0, &f2)];
        let t = collect_consumable_transitions(&packets);
        assert_eq!(t[0].state_name, ConsumableState::Teardown);
        assert_eq!((t[0].duration_s, t[0].ends_at, t[0].remaining_s_at(12.0)), (None, None, None));
        assert_eq!(t[1].state_name, ConsumableState::Unknown);
        assert_eq!((t[1].duration_s, t[1].ends_at), (None, None));
    }

    #[test]
    fn ignores_flag1_warning_family_and_bad_shapes() {
        // flag=1（炮弹警告族，26B）不得进消耗品集合；bodyLen 非 16 亦拒
        let mut warn = vec![0u8; 26];
        warn[4] = 0x01;
        warn[5..9].copy_from_slice(&17u32.to_le_bytes()); // 26-9=17 ≠ 16
        let mut wrong_len = mk32(0x22, 0x08, 2, 1.0, 1.0);
        wrong_len[5..9].copy_from_slice(&15u32.to_le_bytes()); // 声明 15 与实际 16 不符
        let packets: Vec<(u32, f32, &[u8])> = vec![(32, 1.0, &warn), (32, 2.0, &wrong_len)];
        assert!(collect_consumable_transitions(&packets).is_empty());
    }
}

#[cfg(test)]
mod loadout_tests {
    use super::*;
    use std::borrow::Cow;

    /// 合成 Type5 loadout 块：`0A <k>` + k×14B 描述符 + `0B <n>` + nB 配件串 +
    /// `0C 00 0D` 帧尾（真实报文串后恒有该三字节，见 scan_loadout 护栏）。
    fn mk_type5_loadout_with(k: u8, eq: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 60];
        p[0..4].copy_from_slice(&0x31u32.to_le_bytes());
        p[51..53].copy_from_slice(&1000u16.to_le_bytes());
        p.extend_from_slice(&[0x0A, k]);
        for i in 0..k {
            let mut it = [0u8; 14];
            it[0] = 0x80 + i; // 可辨识的每槽字节
            p.extend_from_slice(&it);
        }
        p.extend_from_slice(&[0x0B, eq.len() as u8]);
        p.extend_from_slice(eq); // 装备串（数值 ID 域 100..=123）
        p.extend_from_slice(&[0x0C, 0x00, 0x0D, 0x00, 0x00, 0x00]);
        p
    }

    /// 合成一条 14B item 描述符：`[wire, state, 12B payload]`；`provision=true` 时
    /// 载荷尾写静态标记 `f32 -1.0`（`00 00 80 BF`）。
    fn descriptor(wire: u8, provision: bool) -> [u8; 14] {
        let mut it = [0u8; 14];
        it[0] = wire;
        it[1] = 1;
        if provision {
            it[10..14].copy_from_slice(&[0x00, 0x00, 0x80, 0xBF]);
        }
        it
    }

    /// 合成带指定描述符块的 Type5 loadout 载荷。
    fn mk_type5_items(items: &[[u8; 14]], eq: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 60];
        p[0..4].copy_from_slice(&0x31u32.to_le_bytes());
        p[51..53].copy_from_slice(&1000u16.to_le_bytes());
        p.extend_from_slice(&[0x0A, items.len() as u8]);
        for it in items {
            p.extend_from_slice(it);
        }
        p.extend_from_slice(&[0x0B, eq.len() as u8]);
        p.extend_from_slice(eq);
        p.extend_from_slice(&[0x0C, 0x00, 0x0D, 0x00, 0x00, 0x00]);
        p
    }

    /// 合成满配（9 件）Type5 loadout 块。
    fn mk_type5_loadout() -> Vec<u8> {
        mk_type5_items(
            &[
                descriptor(0x0B, false),
                descriptor(0x0D, false),
                descriptor(0x09, false),
                descriptor(0x0E, true),
                descriptor(0x1D, true),
                descriptor(0x15, true),
            ],
            &[100, 101, 102, 103, 104, 105, 106, 107, 108],
        )
    }

    /// 只有装备串与帧尾、没有 `0A <k>` item 描述符块的 Type5 载荷。
    fn mk_type5_equipment_only(eq: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 60];
        p[0..4].copy_from_slice(&0x31u32.to_le_bytes());
        p[51..53].copy_from_slice(&1000u16.to_le_bytes());
        p.extend_from_slice(&[0x0B, eq.len() as u8]);
        p.extend_from_slice(eq);
        p.extend_from_slice(&[0x0C, 0x00, 0x0D, 0x00, 0x00, 0x00]);
        p
    }

    #[test]
    fn collects_equipment_and_six_raw_descriptors() {
        let payload = mk_type5_loadout();
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let l = collect_vehicle_loadout(&packets);
        let v = l.get(&0x31).expect("实体 loadout 应被采集");
        assert_eq!(v.items.len(), 6, "标准 3 消耗品 + 3 给养 = 6 条描述符");
        assert_eq!(v.equipment, [100, 101, 102, 103, 104, 105, 106, 107, 108]);
        assert_eq!(v.equipment_count, 9);
        assert_eq!(v.equipment_ids, vec![100, 101, 102, 103, 104, 105, 106, 107, 108]);
        assert_eq!(v.items[0][0], 0x0B, "描述符原样保留（不解析内部）");
        assert_eq!(v.items[5][0], 0x15);
        assert_eq!(v.consumables.len(), 3);
        assert_eq!(v.provisions.len(), 3);
        assert!(v.abilities.is_empty());
        assert_eq!(
            v.consumables
                .iter()
                .map(|i| i.logical_id.as_deref())
                .collect::<Vec<_>>(),
            vec![
                Some("MULTI_PURPOSE_RESTORATION_PACK"),
                Some("REPAIR_KIT"),
                Some("ADRENALINE")
            ]
        );
        assert_eq!(
            v.provisions
                .iter()
                .map(|i| i.logical_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("LARGE_FOOD"), Some("IMPROVED_FUEL"), Some("SMALL_FOOD")]
        );
        // 兼容入口取同一 9B 串
        assert_eq!(
            collect_vehicle_equipment(&packets)
                .get(&0x31)
                .copied()
                .unwrap(),
            v.equipment
        );
    }

    /// item 描述符数量也是变长的：3 条（少带消耗品/给养）不能被旧 `k>=6` 守卫丢弃。
    /// 实测 Takuchan（J39 样本）Type5：`0A 03` + `0B 05 67 6D 7A 6B 6E`，
    /// 103=校准弹、110=强化装甲；旧实现导致射击复现无法自动勾选校准弹。
    #[test]
    fn collects_three_descriptor_loadout_with_calibrated_shells() {
        let payload = mk_type5_loadout_with(3, &[103, 109, 122, 107, 110]);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets)
            .get(&0x31)
            .cloned()
            .expect("3 条描述符的 loadout 应被采集");
        assert_eq!(v.items.len(), 3);
        assert_eq!(v.equipment, [103, 109, 122, 107, 110, 0, 0, 0, 0]);
        let eq = VehicleEquipment::from_ids(&v.equipment);
        assert!(eq.calibrated_shells, "103 = 校准弹应判为已装备");
        assert!(eq.enhanced_armor, "110 = 强化装甲应判为已装备");
        assert!(eq.improved_pitch_limits, "122 = 改进型垂直稳定器应判为已装备");
        let json = serde_json::to_value(&eq).unwrap();
        assert_eq!(json["improved_pitch_limits"], true);
    }

    /// 不含 122 时标志为 false，且 false 不进入 JSON（旧消费方字节不变）。
    #[test]
    fn equipment_omits_improved_pitch_limits_when_not_equipped() {
        let payload = mk_type5_loadout_with(3, &[103, 109, 107, 110]);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let eq = VehicleEquipment::from_ids(collect_vehicle_equipment(&packets).get(&0x31).unwrap());
        assert!(!eq.improved_pitch_limits);
        let json = serde_json::to_value(&eq).unwrap();
        assert!(
            json.get("improved_pitch_limits").is_none(),
            "false 时序列化省略，旧输出不变"
        );
    }

    /// 有效极限只按实体自己的配件串叠加；无 122 借用原值，有 122 返回 +4°仰/+3°俯。
    #[test]
    fn effective_pitch_limits_applies_only_equipment_122() {
        let base = GunPitchRange {
            dep: 6.0,
            ele: 18.0,
            front: None,
            back: None,
            transition: None,
        };
        let none = effective_pitch_limits(&base, None);
        assert!(matches!(none, Cow::Borrowed(_)));
        assert_eq!(none.dep, 6.0);
        assert_eq!(none.ele, 18.0);

        let other = [103u8, 110, 0, 0, 0, 0, 0, 0, 0];
        let other = effective_pitch_limits(&base, Some(&other));
        assert!(matches!(other, Cow::Borrowed(_)), "其他配件不得改变俯仰");

        let with_122 = [103u8, 122, 0, 0, 0, 0, 0, 0, 0];
        let with_122 = effective_pitch_limits(&base, Some(&with_122));
        assert!(matches!(with_122, Cow::Owned(_)));
        assert!((with_122.dep - 9.0).abs() < 1e-6);
        assert!((with_122.ele - 22.0).abs() < 1e-6);
    }

    /// XM551 #30 射手（Player_9617219477）的实测串：`0A 04` + 4×14B + `0B 09` +
    /// `[103,109,115,107,111,116,105,112,101]` + `0C 00 0D`。4 条描述符同样必须被采集。
    #[test]
    fn collects_four_descriptor_loadout_from_xm551_sample() {
        let eq_ids = [103, 109, 115, 107, 111, 116, 105, 112, 101];
        let payload = mk_type5_loadout_with(4, &eq_ids);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets)
            .get(&0x31)
            .cloned()
            .expect("4 条描述符的 loadout 应被采集");
        assert_eq!(v.items.len(), 4);
        assert_eq!(v.equipment, eq_ids);
        assert!(VehicleEquipment::from_ids(&v.equipment).calibrated_shells);
    }

    /// 装备检测不再依赖 `0A KK` item 描述符块：载荷里只有 `0B` 装备串 + 帧尾时，
    /// 仍必须识别 103 校准弹（items 为空），否则解析器结构一变就会整条丢装备。
    #[test]
    fn collects_equipment_without_item_descriptor_block() {
        let eq_ids = [103, 109, 115, 107, 111, 116, 105, 112, 101];
        let payload = mk_type5_equipment_only(&eq_ids);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets)
            .get(&0x31)
            .cloned()
            .expect("装备串 + 帧尾应足够采集 loadout");
        assert!(v.items.is_empty(), "缺失描述符块时 items 为空");
        assert_eq!(v.equipment, eq_ids);
        assert!(VehicleEquipment::from_ids(&v.equipment).calibrated_shells);
    }

    /// 变长配件串（空槽省略）必须被采集：此前硬编码 `0B 09` 把不满 9 件的玩家整条
    /// loadout 丢弃 → 查看器"校准弹"判成未装备（60TP 报障，2026-10-10）。
    #[test]
    fn collects_variable_length_equipment_string() {
        // 2 件：103 校准炮弹 + 108 改进型模块（60TP 报障样本的实测串）
        let payload = mk_type5_loadout_with(6, &[103, 108]);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_equipment(&packets);
        assert_eq!(
            v.get(&0x31),
            Some(&[103, 108, 0, 0, 0, 0, 0, 0, 0]),
            "变长串按原序填入、空槽补 0"
        );
        let eq = VehicleEquipment::from_ids(v.get(&0x31).unwrap());
        assert!(eq.calibrated_shells, "103 = 校准弹应判为已装备");
        assert!(!eq.enhanced_armor);
        assert_eq!(eq.raw.len(), 9);
        assert_eq!(eq.equipment_count, 2);
        assert_eq!(eq.equipment_ids, vec![103, 108]);
        let l = collect_vehicle_loadout(&packets);
        assert_eq!(l.get(&0x31).unwrap().equipment_count, 2);
        assert_eq!(l.get(&0x31).unwrap().equipment_ids, vec![103, 108]);

        // 5 件、不含 103/110 → 两个标志均 false（短串不是"全 true"的兜底）
        let payload = mk_type5_loadout_with(6, &[100, 109, 114, 104, 111]);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let eq = VehicleEquipment::from_ids(collect_vehicle_equipment(&packets).get(&0x31).unwrap());
        assert!(!eq.calibrated_shells && !eq.enhanced_armor);

        // 帧尾护栏：串后不是 `0C 00 0D`（改尾字节 / 直接截断）的伪造块不得被采纳
        let mut bad = mk_type5_loadout_with(6, &[103, 108]);
        let len = bad.len();
        bad[len - 4] = 0x0E; // 0C 00 0D → 0C 00 0E
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &bad)];
        assert!(
            collect_vehicle_equipment(&packets).is_empty(),
            "帧尾不符 → 拒收"
        );
        let mut cut = mk_type5_loadout_with(6, &[103, 108]);
        let len = cut.len();
        cut.truncate(len - 6); // 载荷止于配件串
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &cut)];
        assert!(
            collect_vehicle_equipment(&packets).is_empty(),
            "帧尾缺失 → 拒收"
        );
    }

    /// GravityMode 的 7 条真实形态：3 消耗品 + 1 条模式能力（0x67，无给养静态尾）+
    /// 3 条给养。能力必须与给养分流，不能当成第 4 个给养（否则给养数 >3）。
    ///
    /// 真实样本（J39 eid=0x00166626，11.20.0）：`0A 07` + `0D/0B/09/67/0F/16/1D`
    /// 各 14B + `0B 09 67 6D 72 68 6F 75 69 71 65` + `0C 00 0D`。
    #[test]
    fn separates_mode_ability_from_provisions() {
        let items = [
            descriptor(0x0B, false),
            descriptor(0x0D, false),
            descriptor(0x09, false),
            descriptor(0x67, false), // GravityMode 额外能力
            descriptor(0x0E, true),
            descriptor(0x1D, true),
            descriptor(0x15, true),
        ];
        let eq = [103, 109, 114, 104, 111, 117, 105, 113, 101];
        let payload = mk_type5_items(&items, &eq);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets)
            .get(&0x31)
            .cloned()
            .expect("7 条描述符的 loadout 应被采集");
        assert_eq!(v.items.len(), 7);
        assert_eq!(v.consumables.len(), 3);
        assert_eq!(v.abilities.len(), 1);
        assert_eq!(v.abilities[0].wire_code, 0x67);
        assert_eq!(v.abilities[0].kind, LoadoutItemKind::Ability);
        assert_eq!(
            v.abilities[0].logical_id.as_deref(),
            Some("GRAVITY_JUMP"),
            "0x67 = gravityJump/Thruster（坦克百科 lunar 能力）"
        );
        assert_eq!(v.provisions.len(), 3);
        assert_eq!(
            v.provisions
                .iter()
                .map(|i| i.logical_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("LARGE_FOOD"), Some("IMPROVED_FUEL"), Some("SMALL_FOOD")]
        );
        assert!(v.consumables.iter().all(|i| i.kind == LoadoutItemKind::Consumable));
    }

    /// 11.20/11.21 补给块扩展：同一连续大/小补给块 + 其他系 + 防破片内衬 + 欧系/瑞典
    /// 食物对（0x46..0x49，见下方受控实验注释）与波兰 0x6D/0x6E。
    #[test]
    fn maps_1121_food_block_and_spall_liner() {
        for (wire, logical) in [
            (0x13, "LARGE_FOOD"),
            (0x14, "LARGE_FOOD"),
            (0x15, "SMALL_FOOD"),
            (0x1A, "SMALL_FOOD"),
            (0x1B, "SMALL_FOOD"),
            (0x2D, "LARGE_FOOD"),
            (0x2E, "SMALL_FOOD"),
            (0x32, "SPALL_LINER"),
        ] {
            assert_eq!(provision_logical_id(wire), Some(logical), "wire {wire:#04x}");
        }
        // 欧系/瑞典食物对：0x46=欧系大、0x47=欧系小（WotbTools 11.19 生产映射）；
        // 瑞典对顺序相反——0x48=瑞典小(脆饼)、0x49=瑞典大(三文鱼)。
        // **受控实验钉死（2026-10-31）**：Kranvagn 亲装大补给 → 线码 0x49。
        // v0.4.18 曾按「103..110 连续 −33」把 0x48/0x49 反标，已被证伪撤销。
        for (wire, logical) in [
            (0x46, "LARGE_FOOD"),  // 103 Cheesehead（意大利车观测）
            (0x47, "SMALL_FOOD"),  // 104 Cinnamon Roll
            (0x48, "SMALL_FOOD"),  // 106 Crispbread（WotbTools 11.19 结算体观测）
            (0x49, "LARGE_FOOD"),  // 105 Grilled Salmon（受控实验 + 11.19）
        ] {
            assert_eq!(provision_logical_id(wire), Some(logical), "wire {wire:#04x}");
        }
        // 捷克：0x4A = 红烩牛肉/Goulash(107) 大——受控实验钉死（2026-10-31 Vz.55 亲装
        // 大口粮 → 0x4A）；0x4B = 油炸奶酪/Fried Cheese(108) 小——0x4A 钉死后按成对 +
        // 24 槽 24 件计数闭合消去。曾把 0x4C/0x4D 当捷克（−33 外推）——从未观测，不猜。
        assert_eq!(provision_logical_id(0x4A), Some("LARGE_FOOD"));
        assert_eq!(provision_family(0x4A), Some("czech"));
        assert_eq!(provision_logical_id(0x4B), Some("SMALL_FOOD"));
        assert_eq!(provision_family(0x4B), Some("czech"));
        // 曾把 0x4C/0x4D 当波兰（−33 结构外推、从未自然观测）——已撤销。
        assert_eq!(provision_logical_id(0x4C), None, "0x4C 从未观测，不猜名");
        assert_eq!(provision_logical_id(0x4D), None, "0x4D 从未观测，不猜名");
        // 波兰：0x6D=茹尔汤(109)——受控实验钉死（2026-10-31 60TP 亲装茹尔汤 → 0x6D，
        // 与 WotbTools 11.18/11.20 自然观测一致）；0x6E 与 0x6D 成对 → Faworki(110) 小。
        assert_eq!(provision_logical_id(0x6D), Some("LARGE_FOOD"));
        assert_eq!(provision_logical_id(0x6E), Some("SMALL_FOOD"));
        assert_eq!(provision_family(0x6D), Some("polish"));
        assert_eq!(provision_family(0x6E), Some("polish"));
        // 系别族：七国块逐码 + 欧系/瑞典/其他系 + 通用给养无族。
        for (wire, family) in [
            (0x0E, "germany"),
            (0x0F, "usa"),
            (0x10, "ussr"),
            (0x11, "uk"),
            (0x12, "japan"),
            (0x13, "china"),
            (0x14, "france"),
            (0x15, "germany"),
            (0x16, "usa"),
            (0x17, "ussr"),
            (0x18, "uk"),
            (0x19, "japan"),
            (0x1A, "china"),
            (0x1B, "france"),
            (0x2D, "other"),
            (0x2E, "other"),
            (0x46, "european"),
            (0x47, "european"),
            (0x48, "swedish"),
            (0x49, "swedish"),
        ] {
            assert_eq!(provision_family(wire), Some(family), "wire {wire:#04x}");
        }
        assert_eq!(provision_family(0x1D), None, "燃料/防护包等通用给养无族");
        for (wire, logical) in [
            (0x0E, "LARGE_FOOD"),
            (0x16, "SMALL_FOOD"),
            (0x1C, "STANDARD_FUEL"),
            (0x1D, "IMPROVED_FUEL"),
            (0x1E, "PROTECTIVE_KIT"),
            (0x44, "SANDBAG_ARMOR"),
            (0x45, "ENHANCED_SANDBAG_ARMOR"),
            (0x6A, "GEAR_OIL"),
            (0x6B, "IMPROVED_GEAR_OIL"),
            (0x6C, "IMPROVED_GUNPOWDER"),
        ] {
            assert_eq!(provision_logical_id(wire), Some(logical), "wire {wire:#04x}");
        }
        assert_eq!(consumable_logical_id(0x0B), Some("MULTI_PURPOSE_RESTORATION_PACK"));
        assert_eq!(
            consumable_logical_id(0xA9),
            Some("DYNAMIC_ARMOUR_SYSTEM"),
            "0xA9 = armorMover（目录 id 73；Titan Strife）"
        );
        assert_eq!(consumable_logical_id(0xBD), Some("REDUCED_ENGINE_POWER_BOOST"));
        assert_eq!(ability_logical_id(0x67), Some("GRAVITY_JUMP"));
        assert_eq!(ability_logical_id(0xFE), None, "未知能力不猜名");
        assert_eq!(provision_logical_id(0xFE), None, "未知线码不猜名");
        assert_eq!(consumable_logical_id(0xFE), None, "未知线码不猜名");
    }

    /// 未闭合给养线码：保留 raw wire/state/payload，logical_id = None。
    #[test]
    fn unknown_provision_wire_keeps_raw() {
        let items = [
            descriptor(0x0B, false),
            descriptor(0x0D, false),
            descriptor(0x09, false),
            descriptor(0x9F, true), // 未闭合给养线码
        ];
        let payload = mk_type5_items(&items, &[100, 109, 114, 104]);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets).get(&0x31).cloned().unwrap();
        assert_eq!(v.provisions.len(), 1);
        let p = &v.provisions[0];
        assert_eq!((p.wire_code, p.state, p.logical_id.as_deref()), (0x9F, 1, None));
        assert_eq!(p.payload_raw[8..12], [0x00, 0x00, 0x80, 0xBF]);
        assert_eq!(p.kind, LoadoutItemKind::Provision);
    }

    /// 受控实验回归（2026-10-31）：Kranvagn 亲装大补给 → `0x49`（瑞典大/三文鱼）；
    /// 60TP 亲装茹尔汤 → `0x6D`（波兰大）；Vz.55 亲装大口粮 → `0x4A`（捷克大/Goulash）；
    /// AC Atlas 亲装大+小口粮 → `0x2D`/`0x2E`（其他系大小次序直接钉死，且证实
    /// **装载可同时携带同族大+小两个食物**——同场 Maus 0x0E+0x15 / LTG 0x10+0x17 /
    /// Bulldog 0x0F+0x16 为双食物自然样本，"53TP 带 0x6D+0x6E" 并非异常）。
    /// family 随解码导出；通用给养（燃料）无族。
    #[test]
    fn controlled_food_experiments_export_family() {
        let items = [
            descriptor(0x0B, false),
            descriptor(0x0D, false),
            descriptor(0x09, false),
            descriptor(0x49, true), // Kranvagn 大补给（受控）
            descriptor(0x4A, true), // Vz.55 大口粮（受控：捷克大/Goulash）
            descriptor(0x1D, true), // 改进燃料（通用，无族）
            descriptor(0x6D, true), // 60TP 茹尔汤（受控）
            descriptor(0x2D, true), // Atlas 大口粮（受控：其他系大）
            descriptor(0x2E, true), // Atlas 小口粮（受控：其他系小，与大并存）
        ];
        let payload = mk_type5_items(&items, &[100, 109, 114, 104]);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets).get(&0x31).cloned().unwrap();
        let by_wire = |w: u8| {
            v.provisions
                .iter()
                .find(|p| p.wire_code == w)
                .unwrap_or_else(|| panic!("wire {w:#04x} missing"))
        };
        let krv = by_wire(0x49);
        assert_eq!(krv.logical_id.as_deref(), Some("LARGE_FOOD"));
        assert_eq!(krv.family.as_deref(), Some("swedish"));
        let cze = by_wire(0x4A);
        assert_eq!(cze.logical_id.as_deref(), Some("LARGE_FOOD"));
        assert_eq!(cze.family.as_deref(), Some("czech"));
        let fuel = by_wire(0x1D);
        assert_eq!(fuel.family, None, "通用给养无族");
        let pol = by_wire(0x6D);
        assert_eq!(pol.logical_id.as_deref(), Some("LARGE_FOOD"));
        assert_eq!(pol.family.as_deref(), Some("polish"));
        // Atlas 双口粮：大在前小在后，均可导出族
        let oth_l = by_wire(0x2D);
        assert_eq!(oth_l.logical_id.as_deref(), Some("LARGE_FOOD"));
        assert_eq!(oth_l.family.as_deref(), Some("other"));
        let oth_s = by_wire(0x2E);
        assert_eq!(oth_s.logical_id.as_deref(), Some("SMALL_FOOD"));
        assert_eq!(oth_s.family.as_deref(), Some("other"));
        // 消耗品/能力不携带族
        assert!(v.consumables.iter().all(|c| c.family.is_none()));
    }

    /// 3 条描述符的真实玩家（Takuchan 形态）：全部为消耗品，无给养/能力；
    /// 装备串 5 件仍按线序给出，不得因给养缺失丢装备。
    #[test]
    fn three_consumables_without_provisions_still_decode() {
        let items = [
            descriptor(0x0B, false),
            descriptor(0x0D, false),
            descriptor(0x09, false),
        ];
        let eq = [103, 109, 122, 107, 110];
        let payload = mk_type5_items(&items, &eq);
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets).get(&0x31).cloned().unwrap();
        assert_eq!(v.consumables.len(), 3);
        assert!(v.provisions.is_empty());
        assert!(v.abilities.is_empty());
        assert_eq!(v.equipment_count, 5);
        assert_eq!(v.equipment_ids, eq.to_vec());
        assert_eq!(v.equipment, [103, 109, 122, 107, 110, 0, 0, 0, 0]);
    }

    /// 首条 Type5 只有配件串（无 `0A` 块），后续物化包带 item 描述符：
    /// 以带 items 的版本替换，配件/物品都不丢（首条 equipment-only 不是终态）。
    #[test]
    fn later_full_block_replaces_equipment_only_first_packet() {
        let eq = [103, 109, 114, 104, 111, 117, 105, 113, 101];
        let first = mk_type5_equipment_only(&eq);
        let second = mk_type5_items(
            &[
                descriptor(0x0B, false),
                descriptor(0x0D, false),
                descriptor(0x09, false),
                descriptor(0x0E, true),
                descriptor(0x1D, true),
                descriptor(0x15, true),
            ],
            &eq,
        );
        let packets: Vec<(u32, f32, &[u8])> =
            vec![(5, 1.0, first.as_slice()), (5, 2.0, second.as_slice())];
        let v = collect_vehicle_loadout(&packets).get(&0x31).cloned().unwrap();
        assert_eq!(v.items.len(), 6);
        assert_eq!(v.consumables.len(), 3);
        assert_eq!(v.provisions.len(), 3);
        assert_eq!(v.equipment_ids, eq.to_vec());
    }

    /// 消耗品线码 → 槽位：按 loadout 的 3 个消耗品槽顺序（0..2）；给养/能力/未知 = None。
    #[test]
    fn maps_consumable_wire_code_to_loadout_slot() {
        let payload = mk_type5_loadout();
        let packets: Vec<(u32, f32, &[u8])> = vec![(5, 1.0, &payload)];
        let v = collect_vehicle_loadout(&packets).get(&0x31).cloned().unwrap();
        assert_eq!(consumable_slot_of(&v, 0x0B), Some(0));
        assert_eq!(consumable_slot_of(&v, 0x0D), Some(1));
        assert_eq!(consumable_slot_of(&v, 0x09), Some(2));
        assert_eq!(consumable_slot_of(&v, 0x0E), None, "给养不占消耗品槽");
        assert_eq!(consumable_slot_of(&v, 0x67), None, "模式能力不占消耗品槽");
        assert_eq!(consumable_slot_of(&v, 0xFE), None, "未知线码不猜槽位");
    }
}

#[cfg(test)]
mod module_crew_tests {
    use super::*;

    /// 合成 Avatar method16 包（恒定 22B）：[avatarEid][method=16][argLen=10][vehicleId][codeA][codeB][relatedEid]
    fn mk16(avatar: u32, vehicle: u32, code_a: u8, code_b: u8, related: u32) -> Vec<u8> {
        let mut p = vec![0u8; 22];
        p[0..4].copy_from_slice(&avatar.to_le_bytes());
        p[4..8].copy_from_slice(&16u32.to_le_bytes());
        p[8..12].copy_from_slice(&10u32.to_le_bytes());
        p[12..16].copy_from_slice(&vehicle.to_le_bytes());
        p[16] = code_a;
        p[17] = code_b;
        p[18..22].copy_from_slice(&related.to_le_bytes());
        p
    }

    #[test]
    fn maps_module_and_crew_state_families() {
        let engine_dmg = mk16(0xAA, 0x21, 4, 31, 0); // 模块：引擎受损
        let gun_clear = mk16(0xAA, 0x21, 19, 36, 0); // 模块：主炮完全修复
        let crew_shock = mk16(0xAA, 0x22, 10, 41, 0x21); // 乘员：炮手震伤（related=攻击者）
        let crew_heal = mk16(0xAA, 0x22, 22, 43, 0); // 乘员：装填手治愈
        let unknown_mod = mk16(0xAA, 0x23, 4, 42, 0); // 42 未定义 → UNKNOWN 组件
        let packets: Vec<(u32, f32, &[u8])> = vec![
            (8, 20.0, &gun_clear),
            (8, 10.0, &engine_dmg),
            (8, 30.0, &crew_shock),
            (8, 40.0, &crew_heal),
            (8, 50.0, &unknown_mod),
        ];
        let ev = collect_module_crew_states(&packets);
        assert_eq!(ev.len(), 5);
        assert_eq!(
            (ev[0].clock, ev[0].vehicle_eid),
            (10.0, 0x21),
            "按 clock 升序"
        );
        assert_eq!(ev[0].component, ModuleComponent::Engine);
        assert_eq!(ev[0].state, ModuleState::DamagedDegraded);
        assert_eq!(ev[1].component, ModuleComponent::Gun);
        assert_eq!(ev[1].state, ModuleState::FullRepairedClear);
        // 乘员族：同一 codeA 在乘员下解读为震伤/治愈（与模块族不同）
        assert_eq!(ev[2].component, ModuleComponent::Gunner);
        assert_eq!(ev[2].state, ModuleState::CrewShellShocked);
        assert_eq!(ev[2].related_eid, 0x21);
        assert_eq!(ev[3].component, ModuleComponent::Loader);
        assert_eq!(ev[3].state, ModuleState::CrewHealed);
        // 未定义码保持 UNKNOWN，且 raw 码保留
        assert_eq!(ev[4].component, ModuleComponent::Unknown);
        assert_eq!(ev[4].component_code, 42);
    }

    #[test]
    fn rejects_non_method16_and_wrong_shape() {
        let mut other = mk16(0xAA, 0x21, 4, 31, 0);
        other[4..8].copy_from_slice(&35u32.to_le_bytes()); // method=35 → 非 method16
        let mut bad_len = mk16(0xAA, 0x21, 4, 31, 0);
        bad_len[8..12].copy_from_slice(&9u32.to_le_bytes()); // argLen != 10
        let short = vec![0u8; 21]; // 非 22B
        let packets: Vec<(u32, f32, &[u8])> = vec![
            (8, 1.0, &other),
            (8, 2.0, &bad_len),
            (8, 3.0, &short),
            (7, 4.0, &short),
        ];
        assert!(collect_module_crew_states(&packets).is_empty());
    }
}

#[cfg(test)]
mod launch_identity_tests {
    use super::*;

    fn method29(shooter: u32, shot_id: u32, point: [f32; 3], vel: [f32; 3]) -> Vec<u8> {
        let mut args = vec![0u8; 37];
        args[0..4].copy_from_slice(&shooter.to_le_bytes());
        args[4..8].copy_from_slice(&shot_id.to_le_bytes());
        args[8] = 4;
        for (i, v) in point.into_iter().chain(vel).enumerate() {
            let off = 9 + i * 4;
            args[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
        let mut p = vec![0u8; 12];
        p[4..8].copy_from_slice(&0x1du32.to_le_bytes());
        p[8..12].copy_from_slice(&(args.len() as u32).to_le_bytes());
        p.extend_from_slice(&args);
        p
    }

    #[test]
    fn repeated_method29_with_same_shot_id_is_one_shoot_and_keeps_primary_launch() {
        let first = method29(
            7,
            45509301,
            [99.82513, 25.24507, 20.54626],
            [-616.206, -2.696, -287.546],
        );
        let continuation = method29(
            7,
            45509301,
            [-51.16804, 24.38959, -49.91319],
            [-668.906, -87.832, 85.235],
        );
        let packets: Vec<(u32, f32, &[u8])> =
            vec![(8, 110.26195, &first), (8, 110.35225, &continuation)];

        let (launches, short) = collect_launches(&packets, |_| true);
        assert!(short.is_empty());
        // 收集层保留全部段（不去重）：同一发的续段要留给组装层作落点用
        assert_eq!(launches.len(), 2, "收集层不去重（续段由组装层折叠）");
        assert_eq!(launches[0].shot_id, 45509301);
        assert!((launches[0].t - 110.26195).abs() < 1e-5);
        assert_eq!(launches[0].point, [99.82513, 25.24507, 20.54626]);
        assert_eq!(launches[0].vel, [-616.206, -2.696, -287.546]);
        // 组装层折叠：射击数仍为 1，链首 = 发射段，续段首点 = 落点（画到这里为止）
        let (is_cont, segs) = shot_segments(&launches);
        assert_eq!(
            is_cont,
            vec![false, true],
            "同一 (shooter, shotId) 只出一次射击"
        );
        let got = segs.get(&(7, 45509301)).cloned().unwrap_or_default();
        assert_eq!(got.len(), 1, "续段按序全部保留（折线用）");
        assert_eq!(got[0].0, 110.35225);
        assert_eq!(got[0].1, [-51.16804, 24.38959, -49.91319]);
    }
}

#[cfg(test)]
mod shot_segments_tests {
    use super::*;

    fn l(t: f32, shooter: u32, shot_id: u32, point: [f32; 3]) -> LaunchEntry {
        LaunchEntry {
            t,
            shooter,
            shot_id,
            point,
            vel: [0.0, 0.0, -680.0],
            shell_code: 0,
            shooter_prop2: None,
        }
    }

    /// 同 (shooter, shotId) 的第二条 = 续段（跳弹/穿透），且续段首点被登记为该发的落点；
    /// 不同射手/不同 shotId → 各自独立，互不影响
    #[test]
    fn continuation_is_folded_into_its_own_shot() {
        let launches = vec![
            l(71.219, 11, 48267266, [109.65, 32.33, 135.39]), // 发射段
            l(71.219, 11, 48267266, [131.92, 30.58, 72.50]),  // 跳弹续段（原点=受击方装甲处）
            l(71.5, 11, 48267267, [100.0, 30.0, 150.0]),      // 同射手另一发（无续段）
            l(71.6, 12, 48267266, [10.0, 20.0, 10.0]),        // 另一射手（不同链）
        ];
        let (is_cont, segs) = shot_segments(&launches);
        assert_eq!(is_cont, vec![false, true, false, false]);
        assert_eq!(
            segs.get(&(11, 48267266)).cloned(),
            Some(vec![(71.219, [131.92, 30.58, 72.50], [0.0, 0.0, -680.0])])
        );
        assert!(!segs.contains_key(&(11, 48267267)));
        assert!(!segs.contains_key(&(12, 48267266)));
    }

    /// 三段链：续段**全部按序保留**（折线要一路画到服务器终点）
    #[test]
    fn all_continuations_are_kept_in_order() {
        let launches = vec![
            l(10.0, 5, 7, [0.0, 0.0, 0.0]),
            l(10.1, 5, 7, [10.0, 1.0, 0.0]),
            l(10.3, 5, 7, [30.0, 5.0, 0.0]),
        ];
        let (is_cont, segs) = shot_segments(&launches);
        assert_eq!(is_cont, vec![false, true, true]);
        let got = segs.get(&(5, 7)).cloned().unwrap_or_default();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1, [10.0, 1.0, 0.0]);
        assert_eq!(got[1].1, [30.0, 5.0, 0.0]);
    }

    /// 折线几何：段时长按 |Δ|/段速度；via 与 leg_secs 满足 len(legs) = len(via)+1
    #[test]
    fn legs_use_per_segment_speed() {
        let chain = vec![
            (1.0f32, [100.0, 0.0, 0.0], [0.0, 0.0, -500.0]),
            (2.0f32, [100.0, 0.0, -300.0], [0.0, 0.0, -250.0]),
        ];
        let (via, legs) = shot_legs(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1000.0],
            Some(&chain),
            [100.0, 0.0, -800.0],
        );
        assert_eq!(via, vec![[100.0, 0.0, 0.0], [100.0, 0.0, -300.0]]);
        assert_eq!(legs.len(), 3);
        assert!((legs[0] - 0.1).abs() < 1e-6, "段0: 100m / 1000mps");
        assert!((legs[1] - 0.6).abs() < 1e-6, "段1: 300m / 500mps");
        assert!(
            (legs[2] - 2.0).abs() < 1e-6,
            "段2: 500m / 250mps（末段用最后续段速度）"
        );
    }

    /// 无续段（普通弹）→ 表内无键、折线退化为单段直线
    #[test]
    fn plain_shot_has_no_continuation() {
        let launches = vec![l(10.0, 5, 7, [0.0, 0.0, 0.0])];
        let (is_cont, segs) = shot_segments(&launches);
        assert_eq!(is_cont, vec![false]);
        assert!(segs.is_empty());
        let (via, legs) = shot_legs([0.0, 0.0, 0.0], [0.0, 0.0, -680.0], None, [0.0, 0.0, -68.0]);
        assert!(via.is_empty());
        assert_eq!(legs.len(), 1);
        assert!((legs[0] - 0.1).abs() < 1e-3);
    }
}

#[cfg(test)]
mod dmg_attribution_tests {
    use super::*;

    fn ev(clock: f32, victim: u32, hp: u16, source: u32) -> HpEvent {
        HpEvent { clock, victim, hp, source, cause: 0 }
    }

    /// 同 tick 双命中同一受害者：逐事件采样必须保留两条降幅（旧"同钟异值整组丢弃"
    /// 会吞掉中间采样，把两发与下一发合成一个假降幅——实测 357+341+389→1087）。
    #[test]
    fn same_tick_events_survive_as_separate_losses() {
        let author = 1001u32;
        let other = 1002u32;
        let victim = 2001u32;
        let events = vec![
            ev(74.94986, victim, 3213, author),
            ev(74.94986, victim, 2872, other),
            ev(82.632904, victim, 2483, author),
        ];
        let mut seed = HashMap::new();
        seed.insert(victim, (56.93566f32, 3570u16));

        // 全 source：三段降幅逐事件成立
        let all = derive_dmg_losses(&events, None, &seed);
        let got: Vec<(u32, u32, u16)> = all
            .iter()
            .map(|l| (l.source, l.dmg, l.hp_cur))
            .collect();
        assert_eq!(got, vec![(author, 357, 3213), (other, 341, 2872), (author, 389, 2483)]);

        // 作者过滤：只剩本人的两段（357 / 389），绝不合成 1087
        let mine = derive_dmg_losses(&events, Some(author), &seed);
        let got: Vec<u32> = mine.iter().map(|l| l.dmg).collect();
        assert_eq!(got, vec![357, 389]);
    }

    /// 确定性时钟配对：命中时刻 == 降幅事件时钟才成立；区间内的中间降幅不再被
    /// "就近归属"吞掉（旧语义下 (t_prev, t_cur] 区间包含即认领）。
    #[test]
    fn assign_pairs_by_exact_clock_only() {
        let author = 1001u32;
        let other = 1002u32;
        let victim = 2001u32;
        let losses = vec![
            DmgLoss { victim, source: author, t_cur: 74.94986, dmg: 357, hp_cur: 3213 },
            DmgLoss { victim, source: other, t_cur: 74.94986, dmg: 341, hp_cur: 2872 },
            DmgLoss { victim, source: author, t_cur: 82.632904, dmg: 389, hp_cur: 2483 },
            // 位于两次命中之间的"孤儿"降幅（如别的来源未广播的伤害落入作者链）
            DmgLoss { victim, source: author, t_cur: 80.0, dmg: 999, hp_cur: 0 },
        ];
        let shots: Vec<Option<(f32, u32, u32)>> = vec![
            Some((74.94986, author, victim)),  // 首发：357（不是 999、不是 341）
            Some((74.94986, other, victim)),   // 同刻他射：341
            Some((82.632904, author, victim)),  // 第二发：389
        ];
        let got = assign_dmg_losses(&shots, &losses);
        assert_eq!(got.get(&0), Some(&(357, 3213)));
        assert_eq!(got.get(&1), Some(&(341, 2872)));
        assert_eq!(got.get(&2), Some(&(389, 2483)));
        assert_eq!(got.len(), 3, "孤儿降幅不得被任何一发认领");
    }

    /// 同射手同受害者同钟多发（弹夹双击）：按未消费第一段认领，互斥不重复计数。
    #[test]
    fn same_clock_competition_claims_in_launch_order() {
        let author = 1001u32;
        let victim = 2001u32;
        let losses = vec![
            DmgLoss { victim, source: author, t_cur: 50.0, dmg: 100, hp_cur: 900 },
            DmgLoss { victim, source: author, t_cur: 50.0, dmg: 120, hp_cur: 780 },
        ];
        let shots: Vec<Option<(f32, u32, u32)>> = vec![
            Some((50.0, author, victim)),
            Some((50.0, author, victim)),
            Some((50.0, author, victim)), // 无段可认领 → 不产出（走 dc 兜底）
        ];
        let got = assign_dmg_losses(&shots, &losses);
        assert_eq!(got.get(&0), Some(&(100, 900)));
        assert_eq!(got.get(&1), Some(&(120, 780)));
        assert_eq!(got.get(&2), None);
    }
}

#[cfg(test)]
mod direct_hit8_damage_tests {
    use super::*;

    /// 合成直击形态 method8 载荷：信封 [eid][0x08][args_len] + args 21B
    /// （[shooter][victim][count=1][result][cmpIndex][hash6 6B][?][u16 伤害][u8 状态]）。
    fn payload(shooter: u32, victim: u32, result: u8, damage: u16, trailer: u8) -> Vec<u8> {
        let mut args = vec![0u8; 21];
        args[0..4].copy_from_slice(&shooter.to_le_bytes());
        args[4..8].copy_from_slice(&victim.to_le_bytes());
        args[8] = 0x01;
        args[9] = result;
        args[10] = 1;
        args[11..17].copy_from_slice(&[0xAA; 6]);
        args[18..20].copy_from_slice(&damage.to_le_bytes());
        args[20] = trailer;
        let mut p = Vec::new();
        p.extend_from_slice(&victim.to_le_bytes());
        p.extend_from_slice(&0x08u32.to_le_bytes());
        p.extend_from_slice(&(args.len() as u32).to_le_bytes());
        p.extend_from_slice(&args);
        p
    }

    /// 状态字节 ≠0 = 服务器已结算：u16 即该发实际伤害（overkill 已封顶）。
    #[test]
    fn settled_tail_damage_is_decoded() {
        let p = payload(7, 9, 3, 577, 0x01);
        let packets: Vec<(u32, f32, &[u8])> = vec![(8, 12.5, p.as_slice())];
        let dh = collect_direct_hits8(&packets);
        assert_eq!(dh.len(), 1);
        assert!(dh[0].settled);
        assert_eq!(dh[0].damage, 577);
    }

    /// 状态字节 =0 = 未结算：u16 是标称占位值（如 340），消费方不得当伤害用（settled=false）。
    #[test]
    fn unsettled_tail_is_flagged() {
        let p = payload(7, 9, 2, 340, 0x00);
        let packets: Vec<(u32, f32, &[u8])> = vec![(8, 12.5, p.as_slice())];
        let dh = collect_direct_hits8(&packets);
        assert_eq!(dh.len(), 1);
        assert!(!dh[0].settled);
        assert_eq!(dh[0].damage, 340, "字段照读，但 settled=false 不得入伤害");
    }

    /// 更短收敛形态（args_len < 21）不带结算字段 → damage=0 / settled=false（fail-closed）。
    #[test]
    fn shorter_shape_has_no_settled_damage() {
        let mut args = vec![0u8; 20];
        args[0..4].copy_from_slice(&7u32.to_le_bytes());
        args[4..8].copy_from_slice(&9u32.to_le_bytes());
        args[8] = 0x01;
        args[9] = 3;
        let mut p = Vec::new();
        p.extend_from_slice(&9u32.to_le_bytes());
        p.extend_from_slice(&0x08u32.to_le_bytes());
        p.extend_from_slice(&(args.len() as u32).to_le_bytes());
        p.extend_from_slice(&args);
        let packets: Vec<(u32, f32, &[u8])> = vec![(8, 1.0, p.as_slice())];
        let dh = collect_direct_hits8(&packets);
        assert_eq!(dh.len(), 1);
        assert!(!dh[0].settled);
        assert_eq!(dh[0].damage, 0);
    }
}
