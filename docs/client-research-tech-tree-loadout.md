# 客户端研发 / 科技树 / 装备-消耗品系统（WotbTools 复刻基线）

> 证据基线：**WoT Blitz `11.21.0.1235_7376187` / `release/11.21.0` / `WOTB_Win7`**
> （本机 Steam 客户端 `D:/SteamLibrary/steamapps/common/World of Tanks Blitz/Data`；
> 仓库数据版本 `data/data_version.json` = `release/11.21.0`、738 辆）。
>
> 本文是「客户端静态数据 + UI 原型」的**系统性事实清单**，用于在 WotbTools 复刻
> 研发/科技树/装备/消耗品/参数预览。每条结论标注证据等级：
>
> - ✅ **数据直给**：客户端文件里有原值，可直接读；
> - ◐ **结构派生**：由字段语义/文件结构推出，并已在全量数据上做一致性校验（附校验口径）；
> - ⛔ **代码内语义**：数据文件里没有，只存在于客户端可执行文件；复刻时必须
>   fail-closed 或另行验证，禁止用样本统计猜。
>
> 本文不改变任何现有管线；它描述的是**下一批可选的数据面/前端能力**。AGENTS.md 的
> 「不做统计判定」规则在本文所有复刻建议中同样适用。

---

## 0. 结论摘要

1. **研发图与科技树是两份不同的数据，不能互推**：
   - 研发图（谁解锁谁、多少经验）在**车辆 XML 的 `<unlocks>`** 里（模块 → 模块 / 整车）；
   - 科技树 yaml（`Configs/TechTree/{nation}_tree.yaml`）**只有显示坐标与 `premium_rows`**，
     没有任何边或成本。
2. **模块不是「一辆车一张清单」**：底盘/炮塔/火炮是**逐车内联**定义（同一门炮在不同
   炮塔上数值不同）；发动机/电台/油箱是**国家级共享组件**（`<V-2>shared` 引用）。
3. **装备是全局固定骨架**：每车 **9 槽 = 3 组（火力/防护/特殊）× 3 级（I/II/III）×
   每槽 2 个候选**；全服只有 **level-0** 的三个槽会因 `optDevicePreset`（9 个定义、
   8 个在用）换件，level-1/2 六个槽在所有预设里完全相同。共 24 件装备定义，其中
   `stereoscope` 未被任何预设引用。
4. **消耗品/补给是逐车槽位 + 过滤器**：`consumableSlots` 1..3、`provisionSlots` 0..3；
   物品用 `vehicleFilter`（等级/国家/点名/extendedTags，含 exclude）决定能否上车；
   标准战斗物品 13 消耗品 + 34 补给，另有 7/8 个模式专属文件（64/85 条总量）。
5. **回放能给出「本场真实装配」的装备 id**：Type5 物化尾部 `0B <n>` + nB（100..123），
   仓库已有 `collect_vehicle_loadout`。这是把「装备 → 参数」落到真实对局的桥。
6. **HP 有独立校验；`weight` 是编码陷阱；DPM 只是定义式**：
   - `HP = hull.maxHealth + max(炮塔.maxHealth)`（738/738；另有回放残差 0/5/6/9/11% 的
     独立支持，见 §2.5）；
   - `DPM = 单发伤害 × 60 / 装填` 是**本仓的定义式**（`tank_configs.rs` 自己算的，
     1308/1308 只是编码自洽，不是客户端验证）；
   - **`tanks.pb` field31 / `tank_data.configs[].weight` = `hull.weight`，是编码事实，
     不是战斗重量**。客户端战斗重量 = 车体 + 所选模块权重之和，**会随炮塔/炮/底盘/发动机
     变化**。初版把编码事实误写成“重量不随模块变化”，已纠正（§2.5/§8.2）。
7. **三项原本“只有客户端代码”的语义，已有两项的对照实现**（见 §8.5）：
   - 装备/消耗品的系数合成：BlitzKit `coefficient()` = 同调用内加法、跨来源乘法；
   - 乘员等级曲线：BlitzKit `degressiveStat(crew) = 0.875/(0.375*crew+0.5)-1`；
   - **仍无对照实现的是模块树节点 x/y 布局**（`modulesModel` 模型侧），只能先用
     确定性分层布局并在 UI 标注“复刻布局”。

---

## 1. 数据源地图

所有路径相对客户端 `Data/`。所有 `.dvpl` 用仓库已有的
`tools/wotbtools/wotb_sc2.py::decode_dvpl` 解码（`DVPL_FOOTER = "<III4s4s"`）。

| 数据 | 路径 | 内容 | 证据 |
|---|---|---|---|
| 车辆清单/售价/等级 | `XML/item_defs/vehicles/{nation}/list.xml.dvpl` | `id`（list id）、`price`（credits 或 `prx_*:amount`）、`level`（tier）、`tags`、`combatRole`、`enrichmentPermanentCost` | ✅ |
| 车辆定义 | `XML/item_defs/vehicles/{nation}/{stem}.xml.dvpl` | 乘员、速度、隐蔽、槽位数、`optDevicePreset`、内联模块（chassis/turrets+guns）、共享模块引用（engines/radios/fuelTanks）、`<unlocks>` | ✅ |
| 共享模块 | `XML/item_defs/vehicles/{nation}/components/{chassis,turrets,guns,shells,engines,radios,fuelTanks}.xml.dvpl` | `<ids>`（stem → 局部 id）+ `<shared>`（真实定义） | ✅ |
| 科技树 | `Configs/TechTree/{nation}_tree.yaml.dvpl` | `premium_rows` + `tanks: {stem: {position: [col, row]}}` | ✅ |
| 科技树网格（遗留） | `Configs/TechTree/modules_shared.yaml.dvpl`、`tree_shared.yaml.dvpl` | `grid_properties.start/step/offset`；当前 Screens3 UI 不读取，见 §5.3 | ◐ |
| 装备定义 | `XML/item_defs/vehicles/common/optional_devices.xml.dvpl` | 24 件装备：id/名称键/描述键/图标/`<script>`/`<display_params>` | ✅ |
| 装备槽预设 | `XML/item_defs/vehicles/common/optional_device_slots.xml.dvpl` | 9 个预设 × level0/1/2 × 3 组 × device0/device1 | ✅ |
| 消耗品 | `XML/item_defs/vehicles/common/consumables/list.xml.dvpl` → `common/prototypes/madgames/hellgames/lunar/bossmode/gravitizing.xml` | 64 条（标准 `common.xml` 13 条） | ✅ |
| 补给 | `XML/item_defs/vehicles/common/provisions/list.xml.dvpl` → `common/prototypes/madgames/hellgames/vampiric/bossmode/gravitizing/zombie.xml` | 85 条（标准 `common.xml` 34 条） | ✅ |
| 默认消耗品 | `XML/item_defs/vehicles/common/consumablesDefault.yaml.dvpl` | 按 tier（1..10）+ 单车覆盖的默认配置 | ✅ |
| 参数结构 | `hangarParamsStructureConfig.yaml.dvpl` | 参数分组（base/rawbase/protectability/gun/mobility + 逐组件）、表格列、子节 | ✅ |
| 参数格式 | `hangarParamsConfig.yaml.dvpl` | 每个参数的单位/小数位/`biggerIsBetter` | ✅ |
| 平衡归一 | `hangarBaseParamsBalance.yaml.dvpl`、`hangarBaseParamsPrecomputedData_generated.yaml.dvpl` | 参数归一化权重/分位范围（UI 条形图用） | ✅ |
| 科技树/研发 UI | `UI/Screens3/Lobby/{TanksTechTree,ModulesTree}/*.yaml.dvpl` | 屏幕原型、网格常量、数据绑定表达式、状态机 | ✅ |
| 装备/消耗品 UI | `UI/Screens3/Lobby/Inventory/OptionalDevices/*`、`InventoryPanel/InventoryPanelContent/{ConsumablesContent,ModulesContent,OptionalDevicesContent}.yaml.dvpl` | 槽位/物品面板 | ✅ |
| 参数面板 UI | `UI/Screens3/Lobby/Common/VehicleParamsPanel/*.yaml.dvpl` | 参数分组渲染、`changed` 差分节 | ✅ |
| 本地化 | `Strings/en.yaml.dvpl` | 装备/补给/消耗品名与描述模板（`%(...)` 占位） | ✅ |
| 图标 | `Gfx/UI/ModulesTechTree/vehicle{Chassis,Turret,Gun,Engine,Hull}` | 模块节点图标（无 radio/fuelTank 专用图标） | ✅ |

**注意**：

- `XML/item_defs.zip` 是**旧快照**（869 条 vs 磁盘 881 条），不要读；提取链一律读磁盘
  逐文件（仓库 `tools/extract_vehicles.py` 顶部已声明该铁律）。
- XML 重复标签是 **last-wins**（`armor_N`/`pitchLimits`/`transition`）。
- `hangarParams*.yaml` 是 **keyedArchive YAML**（不是普通 YAML），读取后先按
  `keyedArchive` 层级解包。
- 本地化键有两种前缀：`optionalDevices/...`（装备）与 `#artefacts:...`（消耗品/补给），
  另有 `consumables/...`、`provisions/...` 的新式键；**同一物品可能只有其中一种**。

---

## 2. 车辆与模块数据模型

### 2.1 车辆节点（`list.xml`）✅

```xml
<T-34>
  <id>0</id>                          <!-- list id；tank_id = (id << 8) | nation_low -->
  <price>400000</price>               <!-- credits；也可能是 prx_season_1:570 这类代币串 -->
  <enrichmentPermanentCost>3800</enrichmentPermanentCost>
  <tags>mediumTank enhancedSuspension_Christie_user mediumCaliberTankRammer_user ...</tags>
  <level>5</level>
  <combatRole><arcade>cavalry</arcade><hellgames>vampire_2</hellgames>...</combatRole>
  <configurationModes>production development supertest</configurationModes>
</T-34>
```

- `price` 是**字符串语义**：数字 = credits；`prx_<currency>:<amount>` = 活动代币。
  全量 758 条 list 中 **16 辆**为代币价（`Oth36..Oth76` 赛季车）。
- `level` = tier（1..10；11 级超测数据不在本基线）。
- `tags` 是模块/装备过滤器的输入（如 `clip` 用于排除 `berserk`），**不是展示文本**。
- `combatRole` 的键是模式名（`arcade`/`hellgames`/`lunar`/`gravitizing`），值是
  `combat_roles.yaml` 的 role id；role 决定模式默认技能，不改坦克基础参数。

### 2.2 模块 schema：内联 vs 共享 ✅

车辆 XML 的顶层模块容器：

| 容器 | 形态 | 说明 |
|---|---|---|
| `chassis` | 内联 | `<T-34_mod_1941> ... </T-34_mod_1941>`，含全部数值 |
| `turrets0` | 内联 + 嵌套 `guns` | 炮塔内联；**火炮挂在炮塔下**，同一门炮在不同炮塔可不同数值 |
| `engines` | 引用 | `<V-2>shared<unlocks>...</unlocks></V-2>` |
| `radios` | 引用 | `<_9R>shared</_9R>` |
| `fuelTanks` | 引用 | `<Average>shared</Average>` |

共享组件的真实定义在 `{nation}/components/*.xml` 的 `<shared>` 下：

