//! 浏览器通道解析入口（架构契约第 6 节）：`.wotbreplay` 字节 → 核心库 → 独立能力 JSON。
//!
//! 能力边界（契约 v2）：Agent Rust Core 暴露**结果解释**、**时序解释**与**轻量证据
//! probe** 三类独立能力，消费方（WotBTools）按需取用——只要结果时不得被迫物化全场
//! 时序（~10MB 级），时序消费也不依赖结果通道；初始位姿 probe 只取一个事实，不得
//! 构建 Playback/时间线。名人堂（HoF）是消费方产品域：由消费方从结果能力自行投影，
//! Agent 公开面不感知。
//!
//! JS 入口（wasm32，`js` 模块）：
//! - `parseResult(bytes)`    → 结算 JSON（BattleSummary：花名册/胜负/地图/全员统计）；
//!   只读 meta + battle_results，**不读包流、不建时序模型**，单文件毫秒级；
//! - `parsePlayback(bytes)`  → PlaybackData JSON（位姿网格/弹道/击杀/阶段/可见性）；
//! - `parseShotReplays(bytes)` → 全员射击链 JSON（`{shots, author_path, others}`，
//!   契约 v0.1.9：作者严格路径 fail-visible，不再静默吞空）；
//! - `parseAiReview(bytes)`  → AI 事件数据 JSON（DTO 冻结 v1）；
//! - `parseAuthorInitialPosition(bytes)` → 录像者车辆首条可信 type=10 世界坐标
//!   JSON，或 `null`（轻量能力：不构建 ReplayModel / PlaybackData / AiReviewFacet）；
//! - `parseRecorderCamera(bytes)` → 录像者相机状态 JSON，或 `null`（结构校验未通过）。
//!
//! 客户端路径的已知取舍（与服务端路径的差异，均为数据可得性而非实现差异）：
//! - 无 tank_cache / models.pb：`tank_name` 空串、`gun_pitch` 走车体 pitch 兜底、
//!   无俯仰极限锚定（逐发质量标记如实透传）；前端可按 tank_id 自行映射展示名。
//!   俯仰锚定与弹种反解均可由消费方经可选参数注入（`limits_json` / `shells_json`，
//!   数据源 = 静态资产面 / `dump-shell-kinds` 产物），注入后与服务端路径同级；
//!   `limits_json` 只含基础 models.pb 极限，实体实际携带的俯仰配件（122）由 core
//!   从回放 Type5 配件串自动叠加；
//! - 无地图显示名注册表：`map_name` 为解析器枚举名，前端以 `map_id` 键控底图与语义。
//!
//! 纯逻辑不依赖平台 API，原生与 wasm32 同构、原生可测（tests/facets_smoke.rs）。

use std::collections::HashMap;
use std::io::Cursor;

use wotb_replay_core::replay::combat::GunPitchLimits;
use wotb_replay_core::replay::model::{ReplayModel, ScanInput};
use wotb_replay_core::replay::parser::ReplayParser;
use wotb_replay_core::replay::packets::read_raw_packets;
use wotb_replay_core::replay::playback::{PlaybackPlayer, PlaybackRenderInput};

fn roster_of(summary: &wotb_replay_core::models::battle::BattleSummary) -> Vec<PlaybackPlayer> {
    summary
        .players
        .iter()
        .map(|p| PlaybackPlayer {
            account_id: p.account_id,
            nickname: p.nickname.clone(),
            team: p.team,
            tank_id: p.tank_id,
        })
        .collect()
}

/// 解析可选的车型名表 JSON（`{tank_id: name}`，`tankNamesJson` 注入参数）。
/// 缺省/空串/非法 JSON 一律按"未注入"处理——注入失败不得让整条解析链失败。
fn parse_tank_names(json: Option<&str>) -> HashMap<u32, String> {
    json.filter(|s| !s.trim().is_empty())
        .and_then(|s| serde_json::from_str::<HashMap<String, String>>(s).ok())
        .map(|m| {
            m.into_iter()
                .filter_map(|(k, v)| k.parse::<u32>().ok().map(|id| (id, v)))
                .collect()
        })
        .unwrap_or_default()
}

