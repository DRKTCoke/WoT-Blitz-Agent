# 文档索引

> 全部 Markdown 文档的定位与当前状态。入口永远是根目录 [README](../README.md)。
> 本页只承载**当前状态与结论**；推导过程与历史记录见 git 历史。

## 使用文档（随项目演进，保持最新）

| 文档 | 定位 |
|---|---|
| [README.md](../README.md) | 项目总入口：定位、能力总览、与 WotbTools 的关系（分发形态）、仓库结构 |
| [docs/回放与射击逆向总集.md](回放与射击逆向总集.md) | 射击/回放逆向**唯一权威参考**：第一篇=协议层数据段（字节布局/语义/使用状态/死路清单/消费地图）、第二篇=客户端处理架构、第三篇=客户端弹道与命中表现（含 DecodeShotSegment 权威定义）、第四篇=WI 对照与射击复现实现、第五篇=遗留未定项 |
| [docs/replay-contract-v2-supremacy-type39.md](replay-contract-v2-supremacy-type39.md) | **回放契约 v2**：争霸基地状态（sparse 重建 + 空闲整播判据）、实时点数、type39 原始帧、独立炮塔与生命轨道等 additive 切面契约、门禁与版本护栏 |
| [docs/author-initial-position.md](author-initial-position.md) | **录像者初始世界位姿能力**（v0.4.15 additive）：`parseAuthorInitialPosition` 的 JSON 形状、身份/位姿证据规则、`null` 失败边界、坐标轴约定、性能与架构边界、真实夹具回归 |
| [docs/recorder-camera.md](recorder-camera.md) | **录像者相机能力**（v0.4.21 additive）：`parseRecorderCamera` 的 DTO、结构校验、抽稀容差与证据边界 |
| [docs/wotbtools-cross-reference.md](wotbtools-cross-reference.md) | 与 WotbTools 逆向结论的逐条裁决（采纳/驳回/互证），防止误采或回退已定案；附跨仓协作现状 |
| [docs/tank-suspension-client-re.md](tank-suspension-client-re.md) | **客户端坦克悬挂逆向**：处理链与节点绑定契约、逐车 `suspension:` 数据面（`wheels`/履带折线/`chassis.textureScale`）、负重轮-履带求解公式（反汇编还原）、对齐可行性分级与剩余未定项 |
| [docs/architecture-debt.md](architecture-debt.md) | 架构债与长期改动方案：已完成项（combat.rs 拆分、双路径合并）与仍留存项 |
| [docs/game-data-sources.md](game-data-sources.md) | **数据来源权威表**：每份数据取自 BlitzKit / 本机客户端 / WG API / 自产；本地提取链、间隙甲 spaced 规则、DVPL 校验契约、DLC 覆盖层、资产 origin 发布流程（Alibaba；COS legacy）与运维陷阱、俯视合成朝向契约 |
| [docs/data-inventory.md](data-inventory.md) | **数据面清单 / 运行所需数据总账**：运行期 11+1 项核心数据的"目前来源 / 可替代来源 / 替代完成度"+ 派生/注入数据 + 非数据类外部依赖；分发渠道与包内布局；工具索引 |
| [docs/asset-store.md](asset-store.md) | **资产存储与去重**：不可变 release 树 + 内容寻址对象库（硬链接）、跨版本/跨 dataset 去重实测、迁移/adopt/GC/备份契约 |
| [docs/client-research-tech-tree-loadout.md](client-research-tech-tree-loadout.md) | **客户端研发 / 科技树 / 装备-消耗品系统事实总集**（WotbTools 复刻证据基线）：数据源地图、模块 schema、`<unlocks>` 研发图全量、科技树坐标、Screens3 UI 数据契约、9 槽装备骨架与 24 件全表、消耗品/补给过滤器与效果、参数键与已证公式；含 fail-closed 边界与未决项 |
| [docs/wotbtools-research-loadout-plan.md](wotbtools-research-loadout-plan.md) | **WotbTools 复刻实施方案与落地记录**（2026-10-27 裁定后范围）：单一整合配置界面（模块/装备/消耗品补给选择，BlitzKit 布局同构），与装甲查看器模型及百科数值联动；**不做**研发/购买/科技树/分入口；数据源铁律（客户端解析权威 / BlitzKit 备用+公式参照，隐蔽加成按客户端）、`data/research/` 导出契约、计算层规格、联动契约（configIndex/delta/URL）、已知偏差表、P0–P4 分期与测试门禁；§12 实现状态与实现期口径 |
| [docs/decoupling-status.md](decoupling-status.md) | 解耦现状（BlitzKit → 本机客户端自产，已全量完成）：已定案差异清单、封面图与命名域细节、对外沟通材料 |
| [docs/local-model-export.md](local-model-export.md) | 坦克模型本机自产管线：实现铁律（违反会静默出错）、贴图语义口径与通道搬迁、与 BlitzKit 的逐维度对照结论 |
| [docs/map-render-align-audit.md](map-render-align-audit.md) | 地图渲染 × 客户端**差异总账**（36 图普查 33 条逐项现役状态）；**未落地项以此文为准**；光照可行性报告删档后的未定项收编于其 §五 |

## 档案（已执行完毕 / 报告留档）

| 文档 | 状态 |
|---|---|
| [docs/map-lighting-plan.md](map-lighting-plan.md) | 光照实施设计：期 0–2 主体已落地（逐图光照/光照图/IBL）；剩余项见该文与 map-render-align-audit |

> 早期可行性报告（pb 本地提取、GLB 本地导出、地图光照、履带印）与 vue-migration-plan
> 已于 2026-10-29 删档：结论均已并入上列活文档（data-inventory / game-data-sources /
> local-model-export / map-render-align-audit），原文见 git 历史。

> `docs/mobile_plan.md` 及移动端（`mobile/` Tauri 壳、`mobile_assets/` 随包资产）已随
> Android 分发形态一同移除（详见 [README §与 WotbTools 的关系](../README.md)）。

## 当前状态

### 版本与发行

- 版本源 = `Cargo.toml` 的 `[workspace.metadata.release].version`（现值 **v0.4.23**）。
  该值变更合并到 `main` 后，Release workflow 自动测试、构建、创建 `v<version>` tag
  并发布 `wotb-replay-wasm-v<version>.zip`；发布策略与 Android 同步规则见
  [AGENTS.md](../AGENTS.md)。
- 消费方式：WotbTools 经 `deploy/agent/source.json` 按 commit + sha256 锁定 WASM，
  pin bump 时重生成 AI 投影 golden（provenance 字段变化；**行为变化才动字段值**）。
- 面向消费方的版本记录（v0.3.4 起）：

