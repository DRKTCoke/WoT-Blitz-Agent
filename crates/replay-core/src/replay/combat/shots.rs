//! 射击复现提取：ShotReplayData 数据面、ShotScanShared 共享扫描、
//! 作者严格路径与他人宽松路径（合并方案见 docs/architecture-debt.md 第 2 节）。

use super::*;
use crate::replay::filter::SegmentedPoseTimeline;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 单发射击的数据质量标注（宽松降级与快照陈旧度的可视化依据）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShotQuality {
    /// 射手状态快照距开火时刻的偏移（ms，负=早于开火；|值|大 = type=10 稀疏）
    pub shooter_state_dt_ms: i32,
    /// 射手位置为炮口坐标兜底（type=10 快照缺失，AoI 裁剪；见 ShooterAimData 注）
    #[serde(skip_serializing_if = "is_false")]
    pub shooter_pos_from_muzzle: bool,
    /// 目标状态采样距命中包时刻的偏移（ms，≤0 = 状态为命中批次前最后已知值）；脱靶弹无目标 = None。
    /// 命中弹的状态来源 = method8 通知处理时刻受击者的运行状态（WI 对齐锚点）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_state_dt_ms: Option<i32>,
    /// 炮塔朝向降级为车体朝向（type=7 prop2 缺失）："shooter" / "target"
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub turret_degraded: Vec<String>,
    /// 命中弹未在血量链找到伤害区间（服务器未记账 HP，damage 可能为 0）
    #[serde(skip_serializing_if = "is_false")]
    pub dmg_unattributed: bool,
    /// 受击方在开火前已死亡（HP 终态证据 / 作者 method38 `TARGET_ALREADY_DEAD`）：
    /// 服务器 method8 仍可能报告残骸命中，但该发按 miss 处理（target_eid/name 置空，
    /// 不进入伤害归属与本地穿深判定）。false 时省略（旧产物缺省 = false）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub target_dead: bool,
    /// 锚点快照来源（仅降级/回退路径序列化，供 UI 徽章告警；正常路径不序列化）：
    /// - shooter："nearest"=最近包（AoI 稀疏）、"extrapolated"=段末速度外推；"filtered"（段内插值）=正常不序列化；
    /// - target："nearest"/"filtered"/"extrapolated" 均为 method8 命中通知缺失后的回退路径；
    ///   "wi_hit_state"（method8 通知状态）= 正常路径不序列化。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shooter_anchor_src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_anchor_src: Option<String>,
    /// 弹种来自 method0x07 开火广播兜底（type=32 命中通知未转发）；false = segment 权威来源
    #[serde(skip_serializing_if = "is_false")]
    pub shell_from_broadcast: bool,
    /// 弹种来自 method0x1b 地形命中广播兜底（0x07 亦未覆盖：他人脱靶弹/作者 0x07 空窗）；
    /// 同时意味着 terrain_impact 附带精确落点（撞静态物的弹无 0x1b，不适用）
    #[serde(default, skip_serializing_if = "is_false")]
    pub shell_from_terrain: bool,
    /// 弹种由 method29 args[8]**弹种编码**指纹回填（发射包自带，每发必有）：段包缺失
    /// （cmpIndex=0 底盘/履带命中无 26/27B 通知）或脱靶时的弹种来源。映射由同场同射手
    /// 已判定发次现建（自校准）；编码多候选（同车标准/金币弹同族共码）时经弹速去重后唯一
    /// 也算本档。见 [`fill_shells_from_launch_fingerprints`]。
    #[serde(default, skip_serializing_if = "is_false")]
    pub shell_from_launch_code: bool,
    /// 弹种由发射弹速指纹回填（编码映射不可用时的兜底；同场同射手同弹速唯一弹种）
    #[serde(default, skip_serializing_if = "is_false")]
    pub shell_from_velocity: bool,
    /// 射手炮管俯仰由发射速度向量推算（prop2 缺失回退；作者路径恒 false——作者回退走 method36 field2）
    #[serde(skip_serializing_if = "is_false")]
    pub shooter_pitch_from_velocity: bool,
    /// 射手炮管俯仰回退到 method36 field2（车体系炮管俯仰，WotbTools PROVEN；仅作者路径 prop2 缺失时）
    #[serde(default, skip_serializing_if = "is_false")]
    pub shooter_pitch_from_method36: bool,
    /// 炮管俯仰回退（prop2 frac 不可得）："shooter"=射手回退速度向量/method36 field2，"target"=受击方回退车体 pitch
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gun_pitch_degraded: Vec<String>,
    /// 俯仰采样流陈旧（prop2 断流 >2s，AoI 边界/补发簇；frac 恒定本身是炮管定点/贴
    /// 极限的如实上报，不算冻结）："shooter" / "target"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pitch_frozen: Vec<String>,
}

/// 受击方在 `t`（开火时刻）之前是否已死亡。
///
/// 判据 = HP 事件链最后一条 `clock <= t - TOL` 的状态：终态（0 / 负 i16 哨兵，
/// 与 `hp_terminal_normalized` 同判）→ true；正 HP（含复活后的正向事件）→ false。
/// 无 HP 证据 → false（fail-open：AoI 缺失不误杀活目标）。
/// TOL=0.1s 吸收时钟量化：同 tick 的击杀（死亡事件略早于 method29）仍算有效射击。
fn victim_dead_before(hp_events: &[HpEvent], eid: u32, t: f32) -> bool {
    const TOL: f32 = 0.10;
    hp_events
        .iter()
        .rev()
        .find(|e| e.victim == eid && e.clock <= t - TOL)
        .map(|e| hp_terminal_normalized(e.hp) == 0)
        .unwrap_or(false)
}

/// 一次射击事件命中的**单个受击者**（一发多 victim：HE 溅射可同时命中多辆；一发在同一
/// 受击者上的多次装甲交互也各成一条）。顶层 `target_*` 字段 = `is_primary=true` 的那条。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShotHit {
    pub target_eid: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_name: String,
    /// 该受击者的服务器结算伤害（method8 尾部 settled 值；未结算 = 0）
    pub damage: u32,
    /// 命中结果枚举（method8 同域；255 = 未获取）
    pub game_hit_result: u8,
    /// 主目标（与顶层 target_eid/target_name/damage 同源）
    pub is_primary: bool,
    /// 装甲组/段（同事件 type=32 的 segment，按 hash6 令牌配对；未转发 = 缺省）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor_group: Option<u8>,
    /// 命中事件令牌（hash6 hex；method8 ↔ type=32 同源）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit_token: Option<String>,
    /// 服务器下发的受击部件索引（method8 cmpIndex）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_part_index: Option<u8>,
    /// 本次命中受损/摧毁模块位图（作者 method38 逐受击者；他人不可得）——不参与判定
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crit_modules: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destroyed_modules: Option<u32>,
}

/// 一次射击事件的"复现数据"：双方位置/朝向（type=10 实体状态包解码），供 3D 查看器复现热力图视角。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShotReplayData {
    pub index: usize,
    pub time_s: f32,
    pub damage: u32,
    pub target_name: String,
    /// 受击方实体 id（作者 = method38 受击者，服务器权威；他人 = method8 直击通知）。
    /// 身份域与显示域（[`Self::target_name`]）解耦——投影/消费方联表一律用 eid；
    /// 名字缺失不作为提取失败条件（fail-soft）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_eid: Option<u32>,
    /// 本发命中的**全部**受击者（一发多 victim / 多次装甲交互都保留；主目标在首位）。
    /// 空 = 无额外命中（缺省不序列化，旧消费方行为不变）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hits: Vec<ShotHit>,
    /// 受击方**弹着时刻**的 armorsStates 状态号（Vehicle propID 16，additive，skip-when-none）。
    /// None = 受击车无状态流或弹着早于首个样本（首样本 = 入视野 create 快照；更早区间
    /// 按声明文件 `data/armor_states.json` 里 `<default/>` 态兜底，不猜）。仅动态装甲车
    /// （全库 6 辆）会有值；由 [`attach_shot_armor_states`] 在持 packets 的生产入口回填，
    /// 消费方（射击复现）按声明门控后取该态的激活板集。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_armor_state: Option<u8>,
    pub is_kill: bool,
    /// 射手实体 id（method29 shooterEntityId；作者路径恒为作者本人实体）
    pub shooter_eid: u32,
    /// 射手昵称（type=5 名册；未知为空串）
    #[serde(skip_serializing_if = "String::is_empty")]
    pub shooter_name: String,
    /// 是否回放作者本人的射击。false = 其他玩家：命中结果来自 method8 枚举 + 血量链伤害（无 method38 细节）
    #[serde(skip_serializing_if = "is_false")]
    pub is_author: bool,
    /// 射手（作者玩家实体）位置 [x, y(离地), z]
    pub shooter_pos: [f32; 3],
    /// 射手朝向（3 个浮点，语义为最可能猜测：偏航/俯仰/侧倾，弧度）
    pub shooter_ang: [f32; 3],
    pub target_pos: [f32; 3],
    pub target_ang: [f32; 3],
    /// 目标炮塔绝对朝向（弧度，与 hull yaw 同参考系，顺时针为正）；来源 type=7 sub=2 高 10 位粗值，
    /// ang = (u16>>6)/1024×2π − π（实测校准有 180° 偏移；低 6 位 = 炮管俯仰比例，不参与偏航）。
    pub target_turret_yaw: f32,
    /// 受击方炮管俯仰（弧度，炮塔系，正=仰角）；来源 prop2 低 6 位 frac：
    /// pitch = ele − frac/63×(dep+ele)（按车型极限锚定，frac=63 ↔ 俯角极限、0 ↔ 仰角极限）。
    /// 回退（无 prop2 采样/无车型极限）：车体 pitch（type10，正=车头下坡，语义不同仅兜底），
    /// 见 quality.gun_pitch_degraded。流断流 >2s 时 quality.pitch_frozen 提示陈旧。
    pub target_gun_pitch: f32,
    /// 存在 type=32 服务器解码的抵达成角（来向方位角校验通过，正=仰角）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub target_gun_pitch_server: bool,
    /// 恒 0（占位兼容字段）。type=32 通知包内**无**受击者炮塔角——26/27B 的
    /// 字节9=每实体序号、26B 字节11=cmpIndex、27B 字节10(7bit) 均非炮塔角；
    /// 主用 target_turret_yaw（prop2）。
    pub type32_turret_yaw: f32,
    /// 射手炮塔绝对朝向（弧度）= sub2_rel + shooter_hullYaw，用于精确入射方位角（替代位置差推算）。
    pub shooter_turret_yaw: f32,
    /// 射手炮管俯仰（弧度，炮塔系，正=仰角）；来源与受击方同源 = prop2 frac 解码（按射手车型极限）。
    /// 回退：作者 = method36 field2 车体系炮管俯仰（quality.shooter_pitch_from_method36）；
    /// 他人 = 发射速度向量反解（quality.shooter_pitch_from_velocity）。
    pub shooter_gun_pitch: f32,
    /// 弹着点相对【命中通知状态目标位置】的偏移 [x, y, z]（米）；来源 type=8 method20（shotId 配对）。
    /// 注意：method20 为弹道终点（穿透后出射/停止点，可在目标另一侧）。
    pub aim_point: [f32; 3],
    /// 炮口位置相对【命中通知状态目标位置】的偏移（与 aim_point 同基准）；viewer 与 launch_velocity 组合成弹道射线。
    pub launch_point_rel: [f32; 3],
    /// 弹道两点（回放世界系，米）：ball_a = 炮口发射位置（method29），ball_b = 弹道终点（method20，shotId 配对）；
    /// 两点确定弹道直线——viewer 用方向做 raycast，轴映射只需一次方向变换。
    pub ball_a: [f32; 3],
    pub ball_b: [f32; 3],
    /// 发射速度向量 [vx, vy, vz]（m/s）；method29 launchVelocity 服务器权威弹道方向（含俯仰），
    /// 与 launchPoint→终点连线夹角实测 <0.1°。
    pub launch_velocity: [f32; 3],
    /// 弹道折线中间点（跳弹/穿透出射点，按飞行顺序；直射弹为空）——同 (shooter, shotId) 的
    /// 后续 method29 起点。渲染：`ball_a → via… → ball_b`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via: Vec<[f32; 3]>,
    /// 各段时长（秒）：段 0 = ball_a→via[0]，…，末段 = …→ball_b；`len == via.len() + 1`。
    /// 按几何/段速度计算（不用包时钟——10Hz 量化会让跳弹段变零长）。空 = 单段直线弹。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leg_secs: Vec<f32>,
    /// 命中结果位图（u32 = flags16 | headerHi16<<16；wotinspector hit_flags 同源）。
    /// 全 16 位命名见 [`hit_flags_mod`]（WotbTools 全位 PROVEN）：0x0001 直接击杀 / 0x0002 目标已死 /
    /// 0x0004 起火 / 0x0008 跳弹 / 0x0010 材料击穿 / 0x0020 未击穿 / 0x0040/0x0080 间隙层穿与未穿 /
    /// 0x0100/0x0200 模块穿与未穿 / 0x0400 履带 / 0x0800 火炮 / 0x1000-0x8000 爆炸分支。
    /// headerHi 保留 raw（多数 0x0002=录像者关联位；Maus 边界 0x0012/0x0028，禁当命中位解码）。
    pub hit_flags: u32,
    /// 模块受损位掩码——wotinspector crit_modules 同源；bit = componentToken - 31（token 31..43 → bit 0..12；实测 token33 受损 → WI bit2=0x04 ✓）。
    pub crit_modules: u32,
    /// 模块摧毁位掩码（映射同上，state=2；实测 token35 履带摧毁 → bit4）。
    pub destroyed_modules: u32,
    /// 游戏原生命中段 u64（type=32 警告包尾 8 字节 LE）：`[result u8][shell_global_id u24 LE][0x00][X][Y][Z]`。
    /// - result：命中结果枚举（同 game_hit_result）。
    /// - shell_global_id（u24 LE）= (shells.xml 局部 id << 8) | 国家基数（nation_id×16+10：uk=0x5a、japan=0x6a、usa=0x2a）。
    /// - 字节4 恒 0x00；末 3 字节语义未解（服务器不下发片元编号）；服务器仅转发部分命中通知，0 = 未获取。
    pub segment: u64,
    /// 命中弹种全局 id（24 位，含国家基数字节；与 WI shell_id 同值同源；0 = 未获取）
    pub shell_id: u32,
    /// 弹种原始串（tanks.pb shells.xml：ap/ap_cr/heat/he 及 *_premium 修饰；ShellKindTable 按
    /// shell_id 全局 id 回填，兜底链各级均可命中；未识别为空串）。前端自行做标签映射与金弹判定，勿在此归一化。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shell_kind: String,
    /// segment[7]（B7）= 命中装甲板 plateId；viewer 片元验证 fragOk 用同源值
    pub armor_group: u8,
    /// segment[5..7] 组成的 u16 BE——非三角形索引：res=1 未穿时恒负值（可作未穿
    /// 判定冗余位）、res=0 恒 ~71-83；参照系未解，保留透传
    pub hit_triangle: u16,
    /// 游戏命中结果枚举（method8 b9 / type=32 segment 低字节同源）：
    /// 0=无命中结果 1=未击穿 2=间隙层止 3=有伤害（击穿/HE 爆炸）；
    /// **4 = raw（语义未定）**——观测中既有履带/模块交互（cmpIndex 0/3、0 伤），
    /// 也有带伤 case；按 `docs/wotbtools-cross-reference.md` §三.2 裁决，禁命名为
    /// 跳弹或履带，消费方保留 raw、不要把 4 映射成跳弹/击穿/未穿。255 = 未获取。
    pub game_hit_result: u8,
    /// method8 ↔ type=32 同事件共享的 6 字节 = 游戏客户端 DecodeShotSegment 两点编码
    /// （出入点的部件 AABB 量化坐标，权威定义见《回放与射击逆向总集》第三篇 §二）；
    /// 3D 查看器据此标注出入点并构建 P1→P2 判定射线。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_token: Option<String>,
    /// 特殊弹药效果 ID（WotbTools PROVEN：1=精准火力 2=钨芯弹，可同发共存）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<u32>,
    /// 开火时刻（秒）——与 method29 发射包确定性匹配
    pub fire_time: f32,
    /// Shot 顶层身份：unique shotId = 一次开火。method29 发射 ↔ method20 终点以此确定性配对；
    /// 同一 shotId 的后续 method29 / 多次装甲交互不得展开成额外 Shot。
    pub shot_id: u32,
    /// 弹药槽位——type=28 选择状态在发射时刻的值（3D 视图弹种选择器索引用）
    pub shell_slot: u32,
    /// type=35 服务器竞技场 tick 计数器 @ 开火时刻（10Hz u8 递增，回放时钟秒×10）；开火 tick 判定 100/100 实测对齐。
    pub fire_tick: f32,
    /// 受击坦克 type=10 采样（命中 ±1s，位置相对命中通知状态锚点，世界系米；dt 锚定命中包时刻）。
    /// 末项可能为合成"游戏渲染位"采样（render=true，dt=0，滤波器命中帧输出，viewer ◎渲染位）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tick_samples: Vec<TickSample>,
    /// 射手坦克 type=10 采样（开火 ±0.2s，世界系绝对坐标）；末项可能为合成渲染位采样（render=true）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shooter_tick_samples: Vec<TickSample>,
    /// 地形命中数据（Avatar method 0x1b；全局广播含所有玩家脱靶弹，shotId 精确配对；
    /// 仅当该发未命中任何坦克且撞到地形时存在，约覆盖 2/3 地形弹）。
    /// args(34) = [shotId u32][shell_global_id u32][material u8][impactPoint 3×f32][terminalDir 3×f32][tail u8]。
    /// impact_point == method20 弹道终点；terminal_dir = 弹道末段速度方向向量（与发射速度同向）；
    /// material 落点材质类（观测 0/1/2/4/5，命名未定）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terrain_impact: Option<TerrainImpactData>,
    /// 开火时刻瞄准快照（Avatar method36，缺失时 None）。炮塔相对偏航为 f64 全精度（prop2 为 u16 量化）；
    /// state_before/after 即 method36 field6.field1 动态扩散 bloom（见 ShooterAimData 注）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shooter_aim: Option<ShooterAimData>,
    /// 射手车辆配件（Type5 物化 9 字节选择串；calib shells / enhanced armor 供查看器
    /// 自动穿深/厚度系数，替代手动勾选）。物化缺失（AoI 未覆盖）时 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shooter_equipment: Option<VehicleEquipment>,
    /// 受击方车辆配件（脱靶弹无目标时 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_equipment: Option<VehicleEquipment>,
    /// 兼容旧字段：= type32_turret_yaw（曾误标为"来袭方向"，实为受击者炮塔角）。
    pub incoming_yaw: f32,
    /// 兼容旧字段：= target_gun_pitch（曾误标为"来袭俯角"，实为受击者炮管俯仰）。
    pub incoming_pitch: f32,
    /// 数据质量标注（快照陈旧度/降级项；作者路径填快照偏移，他人路径另含降级标记）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ShotQuality>,
    /// 服务器下发的受击部件索引 cmpIndex（method8 args[10]；0=底盘/履带 1=车体 2=炮塔 3=炮管，
    /// 命中高度分层实证）——游戏 showDamageFromShot/DecodeShotSegment 用它在指定部件上放
    /// 弹着点；与本地 raycast 的部件选择对照 = 命中位置偏差的校准基准。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_part_index: Option<u8>,
    /// 射手渲染层锚点（客户端位置滤波器输出；None = type=10 采样缺失不可构建时间线）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shooter_render: Option<RenderAnchorData>,
    /// 受击者渲染层锚点（仅命中弹；"玩家当时看到的"受击者位姿）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_render: Option<RenderAnchorData>,
    /// 受击方渲染时间线（滤波器输出 0.1s 降采样，命中 −3.0~+2.0s，pos 绝对世界坐标）——
    /// 滑块严格对齐游戏每帧实际显示位姿（含 latency 移位/误差盒钳位/外推）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_render_timeline: Vec<RenderTimelineSample>,
    /// 射手方渲染时间线（开火 −2.0~+2.0s，pos 绝对世界坐标）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shooter_render_timeline: Vec<RenderTimelineSample>,
    /// 受击方炮塔相对角时间线（prop2，命中 −3.0~+2.0s，[dt, rel_yaw 弧度]）——滑块实时驱动炮塔
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_turret_timeline: Vec<(f32, f32)>,
    /// 射手方炮塔相对角时间线（prop2，开火 −2.0~+2.0s，[dt, rel_yaw 弧度]）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shooter_turret_timeline: Vec<(f32, f32)>,
    /// 射手炮管俯仰时间线（prop2 frac 解码，开火 −2.0~+2.0s，[dt, 弧度，正=仰角]）；
    /// 无车型极限锚定时作者路径回退 method36 field2（车体系炮管俯仰，弧度）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shooter_gun_timeline: Vec<(f32, f32)>,
    /// 受击方炮管俯仰时间线（prop2 frac 解码，命中 −3.0~+2.0s，[dt, 弧度，正=仰角]）——
    /// 滑块实时驱动炮管（与 target_turret_timeline 配对）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_gun_timeline: Vec<(f32, f32)>,
}