/// 结果能力（Result interpretation）：字节 → BattleSummary JSON。
/// 只解析 meta + battle_results——**不读包流、不建时序模型**，单文件毫秒级，
/// 供批量扫描与消费方 HoF 投影（HoF 不是 Agent 公开能力，见契约 v2）。
///
/// `tank_names_json`：可选的车型名表（`{tank_id: name}`，`wotb-agent dump-tank-data`
/// 或资产面 `tank/{id}.json` 可组装）。客户端路径无 tank_cache，缺省时 `tank_name`
/// 为 `tank_{id}`（**不是空串**）。
pub fn result_json(bytes: &[u8], tank_names_json: Option<&str>) -> anyhow::Result<String> {
    let mut replay = wotbreplay_parser::replay::Replay::open(Cursor::new(bytes))?;
    let mut summary = ReplayParser::new().parse_replay(&mut replay, "client.wotbreplay")?;
    // arenaBonusType / 地图代号 / 客户端版本来自容器原始字节（crate 未暴露）
    wotb_replay_core::replay::parser::apply_container_fields(&mut summary, bytes);

    // 车型名表注入：只替换命中项，未命中保持 `tank_{id}`（unknown ≠ 编造）
    let names = parse_tank_names(tank_names_json);
    if !names.is_empty() {
        if let Some(n) = names.get(&summary.author_tank_id) {
            summary.author_tank_name = n.clone();
        }
        for p in &mut summary.players {
            if let Some(n) = names.get(&p.tank_id) {
                p.tank_name = n.clone();
            }
        }
    }
    Ok(serde_json::to_string(&summary)?)
}

/// 轻量初始位姿能力（Author initial position）：字节 → 录像者车辆首条可信
/// type=10 世界坐标 JSON，或 `null`（证据不足）。
///
/// 与 `parsePlayback` 的关键差异：**不构建 ReplayModel / PlaybackData /
/// AiReviewFacet**。只读 meta + battle_results 解析作者身份，随后定向扫描包流：
/// 1. 身份 = ARENA_INFO comp blob 的 account_id 精确匹配，未命中退回既有 type=5
///    昵称解析（与 ReplayModel::scan 同一函数，SSOT）；
/// 2. 位姿 = 该 eid 的 type=10 采样中 parent==0（世界系）、位置/时钟有限、
///    载荷可解码的最早 replay clock 采样。
///
/// 输出坐标是**原始 replay 世界坐标** [x, y, z]（y 为高程），不做地图坐标转换、
/// 不输出 NORTH/SOUTH/WEST/EAST 或 spawn 语义——那些属于消费方产品域。
/// 截断/不完整回放只要已出现作者世界位姿即可返回；无证据（作者未解析 / 无
/// 可信采样 / 全被跳过）返回 `null`，绝不编造 [0,0,0]。
pub fn author_initial_position_json(bytes: &[u8]) -> anyhow::Result<String> {
    // 能力实现全部在核心库（原生/ wasm32 同构）；WASM 只做 JSON 序列化。
    let position = wotb_replay_core::replay::author::from_replay_bytes(bytes)?;
    Ok(serde_json::to_string(&position)?)
}

/// 录像者相机能力：Type23 模式 + Type31 缩放 + Type39 瞄准 + method36 bloom。
/// 协议形状不符合已验证结构时返回 `null`，消费方据此禁用录像者视角。
pub fn recorder_camera_json(bytes: &[u8]) -> anyhow::Result<String> {
    let camera = wotb_replay_core::replay::recorder_camera::from_replay_bytes(bytes)?;
    Ok(serde_json::to_string(&camera)?)
}