| 版本 | 内容 |
|---|---|
| v0.4.23 | **俯仰解码接入装备 122（解析器行为修正 + additive）**：3D 回放 `vehicles[].gun_pitch` 与射击复现 `shooter_gun_pitch` / `target_gun_pitch` / 两侧时间线按同 eid 的 Type5 `0B` 配件串叠加「改进型垂直稳定器」(122) 的 +4°仰 / +3°俯；基础与 front/back 扇区同增益，`transition` 不变。`VehicleEquipment` additive `improved_pitch_limits`（仅 true 序列化）。无 122 的场次输出逐位不变；当前 12 辆可用车（11 辆 `AlternatePitchGrouserPreset` + Rinoceronte 的 `DrumGunPitchLimitsPreset`）。数据锚定测试锁定 `devices.json` id=122 数值并断言只有 122 声明俯仰增量。WotbTools 装甲查看器配套接入手动钳制；pin bump 后重生成 AI projection golden（provenance 必变，若 fixture 含 122 车则 pitch 值变化属预期）。 |
| v0.4.22 | **作者当前弹种时间线（additive）+ 接收突发与补给线修正**：①playback 切面新增 `author_shell_timeline`（method 0x07 确认流，折叠重复值，按作者首次死亡截断，语义证据见总集 §3.5），供 WotbTools 3D 回放开镜穿深着色；②全车辆 type=10 + prop2 接收突发时间轴重建；③补给线码受控实验纠正与系别族字段 `family`。②③改变解析输出，下游换 pin 时要重审 golden。 |
| v0.4.21 | **录像者相机轻量能力（additive）**：新增第 6 个独立 WASM 入口 `parseRecorderCamera(bytes)`，输出 Type39 瞄准关键帧、Type23 模式、Type31 缩放、method36 bloom 与 `BackwardZoom`；协议形状不符时返回 `null`。证据：46 份本地回放（11.18 / 11.19 / 11.20）零误拒，Type23/31 计数与独立分帧脚本一致；最大 JSON 约 560 KiB，解析约 40 ms。波及面：仅新增入口，现有五个入口输出不变；`collect_aim_snapshots` 与作者身份解析为无行为变化的抽取。 |
| v0.4.20 | **射击提取链单次扫描收口 + 退化兜底 fail-visible 化（内部性能 + 错误面）**：①一次共享扫描三路提取（`extract_shot_bundle_auto_with_limits`：作者严格/他人宽松/未归属命中），`parseShotReplays` 的包流扫描 3→1、CLI diagnose 2→1；②他人宽松路径 `Result` 化——数据缺失照旧 `skipped_*` 计数，仅内部不变量破坏（非续段 + 有终点发射缺少 `notice_windows` 窗口）上抛 Err，WASM 输出 `others?`/`others_error?` 互斥键（与 `author_path="error"` 同款 fail-visible）；③`limitsJson`/`shellsJson` **非空但非法**的注入 JSON → 整体 Err（静默当空表会让俯仰/弹种无声降级且无逐发标记可查；`tankNamesJson` 是展示串，仍按未注入）；④supremacy `base_index>3` 直构输入 panic（wire 收集器已过滤，静默 continue 会让基地时间线缺段无痕迹）。性能（输出逐位不变）：`CombatTimeline::from_parts` 复用共享扫描产物（scan 少 3 次全包流遍历）、`ReplayParser::parse_file` 单次读盘（批量扫描 IO 减半）、anchors 段定位/侧倾插值二分化（≈80k 次调用热路径 O(n)→O(log n)）、`ShellKindTable::shared()` 进程级缓存。证据：全 workspace 测试 + clippy 0；射击/回放/AI 切面单测零变化。**消费方**：WotbTools 读 `others` 前先判 `others_error`；注入表组装不变；pin bump 后重生成 AI 投影 golden（预期仅 provenance）。详见本页 2026-10-31 节与 WotbTools `contracts/agent/replay-facets-v2.md` §5。 |
| v0.4.19 | **消耗品线码 0x3F 闭合并补发**（additive）：`0x3F` = `SHELL_RELOAD_BOOST`（客户端 `newAutoloader`：dur 20s / cd 65s）。证据：M-VI-Yoh（弹夹重坦）slot0 激活 20.0s、冷却 55.555 = 65 × 0.8547（装备 101 冷却缩短），为目录唯一 dur20/cd65 条目；同场 0x3D/0x3E/0x42 已在改进消耗品块内闭合。**消费方**：WotbTools pin bump 后 HUD 用 `reload.webp` 显示该消耗品；未知线码一律不渲染徽标（fail-closed）。**覆盖核对**：客户端 11.21.0 目录 common（可装配）消耗品 13 件与 13 个线码一一对应、0 缺口；其余 51 条为特殊模式专属（bossmode/hellgames/madgames/gravitizing/prototypes/lunar），仅 `0x67` gravityJump 已闭合，未观测线码继续 fail-closed。 |
| v0.4.18 | **战斗装载与消耗品状态闭合（additive）**：①Type5 loadout 解码 3 消耗品 + 0..3 给养 + 模式额外能力 + 9 槽配件；`0A` 描述符按 `payload f32 -1.0` 分流给养，GravityMode 7 条布局能力单列；`vehicles[].loadout`、`equipment_count/ids`、`ConsumableTransition.logical_item_id`。②**本批语料内**全部观测 wireCode 闭合（26 场；语料外 `0x3F` 由 v0.4.19 补入，见上行）：`0x67`=`gravityJump`/Thruster、`0xA9`=`DYNAMIC_ARMOUR_SYSTEM`、欧系/瑞典/捷克/波兰食物块 `0x46..0x4D` 与 `provisions.pb` id 103..110 逐项相差 33（`0x4A`=Goulash/LARGE；纠正 WotbTools `0x48`/`0x49` 互换推断）。③消耗品状态：`state_name`、`duration_s`、`ends_at`（剩余 `clamp(ends_at−t,0,duration_s)`）、`ever_activated`、`slot`；Type32 给养注册原样透传（v0.4.17 首版曾剔除，v0.4.18 恢复；消费方按 slot 过滤）。**消费方**：WotbTools pin bump 后改读 `loadout`/新字段，在 2D/3D HUD 血量条旁显示定时消耗品图标 + 剩余时间环；旧 6 条样本 `loadout_items`/`equipment` 逐字节不变。证据：26 个回放 0 UNKNOWN；J39 定时/瞬发/重力跳冷却闭合；全量 core/workspace 测试 + clippy。 |
| v0.4.17 | 同批首版（战斗装载/消耗品状态）；Type32 给养注册被误剔除，由 v0.4.18 恢复透传。 |
| v0.4.16 | **命中通知归属：时钟容差 → 弹着区间顺序（解析器行为修正）**：method8 / type=32 都没有 shotId（P2 探针 0/457、0/2729），改用**同射手 `[本发弹着, 下一发弹着)` 区间**归属（`notice_windows`，零时钟容差）——旧 `|t−end_time|≤0.05s` 窗小于 10Hz 包钟量子（≈81ms），一次 tick 抖动就把命中通知判成“未转发”（20261029 HE 样本 +0.0811s：`hit_token/segment/game_hit_result` 全丢 → 前端退回弹道弦，82.12°/73.0mm 判穿；服务器段 83.76°/92.0mm 不穿 → 287 溅射，判定结论相反）。哈希令牌降级为 method8↔type=32 的同事件连接；段包未转发/履带命中不发族包时由 method8 兜底，不再整发 fail。**additive**：`shots[].hits[]`（一发多 victim / 多次装甲交互全保留，主目标在首位；额外受击者须与主事件同服务器批次）与 `shot_replays_json.unattached_hits[]`（对应射击未录入的命中事件：射手/受击者/时刻/伤害/结果/cmpIndex/令牌 + segment/shell_id/armor_group/hit_triangle + 受击者位姿与炮塔角——射击复现所需最小集，射击方姿态非必要）。证据：14 场语料 573 发逐发对拍，27 发语义变化全部为增益（26 发他人射击由无目标→服务器真值，受击者距终点 ≤13m；1 发作者同 bug），8 条 unattached 显式输出，0 解析失败；5 条新单测。**消费方**：additive 缺省即旧行为；`hit_token/game_hit_result` 覆盖上升 → WotbTools pin bump 后重生成 AI 投影 golden。详见本页 §命中通知归属（2026-10-29）。 |
| v0.4.15 | **录像者初始世界位姿轻量能力（additive）**：新增第 5 个独立 WASM 入口 `parseAuthorInitialPosition(bytes)` → `{author_eid, clock, position}` 或 `null`。只读 meta + battle_results 解析作者身份，随后定向扫描 type=10（`eid==author_eid`、`payload>=48B`、`parent==0`、位置/时钟 finite），取 replay clock 最小的世界观测；**不构建 ReplayModel / PlaybackData / AiReviewFacet**，不读地图语义、不物化时间线。身份解析与 `ReplayModel::scan` 共用 comp `account_id` 精确匹配 → Type5 昵称回退（SSOT）；type=10 解码复用共享 `decode_st10_sample`（不新增 decoder）。输出原样 replay 世界坐标（`x/z` 水平、`y` 高程），NORTH/SOUTH/出生侧分类留给消费方。证据：真实夹具 `2.wotbreplay` 锁定 author_eid/clock/坐标并与同场 Type5 战斗车辆物化位姿跨源核对 <0.05m；去掉 battle_results 的副本（meta 昵称回退）仍返回同一位姿；全部夹具 finite 且 <0.1m；WASM shape / `null` / Result 边界 smoke。**兼容面**：`parseResult` 不读包流、输出零变化；Playback 切面全部夹具逐字节不变；AI 切面仅同刻事件顺序（既有 HashMap 顺序非确定）无语义差异。契约见 [author-initial-position.md](author-initial-position.md)。 |
| v0.4.14 | **血量链死亡证据与显示去重分离（v0.4.13 的复审缺陷）**：v0.4.13 把 `zero_hits`（method1 归零事件的凶手/死因登记）放在显示链相邻同值去重之后——终态快照（0/0xFFFD/0xFFFE/0xFFFF，隐藏段死亡后残骸重入）先到时，随后（或同钟）的 method1 归零被相邻去重吞掉，死亡归属退化 `0/255` 甚至消失（存在 prop1 时）。现改为**归零是否立死看上一条 method1 更新的值**（同值归零 = 同一次死亡的 overkill 重复，不重复立死），与夹在中间的快照无关；显示链行为与 v0.4.13 逐位一致，仅死亡归属修正。**消费方**：`death_t`/`killer_eid`/`cause` 对「隐藏段死亡 + 残骸重入」场景恢复 method1 归属；切面 v2 形状不变，AI 投影 golden 预期仅 provenance 变化。证据：WotbTools #767 复审 P2-2 反例 + 回归 `hp_series_zero_hit_survives_terminal_snapshot_adjacency`（双向 + overkill 重复归零仍单条；6 合成单测 + 13 样本探针 0 脱节 + clippy 零警告）。 |
| v0.4.13 | **血量链合并顺序修正（v0.4.12 的复审缺陷）**：v0.4.12 的实现先对 method1 做相邻同值去重、再把重入快照插回——快照夹在两条同值 method1 之间时（隐藏段死亡无 method1 归零 + 复活回满血再受击、值恰同受击前，复活语义可达），后一条真实受击被吞（`t` 之后 `hpAt` 读到旧值，伤害飘字同源受累）。现改为**全部血量证据（种子 + method1 + 重入快照）统一按时间合并（同钟快照在前 = 客户端物化先于属性更新）、最后做一次相邻同值去重**；`zero_hits` 登记移入最终去重（幸存归零才立死，逐位等价）。**消费方**：普通回放与 v0.4.12 输出逐位一致，仅复活/同值边界修正；切面 v2 形状不变，AI 投影 golden 预期仅 provenance 变化。证据：WotbTools #767 复审 P2 反例 + 回归 `hp_series_snapshot_between_equal_method1_values_keeps_latter`（5 合成单测 + 13 样本探针 0 脱节 + clippy 零警告）。 |
| v0.4.12 | **隐藏段（盲炮）掉血在二次点亮兑现（血量链修正）**：重入 Type5 物化快照的当前血量（`AoiPresence.hp_raw`，offset51 重广播语义）并入血量链 `hp_series`——Type4 隐藏段零更新（type=7/8/10 全断）、method1 只覆盖在场段，此前丢视野期间的掉血在二次点亮后链上无体现、直到下一次受击才被动修正（J20 实测 CN_LEVITHAN 在场末 1192 → 重入快照 348，缺 844 点）。现按在场段逐段在 `t_in` 落台阶，与客户端 `onEnterAoI` 物化时 HP 条直接跳到快照值同构；哨兵族钳 0 与 method1 显示口径同判，首段与 seed 同包同值去重还原（无隐藏段变化的既有回放零扰动），无锚点实体以首个带血量快照为链起点；凶手归属 fail-closed 不做（隐藏段无 method1 来源）。**消费方**：`vehicles[].hp` 在有隐藏段掉血的车辆上新增台阶点——WotbTools 伤害飘字由血量链差分推导（`buildTransientSources`），随 pin bump 自动在二次点亮显示该次掉血；切面 v2 形状不变，AI 投影 golden 预期仅 provenance 变化。证据与验证（4 合成单测 + 13 样本探针 1→0）见本文件 2026-10-26 条目与 [wotbtools-cross-reference.md](wotbtools-cross-reference.md) §五。合并顺序缺陷由 **v0.4.13** 修正。 |
| v0.4.10 | **车辆集合判据修正 + 装甲厚度序列化精度**：①`vehicles[]` 入选改为 **Type5 class=2 实体类型 ∧ 结算花名册联表成功 ∧ 有 type=10 位姿**，删除 `st10 ∧ prop2` 双流交集、20 采样阈值与 prop2 退化分支——修复炮塔全程不转的敌方车辆整条丢失（2026-10-19 训练房 SU-130PM 样本，709 条 type=10、0 条 prop2），同时排除相机/区域可破坏物/炮塔残骸与未进结算花名册的观察者/中途离开实体。影响面：`vehicles[]`、`visibility[]`、时间轴 `meta.t_start/samples/duration`、`shots[]` 联表兜底；切面 v2 形状不变，WotbTools 随 pin bump 直接受益。②`tank/{id}.json` 的 plate/track 厚度序列化从 0.1mm 改为 0.01mm，保留 6.35 等真实板值（旧口径生成 6.3，HE/判定少 0.05mm）；738 辆数据重生成、manifest 刷新，8 个字节变化的 tank JSON 已差分同步 COS（730 个按 sha256 跳过）。消费方随 pin bump 重生成 AI 投影 golden：车辆集合修复带来的字段值变化属预期。 |
| v0.4.9 | **血量终态分类字段（additive）**：`AiReviewFacet` 的 `damage`（method1）与 `health`（prop3）事件新增 `hp_state`（`current`/`zero`/`terminal`，**恒在场**）——血量 u16 的**唯一权威分类**（规则 = `hp_terminal_normalized`，int16 ≤ 0 全域；实测终态族 0xFFFF/0xFFFE/0xFFFD，禁按分布裁剪），消费方判死亡面读它、禁自行解释 `hp_raw`。动因：消费方 canonical 只认 {0x0000, 0xFFFD}，−1/−2 档的弹药架殉爆被判成存活（血量冻在最后正值、3D 装填条继续外播；实例 1910 局 Intotherainy `0xFFFE`）。**消费方适配**：字段 additive、旧产物缺省即旧行为；**要覆盖 −1/−2 档死亡必须消费 `hp_state`**（缺省路径保持 legacy 分类——仅 {0x0000, 0xFFFD} 终态，禁在缺省上放宽）；pin bump 后 AI 投影 golden 预期仅 provenance 变化（新字段不进投影产物）。`PlaybackData` v2 / `AiReviewFacet` v1 形状不变。裁决与迁移清单见 [wotbtools-cross-reference.md](wotbtools-cross-reference.md) §七。 |
| v0.4.8 | **复活模式生命轨道（additive）+ method8 结算伤害直读 + 伤害归属确定性化 + 争霸残留占领清理**：①`VehicleTrack` 新增 `death_times`（**≥2 死才输出**）与 `resurrection_times`（**非空才输出**）——复活模式（爆燃等）同 eid 死而复生不再被判终态；普通回放 JSON 零扰动，消费方按"字段缺省 = 死亡终态"处理即行为逐位不变。②`shots[]` 伤害改**直读服务器结算值**（method8 直击通知尾部 `[u16 伤害][u8 状态]`；状态 ≠0 才已结算、overkill 已封顶），作者/他人两路径同优先，HP 链降级为无结算值时的兜底；22 场对账仅 3 处变化且全部为修正。③**伤害归属确定性化**：逐事件采样（删除"同钟异值整组丢弃"）+ method8 命中时钟精确配对，配不上 → 0 + `dmg_unattributed`（fail-closed）；22 场仅 5 发变化、全部对账。④已归属基地的占领中断后**清空残留进度**（保留归属；新增合成回归与公开契约）。⑤clippy `-D warnings` 清零（纯机械等价替换）。**消费方注意**：`shots[]` 的数值/归属与 HP 链兜底路径有可见变化（皆 bug 修正），WotbTools 随 pin bump 须重生成 golden 并逐条归因（复活字段则无需适配，缺省即旧行为）。切面 v2 形状未变。**附带（工具/资产面，非 WASM 行为）**：2D 战术底图改 pack `ground.webp` 派生 + COS `map2d/` 免发版通道（索引 schemaVersion 2 带世界框/取向几何契约）+ 客户端 HUD 小地图取景实测记录。 |
| v0.4.7 | **独立炮塔回放轨道（additive）**：`detached_turrets`——class-8 初始化关联车辆，输出 AoI 区间与 raw 三轴关键帧（同刻移动收敛为最后接收帧，未知布局/身份冲突/非有限位姿 fail-closed）。附带：共享 st10/prop2 排序统一 `total_cmp`（NaN clock 不再 panic）；`packet_histogram` 键口径订正为包类型（仓内无消费方）。切面 v2 不变，消费方零适配。 |
| v0.4.6 | **动态装甲状态载体重定位（数值语义订正）+ 穿深距离衰减 + 顶层弹表单权威化**：①`armorsStates` 真实载体 = **propID 16**（载荷 `[u8 n][n×u32 描述符]`，bit16=激活态；初态经 type=5 create 表物化）——字段名与形状不变，`armor_states` / `shots[].target_armor_state` 对动态车自本版起可信；②`PenetrationRequest` additive `distance_m`/`penetration_far`，d>100m 线性衰减到 far（HE 豁免；对齐客户端 0x719740）；③`tank/{id}.json` 顶层 `shells` 改为直接引用末档配置弹表同一份 JSON（结构性单源化，杜绝重抄漏字段）。数据面 735 辆重生成随包同步；WotbTools 随 pin bump 重生成 golden（仅 provenance 变化）。 |
| v0.4.5 | **动态装甲状态时间轴（additive）**：`PlaybackData.armor_states` / `AiReviewFacet.armor_states`（列式轨道，空则整字段省略）+ `shots[].target_armor_state`（弹着时刻状态号，skip-when-none）；数据面 `data/armor_states.json`（6 车 12 态，`<armorsStatesController>` 声明 = 状态语义权威源）。**数值语义以 v0.4.6 为准**（本版载体判定已被证伪）。 |
| v0.4.4 | **作者车接收突发时间轴重建（解析器）+ 履带印数据面（tread/）**：①包头 clock 是录制端到达时间，网络突发把同车多个 type=10 压成同一时间戳——现对**作者车**按标称 10 Hz 重建时间轴（`filter::respace_same_clock_bursts`，越界回填整段放弃），**远程车保持原样**；消费方零适配（仅作者车位姿数值更平滑，golden 若有对应差异属预期）。②包内 `tread/`：全车产 `tread/<id>.json`（authored 参数或国别主贴图回落）+ 13 张贴图 + `textures.json` alpha 归一化表。 |
| v0.4.3 | **弹种识别补齐 + 顶层弹表判定输入（数据面）**：①Type5 配件串改**变长解析**（`0B <n>`+nB，n=1..=9，空槽省略）——修复不满 9 件玩家 loadout 整条被丢弃；②**弹种指纹回填**（`method29 args[8]` 弹种编码，同场同射手自校准、弹速去重）——`cmpIndex=0` 命中游戏不发 26/27B 段包时回填 `shell_id`；`ShotQuality` additive 新增 `shell_from_launch_code`/`shell_from_velocity`；③顶层弹表补 `caliber`/`normalization`/`ricochet`（2079/2079），车级 `caliber` 改取顶级炮弹径。切面契约未变。 |
| v0.4.2 | **无炮塔流回放放行 + 光照/IBL 资产**：①`prop2`（type=7 炮塔流）全局缺失时车辆筛选退化为仅 type=10（修复此类回放整场拒开；仍 fail-closed）；②资产面：逐图 `lighting.json`（太阳方向/色/强度、环境色、雾原始块）与 `ibl.webp`（逐图等距柱状环境反射）随包；extras 贴图解（光照图/贴花/水面/细节等）随后续版本持续收编。切面契约未变。 |
| v0.4.1 | **数据面与资产管线修正 + 弹种权威化**：地图几何改文件内 authored 法线；PVR3 单通道 L8/A8 解码补全；`PenetrationRequest` additive `shell_type_id`、`tank/{id}.json` additive `type_id`——弹种判定改走 field9（客户端 `<kind>` 枚举，4 值闭集），icon 词表降级为回退；DVPL 解压分配上限 512 MiB。资产面随全量重导同步 COS。切面契约未变。 |
| v0.4.0 | **回放解析确定性收口（语义/契约变更，消费方需 v0.4.0 适配——WotbTools PR #555 已同步）**：`damage_received` 无证据 = `null`（≠0）；击杀原因未知 = 255 哨兵；`infer_shots` 推断路径删除；`shots_from_loose_path` additive provenance；ARENA_INFO comp blob 全玩家提取（`turret_local`/`gun_local` 覆盖率 1/N → N/N）；遗留 #7–#12 清零（HP seed 分级、昵称窗、白名单 masked 匹配、令牌优先等）；type=28 槽位兜底 fail-closed。 |
| v0.3.15 | **实际搭载配置透出 + AoI 重入段化滤波**：`vehicles[]` additive `config_idx`（comp blob 证据链钉定的配置下标）/`burst_size`（弹容原值——**装填条弹容 N 的唯一权威 = 实际搭载配置**，禁跨配置取最大/剩余弹数推断）/`turret_local`+`gun_local`；位置滤波器按 AoI 在场段独立实例化（客户端 `onEnterAoI → setFilterOnEntity()` 同构），`visibility[]` additive `pose`（段首物化快照种子）⇒ 重入车辆 `pos/hull_yaw/pose_kf` 与炮口锚点数值修正，消费端零改动。 |
| v0.3.14 | type=5 昵称去掉 30 字节上限（长度域放宽到 u8 全域 `1..=255`，保留 len==0 拒绝 + UTF-8/控制字符校验）——修超长全角昵称导致阵营/车型联表失败。纯解码修复。 |
| v0.3.13 | **渲染位姿关键帧折线 + 弹道折线**：`vehicles[].pose_kf`（additive；贪心走廊拟合，消费端线性插值即复现客户端滤波器画面，修 0.1s 网格混叠顿挫）；`shots[].via`/`leg_secs`（跳弹/穿透续段折线，`flight_secs`/`end_time` 与之同刻闭合，归属 = (shooter, shotId) 链 + method20 配对，无相关性匹配）。 |
| v0.3.12 | **可破坏地形事件切面**：`destructible_areas`（区域实体锚点）与 `destructible_events`（时刻/类别/槽位/倒向；槽位与区域格子联表 = 唯一物体寻址）；配套资产管线（36 图清单 + lka serverId、scenery GLB 的 D_ 损毁态网格）；场景变体组打标（`mdVariant`/`variantByMapId`，消费端按对局 map_id 剔除非本组节点）。 |
| v0.3.11 | 基地占领中断归零（proto3 零值省略判据）+ 单基地存在性补正（目标族出现即真）；删除 `aim_frames`（零消费方，占回放 JSON 57.6%；原始 type39 帧保留给射击复现，`PlaybackData.version` 保持 2）；`vehicles[].hull_roll`（additive）。 |
| v0.3.10 | 射击复现多 interaction 关联修复：`unique shotId = 一次开火 = 一个 Shot`；优先 `method8.hash6 ↔ type32.hash6` 令牌关联，证据不足 fail-fast。 |
| v0.3.9 | 装填数据补齐：`reloads` 相位语义定稿（m0x30 subtype 15/16/17）+ additive `reload_effective`（0x23 = 当前生效完整装填配置时长）。 |
| v0.3.8 | `roster_complete`（结算阵容与战绩账号集合一致性门禁）与 `author_vehicle_codename`。 |
| v0.3.7 | 原始未滤波位姿（type=10 `poses`）与炮塔观测（prop2 `turrets`）——切面 0.1s 网格是渲染滤波输出，不能当位置证据。 |
| v0.3.6 | prop3 血量属性广播（录像者自身血量的唯一来源，与 method1 非镜像）。 |
| v0.3.5 | 原始 HP 证据 `Damage.hp_raw`（未钳制 u16，区分零血与终态未知）与 AoI 开段物化 HP；method8 原始命中通知（全变体、不分类）。 |
| v0.3.4 | 包流自行分帧（不再依赖 crate 对 payload 的反序列化——单个 pickle 形状偏差不再让整场失败）。 |

### 资产原发布补齐：连发组 DPM + 口径权威的 tank/*.json 上线（2026-11-04，数据面发布勘误）

**问题（修了但没上线）**：2026-10-28 连发组（PomPom）DPM 修复与 2026-10-27 口径权威修正
都已合入本仓并随 v0.4.16 发布，但**阿里资产原从未收到对应 `tank/*.json`**——origin 各代
release 全部是硬链接增量发布（research / destructibles / map2d / cover），10-28 之后没有
任何一代覆盖过 tank 文件，线上 tank 字节停留在 2026-10-21 时代（manifest
`upstream_commit 4bfccf7c`，PR #27）。前端 `characteristics.ts` 的整匣循环公式在数据缺
`burst_group_*` 字段时按契约退化为旧式，百科 DPM 因此一直显示旧值（AC Atlas V2 1573 而非
2623）；口径也停留在旧 `parse_gun_caliber` 时代（AT 7 顶级 20-pdr 等显示 120 回退值）。