| 组件 | 关键字段 |
|---|---|
| engine | `power`、`fireStartingChance`、`weight`、`maxHealth`、`maxRegenHealth`、`tags`（diesel/gasoline）、`level`、`price` |
| radio | `distance`、`weight`、`maxHealth`、`level`、`price` |
| fuelTank | `weight`、`maxHealth`、`price`（无 `level`） |
| chassis | `rotationSpeed`、`terrainResistance`（3 值：硬/中/软）、`maxLoad`、`weight`、`maxHealth`、`bulkHealthFactor`、`brakeForce`、`level`、`price` |
| turret | `rotationSpeed`、`circularVisionRadius`、`maxHealth`、`weight`、`yawLimits`、`primaryArmor`、`level`、`price` |
| gun | `reloadTime`、`aimingTime`、`shotDispersionRadius`、`maxAmmo`、`pitchLimits`、`rotationSpeed`、`weight`（共享定义）、`invisibilityFactorAtShot`、`<shots>`（弹种列表） |

### 2.3 模块 id ✅

`module_id = (local_id << 8) | nation_low`，`local_id` 来自 `components/*.xml` 的
`<ids>` 表（`engines/radios/fuelTanks/chassis/turrets/guns/shells` 各一张）。
国家低位：`ussr=1 germany=17 usa=33 china=49 france=65 uk=81 japan=97 other=113
european=129`。

用途：`<unlocks>` 的目标是 **stem**，不是 id；id 只在编码 pb/对比 BlitzKit 时需要。
**不要用 id 排序当研发顺序**（BlitzKit 的 models.pb 就是 id 升序，已在本仓
[data-inventory.md](data-inventory.md) 记录为 105 辆枪序/92 辆末位主炮不同）。

### 2.4 模块字段覆盖（全量）✅

对 758 辆车、6204 条模块条目统计（含共享模块的逐车引用）：

| 类别 | 字段（出现率 100% 的在前） |
|---|---|
| chassis（1034） | `level price drivingWheels hullPosition topRightCarryingPoint maxClimbAngle navmeshGirth weight maxLoad terrainResistance brakeForce rotationIsAroundCenter rotationSpeed bulkHealthFactor maxHealth maxRegenHealth hysteresisHealth repairCost healthRegenPerSec`；另 `trackThickness` 43 辆 |
| turret（968） | `level price yawLimits gunPosition weight maxHealth rotationSpeed circularVisionRadius showEmblemsOnGun physicsShape`；`primaryArmor` 940 |
| gun（1577） | `reloadTime invisibilityFactorAtShot aimingTime shotDispersionRadius maxAmmo pitchLimits rotationSpeed`；`weight` 在共享 `guns.xml`（内联不重复）；`turretYawLimits` 256、`pumpGunMode`+`pumpGunReloadTimes` 54 |
| engine/radio/fuelTank | 见 §2.2 共享字段 |

> **注意（2026-10-26 纠正）**：模块 `weight` 是**战斗重量求和的输入**，不是只用于局部
> 语义。`tank_data.configs[].weight` 恒等于 `hull.weight`，这是**编码事实**（§2.5），
> **不能**据此得出“重量不随模块变化”。共享 `guns.xml` 的 135/135 门苏联炮都带
> `weight`；本仓 pb 只是没有把总和编码进去。

### 2.5 派生公式与两个「层」的口径

**HP（口径校验）**：

| 公式 | 校验 | 结论 |
|---|---|---|
| `hp = hull.maxHealth + max(炮塔.maxHealth)` | 738/738（`TankResolver::from_blitzkit` 与 `configs[]` 同源）+ 回放残差独立校验 | 这是 `tank_cache`/`tank_data` 的总 HP 口径（取血量最高的炮塔）；装 stock 炮塔时 = hull + 该炮塔。`tanks.pb` field10 本身只是**车体 HP**（IS-7 = 2040），不要直接当总血量 |
| `dpm = standard_damage * 60 / reload_time` | 定义式（`tank_configs.rs` 计算；1308/1308 只是自洽） | 单发炮；`standard_damage` 取 AP/首个非金币弹；弹夹为 `None`。**不是客户端验证值**，客户端 UI 可能按当前弹种显示 |
| `DPM（弹夹/连发）= damage × 整夹 × 60 / (reload + (组数−1)×intraClip + 组数×(每组−1)×组内间隔)` | BK `resolveDpm` 同式（`gun.burst` 分支）；无连发组 = `reload + (整夹−1)×intraClip` | 组数 = 整夹 / `burst_group_count`；`burst_group_interval` = 客户端 `<burst rate>` 的 `60/rate`（AC Atlas V2：13 + 3 + 1）。pb gun **field 20** = `Burst{count, interval}`；非弹夹炮的同名字段不参与计算 |

> 回放侧独立校验（2026-10-26，`wotb-agent loadout` 87 条玩家记录）：`hp_initial` 相对
> 上述基础 HP 的残差只出现 0 / 5% / 6% / 9% / 11%，与装备/补给的 HP 加成吻合；若基础
> HP 取 hull-only，残差会是**几十个百分点**（远大于观测到的 11%），与观测不符。
> **结论：总 HP = 车体 + 顶级炮塔**。

**`weight`（编码事实 ≠ 游戏语义，2026-10-26 纠正）**：

- `tanks.pb` field31 与 `tank_data.configs[].weight` 都等于 `hull.weight`
  （`tools/extract_vehicles.py` L625；`tools/emit_vehicle_pb.py` field31）——这是
  **编码层事实**，因此同一辆车的所有配置数值相同。它**不能**证明游戏内战斗重量恒定。
- **客户端/BlitzKit 口径**（对照实现，`packages/website/src/core/blitzkit/tankCharacteristics.ts`
  L341-342）：

  ```ts
  const weightKg =
    tank.weight + engine.weight + track.weight + turret.weight + gun.weight;
  ```

  其中 `tank.weight` = 车体重量（= pb field31 / XML `hull.weight`），`track` = 底盘。
  **不含电台/油箱**（两者的 `weight` 在客户端数据里存在，但 BlitzKit 的整车求和不用）。
  因此重量**随炮塔/炮/底盘/发动机变化**，并用于
  `powerToWeightRatio = enginePower / weightTons`（L480-486）。
- **`maxLoad` 是什么**：`chassis.maxLoad` 是底盘的**载重上限**（kg）。但它在 11.21 里
  只是一个数据字段：BlitzKit 的 tank proto **没有 maxLoad**、`tankCharacteristics` 也
  不用它；11.21 的全部 UI YAML/文案**零引用**（只有 `hangarParamsConfig` 里有一条
  `chassiMaxLoad` 的格式化项，且 `hangarParamsStructureConfig` 没有把它放进参数面板）；
  模块安装的锁定原因枚举 `eModulesActionLockReason` 也没有“超重”项。**因此没有证据表明
  Blitz 11.21 用 maxLoad 做装配校验或超重惩罚**——它更像遗留字段，只能做一致性旁证。
  这一条的最终裁决仍需客户端代码。
- 旁证（按上面的 BK 公式、各模块取文档序末位 = 研发序顶级）：`747/758 ≤ maxLoad`，
  `total/maxLoad` 中位数 **0.908**（p25 0.853 / p75 0.940）；只算 hull 时中位数
  **0.458**。典型车：T-34 29.6/31.3 t（0.946）、T-95 87.4/91.9 t（0.951）、
  Maus 188.5/192.9 t（0.977）。
- **11 辆超限（BK 公式；必须逐条处理，禁止用比例修正）**：其中 3 辆
  `maxLoad=25157` 疑似占位值（`Ch47_BZ_176`、`It21_Lion`、`Ch60_Type59_spc`）；
  `Ch45_WZ_114` 的 `maxLoad=12000` 甚至小于车体 20820；其余多为 1.0–1.3×，很可能是
  “末位炮塔 × 末位炮”并非可同时装配的组合。复刻时对这类车 fail-closed（不显示总重/
  载重比）或按实际配置求值，不做统计修正。
  清单（`total/maxLoad`）：`Ch45_WZ_114` 3.64、`Ch47_BZ_176` 2.77、`It21_Lion` 1.73、
  `Ch60_Type59_spc` 1.51、`F116_Bat_Chat_Bourrasque` 1.31、`A146_TL_7_120` 1.26、
  `Ch44_114_SP2` 1.19、`R119_Object_777` 1.19、`Ch22_113` 1.06、
  `JagdPzIV_AusfBP` 1.02、`IS-7` 1.01。
- 客户端是否把电台/油箱计入总重：**未直接验证**；BlitzKit 的实现是**不计**。
  若加上两者（几百 kg），`GB21_A27BP` 会从 1.00 变成略超 1.0，超限清单变 12 辆。

装甲：pb `armor.hull.front/sides/rear` = `hull.primaryArmor` 指向的 `armor_N` 板值
（`primaryArmor = front sides rear` 顺序）；炮塔同理。✅（仓库提取链已实现）

---

## 3. 研发图（`<unlocks>`）✅

### 3.1 结构

`<unlocks>` 是**每个模块自己的**子节点，列出它解锁的下一层：

```xml
<!-- T-34：底盘链 -->
<T-34_mod_1941>
  <level>4</level><price>0</price> ...
  <unlocks><chassis>T-34_mod_1943<cost>7000</cost></chassis></unlocks>
</T-34_mod_1941>

<!-- T-34：炮塔 → 炮 → 整车（注意嵌套是 XML 树，不是额外语义） -->
<T-34_mod_1940>
  <unlocks><turret>T-34_mod_1942<cost>3000</cost></turret></unlocks>
  <guns>
    <_76mm_L-11>
      <unlocks><gun>_76mm_F-34<cost>2000</cost></gun></unlocks>
      ...
```

T-34 的完整研发图（实测）：

```
chassis: T-34_mod_1941 --7000--> T-34_mod_1943
turret : T-34_mod_1940 --3000--> T-34_mod_1942
guns(T-34_mod_1940): L-11 --2000--> F-34 --2000--> ZiS-4 --31000--> vehicle MT25
guns(T-34_mod_1942): (L-11/F-34/ZiS-4 同上，数值不同) ; S-54 --29000--> vehicle T-34-85
engine : V-2 --3000--> V-2-34
```

要点：

- **整车解锁挂在某个具体模块上**（通常是顶级炮/顶级炮塔）；科技树的边 = 这些
  `<vehicle>` 解锁，而不是「同级相邻」。
- 同一门炮可出现在多个炮塔的 `<guns>` 里，**数值独立**（T-34 的 F-34 在 stock
  炮塔 `reloadTime=11.5`，在升级炮塔 `10.8`）。
- `cost` 是**研发成本**，不是模块购买价；`<price>` 是模块的 credits 价（多数为 0）。

### 3.2 全量统计（758 辆 / 6204 模块）✅

| 边类型 | 数量 | 说明 |
|---|---|---|
| `chassis` | 276 | 底盘 → 下一底盘 |
| `turret` | 208 | 炮塔 → 下一炮塔 |
| `gun` | 457 | 炮 → 下一炮 |
| `engine` | 351 | 发动机 → 下一发动机 |
| `vehicle` | 310 | 模块 → **下一辆坦克** |
| 合计 | **1410** | |

- **462 辆没有任何 `<unlocks>` 边**（金币/收藏/活动车：不可研发）。
- `cost` 语义：
  - 数字 = XP；范围 `chassis 700..40000`、`turret 45..48000`、`gun 45..101500`、
    `engine 450..65500`、`vehicle 210..350000`（车辆 p50 = 75000）；
  - `prx_<currency>:<amount>` = 活动代币（**192 条边**：chassis 49 / gun 49 / turret 45 /
    engine 49），全部出现在 16 辆赛季车上；
  - 2 条 `cost=0` 的边（免费解锁）。