/// shots 切面 additive 回填：受击方**弹着时刻**的 armorsStates 状态号
/// （[`ShotReplayData::target_armor_state`]）。在**持 packets 的生产入口**调用
/// （wasm `shot_replays_json` / agent CLI / viewer 路径），提取层本身不持有全量包流。
/// 弹着时刻 = `fire_time + Σleg_secs`（无折线 = 直线估计，与 PlaybackShot 同式）；
/// 无轨道 / 快照前区间不注入——消费方按声明 `<default/>` 态兜底。
pub fn attach_shot_armor_states(
    shots: &mut [ShotReplayData],
    packets: &[(u32, f32, &[u8])],
) {
    let tracks = crate::replay::combat::collect_armor_state_tracks(packets);
    if tracks.is_empty() {
        return;
    }
    for s in shots {
        let Some(eid) = s.target_eid else { continue };
        let Some(track) = tracks.iter().find(|t| t.eid == eid) else { continue };
        let flight = if s.leg_secs.is_empty() {
            let d2 = (s.ball_b[0] - s.ball_a[0]).powi(2)
                + (s.ball_b[1] - s.ball_a[1]).powi(2)
                + (s.ball_b[2] - s.ball_a[2]).powi(2);
            let v2 = s.launch_velocity[0] * s.launch_velocity[0]
                + s.launch_velocity[1] * s.launch_velocity[1]
                + s.launch_velocity[2] * s.launch_velocity[2];
            let v = v2.sqrt();
            if v > 0.0 {
                ((d2.sqrt()) / v).clamp(0.02, 8.0)
            } else {
                0.5
            }
        } else {
            s.leg_secs.iter().sum()
        };
        s.target_armor_state = track.state_at(s.fire_time + flight);
    }
}

/// method 0x1b 地形命中数据（字段语义见 [`ShotReplayData::terrain_impact`]）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainImpactData {
    /// 落点材质类（0/1/2/4/5 观测，具体命名未定）
    pub material: u8,
    /// 精确落点（回放世界系，米；== method20 弹道终点）
    pub impact_point: [f32; 3],
    /// 弹道末段速度方向向量（世界系；与发射速度同向，norm 非单位长度，非位置点）
    pub terminal_dir: [f32; 3],
}

/// Avatar method36 (0x24) 开火时刻瞄准快照（可选，无快照则 None）；args = [payloadLen u8][protobuf]。
/// field1(f64)=炮塔相对车体偏航（与 prop2 同语义）；开火时刻成对出现（射击前/后各一条，f1 恒同）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShooterAimData {
    pub turret_rel_yaw: f64,
    /// 成对快照 field6.field1 = 动态扩散 bloom（射击前；语义见 [`AimSnapshot::bloom`]）
    pub state_before: f64,
    /// 成对快照 field6.field1 = 动态扩散 bloom（射击后；无成对快照时 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_after: Option<f64>,
    /// root.field2 = 炮管俯仰（车体系 rad，WotbTools PROVEN）——作者俯仰权威来源（prop2 锚定缺失时的回退也走它）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gun_pitch: Option<f64>,
    /// type39 f0 世界系炮线 yaw（rad）——viewer 画真实 3D 炮线
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_gun_yaw: Option<f32>,
    /// type39 f1 世界系炮线 pitch（rad，存储取负，还原时取反）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world_gun_pitch: Option<f32>,
}

/// method36 (0x24) 单快照解码结果（WotbTools PROVEN 全字段；根标量均为 fixed64）。
#[derive(Debug, Clone, Default)]
pub struct AimSnapshot {
    /// root.field1 = 炮塔/炮管相对车体偏航（rad；与 prop2 同语义 f64 全精度）
    pub turret_rel_yaw: Option<f64>,
    /// root.field2 = 炮管俯仰（车体系 rad）
    pub gun_pitch: Option<f64>,
    /// root.field3 = 水平炮塔最大角速度（rad/s 车型常量）
    pub yaw_speed_limit: Option<f64>,
    /// root.field4 = 垂直炮管最大角速度（rad/s 车型常量；火炮受损 ×0.675，修复复原）
    pub pitch_speed_limit: Option<f64>,
    /// root.field5 = 瞄准时间物理标量（Reticle Calibration 恰 ×0.70）
    pub aim_time: Option<f64>,
    /// field6.field1 = 动态扩散/开花 bloom（开火后正跳；火炮受损 ×2）
    pub bloom: Option<f64>,
}

/// Avatar method36 (0x24) 瞄准快照序列（envelope = avatar = 录像者本人）；
/// args = [len u8][protobuf]；布局不合法的包 fail-soft 跳过。
pub fn collect_aim_snapshots(packets: &[(u32, f32, &[u8])]) -> Vec<(f32, AimSnapshot)> {
    let mut out = Vec::new();
    for (_, clock, p) in packets {
        if p.len() < 14 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x24 {
            continue;
        }
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        if args_len < 2 || 12 + args_len > p.len() {
            continue;
        }
        let snap = parse_method36(&p[12..12 + args_len]);
        if snap.turret_rel_yaw.is_some() && snap.bloom.is_some() {
            out.push((*clock, snap));
        }
    }
    out
}

/// 解析 method36 args → [`AimSnapshot`]。初始化变体（payloadLen=73）缺 root.field1/field2，
/// 相应字段为 None；格式不合法返回全 None（fail-soft，调用方忽略）。
fn parse_method36(args: &[u8]) -> AimSnapshot {
    let mut out = AimSnapshot::default();
    if args.is_empty() {
        return out;
    }
    // args[0] = payload 长度前缀（= args.len()-1），容错取 min
    let end = (args[0] as usize + 1).min(args.len());
    let payload = &args[1..end];
    let fields = match proto_fields(payload) {
        Some(f) => f,
        None => return out,
    };
    let fixed64 = |b: &[u8], f: &(u32, u8, usize, usize)| {
        f64::from_le_bytes(b[f.2..f.2 + 8].try_into().unwrap())
    };
    for f in &fields {
        if f.1 != 1 {
            continue;
        }
        match f.0 {
            1 => out.turret_rel_yaw = Some(fixed64(payload, f)),
            2 => out.gun_pitch = Some(fixed64(payload, f)),
            3 => out.yaw_speed_limit = Some(fixed64(payload, f)),
            4 => out.pitch_speed_limit = Some(fixed64(payload, f)),
            5 => out.aim_time = Some(fixed64(payload, f)),
            _ => {}
        }
    }
    out.bloom = fields.iter().find(|f| f.0 == 6 && f.1 == 2).and_then(|f| {
        let sub = &payload[f.2..f.2 + f.3];
        proto_fields(sub)?
            .iter()
            .find(|s| s.0 == 1 && s.1 == 1)
            .map(|s| fixed64(sub, s))
    });
    out
}

/// method38 (0x26) 命中反馈（作者 Avatar 专属）：args 布局（WotbTools PROVEN）：
/// [victimVehicleId u32][resultFlags16 u16][headerHi16 u16][resultCount u8][resultCount × (componentToken u8 + rawState u8)]
/// [modifierCount u8][modifierCount × modifierId u32]；rawState（WotbTools PROVEN）：1=受损或乘员受伤
/// 2=critical/禁用族（"摧毁"过强，是否=摧毁 PARTIAL）0=命中/参与但无新持久负面（"无变化"过强）；
/// modifierId 加性叠加可并存 [1,2]；组件号 31=引擎 32=弹药架 33=油箱 34/35=右/左履带 36=火炮
/// 37=炮塔旋转机 38=观察装置 39=车长 40=驾驶员 41=炮手 42=UNKNOWN 禁猜 43=装填手（WotbTools 枚举）
#[derive(Debug, Clone)]
pub(crate) struct HitFeedback {
    t: f32,
    victim: u32,
    flags: u32,
    crit_modules: u32,
    destroyed_modules: u32,
    components: Vec<(u8, u8)>,
    modifiers: Vec<u32>,
}

/// 两条射击提取路径（作者严格 / 他人宽松）的共享预分析产物：一次收集、两路复用，
/// `ReplayModel::scan` 亦从此取位姿/血量/名字索引，整场分析只此一份。
pub(crate) struct ShotScanShared {
    /// 全部 Shot 的 primary method29（作者 + 他人；按发射时刻排序，shotId 全局去重）。
    /// unique shotId = 一次开火；同 shotId 的后续 method29 不增加 Shot 数。
    pub launches: Vec<LaunchEntry>,
    /// 每射手首个 args<37 包的 args_len（作者路径 fail-fast 证据；正常回放为空表）
    pub short_args: HashMap<u32, usize>,
    /// method20 弹道终点（shotId 配对，含 miss 的空地终点）
    pub endpoints: HashMap<u32, (f32, [f32; 3])>,
    /// **命中通知归属区间**（shotId → `[本发弹着时刻, 同射手下一发弹着时刻)`；末发上界 +∞）。
    /// 弹着时刻 = [`shot_impact_time`]（有续段 = 首个接触时刻）。method8 / type=32 都不带
    /// shotId（P2 探针 0/457、0/2729），服务器自己的终点包是唯一可用的结构锚：一条命中
    /// 通知不可能早于本发弹着，也不会晚到同射手下一发弹着之后。**纯顺序语义、无时钟容差**，
    /// 取代此前的 ±0.05s 窗——该窗小于 10Hz 包钟量子（≈81ms），一跳 tick 即判“未转发”
    /// （2026-10-29 287 伤害 HE 样本：通知晚 +0.0811s）。
    pub notice_windows: HashMap<u32, (f32, f32)>,
    /// method8 直击通知（全局广播；按时钟排序）
    pub direct_hits8: Vec<DirectHit8>,
    /// type=32 命中通知（AoI 广播含他人；保持包序——hash6 令牌配对与顺序无关）
    pub warnings32: Vec<ArenaWarning32>,
    /// 0x1b 地形命中（shotId 配对，含全部玩家脱靶弹）
    pub terrain_impacts: HashMap<u32, (u32, TerrainImpactData)>,
    /// type=5 昵称表（eid → 昵称）
    pub names: HashMap<u32, String>,
    /// Type5 物化 9 字节配件选择
    pub vehicle_equipment: HashMap<u32, [u8; 9]>,
    /// type=35 tick 计数器（保持包序）
    pub tick_timeline: Vec<(f32, u8)>,
    /// type=36 会话十分秒计数全值 (clock, value)——[`tick_at_anchored`] 的绝对刻度锚
    /// （每场至多 1 条；缺省 None = 无锚，展开回退启发式）。全语料 31/31 场低字节
    /// 与 t35 起始值一致（investigation/client-usage-vs-agent-implementation.md）。
    pub session_tick: Option<(f32, u32)>,
    /// per-entity type=10 状态采样（**已按 clock 稳定排序**——锚点选择与 roll 线性插值依赖时序）
    pub st10: HashMap<u32, Vec<St10Sample>>,
    /// per-entity prop2 打包角流（**已按 clock 稳定排序**，等钟保持包序——prop2_at 的
    /// 夹逼插值语义本就假设时钟序，排序同时让二分查找成立）
    pub prop2: HashMap<u32, Vec<(f32, f32, u16)>>,
    /// type=7 刷新簇时钟（AoI 补发/通道切换签名）
    pub refresh_clusters: HashMap<u32, Vec<f32>>,
    /// method1 血量事件（全实体、按时钟排序）
    pub hp_events: Vec<HpEvent>,
    /// type=5 满血锚点（血量链 seed）
    pub initial_hp: HashMap<u32, (f32, u16)>,
    /// AoI 在场段（D1 段化滤波：段边界 + Type5 物化快照种子；render_cache 构建消费）
    pub aoi_presence: Vec<AoiPresence>,
    // —— 以下为作者路径（Avatar 专属包）数据，他人路径不消费 ——
    /// method38 命中结果（同钟同受击者合并后，按时钟排序）
    pub hit_results38: Vec<HitFeedback>,
    /// type=28 弹药槽位选择时间线（按时钟排序）
    pub ammo_selects: Vec<(f32, u32)>,
    /// method 0x07 弹种广播时间线（按时钟排序；a0=0/1 恒成对同值，a0=18 非 弹种数据已排除）
    pub shell_broadcasts: Vec<(f32, u32)>,
    /// method36 瞄准快照（布局合法帧，按时钟排序）
    pub aim_snapshots: Vec<(f32, AimSnapshot)>,
    /// 作者俯仰回退序列（method36 field2 车体系俯仰，rad）——prop2/锚定缺失时的俯仰源
    pub aim_pitch_series: Vec<(f32, f32)>,
    /// type=39 作者瞄准/炮线帧（世界系 yaw/pitch）
    pub type39_frames: Vec<Type39Frame>,
    /// 作者伤害计数器（type=7 sub=10）增量序列（已剔除非弹伤害污染 tick）
    pub dc_increments: Vec<(f32, u32)>,
}

/// 一次包扫描构建 [`ShotScanShared`]（各收集器独立成 pass，语义与原逐路径收集逐位一致；
/// `author_player_eid` 仅用于 Avatar 专属流的作者过滤，0 = 未解析作者）。
pub(crate) fn build_shot_scan_shared(
    packets: &[(u32, f32, &[u8])],
    author_player_eid: u32,
) -> ShotScanShared {
    let (launches, short_args) = collect_launches(packets, |_| true);
    let endpoints = collect_endpoints(packets);
    let direct_hits8 = collect_direct_hits8(packets);
    let warnings32 = collect_warnings32(packets);
    let terrain_impacts = collect_terrain_impacts(packets);
    let names = extract_entity_names(packets);
    let vehicle_equipment = collect_vehicle_equipment(packets);
    let tick_timeline = collect_tick_timeline(packets);
    let session_tick = collect_session_tick(packets);
    let (mut st10, mut prop2) = build_entity_indexes(packets);
    for v in st10.values_mut() {
        v.sort_by(|a, b| a.clock.total_cmp(&b.clock));
    }
    for v in prop2.values_mut() {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    let refresh_clusters = collect_refresh_clusters(packets);
    let hp_events = parse_hp_events(packets);
    let initial_hp = collect_initial_hp(packets);
    let aoi_presence = collect_aoi_lifecycle(packets);

    // ③ method38 命中结果（Avatar 方法 = 仅作者自己的射击反馈）
    let mut hit_results: Vec<HitFeedback> = Vec::new();
    for (_, clock, p) in packets {
        if p.len() < 12 + 9 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x26 {
            continue;
        }
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        if args_len < 9 || 12 + args_len > p.len() {
            continue;
        }
        let a = &p[12..12 + args_len];
        let mut crit_modules = 0u32;
        let mut destroyed_modules = 0u32;
        let mut components: Vec<(u8, u8)> = Vec::new();
        let mut off = 9usize;
        for _ in 0..a[8] {
            if off + 2 > args_len {
                break;
            }
            let (tok, state) = (a[off], a[off + 1]);
            components.push((tok, state));
            let bit = (tok as u32).checked_sub(31).map(|b| 1u32 << b).unwrap_or(0);
            match state {
                1 => crit_modules |= bit,
                2 => destroyed_modules |= bit,
                _ => {}
            }
            off += 2;
        }
        let mut modifiers: Vec<u32> = Vec::new();
        if off < args_len {
            let mcount = a[off];
            off += 1;
            for _ in 0..mcount {
                if off + 4 > args_len {
                    break;
                }
                modifiers.push(u32::from_le_bytes([
                    a[off],
                    a[off + 1],
                    a[off + 2],
                    a[off + 3],
                ]));
                off += 4;
            }
        }
        hit_results.push(HitFeedback {
            t: *clock,
            victim: u32::from_le_bytes([a[0], a[1], a[2], a[3]]),
            // 完整位图 = flags16 | headerHi<<16（wotinspector hit_flags 同源）
            flags: u32::from_le_bytes([a[4], a[5], a[6], a[7]]),
            crit_modules,
            destroyed_modules,
            components,
            modifiers,
        });
    }
    hit_results.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap());
    // ③' 同钟同受击者合并：一发命中可产生多条结果消息（多次装甲交互，同钟重复计为一次命中事件）；
    // 位图取并集；组件按 token 取最大 state；modifiers 去重合并。
    let mut merged38: Vec<HitFeedback> = Vec::new();
    for h in hit_results {
        if let Some(last) = merged38.last_mut() {
            if (last.t - h.t).abs() <= 0.05 && last.victim == h.victim {
                last.flags |= h.flags;
                last.crit_modules |= h.crit_modules;
                last.destroyed_modules |= h.destroyed_modules;
                for (tok, st) in h.components {
                    match last.components.iter_mut().find(|(t, _)| *t == tok) {
                        Some(e) => e.1 = e.1.max(st),
                        None => last.components.push((tok, st)),
                    }
                }
                for m in h.modifiers {
                    if !last.modifiers.contains(&m) {
                        last.modifiers.push(m);
                    }
                }
                continue;
            }
        }
        merged38.push(h);
    }

    // ⑤' 弹药选择时间线（type=28，payload=u32 LE 槽位；录像者本人的选择状态）
    let mut ammo_selects: Vec<(f32, u32)> = Vec::new();
    for (t, clock, p) in packets {
        if *t != 28 || p.len() < 4 {
            continue;
        }
        ammo_selects.push((*clock, u32::from_le_bytes([p[0], p[1], p[2], p[3]])));
    }
    ammo_selects.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());

    // ⑤'' Avatar method 0x07 弹种广播时间线：args(5) = [a0 u8][shell_global_id u32 LE]。
    // 命中通知未转发时（含脱靶弹）以此兜底。
    let mut shell_broadcasts: Vec<(f32, u32)> = Vec::new();
    for (_, clock, p) in packets {
        if p.len() < 17 {
            continue;
        }
        if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 0x07 {
            continue;
        }
        let args_len = u32::from_le_bytes([p[8], p[9], p[10], p[11]]) as usize;
        if args_len < 5 || 12 + args_len > p.len() {
            continue;
        }
        if p[12] != 0 && p[12] != 1 {
            continue;
        }
        shell_broadcasts.push((*clock, u32::from_le_bytes([p[13], p[14], p[15], p[16]])));
    }
    shell_broadcasts.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());

    // ⑤'''' Avatar method36 (0x24) 瞄准快照时间线（envelope = avatar = 录像者本人）；
    // args = [len u8][protobuf]，开火时刻成对；布局不合法的包 fail-soft 跳过（解码见 AimSnapshot）。
    let mut aim_snapshots = collect_aim_snapshots(packets);
    aim_snapshots.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let aim_pitch_series: Vec<(f32, f32)> = aim_snapshots
        .iter()
        .filter_map(|(c, s)| s.gun_pitch.map(|p| (*c, p as f32)))
        .collect();
    let type39_frames = collect_type39_frames(packets);

    // 作者伤害计数器（type=7 sub=10）增量序列——首次命中（血量链无前值）兜底；
    // 撞击/火伤等非弹伤害增量与 method1 cause≠0 且涉及作者的事件同批剔除
    let mut non_shell_ticks: Vec<f32> = hp_events
        .iter()
        .filter(|e| {
            e.cause != 0 && (e.source == author_player_eid || e.victim == author_player_eid)
        })
        .map(|e| e.clock)
        .collect();
    non_shell_ticks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut dc_increments: Vec<(f32, u32)> = Vec::new();
    {
        let mut last_cum: u32 = 0;
        for (t, clock, p) in packets {
            if *t != 7 || p.len() < 16 {
                continue;
            }
            if u32::from_le_bytes([p[4], p[5], p[6], p[7]]) != 10 {
                continue;
            }
            let cum = u32::from_le_bytes([p[12], p[13], p[14], p[15]]);
            if cum > last_cum {
                // 二分判定 |clock - tc| <= 0.3 的污染 tick（non_shell_ticks 已排序）
                let lo = non_shell_ticks.partition_point(|tc| *tc < clock - 0.3);
                let polluted = lo < non_shell_ticks.len() && non_shell_ticks[lo] <= clock + 0.3;
                if !polluted {
                    dc_increments.push((*clock, cum - last_cum));
                }
                last_cum = cum;
            }
        }
    }

    let (launch_is_cont_all, segments_all) = shot_segments(&launches);
    let notice_windows = build_notice_windows(
        &launches,
        &launch_is_cont_all,
        &segments_all,
        &endpoints,
    );

    ShotScanShared {
        launches,
        short_args,
        endpoints,
        notice_windows,
        direct_hits8,
        warnings32,
        terrain_impacts,
        names,
        vehicle_equipment,
        tick_timeline,
        session_tick,
        st10,
        prop2,
        refresh_clusters,
        hp_events,
        initial_hp,
        aoi_presence,
        hit_results38: merged38,
        ammo_selects,
        shell_broadcasts,
        aim_snapshots,
        aim_pitch_series,
        type39_frames,
        dc_increments,
    }
}