/// 时序能力（Temporal interpretation）：字节 → PlaybackData JSON。
/// 单次扫描构建全场时序（与服务端 `/api/playback/data` 同一构建语义）。
///
/// `tank_names_json`：可选的车型名表（同 [`result_json`]）——注入后 `vehicles[].tank_name`
/// 为真实车型名；缺省时为空串（客户端路径无 tank_cache，前端按 `tank_id` 自行映射）。
pub fn playback_json(
    bytes: &[u8],
    tank_names_json: Option<&str>,
    limits_json: Option<&str>,
) -> anyhow::Result<String> {
    let mut replay = wotbreplay_parser::replay::Replay::open(Cursor::new(bytes))?;
    let summary = ReplayParser::new().parse_replay(&mut replay, "client.wotbreplay")?;
    let packets = read_raw_packets(bytes)?;
    let packets: Vec<(u32, f32, &[u8])> = packets
        .iter()
        .map(|p| (p.packet_type, p.clock_secs, p.payload.as_slice()))
        .collect();

    // 俯仰锚定表注入（与 parseShotReplays 的 limitsJson 同款）：
    // {昵称: GunPitchRange}，消费方由资产面 tank/{id}.json 的**基础** pitch_limits 组装。
    // 实体实际携带的俯仰配件（122：+4°仰/+3°俯）由 core 按同 eid 的 Type5 配件串在解码时
    // 叠加，注入表不要预先含设备加成（会双重计算）。
    // 缺省空表时 gun_pitch 走 hull_pitch 兜底（坡上炮管俯仰不正确的根因）。
    // 非空但**非法**的注入 JSON → Err（fail-visible）：静默当空表会让俯仰无声降级且
    // 无逐发质量标记可查，属调用方契约违约而非数据边界（tankNamesJson 是展示串，仍按未注入处理）。
    let limits: GunPitchLimits = match limits_json {
        Some(s) if !s.is_empty() => serde_json::from_str(s)
            .map_err(|e| anyhow::anyhow!("limitsJson 非法（俯仰锚定表注入违约）: {e}"))?,
        _ => GunPitchLimits::new(),
    };
    let model = ReplayModel::scan(&ScanInput {
        packets: &packets,
        roster: &roster_of(&summary),
        author_account_id: summary.author_account_id,
        pitch_limits: &limits,
    })?;
    // 投影层身份一律取模型实体并表；车型名表由消费方可选注入（缺省空表 → tank_name 空串）
    let tank_names = parse_tank_names(tank_names_json);
    let render = PlaybackRenderInput {
        winner_team: summary.winner_team,
        map_id: summary.map_id,
        map_name: summary.map_name.clone(),
        pitch_limits: &limits,
        tank_names: &tank_names,
    };
    let mut playback = wotb_replay_core::replay::playback::from_model(&model, &render)?;
    // armorsStates 时间轴 + 逐弹弹着状态（原始包证据，与下方 comp 透传同层回填；
    // 仅动态装甲车非空，静态车字段整体省略）
    wotb_replay_core::replay::playback::attach_packet_evidence(&mut playback, &packets);
    // comp blob 模块局部 id 透传（纯回放证据，无坦克数据依赖）：消费端用
    // 资产面 tank/{id}.json 的 configs[].turret_local/gun_local 联表，即可在客户端
    // 路径钉定实际搭载配置（弹容 N 的上游权威 = 服务器路径的 config_idx/burst_size）。
    let valid_tanks: Vec<u32> = summary.players.iter().map(|p| p.tank_id).collect();
    let comps = wotb_replay_core::replay::playback::collect_comp_descriptors(&packets, &valid_tanks);
    wotb_replay_core::replay::playback::annotate_vehicle_comp_locals(&mut playback.vehicles, &comps);
    Ok(serde_json::to_string(&playback)?)
}

