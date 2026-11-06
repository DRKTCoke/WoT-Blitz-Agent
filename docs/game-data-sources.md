# 游戏数据来源与 BlitzKit 依赖评估

> 回答两个问题：**每一份数据到底从哪来**，以及**能否摆脱对 BlitzKit 数据源的依赖、
> 改由解析本机游戏文件自产**。文末附资产 origin（Alibaba；COS 为 legacy）发布流程与
> 运维陷阱。所有结论均有实测证据。

## 一、结论

**数据内容可以本地自产，运行期 11+1 项数据现已全部客户端化。** BlitzKit 降级为
"**交叉校验对象**"（`fetch-blitzkit` 保留，定期用本地提取结果与其 pb 做 diff，漂移即
告警），运行期与提取链都不再依赖它。

- 已由游戏文件替代：`tanks.pb` 全部内容、`models.pb` 全部内容、GLB 几何、坦克封面图、
  地图资产（场景/地形/底图/小地图/可破坏物）；
- `tank_id` ↔ 游戏模型名的桥：游戏本地文件**不含现成的表**，但
  `tank_id = (list.xml <id> << 8) | 国家基数` 可**纯客户端重建**（全量一致），桥表已
  固化入库（§三）；
- 提取链与对照链共用同一套口径，换源过程以"BK ↔ 客户端"全量对照收口
  （剩余 122 处差异全部定性，见 [data-inventory.md](data-inventory.md) §一）。

---

## 二、数据源清单

### 2.1 当前来源

| 产物 | 当前来源 | 本地可否替代 | 证据 |
|---|---|---|---|
| `data/tanks.pb` 坦克数据库 | 本机客户端解包编码（`tools/extract_vehicles.py` → `tools/emit_vehicle_pb.py` + 补充表；BlitzKit 降为对照校验源） | ✅ 完全 | §2.2 |
| `data/models.pb` 模型定义 | 同上（同批） | ✅ 完全 | §2.3 |
| `data/cache/models/{id}/*.glb` | **本机客户端**（`tools/export_tank_glb.py` 自产；BlitzKit CDN 仅缺失兜底） | ✅ 已实现并接线 | §2.4 |
| `data/cache/tank_images/{id}.webp` | **本机客户端**（`tools/export_tank_icons.py` 剥壳字节；BlitzKit 仅缺失兜底） | ✅ 已实现并接线 | §2.5 |
| `data/game_data/{id}.json` 装甲/碰撞盒 | 本机客户端 DVPL | — | §2.2/§2.3 |
| `data/tank_cache.json` | `tanks.pb` 派生（纯本地，**不经 WG API**） | ✅ 随 pb | — |
| 地图资产（底图/地形/小地图/场景） | 本机客户端 | — | `tools/export_map_glb.py` |
| 玩家战绩 | WG API | 不可（本来就不是本地数据） | — |

`fetch-tanks` 的实现是 `TankResolver::from_blitzkit()` 后落盘，`api_client.rs` 明确
"坦克百科数据已全部来自 BlitzKit，不走 WG 百科"——WG API 只服务战绩查询。

### 2.2 tanks.pb 的内容 ← 游戏 XML

来源是 `Data/XML/item_defs.zip`（3MB、892 条目）内：

- 车辆定义 `vehicles/{nation}/{model_name}.xml.dvpl`（约 790 个；其中 **6 辆**在
  `<extras><armorsStatesController>` 声明动态装甲状态，由 `tools/extract_armor_states.py`
  提取为 `data/armor_states.json`，运行时语义见 [data-inventory.md](data-inventory.md) §1b）
- 共享模块 `vehicles/{nation}/components/{guns,shells,turrets,chassis,engines,radios,fuelTanks}.xml.dvpl`（72 个）
- tier：`Data/Configs/TechTree/{nation}_tree.yaml.dvpl` 的 `position[0]`
- 名称（**两个字符串源**）：
  1. 随包基础快照 `Data/Strings/<lang>.yaml.dvpl` 的 `#<nation>_vehicles:<KEY>`——**必须按
     元素自带的完整前缀键字面查**（`gb_vehicles` 450 键与 `uk_vehicles` 478 键并存、逐条
     不同；裸键回退会跨系相撞），值里的 YAML 转义（`ä`→ä）需解码、值捕获须
     转义感知（`"` 正则会 被 `\"` 截断），A/B 尾字母重复条目的名字可回退其基础键；
  2. **运行时本地化覆盖层**：客户端从 CDN 下载并缓存到
     `%LOCALAPPDATA%/wotblitz/DAVAProject/cache/`——共两处、用途不同（全盘 token 扫描
     31300 个文件定界）：
     * `localizations/<lang>.yaml`（**车辆/模块名**所在，带 `.etag` HTTP 缓存；
       `Data/server_config_urls.yaml` 指向 conf CDN）；
     * `dynamicContentLocalizations/<lang>.yaml`（**活动/offer 文案**，键形如
       `2025-tank-12/Title`；实测**不含**车辆/模块名——名字面剩余的 2 条枪/履带名在
       两处都没有）。**随包 strings 不含上线后新增的
     活动/BP 车与模块名**——BP 车在 `camouflages.yaml` 注册、名字键（`<stem>_Custom[_short]`）
     只在覆盖层里——这就是"游戏能正常显示名字、而只读 `Data/` 的解包查不到"的原因。
     覆盖层按客户端语言缓存（实测机为 zh-Hans），但**专名跨语言同形**（Turbo/Magnate/
     Panlong 等 24/24 与 BlitzKit 英文名逐字一致）；提取器把它作**缺失键回退、只接受拉丁值**。
     **研究导出（物品名）**另外把同一覆盖层按语言匹配用作缺失键回退（zh-Hans 取中文名，
     如 `#artefacts:tungstentip/name` → 钨心弹）；en 无本机覆盖层，5 个后期标准物品名由
     导出器补充（`ITEM_NAME_SUPPLEMENT`），ru 按既有约定回退 en。英文数据不会取中文覆盖层值。
     依赖注记：per-user 运行时缓存（需客户端登录同步过；新装/未同步时会缺）

**定位陷阱**：共享弹种定义不在 `vehicles/` 顶层，按文件名搜 `*shell*` 会一无所获；
正确做法是解码整个 zip 后全文检索弹种名，命中 `components/shells.xml`。

**逐字段实测（T-34 样例，与 BlitzKit `tank_cache.json` 完全一致）**：

| BlitzKit 字段 | 游戏来源 |
|---|---|
| `penetration` / `penetration_far` | `guns.xml` 的 `<shots><X><piercingPower>85 72</piercingPower>`——一个标签两个值，近/远 |
| `damage` / `module_damage` | `shells.xml` 的 `<damage><armor>140</armor><devices>105</devices></damage>` |
| `shell_type`（`ap`/`hc_premium`/…，field7） | 同 shell 的 `<icon>`（9 值显示令牌） |
| 弹种权威枚举（field9） | 同 shell 的 `<kind>` → 0=AP/1=APCR/2=HEAT/3=HE（4 值闭集；与 icon 交叉表零矛盾，`atgm_heat`→HOLLOW_CHARGE 即 field9 修复的坑） |
| `velocity` / `range` | `guns.xml` 的 `<speed>` / `<maxDistance>` |
| `caliber` / `normalization` / `ricochet` / `explosion_radius` | `shells.xml` 的 `<caliber>` / `<normalizationAngle>` / `<ricochetAngle>` / `<explosionRadius>` |
| 装填/瞄准/散布/弹鼓/连发 | gun 级 `<reloadTime>` `<aimingTime>` `<shotDispersionRadius>` `<clip>` `<burst>` |
| `hp` / `hp_top` | `hp` = 车体 `<maxHealth>` + 顶炮塔 `<maxHealth>`（基础）；`hp_top` 仅 XI = `hp` + `<upgradeProgression>` 的 `ATTRIBUTE_NAME_MAX_HEALTH` 节点（顶级配置，见 §2.7） |
| 机动 | `<speedLimits>`、chassis 的 `<rotationSpeed>` |
| shell `id` | `shells.xml` 的 `<id>`，经 `loadout::blitzkit_shell_global_id` 换算到回放/BlitzKit 的全局弹种域 |

**弹药列表的权威来源是共享炮定义**：`vehicles/{nation}/components/guns.xml`
的 `<shots><shell_tag><piercingPower>` 带弹道与穿深；车辆 XML 内联 `<shots>` 是价格/可用性叠加
（`<shell>shared<price>…`），个别车引用的弹种族与共享定义不同（`J24_Type_57`：内联 base 族在
客户端无任何弹道数据 → 穿深 0；共享 A 族 = 218/260/65 与 BlitzKit 一致）。解析规则：按共享
`<shots>` 列表迭代，内联同标签条目合并覆盖其余字段。

**其它名称/参数相关文件**（扫描所见，未接入）：`hangarBaseParamsPrecomputedData_generated.yaml`
（车库预计算表，含各车参数与 stem）、`DAVAProject/cef_data/cache/`（内嵌浏览器新闻缓存，噪声）。

### 2.3 models.pb 的内容 ← 游戏 XML + 几何

| BlitzKit 字段 | 游戏来源 |
|---|---|
| 逐板厚度 `plates` | 车辆 XML `<armor><armor_N>VALUE` |
| 间隙甲 `spaced` | `<armor_N>` 内含 `<vehicleDamageFactor>` 子节点（规则见 §四） |
| 主装甲 `primary` | `<primaryArmor>armor_1 armor_3 armor_12`（位置序 front/sides/rear） |
| 俯仰极限 `pitch` | gun 的 `<pitchLimits>-20 10</pitchLimits>` + `<extraPitchLimits><back>-20 3 27</back>` |
| 射界 `yaw` | turret 的 `<yawLimits>-180 180` |
| 炮塔/主炮→模型节点号 | 碰撞 `.sc2` 的节点名（`hull` / `turret_01` / `gun_01`） |
| 碰撞盒 / 原点 | 碰撞模型几何（需 DAVA 解析） |
| 炮塔初始姿态 `initial_turret_rotation` | 车辆 XML `<turretInitialRotation><yaw>/<pitch>/<roll>`（度）——与 models.pb field3 逐值一致（.sc2 的 TransformComponent 全是单位阵，不是来源） |
| 履带厚度 / 原点 | chassis 段；已按**逐底盘 map** 保留（C1 修复，见 [decoupling-status.md](decoupling-status.md) §2） |
| 炮管裁切平面 `maskSlice` | Parameters YAML `maskSlice.<gun_NN>`：`planePosition`(3D)/`planeNormal`/`planeAxis`/`planeSliceMode`/`frustumCone*`——models.pb 只带 `planePosition[1]` 标量；完整记录导出 sidecar `data/gun_mask_slices.json`（478 条，键 `tank_id:turret_local:gun_local`），包内 `data/gunMaskSlices.json`；与 gunRecoil/gunMarks 同组的**视觉/特效切片**参数，不作装甲判定边界（viewer 裁切仍用 BlitzKit 的 `gun_mask` 标量） |

**俯仰装备加成（2026-11-01 接入）**：客户端装备 **id=122 `improvedPitchLimitsSystem`**
（改进型垂直稳定器）的 `script.params.bonusValues` 为 `upperPitchLimitIncrease=4`、
`lowerPitchLimitIncrease=3`。上表 `pitch` 仍是 models.pb 的**基础极限**；回放解码时按
实体实际配件串（Type5 `0B`）叠加 4°/3°（基础与 front/back 扇区同增益，transition 不变）。
可用性由每车 `<optDevicePreset>` 决定：当前 11 辆 `AlternatePitchGrouserPreset`
+ Rinoceronte 的 `DrumGunPitchLimitsPreset`（共 12 辆）。数值锚定测试见
`crates/replay-core/tests/pitch_device_data.rs`；`tank/{id}.json` 不预计算有效值。