/// serde skip 助手：0 不序列化（未解析的弹种 id）
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// hash6 令牌的 hex 串形式（method8 ↔ type=32 配对日志与 quality 标注共用）。
fn hash6_hex(hash6: &[u8; 6]) -> String {
    hash6.iter().map(|b| format!("{:02x}", b)).collect()
}

/// 本发区间内**全部**命中事件的统一列表（[`ShotReplayData::hits`]）：
/// - 一发多 victim（HE 溅射同时命中多辆）与一发多次装甲交互（hash6 不同）都各成一条；
/// - 同一事件的重发（同刻同 hash6：未结算 + 已结算）合并，保留 settled；
/// - 主目标（= 顶层 target_eid）排首位，其余按「已结算 → 伤害大 → 时刻早」。
///
/// 段/装甲组按 hash6 令牌在 type=32 里配（全局，无时间窗）；未转发则缺省。
#[allow(clippy::too_many_arguments)]
fn collect_shot_hits(
    shooter: u32,
    win_lo: f32,
    win_hi: f32,
    primary_victim: Option<u32>,
    primary_damage: u32,
    primary_result: u8,
    direct_hits8: &[DirectHit8],
    warnings32: &[ArenaWarning32],
    hit_results38: &[HitFeedback],
    names: &HashMap<u32, String>,
) -> Vec<ShotHit> {
    let mut evs: Vec<&DirectHit8> = direct_hits8
        .iter()
        .filter(|d| d.shooter == shooter && d.t >= win_lo && d.t < win_hi)
        .collect();
    // 同事件重发合并（同刻、同 hash6）——保留 settled 的那条
    evs.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.hash6.cmp(&b.hash6)));
    evs.dedup_by(|a, b| a.hash6 == b.hash6 && (a.t - b.t).abs() < 1e-6);
    // **额外受击者必须与主事件同一服务器批次（同一包钟）**：一次开火的多目标命中（HE 溅射 /
    // 穿透后击中后车）由服务器在同一批结算；晚于主事件的通知属于**别的射击**（其发射/终点
    // 未录入 = AoI），贪心收进本发会错绑（语料实测 4 例：被收进来的"第二目标"距终点
    // 60~440m）。主事件缺失（无 method8）时不猜多目标，只用顶层字段。
    let primary_t = evs
        .iter()
        .find(|d| Some(d.victim) == primary_victim)
        .map(|d| d.t);
    if let Some(pt) = primary_t {
        evs.retain(|d| Some(d.victim) == primary_victim || (d.t - pt).abs() < 1e-6);
    } else {
        evs.clear();
    }
    // 主目标优先 → 已结算 → 伤害大 → 时刻早（确定性排序）
    evs.sort_by(|a, b| {
        let pa = (Some(a.victim) == primary_victim) as u8;
        let pb = (Some(b.victim) == primary_victim) as u8;
        pb.cmp(&pa)
            .then((b.settled as u8).cmp(&(a.settled as u8)))
            .then(b.damage.cmp(&a.damage))
            .then(a.t.total_cmp(&b.t))
    });
    let mut primary_taken = false;
    evs.into_iter()
        .map(|d| {
            let is_primary = !primary_taken && Some(d.victim) == primary_victim;
            if is_primary {
                primary_taken = true;
            }
            let warn = warnings32.iter().find(|w| w.hash6 == d.hash6);
            let m38 = hit_results38
                .iter()
                .filter(|h| h.victim == d.victim && (h.t - win_lo).abs() <= 0.6)
                .min_by(|a, b| (a.t - win_lo).abs().total_cmp(&(b.t - win_lo).abs()));
            ShotHit {
                target_eid: d.victim,
                target_name: names.get(&d.victim).cloned().unwrap_or_default(),
                // 主目标的伤害/结果与顶层同源（HP 链兜底值），额外目标用 method8 结算值
                damage: if is_primary {
                    primary_damage
                } else if d.settled {
                    u32::from(d.damage)
                } else {
                    0
                },
                game_hit_result: if is_primary { primary_result } else { d.result },
                is_primary,
                armor_group: warn.map(|w| decode_segment(w.segment).1),
                hit_token: Some(hash6_hex(&d.hash6)),
                server_part_index: d.component_index,
                crit_modules: m38.map(|h| h.crit_modules),
                destroyed_modules: m38.map(|h| h.destroyed_modules),
            }
        })
        .collect()
}

/// type=32 segment 布局解码：`[result][shell_global_id u24 LE（=(局部 id<<8)|国家基数）][00][X][Y][Z=armor_group]`。
/// 返回 (shell_id, armor_group, hit_triangle)；result 由调用方直接取 `segment.to_le_bytes()[0]`。
fn decode_segment(segment: u64) -> (u32, u8, u16) {
    let sb = segment.to_le_bytes();
    let shell_id = (sb[1] as u32) | ((sb[2] as u32) << 8) | ((sb[3] as u32) << 16);
    let hit_triangle = u16::from_be_bytes([sb[5], sb[6]]);
    (shell_id, sb[7], hit_triangle)
}

/// 命中通知归属区间（[`ShotScanShared::notice_windows`] 的构建）：
/// 每射手按**弹着时刻**排序，区间 = `[本发弹着, 同射手下一发弹着)`，末发上界 +∞。
/// 同刻并列（语料 0 例）退化为 1ms 窗并留给调用方按质量标记处理——不猜。
fn build_notice_windows(
    launches: &[LaunchEntry],
    launch_is_cont: &[bool],
    segments: &ShotSegments,
    endpoints: &HashMap<u32, (f32, [f32; 3])>,
) -> HashMap<u32, (f32, f32)> {
    let mut per_shooter: HashMap<u32, Vec<(f32, u32)>> = HashMap::new();
    for (i, l) in launches.iter().enumerate() {
        if launch_is_cont.get(i).copied().unwrap_or(false) {
            continue;
        }
        let Some(&(end_t, _)) = endpoints.get(&l.shot_id) else { continue };
        let impact = shot_impact_time(segments.get(&(l.shooter, l.shot_id)), end_t);
        per_shooter.entry(l.shooter).or_default().push((impact, l.shot_id));
    }
    let mut out: HashMap<u32, (f32, f32)> = HashMap::new();
    for list in per_shooter.values_mut() {
        list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        list.dedup_by_key(|x| x.1);
        for i in 0..list.len() {
            let lo = list[i].0;
            let hi = list.get(i + 1).map(|x| x.0).unwrap_or(f32::INFINITY);
            // 同刻并列：区间退化时给 1ms 尾窗（不把后续通知吞进本发）
            let hi = if hi > lo { hi } else { lo + 1e-3 };
            out.insert(list[i].1, (lo, hi));
        }
    }
    out
}

/// 弹的"抵达时刻"：有续段（跳弹/穿透）时取首个续段时刻（与 method8 同刻），否则取终点
/// 时刻。命中归属、受击方状态快照、prop2 取样窗都以此为基准（作者/他人两路共用）。
fn shot_impact_time(chain: Option<&ShotSegmentChain>, end_time: f32) -> f32 {
    chain
        .and_then(|c| c.first())
        .map(|c| c.0)
        .unwrap_or(end_time)
}

/// 受击者→射手的来向方位角（作者/他人两路共用）；任一端无效（零向量）时 None。
fn bearing_shooter_to_target(tp: [f32; 3], sp: [f32; 3]) -> Option<f32> {
    if tp != [0.0; 3] && sp != [0.0; 3] {
        Some((sp[0] - tp[0]).atan2(sp[2] - tp[2]))
    } else {
        None
    }
}

/// aim_point / launch_point_rel = 相对【命中通知状态】目标位置（type10 接地高度）的偏移；
/// 无有效终点或无目标时归零（作者/他人两路共用）。
fn aim_point_rels(
    ball_b: [f32; 3],
    ball_a: [f32; 3],
    tp: [f32; 3],
    has_target: bool,
) -> ([f32; 3], [f32; 3]) {
    if ball_b != [0.0; 3] && has_target {
        let rel = |p: [f32; 3]| [p[0] - tp[0], p[1] - tp[1], p[2] - tp[2]];
        (rel(ball_b), rel(ball_a))
    } else {
        ([0.0; 3], [0.0; 3])
    }
}

/// 命中通知里**不属于任何可归属射击事件**的那些（该发的发射/终点未录入 = AoI 截断）。
/// 这类事件仍有服务器证据（受击者/伤害/结果/令牌），不能因为"找不到对应 shot"就丢掉：
/// 由 `shot_replays_json` 以独立 `unattached_hits` 输出，消费方按"未知归属"呈现。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnattachedHit {
    pub shooter_eid: u32,
    /// 射手昵称（type=5 名册；method8 自带 shooter eid，名字与目标同域）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shooter_name: String,
    pub victim_eid: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub victim_name: String,
    pub t: f32,
    /// 服务器结算伤害（settled；未结算 = 0）
    pub damage: u32,
    pub game_hit_result: u8,
    /// 命中事件令牌（hash6 hex；与 method8/type=32 同源，可单独解码 P1/P2 两点）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit_token: Option<String>,
    /// 服务器下发的受击部件索引（method8 cmpIndex：0=底盘/履带 1=车体 2=炮塔 3=炮管）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_part_index: Option<u8>,
    /// 同事件 type=32 的 segment（`[result][shell_global_id u24][00][X][Y][Z=plate]`）；
    /// 未转发（cmpIndex=0 不发本族包 / 服务器只转发部分命中）则缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment: Option<u64>,
    /// 段包里解出的全局弹种 id（0 = 未知）；配合 shellKinds 表可还原弹种
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub shell_id: u32,
    /// 段包 B7 = 命中装甲板 plateId
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor_group: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit_triangle: Option<u16>,
    /// 事件时刻受击者的 type=10 位姿（渲染命中标记/局部判定可独立于射手使用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victim_pos: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victim_ang: Option<[f32; 3]>,
    /// 受击者炮塔绝对偏航（rad，与 `victim_ang[0]` 同参考系；prop2 高 10 位 + 车体偏航）——
    /// 受击者姿态的一部分：装甲判定/渲染必须转到命中时刻的炮塔角
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victim_turret_yaw: Option<f32>,
    /// prop2 原始 u16（高 10 位炮塔相对角 / 低 6 位炮管俯仰 frac）——俯仰需车型极限，
    /// 核心层不持有，原样透传给有 asset 的消费方自行解码
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victim_prop2: Option<u16>,
    /// 该事件未落进任何区间的原因：`no_shot_interval`（录像中无该发）
    pub reason: String,
}

/// 收集无法归属到任何 shot 的命中通知（直接吃原始包：内部重建一次共享扫描）。
/// 语义：同射手的所有 `[弹着, 下一发弹着)` 区间都不包含它 = 对应的射击不存在于录像。
/// 同事件重发（同刻同 hash6）合并，保留 settled。
pub fn collect_unattached_hits(packets: &[(u32, f32, &[u8])]) -> Vec<UnattachedHit> {
    collect_unattached_hits_from_shared(&build_shot_scan_shared(packets, 0))
}

/// [`collect_unattached_hits`] 的共享扫描形态（射击 bundle 一次扫描三路复用）。
/// 只消费非作者过滤的流，任何 author_eid 下输出逐位一致。
pub(crate) fn collect_unattached_hits_from_shared(shared: &ShotScanShared) -> Vec<UnattachedHit> {
    let mut per_shooter: HashMap<u32, Vec<(f32, f32)>> = HashMap::new();
    for l in &shared.launches {
        if let Some(w) = shared.notice_windows.get(&l.shot_id) {
            per_shooter.entry(l.shooter).or_default().push(*w);
        }
    }
    let mut evs: Vec<&DirectHit8> = shared
        .direct_hits8
        .iter()
        .filter(|d| {
            !per_shooter
                .get(&d.shooter)
                .map(|ws| ws.iter().any(|(lo, hi)| d.t >= *lo && d.t < *hi))
                .unwrap_or(false)
        })
        .collect();
    evs.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.hash6.cmp(&b.hash6)));
    evs.dedup_by(|a, b| a.hash6 == b.hash6 && (a.t - b.t).abs() < 1e-6);
    evs.into_iter()
        .map(|d| {
            // 段包：同 hash6 的 type=32（同事件令牌；与时间无关）
            let warn = shared.warnings32.iter().find(|w| w.hash6 == d.hash6);
            let (shell_id, armor_group, hit_triangle) = match warn {
                Some(w) => {
                    let (sh, ag, ht) = decode_segment(w.segment);
                    (sh, Some(ag), Some(ht))
                }
                None => (0, None, None),
            };
            UnattachedHit {
                shooter_eid: d.shooter,
                shooter_name: shared.names.get(&d.shooter).cloned().unwrap_or_default(),
                victim_eid: d.victim,
                victim_name: shared.names.get(&d.victim).cloned().unwrap_or_default(),
                t: d.t,
                damage: if d.settled { u32::from(d.damage) } else { 0 },
                game_hit_result: d.result,
                hit_token: Some(hash6_hex(&d.hash6)),
                server_part_index: d.component_index,
                segment: warn.map(|w| w.segment),
                shell_id,
                armor_group,
                hit_triangle,
                victim_pos: d.victim_state.map(|(p, _, _)| p),
                victim_ang: d.victim_state.map(|(_, a, _)| a),
                victim_turret_yaw: d.victim_state.and_then(|(_, ang, _)| {
                    d.victim_prop2.map(|(_, raw)| {
                        let yaw = decode_prop2_u16(raw).0 + ang[0];
                        (yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                            - std::f32::consts::PI
                    })
                }),
                victim_prop2: d.victim_prop2.map(|(_, raw)| raw),
                reason: "no_shot_interval".into(),
            }
        })
        .collect()
}

/// 作者路径 type=32 选择：method38 先确定 victim；一发可在同一 victim 上产生**多次装甲交互**
/// （method8/type=32 各多条，hash6 互不相同）。区间内按**主交互**取段：优先
/// 已结算且伤害最大的 method8 的 hash6，其次任一 hash6 命中的 type=32；无 hash 可用时
/// 仅当区间内 segment 全同才采信，否则 fail-closed（不猜选）。
fn select_author_warning32<'a>(
    warnings32: &'a [ArenaWarning32],
    direct_hits8: &[DirectHit8],
    author_player_eid: u32,
    victim_eid: u32,
    win_lo: f32,
    win_hi: f32,
) -> Result<Option<&'a ArenaWarning32>, usize> {
    let window: Vec<&ArenaWarning32> = warnings32
        .iter()
        .filter(|w| w.eid == victim_eid && w.t >= win_lo && w.t < win_hi)
        .collect();
    if window.is_empty() {
        return Ok(None);
    }

    // 主交互排序：已结算优先 → 伤害大者优先 → 时刻早者优先（确定性，无阈值）
    let mut hashes: Vec<&DirectHit8> = direct_hits8
        .iter()
        .filter(|d| {
            d.shooter == author_player_eid
                && d.victim == victim_eid
                && d.t >= win_lo
                && d.t < win_hi
        })
        .collect();
    hashes.sort_by(|a, b| {
        (b.settled as u8, b.damage)
            .cmp(&(a.settled as u8, a.damage))
            .then(a.t.total_cmp(&b.t))
    });
    for d in &hashes {
        if let Some(w) = window.iter().copied().find(|w| w.hash6 == d.hash6) {
            return Ok(Some(w));
        }
    }
    // 有 method8（本发真值）但窗口内没有任何同 token 的 type=32 = 段包未转发
    // （cmpIndex=0 履带命中不发本族包；服务器只转发部分命中）→ 由 method8 兜底，
    // **不能**把窗口里别的（更晚事件/别的射手命中同一受击者）segment 当成歧义。
    if !hashes.is_empty() {
        return Ok(None);
    }

    // method8 也未转发：区间内 segment 全同（同一事件重发）才采信，否则 fail-closed
    let first = window[0];
    if window.iter().any(|w| w.segment != first.segment) {
        return Err(window.len());
    }
    Ok(Some(first))
}

// ---------- 作者/他人两路径的同构段共享实现 ----------
// 两条路径的本质差异只在"缺失时怎么办"（作者 bail / 他人跳过或兜底）；组装段收敛为
// 单份，时间窗常量（渲染受击 −3.0~+2.0 / 射手 −3.0~+2.0、炮塔角射手 −2.0~+2.0）集中在此。

/// 炮塔相对角：流序快照（method29/8 处理时刻，WI 同基准）优先 → 时钟序 prop2 兜底。
/// None = prop2 缺失（调用方按路径 bail 或降级为车体朝向）。
fn turret_rel_at(
    stream_snapshot: Option<(f32, u16)>,
    series: Option<&Vec<(f32, f32, u16)>>,
    t: f32,
) -> Option<f32> {
    match stream_snapshot {
        Some((_, v)) => Some(decode_prop2_u16(v).0),
        None => prop2_at_arrived(series, t).map(|(r, _)| r),
    }
}

/// prop2 frac 俯仰解码链（双方同构）：流序快照 × 车型锚定 → 时钟序 × 锚定。
/// None = prop2/锚定缺失——调用方按路径回退（作者射手回 method36 且仍缺失 fail-fast，
/// 他人射手回发射速度向量，受击方回车体 pitch），降级标记由调用方打。
fn pitch_from_prop2(
    stream_snapshot: Option<(f32, u16)>,
    series: Option<&Vec<(f32, f32, u16)>>,
    t: f32,
    limits: Option<&GunPitchRange>,
    pitch_frozen: &mut Vec<String>,
    who: &str,
) -> Option<f32> {
    if let Some(((_, v), lim)) = stream_snapshot.zip(limits) {
        let (y, fr) = decode_prop2_u16(v);
        if prop2_frac_frozen(series, t) {
            pitch_frozen.push(who.into());
        }
        return Some(decode_prop2_gun_pitch(fr, lim, y));
    }
    if let Some(((y, fr), lim)) = prop2_at_arrived(series, t).zip(limits) {
        if prop2_frac_frozen(series, t) {
            pitch_frozen.push(who.into());
        }
        return Some(decode_prop2_gun_pitch(fr, lim, y));
    }
    None
}

/// 单发射击的渲染层包（判定锚点 + 渲染锚点 + 四条时间线）。
struct ShotRenderPack {
    shooter_render: Option<RenderAnchorData>,
    target_render: Option<RenderAnchorData>,
    target_render_timeline: Vec<RenderTimelineSample>,
    shooter_render_timeline: Vec<RenderTimelineSample>,
    target_turret_timeline: Vec<(f32, f32)>,
    shooter_turret_timeline: Vec<(f32, f32)>,
    shooter_gun_timeline: Vec<(f32, f32)>,
    target_gun_timeline: Vec<(f32, f32)>,
}

/// 射手炮管俯仰时间线的无锚定回退：作者 = method36 field2 车体系俯仰序列；
/// 他人 = 空（发射速度向量只在开火帧存在，无时间线可言）。
enum ShooterGunFallback<'a> {
    Method36Series(&'a Vec<(f32, f32)>),
    None,
}