**发布**：`tank-burst-dpm-20261104`（基于 `2d-basemap-v3-20261101` 硬链接复制 + **226 件
`tank/*.json` 断链覆盖**，临时文件 + `os.replace` 换 inode）。226 件差异全量 deep-diff 定
性：36 件 `burst_group_count`/`burst_group_interval`（连发组时序字段）+ 207 件 `caliber`
（弹径权威修正，47b980b 删除炮名解析与 120 回退），configs 外零差异。`data/*`、
`tank_images/crops.json`、预览车资产**不动**——origin 的 t11 预览合并层（740 辆）只有
origin 侧带预览键（BZ-79 / Object 432U 均非弹夹炮，is_burst=false，无需 burst 字段），
本地纯 11.21 包不含这些文件，从本地覆盖即预览数据回退。manifest 重算按发布程序保留
`upstream_commit` / `data_version` / `worktree_dirty`，刷新 `generated=
2026-11-04T15:39:34+00:00`；5978 件 / 逻辑 4238.5 MB（manifest 逐文件字节和口径；历史记录
的 4444.3 MB 为 du 块分配口径，不可直比），0 mismatch；226 件字节增量 +0.01 MB。

**验证**：公网回拉 `tank/19825.json` / `10049` / `10001` / `10625` sha256 三方一致
（served = 新 manifest = 本地包）；AC Atlas cfg1 `burst_group_count=3` /
`burst_group_interval=0.25` 已上线；回滚版本 `2d-basemap-v3-20261101` 的 manifest sha256
与 226 个被覆盖文件逐字节未变（断链正确）；WotbTools `deploy/assets/verify.sh
https://assets.wotbtools.com` PASS。前端已上线无需再动——百科 DPM / 口径显示随数据即改。
发布链细节见 [game-data-sources.md](game-data-sources.md) §5.2。

### 炮管俯仰接入装备 122（改进型垂直稳定器，2026-11-01，解析器行为修正 + WotbTools 联动）

**改动**：`decode_prop2_gun_pitch` 的输入从 models.pb 原始 `GunPitchRange` 改为**按实体实际配件**扩展后的有效极限——Type5 `0B` 配件串含 **122 `improvedPitchLimitsSystem`** 时，仰角上限 +4°、俯角下限 +3°（基础值与 front/back 扇区极值同增益，`transition` 不变）。覆盖 3D 回放（`vehicles[].gun_pitch`）、射击复现（`shooter_gun_pitch` / `target_gun_pitch` / 两侧 `gun_timeline`）以及服务端 `/api/playback/data`、`/api/replay/shots`、CLI `combat`、WASM `parsePlayback` / `parseShotReplays`。`VehicleEquipment` additive 新字段 `improved_pitch_limits`（仅 true 时序列化），供 WotbTools 装甲查看器自动勾选；`tank/{id}.json` / `tank_cache.json` 的 `pitch_limits` / `gun_depression` / `gun_elevation` 仍是基础值。

**证据**：`data/research/devices.json` id=122 `bonusValues.upperPitchLimitIncrease=4` / `lowerPitchLimitIncrease=3`；可用性 = 每车 `optDevicePreset`（当前 11 辆 `AlternatePitchGrouserPreset` + Rinoceronte 的 `DrumGunPitchLimitsPreset`）；回放证据 = 同 eid 的 Type5 `0B` 配件串（103/110 同源机制）。`crates/replay-core/tests/pitch_device_data.rs` 对该数据做锚定，并断言**只有 122** 声明俯仰增量——新增第二件俯仰配件时测试失败，强制显式接入。

**影响面**：只有实际装 122 的车辆/玩家输出变化（极限端点精确 +4°/+3°，中间 frac 线性变化）；无 122 的场次逐位不变。`limitsJson` 契约明确为**基础**极限，core 负责叠加设备加成（WotbTools 不应在 `pitchLimits.js` / `ReplayShotsPane` 预加，否则双重计算）。WotbTools 侧装甲查看器新增 122 勾选（目标/被展示车，默认关、射击复现自动勾选），手动拖炮按扩展后极限钳制；需 pin bump + `wotbVersion` bump。



- **性能（输出逐位不变）**：①新增 `extract_shot_bundle_auto_with_limits`——作者严格/他人宽松/未归属命中**一次共享扫描**三路复用，`parseShotReplays` 的包流扫描 3 次 → 1 次（CLI diagnose 2 → 1）；②`CombatTimeline::from_parts` 复用共享扫描的名字/满血锚点产物，`ReplayModel::scan` 少 3 次全包流遍历（AoI 在场段同源复用）；③`ReplayParser::parse_file` 单次读盘（此前 File::open 二次读盘，批量扫描逐文件 IO 翻倍）；④`anchors.rs` 段定位与侧倾插值改 `partition_point` 二分（原线性扫描在 ≈80k 次调用热路径上）；⑤`ShellKindTable::shared()` 进程级缓存（此前 web 端点/工具逐调用全量展开 tanks.pb）。
- **fail-visible（错误面变化，正常数据输出不变）**：①他人宽松路径 `Result` 化——**数据**缺失仍跳过计数（`skipped_*` 契约不变），仅**内部不变量破坏**（非续段 + 有终点的发射缺少命中通知窗口；构建/消费两侧由 `build_notice_windows`/`shot_segments` 保证不可达）上抛 Err，不再用编造兜底窗静默错配归属；②`parseShotReplays` 输出 `others?` / `others_error?` 互斥键（与 `author_path="error"` 同款 fail-visible 形状）；③`limitsJson` / `shellsJson` **非空但非法**的注入 JSON → 整体 Err（此前静默当空表会让俯仰/弹种无声降级且无逐发标记可查；`tankNamesJson` 是展示串，仍按未注入处理）；④`reconstruct_supremacy_base_states` 对 `base_index > 3` 的直构输入 panic（wire 收集器已过滤，静默 continue 会让基地时间线缺段无痕迹）。
- **消费方**：WotbTools 消费 `parseShotReplays` 时先判 `others_error` 再读 `others`；注入表组装处无需变化（合法 JSON 行为不变）。契约细节见 [回放与射击逆向总集](回放与射击逆向总集.md) §八与 WotbTools `contracts/agent/replay-facets-v2.md` §5。

### 补给线码受控实验纠正 + 系别族字段 family（2026-10-31，fail-closed 修正 + additive）

- **动因（受控实验，2026-10-31 本地客户端）**：用户亲装**大补给**的 Kranvagn 回放线码为
  `0x49`（v0.4.18 表标其为瑞典小补给——错）；亲装**茹尔汤**的 60TP 回放线码为 `0x6D`、
  亲装**改进型齿轮油**为 `0x6B`（后者与既有表一致，互证）；亲装**大口粮**的 Vz.55 回放
  线码为 `0x4A`（钉死捷克大）；AC Atlas 同场**亲装大+小口粮** → `0x2D`/`0x2E`（其他系
  大小次序直接钉死）。装填时间推算（弹夹炮 + 瞄准延迟混杂）不可作为补给大小判据，弃用。
- **结构性发现：装载可同时携带同族大+小两个食物**（受控 Atlas 双口粮；同场 Maus
  `0x0E+0x15`、LTG `0x10+0x17`、Bulldog `0x0F+0x16` 为双食物自然样本）——WotbTools
  11.18/11.20 语料"53TP 同时带 `0x6D+0x6E`"并非异常。**消费方 HUD/检视不得假设每车
  至多一个食物。**
- **根因**：v0.4.18 把「`provisions.pb` id 103..110 与线码 `0x46..0x4D` 逐项相差 33」当
  格式规律外推——它只是 103/104 → `0x46/0x47` 两个观测点上的巧合归纳。线码是游戏内部
  补给槽 id 的**自有命名空间**（WotbTools 11.19 文档原话："The replay wireCode is **not**
  assumed to equal BlitzKit/public provision source ID"）：瑞典对 HQ/regular 顺序与欧系对
  **相反**，波兰 `0x6D/0x6E` 恰等于目录 id 109/110 的十进制原值。**每条线码独立取证，
  禁止按数值邻接外推**；WotbTools 11.19 生产映射（`0x48`=SMALL/`0x49`=LARGE）本来就是
  对的，v0.4.18 的"纠正"才是错的。
- **修正**（`provision_logical_id`）：`0x48`=瑞典小(Crispbread)、`0x49`=瑞典大(Grilled
  Salmon)（受控实验 + WotbTools 11.19 结算体观测双证）；`0x4A`=捷克大(Goulash 107，
  **受控钉死**——v0.4.18 的 LARGE 标签结论碰巧正确但证据错误，现改由受控实验支撑)；
  `0x4B`=捷克小(Fried Cheese 108，`0x4A` 钉死后按成对 + 24 槽 24 件计数闭合消去——至此
  **33 件 common 补给 ↔ 33 线码槽全绑定**)；撤销 `0x4C/0x4D`（从未自然观测，`None`）；
  新增 `0x6D`=波兰大(Zurek 109，受控 + 11.18/11.20 自然观测)、`0x6E`=波兰小(Faworki 110，
  与 `0x6D` 成对消去)。
- **additive**：`LoadoutItem.family`（`Option<String>`）+ `provision_family()`——食物族
  系别（germany/usa/ussr/uk/japan/china/france/european/swedish/czech/polish/other），
  通用给养（燃料/防护包/沙袋/内衬/齿轮油）为 `None`；消耗品/模式能力恒 `None`。系别证据
  = 车辆可用性消去（线码绑定唯一目录条目，观测到其出现在仅可用单一食物族的坦克上）。
- **动因（图标错系）**：此前切面只导出 `LARGE_FOOD`/`SMALL_FOOD`，WotbTools 图标映射只能
  兜底"其他系"图标——Kranvagn（瑞典）显示 Hybrid 图标即此根因；`family` 落地后消费方按
  `(family, logical_id)` 选系别正确图标。
- **证据**：16 场语料扫描（14 场库存 + 2 场 Kranvagn 对局）线码↔坦克系别全闭合（`0x4A`
  在 Vz.55 = 捷克可用性消去）；受控回放 2 场端到端复验（Kranvagn `0x49`→LARGE/swedish、
  60TP `0x6D`→LARGE/polish）；Type32 全消耗品线码 0 未知。单测：受控实验回归
  `controlled_food_experiments_export_family` + 旧断言块重写。
- **全量可用性审计（2026-10-31 补，受控实验后复验）**：客户端目录 common 可装配补给恰
  **33 件**（食物 24 + 通用 9；另有 1 件 common+ability `desertPower` 与 51 件特殊模式件
  不占给养槽）↔ 线码槽 33（食物 24 + 通用 9）**计数闭合；受控实验闭合 `0x4A/0x4B` 后
  33/33 全绑定**。tankId→stem→目录 filter 可用性模拟器对语料 **151 组 (坦克, 线码) 观测
  逐条交叉验证，0 违规**：`0x32`=防崩内衬三处观测（Rhm.-B. WT / FV4005 / Grille 15）全在
  目录 include 敞篷 TD 列表内；七国块 14 槽中 13 槽按本国坦克直接观测（唯 `0x1B` 未现）；
  `0x44/0x45` Maus HP ×1.03/×1.06、`0x6C` VK72.01 弹速 ×1.35 行为闭合维持成立；`0x6A`=
  齿轮油由三槽三件消去（`0x6B` 受控、`0x6C` 行为已钉）。未观测槽位
  `0x1B/0x48/0x4B/0x6A/0x6C/0x6E` 各有独立证据（见下条未决）。
- **消费方（WotbTools）**：pin bump 后 ① `itemCodes.ts` 回退表同步 `0x48`=SMALL/
  `0x49`=LARGE（其现表已随 v0.4.19 反标，需换回）、删 `0x4A..0x4D` 食物标签、补
  `0x6D/0x6E`（其 2026-10-30 提交已含）；② 图标/名称按新 `family` 字段（或等价本地表）
  选系别；旧字段 `loadout_items`/`equipment` 逐字节不变，`family` 缺省（旧产物）走
  unknown 系别兜底不变。
- **未决（全部为低风险补对/补位推断，绑定均已建立）**：① `0x48` 族=瑞典是**补对推断**
  （`0x49` 受控已证瑞典大，欧/瑞/捷区域按 (0x46,47)(0x48,49)(0x4A,4B) 成对分配；同机制
  曾在波兰上被证伪——瑞典车亲装脆饼一场可转直接证据）；② `0x1B`（法国小）为七国块
  13/14 直接观测后的补位——法国车亲装肉罐头一场可闭合；③ `0x6E`（波兰小）为与 `0x6D`
  成对消去、`0x6A`（齿轮油）为三槽三件消去——60TP 一场（齿轮油+丝带脆饼）可双双转
  直接证据；④ `0x4B`（捷克小）为成对 + 计数闭合消去——Vz.55 亲装油炸奶酪一场可闭合。
  `0x4C/0x4D` 及其余未观测线码继续 fail-closed。

### 全车辆 type=10 + prop2 接收突发时间轴重建（2026-10-31，解析器行为修正）

- **动因（真实样本，用户报障"3D 回放一段时间明显卡顿 / 作者车炮塔一下子跳变"）**：
  `20261002_2147` 神了 Kranvagn 场 90.254→94.058s **全场网络停摆 3.80s**（期间仅
  type=39 心跳按 ~30 包/0.25s 继续到达，连接未断），补发把每车 25~38 个 type=10、
  21~36 个 prop2 压成 clock=94.058（包头 clock = 录制端到达时间，见总集 §2.1）。
  v0.4.4 只重建作者车 type=10 ⇒ ①作者车炮塔在 t≈94.2 单格跳 **57.07°**（prop2 35 连包
  跨 −52.0°→+4.9°，`prop2_at` 同刻括号下中间帧不可达）；②远程车整场冻结后于
  t≈94.7~94.8 猛冲（pose_kf 100~414 m/s、0.1s 网格步进最大 27m）。
- **行为**：`respace_same_clock_bursts`（标称 10Hz 回填 + fail-closed 护栏，规则不变）
  从"仅作者车 type=10"扩展到**全部车辆 type=10 + prop2**（新增
  `filter::respace_same_clock_bursts_prop2`，二者共享回填核心）。v0.4.4 的"远程车保持
  原样 = 复现录制端观感"口径废止：同 clock 连包是到达时间打戳的传输伪影，与车辆是谁
  无关，重建目标 = 服务器权威 10Hz 快照流的原始时序（同类数据统一施用）。
- **效果（夹具实测）**：突发窗 [91, 95.5] 全车队位姿步进 ≤1.02 m/0.1s（修复前最大
  27m）、全车 pose_kf 0 个超 30 m/s 折段（修复前 100~414 m/s）；作者车炮塔全程
  ≤2.35°/0.1s（修复前 57.07°）。**残余诚实空洞**：补发包数少于停摆槽位时（prop2 变化
  驱动广播），未覆盖段在 `prop2_at` 括号语义下渲染为"保持→一步追上空洞期扫过角"
  （实测最大 晓孩哥 11.12°/0.1s，1.8s 空洞扫 9.5°）——缺失数据的诚实表现而非回归；
  把扫描摊回空洞期会引入命中后反应污染（prop2_at 设计红线），不做。
- **边界**：判定锚点（`prop2_at_arrived`、射击提取 render_cache）不走重建副本，保持
  原始到达时序；单发复现滑块的炮塔/俯仰时间线（`timeline_prop2_client`）**仍走原始流**
  ——滑块窗口窄（−3~+2s）且与弹道炮口锚点同图消费，半套重排会造成滑块姿态与锚点
  不一致，未见报障暂不动（潜在同型伪影，已记录）；AoI 重入瞬移（隐藏期跨段跳变，
  如 MisakuraRin t=193.5 跨 117m）不在本修复范围（前端按 visibility 隐藏，pop-in 非卡顿）。
- **回归**：`burst_respacing_covers_all_vehicles_and_prop2`（新夹具
  `data/replay_samples/20261002_2147__shenle_S16_Kranvagn_15612263746483708.wotbreplay`，
  下载原名 `20261002_2147___神了_S16_Kranvagn_…`：证据锁 + 作者车全程位姿/炮塔步进上限
  + 全车队突发窗步进上限）+ `respace_same_clock_bursts_prop2` 单测 ×2。
  **消费方零适配**：`PlaybackData` 契约不变，仅位姿/炮塔数值在含突发的场次更平滑
  （golden 若有对应差异属预期）。

### 命中通知归属：时钟容差 → 弹着区间顺序（2026-10-29，解析器行为修正，v0.4.16）

