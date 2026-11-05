# WotbTools 复刻实施方案：车辆配置器（模块 / 配件 / 消耗品-补给，UI 对齐客户端）

> 配套数据文档：[client-research-tech-tree-loadout.md](client-research-tech-tree-loadout.md)
> （数据 schema、全量统计、公式校验证据——本文不重复，只引用）。
>
> **数据源铁律（2026-10-27 用户裁定）**：一切数据以**本机客户端解析**（本仓提取链）为
> 唯一权威；BlitzKit 只作两个角色：①客户端提取不可用时的**备用数据源**（须过差异
> 白名单校验，§3.4）；②**计算公式的参照实现**（客户端把公式编译在代码里，数据文件
> 没有；BK 是目前唯一可读的显式公式来源）。冲突时以客户端数据为准；**隐蔽加成（迷彩网/涂装）按客户端数据**
> （2026-10-27 用户裁定，见 §5.3）。
>
> **范围裁定（2026-10-27 用户裁定）**：**不做**研发/购买/解锁机制、不做科技树页、
> 不做分入口——只做**一个整合配置界面**：模块（炮塔/炮/发动机/履带）选择 + 配件
> （9 槽装备）选择 + 消耗品/补给选择，布局类似 BlitzKit Tankopedia，与**装甲查看器
> 模型**和**百科数值**联动。

---

## 0. 结论摘要

1. **一个界面**：在现有 `AgentTankopedia` 详情态扩展为「模型 + 配置面板 + 参数区」的
   整合页（BlitzKit 同构布局）：左侧模型（装甲查看器），右侧配置面板
   （模块选择 → 装备 3×3 → 补给 → 消耗品 → 涂装），下方/侧方参数分区实时联动。
2. **全部已解锁语义**：所有模块/装备槽/物品默认可用，直接选择；无研发状态机、无
   支付弹窗、无虚拟资源。装备槽允许留空（中性初始配置需要；客户端机制不可卸下，
   见 §7 偏差表）。
3. **联动契约**：炮塔/炮选择 → `configIndex` → 装甲查看器模型与板值（现有 `?config=`
   机制）；全部选择 → 计算引擎 → 百科参数区数值 + delta（基准 = 初始配置 + 空装备 +
   空槽）。
4. **数据**：`tools/export_research_loadout.py` 导出 `data/research/`（逐车模块清单、
   24 件装备 + 9 预设、消耗品/补给 + 过滤器 + 按 tier 默认表、参数分组/格式、en/zh
   文案、模块图标）；研发边与科技树坐标**不在本期消费面**（导出器保留可选输出，
   供日后科技树展示）。
5. **计算层**：BlitzKit `tankCharacteristics` 全公式移植（逐条标注 BK 对照），
   纯函数 + golden；迷彩网/涂装按客户端数据；berserk 描述级效果只展示不复算。

---

## 1. 范围与对齐口径

### 1.1 范围

| 做 | 不做（本期，明确裁定） |
|---|---|
| 整合配置界面（模块 + 配件 + 消耗品/补给 + 涂装开关） | 研发/购买/解锁机制（无状态机、无支付、无资源池） |
| 参数分区实时计算 + 上一次值的 delta 呈现（BK useDelta） | 科技树页（数据导出保留，展示后续可选） |
| 与装甲查看器模型联动（configIndex） | 分入口（tab=research/equipment/loadout 之类的拆分） |
| 与百科数值联动（详情页统计改为引擎计算） | 战斗内 HUD、账号/服务器经济、新手引导 |
| 回放装配展示（equipment id → 图标/名称） | 加值配件 124+（11.21 数据只有 100..123） |

### 1.2 对齐三档

| 档 | 内容 | 承诺 |
|---|---|---|
| **A 交互一致** | 选择语义（即点即换、槽位二选一、物品过滤器）、信息结构（3 组装备/参数分组/delta 判定）、联动行为（模型随炮塔/炮切换） | **必须** |
| **B 布局一致** | BlitzKit Tankopedia 的区块次序（配置 → 装备 → 补给 → 消耗品 → 涂装；参数按 Survivability/Firepower/Maneuverability 分区——与客户端 protectability/gun/mobility 同构） | 尽量 |
| **C 视觉一致** | 配色/字体/图标风格 | 不作承诺：WotbTools 设计系统承接（AppButton/现有图标链）。视觉验收归用户（AGENTS 规则） |

---

## 2. 现状盘点

### 2.1 本仓（数据权威）

- 客户端提取链现役（`tools/extract_vehicles.py` 等），`data/` 全量 738 辆 @ 11.21.0；
- Type5 loadout 解码已有（`collect_vehicle_loadout`：3 消耗品 + 0..3 给养 + 模式额外能力 +
  9 槽装备 ID；wireCode 逻辑表见 [回放与射击逆向总集](回放与射击逆向总集.md) §2.6）；
- 数据事实文档已落地（`client-research-tech-tree-loadout.md`）。

### 2.2 WotbTools（实现落点）

- `AgentTankopedia.vue`：列表 + 详情两态，URL 是状态唯一 owner（`tankopediaQuery.js`，
  已有 `?tank=` / `?config=`），数据走 asset origin；
- **装甲查看器**（`AgentArmorView.vue`）：按 `configIndex` 切换炮塔/炮模型与板值，
  与百科详情已有 `?config=` 互跳；
- `common/tankopedia-tier{1..10}.json` 快照管线（`update_tankopedia.py` +
  `validate_tankopedia_equipment.py` fail-closed）+ item-catalog 图标链；
- 前端基建：vue-i18n（en/zh/ru）、`replay-local/compute/*` 纯函数 + vitest + golden
  模式、AppButton/PageHeader/EmptyState、Motion 动效策略。

### 2.3 BlitzKit（备用源 + 公式与布局参照）

- **公式**：`tankCharacteristics.ts` 全套、`coefficient.ts`（同调用加法）、
  `degressiveStat/progressiveStat.ts`、`resolveDpm.ts`、`resolvePenetrationCoefficient.ts`、
  `applyPitchYawLimits.ts`、`checkConsumableProvisionInclusivity.ts`、
  `availableProvisions/createDefaultProvisions.ts`；
- **状态模型**：`DuelMember { tank, engine, turret, gun, shell, track,
  equipmentMatrix[3][3](-1|0|1), consumables[], provisions[], camouflage, crewSkills }`；
- **布局参照**：Tankopedia = 模型区 + Configuration（炮塔/炮/发动机/履带选择）+
  Equipment（3×3 左右按钮矩阵，即点即换，可清空）+ Provisions/Consumables（chip
  多选，池按过滤器）+ Characteristics 分区（Survivability/Firepower/Maneuverability，
  每行 = 名称 + 值 + delta）。

---

## 3. 数据管线（权威链）

### 3.1 导出器（本仓新增）

`tools/export_research_loadout.py`——与 `extract_vehicles.py` 同风格：只读本机客户端
磁盘文件、fail-closed、确定性输出、XML last-wins、`prx_*` 原样保留。输出：

```text
data/research/
  meta.json                    # game_version(=version.txt.dvpl)、generated_at、各文件 sha256、计数
  modules/{nation}.json        # {tankId: {consumableSlots, provisionSlots, optDevicePreset,
                               #   speedLimits, invisibility, crew,
                               #   hull:{hp, weight},
                               #   chassis:[…], turrets:[{…, guns:[…]}], engines, radios, fuelTanks}}
                               #   模块字段 = 数据文档 §2.4 全字段（reload/aim/dispersion/power/
                               #   resistance/traverse/weight/maxLoad/…）；共享组件按 <shared> 解析；
                               #   unlocks 字段保留但本期消费面不用
  devices.json                 # {devices:[24 {id, stem, icon, nameKey, descrKey, script, displayParams}],
                               #   presets:{9 预设名 → 9 槽 × [left, right]},
                               #   unused:["stereoscope"]}
  consumables.json             # 64 条 {id, stem, mode, nameKey, icon, category, tags,
                               #   script, displayParams, filter}
  provisions.json              # 85 条（同构）
  consumables_default.json     # 客户端按 tier 默认表 + 单车覆盖（初始预填用）
  params/structure.json        # hangarParamsStructureConfig 分组树（param/subsection/table/…）
  params/format.json           # 每参数 {units, digits, biggerIsBetter}
  strings/{lang}/research.json # en + zh-Hans：设备/消耗品/补给名与描述、参数名
                               #   tank_descr_param_*、分组标题
  icons/                       # 模块图标 5 张（Gfx/UI/ModulesTechTree/vehicle*.packed.webp）
                               #   + icon_manifest.json（客户端 icon 名 → WotbTools 图标键映射）
  # 以下本期不消费，导出器留可选开关（--with-research-graph），供日后科技树展示：
  tech_tree/{nation}.json      # position 坐标 + premium_rows
  research/{nation}.json       # <unlocks> 边 + 成本
```

规模：`modules/` 按 nation 9 件（单件 < 1 MB），其余均 < 200 KB。

### 3.2 分发（进 WotbTools）

**按数据性质拆分**（2026-10-27 用户裁定：所有配置器数据随资产包发布，新坦克不必发版）：

1. **全部非图标数据**（devices/consumables/provisions/defaults/skills/params/strings/
   camouflage/meta + 逐车 `modules/{nation}.json`）→ 由 `scripts/export_asset_pack.py`
   写入资产包 `research/**`，随 Alibaba asset origin 的新 immutable release 发布；
   WotbTools 经 assetProvider 加载（全局目录 `loadResearchCatalog`、逐车模块
   `loadTankModules`），不进前端 bundle；
2. **全局图标集**（items/modules/skills/shells）留在 WotbTools
   `common/research/icons/`，构建期打成 URL 表；图标不是配置器数值数据；
3. 后续新增坦克/物品/模块数值只需重跑研究导出 + 打包发布，WotbTools 无需发版。

### 3.3 版本锚与更新链

客户端升级 → 本仓 `update-data` + 重跑导出器 → diff 报告（模块字段/装备/过滤器
变化清单）→ 复制进 WotbTools → CI golden 更新。`meta.json` 的 `game_version` 是唯一
版本判据；前端启动校验，不一致显示「数据版本未知」横幅（不阻塞查看）。

### 3.4 备用源（BlitzKit API）的降级契约

仅当本机客户端提取不可用时，允许用 BlitzKit definitions（`tanks.pb` 的模块数值、
`equipment.pb` 的 presets、`consumables/provisions.pb` 的过滤器）生成替代快照，但必须：