/// 智能体评审通道（AI 评审入口）：字节 → `AiReviewFacet` JSON
/// （花名册 + 归一化事件流 spawn/shot/damage/kill/visibility/counter/damage_tick +
/// 结算锚点 + 战局阶段）。与服务端 `wotb-agent facets --parts ai` 同一构建语义。
///
/// 契约：DTO 冻结 v1（与 CLI 通道逐字段同构）。事件流按回放时钟升序；
/// `Shot.target_eid` 取自弹道自带身份（服务器权威），不按昵称反查。
pub fn ai_review_json(bytes: &[u8]) -> anyhow::Result<String> {
    let mut replay = wotbreplay_parser::replay::Replay::open(Cursor::new(bytes))?;
    let summary = ReplayParser::new().parse_replay(&mut replay, "client.wotbreplay")?;
    let packets = read_raw_packets(bytes)?;
    let packets: Vec<(u32, f32, &[u8])> = packets
        .iter()
        .map(|p| (p.packet_type, p.clock_secs, p.payload.as_slice()))
        .collect();

    let limits = GunPitchLimits::new();
    let model = ReplayModel::scan(&ScanInput {
        packets: &packets,
        roster: &roster_of(&summary),
        author_account_id: summary.author_account_id,
        pitch_limits: &limits,
    })?;
    let facet = wotb_replay_core::facets::ai_review::AiReviewFacet::from_model_with_packets(
        &model, &summary, &packets,
    );
    Ok(serde_json::to_string(&facet)?)
}