- **动因（真实样本）**：20261029 样本 HE 一发（shotId 30958325，182.03s，目标 Grille 15，
  服务器结算 287）本地射击复现与服务器判定不一致。根因 = 该发 method8/type=32 命中通知
  比 method20 终点包**晚 +0.0811s**（一个 10Hz tick），落在旧配对窗 `|t−end_time| ≤ 0.05s`
  之外 → `hit_token/segment/armor_group/game_hit_result` 全部置空（255/null），前端退回
  弹道弦判定；弦在 10mm 前甲板上入射 82.12°（等效 73.0mm，HE 75/校准 80.25mm 判穿），
  服务器自己编码的 P1→P2 段是 83.76°（等效 92.0mm，不穿 → 287 溅射）。服务器 segment B7=5
  与本机 raycast 板号一致，证明板对、角度差才是否决点。
- **修正**：命中通知（method8 / type=32）不再用时钟窗，改用**同一射手 `[本发弹着, 下一发
  弹着)` 区间**归属（`ShotScanShared::notice_windows`；弹着时刻 = `shot_impact_time`，
  有续段取首段接触）。纯顺序语义、零容差；通知不再可能"因为晚一个 tick 被判成服务器未转发"。
- **令牌职责收窄**：hash6 只做 method8 ↔ type=32 的**同事件令牌**（事件内唯一；同刻重发按
  hash6 合并、保留 settled）；配对不再依赖时间。type=32 未转发 / cmpIndex=0（不发本族包）
  → 由 method8 兜底（**不**把区间内别的 segment 当歧义）。
- **多 target 保留（一发多 victim）**：`ShotReplayData` additive `hits[]`（`ShotHit`：
  `target_eid/target_name/damage/game_hit_result/is_primary/armor_group/hit_token/
  server_part_index/crit_modules/destroyed_modules`）。HE 溅射同时命中多辆、或一发在同一
  受击者上多次装甲交互（hash6 不同）都各成一条；主目标（= 顶层 `target_*` 字段）排在首位
  并继承顶层伤害/结果。他人路径原先把"不在发射射线上"的溅射第二目标**排除**，现保留。
  **额外受击者必须与主事件同一服务器批次（同一包钟）**——晚于主事件的通知属于未录入的
  另一发（AoI），贪心收进会错绑（语料实测 4 例，被误收的"第二目标"距终点 60~440m）；
  该门用批次时间而非距离阈值。
- **不可归属事件不丢**：`shot_replays_json` 新增 additive `unattached_hits[]`
  （`UnattachedHit`：`shooter_eid`/`shooter_name`/victim/昵称/时刻/结算伤害/结果枚举/
  cmpIndex/令牌 + 同事件 type=32 的 `segment`/`shell_id`/`armor_group`/`hit_triangle` +
  受击者事件时刻位姿 `victim_pos`/`victim_ang`/`victim_turret_yaw`/`victim_prop2`；
  `reason="no_shot_interval"`）——通知落在该射手所有区间之外 = 对应射击不在录像里
  （AoI 截断），显式输出而非丢弃；消费方按"未知归属"呈现，不得反推成某一发。
  **射击复现所需的最小集已齐**：受击者位姿（含炮塔角）+ hash6 命中段 + 弹种 → 可直接跑
  segRay 判定/渲染；缺的只有发射点/速度/开火时刻/shotId（弹道线与交战距离），按 2026-10-29
  用户裁定：表现受击方姿态与命中位置即可，射击方姿态不是必要条件。
- **多交互主段选择**：区间内多条 method8/type=32（一次开火多次装甲交互）按「已结算 →
  伤害大 → 时刻早」确定主交互（与 `shotResultClass` 的伤害仲裁同义），不再整发 fail-fast。
- **证据（14 场语料全量回归，作者+他人 573 发逐发对拍）**：作者路径 54 发有目标 → 52 发区间内
  找到通知、2 发无通知（脱靶，正常）、0 错配；全语料 457 条 method8（同刻重发合并后 442 条）
  中 434 条按区间归属，8 条无对应射击（`unattached_hits` / `no_shot_interval`）。相对旧实现
  **27 发语义变化，全部为增益**：26 发他人射击由"无目标/0 伤/255"变为服务器真值（新绑受击者
  到终点距离 ≤13m，其中 20 发 ≤5m，几何互证），1 发作者（J39 shot 7223366：255/null →
  token/segment/result，同一 bug 的第二例）；无解析失败、无新增错配。`hits[]` 覆盖 425 发；
  语料无"同批次多目标"真实样本（4 例贪心误收已由批次门排除）。Rust 侧新增 5 条合成单测
  （区间序、晚一个 tick 绑定、同批次多 victim、晚到外部通知不误绑、同刻重发合并）。
- **消费方**：`hits[]` / `unattached_hits` additive（缺省 = 旧行为）；`hit_token`/`game_hit_result`
  覆盖上升（同一发的值可能从 255 变为服务器真值）→ WotbTools 随 pin bump 重生成 AI 投影
  golden 并逐条归因。切面 v2 形状不变。

### 回放装载解码补齐：消耗品 / 给养 / 模式能力 / 配件（2026-10-30，additive）

- **动因**：Type5 尾部 loadout 在本仓一直只透出 raw `items`，`VehicleEquipment` 只识别
  103 校准弹 / 110 强化装甲；11.20+ GravityMode 还会在 3 消耗品与 3 给养之间插入一条
  模式额外能力，形成 **7 条描述符**（旧 3+3 位置假设会把能力误判为第 4 个给养）。
- **修正**：`collect_vehicle_loadout` 现按格式不变量解码——前 3 条 = 消耗品槽（位置契约），
  其余条目带 `payload f32 -1.0` 静态尾 = 给养，无静态尾 = **模式额外能力**（单列，不混入
  给养）；同时补齐 `wireCode` 逻辑表（WotbTools 11.19 生产映射 + 本仓 11.20/11.21 样本 +
  客户端坦克百科目录：`0x13/0x14` 中国/法国大补给、`0x15` 德国小补给、`0x1A/0x1B` 中国/法国
  小补给、`0x2D/0x2E` 其他系大/小、`0x32` 防破片内衬；模式能力 `0x67` = `gravityJump`/
  Thruster；消耗品 `0xA9` = `DYNAMIC_ARMOUR_SYSTEM`/armorMover）。~~当时把欧系/瑞典/捷克/
  波兰食物块按「id 103..110 逐项 −33」闭合，并称其纠正了 WotbTools `0x48/0x49`~~——该
  外推**已被 2026-10-31 受控实验证伪并撤销**（`0x49`=瑞典大、`0x4A..0x4D` 大小标签全部
  撤销、波兰实为 `0x6D/0x6E`），见上一节。当前语料的所有观测 loadout 线码均已闭合；
  未观测的新线码仍保持 `None` + raw。
- **对外形状（additive）**：`vehicles[].loadout`（`VehicleLoadout`：equipment/equipment_ids/
  items/consumables/abilities/provisions）；`consumables[].logical_item_id`；
  `shooter_equipment`/`target_equipment` 的 `equipment_count`/`equipment_ids`。旧字段
  `loadout_items`/`equipment` 保持不变（缺省/旧消费方零适配）。CLI `wotb-agent loadout`
  同步打印实装清单。
- **证据**：14 个 11.20/11.21 真实样本（J39/J20/XM551 GravityMode 7 条、T-100 LT 6 条、
  GB109/GB48 等 3/4/5 条真实玩家、Titan Strife `0x2D/0x2E`、Grille 15/Rhm `0x32`）；
  合成块 + 真实块回归见 `collect.rs::loadout_tests`（7 条能力分流、给养静态尾、未闭合码
  raw、短装备串线序）。
- **消费方**：WotbTools 当前 `toLoadout` 要求 `loadout_items.length === 6`，7 条布局会被
  整条丢弃；pin bump 后应改读 `vehicles[].loadout`（或放宽到按静态尾分流）。其
  `frontend/src/replay-local/playback/itemCodes.ts` 线码纠正以 2026-10-31 受控实验节
  为准（`0x48`=SMALL/`0x49`=LARGE、`0x4A..0x4D` 撤销食物标签、`0x6D/0x6E`=波兰、
  `0x67`=GRAVITY_JUMP、`0xA9`=DYNAMIC_ARMOUR_SYSTEM）。旧 6 条样本的
  `loadout_items`/`equipment` 逐字节不变。
- **未决**：仅描述符 12B 内部计时/动态字段（物品身份已全部闭合）；未在语料中自然出现的
  未来线码仍按 fail-closed 保留 raw + `None`。

### 消耗品状态与剩余时间（2026-10-30，additive）

- **状态**：`ConsumableTransition.state_name`（`INITIALIZED` 可用/未使用、`ACTIVATED` 激活、
  `ACTIVE_ENDED_OR_COOLDOWN` 冷却、`TEARDOWN` 拆除、`UNKNOWN`），与 WotbTools canonical
  名称逐字一致。
- **剩余时间**：state 2/3 的 `duration_s` = param（激活持续 / 冷却秒），`ends_at` =
  `clock + param`；播放时刻 t 剩余 = `clamp(ends_at − t, 0, duration_s)`（Rust 辅助
  `remaining_s_at(t)`）。state 1/255 无倒计时。**派生值以服务器后续 state3/state1 为准**
  （实测 state3 可比 state2+param 早约一个包钟；复活/重置时 state1 可提前）。
- **未使用 vs 已就绪**：两者 raw state 都是 1；`ever_activated` 标记该 (eid, wire) 此前是否
  激活过。`slot` = loadout 消耗品槽位 0..2（模式能力/未知线码 None）。
- **给养注册透传**：Type32 flag=0 流同时携带给养注册/拆除（state1/255、param=-1）；
  原样透传，消费方按 `slot`（给养无 loadout 消耗品槽位）过滤，HUD 不会把给养当消耗品。
- **证据**：J39 实测肾上腺素 state2 53.55（dur20）→ state3 73.46（cd68.376）→ state1
  141.85；修理箱瞬发 state2/3 65.49（cd64.103）→ state1 129.52；GravityJump 65.82
  （cd21.368）→ state1 87.19；均闭合到包钟精度。合成测试覆盖定时/瞬发/复活提前
  state1/teardown/未知 state。
- **消费方**：`consumables[]` additive（旧字段 `state`/`param`/`logical_item_id` 不变）；
  WotbTools canonical `ConsumableTransition` 目前只带 state，需在 pin bump 时携带
  `endsAt`/`duration`/`everActivated`/`slot`，`consumableRuntimeSlotsAt` 才能显示倒计时。
  给养事件从消耗品列表移除后，原先 null 身份的噪声消失（无有效状态语义损失）。

### 射击复现匹配与配件解析（2026-10-23）

- **死亡目标/残骸命中剔除**：其他玩家路径在 method8 绑定后按 HP 事件链检查受击方开火前
  是否已死亡（0.1s 容差吸收时钟量化，同 tick 击杀保留）；已死亡 → 按 miss 处理
  （`target_eid`/`target_name` 置空，保留 method20 弹道/地形落点），新增
  `ShotQuality.target_dead`（skip-when-false）与 `OtherShotsExtraction.skipped_dead_target`。
  作者路径同时检查 method38 `TARGET_ALREADY_DEAD (0x0002)` 与 HP 终态。修复内容：
  GB109 样本 6 发、GB48 1 发残骸命中不再发布为活目标（如受击方为已死亡的 P46）。
- **Type5 loadout 描述符数量**：`0A KK` 的 KK 实测 3..=7（旧实现只接受 6..=10，
  少带消耗品/给养的玩家整条 loadout 被丢弃，射击复现无法自动识别校准弹）；
  解析放宽到 1..=10，观察者/非战斗实体由 eid/class/结算花名册过滤，不按 KK 猜身份。
  **装备检测不再依赖 `0A KK` 描述符块**：只认 `0B <n>` 配件串 + `0C 00 0D` 帧尾，
  描述符块缺失/数量不认识时 `items` 为空但 `equipment` 仍采集（XM551 #30 实测
  `0A 04` + 9 件串；校准弹 103 正常识别）。
- **`game_hit_result=4` 保持 raw**：按
  [wotbtools-cross-reference §三.2](wotbtools-cross-reference.md) 裁决，4 的语义未定，
  不得命名跳弹/履带/击穿；消费方（WotbTools）显示「结果未定（4）」并从结果对照中排除。
- **射击复现无主装甲链门禁**：`PenetrationRequest` additive `require_primary`（默认 false
  保持 BlitzKit 相机点击语义；射击复现传 true）。无 hull/turret/gun（0mm Primary 仍算）
  且非 HE/非跳弹时，末层履带/炮管/间隙甲穿透不再返回 PEN + 全额伤害；Rust `/api/penetrate`
  与 WotbTools `penetration.js` 同口径。
- **碰撞数据口径（J20 #77 排查）**：`collision_boxes` 四盒**直读客户端 Parameters YAML**
  （hash6 量化框；hull/turret 的选段用 models.pb 模块→节点映射，数值与 YAML 逐值相等），
  `collision.glb` 是 CollisionMeshes `.sc2/.scg` 的装甲三角网格——两者都是客户端解包、
  **不是 BlitzKit**；部件盒不是网格 AABB（738 辆中 287 辆完全相等，其余主要在 +z 差，
  中位 20.8mm）。射击复现盒缺失改为 fail-closed，不再按网格自算 AABB。J20 #77 的 27.5mm
  盒-网格差不是选板原因：换 GLB 盒重跑仍为 plate3/RICOCHET，且全量换框会让首发板匹配
  488→457、部件匹配 577→502。详见
  [回放与射击逆向总集 第四篇 §6.3「固有差异界限」](回放与射击逆向总集.md) 与
  [game-data-sources §2.4](game-data-sources.md)。
- 行为变化随本轮 **v0.4.11** 发布 WASM；WotbTools 需 pin bump 后生效。

### 文档勘误与代码注释体检（2026-10-24，纯文档/.gitignore，无行为变化）

全仓文档对照现实逐条核查后修复过期事实（权威口径以被引文档为准），并完成两仓代码注释/
工作区体检。**修改面**：仅 docs/、README.md、.gitignore；Rust/Python 代码零改动
（clippy `--workspace --all-targets` 零警告，无需动）。
- **数字过期**：v0.4.7→v0.4.11（data-inventory §2.2）；有意分歧车辆 23→**111 辆**、
  `model` 等价 733/735→**623/735**（data-inventory §一 #8、decoupling-status §1、
  local-model-export §0/头部、feasibility-glb 头部——与 local-model-export §0 表对齐）；
  悬挂 725/735→**728/738**（tank-suspension-client-re §七）；t11 线上 release 名补
  `-destructibles` 后缀（data-inventory §1b）。
- **COS→Alibaba 残留**：local-model-export「工具链（发布链）」的 COS 上传命令改为指向
  Alibaba origin 流程（game-data-sources §5.2）；「包与 COS 随发」措辞更正（local-model-export
  头部、decoupling-status §1）；map-lighting-plan §九 加留档注记（清单不回勾、COS 链已废弃）；
  feasibility-track-marks §二「COS 未同步」按其 §五实施状态回写。
- **自相矛盾/指向漂移**：game-data-sources §5.6 删「Android 再走 ≤1024² 派生」（与本节结尾
  及 data-inventory #10b 矛盾）；map-render-align-audit TL;DR/§三 计数 23/19→**33**（后续追加
  B6-B8/D3-D8/P3 未回写）、重复编号 D5（贴地贴花）改 **D8**；README「契约第 6 节」失效引用
  改指 replay-wasm lib.rs 文档注释 + WotbTools contracts/agent/replay-facets-v2.md；
  「总集 §6.3」歧义（两处 §6.3）标注为第四篇；decoupling-status 冻结故障行指向 §5 的死链改指
  data-inventory §一 #4。
- **注释体检结论（两仓）**：本仓 Rust/Python 注释纪律良好——零被注释掉的死代码、零过期
  TODO（仅 2 处有效 TODO/占位）、横幅仅存在于验证探针 binary 且承载分节语义，**不做删减**
  （注释均为口径/理由载体）；WotbTools（对照 origin/main）frontend+common 零 TODO/FIXME、
  零死代码块、无违规归档目录，其 pin `deploy/agent/source.json` 已是 v0.4.11 无需 bump。
- **.gitignore**：补 `tmp_wi_probe/`、`tmp_pw/`（`tmp_*` 通配只覆盖 txt/json，两个探针目录
  此前以未跟踪状态挂在 git status 里）。
- **遗留提示（2026-10-24 记；2026-10-29 本地清理后更新）**：本地调查产物 `tmp_analysis/`
  与 `investigation/` 按仓规不入库、可按需清理。2026-10-29 已清理调试类残留约 **360 MB**
  （facets 对比基线 113M、`re/shaders/` 贴图提取副产品 139M、`map_dbg_*`/`map_reexport_*`
  88M、`verify_*`/`pb_trial`/`screenshots` 约 10M、`host_delta` staging 8M、`.git` 垃圾
  临时包）；研究类工作台（RE 反汇编转储、客户端 shader 源码、`client_dump/`、研究扫描
  输出）按"研究内容不删"保留，`tmp_analysis/` 现 31M、`investigation/` 41M 原样保留。