1. `meta.json` 写 `source: "blitzkit-api"` + 拉取日期（禁止伪装成客户端来源）；
2. 过差异校验，已知差异白名单：
   - 过滤器只支持 tiers/ids/nations/CLIP，**不支持 extendedTags 全语义**；
   - 科技树坐标 BK 无（本期不消费，无影响）；
   - 显示名/文案来自 BK i18n，可能与客户端 Strings 有差异；
   白名单外差异 → fail-closed 拒绝发布。

### 3.5 图标策略（已落地：客户端原图）

- **映射权威 = 客户端 UI 样式表的 `bg-sprite`**，不按文件名猜：
  - 装备：`UI/Screens3/Lobby/Inventory/OptionalDevices/OptionalDevicesItemImage.style.yaml`
    （`.optional_device_item.<icon>`）；
  - 消耗品/补给：`UI/Styles/Lobby/Inventory/InventoryBigStyles.yaml` +
    `InventoryNormalStyles.yaml`（含事件面板与 Normal 兜底；`.equipment_item.<icon>`）。
- **切图**：`bg-sprite` 指向图集描述（`.txt`：图集名 + 帧矩形）或单图
  （`.packed.webp`，新版 supply 树）；导出器按帧裁成独立无损 WebP：
  - `icons/items/{devices,consumables,provisions}/<icon>.webp`（68 张：21 装备 +
    13 消耗品 + 34 补给；共享图标只导一份）；
  - `icons/modules/{chassis,engine,gun,hull,turret}.webp`（ModulesTechTree 的 5 张类型图，
    客户端模块卡片就用这一套）；
  - `icons/skills/<id>.webp`（avatar.xml 的 20 张技能图）与 `icons/shells/<type>.webp`
    （AmmoSlot.yaml 的 9 类弹种图），同样 96×96 归一。
  - **统一光学尺寸**：所有图标裁到 alpha 内容框后等比缩放、居中放进 96×96 透明画布
    （内容区 84×84）——客户端图集帧是逐图标紧裁（34~83px 不等）、新版 supply 单图还带
    大块透明边，直接 contain 会出现同格图标大小不一。
- **缺图**：只有 `stereoscope`（无任何预设引用、客户端也没有装备图标）记录在
  `icons/manifest.json.missing`；组件按名称文本回退。图标缺失不影响任何数值语义。
- 图标目录由导出器整体重建（清空后重写），`meta.json.files` 逐文件 sha256 覆盖全部 91 个产物。

---

## 4. 数据契约（前端消费面）

```ts
// 逐车模块数据（modules/{nation}.json）
interface VehicleModules {
  tankId: number
  consumableSlots: number          // 1..3
  provisionSlots: number           // 0..3
  optDevicePreset: string          // 9 预设之一 → devices.json
  speedLimits: { forward: number, backward: number }
  invisibility: { still: number, moving: number, firePenalty: number }
  hull: { hp: number, weight: number }
  hasTurret: boolean               // 客户端 <hull><fakeTurrets> 块存在 ⇔ false（738/738 与 BI 一致）
  chassis: ModuleData[]            // 含 rotationSpeed/terrainResistance/maxLoad/weight/…
  turrets: (TurretData & { guns: GunData[], primaryArmor?: { front: string, sides: string, rear: string } })[]
                                   // primaryArmor = 该炮塔的 front/side/rear 主装甲板 id（装甲表随炮塔变）
  engines: EngineData[]            // power/fireStartingChance/weight/…
  radios: RadioData[]              // distance/weight/…
  fuelTanks: FuelTankData[]        // weight/…
}
// ModuleData.stats 均为客户端原值（§2.4 字段），unlocks 保留不用
// GunData.stats 含火炮自己的 rotationSpeed（无炮塔车的火炮转向速度来源，BK gunTraverseSpeed）

// 装备（devices.json）
interface DeviceData { id: number, stem: string, icon: string, name: string,
                       description: string, script: ScriptData, displayParams: object }
interface DevicePreset { name: string, slots: { level: 0|1|2,
                        group: 'attack'|'defence'|'specialization',
                        left: number, right: number }[] }      // 9 槽

// 物品（consumables/provisions.json）
interface ItemData { id: number, stem: string, mode: 'common' | string, name: string,
                     description: string, icon: string, category: string, tags: string[],
                     script: ScriptData, filter?: FilterSpec,
                     isAbility: boolean }   // 客户端 <isAbility/>：战斗内技能，不是机库装配项
type FilterSpec = { include: FilterRule[], exclude: FilterRule[] }
// 一个 FilterRule = 客户端一个 <include>/<exclude> 块：同块内字段**全部满足**（AND）。
// 例：ration = { tiers: {min:6,max:10}, nations: ["ussr"] }（6-10 级且苏联车）。
// 当前客户端数据每件物品只有 1 个 include 块（+ 最多 1 个 exclude 块）；块间语义按 OR 实现，
// 但无数据触发（BK 把条件摊平后 include 全 AND，对现役数据等价）。
type FilterRule = { tiers?: { min: number, max: number }, ids?: string[],   // ids = nation:stem
                    nations?: string[], extendedTags?: string[] }
```

`tank/{id}.json` 继续承担配置详情（configs/armor/shells/GLB 节点数），配置器以
`tankId` 与 `configIndex` 与之联表（`turret_local`/`gun_local` 对号）。

---

## 5. 计算层规格（WotbTools `replay-local/compute/characteristics.ts`）

> 全部公式标注「BK 对照」；纯函数 + vitest golden。**冲突以客户端数据为准；
> 隐蔽加成（迷彩网/涂装）采用客户端数据（用户裁定）**。

### 5.1 状态模型

```ts
interface VehicleSetup {
  tankId: number
  configIndex: number              // tank/{id}.json configs 下标 → 炮塔+炮组合
  engineIndex: number              // 该配置 engines 下标
  trackIndex: number               // 该配置 tracks 下标
  devices: (0 | 1 | null)[]        // 9 槽：0/1 = 左/右候选；null = 空（中性初始配置用；
                                   //   默认全空；客户端机制不可卸下 → §7 偏差表）
  consumables: string[]            // item stem，≤ consumableSlots；默认空（中性基线）
  provisions: string[]             // item stem，≤ provisionSlots；默认空
  crewMastery: number              // 固定 1.0（无技能 UI；技能项恒 0）
  camouflage: boolean              // 涂装开关，默认 false
}
```

- 模块选择模型（BK `DuelMember` 同构）：炮塔 → 炮（限该炮塔列表）→ 发动机 → 履带，
  四个独立选择；`(turret, gun)` 对号到 `configIndex`（多炮车按 `turret_local/gun_local`
  匹配，无匹配组合 fail-closed）。
- 默认状态：装备槽全空；**消耗品/补给/涂装默认全空**（与 BlitzInfo 的中性基线一致，
  便于逐项对照；客户端默认表仍随数据导出，但不自动套用）。

### 5.2 公式总表（BK 对照，实现基线）

