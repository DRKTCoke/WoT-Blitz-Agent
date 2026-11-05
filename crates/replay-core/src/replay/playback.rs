//! 全场实时回放数据层：把现有解析成果（渲染层滤波位姿 + prop2 炮塔角 + 射击/血量链）
//! 组装成前端可直接按 0.1s 网格推进的全场时间线。
//!
//! 与单发复现（viewer Shot Replay 滑块）同语义、不同消费方式：
//! - 位姿 = [`FilteredTimeline`]（filter.rs，客户端 AvatarFilter 移植）60Hz 输出的 0.1s 降采样；
//! - 炮塔角 = [`combat::prop2_at`]（客户端 0x1440C70 时间线语义：0.1s 前瞻/短弧插值/clamp 不外推）
//!   的相对角 + **同网格刻**车体 yaw 合成绝对角（与 combat.rs ⑩/⑪ 步 `rel + hullYaw` 同式）；
//! - 角度序列（hull_yaw/turret_yaw）落盘前做**相位解卷绕**（[`unwrap_angle`]）：相邻网格
//!   物理角差 ≪ π（0.1s 车体/炮塔极限转速远低于 90°/格），解卷绕后序列连续——消费方
//!   朴素线性插值即物理正确，±π 边界不出现 ≈2π 数值跳变（跳变会让插值反甩 ~360°）；
//! - 炮管俯仰 = prop2 frac 按车型极限解码（[`combat::decode_prop2_gun_pitch`]）；
//! - 射击 = 作者严格路径 + 他人宽松路径合并（与 web `replay_shots_handler` 同构）；
//! - 血量 = type=5 满血锚点 + method1 事件链；死亡 = type=7 sub=1（击杀者取 hp==0 事件 source）。
//!
//! 车辆集合 = **Type5 class=2 实体类型 ∧ 结算花名册参与者**（协议实体种类：
//! 2=战斗车辆、3=区域可破坏物、8=炮塔残骸；花名册是显示车辆的权威名单——观察者/
//! 中途离开等未进入 battle_results 的实体不渲染）。type=10 位姿流同样出现在相机/
//! 区域实体上，prop2 炮塔角只在炮塔转动时广播——二者都不是"是不是车"的判据。
//! 昵称联表失败（UTF-8 非法等）的车辆同样不进入显示集合（昵称域见 combat::nickname）。
//!
//! 序列化约定：位姿为列式 flat 数组（`pos` = [x,y,z]×N，其余各 N 项），时刻 `t_i = t_start + i*0.1`；
//! 前端线性插值即可（hull_yaw/turret_yaw 为解卷绕连续域，见上）。
//! **渲染位姿另有 `pose_kf`（关键帧折线）**：`pos`/`hull_yaw`/`hull_pitch` 的 10Hz 网格
//! 会把滤波器的保持-跳变阶梯混叠成速度摆动（见 [`PoseKeyframes`]），渲染消费方应以
//! `pose_kf` 为准（缺省 = 旧 facet，回退网格插值）；`turret_yaw`/`gun_pitch`/`hull_roll`
//! 仍只有网格列（prop2/原始采样语义，不参与折线）。
//! `coverage` = 有效数据区段（原始采样间隙 >2s 视为
//! AoI 空洞——滤波器在无输入期会原地站住，前端按此隐藏车辆避免"幽灵车停在过期位置"）。

use std::collections::{BTreeMap, HashMap, HashSet};
/// serde skip_serializing_if 助手：`false` 按缺省处理（= 无已证实目标）
fn is_false(b: &bool) -> bool {
    !*b
}
/// serde skip_serializing_if 助手：死亡时刻表 ≤1 条时按缺省处理（单死已由 `death_t`
/// 完整表达；≥2 次 = 复活模式才输出，普通回放零扰动）
fn vehicle_death_times_trivial(v: &[f32]) -> bool {
    v.len() <= 1
}

use anyhow::bail;
use serde::Serialize;

use super::combat::{
    self, AoiPresence, AssaultBaseStateTransition, ConsumableTransition, GunPitchLimits,
    ModuleCrewStateEvent, RawReloadDuration, RawReloadPhase, ShotReplayData,
    SupremacyBaseStateTransition, SupremacyPointsSample, VehicleLoadout,
};
use super::filter;

/// 位姿网格步长（秒）——与现有 render_timeline / prop2 密集采样同惯例
pub const GRID_DT: f32 = 0.1;
/// 位姿关键帧走廊容差：位置（米）。2cm 在典型机位（20~50m）下约 1px 量级，仍远超网格路径精度
const KF_TOL_POS: f32 = 0.02;
/// 位姿关键帧走廊容差：角度（弧度 ≈ 0.40°）
const KF_TOL_ANG: f32 = 0.007;
/// 原始采样间隙超过此值视为 AoI 空洞（coverage 断开）
const COVERAGE_GAP: f32 = 2.0;

/// 玩家身份（battle_results 联表产物，调用方组装；replay 层不依赖 TankResolver）
#[derive(Debug, Clone)]
pub struct PlaybackPlayer {
    pub account_id: u32,
    pub nickname: String,
    /// 1 / 2（0 = 未知）
    pub team: u8,
    pub tank_id: u32,
}

/// 构建输入：packets + 战绩联表 + 俯仰极限锚定表 + 地图元信息
/// （`players`/`author_account_id` 仅由 [`build_playback_data`] 用于
/// [`crate::replay::model::ReplayModel::scan`] 联表；投影层身份一律取模型，
/// 见 [`PlaybackRenderInput`]）。
pub struct PlaybackInput<'a> {
    pub packets: &'a [(u32, f32, &'a [u8])],
    pub players: Vec<PlaybackPlayer>,
    pub author_account_id: u32,
    /// 胜方队伍（1/2，0 = 平局/未知）
    pub winner_team: u8,
    pub map_id: u32,
    pub map_name: String,
    pub pitch_limits: &'a GunPitchLimits,
    /// tank_id → 坦克名（调用方由 TankResolver 预解析；缺失可传空表）
    pub tank_names: HashMap<u32, String>,
}

/// 投影（渲染）入参：identity 一律来自 [`crate::replay::model::ReplayModel`] 实体并表
/// （eid → 昵称/account_id/team/tank_id/is_author），本结构**不含花名册**——投影层从
/// 类型上不可能重新 JOIN（身份域单一事实源，防昵称规则漂移）。
pub struct PlaybackRenderInput<'a> {
    /// 胜方队伍（1/2，0 = 平局/未知）
    pub winner_team: u8,
    pub map_id: u32,
    pub map_name: String,
    pub pitch_limits: &'a GunPitchLimits,
    /// tank_id → 坦克名（调用方由 TankResolver 预解析；缺失可传空表）
    pub tank_names: &'a HashMap<u32, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackMeta {
    pub map_id: u32,
    pub map_name: String,
    pub winner_team: u8,
    /// 作者队伍（友方视角配色基准）
    pub friendly_team: u8,
    /// 作者车辆实体 id（0 = 未解析）
    pub author_eid: u32,
    /// 全局网格起点（回放时钟域，秒）
    pub t_start: f32,
    /// 网格点数；时刻 t_i = t_start + i × 0.1s
    pub samples: usize,
    /// 总时长（秒）= t_start + (samples−1) × 0.1
    pub duration: f32,
}

/// 一辆车的全场时间线（列式；N = meta.samples，无效段由 coverage 控制）
#[derive(Debug, Clone, Serialize)]
pub struct VehicleTrack {
    pub eid: u32,
    pub account_id: u32,
    /// type=5 昵称（UTF-8 全域；缺失为空串 → 前端显示 Unknown）
    pub nickname: String,
    pub tank_id: u32,
    pub tank_name: String,
    /// 1 / 2（0 = 未知，前端灰色渲染）
    pub team: u8,
    pub is_author: bool,
    pub max_hp: u16,
    /// 车体位置 flat [x,y,z] × N（回放世界系，米）
    pub pos: Vec<f32>,
    /// 车体偏航（弧度，解卷绕连续域，可超 ±π）× N
    pub hull_yaw: Vec<f32>,
    /// 车体俯仰（弧度）× N
    pub hull_pitch: Vec<f32>,
    /// 车体侧倾（弧度）× N。**与 hull_pitch 不同源**：俯仰取自渲染滤波（AvatarFilter）输出，
    /// 而滤波层不输出侧倾，故本列取**原始 type=10 volatile 采样**的最近邻（与
    /// `combat::anchors::select_anchor_state` 对 pitch/roll 的规则一致：段内不插值，
    /// 不跨 AoI 断段编造中间姿态）；无采样时保持最近已知值。消费端可忽略（0 = 水平）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hull_roll: Vec<f32>,
    /// 炮塔绝对朝向（弧度，解卷绕连续域）= prop2 相对角 + 同刻车体 yaw 再解卷绕 × N
    pub turret_yaw: Vec<f32>,
    /// 炮管俯仰（弧度，正=仰角；无俯仰极限锚定时 = 车体 pitch 兜底）× N
    pub gun_pitch: Vec<f32>,
    /// HP 变化点 [(t, hp)]（含满血锚点）
    pub hp: Vec<(f32, u16)>,
    pub death_t: Option<f32>,
    /// 击杀者实体（method1 hp==0 事件 source；0 = 未知/环境）
    pub killer_eid: u32,
    /// 全部死亡时刻（升序；`death_times[0] == death_t`）。**仅当该车死亡 ≥2 次**
    /// （复活模式，如爆燃）才输出；单死/无死回放整字段省略，既有消费方零扰动。
    /// 第 n 次死亡与第 n-1 次之间必有复活证据（`resurrection_times`）。
    #[serde(default, skip_serializing_if = "vehicle_death_times_trivial")]
    pub death_times: Vec<f32>,
    /// 复活时刻（升序）：死亡后生命证据（method1/prop3 绝对血量）从终态回归正值的时刻。
    /// **仅非空时输出**。生命状态重建：death_t 起死亡，至首个 ≥ 该时刻的复活点回 ALIVE，
    /// 与后续死亡交替——复活证据缺失时该字段缺省，消费方按死亡终态语义处理（fail-closed）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resurrection_times: Vec<f32>,
    /// 该车发射过的弹种全局 id（去重，最多 16 个）——回放事实透传（消费端可自行
    /// 比对弹表；曾用作配置推断证据，2026-10 起配置解析只认 comp blob 精确对号）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shell_ids: Vec<u32>,
    /// 开局 loadout raw item 描述符（Type5 `0A <k>`；恒保留供回滚/未解码字段诊断）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loadout_items: Vec<[u8; 14]>,
    /// 9 字节装备选择串（每字节 = 装备数值 ID 的 ASCII 码点；缺省不输出）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipment: Option<[u8; 9]>,
    /// 开局 loadout 解码视图：3 消耗品 / 0..3 给养 / 模式额外能力 / 配件 ID。
    /// additive（旧消费方忽略；给养与模式能力已按 `f32 -1.0` 静态尾分流）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loadout: Option<VehicleLoadout>,
    /// 实际搭载的炮塔配置（build_configs dense 索引；None = 证据不足，前端用顶级配置）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret_index: Option<u32>,
    /// 实际搭载的主炮配置（dense 索引；语义同上）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gun_index: Option<u32>,
    /// 实际搭载配置在坦克数据 `configs[]` 数组中的下标（与 shots 的 `shooter_config_idx`
    /// 同域）：`configs[config_idx].burst_size` = 该车实际搭载主炮的弹夹容量——装填条
    /// 弹容 N 的权威取值（多炮坦克各炮弹容不同，**禁止**跨配置取最大/用剩余弹数+1 推断）。
    /// None = 证据不足（configs 唯一或证据链未命中），消费端回退顶级配置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_idx: Option<u32>,
    /// 实际搭载主炮的弹夹容量（= `configs[config_idx].burst_size` 原值；**0 = 单发**，
    /// 消费端 ≤1 都读作单发）。config_idx 未解析但 configs 唯一时仍给出（唯一配置无歧义）；
    /// None = 坦克数据缺失或配置证据链未命中，消费端按单发（1）处理、不猜。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burst_size: Option<u32>,
    /// comp blob（ARENA_INFO subtype 1）透传的炮塔模块局部 id（module_id>>8）——
    /// **纯回放证据**，与坦克数据无关：消费端用 `configs[].turret_local` 联表即可在
    /// 无坦克数据注入的路径（WASM 客户端）自行钉定实际搭载配置。None = 无 comp 证据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turret_local: Option<u16>,
    /// comp blob 透传的主炮模块局部 id（语义同上）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gun_local: Option<u16>,
    /// 有效数据区段 flat [t0, t1, ...]（闭区间对）
    pub coverage: Vec<f32>,
    /// 位姿关键帧折线（渲染层**精确**表示，见 [`PoseKeyframes`]）。旧 facet 缺省 = None，
    /// 消费端此时回退到 10Hz 网格列（`pos`/`hull_yaw`/`hull_pitch`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pose_kf: Option<PoseKeyframes>,
}

/// 位姿关键帧折线：渲染层的**显示重建**折点序列。消费端在相邻关键帧间线性插值。
///
/// 两条语义必须分开：
/// - **判定锚点**（`filter::FilteredTimeline` / `SegmentedPoseTimeline::build`）严格复现客户端
///   `setFilterOnEntity` 复位：8 槽按 0.01s 铺开、latency 初值 0.02s。客户端滤波器在段首
///   2~5s 内逐帧输出**保持-跳变阶梯**（latency 尚未收敛到 10Hz 输入间隔）。
/// - **渲染折线**（本结构）由 [`filter::SegmentedPoseTimeline::build_display`] 生成：每个
///   在场段按 10Hz 标称间隔铺满 8 槽、latency 从稳态 0.2s（两个更新间隔）起步——段首只保留
///   0.2s 静止（实体物化），其后输出时刻始终落在两个真实采样之间，逐帧匀速插值。游戏内这
///   段阶梯被模型加载/入场遮挡，我们的查看器模型常驻，若不重建就是用户可见的"刚进视野
///   一顿一顿"。**不得**把该重建用于命中/炮口等判定锚点。
///
/// 为什么不能只给 10Hz 网格：段首阶梯按固定网格重采样会被混叠成速度大幅摆动的滑行
/// （实测某 0.5s 窗内前端线速度 4.5→29.6 m/s，同段真值稳定）；稳态段网格相位误差也会
/// 在关键帧切换处暴露。折线保留真实折点，点数 7~10 点/秒/车。
///
/// 时序语义与网格列一致：早于首点/晚于末点按端点保持。
#[derive(Debug, Clone, Default, Serialize)]
pub struct PoseKeyframes {
    /// Full rigid-body roll for detached turrets only; omitted on vehicle tracks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roll: Vec<f32>,
    /// 关键帧时刻（秒，回放时钟域，升序）
    pub t: Vec<f32>,
    /// flat [x,y,z] × K（回放世界系，米；与 `pos` 网格同坐标系）
    pub pos: Vec<f32>,
    /// 车体偏航（弧度，**解卷绕连续域**；与 `hull_yaw` 网格同域——相邻关键帧 |Δ| < π，
    /// 消费端朴素线性插值即物理正确）
    pub yaw: Vec<f32>,
    /// 车体俯仰（弧度；与 `hull_pitch` 网格同域）
    pub pitch: Vec<f32>,
}

