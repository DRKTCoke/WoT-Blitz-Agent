# 数据面清单：运行所需数据总账（来源 · 可替代性 · 完成度）

> 逐项回答运行期每份数据的**目前来源**、**是否存在可替代来源**、**替代完成度**；
> 另附分发渠道、包内布局、跨领域接线护栏与本地提取工具索引。
>
> 替代完成度刻度：`✅ 已接线`（本机解包即现役来源）· `🟢 数据就绪未接线` ·
> `🟡 有缺口` · `⚪ 阻塞`（需决策）· `➖ 无需替代`。
>
> 姊妹篇：[game-data-sources.md](game-data-sources.md) 讲"每份数据从哪来、能否脱离
> BlitzKit"（逐字段来源映射、spaced 规则、DVPL 契约、资产 origin 发布流程与陷阱——
> Alibaba 为主、COS 为 legacy）；本文讲"谁在用、谁维护、怎么分发"。两份文档共享同一套
> 来源口径，改动其一时须同步另一份。

## 一、运行期核心数据（11 项）

运行期（Web / 3D 查看器 / 回放 / CLI）真正会读的 11 项。数据层 = 本机解包产物覆盖度与
对照结论；接线层 = 代码与分发链的切换状态。提取 / 维护工具索引见 §六。

| # | 数据 | 目前来源 | 可替代来源 | 替代完成度 | 剩余工作 / 备注 |
|---|---|---|---|---|---|
| 1 | `data/tanks.pb`（整车数值：弹种/火炮/HP/机动/模块） | **本机客户端解包编码（现役）**：`tools/extract_vehicles.py` → `tools/emit_vehicle_pb.py` 编码**同格式 pb**（+补充表 `local_pb_supplement.json` 沿用 2 条 BK 自建名） | 它本身就是；BlitzKit CDN 降为对照校验源（`fetch-blitzkit` 保留） | ✅ 数据/接线 100% | 与 BK 全量对照 = **122 处且全部定性**：119 枪序研发序口径 + 1 处 `10625` explosion_radius（**BK 错值**）+ 2 条名字（客户端两源皆无，补表兜底）。换源后 `armor_model.gun` × `game_data` 矛盾 15→0；`tank/{id}.json` 字节级差异 118 辆 = 18 辆实体（17 辆顶级炮板集/族修正 + 10625）+ 100 辆仅 ±0.0 符号零（IEEE 等值，无行为影响）。**字段口径（2026-10-26 核）**：`field10` = **车体 HP**（总 HP 由 `tank_cache`/`tank_data` 按车体+顶炮塔合成，见 [game-data-sources.md](game-data-sources.md) §2.2）；`field31` = `hull.weight`，**不是战斗重量**（客户端战斗重量为车体+所选模块求和，见 [client-research-tech-tree-loadout.md](client-research-tech-tree-loadout.md) §2.5） |
| 2 | `data/models.pb`（逐板装甲/spaced/履带厚/模型原点/包围盒/限位） | 同 #1（同批换源，编码器同） | 它本身就是 | ✅ 数据/接线 100% | 按**游戏研发序**选档（BK 为模块 id 升序：105 辆枪序/92 辆末位主炮不同，其中 15 辆板集与 game_data 实质矛盾，换源即修复）；`initial_turret_rotation` 客户端直源逐值一致；43 辆无炮塔 TD 的炮塔 bbox 为**空心占位**（两侧皆空，见 [game-data-sources.md](game-data-sources.md) §2.3）；履带厚度/原点已改**逐底盘 map**（C1 塌缩修复，消费端按顶级底盘语义选取，见 [decoupling-status.md](decoupling-status.md) §2）；**炮管 maskSlice 完整记录**（pb 只有 `planePosition[1]` 标量，装不下 normal/axis/modes/锥台）另出 sidecar `data/gun_mask_slices.json`（478 条，键 `tank_id:turret_local:gun_local`）→ 包 `data/gunMaskSlices.json`（打包器 fail-closed）；该记录是**视觉/特效切片参数、不是装甲边界**——viewer 炮管裁切仍按 BlitzKit 的 `gun_mask` 标量口径，不读 sidecar（见 [index.md](index.md) 2026-10-28） |
| 3 | `data/tank_cache.json` | 本机自产（派生自现役本机 pb；俯仰 ← models.pb；六面装甲摘要 ← game_data 客户端优先） | 同一派生链 | ✅ 随 ①② | 消费方读取字段清单见 §1a |
| 4/5 | `data/game_data/{id}.json`（装甲模型 + 碰撞盒） | **客户端解包**：`XML/item_defs/vehicles/{nation}/{model}.xml.dvpl` + `3d/Tanks/Parameters/{nation}/{model}.yaml.dvpl` | 它本身就是 | ✅ 100/100 | 链内 2 处 pb 依赖：文件定位用 pb 的 `model_name`（field32）；炮塔/炮管碰撞盒**选段**用 models.pb 的 module→node 映射（缺失回退"末位"） |
| 6 | `data/tank_data/{id}.json`（→ 包 `tank/{id}.json`） | **本机 pb（models/tanks）+ game_data**——全链客户端解包 | 它本身就是 | ✅ 数据/接线 100% | 顶层 `shells` **直接引用末档配置弹表的同一份 JSON**（结构性单源化，字段集天然同步）；`configs[]` 节点数/索引走客户端 GLB ✅；`configs[]` **additive** `turret_plates`/`turret_spaced`（逐炮塔）与 `gun_plates`/`gun_spaced`（逐主炮，均 models.pb；`gun_thickness` 已有）；顶层 `armor_model` 仅保留为详情卡顶级摘要，查看器按当前 config 取 plates、缺失 fail-closed 不回退（见 [index.md](index.md) 2026-10-17 装甲条目）；43 辆无炮塔 TD 的 `collision_boxes.turret` = null（两侧都只有空心占位，非缺数据）；plate/track 厚度序列化精度 **0.01mm**（`round_armor_mm`，保留 6.35；旧 0.1mm 会生成 6.3）；`collision_boxes` 是 Parameters YAML 的部件代理盒（hash6 量化框），**不是** `collision.glb` 的网格 AABB——两者不可互换（换框审计首发板 488→457、部件 577→502，见 [game-data-sources.md](game-data-sources.md) §2.4）；**`configs[].weight` 继承 pb field31 = `hull.weight`，不是整车战斗重量**（见 [client-research-tech-tree-loadout.md](client-research-tech-tree-loadout.md) §2.5）；`configs[].burst_group_count` / `burst_group_interval` = 连发组（PomPom）时序——pb gun field 20（客户端 `<burst count rate>`，interval = 60/rate），**只给弹夹/连发炮的 DPM 用，不参与 is_burst 判定**；非弹夹炮也带字段但计算走 regular |
| 7 | 弹种反解表（全局 shell id → 弹种数据；包 `data/shellKinds.json`） | 本机自产（`ShellKindTable::from_tanks_pb`；CLI `dump-shell-kinds --out data/shell_kinds.json`，输入为现役 pb 且**必须与 pb 同批重导**——打包器 fail-closed） | 客户端 `shells.xml <kind>`（4 值闭集，提取器与编码器均已支持） | ✅ 数据/接线 100%（消费方运行期直读，原内嵌件已退役） | `atgm_heat`→HOLLOW_CHARGE 验证一致；坍缩按 tank_id 升序取首个（确定性） |
| 8 | `data/cache/models/{id}/*.glb` | **客户端解包（现役）**；BlitzKit CDN 退为缺失兜底（`web/assets.rs::ensure_glb_bytes` / `wargaming/model_fetch.rs`，休眠） | 它本身就是 | ✅ 100/100 | 738 辆（glb/ 1476 件）；与 BlitzKit 逐字节一致在 **111 辆上刻意打破**（静态变换烘焙 / StateSwitcher 状态过滤，2026-10-10 起 23 辆、2026-10-17 扩到 111 辆，见 [local-model-export.md](local-model-export.md) §5.1）；11.21.0 另按新客户端重导 3 辆新车与 2 辆碰撞变更（24945/8225） |
| 9 | `data/cache/tank_images/{id}.webp` + `crops.json` | **本机客户端解包**：`tools/export_tank_icons.py`（DVPL 剥壳裸 webp、不重编码；逐车 `iconPath` 声明 738/738 命中）→ `local_tank_icons/` → 同步进 `cache/tank_images/`（两目录逐字节一致）；BlitzKit 退为缺失兜底（休眠） | 它本身就是 | ✅ 数据/接线 100% | 与 BK 同源同画：alpha 轮廓 100% 一致（BK 为二次重编码 + 更紧 1px 裁剪，RGB 均差 1.79 / max ≤17）；**展示裁剪**：`--emit-crops` 输出逐车内容框（alpha>0）→ 包 `tank_images/crops.json`（打包器 fail-closed），消费方展示层按框缩放（不改图标字节） |
| 10 | `data/cache/maps/`、`data/cache/terrain/` | **客户端解包** `.sc2`/`.scg`、heightmap、colormap；地面再经 `tools/composite_overhead.py` 叠俯视合成（渲染件 `release/overhead-bake/` 须与线上同代，见 [game-data-sources.md](game-data-sources.md) §5.5） | 它本身就是 | ✅ 100/100 | 高度图缓存 `{key}.hm.c2.u16.bin`（terrainLayout 2 布局自描述）；小地图按 space **注册表首条目**选取；11.21.0 随版更新 mountain 高度图/小地图，36 图 scenery 全量复导对齐 |
| 10b | 2D 战术底图 canonical（29 张 1254² WebP → WotbTools `frontend/src/assets/maps/`） | **本仓自产**：包内 `map/<key>/ground.webp`（4096²）→ `tools/export_2d_basemaps.py`（冻结配方：对比 ×1.17 / 饱和 ×0.9748 / USM B，全 29 张同一套） | 不适用（既非客户端资产也非 BK；旧批曾用外部 AI 增强件） | ✅ 29/29（`--verify` 全量重算；与用户验收版逐字节一致） | 合计 **21.50 MiB**（q90 m6）；**Web 与 APK 共用这一份字节**（Android 不再派生，2026-10-16 起）；取向由契约确定、无需翻转；**资产 origin 免发版通道**（包内 `map2d/` + `index.json` **schemaVersion 2**：逐图 sha256 + 几何契约 `worldBounds`/`orientation`，在线校验 + 离线回落包内）见 [game-data-sources.md](game-data-sources.md) §5.6 |
| 10a | `data/cache/maps/{key}/destructibles.json`（可破坏物清单） | **客户端解包** `.sc2` + `XML/destructibles.xml.dvpl` | 它本身就是 | ✅ 100/100 | 消费方与回放切面 `destructible_events` 联表（总集 §5.4）；**55 键**（36 基础 + 19 出生点变体；打包器物化 7 个映射有差异的变体目录（其余变体共享基础目录））；共享类型库 `typeTableSize` 随客户端走（11.21.0 = 941） |
| 11 | `data/data_version.json` | 本机自产（`game_version` 取客户端 `Data/version.txt.dvpl`；`tank_count` / `game_data_files`） | 它本身就是 | ✅ 100/100 | 现值 `release/11.21.0` / 738；时间戳仅 `tank_cache_updated_at` / `game_data_updated_at` |