- 车辆目标出现 **16 辆多父节点**（同一辆车可由两条不同分支解锁），例如
  `ussr:KV-3`（KV-1s / KV2）、`germany:Ferdinand`（JagdPanther / PzVI_Tiger_P）、
  `usa:T28`（T25_AT / T28_Prototype）。
- 除多父节点外的 278 条 vehicle 边构成树的骨架；各树 10 个根（部分根不在
  TechTree yaml 的 `tanks:` 里，例如 `ussr:MS-1` 解锁 T-26 但自身不显示）。

### 3.3 模块购买价 ✅

- 6204 条模块条目中 **6178 条 `<price>=0`**；26 条非零，集中在 **9 辆车**
  （中国 IS-2/110/111-1-2-3/113 与法国 B-C 25t / AMX AC / AMX 50 Foch / Foch 155 /
  Renault FT AC）。
- 非零价与研发成本**并存**（例：Ch10_IS2 的底盘 `price=12990` credits，升级底盘
  `cost=9000` XP）。复刻时 UI 要区分「研发 XP」与「购买 credits」两种资源。

---

## 4. 科技树布局 ✅

### 4.1 yaml 结构

```yaml
premium_rows: 2
tanks:
    T-26:   {position: [1, 5]}
    BT-2:   {position: [2, 5]}
    ...
    Object252: {position: [8, 2]}
```

- `position = [列, 行]`，**列**是显示等级（通常 = tier 1..10），**行**是分支行。
  不是 `[行, 列]`——Screens3 的等级锚点是水平 `LinearLayout`，`TankCell` 绑定
  `GridCellComponent.column = model.x`。
- `premium_rows` 是科技树下方「PREMIUM TANKS」带的行数（`TANK_TREE_PREMIUM_VEHICLES_LABEL`）。
- **特例**：`other_tree.yaml` 的列是 1..7，而行恒为 4；这 7 辆车的实际 tier 是 4..10。
  所以 **`position[0]` ≠ tier**，tier 一律读 `list.xml/<level>`。

### 4.2 全量（304 个树节点）✅

| 国家 | 节点 | 列范围 | 行范围 | premium_rows |
|---|---|---|---|---|
| ussr | 41 | 1..10 | 2..9 | 2 |
| germany | 48 | 1..10 | 1..11 | 2 |
| usa | 43 | 1..10 | 1..8 | 1 |
| china | 28 | 1..10 | 2..6 | 1 |
| france | 27 | 1..10 | 2..5 | 1 |
| uk | 32 | 1..10 | 2..7 | 2 |
| japan | 26 | 1..10 | 2..5 | 1 |
| other | 7 | 1..7 | 4..4 | 1 |
| european | 52 | 1..10 | 2..10 | 1 |
| **合计** | **304** | | | |

- `list.xml` 共 758 辆；**454 辆不在树里**（金币/收藏/活动/未实装展示位）。
- 树节点与研发边不完全重合：树里有 10 个「无父节点」的根（含 `Matilda_II_LL`、
  `Object252`、`T23E3` 等非科技树常规车），它们靠 yaml 坐标显示，但没有入边。

### 4.3 `prem_tanks.yaml`（语义未定）◐

```yaml
"ussr:T-34": ["usa:T23E3"]
"ussr:IS":   ["ussr:Object252"]
...
```

12 个键（全部是科技树车）→ 金币车列表。它**不是**科技树边的来源（边在 `<unlocks>`），
也不覆盖全部金币车。客户端如何消费它（金币车展示位/购买前提/活动关联）**数据本身
不能证明**——复刻时不要据此摆放 premium 单元，除非另做验证。

---

## 4.4 客户端模型接口（`wotblitz.exe` 逆向，2026-10-27）

> 目标：`D:/SteamLibrary/steamapps/common/World of Tanks Blitz/wotblitz.exe`
> （PE32 x86，73,841,664 B，sha256 `5030ee4b9f6ec7db1c06efa2b92f9dd7243574be5d8d9578c118119318bc9608`）。
> 方法：PE 节表 + 原始字节扫描定位反射属性名，再定位注册代码；**未运行目标**。

- 类名表（`.rdata` 0x3a2df60 起）：`ModuleNodeModel`、`TankModulesTreeModel`、
  `ModulesTreeNodeModel`、`VehicleNodeModel`——与 Screens3 yaml 的绑定名一致，
  说明网格坐标确由 **`TankModulesTreeModel` 在运行时计算**，不在任何 `Configs/` 数据里。
- `TankModulesTreeModel` 属性（注册字符串，`.rdata` 0x39efd28 起）：`modulesNodes`、
  `currentVehicle`、`prevNodes`、`selectedNode`、`progressingNode`、`currentAction`、
  `actionLockReason`、`canAffordResearch`、**`horizontalSize`/`verticalSize`**、
  `selectedCompactDescr`、`prevTankDescr`、`upgradeToTopPrice`、
  `tempPremiumizationPrice`/`permPremiumizationPrice`、`isPremiumizationAvailable`、
  `isCommonTank`/`isSeasonTank`/`isSeasonActive`、`isBlitzKitInfoAvailable`、
  `eResearchDialogType`。
- `ModulesTreeNodeModel` 的连线方向只有 6 个布尔属性：**`hasArrowLeftUp`、
  `hasArrowLeftDown`、`hasArrowUp`、`hasArrowDown`、`hasArrowLeft`、`hasArrowRight`**
  （按属性序号 0x28..0x2d 顺序注册，注册点 0x28fac4b 起）——即每个节点**最多 6 个方向、
  每个方向一个箭头**，与「每个节点只有一条上级连线、子节点可上/下偏置」的树形布局一致，
  不支持「一条边一条独立箭头」的任意 DAG 画法。
- `eModuleNodeContentType` 注册（0x290b1b3）同时挂 `MODULE`(=2) 与
  `eVehicleNodeStatus` 枚举值，模块/车辆节点共用同一网格模型。
- `Configs/TechTree/modules_shared.yaml`（`start/step/offset` 31/100,104/26,40）在全量
  3473 个 UI 定义中零引用（census §5.3 旧结论仍成立），不是布局来源。

**待完成**：`TankModulesTreeModel` 的节点坐标算法（列/行分配与 6 向箭头判定）需要
Ghidra 反编译该模型更新函数；73 MB x86 的自动分析耗时较长，尚未取到函数体。

## 5. UI 交互契约（Screens3）

> 客户端 UI 是数据驱动 YAML：`UIDataParamsComponent.params` 声明输入模型，
> `UIDataLocalBindingsComponent.data` 是绑定表达式，`UIActionComponent.actionsPath`
> 是动作脚本。**这些绑定表达式就是复刻所需的最小数据契约**。

### 5.1 屏幕与入口 ✅

| 屏幕 | 原型 | 作用 |
|---|---|---|
| 科技树 | `UI/Screens3/Lobby/TanksTechTree/TanksTechTree.yaml` | 国家科技树 + premium 带 |
| 单车研发页 | `UI/Screens3/Lobby/ModulesTree/ModulesTree.yaml` | 上一辆车 / 当前车 / 模块网格 / 参数面板 |
| 研发确认 | `ModulesTree/ResearchDialog.yaml` | XP/金币支付、一键升级 |
| 装备 | `UI/Screens3/Lobby/Inventory/OptionalDevices/OptionalDevices.yaml` | 9 槽 × 2 候选 |
| 消耗品/模块库存 | `UI/Screens3/Lobby/InventoryPanel/InventoryPanelContent/*.yaml` | 槽位式装配 |
| 参数面板 | `UI/Screens3/Lobby/Common/VehicleParamsPanel/VehicleParamsPanel.yaml` | 分组参数 + `changed` 差分 |

### 5.2 单车研发页（ModulesTree）✅

根控件 872×562。横向结构（`TreeContainer`，`LinearLayout` spacing 39）：

```
[PreviousTanks] [ToPrevTank] [CurrentTank] [Modules 网格] [RightEdge]
                              (TankPanel 或 TreeNode)   (502,64)
```

模块网格常量（**可直接复用**）：

| 项 | 值 |
|---|---|
| 节点尺寸 | 64×64（`ModuleNode` 声明 64×72，SizePolicy 固定 64×64） |
| 列宽 / 列间距 | 64 / 40（pitch 104） |
| 行高 / 行间距 | 64 / 33（pitch 97） |
| `columnCount` | `modulesModel.horizontalSize` |
| `rowCount` | `modulesModel.verticalSize` |
| 节点 `column/row` | `nodeModel.x` / `nodeModel.y` |

节点模型 `ModulesTreeNodeModel`（绑定里出现的字段）：

| 字段 | 含义 |
|---|---|
| `contentType` | `eModuleNodeContentType.{MODULE,VEHICLE,EMPTY}` |
| `compactDescr` | 选中/持久化身份 |
| `x`, `y` | 网格坐标（**客户端模型计算，数据文件没有**） |
| `module.status` | `eModuleNodeStatus.{LOCKED,CAN_BE_RESEARCHED,INSTALLED}` |
| `module.price` | `{type, quantity, name}`；`type==stuff` 时是活动代币/ artefacts |
| `module.currentSummaryResearchPrice` | 当前累计研发价（含折扣/碎片进度） |
| `module.level` | 模块等级（chassis/turret 有；gun 无 `level`） |
| `module.specialType` | `DrumGun`/`PumpGun`/`AtgmGun`/`InternalTrack` → 角标图标 |
| `hasArrowLeft/Right/Up/Down/LeftUp/LeftDown` | 与相邻节点的连线方向 |
| `vehicle.status` | `eVehicleNodeStatus.{LOCKED,CAN_BE_RESEARCHED,CAN_BE_PURCHASED,NOT_ENOUGH_MONEY,OWNED}` |
| `vehicle.xp.quantity` | 该车战斗经验 |
| `vehicle.researchPrice` / `purchasePrice` | 研发价 / 购买价（两种资源） |
| `vehicle.vehicleDataModel.isPremium/isCollectible` | 展示分支 |
| `tankStatsModel.isPremiumized` | 「高级化」状态 |

页面模型 `TankModulesTreeModel`：

| 字段 | 用途 |
|---|---|
| `currentVehicle` | 当前车节点 |
| `prevNodes` / `prevTankDescr` | 左侧「上一辆车」列表/跳转 |
| `modulesNodes` | 扁平节点数组，按 `x/y` 落入网格 |
| `horizontalSize` / `verticalSize` | 网格列/行数 |
| `selectedCompactDescr` | 选中节点 |
| `upgradeToTopPrice` | 一键升到顶级的价格（金币） |
| `tempPremiumizationPrice` / `permPremiumizationPrice` | 限时/永久高级化价格 |
| `currentAction` | `eModulesAction.{NONE,RESEARCH,BUY,INSTALL,SELL,REPAIR,SHOW_TANK}` |
| `actionLockReason` | `INSTALLED`/`NO_MODULE`/`NEED_BUY_TANK`/`UNLOCK_BY_CLAN`/`NO_CURRENT_TANK`/`NO_PREV_TANK` |
| `isSeasonTank` | 赛季车（模块用 artefacts 支付） |
| `canAffordResearch` | 研发可负担 |
| `mechanics` | 机制提示（传参数面板） |