### 回放血量链：隐藏段（盲炮）掉血在二次点亮兑现（2026-10-26，解析器行为修正，v0.4.12→v0.4.14）

丢视野期间挨的盲炮/视野外伤害此前**完全脱离回放血量链**：Type4 隐藏段零更新
（type=7/8/10 全断），method1 事件只覆盖在场段，而重入 Type5 物化快照的当前血量
（`AoiPresence.hp_raw`，offset51 重广播语义）此前只透传给评审切面、未并入
`hp_series`——坦克二次点亮后血量链停在丢视野前旧值，直到下一次受击才被动修正，
3D 血条看不出隐藏段掉过血（**J20 样本实测：CN_LEVITHAN 在场末 1192 → 重入快照 348，
隐藏段缺 844 点，链上无任何体现**）。

- **修复**：`ReplayModel::scan` 把全部血量证据（`initial_hp` 种子 + method1 事件 +
  每个在场段开段 Type5 物化快照）**统一按时间合并（同钟快照在前，与客户端
  `onEnterAoI` 物化先于属性更新同构）、最后做一次相邻同值去重**，快照血量在
  `t_in`（二次点亮时刻）落台阶。哨兵族按 `hp_terminal_normalized` 钳 0（与 method1
  显示口径同判，死亡面权威仍在 `hp_state`/prop1）；无锚点实体（首物化短包）以首个
  带血量快照为链起点。**凶手归属 fail-closed 不做**（隐藏段无 method1 来源，证据链
  不足不猜）。**两条构建不变式**：①合并顺序（v0.4.13 修正 v0.4.12 的先去重后并入
  缺陷）——去重必须在全部证据并齐之后，快照夹在两条同值 method1 之间时（隐藏段死亡 +
  复活回满血再受击、值恰同受击前），先去重会吞掉后一条真实受击；②死亡证据分离
  （v0.4.14 修正 v0.4.13 的登记次序缺陷）——method1 归零是否立 `zero_hit` 看上一条
  method1 更新的值（同值归零 = 同一次死亡的 overkill 重复），与夹在中间的终态快照
  无关，否则残骸重入场景的死亡归属被吞。
- **证据与验证**：合成包流 6 条单测锁定（隐藏段掉血兑现/无变化去重/终态哨兵钳 0/
  无锚点起点/快照分隔同值 method1 保留后条）；13 份仓内样本探针（血量链 vs 重入快照
  逐段比对）基线 1 处脱节 → 修复后 0 处，J20 样本血量链在 t=187.25 出现 348 台阶、
  191.33 归零闭合。
- **影响面**：`PlaybackData.vehicles[].hp`（3D 血条曲线）在有隐藏段掉血的车辆上新增
  台阶点（普通回放 v0.4.13 与 v0.4.12 输出逐位一致，仅复活/同值边界修正）；切面 v2
  形状不变；AI 投影 golden 不受影响（投影不走 `hp_series`）。WotbTools 随 pin bump
  生效，3D 血条在二次点亮时出现台阶属预期修正。

### 数据面

- **超测 t11r3（11.4.0.138）数据面 + `*_top` 弹种口径（2026-11-13，数据面 + 前端消费）**：新预览源 `WotBlitz_11.4.0.138_t11r3.zip`（20,878,922,119 B，Last-Modified 2026-11-12；`version.txt` = `11.4.0.138_7408695 11.22_supertest_tier11_6 WOTB_Win7`）实装此前 4 个 pending 位——`J77_Ho_Ri_4`（10593，japan）、`F135_AS_XX_40_t`（23361，france）、`G187_Taschenratte`（30737，germany）、`G189_Hirschkafer`（30993，germany），科技树 **312 节点 / 0 pending / 318 边**，dataset **`11.4.0.138`**（release `t11r3-20261113-r2`）。**两包 CRC 对比**（central directory，Range，不下载 20 GB）：added 216 / removed 3 / changed 143——车辆 XML **9 changed + 4 added**，组件仅 france/germany/japan 六类（弹种/炮/炮塔/底盘/发动机/电台），strings 20 语言全变（+39 键：4 个新机制文案 + 31 条 `tank_descr_param_*`），`tank_upgrade_attributes.xml` 新增 24 条属性图标引用，地图平面零变化。**7 辆既有车 XML 变化经逐字对比确认无属性改动**（都是 `<unlocks><vehicle>` 研发边 + 升级图标重指），因此 738 辆基线 `tank/*.json`/`game_data` 逐字节不变属正确结果，不是漏导。**新增逐弹 `*_top`（additive，卡片口径 = 无配件 + 强化树点满）**：`export_research_loadout.py` 新增 `upgrade_top_shell_bonuses()`（top 选择语义与 `upgrade_top_hp` 一致：全部非 alternative 节点 + 每组首 choice；只统计 `ShellPiercingPowerUpgrade` 近距与 `ShellDamageUpgrade`；slotIndex 缺失/负数 fail-closed），产出旁路表 `data/upgrade_top_shells.json`；Rust `tank_resolver.rs` 读它把加成叠加进 `tank_cache` 逐弹 `penetration_top`/`damage_top`（仅 XI；槽位数 > 顶级炮弹数即 fail-closed）。r2 相对 r1 仅 2 个文件（`data/tank_cache.json`、`data/data_version.json`），9 辆 XI 之外零变化；`test_upgrade_progression.py` 补 5 组用例（top 语义 / 远距不计近距 / 稀疏槽稠密化 / slotIndex 反例）。**起因（用户报障）**：坦克百科列表卡片用 `tank_cache` 基础值算 `pen_max`，与详情页（`projectTankUpgrades` 点满投影）不一致——Ho-Ri 4 卡片显示 298 而详情 320；包内弹种数据与客户端 `shells.xml` 逐值一致（AP 298/268、APCR 363/326、HE 84），**不是解析缺陷**，是卡片口径缺 `*_top` 字段。**验证**：本地 `verify --pack --full` PASS；主机 stamp/delta/`verify --full` PASS、`dataset.sh verify --full` PASS、对象库 `verify --full --links` PASS（160968 条 / 6972 对象）；公网 `versions.json` → `11.4.0.138 = t11r3-20261113-r2`，`tank_cache`/`data_version`/`manifest` 逐文件 sha256 **0 mismatch**，抽样读回 Ho-Ri 4 = `320/390/90`、非 XI 无 `*_top` 字段。**波及面**：dataset `11.4.0.138`（r1 保留回滚）；WotbTools 侧同批适配（`pen_max` 优先 `*_top`、35 个强化/机制图标（`tools/export_augment_icons.py`）、4 机制 + 22 属性三语文案、机制面板 5 项基值绑定、`wotbVersion` 5.2.1）。**运维教训**：①`asset_store.py adopt --releases` 要传 **releases 父目录**（传 release 自身会静默 0 文件）；②往主机 `cp -al` 克隆树覆盖文件必须 `cp --remove-destination`，`scp` 直写会把共享 inode（manifest/version）写穿到上一 release——本次 r1 manifest 被 r2 内容覆盖后已从本地留存的 r1 manifest 断链恢复并 `verify --full` PASS；③`export_asset_pack.py --refresh-manifest` 从**仓库 `data/`** 取 `data_version` 语义块，预览包必须再用包内 `data/data_version.json` 重写 manifest 后 stamp。

- **数据版本命名对齐客户端 + 新增 `11.21.0_1`（2026-11-11，数据面 only；origin 已于同日发布）**：检测链（`tools/check_wg_update.py`）发现客户端 SmartDLC 动态内容 `s11.21.0_1`（2026-11-10 发布：dx11/astc 贴图 + 活动/offer 文案 + 新收藏车 Relic 注册文案，`dc_11.21.0_1_7405732/3/4`；Steam Windows manifest 全程未动——动态内容绕开 manifest）后，把 WotbTools 前端版本选择器的 dataset 命名对齐客户端自身命名：① dataset_id 字符集放宽允许 `_`（本仓 `asset_release.py` 与 WotbTools `useDataVersion.ts` 同款正则同步放宽；`_` 本就是 URL 安全字符）；② 新增 dataset **`11.21.0_1`**（release `11.21.0_1-20261111-r1`）：`pack_incremental.clone_base` 硬链接克隆 11.21.0 包（5965 件 0 复制）+ 复制 manifest + stamp；数据面与 `11.21.0-20261106-r2` **逐字节一致**（content_digest 同为 `sha256:9198d0ca…`，仅 version.json 身份不同）——动态内容走客户端运行时通道，待 11.21.1 客户端数据后充实该 dataset；③ dataset **`supertest` → `11.4.0.130`**（超测客户端自身版本号 `11.4.0.130_7400742`，label 仍"超测服"，release `t11p2-20261106-r8` 不变；下一轮预览按其客户端版本换新 id）；`11.21.0` **不改**——客户端对未打补丁基线的命名就是 `11.21.0`（`regions_11.21.0.yaml`），槽位自 `_1` 起（`s11.21.0_0.yaml` 双 host 404 实测）。**顺带修复本地 supertest 包漂移**（stamp 时暴露）：本地 `asset_pack_t11p2_preview` 为 r4 manifest/data_version + r6 世代 `research/meta.json`+3 modules 的混合态；对照线上逐文件比对定位后，按线上 r8 manifest 逐文件 sha256 回拉 4 个 r4 时代文件（`research/params/{format,structure}.json`、`research/strings/{en,zh-Hans}/research.json`），包内容与线上 r8 逐字节一致（6000/6000），manifest `data_version` 恢复 r6+ 语义块（`release/11.21.0+t11p2` + preview 身份 + 743），content_digest 复现线上登记值 `sha256:97f46b5a…`。**验证**：`test_asset_release.py`（新增 charset 用例）15 用例 PASS；本地 `catalog`（3 dataset，default=11.21.0）+ `verify --root` PASS；三个 datasets 链接穿透读 `index.json`/`version.json` OK。**波及面**：本地 `release/datasets/`（`11.21.0_1` junction、`supertest`→`11.4.0.130` 改名）与 `release/versions.json`；origin 发布（同日，WotbTools #947 合并 + `frontend.yml` 部署 success 后执行）：主机 bin 工具更新（`asset_release.py`/`dataset.sh` 含 `_` 字符集）→ `cp -al` 双 release（`11.21.0_1-20261111-r1` 克自 r2、`t11p2-20261106-r9` 克自 r8 作改名载体，release 不可变约定）→ 主机侧 stamp（digest 复现 `9198d0ca…`/`97f46b5a…`，verify --full PASS）→ delta 双双 0 变更（`--allow-dataset-change`）→ 对象库 adopt 0 新增（内容全部命中既有对象硬链接）→ `dataset.sh link 11.4.0.130` / `unlink supertest` / `link 11.21.0_1`。**公网验证**：`versions.json` 三 dataset（default 仍 11.21.0）逐项回读一致；三包 version.json 身份 + 每包抽样内容文件 sha256 **0 mismatch**；`dataset.sh verify` PASS；WotbTools `deploy/assets/verify.sh https://assets.wotbtools.com` PASS（Windows Git Bash 下 CRLF/URL 两处已知坑，主机 Linux 执行）。旧 bundle 对含 `_` 目录整目录 fail-closed（选择器隐藏降级，默认回放不受影响；旧 APK 待 5.1.1 发布+更新后消除）；`?data=supertest` 旧链接 404（换 `?data=11.4.0.130`）。

- **数据版本更新逻辑：全量差异 + 增量重导 + pack 复用（2026-11-06/07）**：`tools/plan_data_version.py` v2 对“上一个默认客户端 vs 新客户端”做全量源清单比较（目录 sha256 / ZIP CRC32+size），输出逐平面 added/changed/removed、`tanks`（bridge → tank_id/kinds）、`maps`（map_index → space/keys/map_ids）、`wholeTable`、`pins`、`unmappedChanged`、coverage；`--strict` 对未识别变化/未解析 tank/map/缺产物 fail-closed。`tools/apply_data_version.py` 改为 handler 编排：`research` 走全平面同步；`vehicle-assets` 逐车调用原 GLB/悬挂/履带/图标导出器；`core` 从 base pack 反向物化 `work/data` 后按原 `update-data`/`dump-tank-data`/`dump-shell-kinds` 整表重算，只覆盖变化字节；`maps` 按变化 space 重导 GLB/ground/光照/IBL，2D/变体整表重跑。新增 `pack_incremental.py`（硬链接克隆、同哈希复用、断链覆盖、孤儿删除）与 `data_version_handlers.py`（注册表、`--dry-run`/`--only`）。`data_version.json` 增加 base/new、plan sha、changed planes、coverage、scope、inputs。测试 `test_plan_data_version.py` / `test_apply_data_version.py` / `test_map_materialize.py` 通过；真实 r6 smoke：单车资产只改 `data_version`；核心 smoke 只改 5 份预览 `tank/*.json` + cache + data_version，738 份未变 game_data 逐字节保持；地图 smoke（karelia）只改 `scenery.glb`、`index.json`、`map2d`/`cover` 索引与 `data_version`，43 张 2D/封面字节复用，base pack `verify --full` 仍 PASS。文档见 [game-data-sources.md](game-data-sources.md) §5.1b。**首次实战**：strings 平面检出 r6 的 11.21→11.22 文案差异并发布（见下条）。

- **超测 r8：XI 升级树布局字段导出（2026-11-07，数据面 only）**：已上线 `t11p2-20261106-r8`（**6000 件 / 4460.9 MB / 743 辆**，digest `sha256:97f46b5a…`），相对 r7 仅 4 个文件：`research/modules/{china,usa,ussr}.json` + `research/meta.json`。`export_research_loadout.py` 的 `parse_upgrade_progression` 新增布局契约（源自客户端 `TankProgress/UpgradeProgression` 的 `matrix` 与车辆 XML，WotbTools 研发树渲染消费）：`matrix{rows,columns}`；逐节点 `gridPosition [row,column]`（COMMON/MECHANIC 必填、ALTERNATIVE 必无，违例 fail-closed）、`xpCost`、`requiredNodes`（techName 解析 + 迭代 DFS 无环校验）；增强组同带 xpCost/gridPosition/requiredNodes。`test_upgrade_progression.py` 补 3 组布局反例。**验证**：`test_upgrade_progression.py`/`test_asset_release.py` PASS；主机 verify/link PASS；公网回读 4 变更 + 1 对照文件 sha256 **0 mismatch**；WotbTools `deploy/assets/verify.sh` PASS（脚本本体在 Windows Git Bash 下受 python3 文本模式 `\r\n` 影响，生产 Linux 无此问题）。**本地事故与修复**：`asset_release.py` 的 `write_json` 原地 `write_text`，在主机 `cp -al` 硬链接克隆的 r8 上 stamp/delta 时把 r7 的 `version.json`/`delta.json` 共享 inode 一并改写（r3 时代同型事故复发）；已断链恢复 r7 元数据（catalog 快照回填 release/stamped_at/content_digest/manifest_sha256/notes，manifest_sha256 实测比对通过）并重生成两侧 delta；`write_json` 改 temp + `os.replace` 原子替换，新增硬链接回归测试。**波及面**：仅 supertest dataset；r7 保留回滚。

- **超测 r7：XI vehicleAbilities 导出（2026-11-06，数据面 only）**：已上线 `t11p2-20261106-r7`（**6000 件 / 4460.9 MB / 743 辆**，digest `sha256:f5a891f5…`），相对 r6 仅 4 个文件：`research/modules/{china,usa,ussr}.json` + `research/meta.json`。`export_research_loadout.py` 从车辆 XML `<extras>` 中只保留 item 目录带 `<isVehicleAbility>` 的标签，写入逐车 `vehicleAbilities`：BZ-79 `rechargeableNitro`、432U `combatFrenzy`、KR-1 `rammingConfiguration`、Black Rock `burstMode`、XM69 `steadyAimSystem`；同时写 `gameVersion`。`sync_preview_research.py` 增加 5 车能力绑定校验；`meta.json` 保留 r6 的 131 项清单形状，仅刷新 3 个 modules 的 sha256/bytes。**验证**：本地/主机 `asset_release verify --pack --full` PASS；`dataset.sh link`/`verify` PASS；公网 `/v/supertest/` 回读 3 个 modules 与 meta 的 sha256 **0 mismatch**，`versions.json` supertest=r7、default 仍 11.21.0；r6 保留回滚。**波及面**：仅 supertest dataset；WotbTools #904 的机制展示可读取该绑定，无 WotbTools/WASM/Android 发版。