**逐炮塔/逐主炮装甲透出**（2026-10-17）：`tank/{id}.json` 的 `configs[]` 现在带
**additive** `turret_plates`/`turret_spaced`（按 `config.turret_index` 对应的 models.pb
炮塔变体）与 `gun_plates`/`gun_spaced`（逐主炮；`gun_thickness` 已有）。顶层
`armor_model.turret/gun` 仅保留为 tank 详情卡的**顶级摘要**。多炮塔/多主炮车（如 10625
stock `turret_01` plate 11 = 130 mm 主装甲 vs top `turret_02` plate 11 = 30 mm 间隙甲）
必须按当前 config 取表，不能把顶级摘要套到 stock——消费方按 `turret_index`/当前主炮
选表；字段缺失按 0/空 **fail-closed，不回退 `armor_model`**。

**厚度序列化精度**（2026-10-20）：`tank/{id}.json` 的 plate/track 厚度按 **0.01mm**
清洗（`round_armor_mm`），既去掉 f32 尾差又保留 `6.35` 这类真实板值；**不要退回
0.1mm**——那会把 6.35 舍成 6.3，HE/判定少 0.05mm。738 辆数据随本版重生成。

**枪序口径**：顶塔主炮列表按 **XML 文档序 = 游戏研发序**（末位 = 顶级主炮）导出；
BlitzKit 的 models.pb 是**模块 id 升序**（与研发序在 105 辆上序不同——92 辆末位主炮不同，
其中 15 辆末位主炮板集实际不同）。客户端 XML 每个 gun/turret 条目带 `<level>` 研发
等级与 `<unlocks>` 研发图，可作为交叉校验。

**空心碰撞盒口径**：43 辆无炮塔 TD（AT-SPG，固定战斗室车，如 StuG III G /
SU-85 / Hetzer / JPanther / Tortoise / T110E3）的**炮塔**碰撞盒两侧都只有空占位——客户端参数
YAML 无 `turret_NN` 段（实测 SU-85：`turret_` / `gun_` 0 处），item_defs XML 的炮塔 bbox 为全 0，
BlitzKit models.pb 也只有 **4 字节空消息**（`0a 00 12 00`，min/max 两个空 Vec3）。因此判据统一
为：**min/max 双双为空消息 = 无数据**，不是"零盒"。Rust 严格解析（`parse_vec3` 空 → `None`）
把它落成 `null`（包内这 43 辆 `collision_boxes.turret` = null，其余 chassis/hull/gun 三盒
738/738 齐备）；Python 对照器与 `extract_vehicles.py` 为对齐 pb 序列化而保留全 0
（该文件注明"全 0 bbox 不省略——实测 43 个炮塔"）。⚠️ 换源/对照时不得把空心读成实值。

### 2.4 GLB 几何 ← 客户端模型文件

**权威路径不在目录约定里，而在逐车的参数 YAML**：`Data/3d/Tanks/Parameters/<小写国名>/<模型名>.yaml.dvpl`
的 `resourcesPath.blitzModelPath`（碰撞走 `resourcesPath.collisionMesh`）。目录约定只是**回退**：

| 资产 | 权威来源 | 回退（按约定） |
|---|---|---|
| 视觉模型 | `resourcesPath.blitzModelPath` 指向的目录 | `Data/3d/Tanks/<大写国名>/<模型名>.sc2.dvpl` + `.scg.dvpl` |
| 碰撞模型 | `resourcesPath.collisionMesh` | `Data/3d/Tanks/CollisionMeshes/<小写国名>-<模型名>.sc2.dvpl` + `.scg.dvpl` |

**部件的 `collision_boxes` 不是这套碰撞网格的包围盒**：`tank/{id}.json` 的
`collision_boxes` **四盒全部直读** `Parameters/<小写国名>/<模型名>.yaml.dvpl` 的
`collision.*` 段（hull/turret 的**选段**仍用 models.pb 的模块→节点映射，但 bbox 数值
不再经 models.pb），是客户端的**部件代理盒**，也是回放 hash6 命中段
（`DecodeShotSegment`）的 1/255 量化框；`collision.glb` 则是
`CollisionMeshes/*.sc2/.scg` 的装甲三角网格（每顶点第 8 个 float = 装甲板号）。两者都是
本机客户端解包，但定义不同：全量 738 辆中 287 辆的 hull 盒与网格 AABB 完全相等，其余主要
在 +z 有差（中位 20.8mm、p90 81.7mm、最大 426mm）。射击复现/解码必须用部件盒，**不能**用
网格 AABB 替换，盒缺失时 fail-closed（本机审计换框后首发板匹配 488→457、部件匹配 577→502；
见 [回放与射击逆向总集](回放与射击逆向总集.md) 第四篇 §6.3「固有差异界限」——部件盒 vs 网格 AABB）。

**两套国名互不相同，按目录去猜必错**：

- `XML/item_defs/vehicles/` 与 `CollisionMeshes/` 用 **tanks.pb 的小写国名**（`germany`/`uk`/`ussr`…）
- `3d/Tanks/` 下的车辆目录是**另一套大写名**，且不是简单大写化——映射表在
  `tools/export_tank_glb.py` 的 `NATION_DIR`：
  `{germany→German, ussr→USSR, usa→USA, uk→GB, china→China, japan→Japan, france→France, european→European, other→Other}`
- 即便国名对了，`3d/Tanks/<国>/<模型名>.sc2.dvpl` 也**不普遍成立**（如 PzIV 的模型在
  `German/G79_Pz_IV_AusfGH/...`），所以必须走上述权威路径，别用约定拼。

导出实现 = `tools/export_tank_glb.py`（复用地图管线的 DAVA 原语：`wotb_sc2.py` 解
KeyedArchive 实体树、`wotb_scg.py` 解 PolygonGroup 顶点/索引流 + `GlbBuilder`）。
验收口径 = **逐字节 + 节点顺序**（见 [local-model-export.md](local-model-export.md)）。

### 2.5 封面图 ← 客户端本地

> 现役来源 = 本目录（`tools/export_tank_icons.py` 剥壳裸 webp，738/738）→ 同步进
> `data/cache/tank_images/`；BlitzKit `fetch-icons` 与运行期懒下载退为缺失兜底。
> 与 BK 同源同画：alpha 轮廓 100% 一致、RGB 差为 BK 二次重编码噪声（mean 1.79 / max 17）。

- `Data/Gfx/UI/BigTankIcons/<国家标签>-<名字>.packed.webp.dvpl`（2312 个，含 `@2x` 与 `_skinN` 变体）
- `Data/Gfx/UI/BattleScreenHUD/SmallTankIcons/`（1474 个，128×32，作兜底）

**权威来源 = 客户端逐车声明**：`3d/Tanks/Parameters/<nation>/<stem>.yaml.dvpl` 的
`bigIconPath` / `smallIconPath`（764 份参数文件中 755 份为逐车文件、全含该字段）——
**按整文件名（含国家标签）精确解析**（`ussr-IS_2` 必须命中苏联图标，不能归一化后撞上
同键的 `china-IS2`），738/738 全部由此命中。命名比"标签-模型名"复杂，需一并接受的形态：

1. 国家标签有一处客户端错拼须一并接受：`britsh`（另有 `british`）；
2. 有些图标名**不带任何国家标签**（如 `Frankentank_event`）；
3. 有些用**内部代号/缩写**，单靠拼名字推不出（`ST_I` → `ussr-R63_ST_IBD`、
   `Pro_Ag_A` → `germany-Leopard_PT_A`、`M2_med` → `usa-M2_MT`）——走 `ICON_ALIAS`
   人工核对别名表（宁可缺失也不猜，且别名表**优先**于启发式匹配）。

备选来源（仅在声明缺失时使用，按序）：`camouflages.yaml` 注册表 → list.xml 短名/全名 →
模型名及去前缀变体 → stem＋显示名词 → 人工别名表；弱来源加**唯一性护栏**（会撞他车的
命中被弃），并加"剥编号须留 ≥2 词"约束。**刻意不启用模糊匹配**：实验里模糊匹配会落到
**别的车**（`PzV_PzIV`→`PzIV`、`GB24_Centurion_Mk3`→`Oth41_Centurion_Mk3_S2`），
宁可留空也不串车。对照诊断用 `tools/compare_tank_icons.py`。

**展示裁剪**：图标字节保持游戏原始画布（256×128，坦克锚定左上、右下留白），
`export_tank_icons.py --emit-crops` 输出逐车内容框（alpha>0，确定性、无统计判定）
→ `data/cache/tank_icon_crops.json` → 包内 `tank_images/crops.json`（打包器
fail-closed）；消费方展示层按框缩放定位。图标重导后须重跑 `--emit-crops`。

### 2.6 客户端关键路径速查

| 用途 | 路径 |
|---|---|
| 车辆与模块定义（DVPL 压缩，容器为 zip） | `Data/XML/item_defs.zip` |
| 逐车渲染参数（贴图变换/后坐/悬挂/掩体平面） | `Data/3d/Tanks/Parameters/{nation}/{model_name}.yaml.dvpl` |
| 科技树与 tier | `Data/Configs/TechTree/{nation}_tree.yaml.dvpl` |
| 本地化显示名 | `Data/Strings/{lang}.yaml.dvpl` + 运行时本地化覆盖层（§2.2） |
| 坦克图标 | `Data/Gfx/UI/BigTankIcons/`、`.../BattleScreenHUD/SmallTankIcons/` |
| 研发/装备/消耗品/补给定义 | `Data/XML/item_defs/vehicles/common/{devices,consumables,provisions}/list.xml.dvpl` + 逐件 `.xml.dvpl`；`optional_devices.xml.dvpl` / `optional_device_slots.xml.dvpl`（9 槽骨架）/ `consumablesDefault.yaml.dvpl`（按 tier 默认表） |
| 参数面板分组与格式 | `Data/hangarParamsStructureConfig.yaml.dvpl`（分组树）、`Data/hangarParamsConfig.yaml.dvpl`（keyedArchive：units/digits/biggerIsBetter） |
| 物品图标（装备/消耗品/补给） | 映射在 `UI/Styles/Lobby/Inventory/{InventoryBigStyles,InventoryNormalStyles}.yaml.dvpl`（`.equipment_item.<icon>` → `bg-sprite`）与 `UI/Screens3/Lobby/Inventory/OptionalDevices/OptionalDevicesItemImage.style.yaml.dvpl`；图像在 `Gfx/UI/InventoryIcons/{Big,Normal}/<cat>/<file>.txt.dvpl`（图集切片描述）+ `texture0.packed.webp.dvpl`，新版补给/装备在 `Gfx/Shared/tank-supply/**`、`Gfx/Lobby/tank-supply/**`（单图 `.packed.webp`） |
| 模块类型图标 | `Gfx/UI/ModulesTechTree/vehicle{Chassis,Engine,Gun,Hull,Turret}.packed.webp.dvpl`（64×64，客户端模块卡片同款） |
| 逐车模块与研发边 | `Data/XML/item_defs/vehicles/{nation}/list.xml.dvpl` + `<stem>.xml.dvpl`（`<unlocks>` / `optDevicePreset` / `<crew>` / `<invisibility>`）+ `components/{chassis,turrets,guns,engines,radios,fuelTanks}.xml.dvpl`；**读解包目录**，`Data/XML/item_defs.zip` 是过期快照 |
| 客户端版本号 | `Data/version.txt.dvpl`（`update-data` 的版本感知依据） |