节点视觉状态（`ModuleNode.yaml` 绑定）：

| 状态 | 背景 | 动画 |
|---|---|---|
| 已研发 | `grey-shark-70-bg` | 无 |
| 未研发 | `white-wild-sand-15-bg` | 可负担且 `vehicleStatus==OWNED`：蓝光（车经验够）/ 红光（需自由经验） |
| 选中 | `Selection` 覆盖 | — |

### 5.3 科技树页（TanksTechTree）✅

| 常量 | 值 |
|---|---|
| 单元格 | 216×96（`TankCell` 固定尺寸） |
| 等级锚点 | `levels = [1..10]`，水平排列 |
| 网格尺寸 | `columnWidth * cells.columnsCount` × `rowHeight * cells.rowsCount` |
| Premium 带 | 宽 `columnsCount * columnWidth * 2`，高 `premiumRows * rowHeight` |

单元格模型 `TechTreeCell`（绑定字段）：

| 字段 | 含义 |
|---|---|
| `x`, `y` | 网格坐标（`GridCellComponent.column/row`） |
| `vehicleNodeModel` | 车辆节点（同 §5.2 的 vehicle 状态机） |
| `linesMask` | 连线图案 id → `~res:/UI/Screens3/Lobby/TanksTechTree/Lines/Lines{N}.yaml` |
| `licenseStatus` | 科技树许可证：`NOT_AVAILABLE`/`READY_FOR_SELECTION`/`SELECTED` |
| `isCommentedNode` / `isRequestFocus` | 新手引导/聚焦 |

**连线不是动态画线**：`linesMask` 选 28 个手写图案之一（`Lines15`、`Lines128` … `Lines10880`）。
图案由 `ArrowLine/Line` + `Arrow/Arrow` 子控件组成，命名如 `VL`、`RightHLine`、
`BottomVLine`、`BottomArrow`、`LeftHLine`、`LeftCenterArrow`。

**（2026-10-27 修正 + 补充提取）**：图案文件本身**是客户端包内数据**
（`UI/Screens3/Lobby/TanksTechTree/Lines{N}.yaml.dvpl`，11.21 与 11.22 预览包均在；
预览包在 `TanksTechTree/` 直接层，现役包在 `TanksTechTree/Lines/` 子目录）——数据里
没有的只是**每个格子的 mask 编号**（模型计算）。图案几何可完整提取（单位=格内 px，
cell 216×96）：`LeftHLine` 左缘 -0.5 起 13.5px（入线）；`LeftCenterArrow` @13px vCenter
（入线箭头）；`RightHLine` 右缘 0.5 起 19.5px（出线）；`VL`/`VL0`/`VR`/`VR0` 左/右缘
±0.5 的 50%/100% 垂直干线（骑列边界，跨行连接按格分解）；`BottomVLine`/`BottomArrow`
底部中心（同列向下）；`TopArrow` 顶部中心；`LeftHLine1` 第二条入线（bottomAnchor
31.5，双入线场景）。mask 名为位掩码，但位语义未定（28 个观测值无法唯一反解位表）——
复刻按**拓扑路由 + 提取几何**：对每条整车边画「源格右侧 20px → 列边界干线 → 目标格内
13px + 箭头」，与客户端分段分解视觉等价，不依赖 mask 反解。等级条（`LevelCell`）为
216×24、`roman(level)` 罗马数字、可见窗口 `minLevel..maxLevel`；XI 版仅把
`levels` 从 `[1..10]` 扩到 `[1..11]`（预览包 TanksTechTree.yaml diff 唯一实质变化）。

> `Configs/TechTree/tree_shared.yaml`（start/step 15,16 / 200,90）与
> `modules_shared.yaml`（start 30,36 / step 100,104 / offset 26,40）是**遗留常量**：
> 当前 Screens3 的科技树用 216×96 网格，模块树用 64×64 节点 + 104/97 pitch；
> 全部 3473 个 UI 定义文件**零引用**这两个 yaml（模型层是否消费未验证）。
> 仓库内也没有旧版 `UI/Screens/.../ModulesTree` 原型。

### 5.4 研发确认弹窗（ResearchDialog）✅

这是可以直接照抄的**支付算法**（绑定表达式原文）：

```text
maxInt = 2147483000                                  # UI 里的整型上限常量
battleXpNeeded = min(totalPlayerTankXp, researchXpCost)
freeXpNeeded   = min(researchXpCost - battleXpNeeded, totalPlayerFreeXp)
maxPlayerGoldForConvertation = min(int(maxInt / 25), totalPlayerGold)
maxBuyableEliteXp = min(totalPlayerEliteXp,
                        int(maxPlayerGoldForConvertation * 25))
eliteXpNeeded  = min(researchXpCost - battleXpNeeded - freeXpNeeded, maxBuyableEliteXp)
goldForElite   = int(ceil(eliteXpNeeded / 25))          # 25 elite XP = 1 gold
dialogType =
    tankXp >= cost                  -> kBattleXp
    tankXp + freeXp >= cost         -> kFreeXp
    + maxBuyableEliteXp >= cost     -> kEliteXp
    upgradeToTopGoldCost <= gold    -> kUpgradeModules
    else                            -> kNotEnoughXp
```

按钮文案键：`RESEARCH`、`#modules_tech_tree:Top/Button`、
`taskJournal/BuyItemInStore/ProceedToStore/Button`、`#common:DialogButton/Cancel`。

### 5.5 装备页（OptionalDevices）✅

客户端自己的说明文案（`Strings/en.yaml`）：

> `optionalDevices/InfoScreen/WorkingProcess/Description`：
> “For each tank, there are **9 slots with two pieces of equipment in each**.
> … Slots are unlocked sequentially. To unlock level II and III slots, you must first
> unlock level I slots.”
>
> `optionalDevices/InfoScreen/DeviceUsage/Description`：
> “**One type of equipment from each slot** can be mounted … You can switch between
> equipment in a slot at any time, except when the tank is in a battle.”

UI 模型：

| 字段 | 含义 |
|---|---|
| `devices.groups` | 3 组（attack/defence/specialization） |
| `group.slots` | 每组 3 槽（level0/1/2） |
| `slot.devices[0]` / `slot.devices[1]` | 两个候选（`SlotDevices`） |
| `slot` state | `eOptionalDeviceSlotState.{LOCKED,FIRST_INSTALLED,SECOND_INSTALLED}` |
| `devices.deviceColumnsStates[_index]` | 逐组状态 |
| `devices.unlockStatus` | `UNLOCKED` / `READY_TO_UNLOCK` / `REQUIRES_PREVIOUS_TIER` |
| `devices.unlockPrice` | 解锁价格（`ResourceModel`，**服务端模型给，客户端数据没有**） |
| `devices.selectedItem` | 当前选中件 |

组标题本地化：`optionalDevices/Type/{attack,defence,specialization}` =
`COMBAT POWER` / `VITALITY` / `SPECIALIZATION`；等级 `optionalDevices/Level/{1,2,3}` = I/II/III。

### 5.6 消耗品面板 ✅

`ConsumablesContent.yaml` 只按槽位渲染：

```text
slotsCount = tank.info.consumableSlots
slot0 visible = slotsCount > 0
slot1 visible = slotsCount > 1
slot2 visible = slotsCount > 2
```

补给槽位同理（`provisionSlots`，0..3）。战斗内面板
`UI/Screens/Battle/ConsumablesPanel.yaml` + `AimConsumableProgress.yaml` 负责冷却/进度条。

### 5.7 参数面板 ✅

`VehicleParamsPanel` 的结构由 `hangarParamsStructureConfig.yaml` 决定：

- 主分组：`mainSections = ['protectability', 'gun', 'mobility']`；
- 另有 `base`（概览）、`rawbase`、以及逐组件分组 `vehicleChassis`/`vehicleEngine`/
  `vehicleTurret`/`vehicleGun`/`crew`；
- 元素类型：`param` / `subsection` / `subparam` / `ammo_subparam` / `table` / `row` /
  `pairparam` / `baseparam`；
- 每个参数在 `hangarParamsConfig.yaml` 里有 `units`（如 `KILOMETER_PER_HOUR`、
  `DEGREE_PER_SECOND`、`PERCENT`）、`digits`、`biggerIsBetter`。

**差分展示**：`VehicleChangedParamsPanel` 读 `tankParams.sectionsByType["changed"]`，
按 `SectionItem` 重复渲染。客户端在「换模块/换装备/换补给」时用它显示参数变化——
这是复刻「配件对参数影响」时最合适的 UI 形态：**列表 + 前后值/增减**。

---

## 6. 装备（配件）系统

### 6.1 骨架 ✅

- **9 槽 = 3 组 × 3 级**；每槽 **2 个候选**（`device0`/`device1`），玩家二选一；
  槽位状态只有 `LOCKED / FIRST_INSTALLED / SECOND_INSTALLED`——**未解锁槽为空，
  已解锁槽必装其一**（没有「已解锁但空」状态）。
- 全局 24 件装备定义（id 100..123）。
- **所有预设共享同一骨架**；只有 **level-0** 的三个槽因预设改变候选：

| 槽位 | 默认（defaultPreset） A / B |
|---|---|
| L0 · attack | `rammer` / `calibratedShells` |
| L0 · defence | `protectedModules` / `improvedBattleCompartment` |
| L0 · specialization | `coatedOptics` / `camouflageNet` |
| L1 · attack | `enhancedAimDrives` / `increasedPowderCharge` |
| L1 · defence | `multiLayerArmor` / `highQualityAssembly` |
| L1 · specialization | `improvedControls` / `recycledEngineSpeed` |
| L2 · attack | `aimingStabilizer` / `barrelPrecisionChannel` |
| L2 · defence | `reinforcedCaterpillars` / `toolbox` |
| L2 · specialization | `equipmentStorageSystem` / `perfectEquipment` |

- L1/L2 六个槽在**全部实际使用的 8 个预设中完全一致**（`emptyPreset` 除外）。
- 预设差异（相对默认）：

| 预设 | 差异 | 车辆数 |
|---|---|---|
| `drumGunVehiclePreset` | L0 attack A → `improvedVentilation` | 148 |
| `HEprotectionPreset` | L0 defence A/B → `HEprotectedModules` / `HEimprovedBattleCompartment` | 146 |
| `HEdrumGunProtectionPreset` | 以上两项 | 16 |
| `AlternatePitchGrouserPreset` | L0 spec A/B → `improvedPitchLimitsSystem` / `grousers` | 11 |
| `DrumGunPitchLimitsPreset` | 鼓包 + 俯仰/悬挂 | 1 |
| `grousersForGermany` | L0 spec B → `grousers` | 3 |
| `DefaultGrousersWithCamo` | L0 spec A → `grousers`（B 仍 `camouflageNet`） | 1 |
| `defaultPreset` | — | 432 |
| `emptyPreset` | 全空（未使用） | 0 |

### 6.2 24 件装备全表 ✅

`<script>` 是效果权威；`<display_params>` 是 UI 展示值（两者可能单位不同，见 6.3）。