| 参数 | 公式（BK tankCharacteristics.ts） | 备注 |
|---|---|---|
| `weightKg` | `hull + engine + track + turret + gun + radio + fuelTank` | 整车口径（BK 参考实现漏电台/油箱）；738 辆电台/油箱各 1 件不可换装。与 BlitzInfo 11.21 `wt` 逐辆对拍：732 辆中 725 辆 ≤0.02t（余 7 辆为 BI 默认顶配选炮不同，非公式差） |
| 系数合成 | `coefficient(...) = 1 + Σ(enabled ? v : 0)`；**同调用内加法、跨调用相乘** | HP 加法已回放实证 |
| `health` | `(hull.hp + turret.health) × (1 + 0.03 sandbag + 0.06 enhancedSandbag + 0.05 assembly)` | 回放实证 5%+6%=11% |
| 乘员 | `provisionCrewBonus = Σ provision.crew/100 + (ventilation ? 0.08 : 0)`；`commander = 1+bonus`；loader/gunner/driver = commander × 1.1（有该乘员角色）或 1.0（**无 1.05 兜底**） | 食物/通风入口；BlitzInfo 口径（T-34 无炮手 → 炮塔/瞄准/散布无该角色加成） |
| `degressiveStat(c)` | `0.875/(0.375c + 0.5) − 1`（负值） | 装填/瞄准/散布/阻力 |
| `progressiveStat(c)` | `(0.375c + 0.5)/0.875 − 1`（正值） | 视野/炮塔转速 |
| `reload` | `base × (1 − 0.05 rammer) × (1 + degressive(loader)) ÷ (1 + 0.17 adrenaline)` | adrenaline 仅 regular 炮（客户端 filter `exclude extendedTags:["clip"]`） |
| `aimTime` | `base × (1 − 0.12 GLD − 0.3 reticle) × (1 + degressive(gunner))` | |
| `dispersion`（静止） | `base × (1 − 0.11 refined − 0.3 reticle) × (1 + degressive(gunner))` | |
| 乘员技能 | 等级 0–7（`?skills=<id>:<level>`，裸 id 兼容为 7）；每项 per-level 值只读 `skills.json.effects` | **客户端 avatar.xml 权威**；BK 的硬编码技能百分比不采用（2026-10-27 用户裁定：BK 除隐蔽外均可参考，但技能数值仍以客户端为准） |
| 条件技能（静态表） | Close Combat Master：炮塔转速 ×(1+L×rate)；Breakthrough：引擎功率 ×(1+L×rate)；Rage/Desperado：装填 ÷(1+L×rate)；Deadly Accuracy：视野 ×(1+L×rate)、瞄准 ÷(1+L×rate) | 与 BI 实测同口径（T-34：炮塔 49→52.43、视野 240→256.8、瞄准 4.89→4.55、引擎 470→502.9、装填 7.57→7.08） |
| 移动/转车/转炮/射击后/炮损扩圈**系数** | `chassis/gun dispersionFactors × (1 − 0.17 stabilizer) × (1 − level × perLevel)`；**不乘基础半径**（无量纲） | 与 BI “Gun flexibility: Moving +0.158 / Shooting +3.255”同口径；smooth_turret 的 perLevel 客户端为 0.02 |
| 车体转向（技能） | `hullCoeff` 内加法 `+ level × virtuoso.rotationSpeedFactorPerLevel` | 客户端 0.01/级 |
| 隐蔽（技能） | 裸隐蔽 `× (1 + level × camouflage.camouflageFactorPerLevel)`，再与涂装/网绝对值相加 | 客户端 0.03/级 |
| `viewRange` | `base × (1 + 类别加成[LT .11/MT .08/HT .06/其余 .04]) × (1 + progressive(commander))` | |
| `enginePower` | `base × (1 + 0.06 accel + 0.1 reducedBoost + 0.2 boost + 0.35 improvedBoost + 0.03 fuel + 0.1 improvedFuel + 0.03 gearOil + 0.06 improvedGearOil)` | |
| 炮塔转速 | `base × (1 + 0.03/0.1 fuel) × (1 + progressive(gunner))` | |
| 火炮转向速度 | `gun.rotationSpeed × (1 + 0.03/0.1 fuel) × (1 + progressive(gunner))` | BK `gunTraverseSpeed` 同式（不含 close_combat_master）；**无炮塔车（`hasTurret=false`）参数区用它替换炮塔转速行**；Jagdtiger 26.25×1.042857=27.375 |
| 车体转速 | `track.traverse × hullCoeff × (baseEnginePower/stockEngine.power) × (stockWeightKg/weightKg)`；`hullCoeff = (1 + 0.12 control + 0.05 improvedAfterburning) × (1 + progressive(driver))` | 与 BlitzInfo 11.21 `rot` 逐辆对拍：732 辆中 725 辆 ≤0.06（余 7 辆为 BI 默认顶配选炮不同）；发动机/减重升级按功率比与整车重量比生效 |
| 地形阻力 | `raw × (1 − 0.2/0.15/0.3 suspension) × (1 + degressive(driver))`；显示系数 = `100/raw` | |
| p2w（参数行） | `power / weightTons`（硬地；参数区只出一条 `thrustToWeightRatio`） | 三地形变体未展示（BI 同款只给一条）；参数区地形阻力只显示 `100/raw` 的通过性三行，原始阻力不进 UI |
| 前后极速 | `base + Σ(improvedBoost +4/+8, gearOil +2/+2, improvedGearOil +4/+4)` | km/h 加法 |
| `fireChance` | `engine.fire_chance × (1 − 0.2 protectiveKit)` | |
| `chassisRepairTime` | `hysteresisHealth / healthRegenPerSec ÷ ((toolbox ? 1.2 : 1) × (1 + repairSkill×0.04) × (protective ? 1.1 : 1))` | **客户端 hangarParams 的 chassisRepairTime 同源**：BI 11.21 实测 T-34 24/3.5=6.86 s、116-F3 66/9.625=6.86 s；工具箱/防护套件系数取 script（+20%/+10%） |
| 消耗品计时 | 101 高端消耗品：`duration × (1 + 33%)`；118 配送系统：`cooldown ÷ (1 + 0.17)`（描述为 cooldown **speed** +17%） | 客户端装备 script；101/118 在全部预设内；119 望远镜不在 11.21 任何预设内（不可装备），不建模 |
| `penetration` | 近/远穿深均 `× (calibrated ? AP/APCR 1.06 : HEAT/HE 1.07)` | 与客户端脚本一致；弹种明细同乘（校准弹同时改远距穿深） |
| `damage` | `shell.damage × tungsten`（tungstenTip script 的 `minDamageFactor/maxDamageFactor` 均值 = 1.15） | BlitzInfo 实测 +15%；同时作用于 DPM 与模块伤害 |
| 弹间装填 | `intraClip × newAutoloader.clipReloadTimeFactor`（0.7） | 整匣装填不受影响（BlitzInfo T57：2.40→1.68） |
| 弹匣总伤 | `damage × shellCount`（autoloader/autoreloader；regular 不给） | 参数行 `gunClipDamage`（BK clipDamage 同口径） |
| 远距穿深（超负荷） | `near − (near − far_calibrated) × (has107 ? 0.4 : 1)` | 客户端 script `piercingPenaltyFactor500m = -60`：远距**衰减量** ×0.4；弹种明细显示 |
| 改进型引擎增压 | 车体转向 ×`rotationSpeedFactor`（1.05） | 炮塔转向不变（BlitzInfo WZ-111：27.12→28.47） |
| 火药补给 | 弹速 +`projectileSpeedFactor`（35%） | 客户端 script = 35；**BK 旧值 30% 不采用**；BlitzInfo 60TP：780→1053 |
| 改进型悬挂 | 通过性 ×(1+20/15/30%)（阻力 /(1+b)） | BlitzInfo Type 71：91→109 / 83→96 / 64→84 |
| 装甲表 | 板值 × `(1 + armorHomogenizationFactor/100)`（multiLayerArmor 110，未装 = 1） | 客户端 script +4%；BI Main Armor 显示裸板值，配置器按用户要求联动（2026-10-27） |
| `shellVelocity` | `base × (1 + 0.35 supercharger + 0.35 gunpowder)` | 两处均为客户端 script 的 +35%；弹种明细速度同乘 |
| DPM | `resolveDpm`：regular / autoloader / autoreloader（最优弹）；弹夹炮的整匣循环 = `reload + (n−1)×intraClip`，**连发组（PomPom）** = `reload + (n/每组−1)×intraClip + (n/每组)×(每组−1)×组内间隔` | ×60；连发组字段 `config.burst_group_count/burst_group_interval`（pb gun field 20），无组时退化 |
| 有效 DPM（自动装填炮） | BK `dpmEffective` 的 60 s 窗口：`damage / (首发或末发×reloadCoeff + intraClip) × (60 − (n−2)×intraClip) + damage×(n−1)`；`reloads[0] > reloads[1] + intraClip` 时取末发，否则取首发 | 只有 `autoreloader` 且逐发装填 ≥2 时有值；参数行 `gunEffectiveDamagePerMin`（`gunCommonDamagePerMin` 之后）。少于两发 → null（fail-closed） |
| 尺寸（BI Dimensions） | 底盘 + 车体 + **所选炮塔**碰撞盒并集，不含炮管；包围盒坐标 `[x 宽, y 长, z 高]`；挂点 `[x, 上, 前]` 换序后叠加（炮塔挂点 = 车体挂点 + 炮塔相对挂点） | BI 11.21 逐值对拍：T-34 顶级炮塔 5.72×2.94×2.63 m、116-F3 7.63×3.49×2.70 m；空盒/缺挂点 → 整组隐藏（fail-closed） |
| 俯仰/朝向 | `applyPitchYawLimits`：yaw 夹取；有效仰角 = `abs(pitch.min) − initialPitch`、有效俯角 = `pitch.max + initialPitch`（4 辆意系 AT-SPG），再 ±稳定器（+4°上/+3°下）；front/back 扇区按 yaw 切换，过渡带（默认 20°）线性插值 | 3D 场景用；BI 11.21 对拍 Minotauro 18/7+pitch3→15/10、Ciclope 20/7+6.5→13.5/13.5 |
| 隐蔽 | 见 §5.3 | 客户端迷彩网 script + camouflages.xml |

### 5.3 隐蔽加成（2026-10-27 用户裁定：按客户端数据）

- **迷彩网**（`devices.json` 的 `camouflageNet` script）：`bonus` 逐车型
  LT 0.06 / MT 0.04 / HT 0.02 / AT-SPG·SPG 0.07；移动时 +bonus，静止
  （`activateWhenStillSec` = 3s）时 ×`activateBonusFactor` = 2 →
  LT 0.12 / MT 0.08 / HT 0.04 / TD·SPG 0.14。客户端描述同口径
  （“The bonus is doubled if the tank remains stationary”）。
- **涂装**（`camouflages.xml` 的 `<invisibility><categories>`）：配置器“涂装”开关用
  `defaultCategory = free`（标准免费涂装，与 disposable / common-camo-credits 同值）：
  LT .03 / MT .03 / HT .02 / TD .04；`reusable` 类别为 .05/.05/.03/.06（本期开关不区分）。
- **五种状态**（BK 同口径；数值仍取客户端）：静止 / 移动 / 静止开火 / 移动开火 / 起火。
  开火 = 对应静止/移动隐蔽 × 炮 `invisibilityFactorAtShot`（剩余比例）；起火 = 静止隐蔽 ×
  车辆 `invisibility.firePenalty`。BI 页面只展示前三种，后两种按 BK 补齐。
- **合成**：`base × (1 + camouflage 技能 perLevel × 等级) + 涂装 + 迷彩网`
  （涂装/迷彩网仍为**绝对值相加，百分点**；2026-10-27 用户二次裁定，替换 BK 的相对
  乘算口径）；技能只放大裸隐蔽，不放大涂装/网。开火 = 静止隐蔽 × 炮
  `invisibilityFactorAtShot`。移动/静止各用各自的客户端基础值与迷彩网 bonus（LT 的
  `moving` 基础值与 `still` 相同，移动时只吃移动 bonus，不再套静止翻倍值）。
  基础值 = 客户端 `<invisibility>`；加成表缺失按 0（fail-closed，不臆造数值）。
- **berserk 的 +10% 模块伤害**：只在客户端描述文案里（`<script>` 不完整，BK 亦未建模）
  ——**只展示描述原文，不复算**。
- 其余客户端有而 BK 未建模的（`antiHighExplosive`、`armorMover`、
  `equipmentDurationFactor/ReloadBoost`）：展示 script 原文，不进 characteristics 数值。

---

## 6. UI 规格：整合配置界面

### 6.1 入口与布局（BlitzKit Tankopedia 同构）

- **入口**：现有 `AgentTankopedia` 详情态就地扩展（**不新增 view/tab**）；
  `?tank=<id>&config=<n>` 已有，新增可序列化的选择状态（§6.5）。
- **布局**（桌面 BlitzKit 同构两栏，移动端纵向堆叠）：

  ```text
  ┌─────────────────────────┬──────────────────────────────┐
  │  模型入口（装甲查看器）    │  参数区（三张卡片自动分栏）      │
  │  配置面板                 │  · 防护（装甲表 + 参数）        │
  │  ① 模块（图标选择）       │  · 火力（参数 + 弹种图标 + 射界图）│
  │  ② 装备 3×3              │  · 机动                        │
  │  ③ 补给 / ④ 消耗品       │                              │
  │  ⑤ 乘员技能（20 图标）     │                              │
  │  ⑥ 涂装                  │                              │
  └─────────────────────────┴──────────────────────────────┘
  ```


- 模型区复用装甲查看器（3D 模型 + 板值）——**不是**两个页面，是详情页内的同一模型区
  （若现有详情页与装甲页分离，则以装甲查看器为模型区宿主、百科数值区并入其侧栏，
  具体归并方式 WotbTools 按现有信息架构定，契约见 §6.4；**落地取此分支**：详情页左栏是
  「装甲查看器入口卡」（携带 `?config=`，不嵌入 3D 宿主），右栏是配置面板）。