- **超测 r6：预览 strings/meta 同步（2026-11-06，数据面 only）**：已上线 `t11p2-20261106-r6`（**6000 件 / 4460.9 MB / 743 辆**，digest `sha256:ed940dfb…`），相对 r5 仅 3 个文件：`research/strings/en/research.json`、`research/strings/zh-Hans/research.json`、`research/meta.json`。r4/r5 的 `research/strings` 仍是 **11.21 基线**（`meta.gameVersion=11.21.0.1235_7376187`）；新全量同步链的 strings 平面检出预览 11.22 差异并发布：两语 `meta.gameVersion` 切到 `11.4.0.130_7400742 11.22_supertest_tier11_5`，新增 `unit_measure_short_meter_per_second_squared = "m/s²"`（46→47 条），**23 条参数名去掉客户端已移除的 `<text_units>%(units)</text_units>` 包裹**（改为 `%(units)`；其余文案不变），`meta.json` 刷新 strings sha256/字节数与 preview 时间戳；其余 5997 件与 r5 逐字节相同。**验证**：本地 `asset_release verify --pack --full` PASS；主机 `asset_release.py verify --pack --full` PASS、`dataset.sh link`/`verify` PASS；公网 `/v/supertest/` 回读 manifest/version/meta/strings 5 件 sha256 **0 mismatch**，`versions.json` 的 supertest=r6、default 仍 11.21.0。**波及面**：仅 `supertest` dataset；无 WotbTools/WASM/Android 变更，r5 保留回滚。

- **超测 r5：模块耐久参数补导出（2026-11-06，数据面 only）**：已上线 `t11p2-20261106-r5`（**6000 件 / 4460.9 MB / 743 辆**，digest `sha256:1ec79041…`），相对 r4 仅 3 个文件：`research/params/structure.json`、`research/params/format.json`、`research/meta.json`。新增预览客户端 `modulesDumpSection`：**7 类部件（履带/发动机/火炮/弹药架/炮塔旋转机构/观瞄设备/油箱）× 6 指标（MaxHealth / RegenHealth / HysteresisHealth / RegenPerSec / BurnPerSec / RepairTime）= 42 个参数**；`params/format.json` 70→112 键（保留现役 `gunCommonReloadTime`，新增 `previewGameVersion`），结构保留 `crew` 并追加 `modulesDumpSection`，但**不加入 `mainSections`**，WotbTools 参数面板暂不展示。`export_research_loadout.py` 修复预览 `hangarParamsConfig` 的 `numeric.keyedArchive` 包装（此前 42 个 format 条目全为空）；`tools/sync_preview_research.py` 增加 params 增量合并与 42 项校验。**验证**：`asset_release verify --pack --full` PASS；主机 import/materialize/verify、`dataset.sh link`/`verify` PASS；公网 r5 回读 `moduleDurabilityParams=42`、`fmt=112`。**波及面**：仅 `supertest` dataset；无 WotbTools/WASM/Android 变更，r4 保留回滚。

- **超测 r4：XI 装甲摘要/升级树/顶级 HP/消耗品补给（2026-11-06）**：已上线 `t11p2-20261106-r4`（同日中间版 `r4-xi-items` 仅 items/modules，最终 r4 相对其 6 文件 = `tank_cache` + 5 预览 `tank/*.json`；两者均保留回滚）（**6000 件 / 4460.9 MB / 743 辆**，digest `sha256:1dcf3221…`），修复 5 辆 XI 的 **tank 顶层 `armor` 摘要**（此前 `update-data` 在提取 `game_data` 之前重建 cache，空目录使 `extract_armor_summary` 回退“取最大板厚”：XM69 车体 230/230/230 → 客户端 `primaryArmor` **135/80/40**，炮塔 210/210/210 → **210/90/55**；BZ-79/432U/KR-1/Black Rock 同步修正），并把 `data/tank_cache.json` 的 739/743 条错误摘要全部按 `game_data` primary 重算。**新增数据面**：5 辆 XI 的 `research/modules/{nation}.json` 写入客户端 `upgradeProgression`（节点/备选组/属性定义/provenance，与 WotbTools fixture 逐项一致）；`data/upgrade_top_hp.json` → `tank_cache.hp_top` / `tank/{id}.json.hp_top`（XM69 2200、BZ-79 2850、KR-1 3100、Black Rock 2900、432U 2200）；`research/consumables.json` / `provisions.json` 从预览客户端重导，**过滤器 tier 上限 10→11**（65/89 件，common 14/38），新增 `isVehicleAbility` 条目按战斗能力排除出装配池。**根因修复**：`update-data` 改为先 `extract_all` 再重建 `tank_cache`，空 `game_data` 直接报错；预览构建预置 738+5 份 `game_data` 后再 dump。**WotbTools**：XI 卡片血量读 `hp_top`，详情/装甲查看器缺省 `upgrade=top`（显式 `upgrade=stock` 可回退），同 PR bump `wotbVersion=4.5.1`。**验证**：`asset_release verify --pack --full` PASS；Rust workspace 测试全过；WotbTools 前端 6188 项通过（另 `agent-wasm-dist.contract` 在 Windows CRLF 下需 LF 本地化，LF 下 9/9 通过，与本次改动无关）。**波及面**：仅 `supertest` dataset；默认 `11.21.0` 与 `data/research`（738 辆，consumables 64/provisions 85）不变；r3 保留回滚。

- **11.21 + 超测数据面重建（2026-11-06，r2 → r3）**：按最新口径重做两个 dataset 并发布——`11.21.0-20261106-r2`（**5965 件 / 4433.0 MB / 738 辆**，digest `sha256:9198d0ca…`）与 `t11p2-20261106-r3`（**6000 件 / 4460.9 MB / 743 辆**，digest `sha256:3c0a4b62…`；r2 为同日中间版，保留回滚）。**11.21 变化**：补齐 7 个变体的 2D 底图/封面（43/43），相对上一 release 仅 14 个新增派生文件 + 3 个索引（delta 17 件，5.8 MB）；`map/` 几何与 `ground.webp` 零变化。**超测变化**：改为在 r2 基线 + 5 辆 XI 上全量重建，743 辆 tank JSON 统一带 burst 字段（相对旧超测 release 708 个 tank JSON + 9 个数据/索引文件变更，delta 717 件，10.1 MB）；research 743 辆 / 308 节点 / 4 pending / 314 边，shellKinds 2099；地图/2D/封面零变化。**r3 修正**：补 `data/cache/models` 后重跑 `dump-tank-data`，5 辆预览车的 `model_gun/turret_count` 0→1/1（Black Rock 2/1），仅 5 份 tank JSON 变化（38 KB）。**重建链（关键次序）**：pack → composite（36 图，固定 `YX`）→ materialize variants（7 个）→ `export_2d_basemaps.py`（此时 index 才是 43 key）→ `--refresh-manifest` → stamp；超测 research 从**纯 11.21 副本**出发，先把预览树 8 个 XI 登记位补成 pending，再跑 `update_t11_techtree.py` + modules merge，避免污染 `data/research`。**验证**：两包本地 `verify --pack --full` PASS；主机 `asset_release verify --pack --full` PASS；`dataset.sh link` 后 `versions.json` default 仍 11.21.0；公网 r2 变更集 734 文件 / 15.97 MB、r3 变更集 5 文件均逐文件回读 sha256 **0 mismatch**；CORS/ETag/Cache-Control 契约手工核验通过（`verify.sh` 的 dataset 段在 Windows Git Bash 下受 `python3` CRLF 输出影响，生产 Linux 无此问题）。对象库 6891 对象 / 4272.0 MB，16 个 release 逻辑 70614.4 MB（去重 94.0%）。**本地事故与修复**：超测构建直接 `write_text` `data/data_version.json`（`cp -al` 硬链接）曾把本地 11.21 包该文件写成超测内容（416 B vs 188 B）；已断链恢复并 `verify --pack --full` PASS，线上 11.21 release 不受影响（发布在其前，主机为不可变对象物化）。**波及面**：两个 dataset 的版本选择器/公网路径；`11.21.0-20261106` 与 `t11p2-20261106` 保留回滚。契约见 [game-data-sources.md](game-data-sources.md) §5.2、[data-inventory.md](data-inventory.md) 包统计。
- **11.21 现役包全量重建（2026-11-06，数据面修复）**：生产 `11.21.0` 从
  `initial-20261019`（无 `research/`）重建为 **`11.21.0-20261106`**（**5951 件 /
  4427.2 MB / 738 辆**，digest `sha256:11e7bfa8…`）。**触发**：默认通道切回正式服后
  科技树 `/research/tech_tree/{nation}.json` 404——旧 release 早于 research 数据面。
  **重建链**：`dump-shell-kinds`（与 `tanks.pb` 同版）→ `export_tank_icons.py --all`
  + `--emit-crops`（738/738）→ `export_research_loadout.py --out data/research
  --with-research-graph`（纯 11.21：738 辆 / 304 节点 / 0 pending / 310 边，无预览
  节点）→ `export_asset_pack.py --map-index map_index.json` →
  `composite_overhead.py`（36 图，固定 `YX`）→ `export_2d_basemaps.py`（36 图 +
  cover）→ `materialize_destructible_variants.py`（7 个变体目录）→
  `--refresh-manifest` → `stamp`。相对 `initial-20261019` **935 件变更**（185 新增 /
  750 修改 / 0 删除）：`research/**` 29 件（含 9 国 `tech_tree`）、738 辆
  `tank/*.json`（连发组字段）、`data/tanks.pb`、`data/gunMaskSlices.json`、9 图
  `destructibles.json`（变体元数据）、`map2d` 36 图与 7 个变体目录。
  **验证**：本地 `verify --pack --full` PASS；主机同命令 PASS；`dataset.sh link`
  PASS（catalog default 仍 11.21.0）；公网回读 `/version.json` = 新 release、
  `/research/tech_tree/ussr.json` 200（41 节点 / 43 边 / 0 preview）、
  `/manifest.json` 5951 件。旧 `initial-20261019` 保留回滚。
- **内容寻址对象库（2026-11-05，存储去重，additive）**：`/srv/wotb-assets/objects/<sha[:2]>/<sha256>`
  保存唯一内容，`releases/<id>` 树改为对象硬链接；Caddy 仍按静态文件伺服，URL/ETag 语义不变。
  新增 `tools/asset_store.py`（`import` / `materialize` / `adopt` / `gc` / `stats` / `verify`）
  与 `tools/test_asset_store.py`（9 用例）。实测：11.21 单包 **5965 条 / 逻辑 4432.9 MB →
  5841 个唯一对象 / 4216.7 MB（包内去重 4.9%）**；11.21 与 t11_preview 的 5907 条共同路径中
  **5899 条逐字节相同**；3 个本机包合计 **13075.9 MB → 4237.4 MB（67.6%）**。**生产迁移
  （2026-11-05）**：`/srv/wotb-assets` 的 9 个 release 逻辑 **39449.5 MB** → 全局唯一
  **6107 个对象 / 4237.9 MB（去重 89.3%）**；`verify --full --links` PASS（53035 条路径、
  6107 个对象全量哈希），release 树在盘上 **4.0 GB**（对象硬链接共享）。**t11p2 发布后
  （2026-11-06）**：10 个 release，**6147 个对象 / 4257.6 MB（去重 90.3%）**，盘上
  **4.1 GB**。**波及面**：
  存储/发布流程；`cp -al` 旧流程仍可用，`adopt` 可原地迁移已有 release。
  详见 [asset-store.md](asset-store.md)。
- **超测服 t11p2 预览包（2026-11-05/06，数据面更新）**：新预览源
  `WotBlitz_11.4.0.130_t11p2.zip`（`11.22_supertest_tier11_5`）整理上线，dataset 通道
  重组为 `supertest`（超测服，当时 default）+ `11.21.0`（现役）——超测服整体复用同一
  通道随预览轮次重指（t11b → t11p2 → …），`11.22` 通道待正式服包发布时加入（终态
  三通道：超测服 / 11.21 / 11.22）；旧通道 `11.21.0-t11-preview` 已 `unlink` 下线。
  生产 release **`t11p2-20261106`**（**6000 件 / 4460.8 MB / 743 辆 = 738 + 5 XI**，
  digest `sha256:ea489ff1…`；首发 `t11p2-20261105` 因穿深缺陷同日替换，均保留回滚）。
  新实装 XI：`R228_KR_1`（28161）、`A179_Black_Rock`（30241，收藏车不上树）、
  `A183_XM69_Hacker`（30497）；BZ-79/432U 数值随 t11p2 更新。**提取纪律**：对基线
  预览 ZIP 做逐成员 CRC 对比，只拉变化成员（车辆 XML 94+3、共享元数据、预览车全套；
  实测 327 文件 / 202 MB）；新坦克复用的旧车贴图直接用本机 11.21 客户端字节
  （`tex_fail=[]`）。**弹道迁移适配**：t11p2 把 piercingPower/speed/maxDistance 从炮
  `<shots>` 迁入 `shells.xml` 弹种条目，`tools/extract_vehicles.py` 按"shots 有值优先、
  为空回落弹种条目"适配（首发漏适配 → 全弹种穿深 0，已修）；`dump-shell-kinds` 富表
  随新 pb 重导（弹种表 2090 → 2099）。**科技树外科更新**（`tools/update_t11_techtree.py`，
  新工具）：97 辆 CRC 变化车逐车重算研发边，`R228`/`A183` 由 pending 转正 →
  **308 节点 / 4 pending / 314 边**。**波及面**：预览通道数据与工具链；11.21 现役通道
  零变化；`M6`/`T-43` 部分根模糊匹配污染由匹配器词缀锚定修复（完整客户端 738 辆
  game_data 逐字节无回归）。来源与口径见 [game-data-sources.md](game-data-sources.md) §2.7。
  **公网路径**：`/versions.json` 与
  `/v/<id>/...` 需 WotbTools 的 Caddyfile（`/v/*` + `/versions.json`）redeploy 后生效——
  2026-11-06 已随 Caddyfile redeploy 上线（`/versions.json` 与 `/v/<id>/...` 回读 200）；新 Caddyfile 已用临时容器对真实
  `/srv/wotb-assets` 验证（`/index.json`、`/version.json`、`/versions.json`、dataset
  路径全 200），未改动生产 Caddy。前端选择器与 Caddy 同批部署。
  契约见 [game-data-sources.md](game-data-sources.md) §5.2 与
  [data-inventory.md](data-inventory.md) §2.1。
- **按游戏版本的 dataset 版本管理（2026-11-05，additive 发布面）**：资产 origin 从"单份
  `current`"扩展为 `datasets/<dataset-id> -> releases/<release-id>` + `current -> datasets/<default-id>` +
  主机级 `versions.json`。每个 release 写不可变 `version.json`（`dataset_id` / `game_version` /
  `variant` / `release` / `content_digest` / `manifest_sha256` / `tank_count` / `status`）；
  `content_digest` 只由 manifest 的 path→sha256 与语义化 `data_version` 决定（排除
  `generated` / `upstream_commit` / `worktree_dirty` / 提取时间戳），同内容重建不漂移。
  新增 `tools/asset_release.py`（`stamp` / `catalog` / `verify` / `delta`）与
  `tools/test_asset_release.py`（12 用例）；`scripts/export_asset_pack.py --refresh-manifest`
  同步排除 `version.json` / `delta.json`（发布元数据不进内容清单）。对外路径
  `/v/<dataset-id>/...` + `/versions.json`，根路径仍指 `current`（旧客户端零改动）；
  WotbTools 侧增加显式数据版本下拉（侧边栏 logo 下方，移动端在更多设置）、`?data=`/localStorage
  选择、Android `/agent-assets/v/<id>` 透传、`deploy/assets/{Caddyfile,dataset.sh,verify.sh}`
  与 CI fixture，`wotbVersion` 4.2.3→4.2.4。**证据**：真实 11.21 包（5965 件/738 辆）
  `stamp --dry-run` 通过；Python 12 用例、WotbTools `vue-tsc` 与定向 Vitest 用例通过。
  **生产登记（2026-11-05，2026-11-06 通道重组）**：最初登记 `11.21.0-t11-preview`
  （default）+ `11.21.0`；t11p2 上线时改为 `supertest` + `11.21.0`；**2026-11-06 默认回到正式服**：`11.21.0`（default，release `11.21.0-20261106`）+ `supertest`（超测服，显式选择），
  旧通道 `unlink` 下线，`current` 根路径行为对旧客户端保持连续。**公网路径**：`/versions.json` 与
  `/v/<id>/...` 需 WotbTools 的 Caddyfile（`/v/*` + `/versions.json`）redeploy 后生效——
  2026-11-06 已随 Caddyfile redeploy 上线（`/versions.json` 与 `/v/<id>/...` 回读 200）；新 Caddyfile 已用临时容器对真实
  `/srv/wotb-assets` 验证（`/index.json`、`/version.json`、`/versions.json`、dataset
  路径全 200），未改动生产 Caddy。前端选择器与 Caddy 同批部署。
  契约见 [game-data-sources.md](game-data-sources.md) §5.2 与
  [data-inventory.md](data-inventory.md) §2.1。