逐车 yaml 的 `suspension:` 块（逐轮行程数组 + 履带折线 + 弯曲/铺放系数）已由
`tools/export_tank_suspension.py` 导出为包内 `suspension/<tank_id>.json`（728/738 辆；
9 辆无 `suspension` 块、1 辆空悬挂按 fail-closed 不产文件）；字节结构、节点绑定契约与
未定项见 [tank-suspension-client-re.md](tank-suspension-client-re.md)。

同文件族的 `treadParameters:` 块（地面履带印：印记条带宽/间距 + `texturePath`）
与 `Data/3d/Tanks/Common/images/*tread*.dx11.dds.dvpl` 贴图（13 张，256² 软边泥痕条带）
已由 `tools/export_tank_tread.py` 导出为包内 `tread/`（**738/738 辆**：authored + 国别主
贴图回落）+ `tread/textures/*.webp`（13 张 DDS 无损转码）+ `tread/textures.json`
（alpha 归一化表）；`.tex` 引用在数据面的落点规则 = `<stem>.tex` → `<stem>.dx11.dds.dvpl`
（与坦克贴图同）。定案口径见 [data-inventory.md](data-inventory.md) 履带印行；前端消费 =
WotbTools `scene/treadMarks.js` 几何贴花。

配置器数据（`data/research/` 的非图标部分：装备/物品/技能/参数/文案/涂装/meta +
逐车 `modules/{nation}.json`）由 `scripts/export_asset_pack.py` 导出为包内 `research/**`；
WotbTools 经 assetProvider 加载，**不进入前端 bundle**。全局图标集留在 WotbTools
`common/research/icons/`（构建期打 URL）。后续游戏新增坦克/物品时只需重跑研究导出与
打包、发布新的资产包 release，WotbTools 无需发版。

### 2.7 11.22 tier XI 超测预览 ZIP（非现役源）

**当前预览源（2026-11-05 起）**：`http://cdn.static.wotb.app/win7/WotBlitz_11.4.0.130_t11p2.zip`
（20,718,671,157 B ≈ 19.3 GiB，Last-Modified 2026-11-02；`version.txt.dvpl` =
`11.4.0.130_7400742 11.22_supertest_tier11_5 WOTB_Win7`）。生产 dataset 通道
`supertest`（超测服）指向该包构建的 release `t11p2-20261106-r3`（r2 为同日中间版，保留回滚）。

**实装范围（t11p2）**：`list.xml` 共 5 辆 level 11：

| 车辆 | 国别 | list id | tank_id | 科技树 | 状态 |
|---|---|---|---|---|---|
| `Ch67_BZ_79` | china | 59 | 15153 | `[11, 3]` | t11b 已有；t11p2 仅数值 XML 变 |
| `R230_Object_432U` | ussr | 109 | 27905 | `[11, 8]` | 同上 |
| `R228_KR_1` | ussr | 110 | 28161 | `[11, 5]` | **t11p2 新实装** |
| `A179_Black_Rock` | usa | 118 | 30241 | 无（收藏车，不上树） | **t11p2 新实装** |
| `A183_XM69_Hacker` | usa | 119 | 30497 | `[11, 6]` | **t11p2 新实装** |

`tank_id = (list id << 8) | 国家低位`（ussr=1 / usa=33 / china=49）。仍在登记、未实装的
4 个位：`G189_Hirschkafer` `[11,3]`、`G187_Taschenratte` `[11,7]`、`J77_Ho_Ri_4`
`[11,2]`、`F135_AS_XX_40_t` `[11,4]`（pending，只有坐标）。注意 t11p2 从客户端移除了
英系收藏车 `GB144_Cutter`——它不在 11.21 基线里，对本仓无影响。

**提取纪律（t11p2 起，按 CRC 对比控制范围）**：完整包 20 GB 不做全量下载。先对
"基线预览 ZIP ↔ 新预览 ZIP" 的 central directory 做 **逐成员 CRC 对比**
（`fetch_t11_techtree.RangeReader` + CD 解析），只拉真正变化的成员：

- t11b→t11p2 实测：车辆定义 XML **94 changed + 3 added**，其余 780+ 辆逐字节相同；
  组件 7 国有变；`list.xml` 变化来自新 XI 与 `GB144_Cutter` 移除；9 国科技树
  `{nation}_tree.yaml` 逐字节不变（`prem_tanks.yaml` 变但不在消费链上）。
- `tools/fetch_t11_preview_assets.py`：按"共享元数据 + 预览车全套 + `--xml-list`
  变化车辆 XML"物化最小 Data 根（实测 327 文件 / 202 MB，12 并发 Range）。
- **新坦克复用旧车贴图**（BZ-79→WZ_114 履带、432U→T-60、KR-1→KV、Black_Rock→AEP_1、
  XM69→G165 履带 + M-6-Y 部件）：直接复用**本机 11.21 客户端**字节（与包内其余 738 辆
  同源；DVPL 容器级 CRC 与 t11p2 不同但按"11.21 基座 + XI 预览"语义属 11.21 侧），
  `tex_fail=[]`。
- ⚠️ 部分根 + 模糊匹配的坑：`extract-game` 的文件名模糊匹配曾把 `M6` 内嵌命中
  `A183_XM69_Hacker`（xm69 含 "m6"）、`T-43` 命中 `R230_Object_432U`（object432u
  含 "t43"），装上别人的装甲——匹配器已改为**词缀锚定**（前缀/后缀才允许包含），
  完整客户端 738 辆 game_data 逐字节无回归。

**历史预览源（t11b，2026-10-22 核验）**：
`http://cdn.static.wotb.app/win7/WotBlitz_11.4.0.119_t11b.zip`
（20,666,565,975 B，`11.22_supertest_tier11_4`，Last-Modified 2026-10-19）；当时的
release（`t11-preview-*` 系列）全部保留在生产 `releases/` 作回滚。`cdn.static.wotb.app`
同时是现役客户端 `server_config_urls.yaml.dvpl` 的 `Location2`，域名属官方 CDN。

**产物与边界（t11p2，2026-11-05/06 已上线）**：

- `release/asset_pack_t11p2_preview/`（本地构建产物，gitignored）：11.21 现役基线
  （含 tank-burst 的 226 件数值件）+ **5 辆 XI = 743 辆**；manifest **6000 件 /
  4460.9 MB**；`data_version` = `release/11.21.0+t11p2`（preview：client_version
  `11.22_supertest_tier11_5`，tanks 5 辆 ID）。相对生产上一代
  `tank-burst-dpm-20261104` 共 **42 件变更**（≈19.7 MB）；GLB 全部 `tex_fail=[]`；
  两辆旧预览车（15153/27905）的 GLB/悬挂/履带**逐字节未变**（仅 `tank/*.json`
  数值更新），BZ-79 科技树新增 1 条整车研发边、M48A1→XM69_Hacker 新边由外科重算捕获。
- **2026-11-06 r2 重建**：改为基于 `11.21.0-20261106-r2` 全量重做（旧超测包基于 t11b 基座，
  738 辆基线车缺 burst 字段）；743 辆 tank JSON 统一，相对旧 `t11p2-20261106` 717 件变更
  （708 tank + 9 数据/索引），digest `sha256:89b5cc64…`。research 从纯 11.21 副本出发，
  先把预览树 8 个 XI 登记位补成 pending，再跑外科更新 + modules merge，避免污染
  `data/research`；最终仍为 308 节点 / 4 pending / 314 边。r3 补 `data/cache/models` 后重跑
  `dump-tank-data`，5 辆预览车 `model_gun/turret_count` 0→1/1（Black Rock 2/1），仅 5 份
  tank JSON 变化，digest `sha256:3c0a4b62…`。
- **t11p2 客户端弹道迁移（提取器已适配）**：piercingPower / speed / maxDistance 从
  炮共享定义的 `<shots>` **迁入 `shells.xml` 弹种条目本身**（guns.xml 的 shots 变空
  标签）。`tools/extract_vehicles.py` 改为：shots 有值时仍以 shots 为准（t11b 及以前
  语义），为空回落弹种条目。未适配的症状是全部 15 发弹穿深/初速为 0（首次上线
  `t11p2-20261105` 即此缺陷，已由 `t11p2-20261106` 修正）。`dump-shell-kinds`
  富表同样随 tanks.pb 重导（5 辆弹种行曾带 0 穿深）。
- 科技树（外科更新，`tools/update_t11_techtree.py`）：**308 节点（304 主树 + 4 预览
  定义车）/ 4 pending / 314 整车边**（t11b 基线 306/6/310 + 转正 2 + 边净增 4）；
  97 辆 CRC 变化车逐车重算研发边并替换，未变车辆边按定义不变。
- **预览 research 同步入口（2026-11-06，防遗漏）**：`tools/sync_preview_research.py`
  `--base data/research --out <research_work> --game-data <preview Data> --bridge ...`。
  它以现役 research 为基座（未变文件原样复用），依次跑
  `export_research_loadout.py --modules-only --merge`（预览车 modules +
  `upgradeProgression` + `upgrade_top_hp.json`）与 `--items-only`（consumables /
  provisions / consumables_default，含 tier 11 过滤器与 `isVehicleAbility` 识别），
  再校验 XI 升级树/provenance、`hp_top`、tier-11 物品可用性、技术树 census 与 meta
  计数；失败即退出。`<out>.sync.json` 记录逐文件 reused / updated / new。
  新增/变化的预览共享 XML 必须由该入口覆盖，禁止只跑 `--modules-only` 就发布。
- 配置器 `research/`：modules 合并至 743 辆（usa 109 / ussr 106），逐车
  `gameVersion` = t11p2 客户端版本；`meta.preview` 5 辆；穿深不在 modules
  （配置器弹种数据走 `tank_cache` / `tank/{id}.json`）。**r4 增量**：5 辆 XI 写入
  `upgradeProgression`（车辆 XML `<upgradeProgression>` + `common/tank_upgrade_attributes.xml`
  的属性定义；节点 id/techName/type、`alternativeGroups.choices` 由 techName 对号、
  `provenance` 记客户端路径 + 解码 XML sha256），`tank_cache` / `tank/{id}.json` 新增
  `hp_top`（顶级总 HP）；`consumables.json` / `provisions.json` 从预览客户端重导，
  `vehicleFilter.tiers.max` 10→11（65/89 件，common 14/38），`<isVehicleAbility>` 条目
  按战斗能力排除出配置器装配池；`consumables_default.yaml` 客户端仍未提供 tier 11 条目
  （配置器默认不预选消耗品/补给，不影响可选项）。
- **params 模块耐久增量（r5）**：预览客户端 `hangarParamsStructureConfig.yaml` 的
  `modulesDumpSection` 按 7 类部件（`trackDevice` / `engineDevice` / `gunDevice` /
  `ammobayDevice` / `turretRotatorDevice` / `surveyingDeviceDevice` / `fuelTankDevice`）
  各 6 个指标（`MaxHealth` / `RegenHealth` / `HysteresisHealth` / `RegenPerSec` /
  `BurnPerSec` / `RepairTime`）导出，共 42 个。`export_params` 修复预览 YAML 的
  `numeric.keyedArchive` 包装后读取 units/digits/biggerIsBetter；合并时只追加新键，
  保留现役 `crew`/`gunCommonReloadTime`，且不把 `modulesDumpSection` 放进
  `mainSections`——数据已随包发布，WotbTools 面板暂不展示。