/// 从 60Hz 渲染帧拟合关键帧折线（公开给探针/测试；facet 层在 `VehicleTrack.pose_kf` 落盘）。
///
/// 段化输入（`SegmentedPoseTimeline`）：逐段生成候选帧（yaw 解卷绕链跨段延续）后统一
/// 贪心走廊——跨段边界（AoI 隐藏期）的帧间跳变必然超容差 → 自动保留为陡斜率段，
/// 与「保持→跳变」同型；隐藏期本身不可见（消费端 visibility/coverage 门禁），插值不暴露。
///
/// 贪心走廊：从帧 i 起尽量延长到 j，使 [i, j] 内所有中间帧与线性插值之差 ≤ 容差
/// （位置 = 欧氏距离，角度 = 最短弧差）。走廊跨越「保持→跳变」时中间帧偏差必然超限
/// → 跳变被保留为一段陡斜率，而不是被抹平。
pub fn pose_keyframes(tl: &filter::SegmentedPoseTimeline) -> PoseKeyframes {
    let mut frames: Vec<KfPos> = Vec::new();
    let mut prev_yaw: Option<f32> = None;
    for (_, t_out, seg) in tl.segments() {
        let (start, _) = seg.time_range();
        let n = seg.frame_count();
        for k in 0..n {
            // 帧中点查询：floor((t−start)/dt) 恒等 = k，规避 1/60 累加的浮点边界
            let t = start + (k as f64 + 0.5) * filter::FRAME_DT;
            let Some(p) = seg.pose_at(t, false) else {
                break;
            };
            // 有界段只输出到 t_out：段末时间线自带 +0.5s 外推尾部；短隐藏段
            // （0.1~0.5s）时它会越过下一段 t_in，使全局关键帧时刻非单调、消费端
            // 二分/线性插值跨隐藏段滑移。截断后 prev 段末 ≤ t_out ≤ 下一段 t_in。
            if t_out.is_some_and(|end| p.time > end as f64) {
                break;
            }
            // 航向解卷绕（与 `hull_yaw` 网格列同域）：消费端朴素线性插值即物理正确，
            // ±π 边界不出现 ≈2π 跳变（否则关键帧跨 ±π 时插值会反甩一整圈）
            let yaw = unwrap_angle(prev_yaw, p.ang[0]);
            prev_yaw = Some(yaw);
            frames.push(KfPos {
                t: p.time,
                pos: p.pos,
                yaw,
                pitch: p.ang[1],
            });
        }
    }
    // 消费端按升序二分（trackInterp.keyframeAt）；段截断后的单调性是公开契约。
    for w in frames.windows(2) {
        debug_assert!(
            w[0].t <= w[1].t,
            "pose_kf 时刻必须升序：{} > {}",
            w[0].t,
            w[1].t
        );
    }
    let mut out = PoseKeyframes::default();
    if frames.is_empty() {
        return out;
    }
    // 落盘舍入（pos 1cm / 角度 0.001rad / 时刻 0.1ms）：未舍入的 f32 按最短往返表示序列化
    // （≈9 字符/数），体积大 ~40%。时刻取 0.1ms 而不是 1ms——跳变段只有 ~17ms 长，毫秒级
    // 时间舍入会把该段斜率改变百分之几（1.4m 跳变 → 中段偏差 ~2cm），0.1ms 下可忽略。
    // 保持段内两端取值相同，舍入后仍逐值相等（阶梯不被破坏）。
    let push = |out: &mut PoseKeyframes, f: &KfPos| {
        out.t.push(r4(f.t as f32));
        out.pos
            .extend_from_slice(&[r2(f.pos[0]), r2(f.pos[1]), r2(f.pos[2])]);
        out.yaw.push(r3(f.yaw));
        out.pitch.push(r3(f.pitch));
    };
    push(&mut out, &frames[0]);
    let mut i = 0usize;
    while i + 1 < frames.len() {
        let mut j = i + 1;
        while j + 1 < frames.len() && kf_within(&frames, i, j + 1) {
            j += 1;
        }
        push(&mut out, &frames[j]);
        i = j;
    }
    out
}

/// 走廊判据：[i, j] 段内所有中间帧与线性插值的偏差是否都在容差内（角度 = 最短弧差）。
fn kf_within(frames: &[KfPos], i: usize, j: usize) -> bool {
    let a = frames[i];
    let b = frames[j];
    let span = b.t - a.t;
    if span.is_nan() || span <= 0.0 {
        return false;
    }
    let dyaw = wrap_pi(b.yaw - a.yaw);
    for f in &frames[i + 1..j] {
        let u = ((f.t - a.t) / span) as f32;
        let mut d2 = 0.0f32;
        for c in 0..3 {
            let v = a.pos[c] + (b.pos[c] - a.pos[c]) * u;
            d2 += (v - f.pos[c]) * (v - f.pos[c]);
        }
        if d2 > KF_TOL_POS * KF_TOL_POS {
            return false;
        }
        if wrap_pi(f.yaw - a.yaw - dyaw * u).abs() > KF_TOL_ANG {
            return false;
        }
        if (f.pitch - (a.pitch + (b.pitch - a.pitch) * u)).abs() > KF_TOL_ANG {
            return false;
        }
    }
    true
}

/// 走廊拟合的中间帧（= 一帧 60Hz 渲染位姿）
#[derive(Clone, Copy)]
struct KfPos {
    t: f64,
    pos: [f32; 3],
    yaw: f32,
    pitch: f32,
}

/// 一发射击（弹道飞行 + 结果标记；原始语义透传，前端做标签映射）
#[derive(Debug, Clone, Serialize)]
pub struct PlaybackShot {
    /// 开火时刻（回放时钟域，秒）
    pub t_fire: f32,
    pub shooter_eid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_eid: Option<u32>,
    /// 炮口（method29 launchPoint）
    pub from: [f32; 3],
    /// 弹道终点（method20，穿透出射/停止点）
    pub to: [f32; 3],
    /// 飞行时长（秒）：**折线各段时长之和**（直射弹退化为 |to−from| / |launch_velocity|）
    pub flight_secs: f32,
    /// 弹道折线中间点（跳弹/穿透出射点，按飞行顺序；直射弹缺省）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via: Vec<[f32; 3]>,
    /// 各段时长（秒）：段 0 = from→via[0]，…，末段 = …→to；`len == via.len() + 1`（缺省 = 单段）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leg_secs: Vec<f32>,
    pub shell_speed: f32,
    /// 是否命中车辆（有可解析目标）
    pub hit: bool,
    /// hit_flags 0x0008 跳弹位
    pub ricochet: bool,
    /// 游戏命中结果枚举（0=无 1=未击穿 2=间隙止 3=有伤害 4=履带/模块 255=未获取）
    pub game_hit_result: u8,
    pub damage: u32,
    pub is_kill: bool,
    pub is_author: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub shell_kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub shooter_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target_name: String,
    /// 受击方**弹着时刻**的 armorsStates 状态号（Vehicle propID 16，additive；skip-when-none）。
    /// None = 受击车无状态流或弹着早于首个样本（首样本 = 入视野 create 快照；
    /// 更早区间以声明文件 `data/armor_states.json` 的 `<default/>` 态兜底）。仅动态装甲车
    /// （全库 6 辆）会有值；消费方（射击复现）按声明门控后用该态的激活板集做等效装甲。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_armor_state: Option<u8>,
}

/// 击杀事件（kill feed）
#[derive(Debug, Clone, Serialize)]
pub struct KillEvent {
    pub t: f32,
    pub killer_eid: u32,
    pub victim_eid: u32,
    /// method1 cause 原始值（0=炮弹直击 1=火焰 2=撞击 3=世界/环境 5=溺水；255=未获取）
    pub cause: u8,
    /// wrapper6 >50% 先前伤害助攻者（官方击杀通知同款；无则 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assister_eid: Option<u32>,
    /// wrapper6 非默认死亡原因（1=火 2=撞 3=世界 5=溺水；缺省=普通击毁）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub death_reason: Option<u32>,
}

/// 实际搭载配置描述符（updateArena subtype 1 ARENA_INFO 的玩家条目字段）：
/// comp blob 15B = `[tank_id u16][chassis_local u16][engine_local u16][204 u32][turret_local u16][gun_local u16][00]`，
/// local = item_defs 模块局部 id（module_id >> 8，与 tanks.pb module_id 同基）。确定性搭载证据。
/// 条目为 protobuf 结构（P3 探针 84 条目实证）：`[f1=eid varint][f2=comp blob]
/// [f3=昵称][f4/f5=标志][f7=account_id varint][f8=公会标签]…`——eid 与 account_id
/// 是身份联表的**精确键**（重名/匿名昵称歧义的根除路径）。
#[derive(Debug, Clone)]
pub struct CompDescriptor {
    /// blob 所属玩家昵称（条目 field 1.3，原始 UTF-8 不经 ascii 过滤）
    pub nickname: String,
    /// 条目 field 1.1：该玩家实体 eid（0 = 条目起点未识别，退回昵称联表）
    pub eid: u32,
    /// 条目 field 1.7：account_id（0 = 未提取；与 battle_results 花名册精确联表，
    /// 83/84 条目实证全命中，唯一 miss 恰为花名册截断样本）
    pub account_id: u64,
    pub tank_id: u32,
    pub turret_local: u16,
    pub gun_local: u16,
}

impl CompDescriptor {
    /// 从 ARENA_INFO args（去 [subtype][len] 头后的 protobuf）提取**全部**搭载描述符：
    /// field 1.2 = blob、field 1.3 = 昵称、field 1.1 = eid、field 1.7 = account_id。
    /// 单条 subtype=1 update 携带**全场玩家**的 comp blob（P1 探针 9 场实测：1 条
    /// update / blob 数=花名册人数），必须逐 blob 收集而非取首个。容错扫描：blob 以
    /// tank_id u16le 开头且长度 15、前置标记 `12 0f`——对分组/前缀等编码变体稳健。
    fn parse_args(args: &[u8], valid_tanks: &[u32]) -> Vec<Self> {
        let mut out = Vec::new();
        for i in 2..args.len().saturating_sub(14) {
            if args[i - 2] != 0x12 || args[i - 1] != 0x0f {
                continue;
            }
            let blob = &args[i..i + 15];
            // tank 白名单按低 16 位匹配（blob 只携带低 16 位 tank_id；全宽 id 的
            // 国家位扩展不受影响——与各消费方的 masked 校验同式）
            let tank_id = u16::from_le_bytes([blob[0], blob[1]]) as u32;
            if !valid_tanks.iter().any(|&t| (t & 0xFFFF) == tank_id) {
                continue;
            }
            let turret_local = u16::from_le_bytes([blob[10], blob[11]]);
            let gun_local = u16::from_le_bytes([blob[12], blob[13]]);
            if turret_local == 0 || gun_local == 0 {
                continue;
            }
            // 昵称 = blob 之后首个 `1a <len> <可打印 UTF-8>`（长度域 1..=255 与 type=5
            // 侧 a3efede 同域——超长 UTF-8 昵称在真实对局存在（实测 39B），旧 3..=30 窗
            // 会静默丢 comp；60B 扫描窗实际覆盖 ~42B，同量级）；
            // 返回 account 字段的扫描起点（昵称字段结束处；未识别昵称布局时从 blob 尾起）
            let mut nickname = String::new();
            let mut scan_from = i + 15;
            for j in (i + 15)..args.len().min(i + 60) {
                if args[j] != 0x1a {
                    continue;
                }
                let l = args[j + 1] as usize;
                if l == 0 || j + 2 + l > args.len() {
                    continue;
                }
                if let Ok(s) = std::str::from_utf8(&args[j + 2..j + 2 + l]) {
                    if s.chars().all(|c| !c.is_control()) {
                        nickname = s.to_string();
                        scan_from = j + 2 + l;
                    }
                }
                break;
            }
            let (eid, account_id) = Self::entry_identity(args, i, scan_from);
            out.push(Self {
                nickname,
                eid,
                account_id,
                tank_id,
                turret_local,
                gun_local,
            });
        }
        out
    }

    /// 条目身份字段提取（P3 探针定案，84 条目实证 account 83/84 与花名册精确相等）：
    /// eid = 反向找条目起点 `[0a][LEN varint][08 <eid varint>]`（LEN 为 varint，
    /// 带公会标签的条目 >127B 时 2 字节）；account = 从昵称后前向扫条目内 varint
    /// 字段（tag 低 3 位 = 0 且 field 4..=10，取 field 7）。任一环节失败 → 0
    ///（消费端 0 = 未识别，退回昵称联表，与全仓 author_eid=0 语义一致）。
    fn entry_identity(args: &[u8], blob_at: usize, scan_from: usize) -> (u32, u64) {
        let varint = |b: &[u8], o: usize| -> Option<(u64, usize)> {
            let mut v = 0u64;
            for k in 0..10 {
                let byte = *b.get(o + k)?;
                v |= ((byte & 0x7f) as u64) << (7 * k);
                if byte & 0x80 == 0 {
                    return Some((v, k + 1));
                }
            }
            None
        };
        let mut eid = 0u32;
        for back in 3..=12usize {
            let Some(e) = blob_at.checked_sub(back) else {
                continue;
            };
            if args[e] != 0x0a {
                continue;
            }
            let Some((_, lenlen)) = varint(args, e + 1) else {
                continue;
            };
            let cs = e + 1 + lenlen;
            if cs < args.len() && args[cs] == 0x08 {
                if let Some((v, _)) = varint(args, cs + 1) {
                    eid = v as u32;
                }
                break;
            }
        }
        let mut account_id = 0u64;
        let mut j = scan_from;
        while j < args.len().min(blob_at + 90) {
            let tag = args[j];
            if tag & 0x07 != 0 || !(4..=10).contains(&(tag >> 3)) {
                break;
            }
            let Some((v, used)) = varint(args, j + 1) else {
                break;
            };
            if tag >> 3 == 7 {
                account_id = v;
            }
            j += 1 + used;
        }
        (eid, account_id)
    }
}

/// 收集全部玩家实际搭载描述符：subtype=1 的 updateArena（ARENA_INFO，**单条 update
/// 携带全场玩家的 comp blob**，P1 探针 9/9 场实证；逐 blob 提取，键=昵称）。
/// `valid_tanks` = battle_results 里的 tank_id 全集（blob[0..2] 白名单，防误配）。
pub fn collect_comp_descriptors(
    packets: &[(u32, f32, &[u8])],
    valid_tanks: &[u32],
) -> HashMap<String, CompDescriptor> {
    comp_descriptors_from_updates(
        &combat::collect_arena_updates_filtered(packets, |s| s == 1),
        valid_tanks,
    )
}

/// [`collect_comp_descriptors`] 的共享扫描形态：从已收集的 ARENA_INFO（subtype=1）
/// update 流提取（`ReplayModel::scan` 与 kill_feed/periods 合用一次 arena pass 时复用）。
pub fn comp_descriptors_from_updates(
    updates: &[combat::ArenaUpdate],
    valid_tanks: &[u32],
) -> HashMap<String, CompDescriptor> {
    let mut out: HashMap<String, CompDescriptor> = HashMap::new();
    for u in updates {
        if u.subtype != 1 || u.payload.len() < 15 {
            continue;
        }
        for d in CompDescriptor::parse_args(&u.payload, valid_tanks) {
            out.insert(d.nickname.clone(), d);
        }
    }
    out
}

/// 把 comp blob 的模块局部 id 透传到车辆 facet（`turret_local` / `gun_local`）。
/// **纯回放证据，无坦克数据依赖**——服务器与 WASM（客户端）路径共用：消费端用
/// `configs[].turret_local`/`gun_local` 联表即可在无坦克数据注入的路径（浏览器 WASM）
/// 钉定实际搭载配置（实际搭载配置解析的上游证据链见 tank_configs::resolve_config_index）。
/// 匹配规则 = **eid 优先**（条目 field 1.1，精确键；P3 探针定案），退回昵称 +
/// tank_id 低 16 位（eid 未识别的历史形态）。
pub fn annotate_vehicle_comp_locals(
    vehicles: &mut [VehicleTrack],
    comps: &HashMap<String, CompDescriptor>,
) {
    let by_eid: HashMap<u32, &CompDescriptor> = comps
        .values()
        .filter(|c| c.eid != 0)
        .map(|c| (c.eid, c))
        .collect();
    for v in vehicles.iter_mut() {
        if v.tank_id == 0 {
            continue;
        }
        let matched = by_eid
            .get(&v.eid)
            .copied()
            .or_else(|| comps.get(&v.nickname));
        if let Some(c) = matched {
            if (c.tank_id & 0xFFFF) == (v.tank_id & 0xFFFF) {
                v.turret_local = Some(c.turret_local);
                v.gun_local = Some(c.gun_local);
            }
        }
    }
}