### 6.2 模块选择（配置面板 ①）

- 四行图标选择器（客户端模块页同构）：**炮塔**（该车 turrets 列表）→ **炮**
  （所选炮塔的 guns）→ **发动机**（该配置 engines）→ **履带**（该配置 tracks）；
  每行 = 类型图标 + 候选卡片（类型图标 + 等级角标 + 名称），关键差量（炮：装填；
  发动机：功率；履带：转向/载重）放 tooltip。
- 选择 → `(turret, gun)` 对号 `configIndex`（`turret_local/gun_local` 匹配；无匹配
  组合 fail-closed 禁选）→ 触发模型联动与参数重算。
- **无研发状态**：全部可选（用户裁定：全部解锁后正常选择）；客户端的模块等级/价格
  不展示（如需可作次要文本，不作交互）。

### 6.3 装备 / 补给 / 消耗品（配置面板 ②③④）

- **装备（9 槽 3×3 矩阵，BK EquipmentManager 同构）**：
  - 行 = Level I/II/III，列 = attack/defence/specialization（标题
    `optionalDevices/Type/*` = COMBAT POWER / VITALITY / SPECIALIZATION）；
  - 每槽左右两候选按钮（`devices.json presets[车.optDevicePreset]`）+ 可清空（null）；
    选中态高亮，即点即换；
  - 每格两个候选 = 客户端物品图标（40px，`icons/items/devices/*.webp`）；
    tooltip = 名称 + 描述模板（`display_params` 填 `%(...)`）；名称同时是 `aria-label`；
  - `stereoscope`（未引用件）不出现。
- **补给（图标格多选）**：池 = `mode=common` 且 `vehicleFilter` 对当前车求值通过；
  数量上限 `provisionSlots`；图标来自 `icons/items/provisions/*.webp`，名称走
  `aria-label`/tooltip；默认空（BlitzInfo 中性基线；客户端默认表仍随数据导出）。
- **消耗品（图标格多选）**：同上，上限 `consumableSlots`，图标
  `icons/items/consumables/*.webp`。
- **过滤器求值**：include 各规则 AND、规则间 OR，exclude 命中即排除；
  **extendedTags 全语义自实现**（BK 只有 CLIP）。
- **涂装开关**：默认关；开启后隐蔽参数按 §5.3 涂装公式变化（标准免费类别）。
- **加成比例提示**：面板底部显示当前生效的隐蔽加成百分比（迷彩网“移动/静止”、
  涂装“+N%”）。参数区 delta 是**百分点**（如 +0.7），与百分比加成不是同一单位，
  避免把重坦 +4% 加成误读成“只加了 0.7%”。
- **参数区呈现（对齐 BlitzInfo，2026-10-27 用户确认）**：
  - **紧凑卡片列**（BlitzInfo `sheet-col` 同构，2026-10-27 用户要求“不要一整行只有
    头尾有数据”）：三组参数放进同一 `tp-param-grid`，`column-width: 280px` 自动分栏
    （1440 宽屏 3×≈360px、1024 3×≈282px、手机单栏），组标题小号大写、行用细分隔线；
    行内标签左/数值右只占卡片宽度，不再横跨整个参数栏；
  - 顶部**概览条**：HP / 单发伤害 / 穿深 / 前进极速 / 视野（引擎值，5 格）；
  - 火力组标题显示**口径 + 火炮循环类型**（常规 / 弹夹 / 自动装填，来自 config 的
    `is_burst`/`is_drum`）；
  - 弹夹/自动装填行：`gunClipReloadTime`（整匣）、`gunClipShellCount`、
    `gunClipShellReloadTime`（弹夹弹间）或 `timeBetweenPumpShots`（自动装填间隔）、
    `reloadingShellTime0..5`（逐发自动装填，应用装填手/输弹机系数）；
    DPM 按 BK `resolveDpm` 补齐弹夹/自动装填（此前为 —）；自动装填炮另有
    `gunEffectiveDamagePerMin`（BK `dpmEffective` 60 s 窗口，常规/弹夹不显示）；
  - **转向行随炮塔有无切换**：`hasTurret=false` 的车（客户端 `<hull><fakeTurrets>`
    标记）把“炮塔转速”行替换为“火炮转向速度”（`gunTraverseSpeed`，火炮自己的
    `<rotationSpeed>` × 同一燃料/炮手系数）；有炮塔车仍显示炮塔转速；
  - **炮塔射界**：`turretYawLeftLimit`/`turretYawRightLimit`（模块 `yawLimits`）；
  - **弹种图标选择置于火力卡最前**（2026-10-27）：客户端弹种图标（`icons/shells/**`，9 类）
    + 选中弹的 2 列数值网格（穿深/伤害/弹速/远距/模块/正化/跳弹/射程/备弹/爆炸半径）；点击图标 →
    `?shell=`，概览条与火力参数（伤害/穿深/弹速/DPM）随选中弹重算；换炮塔/炮/配置
    自动回默认 AP。无模块数据的静态详情保留只读全列表。
  - **乘员技能**（2026-10-27）：客户端 `avatar.xml` 的 20 项技能按车型分组展示
    （40px 图标 + 等级角标），点击循环升级 1–7、Shift 直达 7、满级后再点关闭，写 `?skills=<id>:<level>`；默认 0（中性基线），效果全局生效
    （训练资格按车型，已训练技能跨车生效）。技能数值只读导出 `skills.json.effects`
    的 per-level 值（等级 N = perLevel×N），**不采用 BlitzKit 的硬编码加成**；Smooth
    Turn/Ride/Recoil/Turret 进动态扩圈、Virtuoso 进车体转向、Camouflage 乘裸隐蔽，
    Repair 进履带维修时间（客户端 `hysteresisHealth / healthRegenPerSec`）。
  - **强化装甲联动**：`multiLayerArmor`(110) 的 `armorHomogenizationFactor`（+4%）
    进装甲表值与 delta（脚本权威，未装 = 1）；装甲查看器板色仍为裸板值。
  - **消耗品冷却**：图标右下角显示 `script.params.cooldown`，tooltip 含 duration；
    **有持续时间的消耗品**在左上角另显 `script.params.duration`（强调色），如超速传动器
    12s / 冷却 65s；只有冷却的（自动灭火器 70s）不显示 duration（BlitzInfo
    `tile-timing` / `tile-duration` 同构）；
  - **交互式火炮射界图**（BlitzInfo “Gun arcs” 同构）：`GunArcDiagram` 俯视圆盘，
    车头朝上、俯仰带随 `pitch_limits`（models.pb 的 `front`/`back` 扇区 + `transition`
    线性过渡）2° 采样绘制；悬停/拖动（键盘 ←/→）改变炮塔方位角，实时显示
    Bearing / Up / Down，指针半径对应具体 Barrel 俯仰；全向炮塔画环形带、有限射界画
    带端盖的扇区带，±10° 参考环、车头/车体轮廓、离开圆盘复位（鼠标；触屏保留最后值）。
    径向映射**内外翻转**（2026-10-27 用户要求）：**内圈 = 俯角、外圈 = 仰角**；半径锚点
    取 BI 11.21 数值的镜像（+10°=0.7126、+18°=0.8295、−3°=0.4315、−6°=0.3660，
    地平线 0.5）。装备 `improvedPitchLimitsSystem` 的 +4°/+3° 计入（对应 BI
    “stabiliser in the kit” 同语义）。
    - **2026-11-01 实现状态**：装甲查看器手动拖炮接入同一 122 勾选（目标/被展示车，
      默认关；射击复现按 `target_equipment.improved_pitch_limits` 自动勾选/禁用）。
      3D 回放/射击复现的 prop2 俯仰解码由 Agent core 按同 eid 的 Type5 `0B` 配件串
      叠加 +4°/+3°；WotbTools 的 `limitsJson` / `pitchLimits.js` 仍只传基础 models.pb
      极限，**不得在此预加**（否则双重计算）。
  - **初始/顶级快捷**：只切换模块（config/发动机/履带），保留装备与物品选择。

### 6.4 联动契约（配置面板 ↔ 模型区 ↔ 参数区）

| 联动 | 机制 | 说明 |
|---|---|---|
| 炮塔/炮 → 模型 | `configIndex`（现有 `?config=` 契约） | 装甲查看器模型节点与板值随炮塔/炮切换；发动机/履带不影响模型（GLB 只有一组履带网格） |
| 全部选择 → 参数 | `characteristics(setup)` 纯函数 | 参数区实时重算；delta = 当前 − 上一次渲染值（BK useDelta） |
| 参数 → 百科数值 | 详情页统计字段改为引擎输出 | hp/dpm/pen/speed/viewRange/… 与参数区同源（单一计算路径，不再直读静态字段） |
| 状态可分享 | URL 序列化（tankopediaQuery 扩展） | `config/engine/track/shell/dev/cons/prov/skills/camo` 进 URL；返回键/分享保持状态 |
| 回放装配 → 配置器 | `shooter_equipment.raw` id 集合 | 战斗详情/射击复现的装备徽标可一键「在配置器中打开」（id → 槽位由预设反解，歧义 fail-closed 置空） |

- **delta 参照（2026-10-27 改为 BK useDelta 同逻辑）**：当前值 − **上一次渲染值**；初次
  加载无 delta；改变模块/装备/物品/技能/弹种时显示本次变化，下一次变化前保留。不再用
  “初始配置 + 空装备”的固定基准。装备槽仍可留空以得到中性初始配置，但它不是 delta 基准。
- **delta 渲染（BK Info 同构）**：先显示**绝对差值** + 三角箭头（▲/▼），再显示主值；
  颜色 green/red 由 `biggerIsBetter` 与差值方向决定；0 不显示；主值 `digits` 位小数 +
  千分位；单位按 `params/format.json`。
- **参数分组**：`params/structure.json` 驱动（`protectability / gun / mobility` 主分组 +
  概览条 + 逐组件组）；表格（装甲 front/side/rear × turret/hull，`hasTurret` 条件列）与
  条件呈现（`presented: pumpGunMode`）按数据判定；**强化装甲 +4% 进参数区装甲行与 delta**
  （multiLayerArmor 110 的 `armorHomogenizationFactor`）；装甲查看器板色仍为裸板值（与 BK 一致）。

### 6.5 URL 状态（可分享）