/// 射击复现通道：字节 → 全员射击链（作者严格路径 + 他人宽松路径合并，含弹道/
/// 命中判定/质量标记/渲染锚点）。形状与上游 Web `/api/replay/shots` 的 shots 数组同构，
/// 供 WotBTools 射击复现视图直接消费（three.js 渲染在消费方）。
///
/// **契约 v0.1.9（breaking）**：输出形状
/// `{shots, author_path: "ok"|"error", author_error?, author_eid, others:{total_launches,
/// skipped_no_endpoint, skipped_no_target_state, muzzle_fallback}|absent, others_error?}`。
/// 此前作者严格路径任何 Err 都被 `unwrap_or_default()` 静默吞成空数组（作者射击链整体
/// 消失且无诊断）；现 `author_path="error"` 时 `author_error` 携带链式原因（仅 error 态
/// 存在该键），`others` 透传他人宽松路径的跳过/兜底统计（fail-soft 边界透明化），消费方
/// fail-visible。`others` 与 `others_error` 互斥：后者仅在他路径**内部不变量破坏**时出现
/// （数据缺失按设计进 skipped_* 计数），同样不静默吞成空统计。
/// 两路合并后 shots 内 index 为局部值，消费方须按 time_s 全局重编号（契约 §shots）。
///
/// `limits_json`：可选的俯仰锚定表（{昵称: {dep, ele, front?, back?, transition?}}，
/// GunPitchRange serde 形状——消费方由资产面 tank/{id}.json 的**基础** pitch_limits 换算
/// dep=max、ele=−min）。服务端路径由 TankResolver 注入同名锚定；客户端路径
/// 缺省为空表——空表下 prop2 frac 无法按车型极限解码，逐发俯仰降级标记会
/// 如实透传（质量边界，非错误）。**注入表是基础极限**：实体实际携带的俯仰配件
/// （122：+4°仰/+3°俯）由 core 按同 eid 的 Type5 配件串叠加，消费方不要预加，
/// 否则双重计算。**非空但非法**的注入 JSON → Err（fail-visible：
/// 静默当空表会让俯仰无声降级且无逐发标记可查，属调用方契约违约）。
///
/// `shells_json`：可选的全局弹种反解表（`wotb-agent dump-shell-kinds` 产物，
/// {全局弹种 id: {type, penetration, damage, module_damage, explosion_radius}}）。
/// 注入后每发输出补齐 `shell_kind`（与上游 annotate 同源）与 `shell`（完整弹
/// 数据，徽标/判定直接消费）——消费方渲染侧不再需要自带弹种表或槽位兜底；
/// 缺省时 shell_kind 保持空串、无 shell 字段（数据可得性边界，非错误）。
/// **非空但非法**的表 JSON → Err（同上理由）。
pub fn shot_replays_json(
    bytes: &[u8],
    limits_json: Option<&str>,
    shells_json: Option<&str>,
) -> anyhow::Result<String> {
    let mut replay = wotbreplay_parser::replay::Replay::open(Cursor::new(bytes))?;
    let summary = ReplayParser::new().parse_replay(&mut replay, "client.wotbreplay")?;
    let packets = read_raw_packets(bytes)?;
    let packets: Vec<(u32, f32, &[u8])> = packets
        .iter()
        .map(|p| (p.packet_type, p.clock_secs, p.payload.as_slice()))
        .collect();

    let author_nick = summary
        .players
        .iter()
        .find(|p| p.account_id == summary.author_account_id)
        .map(|p| p.nickname.clone())
        .unwrap_or_default();
    let author_eid =
        wotb_replay_core::replay::combat::resolve_author_player_eid_by_nick(&packets, &author_nick);
    // 非空但非法的注入 JSON → Err（fail-visible，与 parsePlayback 的 limitsJson 同理由：
    // 静默当空表会让 prop2 俯仰无声降级且无逐发标记可查）
    let limits: GunPitchLimits = match limits_json {
        Some(s) if !s.is_empty() => serde_json::from_str(s)
            .map_err(|e| anyhow::anyhow!("limitsJson 非法（俯仰锚定表注入违约）: {e}"))?,
        _ => GunPitchLimits::new(),
    };

    // 作者严格路径 + 他人宽松路径 + 未归属命中，一次共享扫描三路提取。strict 失败
    // 不再静默降级为空——诊断上抛，shots 仍含他人宽松路径全量（fail-visible，
    // 整场射击复现不因单路径失败不可用）；他人路径 Err 仅在内部不变量破坏时出现，
    // 以 others_error 透传（不静默吞成空统计）
    let bundle = wotb_replay_core::replay::combat::extract_shot_bundle_auto_with_limits(
        &packets,
        &author_nick,
        author_eid,
        &limits,
    );
    let (author_shots, author_error) = match bundle.author {
        Ok(s) => (s, None),
        Err(e) => (Vec::new(), Some(format!("{e:#}"))),
    };
    let (others, others_error) = match bundle.others {
        Ok(o) => (Some(o), None),
        Err(e) => (None, Some(format!("{e:#}"))),
    };
    let unattached = bundle.unattached_hits;
    let mut shots = author_shots;
    if let Some(others) = &others {
        shots.extend(others.shots.iter().cloned());
    }
    shots.sort_by(|a, b| a.fire_time.partial_cmp(&b.fire_time).unwrap());
    // 动态装甲弹着态注入（additive：仅 6 辆声明车非缺省，消费方按声明门控）
    wotb_replay_core::replay::combat::attach_shot_armor_states(&mut shots, &packets);

    // 弹种反解注入（服务端 annotate + shell 字段注入的客户端等价）：表缺失时
    // 原样输出（消费方按缺数据处理）；非空但非法的表 JSON → Err（fail-visible）
    let shell_table: Option<serde_json::Map<String, serde_json::Value>> = shells_json
        .filter(|s| !s.is_empty())
        .map(|s| {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(s)
                .map_err(|e| anyhow::anyhow!("shellsJson 非法（全局弹种反解表注入违约）: {e}"))
        })
        .transpose()?;
    let mut shots_val = serde_json::to_value(&shots)?;
    if let Some(table) = shell_table {
        if let Some(arr) = shots_val.as_array_mut() {
            for v in arr.iter_mut() {
                let shell_id = v.get("shell_id").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
                if shell_id == 0 {
                    continue;
                }
                let Some(entry) = table.get(&shell_id.to_string()) else {
                    continue;
                };
                if v.get("shell_kind")
                    .and_then(|x| x.as_str())
                    .map(str::is_empty)
                    .unwrap_or(true)
                {
                    if let Some(t) = entry.get("type").and_then(|x| x.as_str()) {
                        v["shell_kind"] = serde_json::Value::String(t.to_string());
                    }
                }
                v["shell"] = entry.clone();
            }
        }
    }
    let mut outcome = serde_json::Map::new();
    outcome.insert("shots".into(), shots_val);
    outcome.insert(
        "author_path".into(),
        serde_json::Value::String(if author_error.is_some() {
            "error".into()
        } else {
            "ok".into()
        }),
    );
    if let Some(err) = author_error {
        outcome.insert("author_error".into(), serde_json::Value::String(err));
    }
    outcome.insert("author_eid".into(), serde_json::json!(author_eid));
    // 有证据但无法归属到任何 shot 的命中通知（对应射击不在录像里 = AoI 截断）。
    // 独立输出而不是丢弃：这些事件带受击者/伤害/结果/令牌，消费方按“未知归属”呈现。
    outcome.insert("unattached_hits".into(), serde_json::to_value(&unattached)?);
    // others 与 others_error 互斥：Err 仅在内部不变量破坏时出现（数据缺失按设计进
    // skipped_* 计数），fail-visible 透传给消费方，不静默吞成空统计
    if let Some(others) = &others {
        outcome.insert(
            "others".into(),
            serde_json::json!({
                "total_launches": others.total_launches,
                "skipped_no_endpoint": others.skipped_no_endpoint,
                "skipped_no_target_state": others.skipped_no_target_state,
                "muzzle_fallback": others.muzzle_fallback,
            }),
        );
    }
    if let Some(err) = others_error {
        outcome.insert("others_error".into(), serde_json::Value::String(err));
    }
    Ok(serde_json::Value::Object(outcome).to_string())
}