| ID | key | 英文名 | 槽位（默认） | script | 效果（数据原文） | 作用参数 |
|---|---|---|---|---|---|---|
| 100 | `rammer` | Gun Rammer | L0 attack A | StaticFactorDevice | `gunReloadTimeFactor -5%` | 装填时间 |
| 101 | `perfectEquipment` | High-End Consumables | L2 spec B | StaticFactorDevice | `equipmentDurationFactor +33%` | 消耗品/技能持续时间 |
| 102 | `improvedVentilation` | Improved Ventilation | L0 attack A（鼓包预设） | StaticAdditiveDevice | `crewLevelIncrease +8` | 乘员等级 |
| 103 | `calibratedShells` | Calibrated Shells | L0 attack B（全部预设） | CalibratedShells | 穿深：AP/APCR +6%，HEAT/HE +7% | 弹种穿深 |
| 104 | `enhancedAimDrives` | Enhanced Gun Laying Drive | L1 attack A | StaticFactorDevice | `gunAimingTimeFactor -12%` | 瞄准时间 |
| 105 | `aimingStabilizer` | Vertical Stabilizer | L2 attack A | StaticFactorDevice | `additiveShotDispersionFactor -17%` | 移动/转炮扩圈 |
| 106 | `barrelPrecisionChannel` | Refined Gun | L2 attack B | StaticFactorDevice | `shotDispersionAngleFactor -11%` | 100m 散布 |
| 107 | `increasedPowderCharge` | Supercharger | L1 attack B | StaticMiscAttrsModifier | `projectileSpeedFactor +35%`；`piercingPenaltyFactor500m -60%` | 弹速 / 远距穿深衰减 |
| 108 | `protectedModules` | Improved Modules | L0 defence A | ImprovedBattleCompartment | 履带血量 ×1.2；撞击吸收 40% | 模块耐久 |
| 109 | `improvedBattleCompartment` | Defense System | L0 defence B | ImprovedBattleCompartment | 乘员被击 0.85；engine 0.9；ammoBay 0.75 | 乘员/模块被击概率 |
| 110 | `multiLayerArmor` | Enhanced Armor | L1 defence A | StaticFactorDevice | `armorHomogenizationFactor +4%` | 车体/炮塔装甲 |
| 111 | `highQualityAssembly` | Improved Assembly | L1 defence B | HighQualityAssembly | `maxHpBonusPercent +5%` | 最大生命 |
| 112 | `reinforcedCaterpillars` | Enhanced Tracks | L2 defence A | ModulesMaxRegenerator | 履带自动修复 | 履带 |
| 113 | `toolbox` | Toolbox | L2 defence B | StaticFactorDevice | `repairSpeedFactor +20%` | 模块维修速度 |
| 114 | `coatedOptics` | Improved Optics | L0 spec A | CoatedOptics | 视野：LT +11%，MT +8%，HT +6%，TD/SPG +4% | 视野 |
| 115 | `camouflageNet` | Camouflage Net | L0 spec B | CamouflageNet | 静止 3s 后隐蔽 ×2；基础：LT 0.06 / MT 0.04 / HT 0.02 / TD+SPG 0.07 | 隐蔽 |
| 116 | `improvedControls` | Improved Control | L1 spec A | StaticPhysicsAttrsModifier | `rotationSpeedFactor +12%` | 车体转向 |
| 117 | `recycledEngineSpeed` | Engine Accelerator | L1 spec B | RecycledEngine | `enginePowerPercentageBonus +6%` | 发动机功率 |
| 118 | `equipmentStorageSystem` | Consumable Delivery System | L2 spec A | StaticAdditiveDevice | `equipmentReloadBoost +0.17` | 消耗品/技能冷却速度 |
| 119 | `stereoscope` | Binocular Telescope | **未被任何预设引用** | Stereoscope | 静止 2s 后视野 +250（display 25 m） | 视野 |
| 120 | `HEprotectedModules` | Improved Modules + | L0 defence A（HE 预设） | ImprovedBattleCompartment | 同 108 + 大口径 HE 伤害 ×0.9 | 模块耐久 / HE 减伤 |
| 121 | `HEimprovedBattleCompartment` | Defense System + | L0 defence B（HE 预设） | ImprovedBattleCompartment | 同 109 + 大口径 HE 穿深 ×0.9 | 被击概率 / HE 穿深 |
| 122 | `improvedPitchLimitsSystem` | Improved Vertical Stabilizer | L0 spec A（俯仰预设） | ImprovedPitchLimitsSystem | 上仰 +4°、下俯 +3° | 俯仰角 |
| 123 | `grousers` | Improved Suspension | L0 spec（悬挂预设） | Grousers | 硬/中/软地通过性 +20/15/30% | 地形阻力 |

> 名称/描述键：`optionalDevices/<Key>/Name|Description`；描述模板里的 `%(...)`
> 占位符与 `<display_params>` 的键一一对应（例如 `%(gunReloadTimeFactor)`）。
>
> 注：`improvedPitchLimitsSystem` 的客户端英文名是 `Improved Vertical Stabilizer`
> （与 105 `aimingStabilizer` 同名），以 `script=ImprovedPitchLimitsSystem` 与中文语义
> 为准，不要按名字归到「垂直稳定器」。
>
> 消费现状（2026-11-01）：Agent core 在 prop2 俯仰解码时按同 eid 的 Type5 `0B` 配件串
> 是否含 122 叠加 +4°仰/+3°俯（基础与扇区同增益）；WotbTools 装甲查看器/射界图提供
> 122 勾选，数值仍从本表同源的 `devices.json` 读取。

### 6.3 单位与展示口径 ✅

- `display_params` 是**UI 展示值**；`script` 是效果值，二者可能差 10 倍：
  `stereoscope` script `circularVisionRadiusIncrease=250`，display `25`（→ 0.1 m 单位）。
- `rammer` 的 display 是 `-5`，script 也是 `-5`（百分比）。
- `improvedBattleCompartment` 的 display 用 `-10/-15/-25`（减伤百分比），script 用
  `0.9/0.85/0.75`（命中系数）——**不要把 display 当乘数直接算**。
- `coatedOptics` / `camouflageNet` / `recycledEngineSpeed` 按**车辆类别**给不同值；
  script 的类名是 `lightTank/mediumTank/heavyTank/AT-SPG/SPG`，display 的键是
  `Light/Medium/Heavy/AT_SPG`（注意 `AT_SPG` 与 `AT-SPG`）。
- `protectedModules` 的 display `maxHealthPercentageBonus=20` 指的是**履带耐久**，
  不是整车 HP；整车 HP 只有 `highQualityAssembly` 的 `maxHpBonusPercent`。
- **display 与 script 可能直接矛盾**：补给 `safety_set` 的 display 是
  `crewProtectionIncrease=15`，script 是 `crewChanceToHitFactor=0.8695`（= −13.05%）。
  两者不是同一口径（display 是取整/旧文案）。**以 script 为计算权威，display 只用于展示**。
- **描述文本可能包含 script 没有的效果**：消耗品 `berserk` 的描述写
  “+17% loading speed / +10% chance of damaging enemy tank modules”，但 `<script>` 只有
  `gunReloadSpeedIncrease:17`——那 10% 在 `Berserk` 类代码里；`shieldKit` 甚至没有
  `<display_params>`。这类效果**不能从数据复算**，只能按描述原文展示（见 §8.4）。

### 6.4 与回放的关系 ✅

Type5 物化尾部 `0B <n>` + nB（n=1..=9，空槽省略）就是本场装备 id 串：

- id 域 100..123；`103`=校准弹、`110`=强化装甲（仓库常量）；
- **空槽不占位**，所以「哪个 id 在哪个槽」不可恢复；只能得到**集合**；
- 同一预设下，id 集合足以确定装了哪些件（每槽最多一件，id 唯一）；
- 现有实现：`crates/replay-core/src/replay/combat/collect.rs::collect_vehicle_loadout`
  → `VehicleLoadout { equipment, equipment_count, equipment_ids, items, consumables,
  abilities, provisions }`；`equipment_ids` 保留线序实装件，`items` 保留原始 14B 描述符，
  消耗品/给养/模式能力按 `0A` 块解码（给养 = `payload f32 -1.0` 静态尾；`0x67` =
  `gravityJump`/Thruster 单列为能力）。`VehicleEquipment::from_ids` 仍把 103/110 翻成
  射击复现布尔，其余进 `raw`/`equipment_ids`；`0xA9` = `DYNAMIC_ARMOUR_SYSTEM`。
  欧系/瑞典/捷克/波兰食物块 `0x46..0x4D` 与 BlitzKit `provisions.pb` id 103..110 逐项
  相差 33，按每国 HQ(大)→regular(小) 闭合（`0x4A` = Goulash/LARGE_FOOD；该映射纠正
  WotbTools 文档把 `0x48`/`0x49` 互换的推断）。

**已验证的端到端例子**：`tmp_analysis/loadout_1375.json`（历史探针）中，
带「耐久加成」的车 `hp_initial / hp_base = 1.05` 精确成立（2400→2520、1700→1785），
与 `highQualityAssembly` 的 +5% 一致；说明「回放装备 id → 参数效果」这条链是通的。

---

## 7. 消耗品与补给系统

### 7.1 槽位与默认配置 ✅

- 逐车 `<consumableSlots>`（1..3）与 `<provisionSlots>`（0..3）。全量分布：

| 组合（消耗品/补给） | 车辆数 |
|---|---|
| 3 / 3 | 579 |
| 3 / 2 | 56 |
| 3 / 1 | 30 |
| 2 / 0 | 35 |
| 1 / 0 | 58 |

- `consumablesDefault.yaml` 按 **tier** 给默认配置（1..10），并支持单车覆盖
  （`other:Oth30_T54MS`）：

| tier | 默认 |
|---|---|
| 1–2 | `largeRecoverkit` |
| 3 | `largeRecoverkit`, `afterburning` |
| 4–10 | `largeRecoverkit`, `afterburning`, `berserk`, `largeRepairkit` |

### 7.2 过滤器（`vehicleFilter`）◐

每个物品可声明：

```xml
<vehicleFilter>
  <include>
    <vehicle><minLevel>4</minLevel><maxLevel>10</maxLevel></vehicle>
    <vehicle><nations>ussr</nations></vehicle>
    <vehicle><name>ussr:IS-7 ussr:IS-4</name></vehicle>
    <vehicle><extendedTags>clip</extendedTags></vehicle>
  </include>
  <exclude>
    <vehicle><name>germany:S01_Frankentank</name></vehicle>
    <vehicle><extendedTags>clip</extendedTags></vehicle>
  </exclude>
</vehicleFilter>
```

- 规则是**同一 include 内 AND、不同 `<vehicle>` 之间 OR**；exclude 命中即不可用。
  这是对 XML 结构的读法；**求值顺序由客户端代码实现，数据只给结构**——复刻时按此实现、
  并在 UI 上抽验（未命中/误命中都要能解释）。
- `extendedTags` 匹配车辆 `list.xml/<tags>`（例如 `berserk` 排除 `clip` 车）。
- 复刻必须按**目标车辆的 tier/nation/tags** 逐条求值，不能只看“通用”就展示。

### 7.3 标准消耗品（`common.xml`，13 条）✅