```text
?view=agent-tankopedia&tank=<id>
  &config=<n>&engine=<n>&track=<n>&shell=<n>
  &dev=<0|1|-9×9 首选编码，如 0,1,null→"01_">
  &cons=<stem,stem>&prov=<stem,stem>&skills=<id,id>&camo=<0|1>
```

编码细节 WotbTools 定（沿用 tankopediaQuery 的 replace/push 规则：选择用 replace，
`?tank=` 切车用 push）。

### 6.6 回放装配展示

- `shots[].shooter_equipment.raw`（id 集合）→ 装备图标 + 名称徽标（射击复现 pane 与
  战斗详情车辆卡）；`calibrated_shells/enhanced_armor` 布尔字段保持不变（向后兼容）；
- `loadout`（`hp_base/hp_initial/hp_bonus_pct`）→ HP 加成徽标（加法口径已实证）；
- 车辆卡「在配置器中打开」→ 跳详情页并按 §6.4 反解选择。

---

## 7. 与客户端的已知偏差（随 UI 声明）

| # | 偏差 | 原因 | 处理 |
|---|---|---|---|
| 1 | 无研发/购买/解锁流程，全部可直接选择 | 用户裁定（2026-10-27） | 模块等级/价格不参与交互 |
| 2 | 装备槽允许留空 | 客户端机制不可卸下；中性初始配置需要 | BK 语义；默认全空 |
| 3 | 涂装开关只建标准免费类别（`free`），不区分 reusable/event 涂装 | 客户端按具体涂装取值 | 数值来自 `camouflages.xml`；后续可做涂装选择 |
| 4 | 无科技树页 | 用户裁定 | 研发图导出器保留可选输出，本期不消费 |
| 5 | 装备/消耗品/补给用客户端图标（图集裁剪），仅 `stereoscope` 无图回退文字 | 该件无预设引用、客户端无装备图标 | `icons/manifest.json.missing` 显式登记；视觉与客户端一致 |
| 6 | berserk +10% 模块伤害等描述级效果不复算 | `<script>` 不完整 | 只展示描述原文 |
| 7 | 乘员技能 1–7 级由点击循环/Shift 直达，不做逐级下拉 | 紧凑 UI 选择 | 技能值 = 客户端 perLevel × level；默认 0（中性基线） |
| 8 | 装甲查看器板值不叠加装备（+4% 装甲只进参数区） | BK 同口径 | 后续可选叠加显示 |
| 9 | 「一键升顶/高级化」等服务器价格项不存在 | 无账号数据 | 不出现 |
| 11 | 消耗品/补给/涂装默认空，不套用客户端挂载默认表 | 与 BlitzInfo 中性基线对照 | 可一键手选；默认表仍导出 |
| 10 | 声音/动效为语义等价 | 素材不搬 | WotbTools Motion 策略 |
| 12 | BlitzKit 仅隐蔽系数不可参考，其余数值/机制可参考；以下四处仍不照抄 BK | 2026-10-27 用户第二次裁定 + 客户端/BI 证据 | （a）无角色兜底 1.0：BK 用 1.05，BI 11.21 实测 T-34 无炮手但炮塔 49/瞄准 2.125 为基线，取 1.0；（b）火药补给 +35%：BK 写 +30%，客户端 script `projectileSpeedFactor=35`；（c）整车重量含电台/油箱：BK 漏掉，BI `wt` 对拍 725/732 一致；（d）BK 的 124 号装备在 11.21 不存在（客户端 24 件 100–123），不生成 +3° 俯角。隐蔽继续用客户端基础值/涂装/迷彩网（BK 的类别加成为近似值）；技能百分比仍取客户端 avatar.xml per-level |

---

## 8. 分期计划

| 期 | 内容 | 交付 | 验收 |
|---|---|---|---|
| **P0 数据管线** | `export_research_loadout.py`（本期消费面：modules/devices/consumables/provisions/defaults/params/strings/icons；研发图为可选开关） | 导出器 + `data/research/` + 本仓文档 | 统计与数据文档 §11.2 一致（24 设备/9 预设/13+34 条/过滤器/默认表）；`prx_*`、emptyPreset、stereoscope 边界用例通过 |
| **P1 计算层** | `characteristics.ts` 全公式 + 过滤器求值（extendedTags 全语义） | 纯函数 + golden | 与 BK 对 5 辆抽样车（LT/HT/TD/弹鼓/双炮塔）逐参数 parity（白名单 = §5.3 隐蔽加成（客户端）+ 描述级效果）；过滤器 golden（与 BK diff 只允许 extendedTags 类差异） |
| **P2 整合配置界面** | 配置面板（模块四选择 + 装备 3×3 + 补给/消耗品 chips + 涂装）+ 参数区 + URL 状态 | 详情页整合布局 | 换 rammer → 装填 −5% delta 可见；换食物 → 乘员链路正确；装备 tooltip 数值与 script 一致；用户视觉验收 |
| **P3 模型联动** | 炮塔/炮选择 ↔ 装甲查看器 configIndex 双向；百科数值切换为引擎输出 | 联动闭环 | 多炮塔/多炮车（T-34-2 类）模型与板值随选择切换；URL 分享恢复状态 |
| **P4 回放装配** | 装备徽标 + HP 加成徽标 + 「在配置器中打开」 | shots/loadout 联动 | 既有回放样本 golden；`111 → HP×1.05` 标注；id 反解歧义 fail-closed |

依赖：P1 可与 P0 尾段并行；P2 依赖 P0+P1；P3 依赖 P2；P4 只依赖 P0 的 devices 映射。

**落地状态（2026-10-27）**：P0、P1、P2、P3、P4 已全部实现（P4 回放装配徽标 + 科技树/研发界面同日落地，见 §12.2）；
数据、引擎、状态层、UI 与三语文案均落在 WotbTools 仓，验收口径见 §12.3。

---

## 9. 测试与门禁

1. **数据 golden（本仓）**：`data/research/**` 快照——24 设备、9 预设、13/34 条、
   逐车模块字段集、过滤器/默认表；`prx_*` 与 0 成本保留；`emptyPreset`/`stereoscope`
   标记存在。
2. **公式 parity（WotbTools）**：`characteristics.ts` 输出 vs BK 快照（5 辆 × 若干
   setup），差异白名单 = §5.3（隐蔽加成按客户端后仅剩描述级效果类）。
3. **过滤器 golden**：每条 common 物品 × 758 车求值快照；与 BK diff 只允许
   extendedTags 类差异，否则 fail。
4. **URL 契约测试**：新增参数的解析/序列化/返回键行为（沿用 tankopediaQuery 测试模式）。
5. **联动测试**：configIndex 对号（含多炮车无匹配组合 fail-closed）；上次值 delta 的
   方向 × `biggerIsBetter` 颜色判定表。
6. **回放装配 golden**：`shooter_equipment` id → 图标/名称零未知（未知 id → 测试红）。
7. **视觉验收**：每期交付给用户「看什么」清单（对齐点 + §7 偏差表），由用户判定。

---

## 10. 未决问题

| # | 问题 | 影响 | 现状/建议 |
|---|---|---|---|
| 1 | ~~客户端战斗重量是否计电台/油箱~~ | p2w / 车体转向 | **已解决（2026-10-27）**：按整车计（hull+engine+track+turret+gun+radio+fuel），与 BlitzInfo 11.21 `wt` 逐辆一致（T-34 29.942、WZ-111 44.5）；电台/油箱 738 辆各 1 件不可换装 |
| 2 | ~~`chassisRepairTime` 真实公式~~ | 修复时间参数 | **已解决（2026-10-27）**：客户端 `hysteresisHealth / healthRegenPerSec`；BI 11.21 T-34 24/3.5、116-F3 66/9.625 均为 6.86 s |
| 3 | 装备 tooltip 的效果文案口径 | 展示 | script 原值 + 描述模板填充；描述级效果（berserk +10%）按原文 |
| 4 | ~~设备图标图集解析~~ | 视觉一致度 | **已解决**：按 UI 样式表 bg-sprite 裁剪（§3.5/§12.2.8） |
| 5 | ~~涂装数据（`camouflages.yaml`）是否纳入~~ | 涂装开关的真实加成表 | **已解决**：客户端 `camouflages.xml` 的 `categories[defaultCategory]`（§5.3），不再用 BK 类别常量 |
| 6 | `other` 国车的模块/装备数据完整性 | 配置器覆盖面 | P0 导出时按 fail-closed 报告缺失，不猜 |
| 7 | 科技树页（后续可选） | — | 数据已保留（导出器可选开关）；需求出现再启 |

---

## 11. 与既有文档的关系

- 数据事实、schema 细节、公式校验证据：[client-research-tech-tree-loadout.md](client-research-tech-tree-loadout.md)；
- 数据来源与提取链铁律：[game-data-sources.md](game-data-sources.md) §2.6；
- 数据面总账（`data/research/` 落地后需同步登记）：[data-inventory.md](data-inventory.md)；
- 跨仓消费与 pin/golden 规则：[AGENTS.md](../AGENTS.md)（P0 落地时按「Parser and
  contract changes」条款在 `docs/index.md` 登记变更并同步 WotbTools）。

---

## 12. 落地记录（2026-10-27）

### 12.1 产物位置

| 层 | 本仓（WoT-Blitz-Agent） | WotbTools |
|---|---|---|
| 导出器 | `tools/export_research_loadout.py` → `data/research/`（磁盘 123 文件 3.71 MB；`meta.json` 记版本 + 全部 122 个产物的 sha256；预览车用 `--game-data` / `--bridge` / `--modules-only --merge` 增量并入） | **全部非图标数据进资产包 `research/**`**（`scripts/export_asset_pack.py` 发布，assetProvider 读取）；`common/research/` 只保留全局图标集；新坦克/物品不需 WotbTools 发版 |
| 目录/过滤 | — | `frontend/src/utils/researchCatalog.js`（`loadResearchCatalog` 请求全局 `/research/*.json`、`loadTankModules` 请求 `/research/modules/{nation}.json`、`availableItems` 全语义 vehicleFilter、`resolveDeviceIds`、`scriptMap`、图标 URL glob） |
| 公式 | — | `frontend/src/replay-local/compute/characteristics.ts`（纯函数；22 条 golden 覆盖 T-34 三套配置 + 乘员曲线 + 迷彩网 + 装甲系数 + 弹速 + 技能/扩圈） |
| 状态/参数行 | — | `frontend/src/replay-local/compute/tankSetup.ts`（config 对号、装备槽编解码、上次值 delta、装甲表；24 条单测） |
| UI | — | `TankConfigPanel.vue`（模块 4 行 + 装备 3×3 + 补给/消耗品 + 20 技能图标 + 涂装）、`TankParamPanel.vue`（三分组 + delta + 弹种图标 + `GunArcDiagram`）、`AgentTankopedia.vue` 详情态左配置/右参数整合 |
| URL | — | `tankopediaQuery.js` 新增 `engine/track/shell/dev/cons/prov/camo`；`config` 仍是炮塔/炮唯一来源 |
| 文案 | 客户端 strings 导出（en + zh-Hans） | 参数名/单位/分组 UI 文案走三语 locale；物品/模块名用客户端 en + zh-Hans，ru 回退 en |