#[allow(clippy::too_many_arguments)] // 两路径组装的公共入参，聚合结构反而不透明
fn shot_render_pack(
    render_cache: &mut HashMap<u32, SegmentedPoseTimeline>,
    st10: &St10Index,
    prop2: &Prop2Index,
    aoi_presence: &[AoiPresence],
    shooter_eid: u32,
    fire_time: f32,
    shooter_judgment_pos: [f32; 3],
    skip_shooter: bool,
    target_eid: Option<u32>,
    hit: bool,
    end_time: f32,
    target_judgment_pos: [f32; 3],
    shooter_limits: Option<&GunPitchRange>,
    target_limits: Option<&GunPitchRange>,
    gun_fallback: ShooterGunFallback,
) -> ShotRenderPack {
    // 渲染层锚点（客户端位置滤波器）：滤波器输出 = 游戏画面里模型实际呈现的位姿；
    // 判定层锚点（调用方 sp/tp）保持不动——装甲命中几何必须用判定层。
    // 他人路径炮口兜底（pos_from_muzzle）时 sp 非车体位姿，跳过射手侧渲染数据。
    let shooter_render = if !skip_shooter {
        render_anchor(
            render_cache,
            shooter_eid,
            st10.get(&shooter_eid),
            aoi_presence,
            fire_time,
            shooter_judgment_pos,
        )
    } else {
        None
    };
    let target_render = if hit {
        target_eid.and_then(|teid| {
            render_anchor(
                render_cache,
                teid,
                st10.get(&teid),
                aoi_presence,
                end_time,
                target_judgment_pos,
            )
        })
    } else {
        None
    };
    // 渲染时间线（滑块严格对齐游戏每帧显示位姿，0.1s 步长）：受击方命中 −3.0~+2.0s、
    // 射手方开火 −3.0~+2.0s。命中后数据保留：滤波器误差盒钳位会渐进滑向通道切换后
    // 的真相，即游戏当时渲染的画面。
    let target_render_timeline = if hit {
        target_eid
            .map(|teid| {
                render_timeline(
                    render_cache,
                    teid,
                    st10.get(&teid),
                    aoi_presence,
                    end_time,
                    -3.0,
                    2.0,
                    0.1,
                )
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let shooter_render_timeline = if !skip_shooter {
        render_timeline(
            render_cache,
            shooter_eid,
            st10.get(&shooter_eid),
            aoi_presence,
            fire_time,
            -3.0,
            2.0,
            0.1,
        )
    } else {
        Vec::new()
    };
    // 炮塔/炮管实时时间线（prop2，客户端语义 0.1s 网格，与锚点同一求值器）：
    // 受击方命中 −3.0~+2.0s、射手方开火 −2.0~+2.0s
    let target_turret_timeline = if hit {
        target_eid
            .and_then(|teid| prop2.get(&teid))
            .map(|series| timeline_prop2_client(Some(series), end_time, -3.0, 2.0, None))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let shooter_turret_timeline =
        timeline_prop2_client(prop2.get(&shooter_eid), fire_time, -2.0, 2.0, None);
    let shooter_gun_timeline = match shooter_limits {
        Some(lim) => {
            timeline_prop2_client(prop2.get(&shooter_eid), fire_time, -2.0, 2.0, Some(lim))
        }
        None => match gun_fallback {
            ShooterGunFallback::Method36Series(series) => {
                timeline_1f(Some(series), fire_time, -2.0, 2.0)
            }
            ShooterGunFallback::None => Vec::new(),
        },
    };
    let target_gun_timeline = if hit {
        target_eid
            .and_then(|teid| prop2.get(&teid))
            .map(|series| timeline_prop2_client(Some(series), end_time, -3.0, 2.0, target_limits))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    ShotRenderPack {
        shooter_render,
        target_render,
        target_render_timeline,
        shooter_render_timeline,
        target_turret_timeline,
        shooter_turret_timeline,
        shooter_gun_timeline,
        target_gun_timeline,
    }
}

/// type=10 采样窗口（base_t ±1s，锚点/补发簇截断 + "游戏渲染位"合成）——
/// 受击方相对判定锚点（relative=true），射手世界系绝对坐标。
/// 窗口右端 +0.09 仅作锚点兜底：有真锚点（|dt|<0.05）时 trim 截断其后样本——
/// 命中/开火后数据包源切换（AoI 远端基线），其后首包含数米级瞬移；
/// 射手前窗取 −1.0s：过窄时靠前 tick 钳死在开火位置呈现"不动"假象。
#[allow(clippy::too_many_arguments)] // 两路径组装的公共入参（同 shot_render_pack）
fn tick_window_samples(
    st10: &St10Index,
    tick_timeline: &[(f32, u8)],
    session_tick: Option<(f32, u32)>,
    refresh_clusters: &HashMap<u32, Vec<f32>>,
    eid: u32,
    base_t: f32,
    anchor_pos: [f32; 3],
    relative: bool,
    render: Option<&RenderAnchorData>,
) -> Vec<TickSample> {
    let mut out = Vec::new();
    if let Some(samples) = st10.get(&eid) {
        for s in samples {
            let dt = s.clock - base_t;
            if !(-1.0..=0.09).contains(&dt) {
                continue;
            }
            let pos = if relative {
                [
                    s.pos[0] - anchor_pos[0],
                    s.pos[1] - anchor_pos[1],
                    s.pos[2] - anchor_pos[2],
                ]
            } else {
                s.pos
            };
            out.push(TickSample::raw(
                dt,
                tick_at_anchored(tick_timeline, session_tick, s.clock),
                pos,
                s.yaw,
                s.pitch,
                s.roll,
            ));
        }
    }
    trim_tick_samples(
        &mut out,
        refresh_cluster_after(refresh_clusters, eid, base_t),
    );
    if let Some(r) = render {
        let pos = if relative {
            [
                r.pos[0] - anchor_pos[0],
                r.pos[1] - anchor_pos[1],
                r.pos[2] - anchor_pos[2],
            ]
        } else {
            r.pos
        };
        out.push(TickSample::render_ghost(
            0.0, pos, r.ang[0], r.ang[1], r.ang[2],
        ));
    }
    out
}

/// 从射击事件抽取复现数据（WotbTools 权威弹丸生命周期，全部 shotId 确定性配对）：
/// method29 (0x1d) 发射 / method20 (0x14) 终点 / method38 (0x26) 命中结果（仅作者）；目标 = method38 victimVehicleId（服务器权威，无则 miss）；
/// 伤害 = method1 血量链逐事件差值（victim + source=作者 + cause=0，命中时刻与降幅
/// 事件时钟确定性配对）。数据完整性 fail-fast：任何缺失/歧义直接返回 Err，不做保守降级（零值掩盖问题）。
/// `pitch_limits` = 昵称→(俯角°,仰角°) 锚定表
/// （[`gun_pitch_limits_from`] 构建）。双方炮管俯仰主来源 = prop2 frac 比例解码；
/// 无锚定表时回退路径（作者 method36 field2 / 他人速度向量 / 受击方车体 pitch）并打质量标记。
pub fn extract_shot_replays_with_limits(
    packets: &[(u32, f32, &[u8])],
    author_player_eid: u32,
    pitch_limits: &GunPitchLimits,
) -> anyhow::Result<Vec<ShotReplayData>> {
    let shared = build_shot_scan_shared(packets, author_player_eid);
    let mut render_cache: HashMap<u32, SegmentedPoseTimeline> = HashMap::new();
    extract_shot_replays_from_shared(&shared, author_player_eid, pitch_limits, &mut render_cache)
}

/// [`extract_shot_replays_with_limits`] 的共享扫描形态：预分析产物由调用方一次构建
/// （[`build_shot_scan_shared`]），作者/他人两路复用；`render_cache`（滤波时间线，
/// 按实体确定）亦可跨路复用。
pub(crate) fn extract_shot_replays_from_shared(
    shared: &ShotScanShared,
    author_player_eid: u32,
    pitch_limits: &GunPitchLimits,
    render_cache: &mut HashMap<u32, SegmentedPoseTimeline>,
) -> anyhow::Result<Vec<ShotReplayData>> {
    if author_player_eid == 0 {
        anyhow::bail!("无法解析作者实体：meta.player_name / battle_results 昵称与 type=5 实体昵称匹配失败（作者实体未在录像中出现或昵称无效）");
    }

    // ① 作者的 method29 发射事件（共享发射表按 shooter 过滤）；args<37 = 布局漂移，fail-fast
    let launches: Vec<LaunchEntry> = shared
        .launches
        .iter()
        .filter(|l| l.shooter == author_player_eid)
        .cloned()
        .collect();
    if let Some(&args_len) = shared.short_args.get(&author_player_eid) {
        anyhow::bail!(
            "作者的 method29 发射包 args 长度 {} < 37（回放版本布局漂移？）",
            args_len
        );
    }

    // ①' 段链分组：同 (shooter, shotId) 的后续 method29 = 同一发的跳弹/穿透续段。
    // 发射段 = 链首（唯一进入组装），续段首点 = 命中/跳弹点（渲染终点），续段本身不是独立射击。
    let (launch_is_cont, segments) = collect::shot_segments(&launches);
    let launches: Vec<LaunchEntry> = launches
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !launch_is_cont[*i])
        .map(|(_, l)| l)
        .collect();

    // ② method20 弹道终点（shotId 配对；含 miss 的空地终点）
    let endpoints = &shared.endpoints;

    // ③ method38 命中结果（Avatar 方法 = 仅作者自己的射击反馈；共享收集 + 同钟同受击者合并，
    // 布局见 ShotScanShared.hit_results38 文档）
    let mut hit_results = shared.hit_results38.clone();

    // ③'' type=32 来袭炮弹警告/命中通知（eid = 受击者，AoI 广播含他人命中）；segment u64 低字节 = 命中结果枚举（与 method8 b9 同域）。
    // 共享表保持包序，此处按时钟排序供时间窗扫描（filter 保序）
    let mut warnings32 = shared.warnings32.clone();
    warnings32.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap());

    // ③''' Vehicle method8 直击通知（全局广播，envelope eid = 受击者）
    let direct_hits8 = &shared.direct_hits8;

    let names = &shared.names;

    // ④' type=7 刷新簇时钟（AoI 补发/通道切换签名）——tick 采样窗口截断依据
    let refresh_clusters = &shared.refresh_clusters;

    // ⑤'' type=35 服务器竞技场 tick 计数器（u8 递增 @10Hz，自然回绕）。
    let tick_timeline = &shared.tick_timeline;
    // type=36 会话十分秒全值锚（tick_at_anchored 的绝对刻度；缺记录时 None = 纯启发式）
    let session_tick = shared.session_tick;

    // ⑥ per-entity 索引（共享一次预建，st10/prop2 均已按 clock 排序）：type=10 状态采样 / type=7 prop2 炮塔偏航。
    let (st10, prop2) = (&shared.st10, &shared.prop2);
    // 射手（作者）俯仰解码锚定：昵称 → 基础 models.pb 极限；本实体实际携带 122 时
    // 叠加 +4°仰/+3°俯（证据 = 同 eid 的 Type5 `0B` 配件串）。无锚定时俯仰回退 method36 field2
    let author_name = names.get(&author_player_eid).cloned().unwrap_or_default();
    let shooter_limits = pitch_limits.get(&author_name).map(|base| {
        effective_pitch_limits(base, shared.vehicle_equipment.get(&author_player_eid))
    });
    let shooter_limits = shooter_limits.as_deref();

    // ⑤' 弹药选择时间线（type=28，payload=u32 LE 槽位；录像者本人的选择状态）
    let ammo_selects = &shared.ammo_selects;

    // ⑤'' Avatar method 0x07 弹种广播时间线：命中通知未转发（含脱靶弹）时的弹种兜底。
    let shell_broadcasts = &shared.shell_broadcasts;

    // ⑤''' Avatar method 0x1b 地形命中包（仅无坦克命中时广播）：shotId 配对；全局广播含
    // 所有玩家脱靶弹，args[4..8] 的 shell_global_id 是弹种兜底链第三级数据源。
    let terrain_impacts = &shared.terrain_impacts;

    // ⑤'''' Avatar method36 瞄准快照 + 作者俯仰回退序列（method36 field2 车体系俯仰）；
    // type=39 世界系炮线帧（仅开火时刻锚定消费，死亡后旋转不适用）
    let aim_snapshots = &shared.aim_snapshots;
    let aim_pitch_series = &shared.aim_pitch_series;
    let type39_frames = &shared.type39_frames;

    // method1 血量事件（全实体、按时钟排序）——降幅推导输入
    // Type5 物化 9 字节配件选择：射击双方搭载注入（viewer 自动穿深/厚度系数）
    let vehicle_equipment = &shared.vehicle_equipment;
    // ⑥' 确定性伤害降幅区间（WotbTools deriveLosses 同款；仅作者造成的 cause=0 炮弹直击降幅）
    // type=5 满血锚点补链：受害者首个 method1 已是掉血后血量时，首刀降幅才可归属
    let dmg_losses = derive_dmg_losses(
        &shared.hp_events,
        Some(author_player_eid),
        &shared.initial_hp,
    );
    // 降幅互斥消费标记（一段降幅只归属一发，防同区间多发重复计数）
    let mut dmg_losses_used: std::collections::HashSet<usize> = std::collections::HashSet::new();
    // 作者伤害计数器增量序列（共享收集，已剔除非弹伤害污染 tick）
    let dc_increments = &shared.dc_increments;
    // 计数器增量按**时钟**消费（命中时钟 == 增量时钟），不再按顺序游标——顺序游标
    // 在"命中存在但无 HP 掉血"（未穿/模块-only）的条目上会错位，把下一发的增量
    // 提前消费（实测 2045：#2 结果 2 无掉血却吞掉 #4 的 604）。
    let mut dc_used: std::collections::HashSet<usize> = std::collections::HashSet::new();
    // method8 命中通知消费标记（作者路径）：通知按 (shooter=作者, victim) 认领，
    // 未消费最早一条 ≥ end_time——method8 的 t 才是真实命中时刻（与 method1 同 tick
    // 逐位相等）；method38/method29续段/method20 都带**射击作用域时钟**（= 开火时刻，
    // 实测有飞行时间时与命中时刻差 0.28s），不能当伤害配对键。
    let mut d8_used: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut hr_cursor = 0usize; // method38 消费游标（按时间顺序，逐发消费）

    // ⑦ 逐发处理（fail-fast：任何数据缺失/歧义直接报错）
    let mut out: Vec<ShotReplayData> = Vec::with_capacity(launches.len());
    // render_cache 由调用方传入（作者/他人两路共享——滤波时间线按实体确定）
    for (i, l) in launches.iter().enumerate() {
        let fire_time = l.t;
        let shot_id = l.shot_id;
        // ctx 仅在报错路径格式化（避免逐发无条件分配）
        let ctx = || format!("shot #{} (shotId={}, t={:.2}s)", i + 1, shot_id, fire_time);

        let (sp, sa, sp_dt, sp_src) = select_anchor_state(
            st10.get(&author_player_eid)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            refresh_clusters,
            author_player_eid,
            fire_time,
        )
        .ok_or_else(|| anyhow::anyhow!("{}: 射手 type=10 状态快照缺失", ctx()))?;

        // 弹道终点（shotId 精确配对）；ball_a = method29 炮口发射位置
        let ball_a = l.point;
        // 弹道拆段（确定性）：续段表给出跳弹/出射点及其段速度；终点 = method20（服务器最终停止点）。
        // 命中归属用的时刻 = **首个续段时刻**（= 装甲接触时刻，与 method8 同刻）——不能等到终点时刻，
        // 跳弹后还飞了 ~1s。无续段时命中时刻 = 终点时刻（直线弹，原语义）。
        let chain = segments.get(&(l.shooter, shot_id));
        let (end_time, ball_b) = endpoints
            .get(&shot_id)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("{}: method20 弹道终点缺失（shotId 无配对）", ctx()))?;
        // `end_time` 语义 = **炮弹抵达目标/落点的时刻**：有续段（跳弹/穿透）时取首个续段时刻
        // （与 method8 同刻），否则取终点时刻。命中归属、受击方状态快照、prop2 取样窗都以此为基准。
        let end_time = shot_impact_time(chain, end_time);
        // 命中通知（method8/type=32）归属区间：`[本发弹着, 同射手下一发弹着)`。
        // 无 shotId 的通知族只能靠这条顺序语义绑定（零时钟容差）。窗口必然在位：
        // 本循环只处理非续段发射（shot_segments 过滤），且终点缺失已在上面 bail——而
        // [`build_notice_windows`] 恰为"每条非续段 + 有终点"的发射建窗。查不到 =
        // 共享扫描构建/消费两侧不变量被破坏（代码缺陷），fail-visible 上抛，不静默退化。
        let Some(&(win_lo, win_hi)) = shared.notice_windows.get(&shot_id) else {
            anyhow::bail!(
                "{}: 非续段发射缺少命中通知窗口（build_notice_windows 不变量破坏）",
                ctx()
            );
        };
        let in_notice_window = |t: f32| t >= win_lo && t < win_hi;
        let (via, leg_secs) = shot_legs(l.point, l.vel, chain, ball_b);
        let fire_tick = tick_at_anchored(tick_timeline, session_tick, fire_time);

        // ⑦' 弹药槽位：发射时刻的最后选择（type=28 时间线 ≤ fire_time 的最新值）
        let mut shell_slot: u32 = 0;
        for (t_sel, slot) in ammo_selects.iter() {
            if *t_sel <= fire_time {
                shell_slot = *slot;
            } else {
                break;
            }
        }

        // ⑧ 目标实体：method38 victimVehicleId（服务器权威，确定性）
        let mut target_eid: Option<u32> = None;
        let mut damage = 0u32;
        let mut target_name = String::new();
        let mut is_kill = false;
        let mut hit = false;
        let mut target_dead = false;
        let mut hit_flags: u32 = 0;
        let mut crit_modules: u32 = 0;
        let mut destroyed_modules: u32 = 0;
        let mut modifiers: Vec<u32> = Vec::new();

        let mut matched: Option<usize> = None;
        let mut cands = 0usize;
        for (j, hr) in hit_results.iter().enumerate().skip(hr_cursor) {
            if hr.t > end_time + 0.5 {
                break;
            } // 已排序，越过窗口即止
            if (hr.t - end_time).abs() < 0.5 && hr.t >= end_time - 0.05 {
                cands += 1;
                if matched.is_none() {
                    matched = Some(j);
                }
            }
        }
        if cands > 1 {
            anyhow::bail!(
                "{}: method38 配对歧义——命中窗口内出现 {} 条命中结果",
                ctx(),
                cands
            );
        }
        if let Some(j) = matched {
            let hr = &mut hit_results[j];
            target_eid = Some(hr.victim);
            hit_flags = hr.flags;
            crit_modules = hr.crit_modules;
            destroyed_modules = hr.destroyed_modules;
            // HitFeedback 恰被消费一次（hr_cursor 单调前进），modifiers 直接移走避免克隆
            modifiers = std::mem::take(&mut hr.modifiers);
            hit = true;
            hr_cursor = j + 1;
        }

        if let Some(teid) = target_eid {
            // fail-soft：名字是显示域，缺失（受击方昵称损坏等）不中止整条作者射击链——
            // 保留 shot，target_name 空串 + target_eid 照传（身份/弹道/判定不受影响）
            target_name = names.get(&teid).cloned().unwrap_or_default();
        }
        // 作者 method38 的 TARGET_ALREADY_DEAD 位是服务器权威“打残骸”信号；HP 终态早于
        // 开火是等价证据（method38 缺失/未转发时兜底）。与其他人路径同语义：按 miss 处理，
        // 不发布活目标、不进入伤害/穿深链，quality.target_dead 标记残骸命中。
        if let Some(teid) = target_eid {
            if hit_flags & hit_flags_mod::TARGET_ALREADY_DEAD != 0
                || victim_dead_before(&shared.hp_events, teid, fire_time)
            {
                target_dead = true;
                hit = false;
                target_eid = None;
                target_name.clear();
            }
        }

        // ⑧' 游戏原生命中段 + 结果枚举（wotinspector segment 对齐）：type=32 警告包优先（segment 低字节=结果枚举），
        // method8 直击通知兜底；两者 hash6 命中令牌一致。歧义 fail-fast；服务器未转发时 segment=0 / result=255。
        let mut segment: u64 = 0;
        let mut shell_id: u32 = 0;
        let mut armor_group: u8 = 0;
        let mut hit_triangle: u16 = 0;
        let mut game_hit_result: u8 = 255;
        let mut hit_token: Option<String> = None;
        let mut bound_warning: Option<&ArenaWarning32> = None;
        if let Some(teid) = target_eid {
            // 一次 Shot 可以在同一 victim/同钟产生多个装甲交互；只按 victim+time 会把
            // 合法的不同 type=32 segment 误判为歧义。作者路径与他人路径统一优先使用
            // method8.hash6 ↔ type32.hash6 的确定性事件令牌；证据不足时仍 fail-fast。
            match select_author_warning32(
                &warnings32,
                direct_hits8,
                author_player_eid,
                teid,
                win_lo,
                win_hi,
            ) {
                Ok(Some(first)) => {
                    segment = first.segment;
                    game_hit_result = first.result;
                    hit_token = Some(hash6_hex(&first.hash6));
                    bound_warning = Some(first);
                    // segment 布局解码：[result][shell_global_id u24 LE（=(局部 id<<8)|国家基数）][00][X][Y][Z=armor_group]
                    // 全局弹种 id = B1B2B3 u24 LE（=(局部 id<<8)|国家基数），与 WI shell_id 同值
                    (shell_id, armor_group, hit_triangle) = decode_segment(segment);
                }
                Ok(None) => {
                    // type=32 未转发：区间内的 method8 可能有多条（一发多次装甲交互）。取
                    // **主交互**（已结算优先 → 伤害大者 → 时刻早者）作为本发的 result/token；
                    // 与 `shotResultClass` 的“伤害仲裁优先”同义，不因多交互而整发失败。
                    let mut r8: Vec<&DirectHit8> = direct_hits8
                        .iter()
                        .filter(|d| {
                            d.shooter == author_player_eid
                                && d.victim == teid
                                && in_notice_window(d.t)
                        })
                        .collect();
                    r8.sort_by(|a, b| {
                        (b.settled as u8, b.damage)
                            .cmp(&(a.settled as u8, a.damage))
                            .then(a.t.total_cmp(&b.t))
                    });
                    if let Some(first) = r8.first() {
                        game_hit_result = first.result;
                        hit_token = Some(hash6_hex(&first.hash6));
                    }
                }
                Err(n) => {
                    anyhow::bail!(
                        "{}: type=32 segment 歧义——窗口内 {} 条互不一致的命中段",
                        ctx(),
                        n
                    );
                }
            }
        }

        // ⑧'' 弹种三级兜底：type=32 segment 权威 → method 0x07 广播 @ 发射时刻（命中通知未转发，
        // 含脱靶弹；30/30 一致）→ 0x1b 地形命中广播（0x07 空窗时；args 自带 shell_global_id）。
        // 兜底发生时置对应质量标记供 UI 徽章提示
        let mut shell_from_broadcast = false;
        let mut shell_from_terrain = false;
        let terrain_hit = terrain_impacts.get(&shot_id);
        if shell_id == 0 {
            for (t_sel, sh) in shell_broadcasts.iter() {
                if *t_sel <= fire_time {
                    shell_id = *sh;
                    shell_from_broadcast = true;
                } else {
                    break;
                }
            }
            if shell_id == 0 {
                if let Some((gid, _)) = terrain_hit {
                    shell_id = *gid;
                    shell_from_terrain = true;
                }
            }
        }
        let terrain_impact = terrain_hit.map(|(_, d)| d.clone());

        // ⑧''' 开火时刻瞄准快照（method36 成对，|dt|≤0.05）：前=射击前，后=射击后。
        let shooter_aim = {
            let cands: Vec<&AimSnapshot> = aim_snapshots
                .iter()
                .filter(|(t2, _)| (*t2 - fire_time).abs() <= 0.05)
                .map(|(_, s)| s)
                .collect();
            // type39 世界系炮线：开火时刻锚定（|dt|≤0.05s，开火帧必被前后帧夹住）
            let gun_line = type39_frames
                .iter()
                .filter(|f2| (f2.clock - fire_time).abs() <= 0.05)
                .min_by_key(|f2| (((f2.clock - fire_time).abs()) * 1000.0) as u32);
            cands.first().and_then(|s| {
                s.turret_rel_yaw.map(|yaw| ShooterAimData {
                    turret_rel_yaw: yaw,
                    state_before: s.bloom.unwrap_or(0.0),
                    state_after: cands.get(1).and_then(|s2| s2.bloom),
                    gun_pitch: s.gun_pitch,
                    world_gun_yaw: gun_line.map(|f2| f2.gun_yaw),
                    world_gun_pitch: gun_line.map(|f2| f2.gun_pitch_world),
                })
            })
        };

        // 伤害归属（**服务器结算值优先 + 确定性时钟兜底**）：method8 直击通知的尾部
        // `[u8][u16 伤害][u8 状态]` 中状态字节 ≠0 = 服务器已结算，u16 即该发实际伤害
        // （overkill 已封顶、复活模式 HP 重置吞并的伤害也如实给出——2026-10-15 全语料实证；
        // 状态字节 =0 的 u16 是标称占位值，不可用）。认领 = (shooter=作者, victim=目标) 中
        // 未消费、t ≥ end_time 的**最早已结算**通知（无则最早任意，供兜底配时钟）。
        // 无已结算值 → 回退 method1 血量链同钟配对（逐事件采样）→ dc 计数器同钟 → 0+标记。
        // 穿透族谓词 0x1110 = 材料击穿 0x0010 / 内部模块穿 0x0100 / HE 爆炸 0x1000。
        let mut dmg_unattributed = false;
        if hit && hit_flags & hit_flags_mod::PENETRATION_FAMILY != 0 {
            let victim = target_eid.unwrap_or(0);
            let cands: Vec<usize> = direct_hits8
                .iter()
                .enumerate()
                .filter(|(di, d)| {
                    !d8_used.contains(di)
                        && d.shooter == author_player_eid
                        && d.victim == victim
                        && d.t >= end_time - 1e-6
                })
                .map(|(di, _)| di)
                .collect();
            let claimed = cands
                .iter()
                .map(|&di| direct_hits8[di].t)
                .min_by(f32::total_cmp)
                .and_then(|mt| {
                    cands
                        .iter()
                        .copied()
                        .filter(|&di| direct_hits8[di].t == mt)
                        .max_by_key(|&di| direct_hits8[di].settled as u8)
                });
            let hit_t = claimed.map(|di| {
                d8_used.insert(di);
                direct_hits8[di].t
            });
            let settled_damage =
                claimed.filter(|&di| direct_hits8[di].settled).map(|di| direct_hits8[di].damage);
            let matched = hit_t.and_then(|t| {
                dmg_losses.iter().enumerate().find(|(li, l)| {
                    !dmg_losses_used.contains(li)
                        && l.victim == victim
                        && (l.t_cur - t).abs() <= 1e-6
                })
            });
            if let Some((li, _)) = matched {
                dmg_losses_used.insert(li);
            }
            let lookup_t = hit_t.unwrap_or(end_time);
            let dc_idx = dc_increments
                .iter()
                .enumerate()
                .find(|(di, inc)| !dc_used.contains(di) && (inc.0 - lookup_t).abs() <= 1e-6)
                .map(|(di, _)| di);
            let dc_delta = dc_idx.map(|di| {
                dc_used.insert(di);
                dc_increments[di].1
            });
            if let Some(dmg) = settled_damage {
                damage = u32::from(dmg);
            } else if let Some((_, l)) = matched {
                damage = l.dmg;
            } else {
                // ③ 无结算值、无血量降幅、无计数器增量 = 无伤害证据 → 0 + 未归属标记
                damage = dc_delta.unwrap_or(0);
                dmg_unattributed = true;
            }
            // hp_cur 已按哨兵族归一化（{0,-1,-2,-3}→0），==0 即终态；溺水（cause=5，HP 可为正）
            // 不产生降幅、不进入本链——非炮弹击杀，正确地不计入 is_kill
            is_kill = matched
                .map(|(_, l)| l.hp_cur == 0)
                .unwrap_or(false)
                || hit_flags & hit_flags_mod::DIRECT_KILL != 0;
        }

        // ⑨ 目标位置与姿态 @ 命中通知状态（WI 对齐确定性锚点）：
        // method8 命中通知包处理时刻（文件序）受击者的最后已知 type=10 姿态——判定批次内
        // 服务器对受击者的最新已知位置；method8 缺失时回退 end_time 插值状态。
        // miss 无目标基准，回退开火时刻。
        let (tp, ta, tp_dt, tp_src) = if hit {
            let teid = target_eid.unwrap_or(0);
            let d8state = direct_hits8
                .iter()
                .find(|d| {
                    d.shooter == author_player_eid
                        && d.victim == teid
                        && in_notice_window(d.t)
                        && d.victim_state.is_some()
                })
                .and_then(|d| d.victim_state);
            match d8state {
                // 正常路径（UI 不告警）；回退路径保留 nearest/filtered/extrapolated 供徽章告警
                Some((pos, ang, state_clock)) => (pos, ang, state_clock - end_time, "wi_hit_state"),
                None => select_anchor_state(
                    st10.get(&teid).map(Vec::as_slice).unwrap_or(&[]),
                    refresh_clusters,
                    teid,
                    end_time,
                )
                .ok_or_else(|| anyhow::anyhow!("{}: 命中时刻目标 type=10 状态快照缺失", ctx()))?,
            }
        } else {
            ([0.0; 3], [0.0; 3], 0.0, "nearest")
        };

        // ⑨' 渲染层锚点 + 时间线（窗口语义与实现见 shot_render_pack，两路径共用；
        // 射手炮管俯仰时间线的无锚定回退 = method36 field2 车体系俯仰序列）。
        // 受击方有效极限同样按该 eid 的实际配件（122）叠加，与 ⑪' 的开火帧解码同口径。
        let target_limits = pitch_limits.get(&target_name).map(|base| {
            effective_pitch_limits(
                base,
                target_eid.and_then(|e| shared.vehicle_equipment.get(&e)),
            )
        });
        let target_limits = target_limits.as_deref();
        let ShotRenderPack {
            shooter_render,
            target_render,
            target_render_timeline,
            shooter_render_timeline,
            target_turret_timeline,
            shooter_turret_timeline,
            shooter_gun_timeline,
            target_gun_timeline,
        } = shot_render_pack(
            render_cache,
            st10,
            prop2,
            &shared.aoi_presence,
            author_player_eid,
            fire_time,
            sp,
            false,
            target_eid,
            hit,
            end_time,
            tp,
            shooter_limits,
            target_limits,
            ShooterGunFallback::Method36Series(aim_pitch_series),
        );

        // ⑨'' 服务器下发的受击部件索引（method8 args[10]，cmpIndex 0..3）：
        // 与本地 raycast 的部件选择对照 = 命中位置偏差的校准基准。
        let server_part_index = if hit {
            let teid = target_eid.unwrap_or(0);
            direct_hits8
                .iter()
                .find(|d| {
                    d.shooter == author_player_eid
                        && d.victim == teid
                        && in_notice_window(d.t)
                })
                .and_then(|d| d.component_index)
        } else {
            None
        };

        // ⑨''' 受击者炮管俯仰：method8/type=32 的抵达成角 pitch
        // （来向方位角校验通过者，见 decoded_target_gun_pitch）——受击者被命中时的
        // 反向瞄准俯仰，viewer 渲染"炮口指向射手"的炮管俯角。无有效解码时回退车体 pitch。
        let bearing = bearing_shooter_to_target(tp, sp);
        let server_gun_pitch = if hit {
            decoded_target_gun_pitch(
                &warnings32,
                target_eid.unwrap_or(0),
                end_time,
                bearing,
                bound_warning,
            )
                .map(|(p, _)| p)
        } else {
            None
        };

        // ⑩ 目标炮塔朝向 = prop2 + hullYaw（命中弹必须有）
        let state_time = end_time; // 炮塔/炮管取样基准 = 命中通知时刻（与 WI turret_yaw 同域）
                                   // ⑩' method8 流序 prop2 快照：method8 包处理时刻受击者的最后已知
                                   // prop2——时钟序"≤t 最后采样"在同 tick 包序错位时会取到 method8
                                   // 之后的更新（炮塔转动中差 1~8 个 coarse 步），流序快照与 WI battle.json 逐位相等。
        let d8_prop2 = if hit {
            let teid = target_eid.unwrap_or(0);
            direct_hits8
                .iter()
                .find(|d| {
                    d.shooter == author_player_eid
                        && d.victim == teid
                        && in_notice_window(d.t)
                })
                .and_then(|d| d.victim_prop2)
        } else {
            None
        };
        let turret_yaw = if hit {
            let teid = target_eid.unwrap();
            turret_rel_at(d8_prop2, prop2.get(&teid), state_time)
                .map(|rel| rel + ta[0])
                .ok_or_else(|| anyhow::anyhow!("{}: 目标炮塔朝向（type=7 prop2）缺失", ctx()))?
        } else {
            0.0
        };

        // ⑪ 射手炮塔朝向 = prop2 + 射手 hullYaw，@ 开火时刻（method29 流序快照优先，语义同 ⑩'）
        let shooter_turret_yaw =
            turret_rel_at(l.shooter_prop2, prop2.get(&author_player_eid), fire_time)
                .map(|rel| rel + sa[0])
                .ok_or_else(|| anyhow::anyhow!("{}: 射手炮塔朝向（type=7 prop2）缺失", ctx()))?;

        // ⑪' 受击方炮管俯仰 = prop2 frac 比例解码（车型极限锚定）@ 命中通知时刻；
        // 流序快照优先（扇区选择用同一快照的偏航角，与 WI 同基准）；prop2 采样或锚定缺失
        // → 回退车体 pitch（type10，语义不同仅兜底，质量标记 "target"）
        let mut gun_pitch_degraded: Vec<String> = Vec::new();
        let mut pitch_frozen: Vec<String> = Vec::new();
        let target_gun_pitch_val = if hit {
            let teid = target_eid.unwrap();
            pitch_from_prop2(
                d8_prop2,
                prop2.get(&teid),
                state_time,
                target_limits,
                &mut pitch_frozen,
                "target",
            )
            .unwrap_or_else(|| {
                gun_pitch_degraded.push("target".into());
                ta[1]
            })
        } else {
            ta[1]
        };

        // ⑫ 射手炮管俯仰 = prop2 frac 比例解码 @ 开火时刻（与受击方同源同锚定；method29 流序快照优先）；
        // prop2/锚定缺失 → 回退 method36 field2（车体系炮管俯仰，WotbTools PROVEN），仍缺则 fail-fast
        let (shooter_gun_pitch, shooter_pitch_from_method36) = match pitch_from_prop2(
            l.shooter_prop2,
            prop2.get(&author_player_eid),
            fire_time,
            shooter_limits,
            &mut pitch_frozen,
            "shooter",
        ) {
            Some(p) => (p, false),
            None => {
                gun_pitch_degraded.push("shooter".into());
                let snap_pitch = aim_snapshots
                    .iter()
                    .min_by_key(|(c, _)| (((*c - fire_time).abs()) * 1000.0) as u32)
                    .and_then(|(_, s)| s.gun_pitch);
                match snap_pitch {
                    Some(p) => (p as f32, true),
                    None => {
                        anyhow::bail!("{}: 射手炮管俯仰（prop2 与 method36 快照均缺失）", ctx())
                    }
                }
            }
        };

        // ⑬ aim_point / launch_point_rel = 相对【命中通知状态】目标位置（type10 接地高度）的偏移
        let (aim_point_val, launch_point_rel) =
            aim_point_rels(ball_b, ball_a, tp, target_eid.is_some());

        // ⑭/⑮ type=10 采样窗口（受击方相对锚点 / 射手世界系；窗口语义与截断规则见
        // tick_window_samples——两路径共用，注释里记录的实测依据不再逐路径复制）
        let tick_samples = if hit {
            target_eid
                .map(|victim| {
                    tick_window_samples(
                        st10,
                        tick_timeline,
                        session_tick,
                        refresh_clusters,
                        victim,
                        end_time,
                        tp,
                        true,
                        target_render.as_ref(),
                    )
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let shooter_tick_samples = tick_window_samples(
            st10,
            tick_timeline,
            session_tick,
            refresh_clusters,
            author_player_eid,
            fire_time,
            [0.0; 3],
            false,
            shooter_render.as_ref(),
        );

        let hits = collect_shot_hits(
            author_player_eid,
            win_lo,
            win_hi,
            target_eid,
            damage,
            game_hit_result,
            direct_hits8,
            &warnings32,
            &hit_results,
            names,
        );

        out.push(ShotReplayData {
            index: i + 1,
            time_s: fire_time,
            damage,
            target_name,
            target_eid,
            hits,
            target_armor_state: None, // 原始包证据：attach_shot_armor_states 持 packets 回填
            is_kill,
            shooter_eid: author_player_eid,
            shooter_name: names.get(&author_player_eid).cloned().unwrap_or_default(),
            is_author: true,
            shooter_pos: sp,
            shooter_ang: sa,
            target_pos: tp,
            target_ang: ta,
            target_turret_yaw: turret_yaw,
            target_gun_pitch: target_gun_pitch_val,
            target_gun_pitch_server: server_gun_pitch.is_some(),
            type32_turret_yaw: 0.0,
            shooter_turret_yaw,
            shooter_gun_pitch,
            aim_point: aim_point_val,
            launch_point_rel,
            ball_a,
            ball_b,
            launch_velocity: l.vel,
            via,
            leg_secs,
            hit_flags,
            crit_modules,
            destroyed_modules,
            segment,
            shell_id,
            shell_kind: String::new(),
            armor_group,
            hit_triangle,
            game_hit_result,
            hit_token,
            modifiers,
            shell_slot,
            fire_time,
            shot_id,
            incoming_yaw: 0.0,
            incoming_pitch: ta[1],
            tick_samples,
            shooter_tick_samples,
            fire_tick,
            terrain_impact,
            shooter_aim,
            shooter_equipment: vehicle_equipment
                .get(&author_player_eid)
                .map(VehicleEquipment::from_ids),
            target_equipment: target_eid
                .and_then(|t| vehicle_equipment.get(&t))
                .map(VehicleEquipment::from_ids),
            quality: Some(ShotQuality {
                shooter_state_dt_ms: (sp_dt * 1000.0).round() as i32,
                shooter_pos_from_muzzle: false,
                target_state_dt_ms: if hit {
                    Some((tp_dt * 1000.0).round() as i32)
                } else {
                    None
                },
                turret_degraded: Vec::new(), // 作者路径 prop2 缺失即 fail-fast，不存在降级
                dmg_unattributed,
                target_dead,
                shell_from_broadcast,
                shell_from_terrain,
                shooter_pitch_from_velocity: false,
                shooter_pitch_from_method36,
                gun_pitch_degraded,
                pitch_frozen,
                shooter_anchor_src: if sp_src != "filtered" {
                    Some(sp_src.into())
                } else {
                    None
                },
                // 受击方 filtered 仅出现于 method8 缺失回退路径（正常路径为 wi_hit_state），需序列化提示
                target_anchor_src: if hit && tp_src != "wi_hit_state" {
                    Some(tp_src.into())
                } else {
                    None
                },
                ..ShotQuality::default() // 弹种指纹回填标记由 ⑯' 统一补置
            }),
            shooter_render,
            target_render,
            target_render_timeline,
            shooter_render_timeline,
            target_turret_timeline,
            shooter_turret_timeline,
            shooter_gun_timeline,
            target_gun_timeline,
            server_part_index,
        });
    }

    // ⑯ method38 = 作者自己的命中反馈——每条都必须配对到一次发射
    if hr_cursor < hit_results.len() {
        anyhow::bail!("存在未被任何发射配对的 method38 命中结果（{} 条未消费，自 t={:.2}s 起）——发射/命中配对不完整",
            hit_results.len() - hr_cursor, hit_results[hr_cursor].t);
    }

    // ⑯' 弹种指纹回填（发射包自带）：对段包/0x07/地形三级仍空壳的发次（典型 = 命中底盘/履带
    // 无 26/27B 段包）做同场自校准回填；只补弹种，不增删发次。
    fill_shells_from_launch_fingerprints(&mut out, &launches);

    Ok(out)
}

/// 作者昵称（battle_results 权威来源）：author.account_id → 花名册昵称。
/// meta.json 常含非 UTF-8 字节导致 crate read_meta 整体失败（serde_json 严格 UTF-8），
/// 而 battle_results（pickle+protobuf）字符串恒为合法 UTF-8——作者身份以此为准。
pub fn author_nick_from_battle_results(
    br: &wotbreplay_parser::models::battle_results::BattleResults,
) -> String {
    br.players
        .iter()
        .find(|p| p.account_id == br.author.account_id)
        .map(|p| p.info.nickname.clone())
        .unwrap_or_default()
}

/// 从 type=5 实体创建包按昵称精确匹配作者玩家实体 eid。
/// 作者昵称来自回放自身：meta.player_name / battle_results author→花名册（不用文件名——
/// 文件名不属于回放数据，改名即失效）。昵称解码走
/// [`decode_type5_nickname`] SSOT（原始 UTF-8 全域，含非 ASCII），与作者昵称精确比较
///（有 battle_results 昵称作锚，无垃圾误配风险）。无匹配返回 0。
pub fn resolve_author_player_eid_by_nick(packets: &[(u32, f32, &[u8])], author_nick: &str) -> u32 {
    if author_nick.is_empty() {
        return 0;
    }
    packets
        .iter()
        .filter_map(|(t, _, p)| {
            if *t != 5 {
                return None;
            }
            decode_type5_nickname(p)
                .filter(|(_, s)| *s == author_nick)
                .map(|(eid, _)| eid)
        })
        .next()
        .unwrap_or(0)
}

/// `pitch_limits` 语义见 [`extract_shot_replays_with_limits`]。
/// `author_nick` = 作者玩家昵称（meta.player_name / battle_results author→花名册），非文件名。
pub fn extract_shot_replays_auto_with_limits(
    packets: &[(u32, f32, &[u8])],
    author_nick: &str,
    pitch_limits: &GunPitchLimits,
) -> anyhow::Result<Vec<ShotReplayData>> {
    let author_player_eid = resolve_author_player_eid_by_nick(packets, author_nick);
    extract_shot_replays_with_limits(packets, author_player_eid, pitch_limits)
}

/// 一次共享扫描完成"作者严格 + 他人宽松"两路提取（web `replay_shots_handler` /
/// CLI `replay` 子命令入口）：预分析产物与滤波渲染缓存两路复用。作者路径 fail
/// 语义与 [`extract_shot_replays_auto_with_limits`] 一致（错误原样上抛，由调用方
/// 决定降级策略）；`author_player_eid` 非零时直接采用，为 0 时按 `author_nick` 解析。
pub fn extract_all_shots_auto_with_limits(
    packets: &[(u32, f32, &[u8])],
    author_nick: &str,
    author_player_eid: u32,
    pitch_limits: &GunPitchLimits,
) -> anyhow::Result<(Vec<ShotReplayData>, OtherShotsExtraction)> {
    let author_eid = if author_player_eid != 0 {
        author_player_eid
    } else {
        resolve_author_player_eid_by_nick(packets, author_nick)
    };
    let shared = build_shot_scan_shared(packets, author_eid);
    let mut render_cache: HashMap<u32, SegmentedPoseTimeline> = HashMap::new();
    let author =
        extract_shot_replays_from_shared(&shared, author_eid, pitch_limits, &mut render_cache)?;
    let others = extract_other_shot_replays_from_shared(
        &shared,
        author_eid,
        pitch_limits,
        &mut render_cache,
    )?;
    Ok((author, others))
}

/// [`extract_all_shots_auto_with_limits`] 的三路形态（WASM `parseShotReplays` /
/// CLI diagnose 入口）：在两路提取之外，同一次共享扫描直接产出未归属命中。
/// 包流扫描是全链最重的工作，此前作者/他人/未归属三路各自
/// `build_shot_scan_shared`（≈3x 扫描）；未归属命中只消费非作者过滤的流
/// （launches/notice_windows/direct_hits8/warnings32/names），任何 author_eid 下逐位一致。
pub struct ShotExtractionBundle {
    /// 作者严格路径：Err 原样携带（fail-visible，与 [`extract_shot_replays_auto_with_limits`]
    /// 语义一致），由调用方决定降级策略；未归属命中不受其成败影响，恒可用。
    pub author: anyhow::Result<Vec<ShotReplayData>>,
    /// 他人宽松路径：数据缺失按设计跳过计数，Err 仅在内部不变量破坏时出现
    /// （见 [`extract_other_shot_replays_with_limits`] 宽松模式说明）。
    pub others: anyhow::Result<OtherShotsExtraction>,
    /// 未归属命中（对应射击不在录像里 = AoI 截断），消费方按"未知归属"呈现。
    pub unattached_hits: Vec<UnattachedHit>,
}

/// 一次共享扫描完成"作者严格 + 他人宽松 + 未归属命中"三路提取。
/// `author_player_eid` 语义同 [`extract_all_shots_auto_with_limits`]（非零直接采用）。
pub fn extract_shot_bundle_auto_with_limits(
    packets: &[(u32, f32, &[u8])],
    author_nick: &str,
    author_player_eid: u32,
    pitch_limits: &GunPitchLimits,
) -> ShotExtractionBundle {
    let author_eid = if author_player_eid != 0 {
        author_player_eid
    } else {
        resolve_author_player_eid_by_nick(packets, author_nick)
    };
    let shared = build_shot_scan_shared(packets, author_eid);
    let mut render_cache: HashMap<u32, SegmentedPoseTimeline> = HashMap::new();
    let author =
        extract_shot_replays_from_shared(&shared, author_eid, pitch_limits, &mut render_cache);
    let others = extract_other_shot_replays_from_shared(
        &shared,
        author_eid,
        pitch_limits,
        &mut render_cache,
    );
    let unattached_hits = collect_unattached_hits_from_shared(&shared);
    ShotExtractionBundle {
        author,
        others,
        unattached_hits,
    }
}
/// 他人路径提取结果：射击列表 + 回退/跳过统计（web 响应 notes 供用户了解数据边界）。
pub struct OtherShotsExtraction {
    pub shots: Vec<ShotReplayData>,
    /// method29 发射总数（含被跳过者）
    pub total_launches: usize,
    /// method20 弹道终点缺失 → 整发跳过（无复现基准）
    pub skipped_no_endpoint: usize,
    /// 受击方状态完全缺失（method8 无状态且 type=10 无采样）→ 整发跳过
    pub skipped_no_target_state: usize,
    /// 射手 type=10 缺失 → 炮口坐标兜底（未跳过，per-shot 另有 quality 标记）
    pub muzzle_fallback: usize,
    /// 多候选 method8 几何判别不出（前后纵列/受击方位姿缺失）→ 回退时间最近绑定的
    /// 次数（综合方案：完整性优先；旧实现此处无差别静默就近，可错配无痕迹）
    pub ambiguous_time_fallback: usize,
    /// method8 指向的受击方在开火前已死亡（残骸命中）→ 按 miss 处理并剔除目标的次数
    pub skipped_dead_target: usize,
}

/// method8 来向角自洽门的有效阈值（与 [`collect::decoded_target_gun_pitch`] 共用同一常量，15°）。
pub(super) const INC_YAW_TOL: f32 = 0.262;
/// 多候选判别的胜出领先幅度（rad）：次优射线偏差须比最优差至少此值才绑定；
/// 两候选同在弹道射线上（前后纵列车）判别不出 → None（fail-closed，禁就近猜）。
const INC_YAW_MARGIN: f32 = 0.05;

/// 多候选 method8 的几何判别。**hash6 来向角是命中事件的自洽属性**（每个 method8
/// 都与其自身受击者几何一致，P2b 探针定案），故判别分两门：
/// ① 自洽门——hash6[2..4] 来向角 vs (atan2(Δx,Δz) 受击者→发射点)，超 15° 的候选
/// 剔除（受击方锚点漂移/数据错配防护）；② 射线门——发射速度方向 vs (受击方位置−
/// 发射点) 水平夹角排序（一发炮弹沿射线飞行，首中目标必贴近射线；溅射第二目标
/// 不在射线上即排除）。射线偏差领先 ≥ [`INC_YAW_MARGIN`] 才绑定；全部被剔/
/// 领先不足 → None（fail-closed；调用方回退时间最近并计 `ambiguous_time_fallback`）。
fn select_dhit_by_inc_yaw<'a>(
    cands: &[&'a DirectHit8],
    victim_pos_at: &dyn Fn(u32) -> Option<[f32; 3]>,
    launch_point: [f32; 3],
    launch_vel: [f32; 3],
) -> Option<&'a DirectHit8> {
    let tau = std::f32::consts::TAU;
    let mut scored: Vec<(&'a DirectHit8, f32)> = Vec::with_capacity(cands.len());
    for d in cands {
        let Some(vp) = victim_pos_at(d.victim) else {
            continue;
        };
        // 门①：来向角自洽（受击者 → 发射点方位角）
        let bearing = (launch_point[0] - vp[0]).atan2(launch_point[2] - vp[2]);
        let inc = (u16::from_le_bytes([d.hash6[2], d.hash6[3]]) as f32 - 32768.0) / 32768.0
            * std::f32::consts::PI;
        let yaw_err =
            ((bearing - inc + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI).abs();
        if yaw_err > INC_YAW_TOL {
            continue;
        }
        // 门②：弹道射线一致性（发射速度方向 vs 发射点→受击方，水平面）
        let to_v = [vp[0] - launch_point[0], vp[2] - launch_point[2]];
        let vdir = [launch_vel[0], launch_vel[2]];
        let mv = (to_v[0] * to_v[0] + to_v[1] * to_v[1]).sqrt();
        let ms = (vdir[0] * vdir[0] + vdir[1] * vdir[1]).sqrt();
        if mv < 1e-3 || ms < 1e-3 {
            continue;
        }
        let cos = (to_v[0] * vdir[0] + to_v[1] * vdir[1]) / (mv * ms);
        let ray_err = cos.clamp(-1.0, 1.0).acos();
        if ray_err <= INC_YAW_TOL {
            scored.push((d, ray_err));
        }
    }
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    let lead = scored
        .get(1)
        .map(|(_, e)| e - scored[0].1)
        .unwrap_or(f32::INFINITY);
    if lead >= INC_YAW_MARGIN {
        Some(scored[0].0)
    } else {
        None
    }
}

// ===== 弹种指纹回填：发射包自带数据（method29），cmpIndex=0 命中无段包时的弹种来源 =====

/// 弹种回填来源（决定质量标记；编码优先）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellFillSource {
    /// 弹种编码（method29 args[8]）唯一命中
    Code,
    /// 编码多候选（同车标准/金币弹同族共码）→ 弹速去重后唯一
    CodeVelocity,
    /// 编码不可用 → 弹速指纹唯一命中
    Velocity,
}

/// 弹速指纹键：|发射速度| × 10 四舍五入（0.1 m/s 桶）。同弹种逐发浮点抖动实测 ≤2e-3 m/s、
/// 不同弹种差 ≥ 数 m/s；同键撞多弹种 = 无法区分 → 保持未知（不猜）。
fn launch_speed_key(vel: [f32; 3]) -> i64 {
    let sp = ((vel[0] as f64).powi(2) + (vel[1] as f64).powi(2) + (vel[2] as f64).powi(2)).sqrt();
    (sp * 10.0).round() as i64
}

/// 指纹匹配核心（纯函数，单测覆盖）：**编码候选优先**——唯一即定（但要过弹速一致性门：
/// 弹速集存在且不含该候选 = 上游映射不一致 → 不采纳，fail-closed）；多候选用弹速集去重后
/// 唯一才定；编码无候选再走弹速集唯一；其余（空/多/矛盾）一律 None（绝不猜）。
fn match_launch_fingerprint(
    code_set: Option<&std::collections::BTreeSet<u32>>,
    speed_set: Option<&std::collections::BTreeSet<u32>>,
) -> Option<(u32, ShellFillSource)> {
    if let Some(set) = code_set.filter(|s| !s.is_empty()) {
        if set.len() == 1 {
            let only = *set.iter().next().unwrap();
            // 一致性门：同弹种同弹速——弹速指纹若存在必须包含编码候选（不含 = 上游映射被
            // 污染，宁缺勿错）。弹速集为空/缺失时不拦（该弹可能只有编码一路的已判定发次）。
            if let Some(sset) = speed_set.filter(|s| !s.is_empty()) {
                if !sset.contains(&only) {
                    return None;
                }
            }
            return Some((only, ShellFillSource::Code));
        }
        if let Some(sset) = speed_set.filter(|s| !s.is_empty()) {
            let narrowed: Vec<u32> = set.iter().copied().filter(|sid| sset.contains(sid)).collect();
            if let [only] = narrowed.as_slice() {
                return Some((*only, ShellFillSource::CodeVelocity));
            }
        }
        return None;
    }
    match speed_set.filter(|s| !s.is_empty()) {
        Some(set) if set.len() == 1 => {
            Some((*set.iter().next().unwrap(), ShellFillSource::Velocity))
        }
        _ => None,
    }
}

/// 弹种指纹回填统计（诊断；他人路径写日志）。
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ShellFillStats {
    pub(crate) filled_by_code: usize,
    pub(crate) filled_by_code_velocity: usize,
    pub(crate) filled_by_velocity: usize,
    /// 有指纹数据但无法唯一化（同键多弹种）→ 保持未知
    pub(crate) unresolved: usize,
}

impl ShellFillStats {
    pub(crate) fn total(&self) -> usize {
        self.filled_by_code + self.filled_by_code_velocity + self.filled_by_velocity
    }
}

/// 弹种指纹回填：对 `shell_id == 0` 的射击，用**同场同射手**已判定发次（段包/0x07/0x1b，
/// 均为服务器权威数据）现建两套映射——"弹种编码 → 弹种"（method29 args[8]，每发必携）与
/// "弹速 → 弹种"——按 **编码优先、弹速去重/兜底** 回填并置质量标记（`shell_from_launch_code` /
/// `shell_from_velocity`）。典型场景：cmpIndex=0 底盘/履带命中无 26/27B 段包、他人脱靶弹
/// 无 0x1b 覆盖。映射按射手分域（同一编码在不同炮上指向不同弹种，禁跨射手混用）且现建现用
/// （游戏版本漂移不越场）。不新增/删除射击，只补弹种。
pub(crate) fn fill_shells_from_launch_fingerprints(
    shots: &mut [ShotReplayData],
    launches: &[LaunchEntry],
) -> ShellFillStats {
    // 发次头 = 同 (shooter, shotId) 首条（发射段）；续段的速度/编码不参与指纹。
    let mut heads: HashMap<(u32, u32), &LaunchEntry> = HashMap::new();
    for l in launches {
        heads.entry((l.shooter, l.shot_id)).or_insert(l);
    }
    let mut by_code: HashMap<(u32, u8), std::collections::BTreeSet<u32>> = HashMap::new();
    let mut by_speed: HashMap<(u32, i64), std::collections::BTreeSet<u32>> = HashMap::new();
    for s in shots.iter().filter(|s| s.shell_id != 0) {
        let Some(l) = heads.get(&(s.shooter_eid, s.shot_id)) else {
            continue;
        };
        by_code
            .entry((s.shooter_eid, l.shell_code))
            .or_default()
            .insert(s.shell_id);
        by_speed
            .entry((s.shooter_eid, launch_speed_key(l.vel)))
            .or_default()
            .insert(s.shell_id);
    }
    let mut stats = ShellFillStats::default();
    for s in shots.iter_mut().filter(|s| s.shell_id == 0) {
        let Some(l) = heads.get(&(s.shooter_eid, s.shot_id)) else {
            continue;
        };
        let code_set = by_code.get(&(s.shooter_eid, l.shell_code));
        let speed_set = by_speed.get(&(s.shooter_eid, launch_speed_key(l.vel)));
        let Some((sid, src)) = match_launch_fingerprint(code_set, speed_set) else {
            if code_set.is_some() || speed_set.is_some() {
                stats.unresolved += 1;
            }
            continue;
        };
        s.shell_id = sid;
        let q = s.quality.get_or_insert_with(ShotQuality::default);
        match src {
            ShellFillSource::Code => {
                stats.filled_by_code += 1;
                q.shell_from_launch_code = true;
            }
            ShellFillSource::CodeVelocity => {
                stats.filled_by_code_velocity += 1;
                q.shell_from_launch_code = true;
            }
            ShellFillSource::Velocity => {
                stats.filled_by_velocity += 1;
                q.shell_from_velocity = true;
            }
        }
    }
    stats
}

/// 其他玩家（队友/敌方）射击的宽松提取：与作者路径（[`extract_shot_replays_with_limits`]）同源数据，但 Avatar 专属包不可得，对应字段降级：
/// method38 命中反馈（作者专属）→ hit_flags/crit/destroyed/modifiers 恒空，结果 = method8 result 枚举；
/// 目标 = method8（±0.05s 命中窗；唯一候选直接绑定，多候选按几何判别
/// [`select_dhit_by_inc_yaw`]，判别不出回退时间最近并计 `ambiguous_time_fallback`）；
/// segment/弹种/装甲组 = method8 hash6 令牌 ↔ type=32 精确配对（同源），未配对时弹种回退
/// 0x1b 地形命中广播的 shell_global_id（全局广播含所有玩家脱靶弹，shotId 配对）并置 shell_from_terrain；
/// 伤害 = 血量链降幅（source=射手，cause=0）；双方炮管俯仰 = prop2 frac 解码（无锚定时射手回退发射速度向量、受击方回退车体 pitch）；
/// method36 瞄准快照 / type=28 弹药槽 = Avatar 专属 → None / 0。
/// 宽松模式：**数据**缺失的射击跳过并计数（AoI 裁剪致远端数据稀疏是预期，与作者路径
/// fail-fast 不同）；但**内部不变量**破坏（非续段 + 有终点的发射缺少命中通知窗口）照旧
/// fail-visible 上抛——那不是数据边界，是代码缺陷，静默兜底会把命中错配到错误的发射。
/// `pitch_limits` 语义见 [`extract_shot_replays_with_limits`]。
pub fn extract_other_shot_replays_with_limits(
    packets: &[(u32, f32, &[u8])],
    author_player_eid: u32,
    pitch_limits: &GunPitchLimits,
) -> anyhow::Result<OtherShotsExtraction> {
    let shared = build_shot_scan_shared(packets, author_player_eid);
    let mut render_cache: HashMap<u32, SegmentedPoseTimeline> = HashMap::new();
    extract_other_shot_replays_from_shared(
        &shared,
        author_player_eid,
        pitch_limits,
        &mut render_cache,
    )
}

/// [`extract_other_shot_replays_with_limits`] 的共享扫描形态（预分析产物复用，见
/// [`ShotScanShared`]；`render_cache` 可与作者路径共享）。
pub(crate) fn extract_other_shot_replays_from_shared(
    shared: &ShotScanShared,
    author_player_eid: u32,
    pitch_limits: &GunPitchLimits,
    render_cache: &mut HashMap<u32, SegmentedPoseTimeline>,
) -> anyhow::Result<OtherShotsExtraction> {
    // ① 全部非作者的 method29 发射事件（作者由严格路径处理；shotId 全局去重）；args<37 的包直接跳过
    let launches: Vec<LaunchEntry> = shared
        .launches
        .iter()
        .filter(|l| l.shooter != author_player_eid)
        .cloned()
        .collect();

    // ①' 段链分组（与作者路径同构）：链首 = 发射段；其余 = 同一发的跳弹/穿透续段
    let (launch_is_cont, segments) = collect::shot_segments(&launches);
    let launches: Vec<LaunchEntry> = launches
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !launch_is_cont[*i])
        .map(|(_, l)| l)
        .collect();

    // ①'' 命中通知窗口不变量校验（fail-visible）：本函数只处理非续段发射（①' 已过滤），
    // 且窗口查找仅发生在终点在位的发射上——而 [`build_notice_windows`] 恰为"每条非续段 +
    // 有终点"的发射建窗。任一发射缺窗 = 共享扫描构建/消费两侧不变量被破坏（代码缺陷），
    // 上抛而不是按编造兜底窗静默错配归属（下文两处窗口查找据此 expect）。
    for l in &launches {
        if shared.endpoints.contains_key(&l.shot_id) {
            anyhow::ensure!(
                shared.notice_windows.contains_key(&l.shot_id),
                "他人路径: shotId={} 非续段发射缺少命中通知窗口（build_notice_windows 不变量破坏）",
                l.shot_id
            );
        }
    }

    // ② method20 弹道终点（shotId 配对，全量收集，与作者路径同构）
    let endpoints = &shared.endpoints;

    // ③ method8 直击通知（全局广播）
    let direct_hits8 = &shared.direct_hits8;

    // ③' type=32 命中通知（AoI 广播含他人；按 hash6 令牌与 method8 精确配对）
    let warnings32 = &shared.warnings32;

    // ③'' method 0x1b 地形命中广播（全局，含所有玩家脱靶弹）：
    // 脱靶弹的 terrain_impact 精确落点 + shell_id 兜底链第三级数据源
    let terrain_impacts = &shared.terrain_impacts;

    let names = &shared.names;

    // Type5 物化 9 字节配件选择（作者路径同款）：他人射击双方搭载注入
    let vehicle_equipment = &shared.vehicle_equipment;

    // ④ type=35 tick 计数器展开（与作者路径同构）
    let tick_timeline = &shared.tick_timeline;
    // type=36 会话十分秒全值锚（与作者路径同构）
    let session_tick = shared.session_tick;

    // ⑤ 血量链降幅（逐事件采样，全 source 保留——作者路径 ⑥' 的无过滤版本，cause=0 炮弹直击；含 type=5 满血锚点）
    let dmg_losses = derive_dmg_losses(&shared.hp_events, None, &shared.initial_hp);
        // ⑤' **身份判定窗口 + 服务器结算值优先**：±0.05s 窗口只用于"哪条 method8 通知属于
        // 这一发"（同窗优先已结算态，见下）；伤害值优先取该通知的结算字段（trailer≠0 的
        // u16，overkill 已封顶、复活模式亦如实）；无结算值 → 回退 ⑤'' 的 HP 链同钟配对。
        let shot_dmg_inputs: Vec<Option<(f32, u32, u32)>> = launches
            .iter()
            .map(|l| {
                let (end_time, _) = endpoints.get(&l.shot_id).copied()?;
                // 命中归属窗口基准 = 首个续段时刻（装甲接触），无续段则终点时刻
                let end_time =
                    shot_impact_time(segments.get(&(l.shooter, l.shot_id)), end_time);
                // 与目标绑定同一区间（`[本发弹着, 下一发弹着)`）——两条链不允许各用一套规则。
                // 窗口在位由 ①'' 不变量校验保证（非续段 + 有终点 ⇒ 建窗）。
                let (win_lo, win_hi) = shared
                    .notice_windows
                    .get(&l.shot_id)
                    .copied()
                    .expect("他人路径: 命中通知窗口缺失（①'' 不变量校验后不可达）");
                let cands: Vec<&DirectHit8> = direct_hits8
                    .iter()
                    .filter(|d| d.shooter == l.shooter && d.t >= win_lo && d.t < win_hi)
                    .collect();
                // 同刻双包（未结算 + 已结算）优先已结算；否则取最接近 end_time 者（身份判定语义不变）
                let d = cands
                    .iter()
                    .copied()
                    .filter(|d| d.settled)
                    .min_by(|x, y| {
                        (x.t - end_time)
                            .abs()
                            .partial_cmp(&(y.t - end_time).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .or_else(|| {
                        cands.iter().copied().min_by(|x, y| {
                            (x.t - end_time)
                                .abs()
                                .partial_cmp(&(y.t - end_time).abs())
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                    })?;
                Some((d.t, l.shooter, d.victim))
            })
            .collect();
    let dmg_assign = assign_dmg_losses(&shot_dmg_inputs, &dmg_losses);

    // ⑥ type=10 状态索引 + type=7 prop2 炮塔偏航索引（共享一次预建，均按 clock 排序）
    let (st10, prop2) = (&shared.st10, &shared.prop2);
    // 补发簇签名（AoI 通道切换点）——tick 采样截断 + 锚点选择依据
    let refresh_clusters = &shared.refresh_clusters;
    let anchor_at = |eid: u32, t: f32| -> Option<([f32; 3], [f32; 3], f32, &'static str)> {
        select_anchor_state(st10.get(&eid)?, refresh_clusters, eid, t)
    };

    // ⑦ 逐发组装（宽松：缺数据跳过 + 计数）
    let mut out: Vec<ShotReplayData> = Vec::with_capacity(launches.len());
    // render_cache 由调用方传入（作者/他人两路共享——滤波时间线按实体确定）
    let mut skipped_no_endpoint = 0usize;
    let mut skipped_no_target_state = 0usize;
    let mut muzzle_fallback = 0usize;
    let mut ambiguous_time_fallback = 0usize;
    let mut skipped_dead_target = 0usize;
    for (li, l) in launches.iter().enumerate() {
        // ctx 仅在跳过/兜底打印时格式化（避免逐发无条件分配）
        let ctx = || {
            format!(
                "shotId={} shooter={:08x} t={:.2}s",
                l.shot_id, l.shooter, l.t
            )
        };
        let ball_a = l.point;
        // 终点 = method20（服务器最终停止点）；续段链给出折线的中间点与各段时长（与作者路径同构）
        let (end_time, ball_b) = match endpoints.get(&l.shot_id).copied() {
            Some(x) => x,
            None => {
                skipped_no_endpoint += 1;
                eprintln!("[replay_others] 跳过 {}: method20 终点缺失", ctx());
                continue;
            }
        };
        let chain = segments.get(&(l.shooter, l.shot_id));
        // `end_time` 语义 = 抵达时刻（有续段时取首续段时刻，与 method8 同刻），与作者路径同构
        let end_time = shot_impact_time(chain, end_time);
        // 命中通知归属区间（与作者路径同构）：`[本发弹着, 同射手下一发弹着)`，零时钟容差。
        // 终点缺失的发已 continue；窗口在位由 ①'' 不变量校验保证（非续段 + 有终点 ⇒ 建窗）。
        let (win_lo, win_hi) = shared
            .notice_windows
            .get(&l.shot_id)
            .copied()
            .expect("他人路径: 命中通知窗口缺失（①'' 不变量校验后不可达）");
        let in_notice_window = |t: f32| t >= win_lo && t < win_hi;
        let (via, leg_secs) = shot_legs(l.point, l.vel, chain, ball_b);
        // 射手状态快照；AoI 裁剪缺失时用炮口坐标兜底（ball_a = method29 服务器权威发射位置），朝向从速度向量推算，不整发跳过
        let (sp, sa, sp_dt, sp_src, pos_from_muzzle) = match anchor_at(l.shooter, l.t) {
            Some((pos, ang, dt, src)) => (pos, ang, dt, src, false),
            None => {
                let yaw = l.vel[2].atan2(l.vel[0]);
                let pitch = {
                    let horiz = (l.vel[0] * l.vel[0] + l.vel[2] * l.vel[2]).sqrt();
                    if horiz > 1e-6 {
                        l.vel[1].atan2(horiz)
                    } else {
                        0.0
                    }
                };
                muzzle_fallback += 1;
                eprintln!(
                    "[replay_others] {}: 射手 type=10 状态缺失，用炮口坐标兜底",
                    ctx()
                );
                (ball_a, [yaw, pitch, 0.0], 0.0, "nearest", true)
            }
        };
        let fire_tick = tick_at_anchored(tick_timeline, session_tick, l.t);

        // 目标 = method8（本发通知区间 `[弹着, 下一发弹着)`）：区间内**全部**通知都是本发
        // 的证据（HE 溅射可同时命中多辆——多 victim 保留，主目标取弹道自洽者）；主目标
        // 唯一时直接绑定，多候选先按几何判别（来向角自洽门 + 弹道射线门，见
        // [`select_dhit_by_inc_yaw`]），判别不出 → 回退时间最近保完整性并计数。
        let cand_dhits: Vec<&DirectHit8> = direct_hits8
            .iter()
            .filter(|d| d.shooter == l.shooter && in_notice_window(d.t))
            .collect();
        let dhit = match cand_dhits.len() {
            0 => None,
            1 => Some(cand_dhits[0]),
            _ => match select_dhit_by_inc_yaw(
                &cand_dhits,
                &|eid| anchor_at(eid, end_time).map(|(p, _, _, _)| p),
                ball_a,
                l.vel,
            ) {
                Some(d) => Some(d),
                // 几何判别不出（前后纵列/受击方位姿缺失）→ 回退时间最近保完整性
                //（窗口匹配本身准确率高），计数外露供质量评估
                None => {
                    ambiguous_time_fallback += 1;
                    cand_dhits.iter().copied().min_by(|x, y| {
                        (x.t - end_time)
                            .abs()
                            .partial_cmp(&(y.t - end_time).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                }
            },
        };
        // 服务器对残骸仍会发 method8（victim = 已死亡车辆）：该发不能作为活目标射击发布。
        // 判据 = HP 终态事件早于开火时刻（0.1s 容差吸收时钟量化；同 tick 击杀保留）。
        // 剔除后按 miss 处理：保留 method20 弹道/地形落点，target_eid/name 置空，
        // quality.target_dead 标记供消费方识别“残骸命中”。
        let mut target_dead = false;
        let dhit = match dhit {
            Some(d) if victim_dead_before(&shared.hp_events, d.victim, l.t) => {
                skipped_dead_target += 1;
                target_dead = true;
                eprintln!("[replay_others] 剔除 {}: 受击方在开火前已死亡", ctx());
                None
            }
            other => other,
        };
        let target_eid = dhit.map(|d| d.victim);
        let hit = target_eid.is_some();

        // segment/弹种/装甲组：method8 hash6 令牌 ↔ type=32 精确配对（优于时间窗）；
        // type=32 未配对（AoI 裁剪）时弹种回退 0x1b 地形命中广播（shotId 精确配对，含所有玩家脱靶弹）
        let mut segment: u64 = 0;
        let mut shell_id: u32 = 0;
        let mut shell_from_terrain = false;
        let mut armor_group: u8 = 0;
        let mut hit_triangle: u16 = 0;
        let mut game_hit_result: u8 = dhit.map(|d| d.result).unwrap_or(255);
        let mut hit_token: Option<String> = None;
        let mut bound_warning: Option<&ArenaWarning32> = None;
        if let Some(d) = dhit {
            hit_token = Some(hash6_hex(&d.hash6));
            if let Some(teid) = target_eid {
                if let Some(w) = warnings32
                    .iter()
                    .find(|w| w.eid == teid && w.hash6 == d.hash6)
                {
                    segment = w.segment;
                    game_hit_result = w.result;
                    bound_warning = Some(w);
                    (shell_id, armor_group, hit_triangle) = decode_segment(segment);
                }
            }
        }
        let terrain_hit = terrain_impacts.get(&l.shot_id);
        if shell_id == 0 {
            if let Some((gid, _)) = terrain_hit {
                shell_id = *gid;
                shell_from_terrain = true;
            }
        }
        let terrain_impact = terrain_hit.map(|(_, d)| d.clone());

        // 伤害归属：method8 结算值优先（权威），无则 HP 链同钟配对预归属结果（同钟竞争取未消费第一段）
        let mut damage = 0u32;
        let mut is_kill = false;
        let mut dmg_unattributed = false;
        let target_name = target_eid
            .and_then(|e| names.get(&e).cloned())
            .unwrap_or_default();
        if let Some(d) = dhit {
            let hp_assign = dmg_assign.get(&li).copied();
            if let Some((_, hp_cur)) = hp_assign {
                is_kill = hp_cur == 0; // 归属链已按哨兵族 {0,-1,-2,-3}→0 归一化
            }
            if d.settled {
                // 服务器结算的该发伤害（overkill 已封顶、复活模式如实）
                damage = d.damage as u32;
            } else if let Some((dm, _)) = hp_assign {
                damage = dm;
            } else if d.result == 3
                || d.result == 4
                || (d.result == 2 && d.component_index == Some(0))
            {
                // 应伤结果（3=击穿 / 4=履带·模块交互可带伤）却无结算值与降幅 = 服务器未记账 HP
                dmg_unattributed = true;
            }
            // result=1/2（未穿/间隙止）无降幅 = 正常零伤，不打标
        }

        // 目标状态 @ 命中通知状态（WI 对齐锚点，与作者路径同源：dhit 快照即受击者在
        // method8 处理时刻的运行状态；状态缺失回退 end_time 插值，仍缺则整发跳过）
        let (tp, ta, tp_dt, tp_src) = if let Some(d) = dhit {
            match d.victim_state {
                // 与作者路径同源：method8 通知状态 = WI 正常语义（不告警）
                Some((pos, ang, state_clock)) => (pos, ang, state_clock - end_time, "wi_hit_state"),
                None => match anchor_at(d.victim, end_time) {
                    Some(x) => x,
                    None => {
                        skipped_no_target_state += 1;
                        eprintln!(
                            "[replay_others] 跳过 {}: 命中时刻目标 type=10 状态缺失",
                            ctx()
                        );
                        continue;
                    }
                },
            }
        } else {
            ([0.0; 3], [0.0; 3], 0.0, "nearest")
        };

        // 渲染层锚点 + 时间线（与作者路径共用 shot_render_pack；射手炮口兜底时跳过
        // 射手侧渲染数据，炮管俯仰时间线无锚定回退 = 空）。
        // 双方有效极限按各自 eid 的实际配件（122）叠加，与开火帧解码同口径。
        let shooter_name_val = names.get(&l.shooter).cloned().unwrap_or_default();
        let shooter_limits = pitch_limits
            .get(&shooter_name_val)
            .map(|base| effective_pitch_limits(base, shared.vehicle_equipment.get(&l.shooter)));
        let shooter_limits = shooter_limits.as_deref();
        let target_limits = pitch_limits.get(&target_name).map(|base| {
            effective_pitch_limits(
                base,
                target_eid.and_then(|e| shared.vehicle_equipment.get(&e)),
            )
        });
        let target_limits = target_limits.as_deref();
        let ShotRenderPack {
            shooter_render,
            target_render,
            target_render_timeline,
            shooter_render_timeline,
            target_turret_timeline,
            shooter_turret_timeline,
            shooter_gun_timeline,
            target_gun_timeline,
        } = shot_render_pack(
            render_cache,
            st10,
            prop2,
            &shared.aoi_presence,
            l.shooter,
            l.t,
            sp,
            pos_from_muzzle,
            target_eid,
            hit,
            end_time,
            tp,
            shooter_limits,
            target_limits,
            ShooterGunFallback::None,
        );
        // 服务器受击部件索引（method8 args[10]）
        let server_part_index = dhit.and_then(|d| d.component_index);
        // 受击者炮管俯仰（原始解码恢复）：来向方位角校验通过的抵达成角 pitch
        let bearing = bearing_shooter_to_target(tp, sp);
        let server_gun_pitch = decoded_target_gun_pitch(
            warnings32,
            target_eid.unwrap_or(0),
            end_time,
            bearing,
            bound_warning,
        )
        .map(|(p, _)| p);

        // 炮塔朝向（prop2 相对角 @ 命中通知时刻；method8 流序快照优先 = WI 逐位同基准，
        // 见 DirectHit8::victim_prop2；AoI 裁剪缺失时降级为车体朝向，不跳过）
        let d8_prop2 = dhit.and_then(|d| d.victim_prop2);
        let mut turret_degraded: Vec<String> = Vec::new();
        let mut gun_pitch_degraded: Vec<String> = Vec::new();
        let mut pitch_frozen: Vec<String> = Vec::new();
        let target_turret_yaw = if hit {
            let teid = target_eid.unwrap_or(0);
            match turret_rel_at(d8_prop2, prop2.get(&teid), end_time) {
                Some(rel) => rel + ta[0],
                None => {
                    turret_degraded.push("target".into());
                    ta[0]
                }
            }
        } else {
            0.0
        };
        let shooter_turret_yaw = match turret_rel_at(l.shooter_prop2, prop2.get(&l.shooter), l.t) {
            Some(rel) => rel + sa[0],
            None => {
                if !pos_from_muzzle {
                    turret_degraded.push("shooter".into());
                }
                sa[0]
            }
        };

        // 炮管俯仰：prop2 frac 比例解码（双方同源，method29/8 流序快照优先）；prop2/锚定缺失
        // → 射手回退发射速度向量反解（垂直/水平分量），受击方回退车体 pitch，均打质量标记
        let shooter_gun_pitch = match pitch_from_prop2(
            l.shooter_prop2,
            prop2.get(&l.shooter),
            l.t,
            shooter_limits,
            &mut pitch_frozen,
            "shooter",
        ) {
            Some(p) => p,
            None => {
                gun_pitch_degraded.push("shooter".into());
                let horiz = (l.vel[0] * l.vel[0] + l.vel[2] * l.vel[2]).sqrt();
                if horiz > 1e-6 {
                    l.vel[1].atan2(horiz)
                } else {
                    0.0
                }
            }
        };
        let target_gun_pitch_val = if hit {
            let teid = target_eid.unwrap();
            pitch_from_prop2(
                d8_prop2,
                prop2.get(&teid),
                end_time,
                target_limits,
                &mut pitch_frozen,
                "target",
            )
            .unwrap_or_else(|| {
                gun_pitch_degraded.push("target".into());
                ta[1]
            })
        } else {
            ta[1]
        };

        // aim_point / launch_point_rel = 相对命中通知状态目标位置的偏移（与作者路径同式）
        let (aim_point_val, launch_point_rel) =
            aim_point_rels(ball_b, ball_a, tp, target_eid.is_some());

        // ⑭/⑮ type=10 采样窗口（与作者路径共用 tick_window_samples）
        let tick_samples = target_eid
            .map(|victim| {
                tick_window_samples(
                    st10,
                    tick_timeline,
                    session_tick,
                    refresh_clusters,
                    victim,
                    end_time,
                    tp,
                    true,
                    target_render.as_ref(),
                )
            })
            .unwrap_or_default();
        let shooter_tick_samples = tick_window_samples(
            st10,
            tick_timeline,
            session_tick,
            refresh_clusters,
            l.shooter,
            l.t,
            [0.0; 3],
            false,
            shooter_render.as_ref(),
        );

        let hits = collect_shot_hits(
            l.shooter,
            win_lo,
            win_hi,
            target_eid,
            damage,
            game_hit_result,
            direct_hits8,
            warnings32,
            &shared.hit_results38,
            names,
        );

        out.push(ShotReplayData {
            index: 0, // 合并后由调用方按 time_s 全局重编号
            time_s: l.t,
            damage,
            target_name,
            target_eid,
            hits,
            target_armor_state: None, // 原始包证据：attach_shot_armor_states 持 packets 回填
            is_kill,
            shooter_eid: l.shooter,
            shooter_name: names.get(&l.shooter).cloned().unwrap_or_default(),
            is_author: false,
            shooter_pos: sp,
            shooter_ang: sa,
            target_pos: tp,
            target_ang: ta,
            target_turret_yaw,
            target_gun_pitch: target_gun_pitch_val,
            target_gun_pitch_server: server_gun_pitch.is_some(),
            type32_turret_yaw: 0.0,
            shooter_turret_yaw,
            shooter_gun_pitch,
            aim_point: aim_point_val,
            launch_point_rel,
            ball_a,
            ball_b,
            launch_velocity: l.vel,
            via,
            leg_secs,
            hit_flags: 0, // method38 作者专属，他人不可得（结果看 game_hit_result）
            crit_modules: 0,
            destroyed_modules: 0,
            segment,
            shell_id,
            shell_kind: String::new(),
            armor_group,
            hit_triangle,
            game_hit_result,
            hit_token,
            modifiers: Vec::new(),
            shell_slot: 0, // type=28 弹药槽是作者本人的选择状态，对他人无意义
            fire_time: l.t,
            shot_id: l.shot_id,
            incoming_yaw: 0.0,
            incoming_pitch: ta[1],
            tick_samples,
            shooter_tick_samples,
            fire_tick,
            terrain_impact,    // 0x1b 全局广播：他人脱靶弹同样有精确落点
            shooter_aim: None, // method36 瞄准快照 = 作者 Avatar 专属
            shooter_equipment: vehicle_equipment
                .get(&l.shooter)
                .map(VehicleEquipment::from_ids),
            target_equipment: target_eid
                .and_then(|t| vehicle_equipment.get(&t))
                .map(VehicleEquipment::from_ids),
            quality: Some(ShotQuality {
                shooter_state_dt_ms: (sp_dt * 1000.0).round() as i32,
                shooter_pos_from_muzzle: pos_from_muzzle,
                target_state_dt_ms: if hit {
                    Some((tp_dt * 1000.0).round() as i32)
                } else {
                    None
                },
                turret_degraded,
                dmg_unattributed,
                target_dead,
                shell_from_broadcast: false, // 0x07 弹种广播 = 作者 Avatar 弹药状态，他人不可得
                shell_from_terrain,
                shooter_pitch_from_velocity: gun_pitch_degraded.iter().any(|s| s == "shooter"),
                shooter_pitch_from_method36: false,
                gun_pitch_degraded,
                pitch_frozen,
                shooter_anchor_src: if sp_src != "filtered" {
                    Some(sp_src.into())
                } else {
                    None
                },
                // 受击方 filtered 仅出现于 method8 缺失回退路径（正常路径为 wi_hit_state），需序列化提示
                target_anchor_src: if hit && tp_src != "wi_hit_state" {
                    Some(tp_src.into())
                } else {
                    None
                },
                ..ShotQuality::default() // 弹种指纹回填标记由 ⑯' 统一补置
            }),
            shooter_render,
            target_render,
            target_render_timeline,
            shooter_render_timeline,
            target_turret_timeline,
            shooter_turret_timeline,
            shooter_gun_timeline,
            target_gun_timeline,
            server_part_index,
        });
    }
    // ⑯' 弹种指纹回填（发射包自带 method29 args[8] 编码 + 弹速）：段包/0x1b 之外的弹种来源
    // （典型 = cmpIndex=0 底盘/履带命中无 26/27B 段包）；同场同射手自校准，只补弹种。
    let fill = fill_shells_from_launch_fingerprints(&mut out, &launches);
    eprintln!("[replay_others] 其他玩家射击提取: {} 发（终点缺失跳过 {}、受击方状态缺失跳过 {}、受击方开火前已死亡剔除 {}、射手状态炮口兜底 {}、多目标歧义回退时间最近 {}、弹种指纹回填 {} = 编码 {} / 编码+弹速 {} / 弹速 {}、指纹未唯一 {}）",
        out.len(), skipped_no_endpoint, skipped_no_target_state, skipped_dead_target, muzzle_fallback, ambiguous_time_fallback,
        fill.total(), fill.filled_by_code, fill.filled_by_code_velocity, fill.filled_by_velocity, fill.unresolved);
    Ok(OtherShotsExtraction {
        shots: out,
        total_launches: launches.len(),
        skipped_no_endpoint,
        skipped_no_target_state,
        skipped_dead_target,
        muzzle_fallback,
        ambiguous_time_fallback,
    })
}

#[cfg(test)]
mod author_warning32_pairing_tests {
    use super::*;

    fn warning(t: f32, eid: u32, hash6: [u8; 6], segment: u64) -> ArenaWarning32 {
        ArenaWarning32 {
            t,
            eid,
            result: segment.to_le_bytes()[0],
            segment,
            hash6,
            inc_yaw: 0.0,
            inc_pitch: 0.0,
        }
    }

    fn direct(t: f32, shooter: u32, victim: u32, hash6: [u8; 6], result: u8) -> DirectHit8 {
        DirectHit8 {
            t,
            shooter,
            victim,
            result,
            component_index: Some(1),
            hash6,
            damage: 0,
            settled: false,
            victim_state: None,
            victim_prop2: None,
        }
    }

    #[test]
    fn unique_method8_hash_disambiguates_multi_interaction_type32_window() {
        // 11.20 China Kranvagn 边界形状：同一 Shot / victim / 时钟出现两个不同
        // type=32 segment；method8 自身重复广播，但两份共享同一 hash6。
        let a = [0x0a, 0xa3, 0x6c, 0x50, 0xa1, 0x5c];
        let b = [0x23, 0xa8, 0x6c, 0x4c, 0x8f, 0x4d];
        let warnings = vec![
            warning(110.35225, 42, a, 0x0346_0000_0135_8a00),
            warning(110.35225, 42, b, 0x033a_b700_0135_8a01),
        ];
        let hits = vec![
            direct(110.35225, 7, 42, a, 0),
            direct(110.35225, 7, 42, a, 0),
        ];

        let picked = select_author_warning32(&warnings, &hits, 7, 42, 110.30225, 110.40225)
            .expect("唯一 method8 hash6 应消除 type32 窗口歧义")
            .expect("应选出命中段");
        assert_eq!(picked.hash6, a);
        assert_eq!(picked.segment, warnings[0].segment);
    }

    #[test]
    fn conflicting_type32_without_any_method8_hash_remains_fail_fast() {
        // 无 method8 token 可对、且两段互不一致 → 不猜（fail-closed）
        let a = [1, 2, 3, 4, 5, 6];
        let b = [6, 5, 4, 3, 2, 1];
        let warnings = vec![warning(20.0, 42, a, 0x10), warning(20.0, 42, b, 0x20)];
        assert!(matches!(
            select_author_warning32(&warnings, &[], 7, 42, 19.95, 20.05),
            Err(2)
        ));
    }

    #[test]
    fn multi_interaction_picks_primary_by_settled_damage() {
        // 一发多次装甲交互（两个不同 hash6/两条 method8）：不再整发 fail，取主交互
        // （已结算 → 伤害大者）。这里是两条未结算，按插入序稳定取第一条。
        let a = [1, 2, 3, 4, 5, 6];
        let b = [6, 5, 4, 3, 2, 1];
        let warnings = vec![warning(20.0, 42, a, 0x10), warning(20.0, 42, b, 0x20)];
        let hits = vec![direct(20.0, 7, 42, a, 0), direct(20.0, 7, 42, b, 1)];
        let picked = select_author_warning32(&warnings, &hits, 7, 42, 19.95, 20.05)
            .expect("多交互不应整发失败")
            .expect("应选出主交互段");
        assert_eq!(picked.hash6, a);
    }

    #[test]
    fn duplicate_same_token_type32_with_same_segment_is_not_ambiguous() {
        let hash = [1, 1, 2, 3, 5, 8];
        let warnings = vec![
            warning(30.0, 42, hash, 0x1234),
            warning(30.0, 42, hash, 0x1234),
        ];
        let hits = vec![direct(30.0, 7, 42, hash, 0)];

        let picked = select_author_warning32(&warnings, &hits, 7, 42, 29.95, 30.05)
            .expect("相同 token + 相同 segment 的重复通知应去重语义")
            .expect("应选出命中段");
        assert_eq!(picked.segment, 0x1234);
    }
}

#[cfg(test)]
mod target_binding_tests {
    use super::*;

    fn dh(t: f32, victim: u32, inc_yaw_u16: u16) -> DirectHit8 {
        DirectHit8 {
            t,
            shooter: 7,
            victim,
            result: 3,
            component_index: Some(1),
            hash6: [
                0x11,
                0x22,
                inc_yaw_u16 as u8,
                (inc_yaw_u16 >> 8) as u8,
                0x80,
                0x00,
            ],
            damage: 0,
            settled: false,
            victim_state: None,
            victim_prop2: None,
        }
    }

    /// 来向角 u16 编码（与 collect.rs 解码式互逆）：yaw ∈ (−π, π] → (yaw/π+1)×32768
    fn enc_yaw(yaw: f32) -> u16 {
        ((yaw / std::f32::consts::PI + 1.0) * 32768.0).round() as u16
    }

    const SHOOTER: [f32; 3] = [0.0, 0.0, 0.0];

    #[test]
    fn two_candidates_bound_by_inc_yaw_geometry() {
        // 射手在原点；受击 A 在 +x（受击→射手方位角 atan2(−100,0) = −π/2）、
        // B 在 +z（atan2(0,−100) = +π ≡ u16 0）。hash6 来向角 = A 的 → 绑 A。
        let a_yaw = -std::f32::consts::FRAC_PI_2;
        let a = dh(20.0, 100, enc_yaw(a_yaw));
        let b = dh(20.0, 200, enc_yaw(std::f32::consts::PI));
        let cands = vec![&a, &b];
        let pos = |eid: u32| match eid {
            100 => Some([100.0, 0.0, 0.0]),
            200 => Some([0.0, 0.0, 100.0]),
            _ => None,
        };
        let picked = select_dhit_by_inc_yaw(&cands, &pos, SHOOTER, [1.0, 0.0, 0.0]);
        assert_eq!(picked.map(|d| d.victim), Some(100), "方位角互异的多候选必须可判别");
    }

    #[test]
    fn colinear_candidates_stay_unbound() {
        // 前后纵列（同方位角）：两候选 hash6 来向角一致 → 领先不足 → 不猜
        let a = dh(20.0, 100, enc_yaw(-std::f32::consts::FRAC_PI_2));
        let b = dh(20.05, 200, enc_yaw(-std::f32::consts::FRAC_PI_2));
        let cands = vec![&a, &b];
        let pos = |eid: u32| match eid {
            100 => Some([100.0, 0.0, 0.0]),
            200 => Some([200.0, 0.0, 0.0]),
            _ => None,
        };
        let picked = select_dhit_by_inc_yaw(&cands, &pos, SHOOTER, [1.0, 0.0, 0.0]);
        assert!(picked.is_none(), "同向纵列候选判别不出必须 fail-closed");
    }

    #[test]
    fn all_candidates_off_tolerance_stay_unbound() {
        // 唯一"可验"候选也超 15° 阈（hash6 与几何全不合）→ 不猜
        let a = dh(20.0, 100, enc_yaw(0.0)); // 实际方位角 −π/2，偏差 90°
        let cands = vec![&a];
        let pos = |eid: u32| match eid {
            100 => Some([100.0, 0.0, 0.0]),
            _ => None,
        };
        let picked = select_dhit_by_inc_yaw(&cands, &pos, SHOOTER, [1.0, 0.0, 0.0]);
        assert!(picked.is_none());
    }

    #[test]
    fn unverifiable_candidate_does_not_block_verifiable_one() {
        // B 无锚点不可验；A 唯一可验且通过 → 绑 A（领先 = ∞）
        let a = dh(20.0, 100, enc_yaw(-std::f32::consts::FRAC_PI_2));
        let b = dh(20.05, 200, enc_yaw(0.0));
        let cands = vec![&a, &b];
        let pos = |eid: u32| match eid {
            100 => Some([100.0, 0.0, 0.0]),
            _ => None,
        };
        let picked = select_dhit_by_inc_yaw(&cands, &pos, SHOOTER, [1.0, 0.0, 0.0]);
        assert_eq!(picked.map(|d| d.victim), Some(100));
    }
}

#[cfg(test)]
mod shell_fingerprint_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn set(ids: &[u32]) -> BTreeSet<u32> {
        ids.iter().copied().collect()
    }

    #[test]
    fn code_unique_wins_first() {
        // 编码候选唯一即定（60TP AP 0x08 → 23434）；弹速一致或缺失均放行
        assert_eq!(
            match_launch_fingerprint(Some(&set(&[23434])), None),
            Some((23434, ShellFillSource::Code))
        );
        assert_eq!(
            match_launch_fingerprint(Some(&set(&[23434])), Some(&set(&[23434]))),
            Some((23434, ShellFillSource::Code))
        );
    }

    #[test]
    fn code_speed_contradiction_is_rejected() {
        // 一致性门：编码唯一候选 = 23434，但弹速指纹只认 23690（同键不同弹）＝上游映射
        // 被污染 → 不采纳（fail-closed，宁缺勿错）
        assert_eq!(
            match_launch_fingerprint(Some(&set(&[23434])), Some(&set(&[23690]))),
            None
        );
    }

    #[test]
    fn code_multi_narrowed_by_speed() {
        // 同车标准/金币弹同族共码（FV215b 183 he/he_premium 同 0x0d）：编码 {21082,21338}，
        // 弹速集只含 21338 → 编码+弹速定 21338
        assert_eq!(
            match_launch_fingerprint(Some(&set(&[21082, 21338])), Some(&set(&[21338]))),
            Some((21338, ShellFillSource::CodeVelocity))
        );
    }

    #[test]
    fn code_multi_unresolvable_stays_unknown() {
        // 编码多候选且弹速不能区分（Rhm. Pzw. 两弹共 0x17 且同速）→ None（fail-closed，禁猜）
        assert_eq!(
            match_launch_fingerprint(Some(&set(&[537370, 537626])), Some(&set(&[537370, 537626]))),
            None
        );
        // 多候选但无弹速数据同样保持未知
        assert_eq!(match_launch_fingerprint(Some(&set(&[21082, 21338])), None), None);
    }

    #[test]
    fn velocity_fallback_only_when_code_unknown() {
        // 编码无候选 → 弹速唯一命中（Kpz 70 Missile HE 632 → 548922）
        assert_eq!(
            match_launch_fingerprint(None, Some(&set(&[548922]))),
            Some((548922, ShellFillSource::Velocity))
        );
        // Obj268 三弹同速（608 m/s）→ 弹速无法唯一 → 未知
        assert_eq!(match_launch_fingerprint(None, Some(&set(&[26122, 26378, 26634]))), None);
        // 两者皆无 → 未知
        assert_eq!(match_launch_fingerprint(None, None), None);
        assert_eq!(match_launch_fingerprint(Some(&set(&[])), Some(&set(&[]))), None);
    }

    #[test]
    fn speed_key_absorbs_float_jitter() {
        // 同弹种逐发浮点抖动（实测 ≤2e-3 m/s）必须落同一 0.1 m/s 桶；
        // 相邻弹种差 ≥ 数 m/s 必须落不同桶（以 620 vs 670 为界验证 0.5 m/s 也分开）
        assert_eq!(
            launch_speed_key([-842.5, 7.9, 56.2]),
            launch_speed_key([-842.5001, 7.9001, 56.2001])
        );
        assert_ne!(launch_speed_key([620.0, 0.0, 0.0]), launch_speed_key([670.0, 0.0, 0.0]));
    }
}

#[cfg(test)]
mod dead_target_tests {
    use super::*;

    fn hp(clock: f32, victim: u32, hp: u16) -> HpEvent {
        HpEvent {
            clock,
            victim,
            hp,
            source: 0,
            cause: 0,
        }
    }

    #[test]
    fn dead_before_fire_is_rejected_with_tolerance() {
        let events = vec![hp(91.2, 7, 0)];
        assert!(victim_dead_before(&events, 7, 91.616));
        // 同一 tick 击杀（死亡事件略早于 method29）不剔除
        assert!(!victim_dead_before(&events, 7, 91.25));
        // 其它实体不受影响
        assert!(!victim_dead_before(&events, 8, 91.616));
    }

    #[test]
    fn positive_hp_after_death_means_resurrected() {
        let events = vec![hp(91.2, 7, 0), hp(95.0, 7, 500)];
        assert!(victim_dead_before(&events, 7, 93.0));
        assert!(!victim_dead_before(&events, 7, 96.0));
    }

    #[test]
    fn no_hp_evidence_fails_open() {
        assert!(!victim_dead_before(&[], 7, 91.616));
        let events = vec![hp(91.2, 7, 800)];
        assert!(!victim_dead_before(&events, 7, 91.616));
    }

    #[test]
    fn negative_hp_sentinel_is_terminal() {
        let events = vec![hp(91.2, 7, 0xFFFD)];
        assert!(victim_dead_before(&events, 7, 91.616));
    }
}

#[cfg(test)]
mod notice_window_tests {
    use super::*;

    const SHOOTER: u32 = 0xA000_0001;
    const VICTIM_A: u32 = 0xB000_0001;
    const VICTIM_B: u32 = 0xB000_0002;

    fn m8(t: f32, victim: u32, hash: [u8; 6], settled: bool, damage: u16) -> DirectHit8 {
        DirectHit8 {
            t,
            shooter: SHOOTER,
            victim,
            result: 3,
            component_index: Some(1),
            hash6: hash,
            damage,
            settled,
            victim_state: None,
            victim_prop2: None,
        }
    }

    fn w32(t: f32, victim: u32, hash: [u8; 6], segment: u64) -> ArenaWarning32 {
        ArenaWarning32 {
            t,
            eid: victim,
            result: 3,
            segment,
            hash6: hash,
            inc_yaw: 0.0,
            inc_pitch: 0.0,
        }
    }

    fn names() -> HashMap<u32, String> {
        HashMap::new()
    }

    /// 区间 = [本发弹着, 同射手下一发弹着)，末发上界 +∞；顺序纯由终点时刻决定。
    #[test]
    fn windows_are_ordered_endpoint_intervals() {
        let launches = vec![
            LaunchEntry {
                t: 10.0,
                shooter: SHOOTER,
                shot_id: 1,
                point: [0.0; 3],
                vel: [0.0; 3],
                shell_code: 0,
                shooter_prop2: None,
            },
            LaunchEntry {
                t: 20.0,
                shooter: SHOOTER,
                shot_id: 2,
                point: [0.0; 3],
                vel: [0.0; 3],
                shell_code: 0,
                shooter_prop2: None,
            },
        ];
        let mut endpoints = HashMap::new();
        endpoints.insert(1u32, (10.0f32, [0.0f32; 3]));
        endpoints.insert(2u32, (20.0f32, [0.0f32; 3]));
        let w = build_notice_windows(&launches, &[false, false], &HashMap::new(), &endpoints);
        assert_eq!(w.get(&1).copied(), Some((10.0, 20.0)));
        assert_eq!(w.get(&2).map(|x| x.0), Some(20.0));
        assert!(w.get(&2).unwrap().1.is_infinite());
    }

    /// 287 伤害样本的形态：命中通知晚一个 tick（+0.0811s）仍按区间绑定，且按 hash6
    /// 找到对应的 type=32 segment（旧 ±0.05 窗会判成“未转发”）。
    #[test]
    fn late_notification_binds_by_interval() {
        let hash = [0x14, 0xf6, 0xc7, 0x48, 0xf3, 0xa2];
        let d8 = vec![m8(182.108, VICTIM_A, hash, true, 287)];
        let warnings = vec![w32(182.108, VICTIM_A, hash, 0x0554_5a00_009c_1a03)];
        let got = select_author_warning32(
            &warnings, &d8, SHOOTER, VICTIM_A, /* lo */ 182.026, /* hi */ 195.910,
        )
        .unwrap()
        .expect("通知在区间内 → 必须绑定");
        assert_eq!(decode_segment(got.segment).1, 5, "armor_group = hull plate 5");
        // 旧窗口（±0.05）会落空——这是本次修复的回归锚
        let legacy = select_author_warning32(&warnings, &d8, SHOOTER, VICTIM_A, 182.026 - 0.05, 182.026 + 0.05)
            .unwrap();
        assert!(legacy.is_none());
    }

    /// 一发多 victim：区间内两条不同受击者的 method8 都保留，主目标在首位并继承顶层
    /// 伤害/结果；额外目标用服务器结算值。
    #[test]
    fn multi_victim_is_kept() {
        let hash_a = [1, 2, 3, 4, 5, 6];
        let hash_b = [7, 8, 9, 10, 11, 12];
        // 同一服务器批次（同一包钟）= 一次开火的多目标命中
        let d8 = vec![
            m8(30.05, VICTIM_A, hash_a, true, 500),
            m8(30.05, VICTIM_B, hash_b, true, 120),
        ];
        let hits = collect_shot_hits(
            SHOOTER,
            30.0,
            40.0,
            Some(VICTIM_B),
            120,
            3,
            &d8,
            &[],
            &[],
            &names(),
        );
        assert_eq!(hits.len(), 2, "两个受击者都必须保留");
        assert!(hits[0].is_primary);
        assert_eq!(hits[0].target_eid, VICTIM_B);
        assert_eq!(hits[0].damage, 120);
        assert_eq!(hits[1].target_eid, VICTIM_A);
        assert_eq!(hits[1].damage, 500);
    }

    /// 晚于主事件的同射手通知**不得**收成本发的第二目标（它属于未录入的另一发 = AoI）。
    #[test]
    fn late_foreign_notification_is_not_a_second_victim() {
        let hash_a = [1, 1, 1, 1, 1, 1];
        let hash_b = [2, 2, 2, 2, 2, 2];
        let d8 = vec![
            m8(30.05, VICTIM_A, hash_a, true, 500),
            m8(35.00, VICTIM_B, hash_b, true, 120), // 5s 后：属于别的射击
        ];
        let hits = collect_shot_hits(
            SHOOTER,
            30.0,
            40.0,
            Some(VICTIM_A),
            500,
            3,
            &d8,
            &[],
            &[],
            &names(),
        );
        assert_eq!(hits.len(), 1, "晚到的外部通知不能错绑成本发的第二目标");
        assert_eq!(hits[0].target_eid, VICTIM_A);
    }

    /// 同事件重发（同刻同 hash6：未结算 + 已结算）合并成一条，保留结算值。
    #[test]
    fn duplicate_event_is_merged() {
        let hash = [9, 9, 9, 9, 9, 9];
        let d8 = vec![
            m8(50.0, VICTIM_A, hash, false, 0),
            m8(50.0, VICTIM_A, hash, true, 440),
        ];
        let hits = collect_shot_hits(
            SHOOTER,
            49.0,
            60.0,
            Some(VICTIM_A),
            440,
            3,
            &d8,
            &[],
            &[],
            &names(),
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].damage, 440);
    }
}

#[cfg(test)]
mod pitch_equipment_tests {
    use super::*;

    fn base() -> GunPitchRange {
        GunPitchRange {
            dep: 6.0,
            ele: 18.0,
            front: None,
            back: None,
            transition: None,
        }
    }

    /// 122 的加成必须作用到 `pitch_from_prop2`（射击复现射手/受击方共用的解码链）：
    /// frac=0（仰角上限）+4°，frac=63（俯角上限）低 3°；无 122 时逐值不变。
    #[test]
    fn prop2_pitch_uses_equipment_aware_limits() {
        let b = base();
        let eq = [103u8, 122, 0, 0, 0, 0, 0, 0, 0];
        let eff = effective_pitch_limits(&b, Some(&eq));
        let mut frozen = Vec::new();

        let base_up = pitch_from_prop2(Some((0.0, 0u16)), None, 0.0, Some(&b), &mut frozen, "s")
            .expect("基础仰角可解");
        let eq_up = pitch_from_prop2(Some((0.0, 0u16)), None, 0.0, Some(eff.as_ref()), &mut frozen, "s")
            .expect("122 仰角可解");
        assert!(
            (eq_up - base_up - 4.0f32.to_radians()).abs() < 1e-6,
            "frac=0 应精确 +4°"
        );

        let base_down = pitch_from_prop2(Some((0.0, 63u16)), None, 0.0, Some(&b), &mut frozen, "s")
            .expect("基础俯角可解");
        let eq_down = pitch_from_prop2(Some((0.0, 63u16)), None, 0.0, Some(eff.as_ref()), &mut frozen, "s")
            .expect("122 俯角可解");
        assert!(
            (base_down - eq_down - 3.0f32.to_radians()).abs() < 1e-6,
            "frac=63 应精确低 3°"
        );

        let no_eq = effective_pitch_limits(&b, Some(&[103u8, 110, 0, 0, 0, 0, 0, 0, 0]));
        let no_eq_up = pitch_from_prop2(Some((0.0, 0u16)), None, 0.0, Some(no_eq.as_ref()), &mut frozen, "s")
            .expect("无 122 可解");
        assert!((no_eq_up - base_up).abs() < 1e-9, "无 122 必须逐值不变");
    }
}