| id | key | 名 | 类别 | script 与关键参数 | 过滤器 |
|---|---|---|---|---|---|
| 1 | `autoExtinguisher` | Automatic Fire Extinguisher | free | `Extinguisher{automatic:true, cooldown:70}` | tier 4..10 |
| 3 | `largeMedkit` | First Aid Kit | free | `Repairkit{cooldown:75}` | tier 4..10，排除 `S01_Frankentank` |
| 5 | `largeRepairkit` | Repair Kit | common | `Repairkit{cooldown:75}` | tier 4..10 |
| 14 | `afterburning` | Engine Power Boost | potent | `Afterburning{enginePowerIncrease:20, duration:12, cooldown:65}` | tier 3..10 |
| 18 | `berserk` | Adrenaline | radical | `Berserk{gunReloadSpeedIncrease:17, duration:20, cooldown:80}` | tier 4..10，排除 `clip` |
| 20 | `largeRecoverkit` | Multi-Purpose Restoration Pack | potent | `Recoverkit{cooldown:100}` | tier 1..10 |
| 27 | `improvedAfterburning` | Improved Engine Power Boost | radical | `ImprovedAfterburning{enginePowerIncrease:1.35, rotationSpeedFactor:1.05, fwdSpeedLimitBias:4, bkwdSpeedLimitBias:8, duration:15, cooldown:65}` | 点名 57 辆 |
| 28 | `fireControlSystem` | Reticle Calibration | radical | `FireControlSystem{shotDispersionAngleFactor:0.7, aimingTimeFactor:0.7, duration:20, cooldown:55}` | 点名 43 辆 |
| 29 | `newAutoloader` | Shell Reload Boost | radical | `NewAutoloader{clipReloadTimeFactor:0.7, gunReloadTimeFactor:1, duration:20, cooldown:65}` | 点名 12 辆（鼓包） |
| 33 | `shieldKit` | Reactive Armor | radical | `ShieldKit{shieldCoef:0.27, duration:10, cooldown:65}` | 点名 26 辆 |
| 45 | `tungstenTip` | Tungsten Tip | radical | `TungstenTip{minDamageFactor:1.05, maxDamageFactor:1.25, duration:15, cooldown:70}` | 点名 55 辆 |
| 73 | `armorMover` | Dynamic Armor System | radical | `ArmorMover{damageReductionPercent:10, duration:17, cooldown:60}` | 点名 5 辆 |
| 80 | `freeAfterburning` | Reduced Engine Power Boost | free | `ImprovedAfterburning{enginePowerIncrease:1.10, duration:10, cooldown:70}` | tier 4..10 |

> `afterburning` 的 `20` 是**百分比**（描述模板 `+%(enginePowerIncrease)%`）；
> `improvedAfterburning` 的 `1.35` 是**乘数**（描述只给展示值 `35`，脚本是 1.35）。
> 同名 script 类型在不同条目里单位不同，必须按字段名（`...Increase` vs `...Factor`）
> 与描述模板的 `%` 号区分，不能按数值大小猜。
> `tungstenTip` 同理：script `minDamageFactor=1.05 / maxDamageFactor=1.25`，
> display 写 `105 / 125`（百分比表示），`avgDamage=15` 是均值增幅。

### 7.4 标准补给（`common.xml`，34 条）✅

按效果族归纳（完整 34 条；其中 **24 条是国家级食物**：12 个 `crewLevelIncrease +10` +
12 个 `+3`，同效果不同名字/国家）：

| 族 | key | 效果 |
|---|---|---|
| 乘员等级 +10 | `chocolate`, `cocacola`, `ration`, `ration_uk`, `ration_japan`, `ration_china`, `ration_other`, `hot_coffee`, `ration_european_hq`, `ration_swedish_hq`, `ration_czech_hq`, `ration_polish_hq` | `Stimulator{crewLevelIncrease:10}`；tier 6..10 + 国家过滤 |
| 乘员等级 +3 | `chocolate_bar`, `cocacola_can`, `condensed_milk`, `black_tea`, `white_rice`, `regular_ration`, `regular_ration_other`, `pasty`, `ration_european_regular`, `ration_swedish_regular`, `ration_czech_regular`, `ration_polish_regular` | `Stimulator{crewLevelIncrease:3}`；tier 4..10 + 国家过滤 |
| 燃油 | `regular_fuel` / `improved_fuel` | `Fuel{enginePowerIncrease:3/10, turretRotationSpeedIncrease:3/10}`（百分比） |
| 防护套件 | `safety_set` | `SafetySet{crewChanceToHitFactor:0.8695, repairSpeedIncrease:10, fireProtectionIncrease:20, explosivesHealthIncrease:10}` |
| 沙袋装甲 | `smallHPStock` / `largeHPStock` | `SmallHPStock{hpStockPercent:3/6}`（**整车 HP**） |
| 齿轮油 | `gearOil` / `improvedGearOil` | `GearOil{fwdSpeedLimitBias:2/4, bkwdSpeedLimitBias:2/4, enginePowerIncrease:3/6}` |
| 火药 | `gunPowder` | `GunPowder{projectileSpeedFactor:35}` |
| 反 HE | `antiHighExplosive` | `AntiHighExplosive{factor:0.8}`（HE 伤害 ×0.8） |
| 沙漠动力 | `desertPower` | `AttributesModifier{firmGroundPassabilityIncrease:30, mediumGroundPassabilityIncrease:30, repairSpeedFactor:1.5}` |

> 补给是**整场被动**，没有 duration/cooldown；消耗品是**主动触发**，有 duration/cooldown。

### 7.5 模式专属物品 ✅

事件/模式文件与条数：

| 文件 | 消耗品 | 补给 |
|---|---|---|
| `common.xml` | 13 | 34 |
| `prototypes.xml` | 1 | 1 |
| `madgames.xml` | 7 | 5 |
| `hellgames.xml` | 11 | 14 |
| `lunar.xml` | 1 | — |
| `bossmode.xml` | 27 | 21 |
| `gravitizing.xml` | 4 | 3 |
| `vampiric.xml` | — | 5 |
| `zombie.xml` | — | 2 |
| **合计** | **64** | **85** |

复刻标准战斗以 `common.xml` 为准；`prototypes.xml`（测试/原型）与 `madgames/hellgames/...`
必须按模式/可用性隔离，不能默认全量展示（脚本族见 §11.4）。

---

## 8. 装备/消耗品对坦克参数的影响

### 8.1 参数全集 ✅

`hangarParamsStructureConfig.yaml` 声明了参数面板的全部键。核心分组：

| 分组 | 参数键 |
|---|---|
| `base` / `rawbase` | `maxHealth`、`baseShellAvgDamage`、`gunShotsPerMinute`、`baseShellAvgPiercingPower`、`hullAverageThickness`、`avgSpeed`、`chassiRotationSpeed` |
| `protectability` | `maxHealth`、`turretArmor{Front,Side,Rear}`、`primaryArmor{Front,Side,Rear}`、`engineFireStartingChance`、`chassisRepairTime`、`circularVisionRadius`、`stillInvisibility`/`movingInvisibility`/`shotInvisibility` |
| `gun` | `gunCommonDamagePerMin`、`gunShellReloadTime`、`gunClipReloadTime`、`gunClipShellReloadTime`、`timeBetweenPumpShots`、`gunClipBurstReloadTime`、`reloadingShellTime0..5`、`gunClipShellCount`、`gunClipBurstCount`、`gunBurstShellCount`、`base/prem/altShellAvg{PiercingPower,Damage,Speed}`、`gunAimingTime`、`gunShotDispersionRadius`、`gunPitchUpLimit`/`gunPitchDownLimit`、`turretYawLeftLimit`/`turretYawRightLimit` |
| `mobility` | `maxFrontSpeed`/`maxBackSpeed`、`avgSpeed`、`thrustToWeightRatio`、`weight`、`turretRotationSpeed`、`chassiRotationSpeed`、`terrainPermeability{Firm,Medium,Soft}`、`enginePower` |
| 逐组件 | `vehicleChassis` / `vehicleEngine` / `vehicleTurret` / `vehicleGun` / `crew` 各自的子集 |

格式（`hangarParamsConfig.yaml`）：每个参数声明 `units`、`digits`、`biggerIsBetter`。
例：`chassiRotationSpeed` 单位 `DEGREE_PER_SECOND`、2 位小数；`gunAimingTime`
2 位小数且 `biggerIsBetter=false`；`engineFireStartingChance` 单位 `PERCENT` 且越小越好。

> `chassiMaxLoad` 只出现在 `hangarParamsConfig`（单位 `TON`），**不在**
> `hangarParamsStructureConfig` 的面板结构里，11.21 的全部 UI 也零引用；它是数据
> 字段而不是当前 UI 的展示项。见 §2.5 的重量/maxLoad 口径。

### 8.2 基础值来源（模块 → 参数）◐

| 参数 | 来源 |
|---|---|
| `maxHealth` | `hull.maxHealth + max(炮塔.maxHealth)`（✅ 738/738） |
| `turretArmor*` / `primaryArmor*` | 炮塔/车体 `primaryArmor` 指向的 `armor_N` 板值 |
| `circularVisionRadius` | 炮塔 `circularVisionRadius` |
| `turretRotationSpeed` | 炮塔 `rotationSpeed` |
| `gunTraverseSpeed` | 火炮 `rotationSpeed`（无炮塔车的炮塔转向替代值；BK 同式） |
| `gunAimingTime` | 火炮 `aimingTime` |
| `gunShotDispersionRadius` | 火炮 `shotDispersionRadius` |
| `gunShellReloadTime` | 火炮 `reloadTime`（同炮不同炮塔可不同） |
| `gunCommonDamagePerMin` / `gunShotsPerMinute` | `damage*60/reload`（✅ 1308/1308） |
| 弹种伤害/穿深/弹速 | 火炮 `<shots>` + `shells.xml`（`damage`/`penetration`/`velocity`/`normalization`/`ricochet`） |
| `gunPitchUpLimit` / `gunPitchDownLimit` | 火炮 `pitchLimits`（min/max） |
| `turretYawLeftLimit` / `turretYawRightLimit` | 炮塔 `yawLimits`（缺省 `-180 180` = 无限制） |
| `maxFrontSpeed` / `maxBackSpeed` | 车辆 `speedLimits` |
| `chassiRotationSpeed` | 底盘 `rotationSpeed` |
| `terrainPermeability*` | 底盘 `terrainResistance`（3 值） |
| `enginePower` | 发动机 `power` |
| `engineFireStartingChance` | 发动机 `fireStartingChance` |
| `weight` | 车体 + 所选模块 `weight` 之和（◐；`tanks.pb`/`tank_data` 的字段只编码 `hull.weight`，见 §2.5） |
| `thrustToWeightRatio` | `enginePower / weight`（派生） |
| `hasTurret` | 是否有炮塔：车辆 XML `<hull><fakeTurrets>` 元素**存在** ⇔ `false`（738/738 与 BI 11.21 一致；114 辆 false，含 Jagdtiger/SU-100/T95/M3 Grant/Mark I Male） |
| `gunClip*` / `gunBurst*` / `reloadingShellTime*` | 火炮 `<clip>` / `<burst>` / `<pumpGunMode>`（**只认车辆内联**，共享定义里的 clip 不算） |
| `chassisRepairTime` | `hysteresisHealth / healthRegenPerSec`（客户端 hangarParams 同源；再乘工具箱/防护套件/Repair 技能加速，见上游计划 §5.2） |
| `still/moving/shotInvisibility` | 车辆 `<invisibility>`（`still/moving/firePenalty`）× 迷彩——**合成规则**⛔ |

### 8.3 修正量来源（装备/消耗品/补给）✅

装备 attribute（`miscAttrs/*`）与目标参数的对应：