/// 战局阶段点（updateArena PERIOD；前端据此显示战斗计时器）
#[derive(Debug, Clone, Serialize)]
pub struct PeriodPoint {
    pub clock: f32,
    pub period: u64,
    pub remaining_s: f64,
    pub duration_s: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetachedTurretTrack {
    pub eid: u32,
    pub vehicle_eid: u32,
    /// Closed raw-clock AoI intervals. Hold the final known pose inside each span.
    pub coverage: Vec<f32>,
    /// Raw Type5/Type10 world poses; all three Euler angles are unwrapped.
    pub pose_kf: PoseKeyframes,
}

/// 作者当前弹种变更事件（`shell_id` = 全局弹种 id，与回放 shell 表同域）
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AuthorShellEvent {
    pub t: f32,
    pub shell_id: u32,
}

/// method 0x07 确认流 → 弹种变更时间线：a0=0/1 成对同值与重复广播折叠为一次变更。
/// 全局弹种 id 域为 24 位（与 terrain_impacts 同掩码）。
/// 作者首次死亡起 0x07 可能跟随被观战车辆，此后的广播不投影；按未取整时钟比较，
/// 避免边界被 `r2` 挪动。作者未识别时无从截断，不输出。
fn author_shell_timeline(timeline: &crate::replay::model::Timeline) -> Vec<AuthorShellEvent> {
    let mut out: Vec<AuthorShellEvent> = Vec::new();
    if timeline.author_eid == 0 {
        return out;
    }
    let first_death = timeline.deaths.get(&timeline.author_eid).map(|d| d.t);
    for (clock, raw) in &timeline.shell_broadcasts {
        if first_death.is_some_and(|death| *clock >= death) {
            continue;
        }
        let shell_id = raw & 0x00FF_FFFF;
        if out.last().is_none_or(|e| e.shell_id != shell_id) {
            out.push(AuthorShellEvent {
                t: r2(*clock),
                shell_id,
            });
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackData {
    /// Authoritative class-8 detached rigid bodies, linked to vehicles[].eid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detached_turrets: Vec<DetachedTurretTrack>,
    /// 切面契约版本（**当前 2**；不兼容变更递增，同版本只加可选字段；消费端按版本门禁拒绝错版）
    #[serde(default)]
    pub version: u32,
    /// Supremacy 基地状态时间线（争霸模式；非争霸场为空）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supremacy_bases: Vec<SupremacyBaseStateTransition>,
    /// Supremacy 实时点数采样（仅真实广播；非争霸场为空）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supremacy_points: Vec<SupremacyPointsSample>,
    /// 攻防战/遭遇战单基地目标存在性（wrapper8/root8 目标族出现即真，**不要求有进度**）。
    /// 缺省/ false = 无已证实的单基地目标（与 WotbTools `assaultObjectivePresent` 同义）。
    #[serde(default, skip_serializing_if = "is_false")]
    pub assault_objective_present: bool,
    /// 攻防战单基地占领进度（wrapper8/root8；非攻防战场次为空）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assault_bases: Vec<AssaultBaseStateTransition>,
    /// 消耗品生命周期事件（Type32 flag=0；非消耗品场次为空）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumables: Vec<ConsumableTransition>,
    /// 车辆模块/乘员状态事件（Avatar method16；无事件场次为空）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub module_crew_states: Vec<ModuleCrewStateEvent>,

    /// 区域可破坏物实体（100m 格子锚点；type=5 class=3，eid 升序）。
    /// `destructible_events[].area_eid` 联表本列表得事件所属格子。
    /// 契约 additive：消费方忽略未知键。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destructible_areas: Vec<crate::replay::destructibles::DestructibleArea>,
    /// 区域可破坏物事件（type=32 短广播、envelope=区域实体；时钟升序）。
    /// `object_id` = 服务器侧 u16 物体 id（body_len=5/6 位置已验证；PARTIAL——
    /// 每图窄窗口、与客户端静态表不同域，跨图标定属资产管线）；
    /// `body_len`/`args` 原样透传。事件与车辆 AoI 解耦：未点亮车辆的破坏照常广播
    /// （受控实验 R132 8/8）。契约 additive：消费方忽略未知键。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destructible_events: Vec<crate::replay::destructibles::AreaDestructibleEvent>,

    /// 实时装填相位（arena subtype 15/16/17，**仅本方全队**；相位码 f2 与计数 f4 原样透传）。
    /// 语义表见 `combat::arena` 的相位常量块：f2=1 剩余弹数更新 / 3 整夹重装 / 4 中途时长变更 /
    /// 5 就绪（f4=1 是就绪标志，非剩余数）/ 6 弹鼓逐发补槽 / 7 夹内推弹（不补弹）/ 8 语义未定（禁猜，
    /// 样本仅 tank 21793 发出、无 f3/f4）；**除 f2=5 外 f4 = 服务器剩余弹数快照**
    /// （消费侧据此重锚，纠正本地外推漂移）。
    /// 契约 additive：同版本只加字段（消费方忽略未知键），不递增 playbacks 契约版本。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reloads: Vec<RawReloadPhase>,
    /// 权威「当前生效完整装填时长」（方法 35/0x23，**仅本方**）：作整夹重装（f2=3）相位的
    /// 时长刻度（相位 f3 只在相位起点给出，配置中途变化只能由本流补上）。
    /// 契约 additive：消费方忽略未知键。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reload_effective: Vec<RawReloadDuration>,
    /// 作者当前弹种时间线（method 0x07 服务器确认；切换即生效，无 selected/loaded 分离）。
    /// 首条 = 开局弹种。**只含作者首次死亡之前的确认**：之后的 0x07 可能属于被观战车辆，
    /// 已在投影时截断；末条之后到首次死亡之间保持末条弹种。
    /// 契约 additive：消费方忽略未知键。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub author_shell_timeline: Vec<AuthorShellEvent>,
    /// 原始 armorsStates 状态轨道（Vehicle propID 16：type=7 更新 + type=5 create 快照，
    /// 仅动态装甲车 `<extras><armorsStatesController>` 产生轨道，状态语义见
    /// data/armor_states.json；静态车为空、字段整体省略）。契约 additive：同版本只加字段，
    /// 消费方忽略未知键。行为学定案与门控要求见 `combat::RawArmorStateTrack` 文档。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub armor_states: Vec<crate::replay::combat::RawArmorStateTrack>,
    pub meta: PlaybackMeta,
    /// 实体 id 升序（确定性输出）
    pub vehicles: Vec<VehicleTrack>,
    /// 按开火时刻排序（作者 + 他人全部射击）
    pub shots: Vec<PlaybackShot>,
    /// true = 本场 shots 来自宽松降级路径（作者严格提取失败，缺 method38 增强字段；
    /// is_author 按 shooter_eid 回填）。正常场次为 false 且不序列化——消费端应据此
    /// 透出数据质量提示，不得当作严格路径产物展示。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shots_from_loose_path: bool,
    /// 按时刻排序的击杀事件
    pub kills: Vec<KillEvent>,
    pub periods: Vec<PeriodPoint>,
    /// AoI 可见窗口（仅收录车辆实体；Type33/5 物化开段、Type4 关段，重入 = 多段）。
    /// 语义来源 = collect_aoi_lifecycle（协议精确边界）；渲染插值防护仍看 coverage。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visibility: Vec<AoiPresence>,
}

/// Project the model's raw rigid bodies without AvatarFilter, terrain snapping,
/// artificial launch arcs or inferred AoI gaps. Initial Type5 is the first pose.
fn detached_turret_tracks(
    model: &crate::replay::model::ReplayModel,
    vehicles: &HashSet<u32>,
) -> Vec<DetachedTurretTrack> {
    use super::detached_turrets::valid_pose;
    model
        .timeline
        .detached_turrets
        .iter()
        .filter_map(|record| {
            if !vehicles.contains(&record.vehicle_eid) {
                return None;
            }
            let mut out = DetachedTurretTrack {
                eid: record.eid,
                vehicle_eid: record.vehicle_eid,
                coverage: Vec::new(),
                pose_kf: PoseKeyframes::default(),
            };
            let raw = model
                .timeline
                .poses
                .get(&record.eid)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if raw.iter().any(|p| !valid_pose(p)) {
                return None;
            }
            for span in &record.presence {
                let start = span.pose.clock;
                let end = span.t_out?;
                if !valid_pose(&span.pose) || !end.is_finite() || end < start {
                    return None;
                }
                out.coverage.extend_from_slice(&[start, end]);
                let mut last_t = start;
                let mut prev: [Option<f64>; 3] = [None; 3];
                let mut push = |p: &combat::St10Sample| -> Option<()> {
                    if !valid_pose(p) {
                        return None;
                    }
                    out.pose_kf.t.push(p.clock);
                    out.pose_kf.pos.extend_from_slice(&p.pos);
                    for (i, (dst, value)) in [
                        (&mut out.pose_kf.yaw, p.yaw),
                        (&mut out.pose_kf.pitch, p.pitch),
                        (&mut out.pose_kf.roll, p.roll),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        // f64 subtraction avoids overflowing finite wire f32s.
                        let angle = match prev[i] {
                            Some(last) => {
                                last + ((value as f64 - last + std::f64::consts::PI)
                                    .rem_euclid(std::f64::consts::TAU)
                                    - std::f64::consts::PI)
                            }
                            None => value as f64,
                        };
                        if !(angle as f32).is_finite() {
                            return None;
                        }
                        dst.push(angle as f32);
                        prev[i] = Some(angle);
                    }
                    Some(())
                };
                push(&span.pose)?;
                let mut samples = raw
                    .iter()
                    .filter(|p| p.clock > start && p.clock <= end)
                    .peekable();
                while let Some(mut p) = samples.next() {
                    // The replay may batch multiple received poses at one clock.
                    // Keep the final received real pose, without inventing subframe times.
                    while samples.peek().is_some_and(|next| next.clock == p.clock) {
                        p = samples.next().unwrap();
                    }
                    if p.clock <= last_t {
                        return None;
                    }
                    push(p)?;
                    last_t = p.clock;
                }
            }
            (!out.pose_kf.t.is_empty()).then_some(out)
        })
        .collect()
}

/// 位置保留 2 位小数（0.01m）；角度 3 位（0.057°，prop2 coarse 步进 ~0.35° 之下）
/// 侧倾取值：**最近邻**（不插值——与 `combat::anchors::select_anchor_state` 对
/// pitch/roll 的规则一致，避免跨 AoI 断段编造中间姿态）。`samples` 按时钟升序
/// （`Timeline::poses` 的契约）。无采样返回 None（调用方落 0 = 水平）。
fn roll_nearest(samples: &[combat::St10Sample], t: f32) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }
    let i = samples.partition_point(|s| s.clock < t);
    let pick = if i == 0 {
        0
    } else if i >= samples.len() {
        samples.len() - 1
    } else if (t - samples[i - 1].clock) <= (samples[i].clock - t) {
        i - 1
    } else {
        i
    };
    Some(samples[pick].roll)
}

fn r2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}
fn r3(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
}
/// 0.1ms 舍入（关键帧时刻：跳变段仅 ~17ms 长，毫秒级舍入会显著改变该段斜率）
fn r4(x: f32) -> f32 {
    (x * 10000.0).round() / 10000.0
}

/// 归一化到 [−π, π]（合成角规范化 + 解卷绕差值短弧化；落盘契约 = 解卷绕连续域，非短弧）
fn wrap_pi(x: f32) -> f32 {
    const PI: f32 = std::f32::consts::PI;
    const TAU: f32 = std::f32::consts::TAU;
    let mut v = x;
    while v > PI {
        v -= TAU;
    }
    while v < -PI {
        v += TAU;
    }
    v
}

/// 相位解卷绕：相对前值走短弧（相邻 0.1s 网格物理角差 ≪ π，连续性由探针 P3 哨兵守护）。
/// 滤波器 yaw 在 waypoint 对切换处、prop2 合成角在 ±π 边界都会出现 ≈2π 的数值跳变
/// （sin/cos 等价、逐帧渲染无害，但落盘后朴素线性插值会反甩 ~360°），解卷绕后序列连续。
fn unwrap_angle(prev: Option<f32>, v: f32) -> f32 {
    match prev {
        Some(p) => p + wrap_pi(v - p),
        None => v,
    }
}

/// 原始采样时刻 → coverage 闭区间对（间隙 > [`COVERAGE_GAP`] 断开）
fn build_coverage(clocks: &[f32]) -> Vec<f32> {
    let mut sorted: Vec<f32> = clocks.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut out = Vec::new();
    let Some(&first) = sorted.first() else {
        return out;
    };
    let mut start = first;
    let mut prev = first;
    for &c in &sorted[1..] {
        if c - prev > COVERAGE_GAP {
            out.push(r2(start));
            out.push(r2(prev));
            start = c;
        }
        prev = c;
    }
    out.push(r2(start));
    out.push(r2(prev));
    out
}

/// coverage 区段内的时刻判定（前端同式；仅测试消费）
#[cfg(test)]
fn coverage_contains(cov: &[f32], t: f32) -> bool {
    cov.as_chunks::<2>()
        .0
        .iter()
        .any(|p| t >= p[0] && t <= p[1])
}

/// 弹道直线飞行时长（秒）：|to−from| / |launch_velocity|，速度不可信时 0.5s 兜底
fn flight_secs(from: &[f32; 3], to: &[f32; 3], vel: &[f32; 3]) -> f32 {
    let speed = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
    let dist =
        ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
    if speed > 1.0 {
        (dist / speed).clamp(0.02, 8.0)
    } else {
        0.5
    }
}

/// 从射击复现数据映射回放弹道记录
fn to_playback_shot(s: &ShotReplayData) -> PlaybackShot {
    // 受击方身份 = 原始 eid 直传（method38/method8 服务器权威）——
    // eid 是身份域，名字仅显示域。
    let target_eid = s.target_eid;
    PlaybackShot {
        t_fire: r2(s.fire_time),
        shooter_eid: s.shooter_eid,
        target_eid,
        from: s.ball_a,
        to: s.ball_b,
        // 折线总时长（跳弹/穿透段按各自段速度计）：直射弹 = 直线估计，逐值不变
        flight_secs: r2(if s.leg_secs.is_empty() {
            flight_secs(&s.ball_a, &s.ball_b, &s.launch_velocity)
        } else {
            s.leg_secs.iter().sum()
        }),
        via: s
            .via
            .iter()
            .map(|p| [r2(p[0]), r2(p[1]), r2(p[2])])
            .collect(),
        leg_secs: s.leg_secs.iter().map(|x| r2(*x)).collect(),
        shell_speed: r2((s.launch_velocity[0] * s.launch_velocity[0]
            + s.launch_velocity[1] * s.launch_velocity[1]
            + s.launch_velocity[2] * s.launch_velocity[2])
            .sqrt()),
        hit: target_eid.is_some(),
        ricochet: s.hit_flags & 0x0008 != 0,
        game_hit_result: s.game_hit_result,
        damage: s.damage,
        is_kill: s.is_kill,
        is_author: s.is_author,
        target_armor_state: None, // 原始包证据：attach_packet_evidence 持 packets 回填
        shell_kind: s.shell_kind.clone(),
        shooter_name: s.shooter_name.clone(),
        target_name: s.target_name.clone(),
    }
}

/// 构建全场回放数据（兼容入口）：单次扫描建内部模型 → 回放切面投影。
/// fail 点不变：回放无任何实体姿态流（非对战文件/损坏）。
pub fn build_playback_data(input: &PlaybackInput) -> anyhow::Result<PlaybackData> {
    let model = crate::replay::model::ReplayModel::scan(&crate::replay::model::ScanInput {
        packets: input.packets,
        roster: &input.players,
        author_account_id: input.author_account_id,
        pitch_limits: input.pitch_limits,
    })?;
    let render = PlaybackRenderInput {
        winner_team: input.winner_team,
        map_id: input.map_id,
        map_name: input.map_name.clone(),
        pitch_limits: input.pitch_limits,
        tank_names: &input.tank_names,
    };
    let mut data = from_model(&model, &render)?;
    // armorsStates 是原始包证据（与 ai_review 的 raw tracks 同层）——渲染层（from_model）
    // 类型上不接触 packets（防投影层重新 JOIN），故在持有 packets 的本入口回填。
    attach_packet_evidence(&mut data, input.packets);
    Ok(data)
}

/// 原始包证据回填：armorsStates 时间轴 + 逐弹弹着时刻状态号。
///
/// 在**持有 packets 的入口**调用（`build_playback_data` / replay-wasm `playback_json`）；
/// 渲染层 `from_model` 类型上不接触 packets（防投影层重新 JOIN，见 PlaybackRenderInput
/// 文档），故回填在其后。幂等：重复调用以最新 packets 为准整体覆盖。
pub fn attach_packet_evidence(data: &mut PlaybackData, packets: &[(u32, f32, &[u8])]) {
    data.armor_states = crate::replay::combat::collect_armor_state_tracks(packets);
    for shot in &mut data.shots {
        let Some(eid) = shot.target_eid else { continue };
        let Some(track) = data.armor_states.iter().find(|t| t.eid == eid) else {
            continue;
        };
        shot.target_armor_state = track.state_at(shot.t_fire + shot.flight_secs);
    }
}

/// 车辆实体集合：**Type5 class=2 ∧ 结算花名册联表成功 ∧ 至少一条 type=10 采样**
/// （BTreeMap 保证确定性顺序）。
///
/// class=2 是回放协议对"战斗车辆"的权威实体类型声明（同族 class=3 区域可破坏物、
/// class=8 炮塔残骸都有 type=10 移动流，不能按流的有无判断）。实体类型与炮塔是否
/// 转动无关：全程静止、炮塔不转的车辆没有 prop2 流，但仍是车辆——2026-10-19 用户
/// 报障样本（训练房 2 人，对手 SU-130PM 全程未转炮塔）旧实现按 `st10 ∧ prop2`
/// 双流交集把对手整条丢掉，播放切面只剩作者一辆车。也**不**设采样数阈值：
/// 类型已由协议确定，稀疏采样不是"可能不是车"的证据。
///
/// 显示名单再以**结算花名册**为准：花名册联表失败的 class=2 实体（观察者、中途离开、
/// 昵称损坏等）不进入 `vehicles[]`——用户口径（2026-10-20）：只保留结算花名册中出现过的。
///
/// 没有 type=10 采样的车辆拿不到位姿，无法生成轨迹列（不是按内容统计筛掉）。
fn vehicle_candidates(
    st10: &HashMap<u32, Vec<combat::St10Sample>>,
    vehicle_eids: &HashSet<u32>,
    roster_eids: &HashSet<u32>,
) -> BTreeMap<u32, usize> {
    st10.iter()
        .filter(|(eid, _)| vehicle_eids.contains(eid) && roster_eids.contains(eid))
        .map(|(eid, samples)| (*eid, samples.len()))
        .collect()
}

/// 从内部模型投影回放切面：位姿滤波/0.1s 网格/角度解卷绕/击杀归属增强都在本层完成，
/// 模型只提供原始采样与事件；身份（昵称/账号/队伍/tank_id/作者标记）一律读模型实体并表。
pub fn from_model(
    model: &crate::replay::model::ReplayModel,
    render: &PlaybackRenderInput,
) -> anyhow::Result<PlaybackData> {
    // 作者档案 = 模型实体并表产物（identity 唯一事实源，本函数不再触碰花名册）
    let author_rec = model.entities.iter().find(|e| e.is_author);
    let author_nickname = author_rec
        .and_then(|e| e.nickname.clone())
        .unwrap_or_default();
    let author_player_eid = model.timeline.author_eid;

    // 实体档案索引：eid → 模型并表记录（昵称/账号/队伍/tank_id 全部取此）
    let ent_by_eid: HashMap<u32, &crate::replay::model::EntityRecord> =
        model.entities.iter().map(|e| (e.eid, e)).collect();

    // 实体索引：模型原始流（type=10 位姿 + prop2 炮塔；车辆身份见 timeline.vehicle_eids）
    let st10 = &model.timeline.poses;
    let prop2 = &model.timeline.turret;
    let entity_names = &model.timeline.entity_names;
    let vehicle_eids = &model.timeline.vehicle_eids;
    // 结算花名册参与者 = 显示车辆的权威名单（账号联表成功；观察者/中途离开无 account_id）
    let roster_eids: HashSet<u32> = ent_by_eid
        .iter()
        .filter(|(_, rec)| rec.account_id.is_some())
        .map(|(eid, _)| *eid)
        .collect();

    // 车辆实体 = Type5 class=2 ∧ 花名册联表成功 ∧ 有 type=10 位姿（BTreeMap 保证
    // 确定性顺序；见 vehicle_candidates——炮塔流是否存在与车辆身份无关）
    let candidates = vehicle_candidates(st10, vehicle_eids, &roster_eids);
    if candidates.is_empty() {
        bail!("无任何可显示车辆（Type5 class=2 ∧ 结算花名册联表成功 ∧ 有 type=10 位姿）");
    }

    // 全部弹道（模型一次扫描产物；时间轴末端扩展与列表共用）
    let shots_raw = &model.timeline.shots;

    // 时刻轴：全局 [min覆盖起点, max覆盖终点/阶段末/末弹] 网格
    let mut t_start = f32::MAX;
    let mut t_end = f32::MIN;
    for eid in candidates.keys() {
        let mut clocks: Vec<f32> = st10[eid].iter().map(|s| s.clock).collect();
        clocks.sort_by(|a, b| a.partial_cmp(b).unwrap());
        t_start = t_start.min(clocks[0]);
        // 时间线终点 = build 的末采样 +0.5s 外推域（filter.rs time_range）
        if let Some(&last) = clocks.last() {
            t_end = t_end.max(last + 0.5);
        }
    }
    let periods = &model.timeline.periods;
    for p in periods {
        t_end = t_end.max(p.clock);
    }
    for s in shots_raw.iter() {
        t_end = t_end.max(s.fire_time + flight_secs(&s.ball_a, &s.ball_b, &s.launch_velocity));
    }
    if !t_start.is_finite() || t_end <= t_start {
        bail!("姿态时间线为空（type=10 采样不足）");
    }
    let samples_n = ((t_end - t_start) / GRID_DT).ceil() as usize + 1;
    let meta = PlaybackMeta {
        map_id: render.map_id,
        map_name: render.map_name.clone(),
        winner_team: render.winner_team,
        friendly_team: author_rec.and_then(|e| e.team).unwrap_or(0),
        author_eid: author_player_eid,
        t_start: r2(t_start),
        samples: samples_n,
        duration: r2(t_start + (samples_n.saturating_sub(1)) as f32 * GRID_DT),
    };

    // 血量链与死亡终态（模型一次扫描产出，循环内查表）
    let initial_hp = &model.timeline.initial_hp;

    let mut vehicles_out: Vec<VehicleTrack> = Vec::with_capacity(candidates.len());
    for eid in candidates.keys() {
        // 段化滤波（D1）：按 AoI 在场段独立滤波 + Type5 物化快照段首种子——
        // 客户端每次进 AoI 新建滤波器；整场单实例会让重入帧从上一段末位滑移追赶
        let presences: Vec<crate::replay::combat::AoiPresence> = model
            .timeline
            .presence
            .iter()
            .filter(|p| p.eid == *eid)
            .cloned()
            .collect();
        let nickname = entity_names.get(eid).cloned().unwrap_or_default();
        // 身份 = 模型实体并表（scan 期一次 JOIN 的产物；此处只读取不重联）
        let rec = ent_by_eid.get(eid).copied();
        let is_author = rec.map(|r| r.is_author).unwrap_or(false);
        // 接收突发时间轴重建（v0.4.4 仅作者车 type=10；2026-10-31 用户决策扩展到全部
        // 车辆 + prop2 炮塔流，证据与口径见 filter::respace_same_clock_bursts 文档）：
        // 录制端按到达时间打 clock，网络停摆把 stall 期的 10Hz 快照压成同 clock 连包，
        // 如实重放 = 冻结→跳变/猛冲；按标称 10Hz 回填重建服务器权威快照流的原始时序。
        let mut st_respaced = st10[eid].clone();
        filter::respace_same_clock_bursts(&mut st_respaced);
        let st_series: &[combat::St10Sample] = &st_respaced;
        let Some(tl) = filter::SegmentedPoseTimeline::build(st_series, &presences) else {
            continue;
        };
        // 渲染折线单独按 Display 复位重建（段首稳态 0.2s 延迟）：网格列仍取客户端滤波
        // 输出（`tl`），只有 `pose_kf` 消费平滑版；判定锚点不受影响。
        let tl_display = filter::SegmentedPoseTimeline::build_display(st_series, &presences);
        let clocks: Vec<f32> = st_series.iter().map(|s| s.clock).collect();
        // 俯仰极限锚定：本车昵称优先；作者无锚定时沿用其表项（与作者路径 prop9 兜底同级）。
        // 有效极限 = 基础 models.pb 极限 + 该实体实际携带的俯仰配件（122：+4°仰 / +3°俯；
        // 证据 = 同 eid 的 Type5 `0B` 配件串）。无 122 时借用原值，不改变既有场次。
        let limits = render
            .pitch_limits
            .get(nickname.as_str())
            .or_else(|| {
                render
                    .pitch_limits
                    .get(author_nickname.as_str())
                    .filter(|_| is_author)
            })
            .map(|base| {
                combat::effective_pitch_limits(base, rec.and_then(|r| r.equipment.as_ref()))
            });
        let limits = limits.as_deref();

        // prop2 流：客户端只在炮塔转动时广播，本车全场未转炮塔 ⇒ 无流（与是否为车辆无关，
        // 见 vehicle_candidates）。此时按"炮塔随车体朝向（相对 0）、炮管水平"的中性口径
        // 渲染（客户端侧该车就是停着没瞄过；不给 angles 一个明确的缺省反而会让整场打不开）。
        // 渲染消费走**重建副本**（同 type=10 口径做接收突发回填）：同 clock 连包在
        // prop2_at 括号语义下中间帧不可达，stall 期整段炮塔旋转会渲染成单格跳变
        // （2026-10-31 神了 Kranvagn 实测 57.07°/0.1s）。判定锚点（prop2_at_arrived）
        // 不走本副本，保持原始到达时序。
        let prop2_respaced: Option<Vec<(f32, f32, u16)>> = prop2.get(eid).map(|s| {
            let mut v = s.clone();
            filter::respace_same_clock_bursts_prop2(&mut v);
            v
        });
        let prop2_series = prop2_respaced.as_ref();

        // 逐网格采样：渲染位姿 + prop2 炮塔/俯仰（同刻合成绝对角）；角度序列解卷绕落盘。
        // 列式数组与网格严格同长——任何跳过都会静默错位（pos[3i+k] 不再对齐 t_i），fail-fast。
        let mut pos = Vec::with_capacity(samples_n * 3);
        let mut hull_yaw = Vec::with_capacity(samples_n);
        let mut hull_pitch = Vec::with_capacity(samples_n);
        // 侧倾列取自（已重排的）type=10 序列（滤波层不输出侧倾，见 hull_roll 注释）
        let mut hull_roll = Vec::with_capacity(samples_n);
        let mut turret_yaw = Vec::with_capacity(samples_n);
        let mut gun_pitch = Vec::with_capacity(samples_n);
        let mut prev_hull_yaw = None;
        let mut prev_turret_yaw = None;
        for i in 0..samples_n {
            let t = meta.t_start + i as f32 * GRID_DT;
            let Some(pose) = tl.pose_at(t as f64, false) else {
                bail!("车辆 {eid} 位姿网格 t={t} 无帧（段化时间线恒可求值，不应发生）");
            };
            let yaw = unwrap_angle(prev_hull_yaw, pose.ang[0]);
            prev_hull_yaw = Some(yaw);
            pos.push(r2(pose.pos[0]));
            pos.push(r2(pose.pos[1]));
            pos.push(r2(pose.pos[2]));
            hull_yaw.push(r3(yaw));
            hull_pitch.push(r3(pose.ang[1]));
            hull_roll.push(r3(roll_nearest(st_series, t).unwrap_or(0.0)));
            // 非空 prop2 序列恒可求值（AoI 前回退初值包 / 末帧保持）
            match combat::prop2_at(prop2_series, t) {
                Some((rel, frac)) => {
                    let turret_abs = unwrap_angle(prev_turret_yaw, wrap_pi(rel + pose.ang[0]));
                    prev_turret_yaw = Some(turret_abs);
                    turret_yaw.push(r3(turret_abs));
                    gun_pitch.push(match limits {
                        Some(lim) => r3(combat::decode_prop2_gun_pitch(frac, lim, rel)),
                        // 无俯仰极限锚定（匿名车/空锚定表）：车体 pitch 兜底（type10 pitch 正向
                        // 与"正=仰角"相反，仅近似——锚定表由 battle_results 全量构建，正常场次不触发）
                        None => r3(pose.ang[1]),
                    });
                }
                None => {
                    // 本车全场无 prop2（该车全程未转炮塔）：炮塔随车体、炮管水平
                    let turret_abs = unwrap_angle(prev_turret_yaw, pose.ang[0]);
                    prev_turret_yaw = Some(turret_abs);
                    turret_yaw.push(r3(turret_abs));
                    gun_pitch.push(0.0);
                }
            }
        }

        // HP 链：模型已按同值去重语义构建（含满血锚点，此处仅做落盘舍入）；
        // 击杀者/死因 = 死亡终态记录（hp==0 事件 + prop1 时刻合并）
        let hp: Vec<(f32, u16)> = model
            .timeline
            .hp_series
            .get(eid)
            .map(|s| s.iter().map(|(t, h)| (r2(*t), *h)).collect())
            .unwrap_or_default();
        let death = model.timeline.deaths.get(eid);
        let death_records = model.timeline.death_records.get(eid).map(|v| v.as_slice());
        let killer_eid = death.map(|d| d.killer_eid).unwrap_or(0);

        // 发射弹种全局 id（配置推断证据；去重上限 16）
        let mut shell_ids: Vec<u32> = Vec::new();
        for s in shots_raw.iter() {
            if s.shooter_eid != *eid || s.shell_id == 0 || shell_ids.len() >= 16 {
                continue;
            }
            if !shell_ids.contains(&s.shell_id) {
                shell_ids.push(s.shell_id);
            }
        }

        vehicles_out.push(VehicleTrack {
            eid: *eid,
            account_id: rec.and_then(|r| r.account_id).unwrap_or(0),
            nickname,
            tank_id: rec.and_then(|r| r.tank_id).unwrap_or(0),
            tank_name: rec
                .and_then(|r| r.tank_id)
                .and_then(|tid| render.tank_names.get(&tid).cloned())
                .unwrap_or_default(),
            team: rec.and_then(|r| r.team).unwrap_or(0),
            is_author,
            max_hp: initial_hp.get(eid).map(|(_, h)| *h).unwrap_or(0),
            pos,
            hull_yaw,
            hull_pitch,
            hull_roll,
            turret_yaw,
            gun_pitch,
            hp,
            death_t: death.map(|d| r2(d.t)),
            killer_eid,
            death_times: death_records
                .and_then(|rs| {
                    (rs.len() >= 2).then(|| rs.iter().map(|d| r2(d.t)).collect::<Vec<_>>())
                })
                .unwrap_or_default(),
            resurrection_times: model
                .timeline
                .resurrections
                .get(eid)
                .map(|ts| ts.iter().map(|t| r2(*t)).collect())
                .unwrap_or_default(),
            shell_ids,
            loadout_items: rec
                .and_then(|r| r.loadout.as_ref())
                .map(|l| l.items.clone())
                .unwrap_or_default(),
            equipment: rec.and_then(|r| r.equipment),
            loadout: rec.and_then(|r| r.loadout.clone()),
            turret_index: None,
            gun_index: None,
            config_idx: None,
            burst_size: None,
            // comp blob 纯回放证据（无坦克数据依赖），随模型实体并表进入所有投影路径
            //（WASM/在线/切面导出同源；config_idx 等坦克数据增值仍由注入方标注）
            turret_local: rec.and_then(|r| r.turret_local),
            gun_local: rec.and_then(|r| r.gun_local),
            coverage: build_coverage(&clocks),
            pose_kf: tl_display.as_ref().map(pose_keyframes),
        });
    }

    // 击杀事件：模型统一产出（击杀播报归属增强，|t − death_t| ≤ 5s 门控已在模型内），
    // 回放切面按候选车集过滤——与原逐车内联组装逐位等价
    let kills: Vec<KillEvent> = model
        .kill_events()
        .into_iter()
        .filter(|k| candidates.contains_key(&k.victim_eid))
        .collect();

    let mut shots: Vec<PlaybackShot> = shots_raw
        .iter()
        .map(to_playback_shot)
        .collect();
    shots.sort_by(|a, b| a.t_fire.partial_cmp(&b.t_fire).unwrap());

    let periods_out: Vec<PeriodPoint> = periods
        .iter()
        .map(|p| PeriodPoint {
            clock: r2(p.clock),
            period: p.period,
            remaining_s: p.remaining_s,
            duration_s: p.duration_s,
        })
        .collect();

    // AoI 可见窗口（仅收录车辆实体；Type33/5 物化开段、Type4 关段，重入 = 多段）
    let visibility: Vec<AoiPresence> = model
        .timeline
        .presence
        .iter()
        .filter(|p| candidates.contains_key(&p.eid))
        .cloned()
        .collect();

    let detached_turrets =
        detached_turret_tracks(model, &vehicles_out.iter().map(|v| v.eid).collect());

    Ok(PlaybackData {
        detached_turrets,
        version: 2, // contract v2：+supremacy_bases/supremacy_points（消费端版本门禁）
        meta,
        armor_states: Vec::new(), // 原始包证据：由 build_playback_data 持 packets 回填
        vehicles: vehicles_out,
        shots,
        shots_from_loose_path: model.timeline.shots_from_loose_path,
        kills,
        periods: periods_out,
        visibility,
        supremacy_bases: model
            .timeline
            .supremacy_bases
            .iter()
            .map(|t| SupremacyBaseStateTransition {
                clock: r2(t.clock),
                ..t.clone()
            })
            .collect(),
        supremacy_points: model
            .timeline
            .supremacy_points
            .iter()
            .map(|p| SupremacyPointsSample {
                clock: r2(p.clock),
                ..p.clone()
            })
            .collect(),
        assault_objective_present: model.timeline.assault_objective_present,
        assault_bases: model
            .timeline
            .assault_bases
            .iter()
            .map(|t| AssaultBaseStateTransition {
                clock: r2(t.clock),
                ..t.clone()
            })
            .collect(),
        consumables: model
            .timeline
            .consumables
            .iter()
            .map(|c| ConsumableTransition {
                clock: r2(c.clock),
                ..c.clone()
            })
            .collect(),
        author_shell_timeline: author_shell_timeline(&model.timeline),
        reload_effective: model
            .timeline
            .reload_effective
            .iter()
            .map(|d| RawReloadDuration {
                clock: r2(d.clock),
                ..d.clone()
            })
            .collect(),
        reloads: model
            .timeline
            .reloads
            .iter()
            .map(|r| RawReloadPhase {
                clock: r2(r.clock),
                ..r.clone()
            })
            .collect(),
        module_crew_states: model
            .timeline
            .module_crew_states
            .iter()
            .map(|m| ModuleCrewStateEvent {
                clock: r2(m.clock),
                ..*m
            })
            .collect(),
        destructible_areas: model.timeline.destructible_areas.clone(),
        destructible_events: model
            .timeline
            .destructible_events
            .iter()
            .map(|e| crate::replay::destructibles::AreaDestructibleEvent {
                clock: r2(e.clock),
                ..e.clone()
            })
            .collect(),
    })
}

/// [`collect_all_shots`] 的产物：合并弹道 + 提取路径 provenance。
pub(crate) struct CollectedShots {
    pub shots: Vec<combat::ShotReplayData>,
    /// true = 作者严格路径失败，本场 shots 全部来自宽松路径（含作者的 `is_author`
    /// 按 shooter_eid 回填）。宽松路径没有 method38 增强字段，质量降级——消费端
    /// 应透出该标记（PlaybackData.shots_from_loose_path），不得当作严格路径产物展示。
    pub from_loose_path: bool,
}

/// 合并作者严格路径 + 他人宽松路径（共享扫描形态：预分析产物与渲染缓存两路复用，
/// 见 [`combat::ShotScanShared`]）。作者严格路径是 fail-fast 设计（边界数据缺失即
/// bail）——全场回放不应因此整场不可用：失败时降级为宽松路径提取**全部**发射
/// （含作者，按 shooter_eid 补回 is_author 标记），并以 `from_loose_path` 透出降级事实。
pub(crate) fn collect_all_shots(
    shared: &combat::ShotScanShared,
    author_player_eid: u32,
    pitch_limits: &GunPitchLimits,
) -> anyhow::Result<CollectedShots> {
    // 滤波时间线缓存跨两路共享（按实体确定；段化 = D1，重入后炮口锚点从物化位开始）
    let mut render_cache: HashMap<u32, filter::SegmentedPoseTimeline> = HashMap::new();
    match combat::extract_shot_replays_from_shared(
        shared,
        author_player_eid,
        pitch_limits,
        &mut render_cache,
    ) {
        Ok(mut all) => {
            let others = combat::extract_other_shot_replays_from_shared(
                shared,
                author_player_eid,
                pitch_limits,
                &mut render_cache,
            )?;
            all.extend(others.shots);
            all.sort_by(|a, b| a.fire_time.partial_cmp(&b.fire_time).unwrap());
            Ok(CollectedShots {
                shots: all,
                from_loose_path: false,
            })
        }
        Err(strict_err) => {
            eprintln!("[playback] 作者严格路径提取失败（{strict_err:#}），降级宽松全路径");
            // 宽松路径自身 Err = 内部不变量破坏（代码缺陷），不属本次降级覆盖的数据边界，
            // fail-visible 上抛（collect_all_shots 的降级契约只针对作者路径的数据缺失）
            let mut all = combat::extract_other_shot_replays_from_shared(
                shared,
                0,
                pitch_limits,
                &mut render_cache,
            )?
            .shots;
            for s in &mut all {
                if author_player_eid != 0 && s.shooter_eid == author_player_eid {
                    s.is_author = true;
                }
            }
            all.sort_by(|a, b| a.fire_time.partial_cmp(&b.fire_time).unwrap());
            Ok(CollectedShots {
                shots: all,
                from_loose_path: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHOR: u32 = 7;

    fn shell_timeline(
        author_eid: u32,
        death: Option<f32>,
        broadcasts: &[(f32, u32)],
    ) -> Vec<AuthorShellEvent> {
        let mut timeline = crate::replay::model::Timeline {
            author_eid,
            shell_broadcasts: broadcasts.to_vec(),
            ..Default::default()
        };
        if let Some(t) = death {
            timeline.deaths.insert(
                AUTHOR,
                crate::replay::model::DeathRecord {
                    t,
                    killer_eid: 0,
                    cause: 0,
                },
            );
        }
        author_shell_timeline(&timeline)
    }

    fn shell(t: f32, shell_id: u32) -> AuthorShellEvent {
        AuthorShellEvent { t, shell_id }
    }

    #[test]
    fn author_shell_timeline_folds_pairs_and_masks_to_24_bits() {
        let got = shell_timeline(
            AUTHOR,
            None,
            &[
                (5.0, 32362),
                (5.0, 32362), // a0=1 成对
                (29.37, 32618),
                (29.37, 32618),
                (29.5, 0x7F00_0000 | 32618), // 重复确认 + byte3 噪声都不是变更
                (30.0, 32362),
            ],
        );
        assert_eq!(
            got,
            vec![shell(5.0, 32362), shell(29.37, 32618), shell(30.0, 32362)]
        );
    }

    /// 19/46 场作者死后 0x07 跟随被观战车辆：首次死亡起的广播不属于作者。
    #[test]
    fn author_shell_timeline_stops_at_first_death() {
        let got = shell_timeline(
            AUTHOR,
            Some(10.0),
            &[(5.0, 1), (9.0, 2), (10.0, 3), (12.0, 4)],
        );
        assert_eq!(got, vec![shell(5.0, 1), shell(9.0, 2)]);
    }

    /// 截断用未取整时钟：取整后 10.003 与 10.004 同为 10.0，比较必须在取整之前。
    #[test]
    fn author_shell_timeline_compares_raw_clock_at_death_boundary() {
        let got = shell_timeline(AUTHOR, Some(10.004), &[(10.003, 1), (10.004, 2)]);
        assert_eq!(got, vec![shell(10.0, 1)]);
    }

    #[test]
    fn author_shell_timeline_keeps_all_when_author_survives() {
        let got = shell_timeline(AUTHOR, None, &[(5.0, 1), (500.0, 2)]);
        assert_eq!(got, vec![shell(5.0, 1), shell(500.0, 2)]);
    }

    /// 作者未识别时拿不到死亡时刻，归属无法证明。
    #[test]
    fn author_shell_timeline_is_empty_without_author() {
        assert!(shell_timeline(0, None, &[(5.0, 1)]).is_empty());
    }

    /// 作者车接收突发时间轴重建（真实回放回归，2026-10-11）。
    ///
    /// 夹具 = 用户报障回放（T100LT 训练房）：录制端在 clock≈27.58 把 4 个 type=10 包压成
    /// 同一时间戳（0.4s 接收突发）。修复前作者车（自车；客户端里是本地物理、不经这段流
    /// 渲染）的渲染位姿被滤波器如实重放为"保持 0.4s → 一帧跳 ~5.6 m"（用户："0:15 卡在
    /// 原地、随后突然前滑一小段，客户端匀速"）。本测试锁两件事：
    /// ① 原始流确实存在同 clock 连包（证据/夹具有效性，防夹具替换后测试静默失效）；
    /// ② 修复后作者车 0.1s 渲染网格全程无超速步进（≤2.2 m/步 = 22 m/s，含加速余量；
    ///    修复前该段 5.6 m/步 ≈ 56 m/s）。
    #[test]
    fn author_burst_respacing_smooths_real_replay() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest.join(
            "../../data/replay_samples/20261011_1924__Anonyme_R132_T100LT_5451148513997276.wotbreplay",
        );
        if !path.exists() {
            eprintln!("(跳过：夹具缺失 {})", path.display());
            return;
        }
        let f = std::fs::File::open(&path).unwrap();
        let mut replay = wotbreplay_parser::replay::Replay::open(f).unwrap();
        let data = replay.read_data().unwrap();
        let packets = crate::replay::packets::test_data_tuples(&data);
        let br = replay.read_battle_results().ok();
        let players: Vec<PlaybackPlayer> = br
            .as_ref()
            .map(|br| {
                br.player_results
                    .iter()
                    .map(|pr| PlaybackPlayer {
                        account_id: pr.info.account_id,
                        nickname: String::new(),
                        team: 0,
                        tank_id: pr.info.tank_id,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let input = PlaybackInput {
            packets: &packets,
            players,
            author_account_id: br.as_ref().map(|b| b.author.account_id).unwrap_or(0),
            winner_team: 0,
            map_id: 0,
            map_name: String::new(),
            pitch_limits: &GunPitchLimits::new(),
            tank_names: HashMap::new(),
        };
        let pb = build_playback_data(&input).expect("夹具回放应可构建");
        let author = pb
            .vehicles
            .iter()
            .find(|v| v.is_author)
            .expect("夹具应含作者车");

        // ① 证据锁：作者车原始 type=10 流存在 ≥2 的同 clock 连包
        let eid = author.eid;
        let mut clocks: Vec<f32> = packets
            .iter()
            .filter(|(t, _, p)| {
                *t == 10 && p.len() >= 48 && u32::from_le_bytes([p[0], p[1], p[2], p[3]]) == eid
            })
            .map(|(_, c, _)| *c)
            .collect();
        clocks.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let max_run = clocks
            .chunk_by(|a, b| a == b)
            .map(|c| c.len())
            .max()
            .unwrap_or(0);
        assert!(
            max_run >= 4,
            "夹具原始流应含 4 连包突发（实测 clock≈27.58），实际最大连包 {max_run}"
        );

        // ② 渲染网格全程无超速步进
        let pos = &author.pos;
        let mut max_step = 0.0f32;
        let mut max_at = 0.0f32;
        for i in 1..(pos.len() / 3) {
            let d = ((pos[i * 3] - pos[(i - 1) * 3]).powi(2)
                + (pos[i * 3 + 1] - pos[(i - 1) * 3 + 1]).powi(2)
                + (pos[i * 3 + 2] - pos[(i - 1) * 3 + 2]).powi(2))
            .sqrt();
            if d > max_step {
                max_step = d;
                max_at = pb.meta.t_start + i as f32 * GRID_DT;
            }
        }
        assert!(
            max_step <= 2.2,
            "作者车渲染网格出现超速步进 {max_step:.2} m/0.1s（t≈{max_at:.1}）——             接收突发时间轴重建失效（修复前该段实测 ≈5.6 m/步）"
        );
        eprintln!(
            "作者车 0.1s 网格最大步进 {max_step:.3} m（t≈{max_at:.1}），原始最大连包 {max_run}"
        );
    }

    /// 全车辆 type=10 + prop2 接收突发重建（真实回放回归，2026-10-31 用户报障；
    /// 下载原始文件名 `20261002_2147___神了_S16_Kranvagn_15612263746483708.wotbreplay`，
    /// 夹具入库时改 ASCII 文件名）。
    ///
    /// 夹具 = 90.254→94.058s 全场网络停摆 3.80s（期间仅 type=39 心跳按 ~30 包/0.25s
    /// 继续到达，连接未断），补发把每车 25~38 个 type=10、21~36 个 prop2 压成
    /// clock=94.058（包头 clock = 录制端到达时间）。修复前（v0.4.4 只重建作者车
    /// type=10）：作者车炮塔在 t≈94.2 单格跳 57.07°（prop2 35 连包跨 −52.0°→+4.9°，
    /// prop2_at 同刻括号下中间帧不可达），远程车整场冻结后于 t≈94.7~94.8 猛冲
    /// （pose_kf 100~414 m/s）。本测试锁三件事：
    /// ① 原始流确实存在同 clock 连包（证据/夹具有效性，防夹具替换后测试静默失效）；
    /// ② 作者车 0.1s 网格全程位姿无超速步进（≤2.2 m，沿 T100LT 阈值）且炮塔角无
    ///    超速步进（≤0.12 rad/0.1s ≈ 69°/s；正常炮塔转速 ~36°/s）；
    /// ③ 突发窗 [91, 95.5]（含修复前猛冲段 94.7~94.8）内所有在流车辆（coverage 覆盖
    ///    窗两端）位姿/炮塔均无超速步进——位姿超速步 = 猛冲回归，炮塔超速步 = prop2
    ///    重排回归。窗内位姿为 0 的保持段是护栏放弃/回填后残余空洞的诚实表现，不算回归。
    ///    炮塔上限分两档：作者车全程 0.12 rad（≈69°/s，正常转速 ~36°/s，修复前该窗
    ///    57.07°）；全车队窗内 0.30 rad——补发包数少于停摆窗槽位时（prop2 变化驱动）
    ///    残余空洞在 prop2_at 括号语义下渲染为"保持→一步追上空洞期扫过角"（实测最大
    ///    晓孩哥 11.12°/0.1s，空洞 1.8s 扫 9.5°），这是缺失数据的诚实表现而非回归；
    ///    0.30 rad 仍远低于修复前全车队最小超速步（雪OwO 30.19°）。
    #[test]
    fn burst_respacing_covers_all_vehicles_and_prop2() {
        const POS_STEP_CAP: f32 = 2.2; // m/0.1s（22 m/s，含加速余量）
        const TURRET_STEP_CAP_AUTHOR: f32 = 0.12; // rad/0.1s（≈69°/s）
        const TURRET_STEP_CAP_FLEET: f32 = 0.30; // rad/0.1s（≈172°/s，见上方残余空洞说明）
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest.join(
            "../../data/replay_samples/20261002_2147__shenle_S16_Kranvagn_15612263746483708.wotbreplay",
        );
        if !path.exists() {
            eprintln!("(跳过：夹具缺失 {})", path.display());
            return;
        }
        let f = std::fs::File::open(&path).unwrap();
        let mut replay = wotbreplay_parser::replay::Replay::open(f).unwrap();
        let data = replay.read_data().unwrap();
        let packets = crate::replay::packets::test_data_tuples(&data);
        let br = replay.read_battle_results().ok();
        let author_account = br.as_ref().map(|b| b.author.account_id).unwrap_or(0);
        let players: Vec<PlaybackPlayer> = br
            .as_ref()
            .map(|br| {
                br.player_results
                    .iter()
                    .map(|pr| PlaybackPlayer {
                        account_id: pr.info.account_id,
                        nickname: String::new(),
                        team: 0,
                        tank_id: pr.info.tank_id,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // ① 证据锁：作者车 type=10 与 prop2 原始流都有 ≥20 的同 clock 连包
        let model = crate::replay::model::ReplayModel::scan(&crate::replay::model::ScanInput {
            packets: &packets,
            roster: &players,
            author_account_id: author_account,
            pitch_limits: &GunPitchLimits::new(),
        })
        .expect("夹具应可扫描");
        let author_eid = model
            .entities
            .iter()
            .find(|e| e.is_author)
            .expect("夹具应含作者")
            .eid;
        let max_run = |mut clocks: Vec<f32>| -> usize {
            clocks.sort_by(|a, b| a.partial_cmp(b).unwrap());
            clocks.chunk_by(|a, b| a == b).map(|c| c.len()).max().unwrap_or(0)
        };
        let st10_run = max_run(model.timeline.poses[&author_eid].iter().map(|s| s.clock).collect());
        let prop2_run = max_run(model.timeline.turret[&author_eid].iter().map(|s| s.0).collect());
        assert!(st10_run >= 20, "夹具 type=10 应含 ≥20 连包突发，实测 {st10_run}");
        assert!(prop2_run >= 20, "夹具 prop2 应含 ≥20 连包突发，实测 {prop2_run}");

        let input = PlaybackInput {
            packets: &packets,
            players,
            author_account_id: author_account,
            winner_team: 0,
            map_id: 0,
            map_name: String::new(),
            pitch_limits: &GunPitchLimits::new(),
            tank_names: HashMap::new(),
        };
        let pb = build_playback_data(&input).expect("夹具回放应可构建");

        let max_pos_step = |v: &VehicleTrack, t0: f32, t1: f32| -> (f32, f32) {
            let mut max = 0.0f32;
            let mut max_at = 0.0f32;
            let n = v.pos.len() / 3;
            for i in 1..n {
                let t = pb.meta.t_start + i as f32 * GRID_DT;
                if t < t0 || t > t1 {
                    continue;
                }
                let d = ((v.pos[i * 3] - v.pos[(i - 1) * 3]).powi(2)
                    + (v.pos[i * 3 + 1] - v.pos[(i - 1) * 3 + 1]).powi(2)
                    + (v.pos[i * 3 + 2] - v.pos[(i - 1) * 3 + 2]).powi(2))
                    .sqrt();
                if d > max {
                    max = d;
                    max_at = t;
                }
            }
            (max, max_at)
        };
        let max_turret_step = |v: &VehicleTrack, t0: f32, t1: f32| -> (f32, f32) {
            let mut max = 0.0f32;
            let mut max_at = 0.0f32;
            for i in 1..v.turret_yaw.len() {
                let t = pb.meta.t_start + i as f32 * GRID_DT;
                if t < t0 || t > t1 {
                    continue;
                }
                let d = (v.turret_yaw[i] - v.turret_yaw[i - 1]).abs();
                if d > max {
                    max = d;
                    max_at = t;
                }
            }
            (max, max_at)
        };

        // ② 作者车全程网格无超速步进
        let author = pb.vehicles.iter().find(|v| v.is_author).expect("夹具应含作者车");
        let (pos_max, pos_at) = max_pos_step(author, 0.0, f32::MAX);
        assert!(
            pos_max <= POS_STEP_CAP,
            "作者车位姿网格超速步进 {pos_max:.2} m/0.1s（t≈{pos_at:.1}）——type=10 突发重建失效"
        );
        let (tur_max, tur_at) = max_turret_step(author, 0.0, f32::MAX);
        assert!(
            tur_max <= TURRET_STEP_CAP_AUTHOR,
            "作者车炮塔角网格超速步进 {:.2}°/0.1s（t≈{tur_at:.1}）——prop2 突发重建失效（修复前实测 57.07°）",
            tur_max.to_degrees()
        );

        // ③ 突发窗内所有在流车辆无超速步进
        let mut checked = 0usize;
        for v in &pb.vehicles {
            if !coverage_contains(&v.coverage, 91.0) || !coverage_contains(&v.coverage, 95.5) {
                continue; // 窗口不在其有效数据段（未进 AoI/已阵亡/诚实空洞）→ 不在本断言范围
            }
            checked += 1;
            let (pos_max, pos_at) = max_pos_step(v, 91.0, 95.5);
            assert!(
                pos_max <= POS_STEP_CAP,
                "{} 位姿窗内超速步进 {pos_max:.2} m/0.1s（t≈{pos_at:.1}）——全车辆 type=10 突发重建失效",
                v.nickname
            );
            let (tur_max, tur_at) = max_turret_step(v, 91.0, 95.5);
            assert!(
                tur_max <= TURRET_STEP_CAP_FLEET,
                "{} 炮塔角窗内超速步进 {:.2}°/0.1s（t≈{tur_at:.1}）——prop2 突发重建失效",
                v.nickname,
                tur_max.to_degrees()
            );
        }
        assert!(
            checked >= 8,
            "突发窗内在流车辆应 ≥8 辆（实测 {checked}），夹具形态可能已变"
        );
        eprintln!(
            "突发窗内在流车辆 {checked} 辆全部过限；作者车全程位姿最大步进 {pos_max:.3} m、炮塔最大 {:.2}°/0.1s",
            tur_max.to_degrees()
        );
    }

    #[test]
    fn vehicle_candidates_follow_type5_class_not_live_streams() {
        // 2026-10-19 用户报障样本：训练房对手 SU-130PM 全程未转炮塔 ⇒ 整场无 prop2。
        // 车辆身份 = Type5 class=2，与炮塔流/采样密度无关。
        let stationary = 0x0016_661au32; // 9 分钟原地不动，无 prop2
        let normal = 0x0016_661bu32;
        let sparse = 0x0016_661cu32; // 只有 1 条位姿（短暂可见）
        let observer = 0x0016_661du32; // class=2 但不在结算花名册（观察者/中途离开）
        let camera = 0x0019_4c93u32; // 无 Type5 class 的 type=10 流（回放相机实体）
        let wreck = 0x0017_5502u32; // Type5 class=8 炮塔残骸
        let mut st10: HashMap<u32, Vec<crate::replay::combat::St10Sample>> = HashMap::new();
        st10.insert(stationary, synthetic_samples(0.0, 0.1, 700));
        st10.insert(normal, synthetic_samples(0.0, 0.1, 95));
        st10.insert(sparse, synthetic_samples(0.0, 0.1, 1));
        st10.insert(observer, synthetic_samples(0.0, 0.1, 400));
        st10.insert(camera, synthetic_samples(0.0, 0.1, 1000));
        st10.insert(wreck, synthetic_samples(0.0, 0.1, 300));
        let vehicles = HashSet::from([stationary, normal, sparse, observer]);
        let roster = HashSet::from([stationary, normal, sparse]);
        let got = vehicle_candidates(&st10, &vehicles, &roster);
        assert_eq!(
            got.keys().copied().collect::<Vec<_>>(),
            vec![stationary, normal, sparse],
            "class=2 ∧ 花名册车辆全收（含无 prop2 与单采样）；未联表/非车辆流全排除"
        );
    }

    /// ARENA_INFO comp blob：`[tank_id u16][chassis u16][engine u16][204 u32][turret u16][gun u16][00]`。
    fn comp_blob(tank: u16, turret: u16, gun: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&tank.to_le_bytes());
        b.extend_from_slice(&7u16.to_le_bytes());
        b.extend_from_slice(&9u16.to_le_bytes());
        b.extend_from_slice(&204u32.to_le_bytes());
        b.extend_from_slice(&turret.to_le_bytes());
        b.extend_from_slice(&gun.to_le_bytes());
        b.push(0);
        assert_eq!(b.len(), 15);
        b
    }

    #[test]
    fn comp_descriptor_parses_all_blobs_in_one_arena_info() {
        // P1 探针实证：单条 subtype=1 update 携带全场玩家的 blob（9/9 场，blob 数=花名册数）
        let mut args = vec![0x12, 0x0f];
        args.extend(comp_blob(4481, 3, 5));
        args.push(0x1a);
        args.push(4);
        args.extend(b"Nick");
        args.extend([0x12, 0x0f]);
        args.extend(comp_blob(6225, 52, 29));
        args.push(0x1a);
        args.push(8);
        args.extend(b"yuki0411");
        let d = CompDescriptor::parse_args(&args, &[4481, 6225]);
        assert_eq!(d.len(), 2, "两个 blob 都必须提取（旧实现取首个即 return）");
        assert_eq!(d[0].nickname, "Nick");
        assert_eq!((d[0].turret_local, d[0].gun_local), (3, 5));
        assert_eq!(d[1].nickname, "yuki0411");
        assert_eq!(
            (d[1].tank_id, d[1].turret_local, d[1].gun_local),
            (6225, 52, 29)
        );
    }

    #[test]
    fn comp_descriptor_rejects_unknown_tank_and_zero_locals() {
        let mut args = vec![0x12, 0x0f];
        args.extend(comp_blob(999, 1, 1)); // tank 白名单外
        args.extend([0x12, 0x0f]);
        args.extend(comp_blob(4481, 0, 5)); // turret_local=0
        assert!(
            CompDescriptor::parse_args(&args, &[4481]).is_empty(),
            "白名单外与零局部 id 一律拒收"
        );
    }

    /// P3 探针定案的条目结构：`[0a LEN][08 eid][12 0f blob][1a 昵称][20/28 标志]
    /// [38 account][42 公会]…`——eid/account 是身份联表精确键（重名/匿名免疫）。
    #[test]
    fn comp_descriptor_extracts_eid_and_account_from_entry() {
        let entry_inner = |eid: u32, tank: u16, turret: u16, gun: u16, nick: &[u8], account: u64| {
            let mut e = Vec::new();
            e.push(0x08);
            // eid varint
            let mut v = eid;
            loop {
                let b = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 {
                    e.push(b);
                    break;
                }
                e.push(b | 0x80);
            }
            e.push(0x12);
            e.push(0x0f);
            e.extend(comp_blob(tank, turret, gun));
            e.push(0x1a);
            e.push(nick.len() as u8);
            e.extend(nick);
            e.extend([0x20, 0x01, 0x28, 0x01]); // f4/f5 标志
            e.push(0x38);
            let mut a = account;
            loop {
                let b = (a & 0x7f) as u8;
                a >>= 7;
                if a == 0 {
                    e.push(b);
                    break;
                }
                e.push(b | 0x80);
            }
            e.extend([0x42, 0x04, b'G', b'A', b'R', b'N']); // f8 公会标签
            e
        };
        let e1 = entry_inner(1892656, 9489, 3, 5, b"Anonyme", 2033684170);
        let e2 = entry_inner(269839187, 6225, 7, 9, b"yuki0411", 2041831618);
        let mut args = Vec::new();
        for e in [&e1, &e2] {
            args.push(0x0a);
            args.push(e.len() as u8);
            args.extend(e);
        }
        let d = CompDescriptor::parse_args(&args, &[9489, 6225]);
        assert_eq!(d.len(), 2, "两个条目都须提取");
        assert_eq!(d[0].nickname, "Anonyme");
        assert_eq!(d[0].eid, 1892656);
        assert_eq!(d[0].account_id, 2033684170);
        assert_eq!(d[1].nickname, "yuki0411");
        assert_eq!(d[1].eid, 269839187);
        assert_eq!(d[1].account_id, 2041831618);
    }

    /// 裸 blob（无条目包装的历史/变体形态）：昵称照提，eid/account 优雅置 0
    ///（消费端 0 = 退回昵称联表）。
    #[test]
    fn comp_descriptor_bare_blob_degrades_identity_to_zero() {
        let mut args = vec![0x12, 0x0f];
        args.extend(comp_blob(4481, 3, 5));
        args.push(0x1a);
        args.push(4);
        args.extend(b"Nick");
        let d = CompDescriptor::parse_args(&args, &[4481]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].nickname, "Nick");
        assert_eq!(d[0].eid, 0);
        assert_eq!(d[0].account_id, 0);
    }

    fn cov(pairs: &[&[f32]]) -> Vec<f32> {
        pairs.iter().flat_map(|p| p.iter().copied()).collect()
    }

    /// coverage 合并：小间隙不断开、>2s 空洞断开、末区间闭合到最后采样
    #[test]
    fn coverage_merges_small_gaps_and_splits_large() {
        // 10Hz 连续 3s → 单段 [10.0, 12.9]
        let c1: Vec<f32> = (0..30).map(|i| 10.0 + i as f32 * 0.1).collect();
        assert_eq!(build_coverage(&c1), cov(&[&[10.0, 12.9]]));
        // 中途 2.6s 空洞 → 两段
        let mut c3 = c1.clone();
        c3.extend([15.5, 15.6, 15.7]);
        let got = build_coverage(&c3);
        assert_eq!(got, cov(&[&[10.0, 12.9], &[15.5, 15.7]]));
        assert!(coverage_contains(&got, 10.0));
        assert!(coverage_contains(&got, 15.6));
        assert!(!coverage_contains(&got, 14.0));
    }

    /// 数值精度：可精确表示的值舍入稳定
    #[test]
    fn rounding() {
        assert_eq!(r2(123.5), 123.5);
        assert_eq!(r2(0.125), 0.13);
        assert_eq!(r3(1.0), 1.0);
        assert_eq!(r3(2.125), 2.125);
    }

    /// 复活轨道字段的 JSON 契约：单死/无复活车不出现 `death_times` /
    /// `resurrection_times`（普通回放零扰动）；≥2 次死亡才出现死亡列表，
    /// 复活列表非空才出现。缺省省略 = additive 契约（同 armor_states 先例）。
    #[test]
    fn respawn_track_fields_are_omitted_when_trivial() {
        let base = |death_t: Option<f32>, deaths: Vec<f32>, res: Vec<f32>| VehicleTrack {
            eid: 1,
            account_id: 0,
            nickname: String::new(),
            tank_id: 0,
            tank_name: String::new(),
            team: 1,
            is_author: false,
            max_hp: 1000,
            pos: Vec::new(),
            hull_yaw: Vec::new(),
            hull_pitch: Vec::new(),
            hull_roll: Vec::new(),
            turret_yaw: Vec::new(),
            gun_pitch: Vec::new(),
            hp: Vec::new(),
            death_t,
            killer_eid: 0,
            death_times: deaths,
            resurrection_times: res,
            shell_ids: Vec::new(),
            loadout_items: Vec::new(),
            equipment: None,
            loadout: None,
            turret_index: None,
            gun_index: None,
            config_idx: None,
            burst_size: None,
            turret_local: None,
            gun_local: None,
            coverage: Vec::new(),
            pose_kf: None,
        };
        let json = |v: &VehicleTrack| serde_json::to_value(v).unwrap();

        let single = json(&base(Some(20.0), vec![20.0], vec![]));
        assert!(single.get("death_times").is_none(), "单死 = death_t 已表达，字段省略");
        assert!(single.get("resurrection_times").is_none(), "无复活 → 字段省略");

        let none = json(&base(None, vec![], vec![]));
        assert!(none.get("death_times").is_none() && none.get("resurrection_times").is_none());

        let respawned = json(&base(Some(20.0), vec![20.0, 58.5], vec![35.0]));
        assert_eq!(
            respawned.get("death_times").unwrap().as_array().unwrap().len(),
            2,
            "≥2 次死亡才输出完整死亡列表"
        );
        assert_eq!(respawned.get("resurrection_times").unwrap().as_array().unwrap().len(), 1);
    }

    /// 飞行时长：300m / 150mps = 2.0s；速度缺失 0.5s 兜底
    #[test]
    fn flight_time() {
        let a = [0.0, 10.0, 0.0];
        let b = [300.0, 10.0, 0.0];
        assert_eq!(flight_secs(&a, &b, &[150.0, 0.0, 0.0]), 2.0);
        assert_eq!(flight_secs(&a, &b, &[0.0, 0.0, 0.0]), 0.5);
    }

    // ---------- 位姿关键帧折线 ----------

    /// 消费端语义求值：端点保持 + 段内线性（与前端 trackInterp 的 sampleKeyframes 同式）
    fn sample_kf(kf: &PoseKeyframes, t: f32) -> ([f32; 3], f32, f32) {
        let last = kf.t.len() - 1;
        if t <= kf.t[0] {
            return (kf.pos[0..3].try_into().unwrap(), kf.yaw[0], kf.pitch[0]);
        }
        if t >= kf.t[last] {
            let o = last * 3;
            return (
                kf.pos[o..o + 3].try_into().unwrap(),
                kf.yaw[last],
                kf.pitch[last],
            );
        }
        let i = kf.t.partition_point(|&x| x <= t) - 1;
        let f = (t - kf.t[i]) / (kf.t[i + 1] - kf.t[i]);
        let pos: [f32; 3] = [0, 1, 2]
            .map(|c| kf.pos[i * 3 + c] + (kf.pos[(i + 1) * 3 + c] - kf.pos[i * 3 + c]) * f);
        let dyaw = wrap_pi(kf.yaw[i + 1] - kf.yaw[i]);
        (
            pos,
            kf.yaw[i] + dyaw * f,
            kf.pitch[i] + (kf.pitch[i + 1] - kf.pitch[i]) * f,
        )
    }

    /// 合成输入：等间隔位置更新，每步 step_m 米（模拟客户端 10~12Hz 位置广播）
    fn synthetic_samples(step_m: f32, interval: f32, n: usize) -> Vec<combat::St10Sample> {
        (0..n)
            .map(|i| {
                let d = i as f32 * step_m;
                combat::St10Sample {
                    clock: 10.0 + i as f32 * interval,
                    pos: [d, 0.0, d * 0.5],
                    yaw: 0.3,
                    pitch: 0.0,
                    roll: 0.0,
                    pos_error: [0.0; 3],
                }
            })
            .collect()
    }

    /// 走廊判据几何：共线 → 通过；中间帧偏移超容差 → 拒绝；航向跨 ±π 用最短弧
    #[test]
    fn kf_within_geometry() {
        let mk = |t: f64, x: f32, yaw: f32| KfPos {
            t,
            pos: [x, 0.0, 0.0],
            yaw,
            pitch: 0.0,
        };
        let line = [mk(0.0, 0.0, 0.0), mk(1.0, 0.5, 0.0), mk(2.0, 1.0, 0.0)];
        assert!(kf_within(&line, 0, 2));
        // 边界两侧用容差本身构造（容差调整后仍成立）：0.5×容差 → 通过；2×容差 → 拒绝
        let soft = [
            mk(0.0, 0.0, 0.0),
            mk(1.0, 0.5 + KF_TOL_POS * 0.5, 0.0),
            mk(2.0, 1.0, 0.0),
        ];
        assert!(kf_within(&soft, 0, 2));
        let bent = [
            mk(0.0, 0.0, 0.0),
            mk(1.0, 0.5 + KF_TOL_POS * 2.0, 0.0),
            mk(2.0, 1.0, 0.0),
        ];
        assert!(!kf_within(&bent, 0, 2));
        // 航向最短弧跨 ±π（+3.10 → −3.10 实为 +0.08 的短弧）
        let wrap = [
            mk(0.0, 0.0, 3.10),
            mk(1.0, 0.5, std::f32::consts::PI),
            mk(2.0, 1.0, -3.10),
        ];
        assert!(kf_within(&wrap, 0, 2), "跨 ±π 的短弧不应被判为大偏差");
        let yawbent = [
            mk(0.0, 0.0, 0.0),
            mk(1.0, 0.5, KF_TOL_ANG * 2.0),
            mk(2.0, 1.0, 0.0),
        ];
        assert!(!kf_within(&yawbent, 0, 2));
    }

    /// 短隐藏段（0.1s 级）时前段滤波尾部的 +0.5s 外推不得越过下一段 t_in：
    /// 否则全局关键帧时刻非单调，消费端二分/插值会跨隐藏段滑移。
    #[test]
    fn keyframes_stay_monotonic_across_short_aoi_gap() {
        let samples = synthetic_samples(1.0, 0.1, 30); // 10.0 .. 12.9
        let seed = |pos: [f32; 3]| {
            Some(combat::MaterializationPose {
                pos,
                yaw: 0.0,
                pitch: 0.0,
            })
        };
        let pres = [
            combat::AoiPresence {
                eid: 1,
                t_in: 10.0,
                t_out: Some(10.65),
                hp_raw: None,
                pose: seed([0.0, 0.0, 0.0]),
            },
            combat::AoiPresence {
                eid: 1,
                t_in: 10.75,
                t_out: None,
                hp_raw: None,
                pose: seed([50.0, 0.0, 0.0]),
            },
        ];
        let tl = filter::SegmentedPoseTimeline::build(&samples, &pres).unwrap();
        let kf = pose_keyframes(&tl);
        assert!(kf.t.len() >= 2);
        for w in kf.t.windows(2) {
            assert!(w[0] <= w[1], "关键帧时刻非单调：{} > {}", w[0], w[1]);
        }
        // 前段最后输出 ≤ t_out；后段最早关键帧不得早于其时间线起点（隐藏段样本可预先汇入）
        assert!(kf.t.iter().all(|t| *t <= 10.65 || *t >= 10.7));
        assert!(kf.t.iter().any(|t| *t >= 10.75), "重入段首关键帧缺失");
    }

    /// 关键帧折线保真：任意时刻与 60Hz 渲染帧之差 ≤ 容差（对前端的契约）
    #[test]
    fn keyframes_reproduce_frames_within_tolerance() {
        let tl =
            filter::SegmentedPoseTimeline::build(&synthetic_samples(1.4, 0.083, 200), &[]).unwrap();
        let kf = pose_keyframes(&tl);
        assert!(kf.t.len() >= 2, "关键帧数过少：{}", kf.t.len());
        assert_eq!(kf.pos.len(), kf.t.len() * 3, "列式数组必须等长");
        assert_eq!(kf.yaw.len(), kf.t.len());
        assert_eq!(kf.pitch.len(), kf.t.len());
        let (start, end) = tl.time_range();
        let mut max_err = 0.0f32;
        for k in 0..tl.frame_count() {
            let t = start + (k as f64 + 0.5) * filter::FRAME_DT;
            let p = tl.pose_at(t, false).unwrap();
            let (kp, ky, kpitch) = sample_kf(&kf, p.time as f32);
            max_err = max_err.max(
                ((kp[0] - p.pos[0]).powi(2)
                    + (kp[1] - p.pos[1]).powi(2)
                    + (kp[2] - p.pos[2]).powi(2))
                .sqrt(),
            );
            assert!(
                wrap_pi(ky - p.ang[0]).abs() <= KF_TOL_ANG + 0.001,
                "yaw 超容差"
            );
            assert!(
                (kpitch - p.ang[1]).abs() <= KF_TOL_ANG + 0.001,
                "pitch 超容差"
            );
        }
        // 上界 = 走廊容差 + 落盘舍入传播（两个端点各 ≤0.5cm 舍入 → 段内 ≤1cm）
        assert!(
            max_err <= KF_TOL_POS + 0.006,
            "位置最大偏差 {max_err}m 超容差"
        );
        assert!(end > start);
        // 航向为解卷绕连续域：相邻关键帧 |Δ| < π（跨 ±π 不出现反甩）
        for i in 0..kf.t.len() - 1 {
            assert!(
                (kf.yaw[i + 1] - kf.yaw[i]).abs() < std::f32::consts::PI,
                "关键帧航向不连续：{i} → {}",
                i + 1
            );
        }
    }

    /// 跳变保留（对照 10Hz 混叠）：客户端在两次位置更新之间保持原地、收包后跳变。
    /// 关键帧折线复现阶梯（存在陡斜率段与保持段），而按 10Hz 网格重采样 + 线性插值会把
    /// 阶梯抹成滑行——同一 60Hz 帧序列上两者偏差相差一个量级。
    #[test]
    fn keyframes_keep_holds_and_jumps_unlike_grid_aliasing() {
        // 12Hz 位置更新（与 0.1s 网格不同相 → 网格重采样必然混叠），每步 1.4m
        let tl =
            filter::SegmentedPoseTimeline::build(&synthetic_samples(1.4, 1.0 / 12.0, 300), &[]).unwrap();
        let kf = pose_keyframes(&tl);
        let (start, end) = tl.time_range();
        let frames: Vec<(f64, [f32; 3])> = (0..tl.frame_count())
            .map(|k| {
                let t = start + (k as f64 + 0.5) * filter::FRAME_DT;
                let p = tl.pose_at(t, false).unwrap();
                (p.time, p.pos)
            })
            .collect();

        let dist = |a: [f32; 3], b: [f32; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        let mut err_kf = 0.0f32;
        for (t, pos) in &frames {
            let (kp, _, _) = sample_kf(&kf, *t as f32);
            err_kf = err_kf.max(dist(kp, *pos));
        }
        // 10Hz 网格重采样 + 线性插值（改动前的渲染路径）
        let grid: Vec<(f32, [f32; 3])> = (0..)
            .map(|i| start + i as f64 * GRID_DT as f64)
            .take_while(|t| *t <= end)
            .map(|t| (t as f32, tl.pose_at(t, false).unwrap().pos))
            .collect();
        let mut err_grid = 0.0f32;
        for (t, pos) in &frames {
            let t = *t as f32;
            let i = grid
                .partition_point(|(gt, _)| *gt <= t)
                .saturating_sub(1)
                .min(grid.len() - 2);
            let f = (t - grid[i].0) / (grid[i + 1].0 - grid[i].0);
            let g = [0, 1, 2].map(|c| grid[i].1[c] + (grid[i + 1].1[c] - grid[i].1[c]) * f);
            err_grid = err_grid.max(dist(g, *pos));
        }
        assert!(err_kf <= KF_TOL_POS + 0.006, "折线偏差 {err_kf}m 超容差");
        assert!(
            err_grid > 0.15,
            "网格混叠偏差应显著（实际 {err_grid}m）——若此断言失败，说明合成节奏与网格同相，\
             调整合成参数而不是放宽断言"
        );
        // 阶梯结构确实被保留：存在 ≥1m 的陡段（跳变）与 ≥0.15s 的零位移保持段
        let mut max_step = 0.0f32;
        let mut max_hold = 0.0f32;
        for i in 0..kf.t.len() - 1 {
            let d = dist(
                [kf.pos[i * 3], kf.pos[i * 3 + 1], kf.pos[i * 3 + 2]],
                [
                    kf.pos[(i + 1) * 3],
                    kf.pos[(i + 1) * 3 + 1],
                    kf.pos[(i + 1) * 3 + 2],
                ],
            );
            let dt = kf.t[i + 1] - kf.t[i];
            max_step = max_step.max(d);
            if d < 1e-4 {
                max_hold = max_hold.max(dt);
            }
        }
        assert!(max_step >= 1.0, "跳变段未被保留（最大段长 {max_step}m）");
        assert!(max_hold >= 0.15, "保持段未被保留（最长保持 {max_hold}s）");
    }

    // ---------- 端到端探针（真实回放分布证据） ----------
    //
    // 运行：WOTB_PLAYBACK_PROBE=data/replay_samples cargo test playback_probe -- --ignored --nocapture
    //
    // 判定项：
    //   P1 车辆收录：候选实体数 ∈ [10, 16]（14 车 ± 解析边界；相机/区域/炮塔残骸
    //      由 Type5 class=2 实体类型过滤）；
    //   P2 位姿一致性：开火时刻的网格采样位 ≈ ShotReplayData.shooter_render（同为滤波器输出，
    //      仅 0.1s 网格舍入差，≤0.6m）；
    //   P3 角度连续性/值域：解卷绕后 hull_yaw/turret_yaw 相邻网格 |Δ| ≤ π/2（0.1s 物理极限，
    //      wrap 跳变 ≈2π 在此拦截）、gun_pitch ∈ [-1.575, 1.575] rad；
    //   P4 血量链：max_hp>0、HP 单调不增、死亡车末值 0。
    #[test]
    #[ignore = "端到端探针：WOTB_PLAYBACK_PROBE=<path|dir> cargo test playback_probe -- --ignored --nocapture"]
    fn playback_probe() {
        let root =
            std::env::var("WOTB_PLAYBACK_PROBE").unwrap_or_else(|_| "data/replay_samples".into());
        let path = std::path::Path::new(&root);
        let files: Vec<std::path::PathBuf> = if path.is_file() {
            vec![path.to_path_buf()]
        } else {
            std::fs::read_dir(path)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "wotbreplay").unwrap_or(false))
                .collect()
        };
        assert!(!files.is_empty(), "未找到回放文件：{root}");

        for file in &files {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let f = std::fs::File::open(file).unwrap();
            let mut replay = wotbreplay_parser::replay::Replay::open(f).unwrap();
            let data = replay.read_data().unwrap();
            let packets = crate::replay::packets::test_data_tuples(&data);
            let br = replay.read_battle_results().ok();
            let players: Vec<PlaybackPlayer> = br
                .as_ref()
                .map(|br| {
                    br.player_results
                        .iter()
                        .map(|pr| {
                            let joined = br
                                .players
                                .iter()
                                .find(|p| p.account_id == pr.info.account_id);
                            PlaybackPlayer {
                                account_id: pr.info.account_id,
                                nickname: joined
                                    .map(|p| p.info.nickname.clone())
                                    .unwrap_or_default(),
                                team: joined
                                    .map(|p| if p.info.team == 1 { 1u8 } else { 2u8 })
                                    .unwrap_or(0),
                                tank_id: pr.info.tank_id,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let input = PlaybackInput {
                packets: &packets,
                players,
                author_account_id: br.as_ref().map(|b| b.author.account_id).unwrap_or(0),
                winner_team: 0,
                map_id: 0,
                map_name: String::new(),
                pitch_limits: &GunPitchLimits::new(),
                tank_names: HashMap::new(),
            };
            let pb = build_playback_data(&input).unwrap_or_else(|e| panic!("{name}: 构建失败 {e}"));

            // P1 车辆收录（仅完整战斗；退化样例（训练房/片段，时长短或车辆少）跳过断言）
            let nv = pb.vehicles.len();
            let named = pb.vehicles.iter().filter(|v| v.team != 0).count();
            eprintln!("--- {name}");
            eprintln!(
                "    P1 车辆 {nv}（具名 {named}），射击 {} 发，击杀 {}，时长 {:.1}s",
                pb.shots.len(),
                pb.kills.len(),
                pb.meta.duration
            );
            if nv < 10 || pb.meta.duration < 60.0 {
                eprintln!(
                    "    ~~ 退化样例（车辆 {nv} / 时长 {:.1}s），跳过断言",
                    pb.meta.duration
                );
                continue;
            }
            // 7v7/10v10 等模式车数不同（GB109=10v10 → 20 车）
            assert!((8..=28).contains(&nv), "P1 车辆数异常 {nv}");
            assert_eq!(
                pb.vehicles.iter().filter(|v| v.is_author).count(),
                1,
                "作者车应恰 1 辆"
            );

            // P2 位姿一致性（开火时刻网格采样 vs 单发复现渲染锚点）
            // PlaybackShot 已精简掉锚点——探针在同模块直接调 collect_all_shots 取原始
            // ShotReplayData（含 shooter_render + fire_time）
            let author_eid = pb.meta.author_eid;
            let shared = combat::build_shot_scan_shared(&packets, author_eid);
            let shots_raw = collect_all_shots(&shared, author_eid, &GunPitchLimits::new())
                .map(|c| c.shots)
                .unwrap_or_default();
            let mut checked = 0usize;
            let mut worst = 0.0f32;
            let by_eid: HashMap<u32, &VehicleTrack> =
                pb.vehicles.iter().map(|v| (v.eid, v)).collect();
            for s in &shots_raw {
                let Some(anchor) = &s.shooter_render else {
                    continue;
                };
                let Some(sv) = by_eid.get(&s.shooter_eid) else {
                    continue;
                };
                // 锚点 = 命中/开火事件帧（ceil）；网格是 0.1s 重采样，事件帧必落在相邻两格
                // 之间——判据：锚点距较近端 ≤ 0.3m + 0.12×局部车速（重采样容差，非固定阈值）
                let fi = (s.fire_time - pb.meta.t_start) / GRID_DT;
                if fi < 0.0 {
                    continue;
                }
                let i0 = (fi.floor() as usize).min(pb.meta.samples - 1);
                let i1 = (i0 + 1).min(pb.meta.samples - 1);
                let gp = |i: usize, k: usize| sv.pos[i * 3 + k];
                let d = |i: usize| {
                    ((gp(i, 0) - anchor.pos[0]).powi(2)
                        + (gp(i, 1) - anchor.pos[1]).powi(2)
                        + (gp(i, 2) - anchor.pos[2]).powi(2))
                    .sqrt()
                };
                let dmin = d(i0).min(d(i1));
                let v = ((gp(i1, 0) - gp(i0, 0)).powi(2)
                    + (gp(i1, 1) - gp(i0, 1)).powi(2)
                    + (gp(i1, 2) - gp(i0, 2)).powi(2))
                .sqrt()
                    / GRID_DT;
                worst = worst.max(dmin);
                checked += 1;
                assert!(
                    dmin <= 0.3 + 0.12 * v,
                    "P2 开火位姿偏差 {dmin:.3}m（局部车速 {v:.1}m/s）@ {} t={}",
                    name,
                    s.fire_time
                );
            }
            assert!(checked > 0, "P2 无可校验射击（无 shooter_render 锚点）");
            eprintln!("    P2 位姿一致性 {checked} 发，最大偏差 {worst:.3}m");

            // P3 角度连续性/值域 + P4 血量链
            for v in &pb.vehicles {
                let n = pb.meta.samples;
                assert_eq!(v.hull_yaw.len(), n, "{} 网格长度", v.eid);
                for i in 0..n {
                    // 解卷绕契约哨兵：解卷绕后相邻网格差恒 = 短弧 ∈ [-π,π]（r3 舍入余量 0.01），
                    // rawΔ≈±2π 的 wrap 跳变在此拦截（防落盘逻辑改动漏掉解卷绕）。
                    // |Δ|>π/2 的事件性跳变（AoI 断流重续朝向真实改变/死亡重置）为合法数据，
                    // 仅信息打印——它们或落在 coverage 空洞边界（前端不渲染），或本就是信息缺口。
                    if i + 1 < n {
                        let dh = v.hull_yaw[i + 1] - v.hull_yaw[i];
                        assert!(
                            dh.abs() <= std::f32::consts::PI + 0.01,
                            "P3 hull_yaw 存在未解卷绕跳变 {} @{} Δ={dh:.3}",
                            v.eid,
                            i
                        );
                        let d_tw = v.turret_yaw[i + 1] - v.turret_yaw[i];
                        assert!(
                            d_tw.abs() <= std::f32::consts::PI + 0.01,
                            "P3 turret_yaw 存在未解卷绕跳变 {} @{} Δ={d_tw:.3}",
                            v.eid,
                            i
                        );
                        let t = pb.meta.t_start + i as f32 * GRID_DT;
                        if dh.abs() > std::f32::consts::FRAC_PI_2
                            || d_tw.abs() > std::f32::consts::FRAC_PI_2
                        {
                            eprintln!("    P3 大角变 {} t={t:.1} Δhull={dh:+.3} Δturret={d_tw:+.3}（death={:?}）",
                                v.eid, v.death_t);
                        }
                    }
                    assert!(
                        v.gun_pitch[i] >= -1.575 && v.gun_pitch[i] <= 1.575,
                        // 值域仅为量纲哨兵（度→弧度错开会到 ±57）：解码值域 = 车型俯仰极限
                        // （SPG 仰角 ~75°=1.31rad），兜底值 = 车体 pitch（跌落/翻滚可近 ±π/2）
                        "P3 gun_pitch 越域 {} {}",
                        v.eid,
                        v.gun_pitch[i]
                    );
                }
                if v.team != 0 {
                    assert!(v.max_hp > 0, "P4 具名车 max_hp=0 {}", v.nickname);
                }
                let mut hp_prev = v.max_hp;
                for (t, h) in &v.hp {
                    if *h > hp_prev {
                        // 复活模式（爆燃等）：死亡后血量链回归正值 = 复活证据，必须
                        // 恰落在解析器声明的复活锚点上；无锚点的回升仍是契约违例。
                        assert!(
                            v.resurrection_times.iter().any(|rt| (rt - t).abs() < 1e-6),
                            "P4 HP 回升无复活锚点 {} {}: {hp_prev}→{h} @t={t}",
                            v.eid,
                            v.nickname
                        );
                        assert!(*h <= v.max_hp, "P4 复活血量超上限 {} {}", v.eid, v.nickname);
                    }
                    hp_prev = *h;
                }
                if v.death_t.is_some() && !v.hp.is_empty() {
                    // 死亡次数 > 复活次数 ⟺ 最后一段生命以死亡收尾且方法1链在证
                    // → 末 HP 归 0。死亡次数 == 复活次数时末 HP 不设断言：复活证据
                    // 可经 prop3（作者车血量常只走 prop3，method1 链不体现）。
                    let deaths_count = if v.death_times.is_empty() {
                        usize::from(v.death_t.is_some())
                    } else {
                        v.death_times.len()
                    };
                    if deaths_count > v.resurrection_times.len() {
                        assert_eq!(
                            v.hp.last().unwrap().1,
                            0,
                            "P4 死亡收尾车末 HP 应为 0 {}",
                            v.nickname
                        );
                    }
                }
            }
        }
    }
    /// 侧倾列（`roll_nearest`）：辅助构造按时钟升序的 type=10 采样
    fn roll_samples(clocks_rolls: &[(f32, f32)]) -> Vec<combat::St10Sample> {
        clocks_rolls
            .iter()
            .map(|(c, r)| combat::St10Sample {
                clock: *c,
                pos: [0.0; 3],
                yaw: 0.0,
                pitch: 0.0,
                roll: *r,
                pos_error: [0.0; 3],
            })
            .collect()
    }

    /// 侧倾取值 = 最近邻：段内不插值、边界保持最近已知（与 anchors 对 pitch/roll 同规则）
    #[test]
    fn roll_nearest_holds_and_never_interpolates() {
        let s = roll_samples(&[(10.0, 0.1), (10.5, 0.2), (20.0, -0.3)]);
        assert_eq!(roll_nearest(&[], 5.0), None, "无采样 → None（调用方落 0）");
        assert_eq!(roll_nearest(&s, 5.0), Some(0.1), "早于首样本 → 首样本");
        assert_eq!(roll_nearest(&s, 10.0), Some(0.1), "命中样本");
        assert_eq!(roll_nearest(&s, 10.24), Some(0.1), "更近前样本");
        assert_eq!(roll_nearest(&s, 10.26), Some(0.2), "更近后样本");
        assert_eq!(roll_nearest(&s, 10.25), Some(0.1), "等距取前（确定性）");
        assert_eq!(
            roll_nearest(&s, 15.0),
            Some(0.2),
            "跨断段只保持最近已知，不插值"
        );
        assert_eq!(roll_nearest(&s, 99.0), Some(-0.3), "晚于末样本 → 末样本");
    }
}

#[cfg(test)]
mod detached_turret_tests {
    use super::*;
    use crate::replay::detached_turrets::{DetachedTurretPresence, DetachedTurretRecord};
    use crate::replay::model::ReplayModel;

    fn pose(t: f32, x: f32, angles: [f32; 3]) -> combat::St10Sample {
        combat::St10Sample {
            clock: t,
            pos: [x, 10.0, 20.0],
            yaw: angles[0],
            pitch: angles[1],
            roll: angles[2],
            pos_error: [0.0; 3],
        }
    }
    fn model() -> ReplayModel {
        let mut m = ReplayModel::default();
        m.timeline.detached_turrets.push(DetachedTurretRecord {
            eid: 20,
            vehicle_eid: 1,
            presence: vec![DetachedTurretPresence {
                pose: pose(2.0, 5.0, [3.1; 3]),
                t_out: Some(9.0),
            }],
        });
        m.timeline.poses.insert(
            20,
            vec![pose(2.1, 6.0, [-3.1; 3]), pose(2.2, 8.0, [-3.0; 3])],
        );
        m
    }
    fn tracks(m: &ReplayModel) -> Vec<DetachedTurretTrack> {
        detached_turret_tracks(m, &HashSet::from([1]))
    }
    #[test]
    fn initial_pose_raw_motion_and_all_three_angles_are_preserved() {
        let r = tracks(&model());
        let p = &r[0].pose_kf;
        assert_eq!(p.t, [2.0, 2.1, 2.2]);
        assert_eq!(p.pos, [5.0, 10.0, 20.0, 6.0, 10.0, 20.0, 8.0, 10.0, 20.0]);
        for angle in [&p.yaw, &p.pitch, &p.roll] {
            assert!(angle[1] > std::f32::consts::PI);
            assert!((angle[1] - angle[0]).abs() < 0.1);
            assert!((angle[2] - angle[1]).abs() < 0.11);
        }
        assert_eq!(r[0].coverage, [2.0, 9.0]); // stationary final pose stays present
    }
    #[test]
    fn same_clock_type10_updates_use_the_final_received_real_pose() {
        let mut m = model();
        m.timeline
            .poses
            .insert(20, vec![pose(2.1, 6.0, [0.0; 3]), pose(2.1, 7.0, [0.1; 3])]);
        let r = tracks(&m);
        assert_eq!(r[0].pose_kf.t, [2.0, 2.1]);
        assert_eq!(r[0].pose_kf.pos[3], 7.0);
        assert_eq!(r[0].pose_kf.roll[1], 0.1);
    }
    #[test]
    fn reentry_has_its_own_seed_and_hidden_updates_do_not_bridge_it() {
        let mut m = model();
        m.timeline.detached_turrets[0].presence[0].t_out = Some(3.0);
        m.timeline.detached_turrets[0]
            .presence
            .push(DetachedTurretPresence {
                pose: pose(3.1, 50.0, [-3.0; 3]),
                t_out: Some(4.0),
            });
        m.timeline
            .poses
            .get_mut(&20)
            .unwrap()
            .extend([pose(3.05, 30.0, [0.0; 3]), pose(3.2, 51.0, [3.1; 3])]);
        let r = tracks(&m);
        assert_eq!(r[0].coverage, [2.0, 3.0, 3.1, 4.0]);
        assert_eq!(r[0].pose_kf.t, [2.0, 2.1, 2.2, 3.1, 3.2]);
        assert_eq!(r[0].pose_kf.pos[9], 50.0);
        assert_eq!(r[0].pose_kf.roll[3], -3.0); // new AoI has independent unwrap
    }
    #[test]
    fn materialization_only_is_valid_and_absent_owner_or_nonfinite_pose_fail_closed() {
        let mut m = model();
        m.timeline.poses.clear();
        let r = tracks(&m);
        assert_eq!(r[0].pose_kf.t, [2.0]);
        assert_eq!(r[0].coverage, [2.0, 9.0]);
        assert!(detached_turret_tracks(&m, &HashSet::new()).is_empty());
        m.timeline
            .poses
            .insert(20, vec![pose(3.0, f32::NAN, [0.0; 3])]);
        assert!(tracks(&m).is_empty());
        m.timeline
            .poses
            .insert(20, vec![pose(f32::NAN, 0.0, [0.0; 3])]);
        assert!(tracks(&m).is_empty());
    }
    #[test]
    fn build_pipeline_links_owner_and_never_mislabels_a_detached_rigidbody_as_a_vehicle() {
        fn create(eid: u32, class: u16, owner: u32) -> Vec<u8> {
            let mut p = vec![0; 48];
            p[..4].copy_from_slice(&eid.to_le_bytes());
            p[4..6].copy_from_slice(&class.to_le_bytes());
            p[38..42].copy_from_slice(&6u32.to_le_bytes());
            p[42] = 1;
            p[44..48].copy_from_slice(&owner.to_le_bytes());
            p
        }
        fn sample(eid: u32, x: f32) -> Vec<u8> {
            let mut p = vec![0; 48];
            p[..4].copy_from_slice(&eid.to_le_bytes());
            p[12..16].copy_from_slice(&x.to_le_bytes());
            p
        }
        // 车辆必须是结算花名册参与者：type5 满血锚点 + 昵称块 + 同名花名册记录
        let mut vehicle = create(1, 2, 0);
        vehicle.resize(58 + 6, 0);
        vehicle[57] = 6;
        vehicle[58..64].copy_from_slice(b"Tester");
        let mut packets = vec![(5, 0.0, vehicle)];
        for i in 0..30 {
            packets.push((10, i as f32 * 0.1, sample(1, i as f32)));
        }
        let limits = GunPitchLimits::default();
        let players = vec![PlaybackPlayer {
            account_id: 7,
            nickname: "Tester".into(),
            team: 1,
            tank_id: 0,
        }];
        let build = |p: &[(u32, f32, Vec<u8>)]| {
            let tuples = p
                .iter()
                .map(|(kind, t, b)| (*kind, *t, b.as_slice()))
                .collect::<Vec<_>>();
            build_playback_data(&PlaybackInput {
                packets: &tuples,
                players: players.clone(),
                author_account_id: 0,
                winner_team: 0,
                map_id: 0,
                map_name: String::new(),
                pitch_limits: &limits,
                tank_names: HashMap::new(),
            })
            .unwrap()
        };
        let baseline = build(&packets);
        packets.extend([
            (33, 3.0, 20u32.to_le_bytes().to_vec()),
            (5, 3.1, create(20, 8, 1)),
        ]);
        for i in 0..30 {
            packets.push((10, 3.2 + i as f32 * 0.1, sample(20, i as f32)));
        }
        let pb = build(&packets);
        assert_eq!(
            pb.vehicles.len(),
            1,
            "class8 excluded from prop2-absent fallback"
        );
        assert_eq!(pb.vehicles[0].eid, 1);
        assert_eq!(pb.detached_turrets[0].vehicle_eid, pb.vehicles[0].eid);
        assert_eq!(pb.detached_turrets[0].pose_kf.t.len(), 31);
        assert_eq!(
            serde_json::to_value(&baseline.vehicles[0].pose_kf).unwrap(),
            serde_json::to_value(&pb.vehicles[0].pose_kf).unwrap()
        );
        assert!(serde_json::to_value(baseline)
            .unwrap()
            .get("detached_turrets")
            .is_none());
        packets.push((10, f32::NAN, sample(20, 0.0)));
        let mut turret7 = vec![0; 14]; // [eid][prop=2][…][packed u16]，NaN 时钟 type=7
        turret7[..4].copy_from_slice(&1u32.to_le_bytes());
        turret7[4..8].copy_from_slice(&2u32.to_le_bytes());
        packets.push((7, f32::NAN, turret7));
        let invalid = build(&packets);
        assert_eq!(invalid.vehicles.len(), 1);
        assert!(
            invalid.detached_turrets.is_empty(),
            "invalid detached clock must not panic or leak"
        );
    }
    #[test]
    fn roll_is_optional_on_vehicle_keyframes_and_complete_on_detached_track() {
        let old = serde_json::to_value(PoseKeyframes::default()).unwrap();
        assert!(old.get("roll").is_none());
        let r = serde_json::to_value(&tracks(&model())[0]).unwrap();
        assert_eq!(r["pose_kf"]["roll"].as_array().unwrap().len(), 3);
        assert_eq!(r["eid"], 20);
        assert_eq!(r["vehicle_eid"], 1);
    }
}