- **strings/meta 预览同步（r6）**：r4/r5 的 `research/strings/{en,zh-Hans}/research.json`
  仍沿用 11.21 基线（`meta.gameVersion=11.21.0.1235_7376187`）。预览 11.22 的 strings
  variant 已去掉单位占位符的 `<text_units>%(units)</text_units>` 包裹（改为 `%(units)`），
  并新增 `unit_measure_short_meter_per_second_squared="m/s²"`（46→47 条）；`sync_globals`
  的 strings 平面按源清单更新两语文件与 `research/meta.json` 的 sha256/字节数、preview
  时间戳，其余文件保持复用。该差异由全量同步链首次检出并随 r6 发布。
- **XI vehicleAbilities（r7）**：车辆 XML 的 `<extras>` 同时承载普通外部配置与 XI 技能；
  导出器只保留客户端 item 目录中带 `<isVehicleAbility>` 的标签，写入 modules 的
  `vehicleAbilities`（5 辆：BZ-79 `rechargeableNitro`、432U `combatFrenzy`、KR-1
  `rammingConfiguration`、Black Rock `burstMode`、XM69 `steadyAimSystem`），并补
  `gameVersion`；`sync_preview_research.py` 对这 5 个绑定 fail-closed 校验。
- **生产已上线 r7**：`releases/t11p2-20261106-r7`（`content_digest sha256:f5a891f5…`；相对 r6 仅
  `research/modules/{china,usa,ussr}.json` + `research/meta.json` 4 文件，新增上述
  `vehicleAbilities`；r6（`ed940dfb…`，strings/meta）、r5（`1ec79041…`，42 个模块耐久参数）、r4（`1dcf3221…`）、
  r4-xi-items（`2820bbff…`）、r3（`3c0a4b62…`）、r2（`89b5cc64…`）、旧
  `t11p2-20261106`（`ea489ff1…`）与首发 `t11p2-20261105` 均保留可回滚），dataset
  通道为 `supertest`（超测服，显式选择）+ `11.21.0`（现役，default）；`11.22` 通道待 11.22
  正式服包发布时加入（三通道终态：超测服 / 11.21 / 11.22）。旧通道
  `11.21.0-t11-preview` 已 `dataset.sh unlink` 下线；`tank-burst-dpm-20261104`
  release 保留作超测通道回滚。弹种表 2090 → **2099**（+9 个新弹种 ID）。
- ⚠️ 组包教训（`cp -al` 后的覆盖必须断链）：本地组包曾用普通 `cp` 覆盖硬链接共享
  inode，把基线包的 pb/tank_cache/research/manifest/version 一度写穿成 t11p2 内容
  （`version.json`/`manifest.json` 两侧 stamp/refresh 轮流互写）；已从仓库源/git HEAD
  **断链恢复**（rm 后重写），双包 `verify --full` PASS。生产不受影响（上传发生在污染
  前，主机侧 manifest/version/文件三方 sha 核验一致）。

**产物与边界（t11b 历史）**：

- `release/asset_pack_t11_preview/`：11.21 现役包 + 2 辆 = **740 辆**；
  `data_version` = `release/11.21.0+t11-preview`、`tank_count=740`；manifest 5780 件。
- `release/asset_pack_t11_preview_delta/`：**30 件**变更/新增（含 manifest 与 8 件
  0.01mm 精度补差），**14.9 MB**；用于在 Alibaba host 的现役 release 上做“硬链接复制 +
  覆盖”，不重传全包。线上 `initial-20261019` 与本地基线仅差这 8 个 `tank/{id}.json`
  （6.3→6.35、15.9→15.87），并入后 manifest 逐文件自洽（预检 0 mismatch）。
- **已上线 `assets.wotbtools.com`（历史）**：该 release 曾作为 `current`；当前
  `current -> datasets/11.21.0`（release `11.21.0-20261106-r2`，5965 件 / 4433.0 MB，
  738 辆）；超测通道 `supertest` 为显式选择（release `t11p2-20261106-r3`，743 辆）。
  `t11-preview-20261029-techtree` 由
  `t11-preview-20261027-research-t11` 硬链接复制 + **9 件 `research/tech_tree/*.json`**
  组成（9 国科技树坐标 + 整车研发边，含 11.22 XI 预览节点）；manifest **5920 件 /
  4430.1 MB、0 mismatch**，`delta.json` 9 件，公网 9 个文件逐文件回拉 sha256 一致，
  `deploy/assets/verify.sh` PASS。
- **上一代 `releases/t11-preview-20261027-research-t11`（保留回滚）**：由
  `t11-preview-20261027-research-all` 硬链接复制 + **3 件 `research/`
  增量**组成（`meta.json`、`modules/china.json`、`modules/ussr.json`）：
  BZ-79（15153）与 Object 432U（27905）的逐车模块并入现有国别文件（china 57 /
  ussr 105，合计 740 辆）；每辆预览车带 `gameVersion`，`research/meta.json` 增
  `preview` 清单与 `previewGeneratedAt`，文件级 `meta.gameVersion` 仍描述 11.21
  主数据集。manifest **5911 件 / 4430.0 MB、0 mismatch**，`delta.json` 3 件；旧
  `t11-preview-20261027-research-all` / `research-modules` /
  `t11-preview-20261022-destructibles` / `t11-preview-20261022` / `initial-20261019`
  均保留回滚。WotbTools 端模块经 assetProvider 懒加载（不随 bundle）；本地验收：
  `node scripts/serve_asset_pack.mjs 8124 release/asset_pack_t11_preview`
  （或本地重建包），前端
  `?assets=http://127.0.0.1:8124&view=agent-tankopedia&tank=15153`。
- **预览车模块导出**：`tools/export_research_loadout.py` 支持预览客户端 Data 根与
  增量 bridge，只改逐车模块、不重导全局数据：
  `--game-data <preview Data root> --bridge data/preview_tank_id_bridge.json
  --modules-only --merge`。`--merge` 保留既有国别文件里的全部车辆，仅覆盖/新增本次
  导出的车；逐车 `gameVersion` 是数据来源权威，`meta.preview` 汇总预览车清单。
  bridge 中任一 `(nation, stem)` 未落到输出（拼错/缺文件）时导出器直接失败，不静默成功。
  预览 ZIP 的 `Strings/*.yaml.dvpl` 对部分新模块只有占位键（值等于键本身），
  导出器仍按原样保留 `name_*`（前端回退 stem），**不臆造显示名**。
- **科技树 XI 节点（2026-10-27）**：预览客户端的 `Configs/TechTree/{nation}_tree.yaml`
  含 8 个 XI 新登记位（BZ-79 `[11,3]`、Object 432U `[11,8]` 已实装；`R228_KR_1`/
  `A183_XM69_Hacker`/`G189_Hirschkafer`/`G187_Taschenratte`/`J77_Ho_Ri_4`/
  `F135_AS_XX_40_t` 只登记、无车辆定义）。`tools/fetch_t11_techtree.py` 对该 ZIP 做
  **ZIP64 central directory + HTTP Range 按需提取**（不下载 20.67 GB；实测 ~7 MB）：
  默认产出 `data/research_t11_preview/tech_tree_extra.json`（9 国树坐标 + 中/苏
  list.xml 的 XI 车事实 + preview bridge 对号的 tankId）供
  `export_research_loadout.py --with-research-graph --tech-tree-extra` 合并；
  `--data-root <dir>` 额外物化**最小预览 Data 根**（version + 两国 Strings + 两车的
  XML/components/Parameters，23 文件）——20 GB 解包删除后 `--modules-only --merge`
  仍可复现。导出合并规则：主数据集（11.21）坐标优先，预览树只**新增** stem；定义车
  进 `nodes`（`preview: true`），未定义的 6 个进 `pending`（只有坐标，不造名称/价格）。
  注意：全量重导 `--with-research-graph` 会把 china/ussr modules 冲回 11.21 集合
  （740→738）——导出器现对已含 `preview` 合并的输出目录直接报错；正确工序：
  临时目录全量导出 → 复制 `tech_tree/` → 重跑 `--modules-only --merge`（见
  data-inventory §配置器数据）。
  预览模式只读 Data 子树：`version.txt`、`Strings/{en,zh-Hans}.yaml.dvpl`、
  `XML/item_defs/vehicles/{nation}/{list,<stem>}.xml.dvpl` 与
  `components/{chassis,turrets,guns,engines,radios,fuelTanks,shells}.xml.dvpl`、
  `3d/Tanks/Parameters/{nation}/<stem>.yaml.dvpl`（从 ZIP 的 `app/Data/` 去前缀
  落到临时根目录即可；无需整包 19 GiB）。


**口径注意**：

- 超测数据，非正式服；只在独立预览包，不并入 `data/tanks.pb`/`tank_cache.json` 的
  11.21 现役数据（WotbTools 生产资产 origin 已切到该独立 release）。
- 图标是预览包占位图：两辆的 128² 基础图与 256² `@2x` 逐字节相同；`crops.json` 按
  128×128 记录（不是现役 256×128 画布）。
- BZ-79 第二弹种 XML 名/icon 为 HEAT（`_152mm_HEAT_Po_152ZPL`、`he_premium`），但
  `<kind>` 写 `HIGH_EXPLOSIVE`；现提取链按 `<kind>` 得 `type_id=3`、`type=he_premium`。
  两者矛盾，接入穿透判定前必须确认，不能静默按 HE 或 HEAT 处理。
- HP 口径：`tank_cache`/`tank_data` 的 `hp` = 车体 + 炮塔（2770 / 2260）；
  `configs[].hull_hp` = 车体（2170 / 1760）。

---

## 三、`tank_id` ↔ 游戏模型名的桥（可由客户端重建）

`tank_id` 是 WG 图鉴分配的编号（T-34 = 1、M4A3E2 = 10017），回放 `battle_results` /
`ARENA_INFO` 里用的就是它；提取时要定位 `vehicles/{nation}/{model_name}.xml.dvpl`，
必须把 `tank_id` 换成 `model_name`。

**桥的现状**：`data/tank_id_bridge.json`（738 条）由 `tanks.pb` 导出（`--emit-bridge`）；
**并可纯客户端重建**——`Data/XML/item_defs/vehicles/<nation>/list.xml` 每个车辆条目的
`<id>` 是国家内局部 id，全局 id = `(局部 id << 8) | 国家基数`（与 guns/shells 同一编码，
国家基数见 `tools/extract_vehicles.py::NATION_LOW`）。全量实测与 pb 导出的桥表
stem/nation 逐条一致，另多出 **20 辆** BK 没有的车（教程 bot、超测/开发车）。
工具入口：`python tools/extract_vehicles.py --emit-bridge --from-client`。`slug`（BK 专有）
无消费者，可留空——文件定位一律用 `stem`（游戏模型名）。

补充事实（全量扫描定界）：客户端**静态文件不含现成的 tank_id 表**——Data 下 31,516 个
文本容器按"已知 tank_id 字面量 ↔ 模型名共现"搜索 0 命中；`tank_id` 由 WG 服务器在车库
协议中下发（`wotblitz.exe` 协议字段名表 + 运行期缓存 `base_stuff_Asia.dat` 双向佐证），
客户端在运行期才知道。`tanks.pb` 每条坦克的主消息内 **field 1 = tank_id、field 32 =
游戏模型名**并列（field 2 = `dev_name` 是 slug，**不能用于文件定位**）。