| attribute | 目标参数 | 作用方式（数据原文） |
|---|---|---|
| `gunReloadTimeFactor` | `gunShellReloadTime` / `gunCommonReloadTime` | 百分比（-5 = 装填 ×0.95） |
| `gunAimingTimeFactor` | `gunAimingTime` | 百分比（-12） |
| `shotDispersionAngleFactor` | `gunShotDispersionRadius` | 百分比（-11） |
| `additiveShotDispersionFactor` | 移动/转炮扩圈（面板无独立键） | 百分比（-17） |
| `projectileSpeedFactor` | `base/prem/altShellAvgSpeed` | 百分比（+35） |
| `piercingPenaltyFactor500m` | 远距穿深衰减（`penetration_far`） | 百分比（-60） |
| `armorHomogenizationFactor` | 车体/炮塔装甲 | 百分比（+4） |
| `maxHpBonusPercent` | `maxHealth` | 百分比（+5） |
| `repairSpeedFactor` | `chassisRepairTime` / 模块维修 | 百分比（+20） |
| `circularVisionRadiusFactor` | `circularVisionRadius` | 按车辆类别百分比（LT 11/MT 8/HT 6/TD·SPG 4） |
| `detectabilityFactor` | 隐蔽（移动为基准；静止 3s 后 ×2） | 按类别绝对值（LT .06/MT .04/HT .02/TD·SPG .07） |
| `rotationSpeedFactor` | `chassiRotationSpeed` | 百分比（+12） |
| `enginePowerPercentageBonus` | `enginePower` | 百分比（+6） |
| `equipmentReloadBoost` | 消耗品/技能冷却 | 加法值（+0.17 → 17%） |
| `equipmentDurationFactor` | 消耗品/技能持续时间 | 百分比（+33） |
| `crewLevelIncrease` | 乘员等级（影响多个参数） | 加法百分点（+8 / +10 / +3） |
| `hpStockPercent`（补给） | `maxHealth` | 百分比（+3/+6） |
| `enginePowerIncrease`（补给/消耗品） | `enginePower` | 百分比或乘数（按条目，见 §7.3） |
| `turretRotationSpeedIncrease`（补给） | `turretRotationSpeed` | 百分比（+3/+10） |
| `fwdSpeedLimitBias` / `bkwdSpeedLimitBias` | `maxFrontSpeed` / `maxBackSpeed` | 加法 km/h（+2/+4） |
| `damageReductionPercent`（消耗品） | 受伤 | 百分比（10） |
| `factor`（`antiHighExplosive`） | HE 伤害 | 乘数（0.8） |

> 百分比语义来自脚本字段：`StaticFactorDevice` 用 `<bonusValues><percent>`，
> `StaticAdditiveDevice` 用 `<value>`；两条路径的合成方式不同（见 §8.4）。
> `display_params` 的单位/口径可能与之不同，计算时一律读 `script`。

### 8.4 叠加与派生：哪些是事实、哪些是 OPEN ⛔

**已由数据/回放验证的**：

- HP +5%（`highQualityAssembly`）：回放 `hp_initial / hp_base` 精确 1.05（数值级验证）；
- **HP 百分比加成按加法合成**（回放实测）：FV215b 基础 2400、装备含 111（+5%）且实测
  +11% → 2664 = 2400×(1+0.05+0.06)；若按乘法会是 2671。Maus 基础 2900、+9% →
  3161 = 2900×1.09（乘法 1.06×1.03=1.0918 → 3166）。整数 HP 四舍五入：
  2750×1.05=2887.5 → 实测 2888。**仅证明 HP 这一类；不代表其他参数也加法**。
  （6%/9% 的第二来源与 HP 补给 `largeHPStock` +6% / `smallHPStock` +3% 吻合；回放
  item 描述符的 `wireCode` 身份已解码（`0x44` 沙袋 / `0x45` 强化沙袋，见
  [docs/回放与射击逆向总集.md](回放与射击逆向总集.md) §2.6），描述符内部计时/动态
  payload 仍原样保留。）
- 校准弹 +6%/+7%（`calibratedShells`）：脚本值直接给出；回放可识别 103
  （**装配识别**，不验证数值）；
- 强化装甲 +4%（`multiLayerArmor`）：脚本值直接给出；回放可识别 110
  （**装配识别**，不验证数值）。

**数据里没有、必须标 OPEN 的**：

1. **同类修正的叠加次序（HP 已解；其余有 BlitzKit 对照）**：HP 百分比加成经回放验证为
   **加法**（5%+6%=11%、6%+3%=9%，见上文）；其余属性没有等价的多源回放样本，但
   BlitzKit 的 `coefficient()` 给出了显式对照：**同一调用内加法、不同来源之间相乘**
   （§8.5）。客户端是否完全一致仍需抽验，但不再是“完全未知”。
2. **乘员等级 → 参数的传播曲线**：`crewLevelIncrease` 只给百分点；BlitzKit 的显式
   公式是 `degressiveStat(crew) = 0.875 / (0.00375 × 100 × crew + 0.5) − 1`
   （100% 乘员 → 0；+8% → ≈ −3.3%），并叠加乘员位置系数（有该乘员 ×1.1，否则 ×1.05）
   与通风 +0.08。**不要用 WoT PC 的 `0.57 + 0.43 × crew/100`**——那不是 BlitzKit 的
   对照公式，也没有在 Blitz 数据中确认。
3. **取整与显示**：每个参数的 `digits` 是显示位数，不是计算精度；1 位/2 位小数的
   取整方向（四舍五入/截断）未在数据中声明。
4. **模块树/科技树的布局算法**：`nodeModel.x/y`、`modulesModel.horizontalSize/
   verticalSize`、`linesMask` 均由客户端模型计算；数据文件只有拓扑与显示坐标
   （科技树）或什么都没有（模块树）。
5. **描述文本里的“隐藏效果”**：`berserk` 的 +10% 模块伤害概率、`shieldKit` 的
   护盾系数都只在代码/描述里，`<script>` 不完整；不能按 script 复算全部效果。

**复刻口径建议（fail-closed）**：

- **只展示可证的量**：装备/补给的 `script` 值 + 描述模板，按「修正项清单」呈现
  （例如“装填时间 -5%”），不要合成一个未经证实的最终值；
- 若要显示最终值，先对**单一修正**做（HP +5%、穿深 +6/7%、装甲 +4%），
  多项修正时给出「未验证的预览」标记或直接留空；
- 回放路径用**实际 HP/装备 id 集合**做交叉校验；对不上就不显示推断值。

### 8.5 BlitzKit 对照（第三方实现，非客户端权威）

> 对照源：`github.com/blitzkit/blitzkit` 的
> `packages/website/src/core/blitzkit/tankCharacteristics.ts` 与
> `packages/core/src/blitzkit/{coefficient,degressiveStat,resolvePenetrationCoefficient}.ts`
> （2026-10-26 读取 `main`；该分支已引用装备 id 124，而本基线 11.21 只到 123，
> 属邻近版本对照，不是逐字节同版）。BlitzKit 是与本仓并行的独立实现，**不是客户端
> 权威**；但它把本文标 OPEN 的几项写成了显式公式，可用作实现基线，再用回放/客户端数据
> 校验。

| 项 | BlitzKit 实现 | 与本文/回放对照 |
|---|---|---|
| 整车重量 | `tank + engine + track + turret + gun`（L341-342）；`weightTons = kg/1000`，用于 `powerToWeightRatio`（L480-486） | 修正本文初版；pb field31 只是 `tank.weight`；BK 不含电台/油箱 |
| 装备槽 | `preset.slots` 9 槽；`equipmentMatrix[Math.floor(i/3)][i%3]`，choice `0`=空 / `-1`=left / `1`=right（L87-101） | 与客户端 3 组 × 3 级 × 2 候选骨架一致 |
| 系数合成 | `coefficient(...)` = `1 + Σ(enabled ? value : 0)`（`coefficient.ts`） | **同一调用内加法**；不同调用之间相乘 |
| HP | `coefficient([sandbag .03],[enhanced sandbag .06],[assembly .05])`（L197-202） | 与回放 5%+6%=11%、6%+3%=9% 一致 |
| 乘员 | `provisionCrewBonus = Σ provision.crew/100 + ventilation .08`；`commander = 1+bonus`；loader/gunner/driver ×1.1（有该乘员）或 ×1.05；`degressiveStat = 0.875/(0.00375*100*crew+0.5)-1`（L142-160 + `degressiveStat.ts`） | 本文 OPEN 项的显式对照；100% 乘员 → 0，+8% → ≈ -3.3% |
| 装填 | `0.95(rammer) × (1+degressive(loader)) / 1.17(adrenaline)`（L187-190） | 不同来源相乘；+17% **速度** = 时间 /1.17，不是 ×0.83 |
| 穿深 | calibrated：AP/APCR ×1.06，HEAT/HE ×1.07（`resolvePenetrationCoefficient.ts`） | 与 `calibratedShells` 脚本一致 |
| 炮速 | `1 + 0.35(supercharger) + 0.30(gunpowder)`（L203-206） | 同调用加法 |
| maxLoad | proto 无字段、stats 不用 | 只能作数据旁证，不是已确认机制 |
| 隐藏效果 | `moduleDamageCoefficient` 只含 tungsten（L186）；`berserk` 的 +10% 模块伤害未建模 | 与本文“描述里有 script 外效果”的判断一致 |

**结论**：重量、装备槽骨架、HP/炮速的同调用加法、乘员曲线、穿深系数都可以按 BlitzKit
的公式实现；但要在代码注释里标“BlitzKit 对照，非客户端权威”，并用回放/客户端 UI 抽验。
**不要把 maxLoad 当成 BlitzKit 已建模的载重机制**——它只是客户端数据字段。

---

## 9. WotbTools 复刻设计

> **完整实施方案见 [wotbtools-research-loadout-plan.md](wotbtools-research-loadout-plan.md)**
> （2026-10-27 裁定范围：单一整合配置界面，不做研发/科技树/分入口；数据源铁律：
> 客户端解析权威、BlitzKit 备用+公式参照；导出契约、计算层规格、联动契约、分期与
> 门禁）。本节保留数据侧的管线与门禁要点；§9.3/§9.5 中研发/科技树相关分期已从
> 实施范围移除，数据仍由导出器可选保留。

### 9.1 建议的数据管线（本仓出数据，WotbTools 出 UI）

新增一个只读导出器（建议 `tools/export_research_loadout.py`，与
`tools/extract_vehicles.py` 同风格：读磁盘、fail-closed、确定性输出）：

```text
data/research/
  index.json                 # 版本、国家、计数、生成时间
  tech_tree.json             # 9 国：nodes[{key,list_id,tank_id,tier,x,y,price,priceRaw}]
                             #        edges[{from,to,cost,costRaw,sourceModule}]
                             #        premium_rows
  modules/<tank_id>.json     # 该车模块 DAG + 每模块字段 + 装备槽/消耗品槽
  loadout/devices.json       # 24 件 + 9 预设 + 槽位骨架
  loadout/consumables.json   # 64 条（含 file/mode 标记）
  loadout/provisions.json    # 85 条
  params/structure.json      # 参数分组/单位/位数/biggerIsBetter
```

**导出规则（必须写进实现）**：

1. `cost`/`price` 保留原始字符串；数字才转 int，`prx_*` 原样保留（不得当 0）；
2. `<unlocks>` 目标按 `nation:` 命名空间化；跨国籍目标保留全名；
3. 共享组件按 `{nation}/components/*.xml` 的 `<shared>` 解析，`<ids>` 只用于 id；
4. 模块价格 0 与「未声明」区分（0 是有效值：免费）；
5. `emptyPreset` / `stereoscope` 这类未引用项单独标 `unused`，不静默丢；
6. 事件模式物品带 `mode` 字段（`common`/`madgames`/…），默认 UI 只取 `common`；
7. 任何缺失/解析失败 → 该条目输出 `null` + `error`，不猜默认值。