来源分布（**全量客户端化后**）：**11 项全部客户端解包/本机自产**；BlitzKit 仅保留
"交叉校验对象"角色（`fetch-blitzkit`）与两处缺失兜底（GLB `web/assets.rs:113`、
图标 `web/assets.rs:233`，均休眠）。

**弹种权威口径**：`tanks.pb` field9（客户端 `shells.xml <kind>` 语义枚举的翻译，
0=AP / 1=APCR / 2=HEAT / 3=HE）是弹种判定权威，field7 的 `icon` 词表降为回退——
#3 的 `shells[].shell_type_id`、#6 的 `tank/{id}.json` `type_id` 均透出该 id；
运行期 `PenetrationRequest` additive `shell_type_id`，`resolve_shell_type` 三级优先
（id → icon 词表 → HE）；另 additive `require_primary`（默认 false = BlitzKit 末层语义，
射击复现传 true：无 hull/turret/gun 的链不再给 PEN + 满伤）。

**枪序与弹药列表口径**：顶塔主炮列表按**客户端 XML 文档序 = 游戏研发序**（末位 =
顶级主炮）导出——BK 的 models.pb 为模块 id 升序，不成立。车辆的弹药列表以**共享炮定义**
（`components/guns.xml` 的 `<shots>`，弹道与穿深只在那里）为权威，车辆内联 `<shots>`
是价格/可用性叠加，按共享列表迭代、内联同标签条目合并覆盖。

### 1a. 消费方从 `tank_cache.json` 读的字段

资产包里的 `data/tank_cache.json` 是百科的**列表/概要**数据源。消费方当前读取：

| 字段 | 说明 |
|---|---|
| `name` / `tier` / `nation` / `type` / `hp` / `hp_top` | 行展示与筛选、排序。`hp` = 车体 + 顶炮塔（基础）；`hp_top` 仅 XI：基础 + 升级树 `ATTRIBUTE_NAME_MAX_HEALTH` 节点（顶级配置）。卡片优先 `hp_top`，`tank/{id}.json` 同名字段 |
| `is_premium` / `is_collector` | 车型标记。**两者是 `tanks.pb` field13 的同一枚举**（1=金币 `is_premium`、2=收藏 `is_collector`），故互斥——实测无"同时为真"。WotbTools 据此给卡片上不同颜色边框 |
| `shells` | 弹种表（`penetration`/`damage`/…）。**取顶级炮塔 × 顶级主炮**（枪序与弹药列表口径见 [game-data-sources.md](game-data-sources.md) §2.3），消费方由此派生 `pen_max` |
| `shells[].shell_type_id` | 弹种权威 id：tanks.pb field9 翻译，1=APCR / 2=HEAT / 3=HE；空值仅出现在 AP 系（proto3 零值省略 → 按 0=AP 或 icon 回退，语义一致）。判定弹种（跳弹角/溅射分支）以此为准，icon 串仅作显示 |
| `shells[].caliber` / `normalization` / `ricochet` | **等效厚度判定输入**：弹径（mm）/ 转正角（°）/ 跳弹临界角（°），2079/2079 全量有值。装甲查看器按弹算 `eff = t / cos(angle − 转正)`，两倍口径规则还会放大转正角（`1.4×norm×caliber/(2t)`）——**缺失即按 0° 兜底 = 判定被静默削弱**。包内 `tank/{id}.json` 顶层弹表与 `/api/shells` 同步同字段（[game-data-sources.md](game-data-sources.md) §2.3）；顶层表与 `configs[]` 同一份 JSON，字段集结构上不可能再漂移 |
| `armor` | 六面装甲摘要（**顶级**炮塔档位，见 [game-data-sources.md](game-data-sources.md) §2.3） |
| `view_range` / `turret_traverse_speed` | 视野 / 炮塔转速，**顶级**炮塔 |
| `speed_forward` / `speed_reverse` / `hull_traverse` / `gun_depression` / `gun_elevation` | 机动与俯仰 |