---

## 四、间隙甲（spaced）判定规则（权威）

实测确认，与 BlitzKit 在板集一致处 **100% 吻合**（hull 729/729、turret 542/542、
gun 600/600）。规则共三条：

1. `plates[N]` = `<armor_N>` 的数值，**省略 0 厚板**——对齐 BlitzKit 的 pb 序列化器省零值语义
2. `spaced` = { N | `<armor_N>` 元素内含 `<vehicleDamageFactor>` 子节点 }，**零厚板保留**
3. `primary` = `<primaryArmor>` 的板列表，按位置映射到 front / sides / rear

**第 2 条的零厚板细节容易踩**：`plates` 是 map，零值被 pb 序列化器省略；`spaced` 是 id
列表，没有零值可省，所以同一块零厚板会"不在 plates 里、却在 spaced 里"。典型样例是
BT-2（1025）：`<armor_8>0<vehicleDamageFactor>0.0</vehicleDamageFactor>`，BlitzKit 的
`plates` 无 8、`spaced` 有 8。若在比对时把零厚板从 spaced 一并排除，会得出"BlitzKit
规则更宽"的错误结论。

---

## 五、运维：提取链与资产面发布

### 5.1 提取链的增量陷阱

- `extract-game` / `update-data` **默认只补缺失文件**（`out_path.exists() && !force` 即跳过），
  已存在的失效文件永远不会自愈——因外部约定变更导致的失效必须靠 `extract-game --force` 全量重跑
- `data_version.json` 只记一个 `game_data_updated_at`，从外部看像全量刷新过，实为混合版本
- 判定数据是否陈旧的**可靠指纹**：文件最后被哪个提交写过（`git log --name-only -- data/game_data/`），
  或批量统计 `gun.spaced` 非空率。注意 `hull_position` 为零值属合法，**不能当陈旧标志**
- `data/shell_kinds.json`（弹种反解表，`dump-shell-kinds --out` 产物）是 **tanks.pb 的直接
  派生**：每次 pb 刷新后必须同批重导，否则打包器物化的 `data/shellKinds.json` 与包内 pb 脱版。
  打包器对它 fail-closed（缺源即拒绝打包），但**同版性只能靠刷新链顺序保证**——先 `update-data`
  再 `dump-shell-kinds`，同一回合完成

### 5.1b 全量版本差异与 pack 复用（2026-11-06 起）

数据版本更新不再只靠人工判断“改了什么”，而是先做**全量源清单对比**：

1. `python tools/plan_data_version.py --base <上一个默认客户端 Data/ZIP> \
   --new <新客户端 Data/ZIP> --base-version <v1> --new-version <v2> \
   --out change_plan.json --base-pack-manifest <base>/manifest.json \
   --new-pack-manifest <new>/manifest.json --strict`
   - 目录按 sha256、ZIP 按 central-directory CRC32+size 做全量比较；
   - 按路径语义分类到 `vehicle-roster` / `vehicle-components` / `vehicle-xml` /
     `vehicle-params` / `vehicle-model` / `shared-xml` / `techtree` / `strings` /
     `tank-icons` / `maps` / `map-registry` / `ui` / `unmapped`；
   - plan v2 追加 `tanks`（bridge 解析的 tank_id/kinds/files）、`maps`
     （`map_index.json` 解析的 space → keys/map_ids）、`wholeTable`、`pins`
     （bridge/pb/map_index 的 sha256）；未解析 tank/map 在 `--strict` 下失败；
   - 输出逐平面 added/changed/removed、车辆 stem、`unmappedChanged`，以及
     `missingOutput` / `orphanOutput`（配合新旧 pack manifest）；
   - `orphanOutput` 豁免发布元数据/索引与 pack 内派生件（`data/models.pb`、
     `data/tank_names.json`、`data/shellKinds.json`、`data/gunMaskSlices.json`、
     `data/data_version.json`），避免真实车型变更被派生输出误报为“无源”；
   - `--strict` 对未识别变化、未解析 tank/map 或缺产物 fail-closed，不再静默跳过。
2. `python tools/apply_data_version.py --base-pack <base> --out-pack <new> \
   --game-data <new Data> --bridge ... --plan change_plan.json [--dry-run] [--only ...]`
   - 先以硬链接克隆 base pack（`research/**` 复制，避免写穿共享 inode）；
   - `research`：调用 `sync_preview_research.py` 做 modules/items/params/devices/
     camouflage/skills/strings 全平面增量，内容相同保留原文件；
   - `vehicle-assets`：按 plan.tanks 逐个调用原 `export_tank_glb.py --tank` /
     `export_tank_suspension.py --tank` / `export_tank_tread.py --tank` /
     `export_tank_icons.py --tank`；图标重新 `--emit-crops`，只覆盖哈希变化的文件，
     被移除车同步删除；
   - `core`（整表重建、只覆盖变化字节）：从 base pack 反向物化 `work/data/`（核心
     pb/cache、game_data、cache/models 等），在 `work/` 下按原顺序运行
     `wotb-agent update-data --force` → `dump-tank-data` → `dump-shell-kinds`；
     `tanks.pb`/`models.pb`/`tank_cache` 整表重写，`game_data/{id}.json`、
     `tank/{id}.json` 全量重算后逐文件 sha256 比较，只覆盖变化项。默认 `--offline`
     复用 base pb；`--core-online` 允许更新 pb，roster/components 变化在 offline
     下 fail-closed，需先准备新 pb；
   - `maps`：按变化 space 重导 GLB/ground/光照/IBL（`export_map_glb.py --map` 等），
     逐文件覆盖；`composite_overhead.py --map` 重合成；随后**整表**重跑
     `materialize_destructible_variants.py` 与 `export_2d_basemaps.py`（用户选定），
     重建 `index.json`/`map2d`/`cover` 索引；
   - `unmapped`/未解析 tank/map/`maps.yaml` 注册表变化直接拒绝 apply；
   - `--dry-run` 只打印各 handler 命令；`--only` 可单独排障；`--keep-work` 保留
     `.work/` staging；`--gun-mask-slices` 在 pb 变化时提供同批 sidecar；
     `--render-dir` 指定俯视渲染缓存；
   - `--refresh-manifest` 后输出 `<out>.apply.json`（逐 handler reused/updated/deleted）。

核心整表格式（`tanks.pb`/`models.pb`/`tank_cache`/`shellKinds`/`gunMaskSlices`）仍按原
编码器整表重写，因为无法安全按车打补丁；但最终 pack 只覆盖哈希变化的字节，未变文件
保持 base 硬链接。逐车 GLB/悬挂/履带/图标和地图 GLB/ground 只重导变化项；地图 2D/变体
按选定口径整表重跑。`data_version.json` 追加 `base_game_version` / `new_game_version` /
`source_plan_sha256` / `changed_planes` / `coverage` / `scope`（tanks/maps/wholeTable）/
`inputs`（pins），便于回读审计。

### 5.2 资产 origin（Alibaba）增量发布流程

**当前生产 origin = `https://assets.wotbtools.com`**（Alibaba 主机 `ali-sh-asset-01` /
47.101.141.111，`/srv/wotb-assets/current -> datasets/<default-id>`，dataset 再指向不可变
`releases/<release-id>`）。Web 与新 Android 都读这里；COS 桶
`wotbtools-assets-1478073677` 只保留给旧 APK 与临时回滚（legacy，见文末）。

发布链固定为 **重打包 → 俯视合成 → 2D 底图 → 变体目录物化 → `--refresh-manifest` →
新 release 增量覆盖 → 主机重算 manifest/校验 → 原子切 `current`**。`--refresh-manifest`
是必需的：合成器在打包之后改写 `ground.webp`，变体工具还会新增目录；不重算则 manifest
条目与包内件不一致。朝向契约、重烘与读回校验见 §5.5。

**变体目录物化**（2026-10-22 起）：
`python tools/materialize_destructible_variants.py --pack release/asset_pack --map-index map_index.json`
——多变体 space 的 lka 是每变体一张分段表，映射确有差异的变体（当前 7 个：
`medvedkovo_03`、`amigosville_03`、`rudniki_01/02/03`、`canal_01`、`neptune_02`）必须
有独立 `map/<key>/`；共享资产用硬链接，index 的该 map_id 指向本 key。其余 12 个变体
分段表与基础变体一致，共用基础目录。工具按 `instances`（含 serverId）逐项比较，
只物化真正不同的变体；不靠内容统计，也不改动共享资产的字节。

**发布（Alibaba）**：

1. 本地完成打包链并用 `--refresh-manifest` 重算；记录变更文件清单（只传这些）。
2. 把变更文件 `scp` 到主机临时目录（如 `/srv/wotb-assets/incoming-<batch>/`）。
3. 主机上 `cp -al releases/<current> releases/<new>`（硬链接复制，不重传全包）；
   需要新变体目录时 `cp -al releases/<new>/map/<base> releases/<new>/map/<variant>`。
4. 覆盖变更文件**必须断链**：`cp --remove-destination`（或临时文件 + `os.replace`）。
   新 release 里未变更的文件与上一 release 共享 inode，直接 `cp` 会同时改写回滚版本。
5. 主机上重算 manifest：遍历新 release 全部文件，排除 `manifest.json` / `version.json` /
   `delta.json`，保留 base manifest 的 `version` / `data_version` / `upstream_commit` /
   `worktree_dirty`，更新 `generated` 与逐文件 sha256；写 `delta.json` 记录相对 base 的变更。
6. **校验 0 mismatch**：重算全部文件哈希与 manifest 条目比对；核对上一 release 的
   `manifest.json` / `index.json` / 被覆盖文件的哈希未变（证明断链正确）；抽查变体目录
   共享文件哈希与基础目录一致。
7. `ln -s releases/<new> current.new && mv -T current.new current` 原子切换；公网逐文件
   回拉比对 sha256，并在 WotbTools 仓跑 `bash deploy/assets/verify.sh https://assets.wotbtools.com`。
8. 旧 release 保留回滚（至少保留当前 + 上一代）；`delta.json` 随 release 留存。

**按游戏版本的 dataset 发布（2026-11-05）**：

数据面不再只有一份 `current`，而是按游戏版本维护 dataset：

```
/srv/wotb-assets/
  releases/<release-id>/              # 不可变，内容同上一节
  datasets/<dataset-id> -> ../releases/<release-id>
  current -> datasets/<default-id>    # 旧客户端兼容入口
  versions.json                       # dataset 目录（前端选择器数据源）
```

- `dataset_id` 由 `game_version` 派生并进 URL：`release/11.21.0` → `11.21.0`；
  预览变体 → `11.21.0-t11-preview`（**禁止 `+` / `/` / `_`**）。
- 每个 release 内的 `version.json` 是不可变身份：`dataset_id` / `game_version` /
  `variant` / `release` / `generated` / `content_digest` / `manifest_sha256` /
  `tank_count` / `preview` / `min_client` / `status`。
- `content_digest` = sha256(规范化 `{contract, data_version(语义子集), files[path,sha256]}`)；
  排除 `generated` / `upstream_commit` / `worktree_dirty` / 提取时间戳，因此同内容重建
  不改变 digest。`manifest.json` / `version.json` / `delta.json` / `overhead/` 不进入内容清单
  （`--refresh-manifest` 已同步排除 `version.json` / `delta.json`）。