#[cfg(target_arch = "wasm32")]
mod js {
    use wasm_bindgen::prelude::*;

    /// JS 入口（结果能力）：`parseResult(new Uint8Array(fileBuffer), tankNamesJson?)`
    /// → BattleSummary JSON 字符串。只读 meta + battle_results（毫秒级），不物化全场时序。
    /// 解析失败以字符串 Error 拒绝（含链式原因），不 panic 跨界。
    /// `tankNamesJson` 可选：`{tank_id: name}` 车型名表——注入后 `tank_name` 为真实名，
    /// 缺省为 `tank_{id}`（客户端无 tank_cache；**不是空串**）。
    #[wasm_bindgen(js_name = parseResult)]
    pub fn parse_result(bytes: &[u8], tank_names: Option<String>) -> Result<String, JsValue> {
        super::result_json(bytes, tank_names.as_deref())
            .map_err(|e| JsValue::from_str(&format!("result parse failed: {e:#}")))
    }

    /// JS 入口（轻量初始位姿）：`parseAuthorInitialPosition(new Uint8Array(fileBuffer))`
    /// → 录像者车辆首条可信 type=10 世界坐标 JSON，或 `null`（证据不足）。
    /// 不构建 PlaybackData/时序模型；输出为原始 replay 世界坐标 [x, y, z]，
    /// **不做**地图坐标转换、不输出 NORTH/SOUTH/WEST/EAST 等产品语义。
    #[wasm_bindgen(js_name = parseAuthorInitialPosition)]
    pub fn parse_author_initial_position(bytes: &[u8]) -> Result<String, JsValue> {
        super::author_initial_position_json(bytes).map_err(|e| {
            JsValue::from_str(&format!("author initial position parse failed: {e:#}"))
        })
    }

    /// JS 入口（录像者相机）：`parseRecorderCamera(new Uint8Array(fileBuffer))`
    /// → 录像者相机状态 JSON，或 `null`（结构校验未通过）。不构建 PlaybackData。
    #[wasm_bindgen(js_name = parseRecorderCamera)]
    pub fn parse_recorder_camera(bytes: &[u8]) -> Result<String, JsValue> {
        super::recorder_camera_json(bytes)
            .map_err(|e| JsValue::from_str(&format!("recorder camera parse failed: {e:#}")))
    }