> 派生字段（`pen_max`）由消费方自行聚合，**不在本仓生产**：前端 `tankopediaQuery.js`
> 取 `shells[].penetration` 的最大值。所以弹种取哪一档炮会直接影响列表卡片的穿深与排序。

### 1b. 派生 / 注入 / 支撑数据

不是独立来源，而是由 §一 各项再加工、或注入消费方的产物——**间接依赖最容易漏记**：

| 数据 | 目前来源 | 可替代来源 | 替代完成度 | 剩余工作 / 备注 |
|---|---|---|---|---|
| `data/local_pb_supplement.json`（编码器取数） | 客户端无对应概念/数据、沿用既有 BlitzKit 值的字段（集中登记，不假装它们是客户端数据） | 逐项消除：`dev_name` 客户端无此概念——**保留为提取链匹配键**（`game_extract.rs::resolve_vehicle_file` 按它解析客户端车辆 XML/YAML；删掉会失配/误配）；名字仅剩 **2 条**（1 枪 + 1 履带，两种字符串源都没有） | ✅ 编码器消费（dev_name 738 / 名字 2） | 由 `tools/emit_vehicle_pb.py --emit-supplement` 生成，随游戏版本人工维护 |
| 包 `data/tank_names.json` ＋ WASM 注入表 `tankNamesJson` | 打包器从 tank_cache 抽（现为本机 pb 携带的客户端名） | 客户端名：随包基础快照 ∪ **运行时本地化覆盖层**（两源合并后仅差 2 条） | ✅ 已接线 | 覆盖层为 per-user 运行时缓存（需客户端登录同步过）；2 条残差由补充表兜底（[decoupling-status.md](decoupling-status.md) §2 C2） |
| WASM 注入表 `limitsJson`（炮管俯仰锚定，**基础极限**） | 包 `tank/{id}.json` 的 `pitch_limits` ← 现役 models.pb | `local_pb` 的 `pitch_limits`（零差异） | ✅ 已接线 | 接线随 #2；实体实际携带的 122 俯仰配件由 core 按同 eid 的 Type5 `0B` 配件串叠加（+4°仰/+3°俯），注入表不得预加 |
| WASM 注入表 `shellsJson`（弹种徽标/判定） | 包 `data/shellKinds.json`（消费方运行期直读） | 客户端 `shells.xml <kind>` | ✅ 已接线 | 接线随 #7 |
| `data/tank_id_bridge.json`（738 条 tank_id ↔ 模型名） | 现表由 `tanks.pb` 导出（`--emit-bridge`） | **纯客户端可重建（735/735 一致）**：`tank_id = (list.xml <id> << 8) | 国家基数`（`--emit-bridge --from-client`）；客户端另有 **20 辆** BK 没有的车（教程 bot/超测）可顺带覆盖 | ✅ 能力已验证（`slug` 列无消费者） | 接线时改用客户端源即可自动覆盖新车 |
| `data/armor_states.json`（**6 辆**动态装甲车的状态声明，12 态） | 客户端 item_defs `vehicles/*/<stem>.xml` 的 `<extras><armorsStatesController>`（`tools/extract_armor_states.py` 直出） | 不适用（客户端独有概念，BlitzKit 无对应） | ✅ 已接线（解析器按它门控状态语义；消费方查"弹着时刻激活板集"同源） | 每态 `armors` = 激活的 `<part>.<armor_N>` 集合；初态 = `<default/>` 标记的 state；状态流载体 = **propID 16 `armorsStates`**（`[u8 n][n×4B]`，bit16=激活态；[总集 §2.5](回放与射击逆向总集.md)） |
| 包内 `data/tanks.pb` / `data/models.pb` | **本机客户端解包编码**：`extract_vehicles` + `emit_vehicle_pb`（+补充表） | 可选：从包内移除（改由消费方自行派生） | ✅ 已接线 | 移除会改变包结构——消费方若直读 pb 需同步 |
| 运行期 web 端点（`/api/tanks` 花名册、`/api/tank_filter`、`tank_detail.is_collector`、`/api/shells`、`models_status`） | 运行期**直读 pb**（花名册 / 名 / tier / nation / type / collector / 弹种 / models.pb id 表） | 改读 tank_cache / 本地数据 | 🟢 代码改动小 | 仅本机 web GUI 用（前端已冻结，不进分发） |
| 坦克悬挂数据（包 `suspension/<id>.json`） | 客户端逐车 yaml `suspension:` 块（`tools/export_tank_suspension.py`） | 不适用 | ✅ 已接线（前端求解器消费：逐轮贴地 + 履带形变 + 自转/花纹滚动） | 728 文件 / 2271 KB；字段 = 逐轮 `{flag,a,b}` + 履带静止折线 + 弯曲/铺放系数 + `chassis.textureScale`；逐轮半径/挂点由前端从模型 GLB 现算；剩余未定项（T4）见 [tank-suspension-client-re.md](tank-suspension-client-re.md) |
| 履带印数据（包 `tread/`） | 客户端逐车 yaml `treadParameters` + 13 张 `*tread*.dds` 贴图（`tools/export_tank_tread.py`） | 不适用 | ✅ 已接线（WotbTools `scene/treadMarks.js` 几何贴花） | 738 json（314 authored + 424 国别主贴图回落）+ 13 贴图 + `textures.json` alpha 归一化表 / 156 KB + 983 KB |
| `data/research/`（研发/配置数据面，132 文件 ≈3.80 MB） | **本机客户端解包**：`tools/export_research_loadout.py`（`meta.json` 记 `11.21.0.1235_7376187 release/11.21.0` + 逐文件 sha256） | 不适用（客户端独有；BlitzKit 只作公式/展示参照，不作数据源） | ✅ 数据 100%：738 辆 / 9 国别模块（纯 11.21；预览增量留在 `data/research_t11_preview/`，不并入现役包；含 `hasTurret`（客户端 `<hull><fakeTurrets>` 标记）、gun `rotationSpeed`、gun/chassis `dispersionFactors`、chassis `hysteresisHealth`/`healthRegenPerSec`/`hullPosition`/`maxLoad`（kg）、车辆级 chassis/hull 碰撞盒 + 逐炮塔盒 + `turretOrigin`/`initialTurretRotation`；技能效果取 `XML/item_defs/tankmen/avatar.xml`，弹种图标取 `Gfx/Shared/tank-supply/ammunition`；物品名缺基础 Strings 时用运行时本地化覆盖层（zh-Hans）+ 5 条 en 补充名）、24 设备 + 9 预设、64 消耗品（common 13）/ 85 补给（common 34）、20 项乘员技能（客户端 avatar.xml 的 per-level 效果 + class 归属）、9 张弹种图标、20 张技能图标、params `format` 70 键、en + zh-Hans 46 条文案、5 张模块图集 | **全部非图标数据随资产包 `research/**` 发布**（全局目录/物品/技能/params/strings/camouflage/meta + 逐车 `modules/{nation}.json`；WotbTools assetProvider 加载；新坦克/物品数值只需更新资产包，不需前端发版）；全局图标集留在 WotbTools `common/research/icons/`；不改 WASM pin。科技树坐标 + 整车研发边随 `--with-research-graph` 落盘为 `tech_tree/{nation}.json`（9 文件：304 节点、0 pending、310 条整车边；列≠tier，tier 读 list.xml；随资产包 `research/tech_tree/**` 发布，WotbTools `agent-techtree` 消费）；ru 文案回退 en。口径与消费契约见 [wotbtools-research-loadout-plan.md](wotbtools-research-loadout-plan.md) |
| 数据版本差异计划 / pack 复用 | `tools/plan_data_version.py` v2（全量源清单：目录 sha256、ZIP CRC32+size；plane 分类；tank_id/map space 解析；`wholeTable`/pins；`--strict` fail-closed）+ `tools/apply_data_version.py`（`pack_incremental` 硬链接克隆/同哈希复用/断链覆盖/孤儿删除；`data_version_handlers` 注册表：research、vehicle-assets 逐车 GLB/悬挂/履带/图标、core 整表重算只覆盖变化字节、maps 按 space 重导 + 2D/变体整表） | `change_plan.json` / `<out>.apply.json`；`data_version.json` 追加 `base_game_version`/`new_game_version`/`source_plan_sha256`/`changed_planes`/`coverage`/`scope`/`inputs` | ✅ 差异驱动；车辆资产逐项、core 整表但只覆盖变化字节、地图 2D/变体整表（用户选定） |
| 解包底座（DVPL/DAVA 容器、DLC `packs/` 覆盖层） | 本机自产代码（Rust `dvpl.rs` + Python `tools/wotbtools/`，fail-closed 两端对齐） | 不适用 | ✅ 100/100 | 本地链硬前提：需本机装客户端（`Data/` + `packs/`），游戏版本更新后整链重跑 |
| 11.22 tier XI 预览包（`release/asset_pack_t11r3_r2/`，**独立预览 release**） | 公开 CDN 超测 ZIP `WotBlitz_11.4.0.138_t11r3.zip`（`11.22_supertest_tier11_6`）的 9 辆 XI 车 + 现役 11.21 包（前代源 `..._130_t11p2.zip` / `..._119_t11b.zip` 保留记录） | 不适用 | ✅ 已上线（当前 `t11r3-20261113-r2`，dataset **`11.4.0.138`**/超测服，显式通道；r2 相对 r1 仅 2 个文件——逐弹 `penetration_top`/`damage_top`（点满强化树口径）；r1 为首发全量重建） | **747 辆（738+9）**：既有 5 辆（BZ-79 15153 / Object 432U 27905 / KR-1 28161 / Black Rock 30241 / XM69 Hacker 30497）+ t11r3 新实装 4 辆（`J77_Ho_Ri_4` 10593、`F135_AS_XX_40_t` 23361、`G187_Taschenratte` 30737、`G189_Hirschkafer` 30993）；manifest **6026 件 / 4482.5 MB**；科技树 **312 节点 / 0 pending / 318 边**；弹种表 **2111**（+12）；consumables/provisions **67/91**（common 16/40）；4 个新机制 `vehicleAbilities` = `unguidedRocket` / `externalAmmoModule` / `auxilliaryWeapons` / `shellOverheat`；7 辆既有车 XML 的 `<unlocks>` 研发边重算（属性零变化 → 738 份基线 tank JSON 逐字节不变）；来源与口径见 [game-data-sources.md](game-data-sources.md) §2.7 |
| 11.21.0_1 动态内容槽位包（origin `datasets/11.21.0_1` → `11.21.0_1-20261111-r1`，**已上线**） | 客户端 SmartDLC `s11.21.0_1.yaml`（2026-11-10 发布：dc `7405732/3/4` 贴图 + 活动/offer 文案 + 收藏车 Relic 注册文案） | 不适用 | ✅ 已上线（origin release `11.21.0_1-20261111-r1` 由 11.21.0 release `cp -al` 克隆 + 主机侧 stamp，delta 0 变更，公网回读一致；数据面 = 11.21.0-20261106-r2 的硬链接克隆，**逐字节一致**、同 content_digest `sha256:9198d0ca…`，5965 件 / 738 辆——动态内容本身走客户端运行时通道，不进包；待 11.21.1 客户端数据后充实） | 占位登记：版本选择器显式暴露 11.21 内部微更新槽位；与 11.21.0 的唯一差异是 version.json 身份；见 [index.md](index.md) 2026-11-11 条目 |