- 工具（本仓）：
  ```bash
  python tools/asset_release.py stamp --pack release/asset_pack \
      --dataset-id 11.21.0 --game-version release/11.21.0
  python tools/asset_release.py catalog --root /srv/wotb-assets/datasets \
      --out /srv/wotb-assets/versions.json --default 11.21.0
  python tools/asset_release.py verify --root /srv/wotb-assets/datasets \
      --catalog /srv/wotb-assets/versions.json
  python tools/asset_release.py delta --old <prev-release> --new <release> --out <release>/delta.json
  ```
- 主机切换（WotbTools `deploy/assets/dataset.sh`，`flock` + 原子符号链接 + 自动重算
  `versions.json` + verify）：`link --dataset <id> --release <release-id>`、
  `default --dataset <id>`、`rollback --dataset <id> --release <release-id>`。
- 对外路径：`/v/<dataset-id>/...` 与 `/versions.json`；根路径仍是 `current`，
  旧客户端零改动。前端通过 `?data=<id>` 与显式下拉切换（WotbTools 实现）。
- **生产登记（2026-11-05/06）**：dataset 通道为 **`11.21.0`（现役，default，
  release `11.21.0-20261106-r2`，738 辆，5965 件，含 `research/tech_tree/**`）+
  `supertest`（超测服，release `t11p2-20261106-r3`，743 辆，仅显式选择）**；`11.22`
  通道待 11.22 正式服包发布时加入（三通道终态：11.21 / 超测服 / 11.22）。旧通道
  `11.21.0-t11-preview` 已 `dataset.sh unlink` 下线（release 保留回滚）；`current`
  指向 11.21.0，`supertest` 不再占默认入口。
  `asset_release.py delta` 对通道改名需显式 `--allow-dataset-change`（delta.json 记
  `dataset_changed_from`）。`/versions.json` 与 `/v/<id>/...` 需 WotbTools 的 Caddyfile
  redeploy 后生效（2026-11-06 已上线，回读 200）；新 Caddyfile 已在临时容器对真实 `/srv/wotb-assets`
  验证（index/version/versions/dataset 路径全 200）。

**存储去重（2026-11-05）**：release 树继续作为不可变的对外视图，但内容收敛到
`/srv/wotb-assets/objects/<sha[:2]>/<sha256>`（内容寻址对象库），release 内文件改为对象
**硬链接**——Caddy 仍按普通静态文件伺服，URL/ETag 语义不变。`tools/asset_store.py` 提供
`import`（完整 pack 或“manifest + 变更文件”增量）、`materialize`（按 manifest 生成硬链接树）、
`adopt`（已有 release 原地迁移/去重）、`gc`（无引用且无硬链接的对象）、`stats`/`verify`。
实测 11.21 单包 5965 条 / 逻辑 4432.9 MB → 5841 个唯一对象 / 4216.7 MB（包内去重 4.9%）；
11.21 与 t11_preview 的 5907 条共同路径中 5899 条逐字节相同；3 个本机包合计
13075.9 MB → 4237.4 MB（67.6%）。**生产迁移（2026-11-05）**：9 个 release 逻辑
**39449.5 MB** → **6107 个对象 / 4237.9 MB（去重 89.3%）**，`verify --full --links` PASS
（53035 条路径、6107 个对象全量哈希），release 树盘上 **4.0 GB**。**t11p2 发布后
（2026-11-06）**：10 个 release，**6147 个对象 / 4257.6 MB（去重 90.3%）**，盘上
**4.1 GB**。旧的 `cp -al` 流程仍可用
（之后 `adopt` 即可收敛）；`objects` 与 `releases` 必须同文件系统；备份用 `rsync -aH`
（否则硬链接会被展开）。完整布局、发布 recipe、迁移与 GC 见
[asset-store.md](asset-store.md)。

**当前 release（2026-11-06 r2）**：`11.21.0-20261106-r2`，**738 辆**（11.21 现役全量重建，
含 `research/tech_tree/**`），**5965 件 / 4433.0 MB**，`content_digest sha256:9198d0ca…`；
`version.json` 登记 dataset `11.21.0`，`current` 指向它。超测通道 `supertest` 仍为
`t11p2-20261106-r3`（743 辆 / 6000 件 / 4460.9 MB），仅显式选择；r2 保留回滚；首发
`t11p2-20261105` 因穿深缺陷（弹道迁移未适配，见 §2.7）被同日替换，两代均保留。

**再上一代 release（2026-11-01）**：`2d-basemap-v3-20261101`，基于 `t11-preview-20261029-techtree`

**上一代 release（2026-11-04，超测通道回滚）**：`tank-burst-dpm-20261104`，基于 `2d-basemap-v3-20261101`
硬链接复制 + **226 件 `tank/*.json` 断链覆盖**（临时文件 + `os.replace` 换 inode）——
连发组（PomPom）DPM 的 `burst_group_*` 时序字段（36 件）与弹径权威修正后的 `caliber`
（207 件，47b980b 删除炮名解析 120 回退）随本地包上线；226 件 configs 外 deep-diff 零差异。
manifest **5978 件 / 逻辑 4238.5 MB、0 mismatch**（字节和口径；下文历史 MB 数为 du 块分配
口径），`delta.json` 226 modified，公网抽验 4 件 sha256 三方一致（served = manifest = 本地包），
`deploy/assets/verify.sh` PASS；回滚版本 manifest sha256 与 226 个被覆盖文件逐字节未变。
**发布注意（两代教训都适用）**：①`cp -al` 后 `manifest.json` 与上一代共享 inode，重算必须写
新 inode（临时文件 + `os.replace`）——原地 `open(w)` 会同时污染回滚版本的 manifest；②origin
各代 release 一律硬链接增量，`data/*` / `tank_images/crops.json` / 预览车资产只在 origin 侧
带 t11 预览合并键（740 辆），本地纯 11.21 包不含，**这些路径禁止从本地包覆盖**，否则预览
数据面回退；③`upstream_commit` 按发布程序保留 base 值 `4bfccf7c`，本次 226 件 tank 字节
实际产自 `cacb55ca` 工具链（本地包 manifest 记录），两条 provenance 并存见
[index.md](index.md) 2026-11-04 条目。此前（10-28→11-04）DPM 修复未上线即因发布链从未
覆盖过 tank 文件——增量发布时**每次都要核对差集是否完整**，不能只覆盖当批次主题文件。

**上一 release（2026-11-01）**：`2d-basemap-v3-20261101`，基于 `t11-preview-20261029-techtree`；`version.json` 登记 dataset `11.21.0-t11-preview`（通道已于 2026-11-06 unlink 下线）。
硬链接复制 + **14 件 map2d 增量（29→43 全量枚举首发布）+ 43 件 cover 全新目录** + 2 个索引
（`map2d/index.json` 43 条、`cover/index.json` 43 条）；旧 29 张 map2d 与上一代**逐字节相同**
（配方确定性），故为纯增量。manifest **5978 件 / 4444.3 MB、0 mismatch**，`delta.json` 59 件
（58 added + 1 modified），公网 14+43+2 件逐文件回拉 sha256 一致，`deploy/assets/verify.sh` PASS；
回滚版本 `t11-preview-20261029-techtree`（5920 件 / 4430.1 MB，manifest 已按磁盘重算并保留原
`generated`，校验 0 mismatch）。

**更早 release（2026-10-29）**：`t11-preview-20261029-techtree`，基于
`t11-preview-20261027-research-t11` 硬链接复制 + 9 件 `research/tech_tree/*.json`
增量（9 国科技树坐标 + 整车研发边，含两辆 XI 预览节点）；manifest **5920 件 /
4430.1 MB、0 mismatch**，`delta.json` 9 件，公网 9 个文件逐文件回拉 sha256，
`deploy/assets/verify.sh` PASS；上一代 `t11-preview-20261027-research-t11`（3 件
research 增量）与 `t11-preview-20261027-research-all` / `research-modules` /
`t11-preview-20261022-destructibles` / `t11-preview-20261022` / `initial-20261019`
保留回滚。

**历史批次（COS，legacy）**：2026-10-18 的 36 图 SpeedTree scenery 重导曾用
`--only map/` 局部发布 15 条 / 511.7 MB；2026-10-20 的 8 个 0.01mm 精度 `tank/{id}.json`
同理。此类记录只说明 COS legacy 的维护方式，不再适用于新内容发布。

**COS legacy 工具**：[tools/upload_asset_pack_cos.py](../tools/upload_asset_pack_cos.py)
（仅旧 APK / 临时回滚；`--only <prefix>` = 只登记本次上传的前缀，manifest 改为远端清单
就地补丁）。注意事项：跳过判定必须是桶内 manifest 的逐文件 sha256（**同尺寸不同内容
也要传**）；凭据只从 `COS_SECRET_ID` / `COS_SECRET_KEY` 读且注意 Windows 尾部 `\r`；
浏览器的 `max-age=3600` 缓存窗口只对 COS legacy 生效。新 origin 由 Caddy 用短 TTL
（`max-age=300, must-revalidate`）+ ETag 提供，App 端每次带 `If-None-Match` 条件重取。

`manifest.json` 的 `generated` 是本次更新时刻，可能与 `index.json` 的 `generated`
不同步（地图资产未变时 index 可保持原样）；`worktree_dirty=true` 的包**不等于**标注
commit 的原样工作区（见 [data-inventory.md](data-inventory.md) §2.1）。

### 5.2b 本机解包 → pb 编码器（现役数据源）

工具链（本仓）：

1. `python tools/extract_vehicles.py --all` → `data/cache/local_pb/{tanks,models}.json`
2. `python tools/emit_vehicle_pb.py --emit-supplement` → `data/local_pb_supplement.json`
   （客户端无对应概念/数据的字段集中登记：`dev_name` 738 与**名字 2 条**——1 枪 + 1 履带，
   两种字符串源都没有；`initial_turret_rotation` 已客户端化、不再需要）
3. `python tools/emit_vehicle_pb.py` → 编码为同格式 pb（只写 `blitzkit.rs` 运行期读取的字段；
   弹鼓按 field1×N + field2/3；全 0 向量/空心盒不写）
4. 对照/回归：`python tools/compare_vehicle_data.py [--pb-dir data/cache/blitzkit_pb_snapshot]`
   （换源前 pb 快照在 `data/cache/blitzkit_pb_snapshot/`，随时可复现 BK↔客户端对照）

**换源转正回归（三门全过）**：① `tank_cache.json` 逐字段对照仅 `10625`
explosion_radius 一处差异（客户端值，BK 错值）；② `armor_model.gun` × `game_data`
矛盾 **15 → 0**（turret 2 / hull 6 残留两源同数 = f32 量化）；③ GLB 节点号耦合抽样 5/5。
`tank_data` 18 辆差异 = 15 辆顶塔末位主炮板集（研发序）+ 2 辆 explosion_radius + 1 辆形状
——即 BK 源下"末位=顶级"误选的修复。弹种 id 常量偏移（`7009`/`12929`）经查为
**BK 侧按 gun module 塌缩 + 孤儿弹种族**所致，客户端数据自洽（共享炮 `<shots>` 为权威后
0 配对失败）。**BK↔客户端剩余差异共 122 处**（119 枪序＝研发序口径、1 explosion_radius＝
BK 错值、2 名字＝补表兜底）。