- **客户端 maskSlice 记录导出；炮管裁切保持 BlitzKit 口径（2026-10-28）**：`maskSlice`
  是客户端炮管**视觉/特效切片**参数（与同车 yaml 的 `gunRecoil`/`gunMarks`/`markerPosition`
  同组），**不是装甲外部模块的边界**。装甲查看器继续用 BlitzKit 口径：`gun_mask` 标量
  （= `maskSlice.<gun_NN>.planePosition[1]`，models.pb field 5）+ 模型原点链沿炮口方向建
  裁切面，有 mask 值并入 `gun_0X`+`gun_0X_mask` 并由平面裁掉炮根，无值只取精确 `gun_0X`；
  裁切贯穿点击/续飞/热力图/厚度视图。曾尝试按完整客户端记录（planePosition/planeNormal/
  slice mode）建面，但全库 477 条 `planeSliceMode=1` 记录里 287 条平面落在炮管网格之外
  （中位切割 0）、15 条会切掉 >60% 炮管（Pz.Sfl. IVc gun_03：平面 y=4.29、炮管
  -2.06..5.38），结果不一致且出现“怪裁切”，该方案已回退。数据面仍保留完整记录：
  `tools/extract_vehicles.py` 提取每门炮 `maskSlice`（position/normal 必须完整，缺则
  fail-closed 不生成），`tools/emit_vehicle_pb.py` 写 sidecar `data/gun_mask_slices.json`
  （键 = `tank_id:turret_local:gun_local`，478 条；同炮不同炮塔节点/平面不同，如 T-34
  gun_01 0.8415 vs gun_07 0.8743），打包器复制为包内 `data/gunMaskSlices.json`（缺源拒绝
  打包，供后续视觉/特效切片使用；viewer 当前不读取）。`data/tanks.pb`/`models.pb` 字节不变。
  **影响面**：资产包新增 1 个文件/包（5907、5921 件，manifest 已重算）；WotbTools 行为与
  2026-10-20 的 BlitzKit 裁切一致；WASM pin、回放切面、地图资产不动。
- **射击分析「目标炮塔朝向缺失」整场 fail 修复（2026-10-28）**：训练房静止靶（全程不转
  炮塔）零 `type=7 prop2` 变化流，作者严格射击路径按「命中弹必须有目标炮塔朝向」直接
  `Err`，WASM `parseShotReplays` 返回 `author_path="error"` + 0 发 → 前端射击分析显示
  解析失败（用户样本：Pz III A 打静止 Super Conqueror 20 发全部丢失）。修复 = 按协议
  语义补初值来源：`type=5 class=2` 车辆实体创建/重入物化包的前缀属性 blob 的 `id2`
  （u16 @48）就是物化时刻的 prop2 **当前值**（《总集》§2.6；14 样本探针：重入快照与
  紧随 type=7 采样同域，coarse 差中位 0、多数 ≤2 级）。`build_entity_indexes` 把它种进
  prop2 索引（布局护栏 class=2 + `id1/id2/id3` 标记，不满足即不采信）。影响面：无
  type=7 炮塔流车辆的回放/射击分析恢复——playback 的炮塔/俯仰列从「随车体 + 俯仰 0」
  中性值变为真实初值（恒定，无变化流时）；严格/宽松/HE 采样与热力图判定共用同一索引
  语义不变。WotbTools 需新 WASM release + pin bump 才生效；资产包、地图资产不动。
- **连发组（PomPom）DPM 修复（2026-10-28）**：`AC Atlas` / `ATAC` 等车的“一次扳机连发 N 发”炮
  （客户端 `<burst count rate>`）此前被当成普通弹夹——整夹每一发都按扳机间隔计，DPM 明显偏低
  （AC Atlas V2：旧式 1573 → 2623，本仓默认乘员系数口径）。修复链全程用客户端语义而不是
  数值猜测：`tools/extract_vehicles.py` 从车辆内联/共享 `<burst>` 导出
  `burst_group_count` / `burst_group_interval`（**只作时序，不参与 is_burst 分类**——
  分类仍只看内联 `clip`/`pumpGunMode`）；`tools/emit_vehicle_pb.py` 写 pb gun **field 20**
  （`Burst{count=1 uint32, interval=2 float}`，与 BlitzKit `GunDefinition.burst` 同义）；
  Rust `blitzkit.rs::parse_gun` 解码、`tank_configs.rs` 落 `configs[].burst_group_*`；
  WotbTools `gunCycleOf` + `characteristics.ts` 按 BK `resolveDpm` 的整匣循环
  `reload + (整夹/每组−1)×弹间 + (整夹/每组)×(每组−1)×组内间隔`（无连发组时退化为原式）。
  数据面：**35 辆车 / 39 个配置**新增字段（其余车辆 `data/tanks.pb` 字节不变；PB 重新编码
  自本机客户端 11.21.0.1235，`models.pb` 未动）；4 个非弹夹炮（如 MS-1 mod. 1 的 20 mm TNSh）
  也带 `<burst>`，字段照存但 `is_burst=false` → 计算走 regular，不受影响。**影响面**：资产包
  `tank/*.json`（35 件/包，两包均已更新并重算 manifest）；WotbTools 前端 bundle 变化
  （Android `wotbVersion` 同 PR）；WASM pin 不变。
- **科技树/研发图导出 + XI 预览合并（2026-10-27，随用户指令解除「不做科技树页」裁定）**：
  `tools/export_research_loadout.py --with-research-graph` 落地 `data/research/tech_tree/{nation}.json`
  （9 国）：`nodes`（stem/tankId/tier/price/tags + 客户端科技树 yaml 原始 `position`——**列≠tier**，
  tier 读 list.xml）+ `premiumRows`（**位置语义已由数据证实**：金币车带画在树顶部，UI 不重排格子）
  + `pending`（未实装 XI 登记位，不造名称）+ `edges`（310 条整车研发边；模块级解锁边
  276/208/457/351 仍在 `modules/{nation}.json`）。**11.22 XI 预览**：新工具
  `tools/fetch_t11_techtree.py` 对公开 CDN 20.67 GB 预览 ZIP 做 ZIP64 + Range 按需提取
  （实测 7.7 MB），`--tech-tree-extra` 并入 8 个 XI 新 stem（2 定义车 + 6 pending）；
  `--data-root` 可物化最小预览 Data 根使 `--modules-only --merge` 可复现。**防回退门禁**：
  输出已含 `preview` 合并时全量重导直接报错（真实事故：全量重导曾把 china/ussr modules
  冲回 738）。校验：`tools/test_research_techtree.py` 通过；meta 逐文件 sha256 0 mismatch、
  740 辆。**客户端 UI 契约补充（11.21 + 预览包 UI yaml 全套比对）**：XI 对科技树屏唯一实质
  改动 = `levels=[1..10]→[1..11]`；`Lines{linesMask}` 28 个连线图案是包内数据（几何已提取），
  仅每格 mask 编号由模型计算（位语义未定，禁猜）；XI 车零 `<unlocks>` 边 = 无研发链。
  **消费面**：WotbTools 新增 `agent-techtree` 视图；`research/tech_tree/**` 已于 2026-10-29 随
  `t11-preview-20261029-techtree` 发布（9 件；manifest 5920 件 / 4430.1 MB、0 mismatch；公网逐文件
  sha256 一致，`deploy/assets/verify.sh` PASS）。全量 schema/统计/证据见
  [client-research-tech-tree-loadout.md](client-research-tech-tree-loadout.md)。
- **WotbTools 复刻实施方案与同日落地（2026-10-27；范围按用户裁定重定）**：新增
  [wotbtools-research-loadout-plan.md](wotbtools-research-loadout-plan.md)（与上条数据盘点配套）。
  **范围**：单一整合配置界面（模块炮塔/炮/发动机/履带 + 装备 9 槽 3×3 + 补给/消耗品
  chips + 涂装，BlitzKit Tankopedia 布局同构，全部已解锁直接选择），与**装甲查看器模型**
  （configIndex 联动）和**百科数值**（计算引擎输出 + BK useDelta 同逻辑 delta）联动；
  **不做**研发/购买/解锁机制、不做科技树页、不做分入口。**数据源铁律**：客户端解析权威、
  BlitzKit 备用 + 公式参照；**隐蔽加成按客户端数据、按百分点绝对值相加**（用户裁定）。
  **同日落地（跨仓）**：`data/research/` 导出（123 文件 3.51 MB：738 辆 / 9 国别模块 /
  24 设备 + 9 预设 / 64 消耗品 + 85 补给 / 20 项乘员技能 / params 70 键 / en+zh-Hans 46 条
  文案 / 102 张客户端图标；`meta.json` 锚 `11.21.0.1235_7376187` + 逐文件 sha256）快照
  分发进 WotbTools `common/research/`；WotbTools 侧测试合计 129 全过。实现期口径与逐项
  UI 契约（射界图/技能联动/装备 tooltip/隐蔽五状态/术语/4 列矩阵布局等）见该文 §12。
  **影响面**：资产包与 WASM pin 不变；WotbTools 前端 bundle 变化（Android `wotbVersion`
  同 PR 递增）；数据面登记见 [data-inventory.md](data-inventory.md) / [game-data-sources.md](game-data-sources.md)。
- **坦克配置器 × BlitzInfo 11.21 跨车型复核（2026-10-27）**：以 11.21 渲染页 DOM +
  `data/wg/11.21/tanks.json` 对照 10 辆不同车型，修正：乘员角色兜底（无该角色不再 ×1.05）、
  俯仰符号（客户端存 `-up down`，此前上下颠倒）、车体转向（当前/初始发动机功率比 ×
  初始/当前**整车**重量；重量补电台/油箱）、无炮塔 TD 隐藏炮塔装甲列。对拍：整车重量与
  车体转向 725/732 辆一致（余 7 辆是 BI 默认顶配选炮不同，非公式差）。**数据面同批修正**：
  `tank/{id}.json` 逐配置 `caliber` 改取该炮**弹表自带弹径**（全车 421 个配置此前与自身
  弹径矛盾），738 份重生成，本地包 `tank/` 与 manifest 已刷新。**影响面**：WotbTools 参数值
  + 装甲查看器火炮口径显示；资产包 `tank/` 变化（发布新 release 后可见），WASM pin 不变。
  详见 [wotbtools-research-loadout-plan.md](wotbtools-research-loadout-plan.md) §12.2.16。
- **坦克配置器 × BlitzKit 数值对照补齐（2026-10-27）**：用户改口「BlitzKit 只是隐蔽
  系数算得不太准，其它的可以参考」。逐项对照 `tankCharacteristics.ts` 后新增：**履带
  维修时间**（客户端 `hysteresisHealth / healthRegenPerSec`）、**车体尺寸**（底盘+车体+所选
  炮塔碰撞盒并集、不含炮管）、**弹匣总伤**、**初始炮塔 pitch 计入有效俯仰**（4 辆意系
  AT-SPG）；肾上腺素改按客户端 filter 的 `clip` 排除。**仍不照抄 BK 的四处**：无角色兜底
  1.05（BI 实测 1.0）、火药补给 +30%（客户端 script 35%）、整车重量漏电台/油箱（BI 对拍
  含）、124 号装备（11.21 客户端 24 件 100–123，不存在）。**隐蔽五状态**：静止/移动/
  静止开火之外补 移动开火 与 起火。**装备联动与 UI 逐项**（101/118 消耗品计时徽标、
  隐藏属性装备标注、tooltip 占位符格式化、弹种明细补穿深衰减/弹速加成、射界图径向翻转、
  `normalization` 改"转正角"、前后对比 BK useDelta 同逻辑、炮候选全集置灰、4 列矩阵模块区、
  火力行去重、i18n 键路径修正）见
  [wotbtools-research-loadout-plan.md](wotbtools-research-loadout-plan.md) §5.2、§7 #12、
  §12.2.20。**数据面**：导出器补车辆级 chassis/hull 碰撞盒 + 逐炮塔盒 +
  `turretOrigin`/`initialTurretRotation`，chassis stats 增 `hysteresisHealth`/`hullPosition`；
  `data/research/` 重生成，WotbTools 快照同步。**影响面**：WotbTools 前端 bundle 变化；
  资产包/WASM pin 不变。测试 targeted 164 / 全量 4822 通过 / 2 跳过。
- **坦克配置器：自动装填有效 DPM + 火炮转向速度（2026-10-27）**：新增 BK
  `dpmEffective` 的 60 s 窗口模型（只对 autoreloader；参数行 `gunEffectiveDamagePerMin`，
  逐发装填 <2 发时 fail-closed 留空）与 `gunTraverseSpeed`（`<rotationSpeed>` × 燃料/炮手
  系数）。**无炮塔车**（客户端 `<hull><fakeTurrets>` 存在 ⇔ `hasTurret=false`，738/738 辆
  与 BI 逐辆一致，114 辆）参数区用火炮转向速度**替换**炮塔转速行。数据面：
  `modules.hasTurret` + gun stats `rotationSpeed` 新导出；**逐车 `modules/{nation}.json`
  随资产包 `research/modules/` 发布**（新坦克/模块数值只需更新资产包）。WotbTools PR #772
  已带 Android `wotbVersion` 3.17.9。详见
  [wotbtools-research-loadout-plan.md](wotbtools-research-loadout-plan.md) §5.2、§12.2.21。
- **资产包发布：配置器全量数据（2026-10-27）**：Alibaba origin 新 immutable release
  **`t11-preview-20261027-research-all`**——research-modules 基座硬链接复制 + 11 个全局
  `research/*.json` 增量；manifest **5911 件 / 4430.0 MB / 0 mismatch**，公网回拉 sha256
  一致、`deploy/assets/verify.sh` PASS。WotbTools 3.17.9 经 assetProvider 读取全部配置器
  数据，`common/research/` 仅图标；新增坦克/物品无需前端发版。
- **资产包发布：预览 XI 车配置器模块（2026-10-27）**：新 release
  **`t11-preview-20261027-research-t11`**——3 件 `research/` 增量（`meta.json` +
  `modules/china.json` + `modules/ussr.json`）；BZ-79（15153）与 Object 432U（27905）
  并入国别文件（合计 740 辆），逐辆预览车带 `gameVersion`、`meta.preview` 记来源；
  manifest 0 mismatch、回拉/verify PASS，`current` 已切至该 release，旧 release 保留回滚。
  导出器新增 `--game-data` / `--bridge` / `--modules-only` / `--merge`（预览增量并入，
  不依赖已删除的 20 GB 解包）；WotbTools 无代码改动即可加载两辆。
- **配置器图标居中 + 英文参数栏修复（2026-10-27）**：`_normalize_icon` 原先把 RGBA 图
  自身作为 `paste` mask（边缘 alpha 二次相乘消失，32/102 张内容框偏移），改
  `alpha_composite` 后重新生成全部图标，新增 `tools/test_research_icons.py` 几何门禁；
  乘员技能图标改 alpha>10 可见内容框裁切再居中。英文 `agentTanks.pen_far` 的
  `"Pen @ far"` 触发 vue-i18n linked-message 编译错误（切 English 参数栏整块消失），改写
  并新增三语消息全量编译门禁。技能帧修复最终由 WotbTools **3.17.11 / 3017011** 承载。
- **装甲厚度序列化精度（2026-10-20）**：`tank/{id}.json` 的 plate/track 厚度清洗从
  0.1mm 改为 0.01mm：去掉 f32 尾差（62.4000015 → 62.4）的同时保留 6.35 等真实板值
  （0.1mm 会把它舍成 6.3，HE/判定少 0.05mm）。738 辆 `data/tank_data` 已重生成，
  包内 `tank/` 与 manifest 同步、COS 已按 `--only tank/` 差分上传。
- **地图加载正确性（2026-10-18，导出器 + 消费端）**：`scenery.glb` 新增语义 extras
  `sky`（客户端 `Skyobject` 材质）与所有 BLEND 材质的 `alphaBlend`；变体映射不完整时
  写 `variantMappingIncomplete`。消费端据此不再按节点名猜天穹、不再把真混合改写成
  不透明+裁切；多变体地图无可用 `map_id` 映射时 fail-closed 隐藏全部变体节点；
  `terrain.json` 缺 `terrainLayout`/`size`/`zmin`/`zmax`/`span` 或 `worldBounds` 非居中
  正方形时 fail-closed 回退 2D（不再按缺省 layout/zmax 解码）；资产 404/解析失败写入
  `assetIssues`/`assetDegraded`，不再静默计为完成。36 图本地 GLB 全量重导已验证：
  每图 1 个 sky 材质、0 个 BLEND 缺 `alphaBlend`、0 个变体映射不完整。