### 9.2 UI 映射（WotbTools）

| 客户端 | WotbTools 建议 | 关键契约 |
|---|---|---|
| 科技树 | 国家页签 + 横向 tier 列 + 分支行 | `nodes.x/y`、`edges`、`premium_rows`、状态机 |
| 单车研发页 | 左「上一辆/当前车」+ 右模块网格 | 64×64 节点、104×97 pitch、`module.status`、箭头 |
| 研发确认 | 弹窗 | §5.4 的 XP/自由经验/金币算法 |
| 装备页 | 3 组 × 3 级 × 2 候选 | §6.1 骨架 + `unlockStatus`（价格服务端，缺失则只读） |
| 消耗品/补给 | 槽位网格 + 过滤器 | `consumableSlots`/`provisionSlots` + `vehicleFilter` |
| 参数面板 | 分组表格 + `changed` 差分 | §8.1 参数键/单位/`biggerIsBetter` |
| 回放装配 | 战斗详情/射击复现里显示本场装备 | `equipment: [u8;9]`（集合，不含槽位） |

**布局常量（可直接用）**：

- 科技树 cell 216×96；等级列 1..10；premium 带高 `premium_rows × 96`。
- 模块节点 64×64；列 pitch 104、行 pitch 97；网格列数/行数取模型值
  （数据没有 → 复刻时用「列 = 依赖深度、行 = 模块族」的确定性布局，并在 UI 上标注
  “布局为复刻推导”，不要声称与客户端逐像素一致）。

### 9.3 交互状态机（建议）

```text
VehicleNode: LOCKED -> CAN_BE_RESEARCHED -> CAN_BE_PURCHASED -> OWNED
                     \-> NOT_ENOUGH_MONEY (buy 失败态)
             （金币/收藏车没有 RESEARCH 段：CAN_BE_PURCHASED -> OWNED）
ModuleNode : LOCKED -> CAN_BE_RESEARCHED -> INSTALLED
PageAction : NONE | RESEARCH | BUY | INSTALL | SELL | REPAIR | SHOW_TANK
LockReason : INSTALLED | NO_MODULE | NEED_BUY_TANK | UNLOCK_BY_CLAN |
             NO_CURRENT_TANK | NO_PREV_TANK
DeviceSlot : LOCKED | FIRST_INSTALLED | SECOND_INSTALLED
DeviceUnlockStatus: UNLOCKED | READY_TO_UNLOCK | REQUIRES_PREVIOUS_TIER
```

### 9.4 测试与门禁（建议）

1. **拓扑不变量**：非根树节点至少一条入边；每条 `vehicle` 边的目标可解析为车辆；
   多父节点集合与本文 §3.2 一致（16 辆）。
2. **成本解析**：int 与 `prx_*` 混合不丢边；`cost=0` 保留。
3. **预设覆盖**：758 辆的 `optDevicePreset` ∈ 9 预设；除 `emptyPreset` 外每预设
   9 槽 × 2 候选；未引用装备（`stereoscope`）有标记。
4. **槽位覆盖**：`consumableSlots` ∈ 1..3、`provisionSlots` ∈ 0..3；分布与 §7.1 一致。
5. **公式 golden**：`hp = hull + max(turret)`（738）、`dpm = dmg*60/reload`（1308）。
   **不要把 `weight = hull.weight` 当 golden**——它是 pb 编码事实，不是战斗重量。
6. **重量结构校验**：按 BlitzKit 公式（`tank+engine+track+turret+gun`）求和，
   747/758 落在 `maxLoad` 内；11 辆异常必须显式列入白名单/报告（清单见 §2.5），
   测试不得用“全部通过”掩盖。**`maxLoad` 不是已确认机制**，只作旁证。
7. **回放装配**：id ⊆ 100..123；`111` 出现时 HP 应按加法叠加（基础 ×(1+5%+其它 HP
   加成)），并用整数 HP 校验取整。
8. **过滤求值**：对每条物品的 include/exclude 用真实车辆集合求值，结果快照 golden。

### 9.5 分期

| 期 | 内容 | 依赖 |
|---|---|---|
| P0 | 科技树数据 + 车辆状态机 + 研发 XP 算法 | §3/§4/§5.4 |
| P1 | 单车模块 DAG + 研发页布局 + 研发确认 | §2/§3/§5.2 |
| P2 | 装备 9 槽 + 24 件 + 预设 + 效果清单 | §6 |
| P3 | 消耗品/补给槽位 + 过滤器 + 默认配置 | §7 |
| P4 | 参数面板 + 已证实的单修正；多项修正标“未验证” | §8 |
| P5 | 回放装配叠加（equipment id 集合） | §6.4 |

---

## 10. 未决问题（必须在复刻前定口径）

| # | 问题 | 影响 | 建议 |
|---|---|---|---|
| 1 | 模块树 `x/y` 与 `horizontalSize/verticalSize` 的布局算法 | 研发页节点位置/箭头 | 先用确定性分层布局，UI 标注“复刻布局”；需要逐像素一致再 RE |
| 2 | 同类百分比修正的叠加次序（HP 已证加法；其余有 BlitzKit 对照） | 参数最终值 | HP 用加法；其余按 BlitzKit `coefficient()`（同调用加法、跨来源乘法）实现并标注“对照来源”，客户端 UI 抽验 |
| 3 | 乘员等级 → 各参数的曲线（BlitzKit 已给显式公式） | 通风/食物/技能的影响 | 按 `degressiveStat(crew)=0.875/(0.375*crew+0.5)-1` 实现，标注“BlitzKit 对照”；仍建议抽验 1–2 辆车 |
| 4 | `prem_tanks.yaml` 的客户端语义 | 金币车展示位置 | 不据此摆放；按 `list.xml` 独立分组 |
| 5 | 装备槽解锁价格 | 装备页解锁按钮 | 服务端模型给；离线只读展示槽位/状态 |
| 6 | 消耗品/补给 14B 描述符**内部字段**（计时器/动态状态） | 回放“本场带什么”的细节 | 物品身份已由 `wireCode` 闭合（3 消耗品 + 0..3 给养 + 模式能力；见 [回放与射击逆向总集](回放与射击逆向总集.md) §2.6）；内部 12B payload 仍保留 raw，不赋语义 |
| 7 | `stereoscope`（119）为何无预设引用 | 装备全集/可获取性 | 标 `unused`；不硬塞进槽位 |
| 8 | 战斗重量的求和成员（客户端是否含电台/油箱）与 11 辆超限异常 | 机动/推重比/载重 | ~~按 BlitzKit 公式 `tank+engine+track+turret+gun`（不含电台/油箱）~~ **已解决（2026-10-27，见 wotbtools-research-loadout-plan §10 #1）：按整车计（hull+engine+track+turret+gun+radio+fuel），与 BlitzInfo 11.21 `wt` 逐辆一致**；`maxLoad` 只作旁证 |
| 9 | 科技树 `position` 的 0/1 基与 `columnsCount` 定义 | 像素对齐 | 复刻用自己的 0 基网格，保持相对拓扑 |
| 10 | 模块 `price` 非零的 9 辆车是否走特殊购买流程 | 研发/购买按钮 | 保留 `price` 字段，UI 区分 XP/credits |
| 11 | `tanks.pb` field10 = 车体 HP，而 `tank_cache`/`tank_data` 的总 HP = 车体+顶炮塔 | 直接读 pb 的消费方会少算炮塔血量 | 统一用 `tank_cache`/`tank_data` 的 `hp`；不要直接用 pb field10 当总血量 |

---

## 11. 附录：复现命令与文件清单

### 11.1 解码任一 DVPL

```python
import sys; sys.path.insert(0, 'tools/wotbtools')
from pathlib import Path
from wotb_sc2 import decode_dvpl
raw = decode_dvpl(Path('D:/SteamLibrary/steamapps/common/World of Tanks Blitz/Data/'
                       'XML/item_defs/vehicles/common/optional_devices.xml.dvpl').read_bytes())
print(raw.decode('utf-8'))
```

### 11.2 本文涉及的全量统计（11.21.0）

| 项 | 值 |
|---|---|
| list 车辆 | 758（9 国） |
| 有 XML 的车辆 | 758（0 解析失败） |
| 科技树节点 | 304 |
| 研发边 | 1410（chassis 276 / turret 208 / gun 457 / engine 351 / vehicle 310） |
| 无研发边车辆 | 462 |
| 多父节点车辆 | 16 |
| 代币成本边 | 192 |
| 模块条目 | 6204（price 0：6178；非零：26） |
| 装备 | 24（被预设引用 23；`stereoscope` 未引用） |
| 装备预设 | 9（实际使用 8） |
| 消耗品 | 64（standard 13） |
| 补给 | 85（standard 34） |
| 槽位组合 | 3/3:579、3/2:56、3/1:30、2/0:35、1/0:58 |

### 11.3 相关既有文档

- [docs/game-data-sources.md](game-data-sources.md) §2.6 客户端关键路径；
- [docs/data-inventory.md](data-inventory.md) 数据面总账（车辆/模型/模块 id 口径）；
- [docs/回放与射击逆向总集.md](回放与射击逆向总集.md) §2.6 Type5 loadout 字节布局；
- [docs/wotbtools-cross-reference.md](wotbtools-cross-reference.md) 跨仓裁决。

### 11.4 模式专属物品的 script 族（11.21.0）

> 这些 script 多数是**模式技能**（`Ability`/`Heal`/`Mark`），不是常规参数修正；
> 复刻标准战斗时不需要展开，但数据管线应完整保留并按 `mode` 标记。

| 文件 | 消耗品 script 族 | 补给 script 族 |
|---|---|---|
| `bossmode` | HealingAura×5、ShieldGiverAbility×5、KamikazeAbility×5、PoisonedShell×5、GravityShot×5、GravityBurst×1、RailgunCharger×1 | AttributesModifier×10、PermanentDamageShield×5、LifestealHeal×5、BigBoss×1 |
| `hellgames` | LightningAbility×2、Moonwalk×2、SplashAbility×1、Curse×1、ReturnInTimeAbility×1、PretendDead×1、HuntersMark×1、TornadoAbility×1、ActiveAttributesModifier×1 | AttributesModifier×4、EngineAndSpeedBoost×2、RammingDamage×2、Fortune×2、ComeBackKit×1、EnemyReloadTime×1、Duplet×1、Stealth×1 |
| `madgames` | Dash×1、Sacrifice×1、ShieldAbility×1、AfterburnerAbility×1、Moonwalk×1、HuntersMark×1、Reflection×1 | LifestealHeal×1、RammingDamage×1、AllyBuff×1、Stealth×1、Revenge×1 |
| `gravitizing` | Moonwalk×2、SplashAbility×1、Curse×1 | EngineAndSpeedBoost×2、Duplet×1 |
| `vampiric` | — | VampiricCurse×2、VampiricBoost×1、LifestealHeal×1、VampiricDeath×1 |
| `zombie` | — | ZombieMark×1、HumanMark×1 |
| `lunar` | GravityJump×1 | — |
| `prototypes` | MakeContusion×1 | CamouflagePaint×1 |