### 5.3 DVPL 外壳的校验契约（fail-closed）

提取链的每条读盘入口都过 `dvpl.rs`：装甲 XML / 碰撞 YAML（`extract-game`）、
`version.txt.dvpl`（`update-data`）、小地图与 `maps.yaml` / `en.yaml`（`map_assets`）。

footer 布局为 `decoded_size(4) + encoded_size(4) + crc32(4) + type(4) + "DVPL"(4)`，其中
**crc32 覆盖存储载荷（压缩后字节），不是原始数据**。解码器核对三条不变量，任一不符即
返回 `Err`：编码长度 == 实际载荷长度、存储载荷 CRC32、解压长度 == `decoded_size`。
Python 侧（`tools/wotbtools/wotb_sc2.py`）同口径——两端一致 fail-closed。

**LZ4 零偏移**：真机语料存在 `offset = 0` 的匹配序列，这不是规范 LZ4，但客户端确实产出、
参考实现（lz4 C，即 Python `lz4.block`）也照样解开——回引位置就是当前写出位置、且该处尚未
写出、缓冲区初值为零，等价于**写出 match_len 个零字节**。故两侧都按零处理而非拒绝。
全树 45016 个文件里仅 1 例：
`3d/Tanks/France/images_pbr/F114_Projet_4_1_skin_MISC.dx11.dds.dvpl`
——位于 `Skin_01` 配置里，`MaterialLibrary::_flatten` 只取 `configName == "Default"`，
**当前不可达**，属潜在陷阱；两端口径已一致，将来导出器支持皮肤配置也不会撞上。

复核探针（应全量零拒绝：本机客户端 `Data/` 45016 个 `.dvpl` / 20.3 GiB）：

```
WOTB_DVPL_PROBE=<Data 目录> cargo test dvpl_client_probe -- --ignored --nocapture
```

### 5.4 DLC 覆盖层（`packs/`）——读取优先级

客户端把 DLC 微更新写到 **`%LOCALAPPDATA%\wotblitz\packs`**，以**相同的相对路径覆盖**
游戏 `Data/` 下的同名文件（第三方 mod 工具文档明写 "its files override the base ones"）。
**微更新不落在 `Data/`**，所以只读 `Data/` 会拿到 DLC 应用前的旧版本。本机实测 45 个
`.dvpl`：5 个与 `Data/` 同名、内容不同（含坦克可视模型 `3d/Tanks/German/Ferdinand.sc2.dvpl`
与同车 `.scg.dvpl`）；另 40 个 `Data/` 里根本没有（全是 `G37_Ferdinand_skin` 皮肤资产）。

**约定：所有客户端资源读取都经统一解析器，packs 优先、缺失回退 `Data/`**：

- Rust：`src/wargaming/game_extract.rs` 的 `packs_dir()` / `resolve_client_path()`；已接入
  车辆 XML/YAML 定位、`version.txt.dvpl`、`maps.yaml` / `Strings/en.yaml`、地图 landscape 目录。
- Python：`tools/wotbtools/dlc_packs.py` 的 `packs_dir()` / `client_path()`；已接入
  `export_tank_glb.py` 的模型/碰撞/参数路径解析。回归测试 `tools/test_dlc_packs.py`。

非 Windows / 无 `packs` 目录（WSL、纯净检出）时自动退回只读 `Data/`，行为不变。

### 5.5 俯视合成（均衡档地面）的朝向契约与重烘

**契约：朝向是常量 `YX`（两轴各翻一次 = 180°），不搜索、不逐图特判。** 推导两侧都是固定
代码（与地图数据无关）：

- **渲染侧** `WotbTools/frontend/scripts/bake-ground-overhead.mjs`：`group.rotation` =
  qFrame `Ry(π)·Rx(−π/2)`，把游戏系 `(x,y,z)` 映到场景系 `(−x, z, y)`；正交相机
  `position(0,1000,0)` + `up(0,0,−1)` 俯视 ⇒ 屏幕右 = `+X_scene`、屏幕上 = `−Z_scene`，
  `readPixels` 再翻成图像行序 ⇒ **渲染图左列 = −X_scene、顶行 = −Z_scene**。
- **底图侧** `ground.webp` 契约（`tools/export_map_glb.py` 分层注释「前端采样
  `uv = (0.5−X/s, 0.5−Z/s)`」；`playbackScene` 的分层 ShaderMaterial 与均衡档
  `PlaneGeometry` + `TextureLoader`(flipY=true) 两路同式）⇒ **底图左列 = +X_scene、
  顶行 = +Z_scene（北）**，即「上=+z/北」。

两者相差 180° ⇒ 合成时对渲染图恒取 `YX`（`tools/composite_overhead.py` 的 `ORIENT`）。
低信号图上的梯度相关 argmax 会退化为噪声判决（曾把图形烘成上下镜像），因此
**该搜索已降级为 `audit` 诊断项**（不影响写盘，选中别的候选时在报告里标注）而不是判据。

**地面语义与合成权重**：烘焙**自己渲染地面**（共享模块
`WotbTools/frontend/src/scene/groundPass.js`：地形几何 + 分层地表 `cm×lm×tile/mask` 实时合成 +
水件/贴花按运行时同判保留渲染 + 贴地薄构件走廊掩码强制细分）——`ground.webp` 为
**"与 3D 档同源的正交俯视渲染"**。合成权重随 `<key>.meta.json` 的 `ground` 标志：
渲染含地面 ⇒ **覆盖即替换**（`out = rgb×a + ground×(1−a)`），无该标志的旧件沿用
`out = base×(1−0.92a) + render×0.92a`。剔除规则与运行时同式（天空=`/sky/`；
`dome/loonar/surroundings/^plane` 名字启发式与"≥90% 图幅壳"几何剔除**均已删除**——
穹顶建筑/外圈山体/大地壳/小件与运行时**逐件同判**保留）。具体剔除规则以
`WotbTools/frontend/src/scene/sceneryAssemblyRules.js`（运行时与烘焙同一份判定）为准。

**重烘（安全口径）**——重烘前**必须**让合成读**合成前底图**，否则会在已合成的图上二次合成：

```bash
python tools/composite_overhead.py --pack release/asset_pack --map himmelsdorf --write \
    --render-dir release/overhead-bake --base-dir data/cache/maps
```

`--render-dir` 直接读 `release/overhead-bake/` 缓存（不必把 64MB/图 的 `.rgba` 复制进包；
包内出现 `overhead/` 时上传工具会连它一起传，见 §5.2 第 4 条）。**重烘后读回校验**（应全为
`YX`、`ok=true`；负向对照：拿旧的镜像件跑同一命令会报 `applied=xY, ok=false`）：

```bash
python tools/composite_overhead.py --pack release/asset_pack --render-dir release/overhead-bake \
    --base-dir data/cache/maps --all --verify
```

⚠️ **渲染缓存必须与线上底图同世代（硬约束）**：地面 pass 世代（P2）的底图**只能**由
「地面 pass 世代的渲染件（meta 带 `ground=true`）」合成——旧世代渲染件（meta 无 `ground`、
alpha 覆盖率 ~20% = 只有俯视件）按本节命令重合成会把 36 张底图**回退一代**。判据是语义
标记而非统计：`meta.ground` 缺失 = 该渲染不含分层地面 pass。**两条硬约束**：

1. **归档助手**：烘焙把渲染件写进**包内** `overhead/`（合成器默认就从那里读），"合成完即清包内"是
   惯例而归档是唯一留存路径——用 `python tools/archive_overhead_bake.py [--verify-only|--clear-pack]`
   归档到 `release/overhead-bake/`：`<key>.rgba` 与 `<key>.meta.json` 必须**成对**（缺一半即整体拒绝，
   不部分归档）、拷贝后逐件字节数核对、逐图打印世代（缺 `ground` 键即警告）。
2. **合成器世代护栏**：`tools/composite_overhead.py` 对 meta 缺 `ground` 键的渲染件 `--write`
   **默认拒写**（fail-closed；每张图报告带 `generation` 字段），显式 `--allow-legacy-render`
   才放行（仅诊断/复现历史，不用于发布）；`--verify` 对旧世代渲染件附
   `generation_warning`（按 0.92 反解地面 pass 成品会失真，该 `ok` 不可用于判定）。
   回归锁：`tools/test_overhead_guard.py`（世代判据 / 拒写闸门 / 成对校验三组用例）。

**现役渲染件**：`release/overhead-bake/` 归档 = ground-pass 世代（与线上底图同代，
包内 == 归档 == 线上）；仅需线上字节时可用资产 origin 只读回拉
（`https://assets.wotbtools.com/map/<key>/ground.webp`；COS legacy 同路径仍可读），
逐张 sha256 对照 manifest。

### 5.6 2D 战术底图（WotbTools 前端资产）的派生链

2D 地图鸟瞰 / 战局回放用的底图不是客户端资产，也**不再**是外部 AI 增强件，而是本仓自产：
`release/asset_pack/map/<key>/ground.webp`（4096²，含俯视屋顶/树冠烘焙）经
`tools/export_2d_basemaps.py` 派生 → `release/2d-basemaps/`（canonical 1254² WebP q90 / method 6
＋ **cover 512² WebP q80**——picker 封面用轻量版，同一调色管线第二次采样，观感与 canonical 一致）
→ 交接 WotbTools `frontend/src/assets/maps/`（web 与 Android 共用这一份 canonical 字节；Android 不再派生，见本节结尾）。

**清单来源（v3，2026-11-01）**：资产包 `index.json` **全量枚举**（去重后 43 key）——不再依赖
WotbTools `mapImages.js`（手维护 29 张，新图/变体空间/老版复刻进不了循环，43 缺 14 的根因）。
pack 里每张图都有 ground.webp（打包链保证），原料齐备即全覆盖。

**冻结配方**（全 43 张同一套，参数 = 工具内常量）：LANCZOS 1254² → 全局对比曲线
`y' = softclip(128 + (y−128)×1.17)`（两端 24 级软肩）→ 饱和 ×0.9748 → 逐通道软肩（无硬裁切）
→ USM 1.0px/80%/thr2 → WebP q90 / method 6。

**取向/世界框 = 契约，不搜索**：`ground.webp` 顶行 = +Z_scene、左列 = +X_scene（与 §5.5 同一契约）；
WotbTools 的 2D 渲染把回放坐标按 `coordinateBounds = ±300` 线性映射（其 `utils/mapView.js`），
代入「场景 x = −游戏 x」的镜像后两侧同为「左 = −300(游戏 x)、上 = +300(游戏 z)」⇒ **无需翻转/裁剪**。
另做过 8 朝向结构相关作为诊断（29 张全部 identity 最优），但按 §5.5 同一纪律：诊断不参与判决。

**刻意不做的**：候选与旧 AI 增强批的色阶宽度差是**逐图属性**（中位 25.9 级，范围 −17…+46），
按 29 张聚合分布求的匹配 LUT 近似恒等；逐图配平会违反 AGENTS.md「同类数据同一变换、不做测量驱动的
逐项决策」⇒ 只用一条全局曲线按同一比例缩差，残余差距随 manifest 输出、如实记录。

**fail closed 前置条件**（工具内断言）：资产包 `index.json` 可解析且 key 去重后非空；
源 `ground.webp` 必须 4096²；`terrain.json.worldBounds` 必须 ±300 m；输出目录含清单之外的
webp 即拒写（防指错目录）。`--verify` 按 manifest 逐张重算并比对 sha256（确定性 + 保真），
cover 512² 按包内 `cover/index.json` 逐张校验字节。