### 12.2 实现期确定的口径（补 §4/§5 未定项）

1. **语义标记而非统计**：`tank/{id}.json` 不含 list.xml `<tags>`，也没有 `stem`；当前物品
   过滤器的 `extendedTags` 只用到 `clip`（berserk/pretendDead 的 exclude），由配置的
   `is_drum`/`is_burst` 语义标记补出（弹夹/连发不吃肾上腺素，与引擎 `isBurstGun` 守卫同口径）；
   18 条 common 物品的 `ids` 规则（smallHPStock/largeHPStock/gearOil/fireControlSystem 等）用
   `modules/{nation}.json` 的**条目键（stem）**补出 `nation:stem`。两者都不是从数值统计推断。
2. **参数小数位**：客户端 `format` 对百分数不给 `digits`，实现统一按 1 位（否则 10% → 10.3%
   的隐蔽差值与 delta 会显示成同一个整数）。
3. **基准**：初始配置（config 0）+ 首档发动机/履带 + 空装备 + 空物品 + 关涂装；装备槽默认全空
   （BK 语义，客户端不可卸下）。
4. **默认消耗品**：客户端 `consumablesDefault.yaml` 的 tier 表先按 vehicleFilter + 槽位过滤
   （tier 5 表 4 件 → T-34 3 槽取前 3）；补给默认空。
5. **模型联动**：不嵌入 3D 宿主；`?config=` 由配置器写、装甲查看器入口携带，模型/板值在查看器里
   随该下标切换（§6.4 的契约路径）。参数区与弹种表穿深同步为引擎输出。
6. **弹种分类**：`ap` / `ap_premium` / `ap_cr` / `ap_cr_premium` ×1.06；`hc` / `hc_premium` /
   `atgm_heat`（HEAT）与 `he` / `he_premium` ×1.07（修掉早期实现漏掉 `ap_premium` 的问题）。
7. **装甲 +4%**：`multiLayerArmor`(110) 按客户端 `armorHomogenizationFactor` 改参数区装甲行与 delta；装甲查看器板值仍为裸板（§6.4 同口径）。
8. **图标映射**：按客户端 UI 样式表的 class→`bg-sprite` 取图（不按 `<icon>` 文件名猜：
   `small-hp-stock`→`add_HP_normal`、`tungstenTip`→`Polish-consumables/consumable_tungsten-tip_m`、
   `grousers`→`optional-device_grousers_m` 等 17 处名字不一致）；图集帧按 `.txt` 的
   `x y w h` 裁剪，单图（新版 supply 树）整图导出。模块只有 5 张类型图（客户端模块卡片同款），
   因此模块候选保留名称作为卡片标题（类型图无法区分同型模块）；装备/补给/消耗品只用图标，
   名称走 `aria-label`/tooltip。
9. **vehicleFilter 语义（2026-10-27 修正）**：一个 `<include>` 块内的条件全部满足（AND），
   不是「有 tiers 就只看 tiers」。客户端与 BK 双重证据：XML 里 `ration` 是
   `<vehicle>6-10</vehicle> + <nations>ussr</nations>`；BK `checkConsumableProvisionInclusivity`
   对 include 用 `every()`（`blitzkit/blitzkit` `packages/core/src/blitzkit/`）。早期实现
   把条件拆成独立规则再 OR，导致所有国家的食物对所有车可见；修正后 T-34（苏 5 级）只剩
   condensed_milk / regular_fuel / safety_set / desertPower。现役 738 辆每辆仍有 ≥1 件可用
   补给与 ≥1 件可用消耗品（补给 1~11 件、消耗品 1~11 件）。
10. **战斗内技能不进入装配池**：客户端 `<isAbility/>`（如 `desertPower`，还带 `<geometryFilter>include desert</geometryFilter>`）
    是地图/模式内技能，不是机库补给；导出 `isAbility` 标记，`availableItems` 一律排除。
    不能按“缺本地化文案”判断（desertPower 恰好无 name，但语义标记才是权威）。
11. **dev 数据刷新**：全局 `common/research/` 在 `frontend/` 之外，Vite 不监听该目录；重新
    导出后必须重启 `pnpm run dev`。逐车 `research/modules/` 由资产包服务（`serve_asset_pack.mjs`
    每次请求读磁盘），替换包内文件后刷新页面即可，无需重启前端。

12. **隐蔽加成改回客户端（2026-10-27）**：迷彩网用 `optional_devices.xml` 的
    `CamouflageNet` script（逐车型 `bonus` + `activateBonusFactor`=2 仅静止生效），
    涂装用 `camouflages.xml` 的 `free` 类别；新增导出 `camouflage.json`（`defaultCategory` +
    全部类别表）。**合成为绝对值相加（百分点）**：116-F3（HT，基础 11.6/5.8）装网+涂装 =
    17.6/9.8。不再使用 BK `camouflageCoefficient` 表与相对乘算（LT 静止 .14→.12、TD .2→.14、
    涂装 LT/MT .02→.03、HT .03→.02）。失效旧函数 `camouflageNetBonus`/`camouflagePaintBonus`。

13. **BlitzInfo 参数对齐（2026-10-27 用户确认）**：新增概览条、火炮类型/口径、弹夹与
    自动装填行（含逐发时间）、弹夹车 DPM、模块伤害/正化/跳弹角、炮塔射界、消耗品冷却
    徽标、初始/顶级快捷。弹夹/自动装填的 DPM 用 BK `resolveDpm`（自动装填取最短单发循环），
    乘员/输弹机系数作用于整匣与逐发装填，弹间装填不受影响。扩圈米数与尺寸仍缺口径/验证，未加。
14. **装甲表随所选炮塔（2026-10-27）**：导出每个炮塔自己的 `primaryArmor`（front/sides/rear
    板 id，客户端 `<primaryArmor>`）；前端用“该炮塔 primaryArmor + 当前 config 的
    `turret_plates`”解析 front/side/rear，缺失时回退顶层 `armor.turret`。T-34 切初始后
    炮塔装甲从 60/60/55 变为 55/55/55（炮塔不同、板映射也不同），车体不变。

15. **全面机制对照修正（2026-10-27）**：按 BlitzInfo 实测修正——钨芯穿甲弹 +15%（伤害/DPM/
    模块伤害，客户端 min/max 因子均值）、炮弹装填加速（弹间 ×0.7，整匣不变）、改进型引擎
    增压车体转向 +5%（原 BK 的 +1% 错误）、火药补给弹速 +35%（原 +30%）、改进型悬挂按
    通过性 +20/15/30%（阻力 /(1+b)，原 ×0.8 错误）、装甲表乘 multiLayerArmor 的 +4%（客户端
    `armorHomogenizationFactor`）；默认消耗品改为空（中性基线），否则基准 DPM 会带上默认 berserk。

16. **BlitzInfo 11.21 跨车型复核（2026-10-27）**：以 11.21 渲染页 DOM + 其
    `data/wg/11.21/tanks.json` 对照 10 辆不同车型（116-F3 弹夹 HT、WZ-111、T57 弹夹 HT、
    Rinoceronte 自动装填 HT、Type 71、T-34 MT、WZ-132 LT、SU-85 无炮塔 TD、AT 7 无炮塔 TD、
    60TP），修正：
    - **乘员角色兜底**：无该角色不再 ×1.05（BK 旧口径），改为 ×1.0。T-34 无炮手 →
      瞄准 2.125 / 散布 0.4 / 炮塔 49（BI JSON 同值）。
    - **俯仰符号**：客户端 `pitch_limits` 存 `-up down`，此前上下颠倒；T-34 现为 +26/−8。
    - **车体转向**：`track.traverse × hullCoeff × (当前/初始发动机功率) × (初始/当前整车重量)`；
      整车重量补电台/油箱（见 §5.2 `weightKg`）。T-34 = 53.674（BI 53.6745）、
      Type 71 = 21.54（BI 21.538）。
    - **无炮塔 TD 装甲表**：`model_turret_count == 0`（SU-85/AT 7）不再显示炮塔装甲列，
      只保留车体。
    - **数据面口径**：`tank/{id}.json` 的逐配置 `caliber` 改取**该炮弹表自带弹径**
      （客户端权威），删除炮名解析 + 120 回退（全车 421 个配置与自身弹径矛盾：AT 7
      20-pdr 83.4 被显示成 120）。738 份 `data/tank_data` 已重生成，包内 `tank/` 与
      manifest 已刷新。
    对拍结果：732 辆可配对，整车重量 725/732 ≤0.02 t、车体转向 725/732 ≤0.06；余 7 辆是
    BI 默认顶配选炮不同（如 ISU-152 的 BI 取 122mm BL-9S 2.59 t，我们末档为 152mm BL-10
    6.5 t，两门均可选），非公式差异。其余复查项一致：WZ-111 DPM 1925.5 / 钨芯 460·2214、
    T57 装填加速 2.40→1.68、Type 71 悬挂 90.7→108.8/95.9/83.7、T-34 视野 240/279.2、
    隐蔽 28.8/21.6。**仍存偏差**：BI JSON 的固有隐蔽与本仓相差 ±0.3–0.5（其页面不显示）；
    BI 非顶级配置页的 Aiming Time / Gun Turn Limit 取**初始炮**，其 JSON 与我们同源；
    扩圈米数与车体尺寸仍缺口径/验证，不建模。