    /// JS 入口（时序能力）：`parsePlayback(new Uint8Array(fileBuffer), tankNamesJson?)`
    /// → PlaybackData JSON 字符串（位姿网格/弹道/击杀/阶段/可见性）。
    /// `tankNamesJson` 可选：同 `parseResult`——注入后 `vehicles[].tank_name` 为真实车型名。
    /// `limitsJson` 可选：**基础**俯仰锚定表 JSON（{昵称: GunPitchRange}）——注入后 prop2
    /// 俯仰按车型极限解码；实体实际携带 122 时由 core 再叠加 +4°仰/+3°俯（不要预加）。
    /// 缺省时 gun_pitch 走 hull_pitch 兜底（坡上炮管不准）；
    /// **非空但非法**的 JSON 以字符串 Error 拒绝（fail-visible，不做静默空表降级）。
    #[wasm_bindgen(js_name = parsePlayback)]
    pub fn parse_playback(bytes: &[u8], tank_names: Option<String>, limits: Option<String>) -> Result<String, JsValue> {
        super::playback_json(bytes, tank_names.as_deref(), limits.as_deref())
            .map_err(|e| JsValue::from_str(&format!("playback parse failed: {e:#}")))
    }

    /// JS 入口（AI 事件数据）：`parseAiReview(new Uint8Array(fileBuffer))`
    /// → `AiReviewFacet` JSON 字符串。与服务端 `wotb-agent facets --parts ai` 逐字段同构
    /// （花名册 + 归一化事件流 + 结算锚点 + 战局阶段），DTO 冻结 v1。
    #[wasm_bindgen(js_name = parseAiReview)]
    pub fn parse_ai_review(bytes: &[u8]) -> Result<String, JsValue> {
        super::ai_review_json(bytes)
            .map_err(|e| JsValue::from_str(&format!("ai review parse failed: {e:#}")))
    }

    /// JS 入口（射击复现用）：`parseShotReplays(new Uint8Array(fileBuffer), limitsJson?, shellsJson?)`
    /// → `{shots, author_path, author_error?, author_eid, others?, others_error?}` JSON 字符串
    /// （弹道/命中判定/质量标记/渲染锚点；契约 v0.1.9 起 fail-visible——作者严格
    /// 路径失败不再静默吞空，`author_path="error"` 时 `author_error` 携带原因）。
    /// `limitsJson` 可选：**基础**俯仰锚定表 JSON（{昵称: GunPitchRange}，消费方由资产面
    /// tank/{id}.json 的 pitch_limits 组装 dep=max、ele=−min）——注入后 prop2 俯仰
    /// 按车型极限解码（与服务端路径同级）；实体实际携带 122 时由 core 再叠加
    /// +4°仰/+3°俯（不要预加，否则双重计算）；缺省空表时俯仰降级标记如实透传；
    /// **非空但非法**的 JSON 以字符串 Error 拒绝（fail-visible，不做静默空表降级）。
    /// `shellsJson` 可选：全局弹种反解表 JSON（`wotb-agent dump-shell-kinds` 产物，
    /// {全局弹种 id: {type, penetration, damage, module_damage, explosion_radius}}）——
    /// 注入后每发补齐 `shell_kind` 与 `shell`（完整弹数据）；缺省时无弹种反解；
    /// **非空但非法**的 JSON 同样拒绝。`others` 缺失而 `others_error` 在场 = 他人路径
    /// 内部不变量破坏（数据缺失按设计进 skipped_* 计数，不会走到这一步）。
    #[wasm_bindgen(js_name = parseShotReplays)]
    pub fn parse_shot_replays(
        bytes: &[u8],
        limits: Option<String>,
        shells: Option<String>,
    ) -> Result<String, JsValue> {
        super::shot_replays_json(bytes, limits.as_deref(), shells.as_deref())
            .map_err(|e| JsValue::from_str(&format!("shot replay parse failed: {e:#}")))
    }
}