引擎产物（WASM Release，`wotb-replay-wasm-*.zip`）构建自 `crates/replay-core|wasm`，**不含
任何 pb**；上表三张注入表（车型名 / 俯仰锚定 / 弹种表）是消费方的间接数据面——它们的
输出（`tank_name`、`gun_pitch` 解码、`shell_kind`/`shell`）都随注入数据来源变化。
俯仰解码的有效极限 = `limitsJson` 基础值 + 同 eid 的 **122 改进型垂直稳定器**加成
（`data/research/devices.json`：+4°仰/+3°俯；回放证据 = Type5 `0B` 配件串）。
`shooter_equipment` / `target_equipment` 的 `improved_pitch_limits` 是同一事实的消费方
提示字段（仅 true 时序列化），不改变 `tank/{id}.json` 的数据面。

### 1c. 非数据类外部依赖

| 依赖 | 用途 | 可替代性 |
|---|---|---|
| WG API（`api.wotblitz.{asia,eu,com}`，`wargaming/api_client.rs`） | 玩家战绩 | ➖ 服务器数据，**无本地替代也不需要**；不落盘、不进分发 |
| 本机客户端安装（Steam `Data/` + `packs/`） | 所有本地解包链的输入前提 | ➖ 必须（`--game-data` / `--game-dir` 可覆盖路径） |
| BlitzKit 作为交叉校验对象 | 定期用本地提取结果与其 pb 做 diff，漂移即告警 | ➖ 保留该角色 |

## 二、分发渠道

共 **三条**，彼此独立：Alibaba 资产 origin（Web 与新 Android 的数据面主力）、COS
（旧 APK 与临时回滚，**legacy**）与 GitHub Release（WASM 引擎）。

### 2.1 Alibaba asset origin（数据面主力）

- origin `https://assets.wotbtools.com`，主机 `ali-sh-asset-01`（47.101.141.111）；
  主机目录 `/srv/wotb-assets/`，`current -> datasets/<default-id>`，dataset 再原子指向
  `releases/<release-id>`（根路径兼容旧客户端）
- 布局与 `release/asset_pack/` **逐项对应**，`manifest.json` 含全量逐文件 sha256
  （在合成/变体物化之后、发布之前用 `--refresh-manifest` 重算，见下）
- **发布 = 新 release + 增量覆盖 + 主机重算 manifest + 原子切 current**：
  `cp -al <current-release> <new-release>` 先硬链接复制（不重传全包），把变更文件
  `scp` 到主机临时目录后用 `cp --remove-destination` 覆盖（**必须断链**；直接 `cp`
  会改写上一 release 共享的 inode），在主机上重算 manifest（排除 `manifest.json` /
  `delta.json`）并逐文件复核 0 mismatch，最后 `ln -s` + `mv -T` 切 `current`。
  旧 release 保留回滚；`delta.json` 记录相对 base 的变更。
- 写入门户 = SSH 密钥 `~/.ssh/wotb_asset_upload`（用户 `admin@47.101.141.111`）
  —— **密钥不入库、不写进任何被跟踪的文件**
- **不要再用 `tools/upload_asset_pack_cos.py` 发布新内容**：该工具只服务 COS legacy
  （旧 APK 与临时回滚）；COS 根目录整体上传不是当前生产面。发布细节见
  [game-data-sources.md](game-data-sources.md) §5.2