17. **消耗品生效时间 + 交互式火炮射界图（2026-10-27）**：消耗品按客户端 script 双徽标——
    左上 `duration`（强调色）、右下 `cooldown`（次要色），如超速传动器 12s / 65s；仅冷却
    的灭火器 70s 不显示 duration。火力组原“俯仰随炮塔角度表”按用户要求改为
    **BlitzInfo Gun arcs 同构的交互图**（`GunArcDiagram.vue`）：俯视圆盘、车头朝上；
    悬停/拖动/键盘 ←→ 改变炮塔方位角，实时显示 Barrel / Bearing / Up / Down，指针半径
    对应具体俯仰；阴影带由当前 config 的 `pitch_limits`（models.pb 的
    min/max/front/back/transition）按 2° 方位采样绘制——全向炮塔为环形带，有限射界为
    带端盖的扇区带；±10° 参考环、车头与车体轮廓；鼠标离开复位 0°，触屏保留最后值。
    径向映射内外翻转（2026-10-27 用户要求）：内圈俯角、外圈仰角；半径锚点取 BI 数值的
    镜像（+10°=0.7126、+18°=0.8295、−3°=0.4315、−6°=0.3660，纯显示层）。
    装备 `improvedPitchLimitsSystem` 的 +4° 仰角 / +3° 俯角计入（对应 BI “stabiliser in
    the kit” 语义）。对照：116-F3 车头 18/6、车尾 18/3；T-34（F-34）车尾下限 +2°。

18. **弹种选择 + 强化装甲联动（2026-10-27 用户要求）**：弹种选择并入火力卡（客户端
    弹种图标 + 选中弹 2 列数值网格），点击图标写 `?shell=` 并重算概览条与火力参数（伤害/穿透/弹速/DPM
    全部取选中弹）；换配置/炮塔/炮自动回默认 AP。`SetupSelection.shell` 为唯一选择态，
    `standardShellOf(config, shell)` 越界回落默认弹；URL 解析/序列化与键清单同步
    （`config/engine/track/shell/dev/cons/prov/skills/camo`）。强化装甲 110 按客户端
    `armorHomogenizationFactor` 把装甲表六值乘 +4%（未装 = 1），delta 相对空装备基准；
    装甲查看器板色仍为裸板值。

19. **BlitzKit 排版参考 + 客户端技能/图标/扩圈（2026-10-27）**：按用户裁定
    「BlitzKit 只参考排版，数值加成有问题」，不采用其技能/加成常量；技能 per-level
    数值改读客户端 `XML/item_defs/tankmen/avatar.xml`，导出 `skills.json`（20 项：
    class 归属、per-level effects、类型、客户端图标；文案取 Strings）。工作台改为
    BlitzKit 同构两栏——左栏模型入口 + 配置，右栏参数卡片；配置面板新增乘员技能
    分组（20 图标、1–7 等级角标、默认 0、`?skills=id:level`，效果全局；点击循环升级、Shift 直达 7）。条件技能也按触发效果计入静态表（BI 口径）：Close Combat Master→炮塔转速、Breakthrough→引擎功率、Rage/Desperado→装填、Deadly Accuracy→视野/瞄准。弹种改为 9 类客户端
    图标选择（`icons/shells/**`）。导出器同时新增 gun/chassis `dispersionFactors`，
    引擎据此生成移动/车体转向/炮塔转向/射击后/炮损扩圈五行，smooth_* 与 stabilizer
    按客户端技能参数与 −17% 组合；Virtuoso 进车体转向、Camouflage 乘裸隐蔽
    （再与涂装/网绝对值相加）。产物当时为 123 文件 3.18 MB（20 技能 + 9 弹种 +
    20 技能图标）。

20. **BlitzKit 数值对照补齐（2026-10-27 用户第三次裁定）**：用户改口「BK 只是隐蔽
    系数算得不太准，其它的可以参考」。逐项对照 `tankCharacteristics.ts` 后：
    - **新增**：履带维修时间（客户端 `hysteresisHealth / healthRegenPerSec`，
      工具箱/防护套件/Repair 技能加速；BI 11.21 T-34 与 116-F3 均为 6.86 s）、
      车体尺寸（底盘+车体+所选炮塔碰撞盒并集，不含炮管；BI T-34 顶级 5.72×2.94×2.63 m、
      116-F3 7.63×3.49×2.70 m 逐值一致）、弹匣总伤；初始炮塔 pitch 计入
      有效俯仰（4 辆意系 AT-SPG）；肾上腺素改用客户端
      filter 的 `clip` 排除（与 BK 的 regular-only 一致）。
    - **不照抄**：BK 的 1.05 无角色兜底（BI 实测 1.0）、火药补给 +30%（客户端 35%）、
      漏电台/油箱重量（BI 对拍含）、124 号装备（11.21 不存在）。隐蔽继续用客户端。
    - **装备联动补齐**：101 高端消耗品（持续 +33%）与 118 配送系统（冷却速度 +17%）
      联动消耗品计时徽标；其余预设装备（108/120 模块耐久、109/121 模块/乘员被击中概率、
      112 履带耐久）只影响界面未展示的模块耐久/概率，BK 同样未建模；这些装备的
      悬停 tooltip 会明确标注“不进入参数表（影响模块耐久/被击中概率）”，避免误判
      未生效；119 望远镜不在 11.21 预设内。
    - **隐蔽五状态**：在静止/移动/静止开火之外补 移动开火 与 起火（客户端系数，BK 口径）。
    - **布局**：配置栏保持原先的左栏排布（模型入口 + 配置，右参数区）；只把配置里的
      坦克模块区改成 4 列矩阵——首行 4 个类型**纯文字**标签（图标只在选项瓦片），之后按
      选项序号逐行展开（炮塔1|火炮1|发动机1|履带1，下一行炮塔2|火炮2…），4 列严格对齐、
      向下延伸，消除模块右侧空白；装备/补给/消耗品/技能/涂装块排布不变。
    - **火力去重/精简**：弹种明细已显示穿深/伤害/弹速，参数行删除 `baseShellAvg*` 三行；
      删除射速行（DPM 已表达），窥探 DPM 整体移除（不再是引擎输出）；其余装填/弹夹/瞄准/散布/俯仰行保留。
    - **弹种明细联动补齐**：校准弹同时乘近距/远距穿深；超负荷的弹速 +35% 与
      `piercingPenaltyFactor500m = -60`（远距衰减量 ×0.4）都进弹种明细；火药补给的弹速
      +35% 也进明细。修复了“选超负荷/火药看不到弹速变化、选校准弹远距穿深不变”的漏接。
    - **术语**：中文 `normalization` 由“正化角”改为“转正角”。

21. **连发组（PomPom）DPM 修复（2026-10-28，用户报告 AC Atlas DPM 偏小）**：客户端
    `<burst count rate>`（一次扳机连发 N 发、组内 60/rate 秒）此前完全没进数据面，弹夹
    公式把整夹每一发都按扳机间隔计，组内弹间被重复放大。按 BK `resolveDpm` 的 `gun.burst`
    分支修正：
    - **数据**：`extract_vehicles.py` 从内联/共享 `<burst>` 导出 `burst_group_count` /
      `burst_group_interval`（不改 is_burst 分类铁律）；`emit_vehicle_pb.py` 写 pb gun
      field 20（`Burst{count, interval}`）；`blitzkit.rs::parse_gun` 解码、`tank_configs.rs`
      写 `configs[].burst_group_*`。35 辆 / 39 个配置受影响（另 4 个非弹夹炮带字段但
      `is_burst=false`，计算不受影响）。
    - **计算**：`gunCycleOf` 透传两字段；autoloader DPM 用整匣循环 + 组内/组间拆分。
      AC Atlas V2（整夹 6、组间 3s、每组 3、组内 0.25s、装填 13s、单发 120）：旧 1573 →
      2623（本仓默认乘员系数）；无连发组时公式不变。
    - **显示**：火力组补客户端同名连发行 `gunClipBurstCount`（连发组数）/
      `gunBurstShellCount`（每组发数）/`gunClipBurstReloadTime`（组内连发间隔），
      非连发组炮不出现；整夹只有一组连发（Annihilator/Cerberus）时**不显示弹间装填**
      （不存在组间扳机间隔，唯一间隔是组内值）。
    - **资产包**：两包 `tank/*.json` 共 70 件替换 + manifest 重算（0 mismatch）。
    - **测试**：characteristics 新增 PomPom 用例（2618–2628 / 每分钟 21.8–21.9 发）；
      tankSetup 覆盖组字段与「缺字段/count=1 视为无组」。
    - **前后对比**：改为 BK `useDelta` 同逻辑——delta = 当前值 − **上一次渲染值**（初次
      加载无 delta；改变选择后显示本次变化，直到下一次变化），渲染顺序为绝对差值 + ▲/▼
      再跟主值，颜色按 `biggerIsBetter`。不再用固定“初始配置”基准。**重置**会把 previous
      对齐到重置后的目标（delta 清零）；重置目标已等于当前状态时直接对齐，不留下残余对比值。
    - **炮候选全集**：火炮候选改为全车炮的全集（跨炮塔按 stem 去重）；当前炮塔不能直接
      装配（不在该塔 guns 内或无对应 config）的炮保留为**灰色不可选**，不再隐藏；悬停/
      aria 给出“当前炮塔无法直接装配”。其余模块候选不变。
    - **装备 tooltip 修正**：客户端 description 的占位符按当前车型/参数格式化——百分比
      取自 script 的 `percent`（displayParams 只给绝对值，104/105 会漏负号）、类别对象
      （光学/迷彩网/引擎加速）取当前坦克类别、补 `%`/`−`、不再出现 `[object Object]`/裸数字；
      108/109/112/120/121 另加“不进入参数表”说明。
    - **i18n 修正**：`crew_count`/`shell_range`/`shell_capacity`/`explosion_radius` 的
      键路径原指到 `agentTanks.*` 顶层（实际在 `agentTanks.configurator.*`），已改正；
      客户端基础 Strings 缺的 5 个标准物品名用运行时本地化覆盖层（zh-Hans）+
      en 补充名（Tungsten Rounds / Head of Cheese / Cinnamon Roll / Beef Goulash /
      Fried Cheese），ru 按既有约定回退 en。
    - **导出器**：`Extractor.extract_model` 复用为尺寸数据源（车辆级 chassis/hull 盒 +
      逐炮塔盒 + `turretOrigin`/`initialTurretRotation`），chassis stats 增
      `hysteresisHealth`/`hullPosition`。
      产物更新为 123 文件 3.51 MB（`meta.json` 仍 122 个 sha256 条目）。