- **运行期 11+1 项数据已全部由本机客户端解包 / 自产承接**（含 `tanks.pb`/`models.pb` 与
  封面图换源转正）：外部依赖只剩 WG 战绩 API（性质不同不可本地化）与 BlitzKit
  "交叉校验对象"角色（两处缺失兜底休眠）。逐项来源 / 替代完成度见
  [data-inventory.md](data-inventory.md) §一、§五；解耦口径见 [decoupling-status.md](decoupling-status.md)。
- **11.22 tier XI 超测预览包（2026-10-22，非现役数据面）**：公开 CDN 的
  `WotBlitz_11.4.0.119_t11b.zip`（`version.txt` = `11.22_supertest_tier11_4`，20.67 GB，
  支持 Range）完整实装两辆 XI 车：中国 **BZ-79**（15153）与苏联 **Object 432U**（27905）；
  科技树另登记 6 个 XI 节点，但无车辆定义/主模型。独立预览包 740 辆（11.21 的 738 + 2，
  含 `tank/`、GLB、图标、`game_data/`、悬挂/履带印）+ 30 件增量已发布为
  `releases/t11-preview-20261022`，其后由 `-destructibles` 与 research 系列取代
  （旧 release 均保留回滚；本地验收仍可走 `?assets=http://127.0.0.1:8124`）。
  来源、口径与注意事项（超测属性、图标占位、BZ-79 弹种 `<kind>` 冲突）见
  [game-data-sources.md](game-data-sources.md) §2.7；预览包不进入 11.21 运行期数据面。
- **游戏版本**：`release/11.21.0`，车辆 **738**（`data_version.json` 为准）。随版刷新链
  （pb / tank_cache / game_data / 图标 / GLB / 悬挂 / 履带印 / shellKinds）与新增
  collector 车清单见 [data-inventory.md](data-inventory.md) §一。
- **可破坏物 `(cell,slot)` 联表修复（2026-10-22，资产管线）**：`parse_lka` 只认
  `er*` 分段表，8 个多变体 space 回退遗留主表、实体 id 复用错联（desert_train 178 棵树
  中 27 棵指向非树物体）。改为**通用前缀发现 + 按 map_id 序数选本变体分段表**，加结构
  自检（不满足 fail-closed）。7 个映射确有差异的变体物化独立 `map/<key>/`（共享资产
  硬链接），其余 12 个变体共用基础目录。已发布新 release
  **`t11-preview-20261022-destructibles`**（manifest 0 mismatch、verify PASS），WotbTools
  无代码改动。
- **资产包与资产 origin**：本地 11.21 包 **5906 件 / 4418.5 MB**（`release/asset_pack/`，
  manifest 逐文件 sha256；含 7 个变体目录）。生产 origin 已迁移到
  **`https://assets.wotbtools.com`**（Alibaba `ali-sh-asset-01`，
  `/srv/wotb-assets/releases/<pack-id>` + `current` 原子软链）；当前线上 release
  以 [data-inventory.md](data-inventory.md) §2.1 为准（本页不再另行维护副本数字，
  此前 20261027 条目的 5911 件 / 4430.0 MB 已被 20261029 科技树 release 取代）。
  发布按「新 release 硬链接复制 + 增量覆盖
  （`cp --remove-destination` / `os.replace` 断链）+ 主机重算 manifest + 校验 +
  原子切 `current`」；**不要再走 COS 根目录全量上传**——COS 仅保留旧 APK 与临时回滚，
  本次未动。发布链、增量发布与运维陷阱见 [data-inventory.md](data-inventory.md) §2.1
  与 [game-data-sources.md](game-data-sources.md) §5.2；历史 COS 同步记录保留在版本条目中。
- **俯视烘焙（均衡档底图）**：渲染件必须与线上同代（世代护栏：meta `ground` 键判据，
  旧世代 `--write` 拒写）；"先归档再合成"的次序约束与重烘命令见
  [game-data-sources.md](game-data-sources.md) §5.5。
- **2D 战术底图换源（2026-10-16）**：29 张底图（web 与 Android 共用的 canonical）改由包内
  `ground.webp` 派生，冻结配方 + 取向由契约确定（无需翻转），工具 `tools/export_2d_basemaps.py`
  （`--verify` 全量重算；与用户验收版逐字节一致），合计 21.50 MiB；**Web 与 APK 共用
  同一份字节**（不再派生缩小副本）。同时产出**资产 origin 免发版通道**：包内
  `map2d/<code>.webp` + `map2d/index.json`（逐图 sha256 判据），App 侧"包内兜底 + 在线
  校验 + 离线可用"。链条、次序陷阱与纪律见 [game-data-sources.md](game-data-sources.md) §5.6。
- **坦克 GLB：`hide_elements` 非激活形态保留在备用 scene、默认不显示（2026-10-17）**：
  10625 CC1 Mk.2 的 `turret_01_nc_skin` 是**传奇涂装的部分组件**（不是完整独立形态）。
  默认 scene 只挂激活态；非激活形态保留在 glTF 备用 scene **`wotb_inactive_states`**
  （scene index 1，节点 `extras.wotb.defaultVisible=false`）——**默认不显示、数据不丢**。
  **92 辆**（173 个多状态容器）重导，包 4201.0 MB、manifest 重算并发布。口径、脚本与
  受影响集见 [local-model-export.md](local-model-export.md) §5.1。
- **装甲查看器：逐炮塔/逐主炮 plates 透出（2026-10-17）**：10625 CC1 Mk.2 的 stock
  `turret_01` plate 11 = **130 mm 主装甲**；top `turret_02` plate 11 = **30 mm 间隙甲**。
  `tank/{id}.json` 的 `configs[]` 新增 additive `turret_plates`/`turret_spaced`（逐炮塔）与
  `gun_plates`/`gun_spaced`（逐主炮，均来自 models.pb）；顶层 `armor_model` 仅保留为
  tank 详情卡的顶级摘要。738 份重生成并发布。WotbTools `tankViewer.js::getPlateThickness`
  对 turret/gun 只读当前 config 的 plates，字段缺失按 0/空 **fail-closed，不回退
  `armor_model`**；切 config 时 `refreshConfigArmorThickness` 重算。数据字段 additive。
- **SpeedTree 静态叶过亮修复（2026-10-18，导出器 + WotbTools 前端）**：客户端
  `speedtree-materials-vp.sl:236-285` 的 SPHERICAL_LIT 净染色 =
  `0.282094 × SH(L0) × vertexOcclusion × shOcclusionMult`。旧导出把 `st_tint` 写成 SH
  字面值（√π = 1.7725），且静态组（`pivot.w=0` 固定叶）整批丢遮挡 ⇒ 消费端 `vOcc=1`，
  叶净值 1.7725 vs 客户端 COLOR_0（叶均值 0.722–0.733），约 **2.4× 过曝**（**57.8% 可见
  叶像素近白**）。修复：`st_tint = 0.282094×SH×shOcclusionMult` + **所有 ST 组静态网格随导
  COLOR_0** 并按客户端 AO 烘焙。36 图全量重导（15 图字节变化，`--only map/` 511.7 MB
  局部发布）；WotbTools `makeBillboardMaterial` 以 `occMean=1` 哨兵识别新包、去掉 1.25
  亮部上限；旧包路径不变。全库 1284 个 ST 网格均带 COLOR_0，DesertSands 棕榈叶
  viewer/client 净值逐通道一致。影响面：全部 speedtree 批次（28,411）及消费方；WotbTools
  侧属 scene-runtime，PR 需按 Android version rule 同步 `wotbVersion`。回归：
  `tools/test_export_speedtree_ao.py` 6 项 + WotbTools `sceneryMaterials` 78 项。

### 回放切面（`PlaybackData` v2 / `AiReviewFacet` v1）

切面版本不变、增量字段一律 **additive + 缺省省略**；证据缺失 fail-closed（Option /
哨兵保留 raw，禁猜）。当前能力面（详情见 [总集 §八](回放与射击逆向总集.md#八提取链消费地图字段级回退与质量标记)
与 [契约 v2](replay-contract-v2-supremacy-type39.md)）：

- **车辆集合（2026-10-20 修正）**：`vehicles[]` 的入选判据 = **Type5 class=2 实体类型
  ∧ 结算花名册联表成功 ∧ 有 type=10 位姿**（协议权威实体种类；class=3 区域可破坏物、
  class=8 炮塔残骸均非车辆；未进入花名册的观察者/中途离开实体不渲染——用户口径
  "只保留结算花名册中出现过的"）。旧口径 `st10 ∧ prop2` 双流交集把"炮塔流"当成车辆
  身份证据——车辆全程不转炮塔时整场没有 prop2，被误判为非车辆：用户样本
  `20261019_0237`（Copperfield 训练房，作者 Grille 15 vs 对手 SU-130PM 静止未瞄炮）
  旧口径只输出 1 辆车，对手实体 `0x10cddb90`（709 条 type=10 位姿、0 条 prop2）
  整条丢失，3D 回放无法显示。反向错误同样存在：旧"prop2 全局缺失 → 仅 st10"退化会把
  无 Type5 class 的相机实体当成第二辆车（`20261010_1212` 训练房样本，相机流位置随后
  跳到 `(0,0,0)`）。现口径同时移除 20 采样阈值（稀疏采样不是"非车辆"的证据）。
  影响面：`vehicles[]` 集合、由候选集派生的 `visibility[]` 与全场时间轴
  （`meta.t_start/samples/duration`），以及 `shots[]` 的射手/目标联表兜底；切面形状不变，
  WotbTools 3D 回放随 pin bump 直接受益，无消费方适配。
- **车辆配置**：`config_idx` / `burst_size` / `turret_local` / `gun_local`（comp blob 证据链）；
- **位姿与弹道折线**：`pose_kf`、`shots[].via`/`leg_secs`、`hull_roll`；
- **`pose_kf` 段首显示重建（2026-10-18）**：客户端 `setFilterOnEntity` 复位后 latency 从
  0.02s 缓慢收敛到 0.2s，段首 2~5s 的逐帧输出是 `保持→整段跳变` 阶梯（本次逐帧探针复现：
  某 AoI 段首 2s 内 **57/119 帧静止**，p90 0.58 m/帧；喂入策略 `全部已到样本` 与
  `只喂最新` 逐帧一致，排除输入侧）。游戏内这段阶梯被模型加载/入场遮挡，我们的查看器
  模型常驻，表现为"对方坦克刚进视野一顿一顿"。渲染折线改由
  `SegmentedPoseTimeline::build_display` 生成：每个在场段按 10Hz 标称间隔铺满 8 槽、
  latency 从稳态 0.2s 起步——段首仅 0.2s 静止，其后逐帧匀速（样例段首 0.4~1.5s 位移
  0.1666~0.1667 m/帧，原客户端语义 37/60 帧静止）。**判定锚点 / 炮口 / 命中位置仍走
  `SegmentedPoseTimeline::build`（0.02s 复位，WI 对齐不变）**；10Hz 网格列
  `pos/hull_yaw/hull_pitch` 未改。同时修复短隐藏段（0.1~0.5s）下前段滤波尾部的 +0.5s
  外推越过下一段 `t_in`、导致 `pose_kf` 时刻非单调的问题（有界段在 `t_out` 截断，消费端
  二分/插值不再跨隐藏段滑移）。波及：`pose_kf` 消费方（WotbTools 2D/3D）自动变平滑；
  WotbTools 侧炮塔/炮管/侧倾网格采样同步改为段内采样（`sampleChannelWithin`，按 playback
  `visibility[]` 协议精确 AoI 段；短隐藏段在 `coverage` 中不断开，故不能用 `coverage`）；
  契约字段形状不变、无新增字段。
- **血量与伤害**：`damage = 服务器结算值`（method8 尾部 u16；未结算时降级 HP 链并置
  `dmg_unattributed`）；`hp_raw`、prop3；**`hp_state`（additive、`damage`/`health` 事件恒在场：
  `current`/`zero`/`terminal`）= 血量编码的唯一权威分类**——消费方判死亡面读它，禁自行解释
  u16（规则与 `hp_terminal_normalized` 同判；分歧实例、消费方义务与实测证据见
  [wotbtools-cross-reference.md §七](wotbtools-cross-reference.md)）。**v0.4.9 起随发布产物在场**：
  消费方须 pin bump 后消费（缺省路径保持 legacy：仅 {0x0000, 0xFFFD} 终态）；
- **动态装甲**：`armor_states` / `shots[].target_armor_state`（propID 16，动态车 6 辆）；
- **复活模式**：`death_times`（≥2 死才输出）/`resurrection_times`（契约 §8；绝对血量
  回升前提待真实样本验证，未验证时字段缺省 = 现状行为）；
- **独立炮塔**：`detached_turrets`（契约 §7）；
- **可见性/装填/可破坏物/争霸**：`visibility[]`（AoI 段 + pose 种子）、`reloads` +
  `reload_effective`、`destructible_areas/events`、`supremacy_bases/points`（契约 §1/§2）。

### 跨仓协作现状与未完项

- 本项目是 WotbTools 的**上游回放解析器**（其服务端解析器已整体删除）；协议/契约问题
  在本仓先修，对方再更新 pin。消费方式、pin bump 与 golden 重生成、Android `wotbVersion`
  同步规则见 [AGENTS.md](../AGENTS.md) 与 README。
- 未完项（消费侧接入归 WotbTools 仓，落地状态以对方仓为准）：
  - **血量终态族消费（2026-10-16 新发现，见 [cross-reference §七](wotbtools-cross-reference.md)）**：
    canonical `hpRawState` 只认 {0x0000, 0xFFFD} 为终态，−1/−2 档死亡被判成存活（2D 血量冻在
    最后正值、3D 装填条继续播放；实例：1910 局 Intotherainy 0xFFFE）——pin bump 时改读上游
    `hp_state`（或扩全族），并给终态样本补 DESTROYED 迁移与装填条 f2=5 取消语义；
  - 复活轨道字段的消费（scene fallback 与 canonical `lifeTransitions` 支持 DESTROYED→ALIVE 回转）；
  - 地图 scenery extras 的消费接线（`decalTexture` / `decalSeparateLm` 等，后端数据已随包）；
  - **履带逐配置装甲/模型适配（2026-10-17 待办）**：`armor_model.chassis` 仍只透出顶级底盘
    `left_track`/`right_track` 厚度（models.pb top track）；`models.pb` 的
    `TrackModelInfo.tracks` 已有逐 module `thickness`/`origin`，但尚未进 `tank/{id}.json`，
    查看器 chassis 仍用顶级摘要。当前扫描 **6 辆** stock/top 厚度不同：4369 Pz. III（10/15）、
    5729 Ju-Nu（20/25）、12081 BZ-68（30/20）、18305 SDP 40 Zadymka（15/20）、
    21569 ERAC 105 Proto（15/20）、21825 Projet Louis（15/20）；另 **6721 BDR G1 B** 仅
    origin 差约 13 mm（厚度同为 20）。可视 GLB 只有一组 `chassis_track_L/R`，当前数据没有
    按履带模块的第二套网格；若客户端确有不同履带外形，需按模型语义另行确认。待办：逐 module
    透出 thickness/origin 并让查看器按当前底盘选；视觉几何差异另行确认。
  - 对方点名要的字段清单（`PlaybackData.damages[]`、`coverage`、`finish_reason`、
    `unsupported_damage`）以 [wotbtools-cross-reference.md](wotbtools-cross-reference.md) §五
    为单一维护点，本页不再复制。
- 解析侧遗留未定项（禁猜、封存）以 [总集 第五篇](回放与射击逆向总集.md#第五篇-遗留未定项) 为准；
  前端侧未做项（自动曝光、水面 HIGH/ULTRA 反射、雾=非目标等）见
  [map-render-align-audit.md](map-render-align-audit.md)。

## 约定

- 逆向结论的**唯一权威**是 [docs/回放与射击逆向总集.md](回放与射击逆向总集.md)；其余文档与其冲突时，
  先查 [wotbtools-cross-reference.md](wotbtools-cross-reference.md) 是否已有裁决。
- 历史推导过程不在工作区文件中保留，需要时查 git 历史。
- `examples/`（逆向探针脚本）与 `tmp_*/`（分析转储）不入库，文档中提及处仅作证据出处记录。