- **按游戏版本的 dataset（2026-11-05）**：origin 同时维护多份数据，路径
  `/v/<dataset-id>/...`，目录 `datasets/<id> -> releases/<release-id>`，`current` 指向
  `versions.json.default` 对应的 dataset（旧客户端兼容入口）。每个 dataset 的不可变
  `version.json` 记录 `dataset_id` / `game_version` / `variant` / `release` /
  `content_digest` / `manifest_sha256` / `tank_count` / `status`；`content_digest` 只由
  manifest 的 path→sha256 与语义化 `data_version` 决定（排除 `generated`、`upstream_commit`、
  `worktree_dirty` 与重新提取的时间戳），同内容重建不漂移。`manifest.json` /
  `version.json` / `delta.json` / `overhead/` 是发布元数据/中间件，不进入内容清单。
  当前生产登记的三个 dataset（2026-11-11 命名对齐客户端，origin 已同日发布并回读 0 mismatch）：`11.21.0`
  （738 辆，release `11.21.0-20261106-r2`，**default**，5965 件，含 `research/tech_tree/**`）、
  `11.21.0_1`（SmartDLC 动态内容槽位命名，release `11.21.0_1-20261111-r1`，数据面与 11.21.0
  **逐字节一致**、同 content_digest——动态内容走客户端运行时通道，待 11.21.1 客户端数据充实）与
  `11.4.0.138`（超测客户端自身版本号，label「超测服 11.4.0.138」，747 辆，release `t11r3-20261113-r3`，
  **显式选择**；2026-11-11 由 id `supertest` 改名，旧 `?data=supertest` 链接随之失效）。
  dataset_id 字符集含 `_`（本日放宽；`?data=` 引入 `_` id 前须先部署放宽校验的 WotbTools
  前端 bundle）。`11.22` 通道待正式服包发布时加入
  （终态通道：超测服 / 11.21 / 11.22）。`current` 指向 default。`/versions.json` 与
  `/v/<id>/...` 需 WotbTools 的 Caddyfile redeploy 后生效（2026-11-06 已上线，回读 200）。工具
  `tools/asset_release.py`（`stamp` / `catalog` / `verify` / `delta`）与前端选择器契约见
  [game-data-sources.md](game-data-sources.md) §5.2
- **内容寻址对象库（2026-11-05）**：唯一内容存放在
  `/srv/wotb-assets/objects/<sha[:2]>/<sha256>`，release 树是对象硬链接（Caddy 静态伺服
  不变）。`tools/asset_store.py` 负责 import / materialize / adopt / gc / stats / verify；
  旧 `cp -al` 流程仍可用，`adopt` 可原地收敛。实测单包内重复 4.9%（4432.9→4216.7 MB），
  3 个本机包合计 13075.9→4237.4 MB（67.6%）。**生产迁移（2026-11-05）**：9 个 release
  逻辑 **39449.5 MB** → **6107 个对象 / 4237.9 MB（去重 89.3%）**，`verify --full --links`
  PASS（53035 条路径），release 树盘上 **4.0 GB**。存储布局、迁移与 GC 见
  [asset-store.md](asset-store.md)

**当前包内布局**（本地实测；本地 11.21 包 **5965 个文件 / 4433.0 MB**；线上当前
`t11r3-20261113-r3` 为 **747 辆**预览数据 + 配置器数据 + `research/tech_tree/**` 9 件，
**6026 个文件 / 逻辑 4482.5 MB**（manifest 逐文件字节和口径；历史 4444.3 MB 为 du 块
分配口径），manifest 0 mismatch；r2 相对 r1 仅 2 件（`data/tank_cache.json` 的逐弹
`penetration_top`/`damage_top` + `data/data_version.json`）；r3 相对 r2 仅 2 件
（`research/modules/japan.json` 的 `provenance.attributeDuplicates` + `research/meta.json`）；r1 相对上一代超测 release
`t11p2-20261106-r9`（743 辆 / 6000 件）为 52 件变更（4 辆新车全套 + 数据/pb/research/
科技树）+ 7 辆既有车 XML 的研发边重算（属性零变化 → 738 份基线 `tank/*.json` 与
`game_data` 逐字节复用）。`map/` 共 **43 个目录 / 669 件**：36 个基础图 + 7 个映射有差异的
变体目录（`medvedkovo_03`、`amigosville_03`、`rudniki_01/02/03`、`canal_01`、
`neptune_02`；共享资产硬链接，`index.json` 按 map_id 指向本 key）。其余 12 个变体
（desert_train_02/03、erlenberg_01/02、medvedkovo_02、amigosville_02、milbase_02..05、
lumber_01、neptune_01）分段表与基础变体一致，共用基础目录。下表按本地 11.21 基线
（738 辆）计数；`map2d`/`cover` 行按 43 图 + index 计数（36 基础图 + 7 个变体，均有独立 2D/封面）。
线上当前默认 release 为 `11.21.0-20261106-r2`（738 辆，5965 件 / 4433.0 MB，含
`research/tech_tree/**`）；超测通道 `11.4.0.138`（原 `supertest`）为 747 辆（release `t11r3-20261113-r3`，manifest 6026 件 /
4482.5 MB），仅显式选择；逐代增量见
[game-data-sources.md](game-data-sources.md) §5.2 release 链）：

| 包内路径 | 文件数 | 来自（见第一节编号） |
|---|---|---|
| `glb/{tank_id}/{model,collision}.glb` | 1476 | #8（738 辆 × 2） |
| `tank/{tank_id}.json` | 738 | #6（由 `tank_data/` 改名） |
| `tank_images/{id}.webp` + `crops.json` | 739 | #9（展示裁剪数据面） |
| `game_data/{id}.json` | 738 | #4 / #5 |
| `map/{key}/…` | 669 | #10：36 基础图 + 7 个变体目录（共享资产硬链接）；每目录 `scenery.glb`/`ground.webp`/`ground.layers.json`/`terrain.{json,u16.bin}`/`mini.webp`/`lighting.json`/`ibl.webp`/`destructibles.json` + 分层 `ground/{cm,lm,tile0,tile1,mask0,mask1[,hmap0,hmap1]}.webp`（`mini.webp` **当前无运行时消费方**，见 §5.7） |
| `data/` | 7 | #1 `tanks.pb`、#2 `models.pb`、#3 `tank_cache.json`、#11 `data_version.json`、打包器派生 `tank_names.json`、#7 `shellKinds.json`、`gunMaskSlices.json` |
| `map/{key}/lighting.json` | 43 | #10 配套：`tools/export_map_lighting.py` 逐图太阳/环境色/雾/IBL（36 + 7 变体） |
| `map/{key}/ibl.webp` | 43 | #10 配套：`tools/export_map_ibl.py` 逐图环境反射（等距柱状，~28KB/图；36 + 7 变体） |
| `suspension/{id}.json` | 728 | §1b 悬挂数据 |
| `tread/{id}.json` + `tread/textures/*.webp` + `tread/textures.json` | 752 | §1b 履带印数据（738 + 13 + 1） |
| `map2d/<code>.webp` + `map2d/index.json` | 44 | §10b：`tools/export_2d_basemaps.py` 写（43 图 + index；**非打包器产物**，重打包后须重跑；资产 origin 免发版通道） |
| `cover/<code>.webp` + `cover/index.json` | 44 | §10b：`tools/export_2d_basemaps.py` 写 512² picker 封面（43 图 + index；**非打包器产物**） |
| `research/**` | 29 | 配置器数据（`meta.json` + 9 国别 `modules/*.json` + 全局目录/物品/技能/params/strings/涂装，20 件）+ `tech_tree/**` 9 件；由 `scripts/export_asset_pack.py` 从 `data/research/` 复制，图标仍留 WotbTools `common/research/icons/` |
| `index.json` | 1 | 打包器生成（地图 id → key/display） |
| `manifest.json` | — | 打包器生成（全量 sha256；本身不登记自己） |