21. **自动装填有效 DPM + 火炮转向速度（2026-10-27 用户要求）**：
    - **无炮塔标记**：客户端车辆 XML 的 `<hull><fakeTurrets>` 元素**存在** ⇔ 无炮塔
      （`hasTurret=false`）。该块不是数值阈值——738/738 辆与 BI 11.21 `hasTurret`
      逐辆一致（114 辆 false，含 Jagdtiger/SU-100/T95/M3 Grant/Mark I Male），
      故作为格式不变量导出到 `modules.hasTurret`。缺 `<hull>` 时留 null（不猜有/无；
      前端按旧数据保留炮塔行）。
    - **火炮转向速度**：客户端 gun XML 的 `<rotationSpeed>` 此前未导出；现在进
      `GunData.stats.rotationSpeed`。引擎 `gunTraverseSpeed = gun.rotationSpeed ×
      (1 + fuel) × (1 + progressive(gunner))`（BK 同式，不含“炮塔”技能
      close_combat_master）；**无炮塔车参数区用它替换 `turretRotationSpeed`**，
      有炮塔车不显示该行。Jagdtiger 26.25×1.042857=27.375°/s；T-34 39.375×1.1=43.313
      （有炮塔，仅内部值）。
    - **有效 DPM**：BK `dpmEffective` 的 60 s 窗口模型原样落在 `autoreloader` 上
      （装填手/输弹机系数作用于逐发装填、弹间装填加速 ×0.7、钨芯伤害同乘）；
      参数行 `gunEffectiveDamagePerMin` 只在有值时出现；逐发装填少于 2 发时 null。
      现有 autoreloader 例子（14/12/10 s、弹间 2.727 s、160 伤害）→ 887.35。
    - **产物**：123 文件 3.51 MB（`meta.json` 仍 122 个 sha256 条目）。**分发**：全部
      非图标数据（全局目录/物品/技能/params/strings/camouflage/meta + 逐车 modules）
      随资产包 `research/**` 发布（assetProvider 加载）；`common/research/` 仅保留全局
      图标集。后续新增坦克/物品只更新资产包。WotbTools PR #772（Android `wotbVersion`
      3.17.9）已落地。

22. **预览 XI 车逐车模块并入资产包（2026-10-27）**：
    - 资产包 `tank/`/GLB/`game_data`/悬挂/履带印已含 BZ-79（15153）与 Object 432U
      （27905），但配置器 `research/modules/{china,ussr}.json` 原先只覆盖 738 辆
      11.21 客户端车；本轮从 11.22 预览 ZIP 的 `Data/` 增量导出两辆并并入。
    - 导出器新增 `--game-data <Data 根>`、`--bridge <映射>`、`--modules-only`、
      `--merge`：只遍历 bridge 中出现的国别，读预览 XML/Parameters/Strings；
      `--merge` 保留既有国别文件全部车辆，只覆盖/新增本次导出的车。
      预览 bridge 为 `data/preview_tank_id_bridge.json`（15153 → china/`Ch67_BZ_79`、
      27905 → ussr/`R230_Object_432U`）。
    - 数据面：china 57 / ussr 105（合计 740）；每辆预览车带 `gameVersion`；
      `research/meta.json` 增 `preview` 清单（tankId/nation/gameVersion）与
      `previewGeneratedAt`，文件级 `meta.gameVersion` 仍是 11.21 主数据集。
      预览 ZIP 的部分新模块名在 `Strings/*.yaml.dvpl` 只有占位键（值等于键本身），
      导出器原样保留，前端回退 stem，不臆造名称。
    - 资产包 release：`t11-preview-20261027-research-t11`（在 `research-all` 上
      硬链接复制 + 3 件 research 增量；manifest 5911 件 / 4430.0 MB / 0 mismatch，
      `delta.json` 3 件；公网 sha256 与 `deploy/assets/verify.sh` 均通过）。

23. **配置器图标居中 + 英文参数栏修复（2026-10-27）**：
    - **图标**：`_normalize_icon` 用 `out.paste(resized, pos, resized)` 把 RGBA 图自身
      当 mask，边缘 alpha 被二次相乘（alpha²）后消失，内容框偏侧；102 张 96×96 图标中
      32 张四边距差 >1px（最差 11px）。改用 `alpha_composite`，重新生成 68 物品 +
      5 模块 + 20 技能 + 9 弹种图标（`data/research` 3.71 MB），同步 WotbTools
      `common/research/icons`；新增 `tools/test_research_icons.py` 几何门禁。
      技能图标改用 alpha>10 的**可见内容框**再居中（近似 BlitzKit 的可见效果；
      sharp().trim() 合并颜色与 alpha，不是同一算法）。
    - **i18n**：英文 `agentTanks.pen_far` 的 `"Pen @ far"` 中 `@` 是 vue-i18n
      linked-message 语法，切换 English 时编译抛 `SyntaxError`，右侧参数栏整块消失；
      改为 `"Far-range pen"`，并在 `messages.test.js` 中逐条编译三语全部消息。
    - **发布**：3.17.9 / 3.17.10 均已 stage 但未 publish（同候选中发现上述缺陷）；
      技能帧修复最终由 **3.17.11 / 3017011** 承载；图标与 i18n 都是 bundled
      frontend 变化，需重新 stage。
    - 视觉验收（图标观感/布局）仍由用户执行。


### 12.3 验证与验收

- 覆盖率：738/738 辆（11.21）+ 2/2 辆（11.22 预览 15153/27905）的
  `configs[].turret_local/gun_local` 全部命中模块 localId；740/740 的
  `optDevicePreset` 都在 9 个预设内；每辆车至少 1 件可用消耗品 / 1 件可用补给（157 辆弹夹/连发车
  按 `clip` 排除 berserk）。
- WotbTools targeted：`characteristics.test.ts`（32，含履带维修/尺寸/弹匣总伤/初始 pitch/无炮塔转向/有效 DPM 两分支）、`tankSetup.test.ts`（26，含弹种选择/pitchAtBearing/装甲系数/技能/装备计时/转向行切换）、`gunArc.test.ts`（8，径向映射/方位钳制/带路径）、
  `tankopediaQuery.test.js`（48）、`researchCatalog.test.js`（11，图标/技能/弹种覆盖 + vehicleFilter AND 语义 + isAbility 排除 + 槽位解析）、`researchCatalogModules.test.js`（2，资产包路径/缓存/失败重试）、
  `AgentTankopedia.test.js`（32，含配置器集成、弹种图标、技能单位/条件联动、维修/尺寸行、101/118 计时、模块矩阵对齐、炮候选置灰、重置清 delta/弹种、弹种明细调参、装备 tooltip 格式化、无炮塔转向行）、`locales/messages.test.js`（14，含参数键三语齐全与 delta 文案）、`navigation.test.js`（18，离页 query 清理）
  — 合计 183 全通过；全量 `vitest run` 由 WotbTools PR CI 执行（含 Docker 构建上下文契约：
  `docker/Dockerfile.frontend` 已 `COPY common/research /common/research`）；`pnpm run typecheck`、
  `pnpm run lint:css` 通过。本仓 Rust：`cargo test --lib wargaming::tank_configs` 5/5
  （新增逐配置口径 = 弹表弹径锚点）。
- **P4 回放装配徽标 + 科技树/研发界面（2026-10-27 同日落地，随用户指令解除范围裁定）**：
  前端（WotbTools `agent-techtree` 视图 + `TankResearchPanel` + `EquipmentBadges` +
  `ArmorLoadout`）：科技树按 `tech_tree/{nation}.json` 原始 x/y 网格渲染（band 不划分——
  §4.1 census 的「premium 带」解读与数据不符，见 docs/index.md 数据面条目），研发连线
  拓扑等价；单车研发图布局为确定性推导（列=最长入边深度、行=模块族），UI 标注
  「布局为复刻推导」；装甲查看器「实际搭载」区（百科 URL 交接 `dev/cons/prov`）与
  回放装备徽标（`shooter_equipment.raw`/`vehicles[].equipment` → devices.json 图标，
  未知 id 走通用未知文案）；车辆卡「在配置器中打开」= `?tank=&eq=` →
  `devicesFromEquipmentIds` 反解（歧义 fail-closed 留空）。配置器手机（<768px）模块
  矩阵折 2 列。§9.3 状态机维持「不做账号态」裁定：科技树/研发页只展示成本与当前搭载。
  数据面（本仓）：`--with-research-graph` 落地 + `tools/fetch_t11_techtree.py`
  （ZIP64+Range 部分提取，`--data-root` 物化最小预览根）+ 预览合并防回退门禁；
  `tools/test_research_techtree.py` 护栏（306 节点/6 pending/310 边/XI 事实）。
  视觉验收（科技树观感/连线密度、移动端排版、研发图布局）由用户执行。

### 12.4 已知偏差增量（并入 §7）

- 配置器固定 100% 熟练度；技能默认 0、可按 1–7 级切换，数值读客户端 `skills.json.effects`。
  WotbTools `common/crew-skills.json` 的「全部技能 7 级」仅回放/AI 投影口径，两者分域
  （`common/AGENTS.md` 已登记）。
- 物品/模块名 ru 回退 en（客户端 strings 只导出 en/zh-Hans）。
- 图标为客户端原图（图集裁剪）；`stereoscope` 无图时回退名称文本。
- BlitzInfo `tanks.json` 的固有隐蔽（`camo`/`camoM`）与本仓 ±0.3–0.5（其页面不显示；本仓取
  `camouflages.xml` 基础值，页面可见值以本仓为准）。
- 套件中 `Scene3DViews` / `main` 的个别用例在整仓并行跑时受环境时序影响（单文件复跑通过），
  与配置器计算无关。
- 交互射界图的阴影带使用当前配置的 `pitch_limits` 几何 + `improvedPitchLimitsSystem`
  的 +4°/+3°（与 BlitzInfo “stabiliser in the kit” 同语义）；实际显示以 2° 采样近似曲线。
- 乘员技能默认 0（中性基线）；BlitzInfo/BlitzKit 初始界面默认部分技能满级，直接比较初始值
  会有差异，选中 7 级后与 BI 的 Gun flexibility 系数一致（T-34：Moving 0.158、Shooting 3.255）。
- 动态扩圈五行是**无量纲系数**（BI “Gun flexibility” 的 +0.158 / +3.255 口径），不是米制
  散布圆半径；不乘基础散布。
- Repair 技能的静态履带维修时间已进表：基准 = 客户端 `hysteresisHealth / healthRegenPerSec`
  （BI 11.21 T-34/116-F3 均 6.86 s，已证），再乘工具箱/防护套件/技能加速；其余静态技能
  （smooth_* / Virtuoso / Camouflage）同口径。