**现役产物**：`release/2d-basemaps/` 43 张（2026-11-01 起 pack index 全量枚举，此前 29 张）
＋ `release/asset_pack/cover/` 43 张封面（512²，44–112KB）＋ cover/index.json；
`release/2d-basemaps-manifest.json`
记录配方、逐件 sha256 与源 ground 的 sha256；与用户对照页验收版本**逐字节一致**。
**WotbTools 侧不再派生**（2026-10-16 决定）：Web 与 APK 随包的就是这一份 canonical 字节，
像素完全一致（此前 Android 用的 ≤1024²/q72 派生链已整体删除）。

**资产 origin 免发版通道（`map2d/` + `index.json`）**：同一次导出还会把同样字节写成
`release/asset_pack/map2d/<code>.webp` + `map2d/index.json`（**schemaVersion 2**：`generated`、
`recipe`、逐图 `path/sha256/bytes/width/height` **＋几何契约 `worldBounds` / `orientation`**，
2026-10-16 起；schemaVersion 1 缺这两项，消费侧一律不换图），并在 staging 目录写下交接件
`map2d-source.json`（schemaVersion 2：`contentGeneratedAt` = 索引 `generated`、`indexSha256`、
`recipe`，以及**与字节的绑定** `maps: {code: {file, sha256}}`（43 条）与 `mapsDigest`）。
该交接件随 43 张 webp 一起进 WotbTools `frontend/src/assets/maps/`：**包内"内容时间戳"的唯一来源**
——运行时用它与资产 origin 索引的 `generated` 做单调比较（`index.generated <= contentGeneratedAt ⇒ 包内即最新`）。
**绝不能在消费侧改成构建时间**：stage 晚于资产发布才构建时会把真正更新的底图判成更旧而永久拒绝升级，
且同一 commit 重跑构建会产出不同字节的 bundle（破坏 stage 可重跑性与 APK 不可变性）。
**绑定是硬要求**：消费侧必须用本地真实字节重算 `maps`/`mapsDigest` 并 fail closed（WotbTools 构建期
比对随包字节、CI 测试比对仓库 canonical 字节；均不查远端 origin）。缺了它，"换图但忘了重跑本工具并同步
交接件"会让旧 `contentGeneratedAt` 配上新图，运行时据索引可能把包内更新的图 OTA 降级。
`--verify` 对这三方（交接件 ↔ 索引 ↔ canonical 字节）逐条同源校验。
复现同一批发布时用 `--generated <ISO-8601Z>` 固定时间戳（不加则每次导出都是新 `generated`）。发布走 §5.2 的
Alibaba 新 release 流程（json → `no-cache`、webp → 短 TTL；COS legacy 的 `--only map2d/`
仅用于回滚维护）。**索引是 App 的唯一判据**：
包内那份是否最新、远端该取哪张，全部按 `sha256` 比对；字节在使用前逐件校验，失败回落包内。
App 侧消费（WotbTools `frontend/src/data/map2dOverlay.js`）与"包内兜底 + 在线校验"的完整规则见
其 `docs/reference/maps.md` §2026-10-16。

**几何契约为什么必须随件落盘**：2D 渲染把回放坐标按注册表 `coordinateBounds`（±300 m）线性映射到
整幅图，**栅格分辨率不参与映射**（底图与标记同框拉伸）——所以 sha256/尺寸全对也仍可能"换了图但取景
变了"（裁剪、补边、换框后尺寸可以不变），只有世界框能判出来。索引 v2 的 `worldBounds` +
`orientation` 是消费侧换图的前置条件：与注册表逐值不符 ⇒ 该图不换，`window.__wotbMap2d.maps[code]`
记 `rejected: geometry`。`--verify` 同时回读**真实像素**：必须方形且与索引声明一致。
**本批（2026-10-16）配准证据**（补齐本批此前未做的几何校验）：29 张逐个与
`map/<key>/ground.webp`（±300 全幅渲染，取向/世界框由 `terrain.json` 契约校验）做零位移相关比对——
移位 (0,0)、取向 identity 最优、尺度 1.000、相关 0.987–0.998（残差 = 导出侧色调/USM 逐点滤波），
即**每一张都是包内渲染的全幅等比降采样**（一次性诊断，不入库；判据仍是契约，不搜索）。

### 5.7 客户端小地图（`map/<key>/mini.webp`）——**已退出地面链路**（2026-10-16 裁定：以 ±300 为准）

`src/wargaming/map_assets.rs` 把小地图 DVPL **原样**写成 `data/cache/maps/<space>.minimap.webp`
（`MiniMapSmall@2x` 优先），`scripts/export_asset_pack.py` 再拷成包内 `map/<key>/mini.webp`。

**实测（2026-10-16，36 张）**：把 `mini.webp` 与同图 `ground.webp`（±300 全幅渲染）对齐，36/36 都
**不落在 600 m 方框上**——小地图内容需要**放大约 3%–7%** 才能与契约全幅对齐（canal/himmelsdorf/
malinovka/mountain 聚在 0.962–0.968 ≈ 覆盖 620 m；medvedkovo 0.926），且外围"均匀带"只有 0–3 px
⇒ 多出来的**不是边框，是真实内容**（水面/场景可以超出 Landscape 的 ±300 框；canal 湖面在
`ground.webp` 里被切在画面外、在小地图里完整闭合）。**即：Landscape `worldBounds = ±300` 是地形框，
不等于客户端 HUD 小地图的覆盖范围。**

**处置（用户裁定，路线 B：以 ±300 为准，外圈不呈现）**：3D 地面贴图**四档统一改用契约化的
`ground.webp`**（`frontend/src/scene/playbackScene.js` 不再按画质档选 `res=mini`；`map-mini`
路由已从 `replaySource.js` 移除；守卫见 WotbTools `playbackScene.test.js`「地面贴图四档统一用
契约化的 `map`」）。历史影响：最低档曾把约 620 m 的小地图按 600 m 铺，内容被压缩约 3% 且圈外内容
摊进边界；2D 底图一直只有 ±300 框 ⇒ 低档 3D 与 2D 对不上（2026-10-16 用户报障）。

**仍在包内的 `mini.webp`**：保留（资产面 `res=mini` 与 `map_minimap_response` 的"提取缓存 → 客户端
随取随解 → 高清底图兜底"链未动），但**没有运行时消费方**；若要省包体（36 张合计约 3.6 MiB）可整体
下线提取与打包，属独立决定。**不要**按像素测量值给 `data/maps/<key>.json` 填逐图 `size_m`
（AGENTS.md：不做测量驱动的逐项决策；无权威判据时 fail closed 并上报）——若将来要走"呈现外圈"的
路线 A，须先取到客户端小地图的**权威世界框**（HUD/地图 UI 配置），再统一归一化重出。

---

## 六、许可


从游戏文件提取的数据（装甲数值、模型、图标）版权归 Wargaming，README 已有声明。
解除 BlitzKit 依赖只把来源从"第三方整理"换成"第一手提取"，**不改变权属**。

## 地形附加数据（让位掩码 / 贴地结构脚印）——已全部撤销

让位掩码（`cover.u16.bin`）与贴地结构脚印（`structs.u8.bin`）两代地形附加件均已撤销
（裁示：**只对齐客户端，不自行发明处理方法**）。**最终态：地形没有任何客户端之外的附加
数据**——包内 `map/<key>/` 只含 `terrain.u16.bin` + `terrain.json`（客户端高度图与元数据）
+ 场景/地面资产；打包器在 sidecar 化时清除历史 cover/structs 字段。地形渲染 = 纯客户端
口径（三判据 LOD + 两通道 morph + 真值高度场一份到底，前端侧实现见 WotbTools
`terrainMesh.js`）。

## 地形高度图布局与提取链（`terrainLayout` 2）

高度图自客户端 `3d/Maps/<space>/landscape/` 下唯一 `*heightmap*.dvpl` 提取（`fetch-terrain`
命令；size/tile 非 512/16 一律拒绝），经 `parse_heightmap` 两步：tile 块重排（块行主序 →
行主序）+ **corner 口径列镜像** `out[c] = grid[min(size−c, size−1)]`——引擎顶点 k 在游戏
x = min + k/size·span（`Heightmap::GetPoint` / `tilemask-vp.sl` 同口径），场景 x = −游戏 x
⇒ 输出列 c 的位置 (c/n−0.5)·span 对应源列 n−c，c=0 夹取到源列 n−1；源列 0（场景 +x 包围盒
边缘、可玩区之外）整列丢弃（n 列表示不下 n+1 个顶点）。产物 = u16 LE 行主序（行 0=南），
以 `map/<key>/terrain.u16.bin` 进包，配 `terrain.json`（`X-Terrain-Meta` 头的 sidecar 化，
写 **`terrainLayout: 2`** 与 **`terrainBinSha256`**——bin 的逐文件 sha256，与 manifest 同源：
`terrain.json`（no-cache）与 bin（max-age=3600）是两个独立请求，发布切换期可能错配
（旧 JSON+新 bin = 双重补正、新 JSON+缓存旧 bin = 漏补正），前端按该 hash 对 bin 的
URL 做缓存破坏即可保证配对；服务端 `/api/playback/terrain` 同链伺服（bin 与
X-Terrain-Meta 同响应返回，天然配对，无需该字段）。

- **缓存自描述**：`data/cache/terrain/{key}.hm.c2.u16.bin`（文件名带布局）。打包器仅无 c2
  时兜底打包且**不写布局标记**（消费端读不到 `terrainLayout` 会 fail-closed 回退 2D——
  必须重跑 `fetch-terrain --force`；不再按旧布局猜）；服务端不再伺服旧名（视作未命中走
  实时提取重写）；`fetch-terrain` 成功写入 c2 后清除旧名。
- **标记必须与字节一致**：消费端（WotbTools `terrainMesh.js`）要求
  `size`/`terrainLayout`/`zmin`/`zmax`/`span` 齐全；`terrainLayout: 2` 原样使用，
  `terrainLayout: 1` 仅作显式旧布局平移一列；缺字段/未知布局 fail-closed 回退 2D——
  不再按缺省 `layout=1`/`zmax=100` 解码（那会把 layout 2 的 bin 平移一列、高度缩放错）。
  `worldBounds` 必须是居中正方形（当前消费口径）；2026-10-18 对 36 张 `terrain.json`
  实测全部 `[-300,300]²`、`terrainLayout: 2`、0 违反。json/bin 两个请求的 HTTP 缓存错配
  窗口（见上）由前端按 `terrainBinSha256` 配对根除；有 sha 但无 WebCrypto 时也 fail-closed。
  **blitzinfo 的 `landscape.crop`/`m` 不在本仓契约内**：当前高度场按 `worldBounds` 全幅
  解释；若未来客户端数据出现非恒等 crop/矩阵，需先补契约字段与测试，不能按字段名直接猜。
- 手动覆盖 `data/maps/<key>.heightmap.u16.bin` 的契约 = corner 口径镜像数据（即
  `parse_heightmap` 的输出格式）。
- 重导链：`fetch-terrain --force` → `export_asset_pack.py` → `composite_overhead`（重导会
  覆写 overhead 烘焙地面，必须重烘——见 §5.5 世代硬约束）→ `--refresh-manifest` → 按 §5.2
  发布到 Alibaba origin 的新 release；
  地形件 ~512 KB/图 × 36 图，模型/地面纹理/场景 GLB 不受影响。