**`scenery.glb` 的语义 extras（2026-10-18）**：材质 `extras.sky` 由客户端 `Skyobject`
材质模板/着色器判定（消费端不再按节点名猜天穹；旧包无标记才名字 fallback）；
所有 `alphaMode=BLEND` 材质写 `extras.alphaBlend`（消费端据此不再把真混合改写成
不透明+裁切）；场景级 `asset.extras` 在变体组数与注册表条目数不一致时可写
`variantMappingIncomplete: true`，消费端对带 `mdVariant` 的节点 fail-closed
（隐藏全部变体节点，不再全量渲染幻影布景）。`terrain.json` 的完整字段/居中
`worldBounds` 契约见 [game-data-sources.md](game-data-sources.md)「地形高度图布局」。

打包器：[scripts/export_asset_pack.py](../scripts/export_asset_pack.py)。注意它不是纯拷贝，
中间有改名与派生：`tank_data/` → `tank/`；`tank_cache.json` 额外派生 `tank_names.json`；
地图地形多一个 `terrain.json` sidecar（把 `X-Terrain-Meta` 头物化，含 `terrainLayout: 2`
布局标记与 `terrainBinSha256` 配对锚点——高度图列口径与提取链见
[game-data-sources.md](game-data-sources.md) "地形高度图布局与提取链"）；
`data/shellKinds.json` 由 `data/shell_kinds.json` 物化（缺源 fail-closed）；`tank_images/crops.json`
由 `data/cache/tank_icon_crops.json` 物化（缺源或数量与图标失配即拒绝打包）。

**发布链（固定次序）**：
`export_asset_pack.py`（重打包）→ **俯视合成**（`composite_overhead.py --all --write`，
渲染件须与线上同代、先归档——[game-data-sources.md](game-data-sources.md) §5.5）→
**2D 底图导出**（`tools/export_2d_basemaps.py`，写 canonical staging 与包内 `map2d/`——
打包器会 `rmtree` 重建整个包，这一步必须在重打包之后，见 [game-data-sources.md](game-data-sources.md) §5.6）→
**变体目录物化**（`tools/materialize_destructible_variants.py --pack release/asset_pack
--map-index map_index.json`：只给 `destructibles.json` 映射确有差异的变体建独立目录，
共享资产硬链接，并改写 index 的 map_id→key）→
**`export_asset_pack.py --refresh-manifest`**（按包内现有内容重算全部条目——合成器在打包
之后改写 `ground.webp`、变体工具新增目录，不重算则 manifest 与包内件不一致）。

**发布（Alibaba origin）**：把变更文件 `scp` 到主机临时目录 → `cp -al <current-release>
<new-release>` → 用 `cp --remove-destination` 覆盖变更文件（**必须断链**，否则会改写
上一 release 的共享 inode）→ 需要新变体目录时 `cp -al <base-dir> <variant-dir>` 再覆盖
`destructibles.json` → 主机上重算 manifest（排除 `manifest.json` / `delta.json`）并
逐文件复核 0 mismatch → `ln -s` + `mv -T` 原子切 `current` → `deploy/assets/verify.sh`
（WotbTools 仓）与逐对象回拉校验。旧 release 保留回滚，`delta.json` 记录相对 base 的变更。

要点：

- **manifest 的逐文件 sha256 是唯一完整性锚点**：主机重算时必须遍历**新 release 的全部
  文件**（含硬链接共享文件与变体目录），并把 `manifest.json` / `delta.json` 排除在 `files`
  之外；校验阶段重算 0 mismatch 才切 `current`。
- **硬链接安全**：新 release 由 `cp -al` 复制而来，所有未变更文件与上一 release 共享 inode。
  覆盖任何文件都必须 `cp --remove-destination` / 临时文件 + `os.replace`；直接写会同时
  改写回滚版本（2026-10-22 发布已按此执行并逐项核对上一 release 的 manifest/index 哈希）。
- **不要整包上传**：只传变更文件；共享资产在主机上硬链接复制。旧实现 `--only <prefix>`
  的 COS 局部发布语义保留给 legacy，不适用于新 origin。
- ⚠️ **上传会遍历包目录下全部文件**：俯视烘焙的渲染中间产物 `<pack>/overhead/*.rgba`
  （按图 64MB）必须在上传前移出——约定缓存在 `release/overhead-bake/`，合成器可直接
  `--render-dir release/overhead-bake` 读缓存；归档助手 `tools/archive_overhead_bake.py`
  的 `--clear-pack` 负责清空包内件。
- ⚠️ **"客户端仍显示旧图"先查缓存窗口**：Caddy 对未内容寻址的文件用短 TTL
  （`max-age=300, must-revalidate`），浏览器新鲜期内不回源（App 端 Native 代理每次带
  `If-None-Match` 条件重取，换内容即时生效）。
- `manifest.json` 的 `upstream_commit` / `worktree_dirty` / `generated` 是包内容的溯源锚点；
  `worktree_dirty=true` 表示该 commit 的原样工作区**不能**复现整包（语义见打包器
  `git_provenance()`）。**现值以包内 `manifest.json` 为准**。

**COS legacy（旧 APK / 临时回滚）**：桶 `wotbtools-assets-1478073677`（`ap-shanghai`）
继续匿名只读；`tools/upload_asset_pack_cos.py` 的 `--only <prefix>` 差分发布与
「按桶内 manifest sha256 跳过」判据仅用于 legacy 维护，**不要用它发布新内容**。凭据只从
环境变量 `COS_SECRET_ID` / `COS_SECRET_KEY` 读；Windows 传参注意尾部 `\r`。

**不进包的**：`data/cache/local_*`（见第三节）、`data/replay_samples/`、
`data/sessions/`、`data/snapshots/`、`data/token_usage.json`。

### 2.2 GitHub Release（引擎，非数据）

`wotb-replay-wasm-v<version>.zip` —— 由 [.github/workflows/release.yml](../.github/workflows/release.yml)
在 `[workspace.metadata.release].version` 变更合并到 `main` 时自动测试、构建、打 tag 并发布
（版本策略见 [AGENTS.md](../AGENTS.md)；现值以 Cargo.toml / 本目录 index.md §版本与发行为准，v0.4.19）。消费方按 **commit + sha256** 锁定，
落到自己的 `/wasm/<40 位 commit>/` 目录；**不浮动跟随 upstream main**。

## 三、跨领域接线点与护栏

逐项替代完成度见 §一 / §1b；本节只留**不属于任何单项、换源必然撞上**的接线点与护栏：

- **打包器的源目录**：现读 `data/cache/models/` 与 `data/cache/tank_images/`，而本地导出器写
  `local_models/` / `local_tank_icons/`。两边内部布局与文件名完全一致，**只需改目录**——
  但漏改不会报错，只有产物悄悄沿用旧来源。两对目录现均已同步并逐字节一致，
  重导后**必须同步**（`cache/models ≡ local_models`、`cache/tank_images ≡ local_tank_icons`）。
- **来源混用无护栏**：打包器只做拷贝 + sha256（传输完整性），**不做来源一致性校验**。同一辆
  车混用两源在 `tank_configs` 的 `turret_index`/`gun_index` 与 GLB 节点号上是**硬耦合**，会
  静默错位（两来源对照铁律见 [local-model-export.md](local-model-export.md) §3）——
  现实教训：models.pb 枪序与客户端研发序不一致曾致包内 15 辆 `armor_model.gun` 与 `game_data`
  矛盾（换源后为 0）。"拷贝 + 哈希"管不住时序与来源，属同类缺口。

## 四、剩余缺口与依赖注记

- **无阻塞性数据缺口**：运行期 11+1 项全部已接线（§五）。以下为依赖注记：
- 名字表残差 **2 条**（1 枪 + 1 履带，客户端两种字符串源都没有；BK 自建名）→ 补充表兜底；
  运行时本地化覆盖层是 per-user 缓存（新装/未同步时会缺），提取链对缺名保持软失败 + 回退。
- 封面图判定已降级为抽查：与 BK 封面同源同画（换源接近视觉无损），对照渲染件在
  `data/cache/local_compare/`。
- 坦克悬挂：机制已还原、数据已导出、前端已接线；**T4 对齐分级待再 RE**
  （见 [tank-suspension-client-re.md](tank-suspension-client-re.md)）。
- 履带印：数据面已导出并接线（前端几何贴花）；单场按需取数（≤14 json + ≤9 贴图）。
- 俯视合成链的世代硬约束（渲染缓存与线上底图同代、先归档再合成）见 §2.1 与
  [game-data-sources.md](game-data-sources.md) §5.5。

## 五、完成度汇总

```
按完成度（§一 / §1b 逐条；共 12 条 = §一 11 项含 10a + §1b 主要派生项另列）：

  ✅ 已接线（本机解包即现役来源）
       #1 整车数值 · #2 装甲/原点/限位 · #3 tank_cache · #6 tank/{id}.json · #7 弹种表/kind
       #4/#5 game_data（装甲 + 碰撞，含 primary）        #8 GLB（738/738）
       #9 封面图（客户端剥壳字节，738/738）              #10/#10a maps/terrain/destructibles
       #11 data_version · 解包底座
       limitsJson · shellsJson · tankNamesJson · 补充表（dev_name 738 + BK 自建名 2 条）
       悬挂（728）· 履带印（738 + 13 贴图）

  ⚪ 无缺口：BK 侧 122 处差异已全部定性（119 枪序＝口径、1 处 explosion_radius＝BK 错值
     已随换源修复、2 条名字＝补表兜底）；桥表可纯客户端重建（新车自动覆盖）

  ➖ 无需替代      WG API（战绩） · tank_id 命名空间（只能固化桥表） · BlitzKit 交叉校验角色
```

**一句话结论**：运行期 11+1 个数据条目**全部**已由本机解包/自产承接——含
`tanks.pb` / `models.pb` 及其全部派生面（tank_cache / tank/{id}.json / shellKinds）
与封面图（客户端剥壳字节）。外部依赖：**BlitzKit 仅剩"交叉校验对象"角色**
（`fetch-blitzkit` 保留）+ 两处缺失兜底（GLB `web/assets.rs:113`、图标 `web/assets.rs:233`，
均休眠）+ WG 战绩 API（性质不同，非客户端可解）。引擎产物（WASM Release）不含任何 pb。

## 六、本地提取工具索引

跑一次"从本机客户端取数"要动到哪些文件。入口默认指向 Steam 安装的
`Data/`，可用 `--game-data` / `--game-dir` 覆盖。

### 6.1 Python 提取链（`tools/`）

底座（所有 Python 导出的公共依赖）：

| 路径 | 作用 |
|---|---|
| `tools/wotbtools/wotb_sc2.py` | DAVA `SceneFileV2`(.sc2) + DVPL 解包（KeyedArchive 读取）。DVPL/LZ4 侧 fail-closed 口径与 Rust `dvpl.rs` 对齐，见 [game-data-sources.md](game-data-sources.md) §5.3；契约单测 `tools/test_dvpl_decode.py` |
| `tools/wotbtools/wotb_scg.py` | DAVA `SCPG`(.scg) 几何解码（顶点流 / 索引 / 图元）。顶点位偏移与 stride 按 DAVA `RenderBase.h` 权威位表（流序 = EVF 位号序），43 种实测掩码零失配 |
| `tools/wotbtools/dlc_packs.py` | **DLC 覆盖层解析**：`client_path()` 让 `packs/<rel>` 优先于 `Data/<rel>`（微更新不落 `Data`，见 [game-data-sources.md](game-data-sources.md) §5.4）；单测 `tools/test_dlc_packs.py`。Rust 侧同义实现 `game_extract::resolve_client_path` |

按提取目标的导出器（接线状态见 §一 / §1b 的"替代完成度"列）：

| 提取目标 | 工具 | 产出落点 | 接线 |
|---|---|---|---|
| 2D 战术底图 canonical（29 张 1254²） | `tools/export_2d_basemaps.py`（冻结配方；`--verify` 重算 + 索引/字节/canonical 三方对齐 + **几何契约回读（worldBounds/取向/方形像素）**；`--self-test` 锁曲线单调/不裁切/色相；`tools/test_2d_basemap_geometry.py` 锁索引 v2 ↔ 交接件 ↔ 注册表常量） | `release/2d-basemaps/` + manifest → 交接 WotbTools `frontend/src/assets/maps/`；同时写包内 `release/asset_pack/map2d/<code>.webp` + `index.json`（资产 origin 免发版通道，发布由 §2.1 新 release 流程承担） | ✅ |
| 地图场景（GLB + 地面 + 分层 + sidecar） | `tools/export_map_glb.py`（`--ground-only` / `--scenery-only` / `--jobs`） | `data/cache/maps/<space>.{glb,ground.*.webp,json}` | ✅ |
| 地图可破坏物清单（碰撞 / 耐久 / 类型库联表；按 map_id 序数选本变体 lka + 结构自检） | `tools/export_map_destructibles.py` | `data/cache/maps/<key>/destructibles.json` | ✅ |
| 坦克模型（`model.glb` + `collision.glb`） | `tools/export_tank_glb.py` | `data/cache/local_models/<id>/` → **同步进 `cache/models/`**；静态变换烘焙 / StateSwitcher 状态过滤见 [local-model-export.md](local-model-export.md) §1 铁律 14/15，单测 `tools/test_export_tank_static_transform.py` | ✅ |
| 坦克封面图 / 图标（含展示裁剪框） | `tools/export_tank_icons.py`（DVPL 剥壳裸 webp；`--emit-crops` 内容框；`fetch-icons` 降为 BK 对照/兜底） | `data/cache/local_tank_icons/`（738/738）→ **同步进 `cache/tank_images/`**；`data/cache/tank_icon_crops.json` → 包内 `tank_images/crops.json` | ✅ |
| 车辆数值（`tanks.pb` / `models.pb` 等价物） | `tools/extract_vehicles.py`（+ `data/tank_id_bridge.json` 的 `--emit-bridge`）→ `tools/emit_vehicle_pb.py` | `data/cache/local_pb/`（738 辆全量）→ **`data/tanks.pb` / `data/models.pb` 现役** | ✅ |
| 坦克悬挂 | `tools/export_tank_suspension.py` | `data/tank_suspension/<id>.json` → 包内 `suspension/`（728 辆；9 辆无块、1 辆空悬挂按 fail-closed 不产文件） | ✅ |
| 地面履带印 | `tools/export_tank_tread.py` | `data/tank_tread/<id>.json` + `data/tank_tread_textures/*.webp` → 包内 `tread/` | ✅ |
| 研发/配置数据面（装备 / 消耗品 / 补给 / 乘员技能 / 弹种图标 / 参数面板 / 逐车模块（含 `hasTurret`、火炮 `rotationSpeed`、扩圈系数）/ 科技树坐标+整车边（`--with-research-graph --tech-tree-extra`）/ 文案 + 物品/模块/技能图标） | `tools/export_research_loadout.py`（客户端解包；`meta.json` 记 gameVersion + 逐文件 sha256；预览车用 `--game-data` / `--bridge` / `--modules-only --merge` 并入，预览 Data 根可用 `tools/fetch_t11_techtree.py --data-root` 按需重建；已含 preview 合并的输出目录禁止全量重导——防回退门禁）；**预览发布统一入口 `tools/sync_preview_research.py`**（基座复用 + modules/items 变更导出 + XI 升级树/hp_top/`upgrade_top_shells.json`/tier-11 物品校验 + `.sync.json` 逐文件 reused/updated/new）；XI 强化树图标 `tools/export_augment_icons.py`（ZIP Range 取 @2x、alpha 裁边、无损 webp → WotbTools `common/research/icons/upgrades/`，机制目录名两处客户端拼写差异显式登记） | `data/research/`（132 文件 ≈3.80 MB，740 辆含两辆 XI 预览）→ **非图标数据全部进资产包 `research/**`**（`scripts/export_asset_pack.py`）；全局图标集进 WotbTools `common/research/icons/` | ✅ 坦克百科配置器消费（新坦克/物品靠资产包更新） |
| 动态装甲状态声明 | `tools/extract_armor_states.py` | `data/armor_states.json`（6 车 12 态；无块车不在产物中） | ✅ |
| 俯视地面合成 | `tools/composite_overhead.py`（**朝向 = 契约常量 `YX`**，8 朝向相关只作 `audit` 诊断，见 [game-data-sources.md](game-data-sources.md) §5.5；**世代护栏**：meta 缺 `ground` 键的旧世代渲染件 `--write` 默认拒写，`--allow-legacy-render` 仅诊断用，回归 `tools/test_overhead_guard.py`）+ `tools/archive_overhead_bake.py`（烘焙件归档助手：成对校验 + 世代体检 + `--clear-pack`） | 写回 `<pack>/map/<key>/ground.webp`（**打包器之后跑**，随后 `--refresh-manifest` 重算 manifest）；重烘须 `--base-dir data/cache/maps`；`--verify` 反解读回实际烘入朝向 | ✅ |

### 6.2 Rust 侧解包（`src/wargaming/`，由 CLI 子命令驱动）

| 模块 | 作用 | 驱动命令 |
|---|---|---|
| `dvpl.rs` | DVPL 解码（**fail-closed**：编码长度 / 存储载荷 CRC32 / 解压长度三项核对，不符即 `Err`，见 [game-data-sources.md](game-data-sources.md) §5.3）+ 装甲 XML / 碰撞 YAML 解析（`ArmorModel::parse_from_xml`、`CollisionData::parse_from_yaml`） | `parse-game`、`extract-game` |
| `game_extract.rs` | 批量提取 → `data/game_data/{id}.json`（含 `armor_model` / `collision`） | `extract-game --force`、`update-data` |
| `map_assets.rs` | 与客户端同链解析地图注册表，提取底图 / 地形 / 小地图。**小地图缓存按 space 一份**：`minimap_cache_entries` 取该 space 的**注册表首条目**（与打包器 `seen_spaces` 同规则） | `fetch-terrain`、`fetch-minimaps`（运行期 `/api/playback/map`） |
| `data_version.rs` | `data/data_version.json` 版本指纹（`game_version` 读 `Data/version.txt.dvpl`） | `update-data` |
| `model_fetch.rs` | GLB 全量预热（BlitzKit 兜底路径，已非主来源） | `fetch-models` |
| `tank_configs.rs` | 配置 / 装甲领域层（`synth_armor_model`：逐板装甲仍读 models.pb，`primary` 从 game_data 拷贝） | `dump-tank-data` |
| `tank_resolver.rs` | 装甲**摘要**（优先读客户端 `game_data` 的 `armor_model`，缺失回退 BlitzKit） | CLI 分析链 |

### 6.3 对照 / 验证（同目录，非导出源）

`tools/compare_tank_glb.py`（本地导出 vs BlitzKit 逐辆几何对照）、`tools/compare_tank_icons.py`
（封面图逐像素对齐对照）、`tools/compare_vehicle_data.py`（车辆数值逐字段对照）、
`tools/test_export_variants.py`（变体标签单测）、`tools/probe_switch.py`（.sc2 状态开关解剖）、
`tools/decode_experiment.py`（可破坏物候选物理解码实验）。

### 6.4 打包 / 分发 / 本机辅助

| 路径 | 作用 |
|---|---|
| `scripts/export_asset_pack.py` | 收拢上述产物为 `release/asset_pack/`（`index.json` / `manifest.json` + 逐文件 sha256）；`--refresh-manifest` = 不重打包、按包内容重算 manifest（合成后 / 上传前跑） |
| `tools/materialize_destructible_variants.py` | 打包链末端：只给 `destructibles.json` 映射确有差异的地图变体物化独立 `map/<key>/`（共享资产硬链接，index 按 map_id 指向本 key）；必须在 composite/2D 之后、`--refresh-manifest` 之前 |
| `tools/upload_asset_pack_cos.py` | **legacy only**：资产包 → COS 差分上传（旧 APK/回滚；跳过判定 = 桶内 manifest 的逐文件 sha256；新内容走 §2.1 的 Alibaba 新 release） |
| `scripts/serve_asset_pack.mjs` | 本机 CORS 伺服 `release/asset_pack`（消费方 dev server 取用） |
| `scripts/build-wasm.ps1` | WASM 回放解析产物构建（引擎侧，与资产提取无关） |

### 6.5 不在本仓 / 未跟踪

- **俯视渲染器**：消费方仓 `WotbTools/frontend/scripts/bake-ground-overhead.mjs`（headless
  Chrome + three.js，产物 `.rgba` 交 `tools/composite_overhead.py` 合成）。
- **逆向探针**：`examples/*.rs`（`destructible_probe*` / `filter_*_probe` /
  `m29_*_probe` / `p1_comp_probe` / `dump_dvpl` 等），`.gitignore` 覆盖、仅本机存在；
  `cargo test` 仍编译本地副本，但**不入库**。
- **逆向调查报告与转储**：`investigation/`（协议 / 对照报告、facet 转储等本机证据材料；
  docs 正文以相对路径引用）——`.gitignore` 覆盖、**不入库**。
- **本机构建产物**：`frontend/public/wasm/`（`scripts/build-wasm.ps1` 产物，
  **随包不随库**）——`.gitignore` 覆盖。

**一句话记法**：地图 / 坦克 / 图标 / 车辆数值在 `tools/`（Python，底座是 `tools/wotbtools/`
解 DAVA 容器）；装甲 / 碰撞 / 底图在 `src/wargaming/`（Rust，CLI 驱动）；
`scripts/export_asset_pack.py` 收口，按 §2.1 发布到 Alibaba origin 的新 release；
`tools/upload_asset_pack_cos.py` 仅用于 COS legacy 维护。
