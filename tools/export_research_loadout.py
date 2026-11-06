#!/usr/bin/env python3
"""导出客户端研发/配置数据面（data/research/），供 WotbTools 配置器消费。

数据权威 = 本机客户端磁盘文件（与 tools/extract_vehicles.py 同源同语义，复用其
Names/Components/DefResolver/Extractor 设施）。fail-closed：缺文件/解析失败即报错，
不猜默认值。

用法：
    python tools/export_research_loadout.py [--out data/research] [--langs en,zh-Hans]
                                            [--with-research-graph]

输出（本期消费面）：
    meta.json                   game_version / generated_at / 逐文件 sha256 / 计数
    modules/{nation}.json       逐车模块数据（含 localId/tags/槽位数/预设名/共享组件；
                                hasTurret（<hull><fakeTurrets> 标记）；chassis stats 含
                                hysteresisHealth/hullPosition，gun stats 含 rotationSpeed；
                                车辆级 bbox chassis+hull、逐炮塔 bbox 与 turretOrigin 供尺寸计算）
    devices.json                24 件装备 + 9 预设（三语名/描述/script/display_params）
    consumables.json            消耗品（含 mode 标记与 vehicleFilter）
    provisions.json             补给（同构）
    consumables_default.json    客户端按 tier 默认消耗品表 + 单车覆盖
    params/structure.json       hangarParamsStructureConfig 分组树
    params/format.json          每参数 units/digits/biggerIsBetter
    strings/{lang}/research.json 参数名（tank_descr_param_*）+ 单位缩写
    camouflage.json             涂装隐蔽加成（camouflages.xml 的 <invisibility><categories>）
    icons/items/**              物品图标（客户端 UI 样式表 bg-sprite → 图集裁剪/整图，统一 96×96）
    icons/modules/*.webp        模块类型图标 5 张（ModulesTechTree，统一 96×96）

可选（--with-research-graph，科技树/研发界面消费面）：
    tech_tree/{nation}.json     nodes（stem/tankId/tier/x/y/price/tags；x=列 y=行，列≠tier）
                                + pending（科技树已登记但未实装的 XI 展示位，仅坐标）
                                + edges（`<unlocks><vehicle>` 整车研发边，from/to/cost/
                                costRaw/sourceModule/sourceKind）
    --tech-tree-extra           XI 预览增量（tools/fetch_t11_techtree.py 产物）：
                                合并 11.22 超测树的 XI 节点（BZ-79 / Object 432U 为
                                preview 节点，其余 6 个登记位进 pending）
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import sys
from datetime import datetime, timezone

import xml.etree.ElementTree as ET

TOOLS_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = TOOLS_DIR.parent
sys.path.insert(0, str(TOOLS_DIR))
sys.path.insert(0, str(TOOLS_DIR / "wotbtools"))

from extract_vehicles import (  # noqa: E402
    Components,
    DefResolver,
    Extractor,
    Names,
    default_game_data,
    last_child,
    parse_pitch,
    parse_vec_text,
)
from wotb_sc2 import decode_dvpl  # noqa: E402

NATIONS = ["ussr", "germany", "usa", "china", "france", "uk", "japan", "other", "european"]
PRESET_GROUPS = ("attack", "defence", "specialization")
PRESET_LEVELS = ("level0", "level1", "level2")

# 客户端基础 Strings 缺后期物品名（运行时本地化覆盖层有 zh-Hans，但 en 无覆盖层）：
# 仅补标准可装配物品的 en 显示名；zh-Hans 由覆盖层解析，ru 按既有约定回退 en。
# 来源：客户端 battle 文案（tungsten_tip_in_use = "Tungsten Rounds"）与官方物品名。
ITEM_NAME_SUPPLEMENT = {
    "#artefacts:tungstentip/name": {"en": "Tungsten Rounds"},
    "provisions/headOfCheese/Name": {"en": "Head of Cheese"},
    "provisions/cinnamonRoll/Name": {"en": "Cinnamon Roll"},
    "provisions/ration_czech_hq/Name": {"en": "Beef Goulash"},
    "provisions/ration_czech_regular/Name": {"en": "Fried Cheese"},
}

# 乘员技能（BlitzKit crew-skills.json 的 20 个车型技能；训练资格按车型，效果全局生效）。
SKILL_IDS = (
    "lucky_shot", "surrounded_by_enemy", "soft_recoil", "skilled_ranging", "fast_capture",
    "battle_rage", "armour_piercing", "tutor", "smooth_driving", "smooth_turret",
    "skilled_repairer", "close_combat_master", "fire_fighting", "desperado", "repair",
    "sniper", "deadly_accuracy", "camouflage", "virtuoso", "smooth_turn",
)
# 弹种图标（AmmoSlot.yaml 的 iconPath 映射；normal 大图）。
SHELL_TYPES = (
    "ap", "ap_premium", "ap_cr", "ap_cr_premium", "he",
    "he_premium", "hc", "hc_premium", "atgm_heat",
)


def load_dvpl_xml(path: pathlib.Path) -> ET.Element:
    return ET.fromstring(decode_dvpl(path.read_bytes()))


def load_dvpl_xml_bytes(path: pathlib.Path) -> tuple[bytes, ET.Element]:
    """解码后的 XML 字节 + 根元素：provenance 的 sha256 用**解码后**内容
    （与 WotbTools fixture 的 `upgradeProgression.provenance.sha256` 同口径）。"""
    raw = decode_dvpl(path.read_bytes())
    return raw, ET.fromstring(raw)


def ftext(el: ET.Element | None, tag: str) -> str | None:
    if el is None:
        return None
    v = el.findtext(tag)
    return v.strip() if v and v.strip() else None


def ffloat(el: ET.Element | None, tag: str) -> float | None:
    v = ftext(el, tag)
    if v is None:
        return None
    try:
        return float(v)
    except ValueError:
        return None


def fint(el: ET.Element | None, tag: str) -> int | None:
    v = ftext(el, tag)
    if v is None:
        return None
    try:
        return int(float(v))
    except ValueError:
        return None


def parse_floats(s: str | None) -> list[float] | None:
    if not s:
        return None
    parts = s.replace(",", " ").split()
    try:
        return [float(x) for x in parts]
    except ValueError:
        return None


def parse_dispersion_factors(el: ET.Element, shared: ET.Element | None) -> dict | None:
    """炮/底盘 `<shotDispersionFactors>` 子项（turretRotation/afterShot/vehicleMovement/...）。"""
    node = el.find("shotDispersionFactors")
    if node is None and shared is not None:
        node = shared.find("shotDispersionFactors")
    if node is None:
        return None
    out = {}
    for child in node:
        if not isinstance(child.tag, str):
            continue
        try:
            out[child.tag] = float((child.text or "").strip())
        except ValueError:
            continue
    return out or None


def parse_cost(raw: str | None) -> dict:
    raw = (raw or "").strip()
    if not raw:
        return {"xp": 0}
    if ":" in raw:
        cur, amt = raw.split(":", 1)
        try:
            return {"tokens": {"currency": cur, "amount": int(amt)}}
        except ValueError:
            return {"raw": raw}
    if raw.isdigit():
        return {"xp": int(raw)}
    return {"raw": raw}


# XI 升级节点的 nodeType 是 WotbTools `resolveUpgradeSelection` 的封闭枚举；
# 出现未知值时 fail-closed（不导出一个消费方无法解释的节点）。
UPGRADE_NODE_TYPES = (
    "NODETYPE_COMMON",
    "NODETYPE_MECHANIC",
    "NODETYPE_ALTERNATIVE_MECHANIC",
)


def parse_upgrade_attribute_value(raw: str | None) -> bool | int | float:
    """升级节点 `<value>`：布尔 / 整数 / 浮点。非数值文本直接报错（不猜）。"""
    text = (raw or "").strip()
    if text.lower() == "true":
        return True
    if text.lower() == "false":
        return False
    try:
        return int(text)
    except ValueError:
        pass
    try:
        return float(text)
    except ValueError as exc:
        raise ValueError(f"upgrade attribute value is not numeric/boolean: {raw!r}") from exc


def parse_upgrade_attributes(root: ET.Element) -> dict[str, dict]:
    """`vehicles/common/tank_upgrade_attributes.xml` → attributeName → 定义。

    保留客户端的 modifier / script（内联 `<params>` 展开为键值）/ icon / title；
    WotbTools 的 `attributeDefinitions` 契约正是这四类字段。
    """
    out: dict[str, dict] = {}
    for el in root.findall("./attributes/attribute"):
        name = ftext(el, "attributeName")
        modifier = ftext(el, "modifier")
        if not name or not modifier:
            raise ValueError("upgrade attribute without attributeName/modifier (fail-closed)")
        entry: dict = {"modifier": modifier}
        script_el = el.find("script")
        if script_el is not None:
            script = (script_el.text or "").strip()
            if script:
                entry["script"] = script
            params_el = script_el.find("params")
            if params_el is not None:
                params = {
                    child.tag: (child.text or "").strip()
                    for child in params_el
                    if isinstance(child.tag, str)
                }
                if params:
                    entry["params"] = params
        icon = ftext(el, "icon")
        title = ftext(el, "title")
        if icon:
            entry["icon"] = icon
        if title:
            entry["title"] = title
        out[name] = entry
    return out


def parse_upgrade_progression(
    el: ET.Element,
    attribute_definitions: dict[str, dict],
    game_version: str,
    source: str,
    source_sha256: str,
    attributes_source: str,
    attributes_sha256: str,
) -> dict:
    """车辆 XML `<upgradeProgression>` → WotbTools `UpgradeProgression` 形状。

    节点按 id 唯一；`alternativeMechanicGroups` 的 `choices` 是
    `<alternativeNodes>` 里 techName 对号后的 id（客户端只给 techName）。
    """
    nodes: list[dict] = []
    ids: set[int] = set()
    by_tech: dict[str, int] = {}
    for node_el in el.findall("./nodes/node"):
        node_id = fint(node_el, "id")
        tech_name = ftext(node_el, "techName")
        node_type = ftext(node_el, "nodeType")
        if node_id is None or not tech_name or node_type not in UPGRADE_NODE_TYPES:
            raise ValueError(
                f"invalid upgrade node (id={node_id!r}, techName={tech_name!r}, type={node_type!r})"
            )
        if node_id in ids:
            raise ValueError(f"duplicate upgrade node id {node_id}")
        if tech_name in by_tech:
            raise ValueError(f"duplicate upgrade node techName {tech_name}")
        ids.add(node_id)
        by_tech[tech_name] = node_id
        attributes = []
        for attr_el in node_el.findall("./attributes/attribute"):
            name = ftext(attr_el, "attributeName")
            if not name:
                raise ValueError(f"upgrade node {node_id} has an attribute without name")
            if name not in attribute_definitions:
                raise ValueError(f"upgrade attribute {name} has no definition (fail-closed)")
            attributes.append({
                "name": name,
                "value": parse_upgrade_attribute_value(ftext(attr_el, "value")),
            })
        node = {
            "id": node_id,
            "techName": tech_name,
            "type": node_type,
            "attributes": attributes,
        }
        # 节点级展示元数据：客户端只在部分节点（多为 alternative 选项）给出；
        # 有则保留（与 WotbTools fixture 同形状），没有就不写字段。
        for key in ("displayName", "description", "icon"):
            value = ftext(node_el, key)
            if value:
                node[key] = value
        nodes.append(node)

    groups = []
    grouped_nodes: set[int] = set()
    group_ids: set[str] = set()
    for group_el in el.findall("./alternativeMechanicGroups/alternativeMechanicGroup"):
        group_id = ftext(group_el, "techName")
        if not group_id or group_id in group_ids:
            raise ValueError(f"invalid/duplicate upgrade group id {group_id!r}")
        group_ids.add(group_id)
        choices = []
        for alt_el in group_el.findall("./alternativeNodes/node"):
            tech_name = (alt_el.text or "").strip()
            if tech_name not in by_tech:
                raise ValueError(f"upgrade group {group_id} references unknown node {tech_name!r}")
            node_id = by_tech[tech_name]
            if node_id in grouped_nodes:
                raise ValueError(f"upgrade node {node_id} assigned to multiple groups")
            grouped_nodes.add(node_id)
            choices.append(node_id)
        if not choices:
            raise ValueError(f"upgrade group {group_id} has no alternative nodes")
        group = {"id": group_id, "choices": choices}
        for key in ("icon", "displayName", "description"):
            value = ftext(group_el, key)
            if value:
                group[key] = value
        groups.append(group)

    for node in nodes:
        if node["type"] == "NODETYPE_ALTERNATIVE_MECHANIC" and node["id"] not in grouped_nodes:
            raise ValueError(f"ungrouped alternative upgrade node {node['techName']}")

    # 只带本车节点实际引用的定义（与 WotbTools fixture 同口径）：属性表是全局的
    # 98 条，逐车全量复制会无谓放大模块 JSON；消费方只按节点属性名查表。
    used = {attr["name"] for node in nodes for attr in node["attributes"]}
    used_definitions = {name: attribute_definitions[name] for name in sorted(used)}

    return {
        "nodes": nodes,
        "alternativeGroups": groups,
        "attributeDefinitions": used_definitions,
        "provenance": {
            "gameVersion": game_version,
            "source": source,
            "sha256": source_sha256,
            "attributesSource": attributes_source,
            "attributesSha256": attributes_sha256,
        },
    }


def upgrade_top_hp(tank: dict, progression: dict) -> int | None:
    """顶级配置总 HP = 车体 HP + 顶炮塔 HP + 所有 MAX_HEALTH 升级节点。

    顶级选择语义与 WotbTools `resolveUpgradeSelection(mode='top')` 一致：
    所有非 alternative 节点 + 每个 alternative group 的首个 choice。
    """
    hull = tank.get("hull") or {}
    turrets = tank.get("turrets") or []
    if hull.get("hp") is None or not turrets:
        return None
    turret_hp = (turrets[-1].get("stats") or {}).get("maxHealth")
    if turret_hp is None:
        return None
    selected = {
        node["id"] for node in progression["nodes"]
        if node["type"] != "NODETYPE_ALTERNATIVE_MECHANIC"
    }
    for group in progression["alternativeGroups"]:
        selected.add(group["choices"][0])
    bonus = 0.0
    for node in progression["nodes"]:
        if node["id"] not in selected:
            continue
        for attr in node["attributes"]:
            if attr["name"] == "ATTRIBUTE_NAME_MAX_HEALTH":
                bonus += float(attr["value"])
    return int(round(float(hull["hp"]) + float(turret_hp) + bonus))


def module_common_fields(el: ET.Element, shared: ET.Element | None, resolver_local_id: int | None,
                         names_by_lang: dict, tables_by_lang: dict, nation: str) -> dict:
    """模块公共字段：stem/localId/名称/等级/价格 + unlocks（last-wins 解析由 ElementTree 天然保证）。"""
    user_string = ftext(el, "userString") or (ftext(shared, "userString") if shared is not None else None)
    out = {
        "stem": el.tag,
        "localId": resolver_local_id,
        "level": fint(el, "level") if fint(el, "level") is not None else (fint(shared, "level") if shared is not None else None),
        "price": ffloat(el, "price") if ffloat(el, "price") is not None else (ffloat(shared, "price") if shared is not None else None),
    }
    for lang, names in names_by_lang.items():
        resolved = names.display(nation, user_string)
        if resolved is None and user_string:
            resolved = flat_lookup(tables_by_lang[lang], user_string)
        out[f"name_{lang}"] = resolved
    unlocks = []
    u = el.find("unlocks")
    if u is not None:
        for target in u:
            if not isinstance(target.tag, str) or target.tag == "cost":
                continue
            unlocks.append({
                "kind": target.tag,
                "stem": (target.text or "").strip(),
                "cost": parse_cost(target.findtext("cost")),
            })
    out["unlocks"] = unlocks
    return out


def flat_lookup(table: dict, key: str | None) -> str | None:
    """装备/物品键：Strings 表里 `#段:键`（含 # 原文）与平键并存，按原文→去#→段后序贯查。"""
    if not key:
        return None
    for k in (key, key[1:] if key.startswith("#") else None):
        if k and k in table:
            return table[k]
    k2 = key[1:] if key.startswith("#") else key
    if ":" in k2:
        v = table.get(k2.split(":", 1)[1])
        if v is not None:
            return v
    return None


def load_localization_overlay(lang: str) -> dict:
    """运行时本地化覆盖层（客户端语言缓存）：基础 Strings 缺的后期物品名在这里。

    只读本机缓存；缺失/不可解析返回空表（fail-closed，不影响其余导出）。键是原文
    （含 `#artefacts:...` / `provisions/...`），与 flat_lookup 的查找口径一致。
    """
    for d in Names.OVERLAY_DIRS:
        p = pathlib.Path(os.path.expandvars(os.path.expanduser(d))) / f"{lang}.yaml"
        if p.exists():
            try:
                import yaml as _yaml
                data = _yaml.safe_load(p.read_text(encoding="utf-8", errors="replace"))
                return data if isinstance(data, dict) else {}
            except Exception:  # noqa: BLE001
                return {}
    return {}


def export_module_kind(root: ET.Element, kind: str, comp: Components,
                       names_by_lang: dict, tables_by_lang: dict, nation: str,
                       ids_map: dict[str, int] | None = None,
                       shared_map: dict[str, ET.Element] | None = None) -> list[dict]:
    """内联（chassis/turrets）或共享引用（engines/radios/fuelTanks）的模块列表。

    radios/fuelTanks 不在 Components 覆盖内（提取链的 pb 不需要），由 ids_map/shared_map
    传入；其 userString 在共享定义上，名称解析优先 flat_lookup。"""
    out = []
    container = last_child(root, kind if kind != "turrets" else "turrets0")
    if container is None:
        return out
    for entry in container:
        if not isinstance(entry.tag, str):
            continue
        shared = comp.shared_def(kind, entry.tag) if shared_map is None else shared_map.get(entry.tag)
        local = comp.local_id(kind, entry.tag) if ids_map is None else ids_map.get(entry.tag)
        m = module_common_fields(entry, shared, local, names_by_lang, tables_by_lang, nation)
        if kind == "chassis":
            m["stats"] = {
                "rotationSpeed": ffloat(entry, "rotationSpeed") or ffloat(shared, "rotationSpeed"),
                "terrainResistance": parse_floats(ftext(entry, "terrainResistance") or (ftext(shared, "terrainResistance") if shared is not None else None)),
                "maxLoad": ffloat(entry, "maxLoad") or ffloat(shared, "maxLoad"),
                "weight": ffloat(entry, "weight") or ffloat(shared, "weight"),
                "maxHealth": ffloat(entry, "maxHealth") or ffloat(shared, "maxHealth"),
                "bulkHealthFactor": ffloat(entry, "bulkHealthFactor") or ffloat(shared, "bulkHealthFactor"),
                # 履带耐久：客户端 hangarParams 的 chassisRepairTime = hysteresisHealth /
                # healthRegenPerSec（BI 11.21 同值：T-34 24/3.5 = 6.86 s、116-F3 66/9.625）。
                "hysteresisHealth": ffloat(entry, "hysteresisHealth") or ffloat(shared, "hysteresisHealth"),
                "healthRegenPerSec": ffloat(entry, "healthRegenPerSec") or ffloat(shared, "healthRegenPerSec"),
                # 车体在底盘上的挂点（车辆 XML <hullPosition>，顺序 [x, 上, 前]；
                # 尺寸并集换算为 [x, 前, 上] 见前端 compute）。
                "hullPosition": parse_vec_text(ftext(entry, "hullPosition") or (ftext(shared, "hullPosition") if shared is not None else None)),
                # 移动/车体转向扩圈系数（客户端 chassis <shotDispersionFactors>）
                "dispersionFactors": parse_dispersion_factors(entry, shared),
            }
        elif kind == "turrets":
            m["stats"] = {
                "rotationSpeed": ffloat(entry, "rotationSpeed") or ffloat(shared, "rotationSpeed"),
                "circularVisionRadius": ffloat(entry, "circularVisionRadius") or ffloat(shared, "circularVisionRadius"),
                "maxHealth": ffloat(entry, "maxHealth") or ffloat(shared, "maxHealth"),
                "weight": ffloat(entry, "weight") or ffloat(shared, "weight"),
            }
            yaw = parse_floats(ftext(entry, "yawLimits") or (ftext(shared, "yawLimits") if shared is not None else None))
            m["stats"]["yawLimits"] = yaw if yaw and len(yaw) == 2 else None
            # 每个炮塔自己的主装甲板（front sides rear，如 armor_1 armor_3 armor_4）：
            # 装甲表的 front/side/rear 随所选炮塔变，不能复用顶层 armor_model（顶级炮塔）映射。
            primary = ftext(entry, "primaryArmor") or (ftext(shared, "primaryArmor") if shared is not None else None)
            if primary:
                parts = primary.split()
                if len(parts) >= 3:
                    m["primaryArmor"] = {"front": parts[0], "sides": parts[1], "rear": parts[2]}
            guns = []
            guns_el = entry.find("guns")
            if guns_el is not None:
                for g in guns_el:
                    if not isinstance(g.tag, str):
                        continue
                    gshared = comp.shared_def("guns", g.tag)
                    glocal = comp.local_id("guns", g.tag)
                    gm = module_common_fields(g, gshared, glocal, names_by_lang, tables_by_lang, nation)
                    gm["stats"] = {
                        "reloadTime": ffloat(g, "reloadTime") or ffloat(gshared, "reloadTime"),
                        "aimingTime": ffloat(g, "aimingTime") or ffloat(gshared, "aimingTime"),
                        "shotDispersionRadius": ffloat(g, "shotDispersionRadius") or ffloat(gshared, "shotDispersionRadius"),
                        "maxAmmo": ffloat(g, "maxAmmo") or ffloat(gshared, "maxAmmo"),
                        "weight": ffloat(gshared, "weight") if gshared is not None else ffloat(g, "weight"),
                        "invisibilityFactorAtShot": ffloat(g, "invisibilityFactorAtShot") or ffloat(gshared, "invisibilityFactorAtShot"),
                        # 火炮转向速度（无炮塔车的“炮塔转速”替代值）：客户端 gun XML
                        # <rotationSpeed>，与炮塔 <rotationSpeed> 同单位/同系数（BK gunTraverseSpeed）。
                        "rotationSpeed": ffloat(g, "rotationSpeed") or ffloat(gshared, "rotationSpeed"),
                        # 炮塔转向/射击后/炮管损坏扩圈系数（客户端 gun <shotDispersionFactors>）
                        "dispersionFactors": parse_dispersion_factors(g, gshared),
                    }
                    # pitchLimits 是元素型（<min>/<max>[+front/back/transition]，重复标签
                    # last-wins）——复用提取链 parse_pitch 的权威解析，不自行猜文本格式。
                    gm["stats"]["pitchLimits"] = parse_pitch(DefResolver(g, gshared))
                    guns.append(gm)
            m["guns"] = guns
        elif kind == "engines":
            m["stats"] = {
                "power": ffloat(entry, "power") or ffloat(shared, "power"),
                "fireStartingChance": ffloat(entry, "fireStartingChance") or ffloat(shared, "fireStartingChance"),
                "weight": ffloat(entry, "weight") or ffloat(shared, "weight"),
                "tags": (ftext(entry, "tags") or (ftext(shared, "tags") if shared is not None else None) or "").split(),
            }
        elif kind == "radios":
            m["stats"] = {
                "distance": ffloat(entry, "distance") or ffloat(shared, "distance"),
                "weight": ffloat(entry, "weight") or ffloat(shared, "weight"),
            }
        elif kind == "fuelTanks":
            m["stats"] = {"weight": ffloat(entry, "weight") or ffloat(shared, "weight")}
        out.append(m)
    return out


def parse_filter(el: ET.Element | None) -> dict | None:
    """vehicleFilter：一个 <include>/<exclude> 内的全部条件是 **AND**；多个块之间 OR。

    客户端示例（ration）：`<include><vehicle><minLevel>6</minLevel>…</vehicle><nations>ussr</nations></include>`
    = 6-10 级 **且** 苏联车。逐条件拆成独立规则会把 AND 变成 OR（苏系食物对所有国家出现）。
    """
    if el is None:
        return None
    out = {}

    def merge(rule: dict, key: str, values: list[str]) -> None:
        rule.setdefault(key, []).extend(values)

    for side in ("include", "exclude"):
        rules = []
        for group in el.findall(side):
            rule: dict = {}
            for child in group:
                if not isinstance(child.tag, str):
                    continue
                min_level = fint(child, "minLevel")
                max_level = fint(child, "maxLevel")
                if min_level is not None or max_level is not None:
                    rule["tiers"] = {
                        "min": min_level if min_level is not None else 1,
                        "max": max_level if max_level is not None else 10,
                    }
                names = ftext(child, "name")
                if names:
                    merge(rule, "ids", names.split())
                if child.tag == "nations":
                    nations = (child.text or "").strip()
                else:
                    nations = ftext(child, "nations")
                if nations:
                    merge(rule, "nations", nations.split())
                if child.tag == "extendedTags":
                    tags = (child.text or "").strip()
                else:
                    tags = ftext(child, "extendedTags")
                if tags:
                    merge(rule, "extendedTags", tags.split())
            if rule:
                rules.append(rule)
        if rules:
            out[side] = rules
    return out or None


def parse_item_script(el: ET.Element) -> dict:
    s = el.find("script")
    if s is None:
        return {"type": None, "params": {}}
    params: dict = {}
    for c in s:
        if not isinstance(c.tag, str):
            continue
        if len(c):
            sub = {}
            for g in c:
                if not isinstance(g.tag, str):
                    continue
                sub[g.tag] = (g.text or "").strip() if not len(g) else {
                    gg.tag: (gg.text or "").strip() for gg in g if isinstance(gg.tag, str)}
            params[c.tag] = sub
        else:
            params[c.tag] = (c.text or "").strip()
    return {"type": s.tag, "params": params}


def parse_display_params(el: ET.Element) -> dict | None:
    d = el.find("display_params")
    if d is None:
        return None
    out = {}
    for c in d:
        if not isinstance(c.tag, str):
            continue
        if len(c):
            out[c.tag] = {g.tag: (g.text or "").strip() for g in c if isinstance(g.tag, str)}
        else:
            out[c.tag] = (c.text or "").strip()
    return out


def load_component(comp_dir: pathlib.Path, kind: str) -> tuple[dict[str, int], dict[str, ET.Element]]:
    """Components 未覆盖 radios/fuelTanks（提取链的 pb 不需要）；导出器自加载。"""
    ids: dict[str, int] = {}
    shared: dict[str, ET.Element] = {}
    p = comp_dir / f"{kind}.xml.dvpl"
    if not p.exists():
        return ids, shared
    root = load_dvpl_xml(p)
    ids_el = last_child(root, "ids")
    if ids_el is not None:
        import re
        for ch in ids_el:
            m = re.fullmatch(r"\d+", (ch.text or "").strip())
            if m is not None:
                ids[ch.tag] = int(m.group(0))
    shared_el = last_child(root, "shared")
    if shared_el is not None:
        for ch in shared_el:
            shared[ch.tag] = ch
    return ids, shared


def export_items(game_data: pathlib.Path, kind: str, tables_by_lang: dict[str, dict],
                 *, include_vehicle_ability: bool = False) -> list[dict]:
    list_path = game_data / f"XML/item_defs/vehicles/common/{kind}/list.xml.dvpl"
    root = load_dvpl_xml(list_path)
    items = []
    for ref in root.findall("items"):
        mode = pathlib.PurePosixPath(ref.get("path")).stem  # common / madgames / ...
        item_root = load_dvpl_xml(game_data / f"XML/item_defs/vehicles/common/{kind}" / (ref.get("path") + ".dvpl"))
        for el in item_root:
            if not isinstance(el.tag, str) or fint(el, "id") is None:
                continue
            user = ftext(el, "userString")
            descr = ftext(el, "description")
            item = {
                "id": fint(el, "id"),
                "stem": el.tag,
                "mode": mode,
                "icon": ftext(el, "icon"),
                "category": ftext(el, "category"),
                "tags": (ftext(el, "tags") or "").split(),
                "extraEffectDuration": ffloat(el, "extraEffectDuration"),
                "script": parse_item_script(el),
                "displayParams": parse_display_params(el),
                "filter": parse_filter(el.find("vehicleFilter")),
                # 战斗内技能（isAbility / XI 的 isVehicleAbility）：不是机库装配项
                # （如 desertPower 还有 geometryFilter；XI 的 steadyAimSystem 等由
                # upgradeProgression 节点表达），配置器物品池必须排除；语义标记来自
                # 客户端 XML，不按名称/缺少文案判定。
                "isAbility": el.find("isAbility") is not None
                or el.find("isVehicleAbility") is not None,
            }
            if include_vehicle_ability:
                item["isVehicleAbility"] = el.find("isVehicleAbility") is not None
            for lang, table in tables_by_lang.items():
                item[f"name_{lang}"] = flat_lookup(table, user)
                item[f"description_{lang}"] = flat_lookup(table, descr)
                if item[f"name_{lang}"] is None and user in ITEM_NAME_SUPPLEMENT:
                    item[f"name_{lang}"] = ITEM_NAME_SUPPLEMENT[user].get(lang)
            items.append(item)
    items.sort(key=lambda x: (x["mode"] != "common", x["id"]))
    return items


def vehicle_ability_stems(game_data: pathlib.Path,
                          tables_by_lang: dict[str, dict]) -> set[str]:
    """客户端 item 目录中带 `<isVehicleAbility>` 的 stem；逐车 `<extras>` 只保留这些。"""
    return {
        item["stem"]
        for kind in ("consumables", "provisions")
        for item in export_items(game_data, kind, tables_by_lang,
                                 include_vehicle_ability=True)
        if item.get("isVehicleAbility")
    }


def write_item_catalogs(game_data: pathlib.Path, out_root: pathlib.Path, version: str,
                        tables_by_lang: dict[str, dict], generated_at: str) -> None:
    """预览客户端增量：刷新全局 consumables/provisions（含 tier 11 过滤器与 XI 能力条目）。

    只改全局物品目录，不重导车辆模块；`meta.json` 的计数与逐文件 sha 同步刷新。
    """
    import yaml as _y

    consumables = export_items(game_data, "consumables", tables_by_lang)
    provisions = export_items(game_data, "provisions", tables_by_lang)
    catalog = {
        "consumables.json": {"meta": {"gameVersion": version, "count": len(consumables)},
                             "items": consumables},
        "provisions.json": {"meta": {"gameVersion": version, "count": len(provisions)},
                            "items": provisions},
    }
    default_path = game_data / "XML/item_defs/vehicles/common/consumablesDefault.yaml.dvpl"
    if default_path.exists():
        raw = _y.safe_load(decode_dvpl(default_path.read_bytes()).decode("utf-8")) or {}
        catalog["consumables_default.json"] = {
            "meta": {"gameVersion": version},
            "byTier": {k: v for k, v in raw.items() if str(k).isdigit()},
            "overrides": {k: v for k, v in raw.items() if not str(k).isdigit()},
        }
    out_root.mkdir(parents=True, exist_ok=True)
    for name, payload in catalog.items():
        (out_root / name).write_text(
            json.dumps(payload, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    meta_path = out_root / "meta.json"
    if meta_path.exists():
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        counts = meta.setdefault("counts", {})
        counts["consumables"] = len(consumables)
        counts["provisions"] = len(provisions)
        counts["consumablesCommon"] = sum(1 for x in consumables if x["mode"] == "common")
        counts["provisionsCommon"] = sum(1 for x in provisions if x["mode"] == "common")
        files = meta.setdefault("files", {})
        for name in catalog:
            data = (out_root / name).read_bytes()
            files[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
        meta["itemsGeneratedAt"] = generated_at
        meta_path.write_text(json.dumps(meta, ensure_ascii=False, separators=(",", ":")),
                             encoding="utf-8")
    print(f"items: consumables={len(consumables)} provisions={len(provisions)} "
          f"(version {version})")


def export_devices(game_data: pathlib.Path, names_by_lang: dict[str, Names],
                   tables_by_lang: dict[str, dict]) -> dict:
    root = load_dvpl_xml(game_data / "XML/item_defs/vehicles/common/optional_devices.xml.dvpl")
    devices = []
    for el in root:
        if not isinstance(el.tag, str) or fint(el, "id") is None:
            continue
        user = ftext(el, "userString")
        descr = ftext(el, "description")
        d = {
            "id": fint(el, "id"),
            "stem": el.tag,
            "icon": ftext(el, "icon"),
            "script": parse_item_script(el),
            "displayParams": parse_display_params(el),
        }
        for lang, table in tables_by_lang.items():
            d[f"name_{lang}"] = flat_lookup(table, user)
            d[f"description_{lang}"] = flat_lookup(table, descr)
        devices.append(d)
    devices.sort(key=lambda x: x["id"])

    slots_root = load_dvpl_xml(game_data / "XML/item_defs/vehicles/common/optional_device_slots.xml.dvpl")
    presets = {}
    for pr in slots_root.find("presets") or []:
        if not isinstance(pr.tag, str):
            continue
        slots = []
        for level in PRESET_LEVELS:
            level_el = pr.find(level)
            if level_el is None:
                continue
            for group in PRESET_GROUPS:
                g = level_el.find(group)
                if g is None:
                    continue
                left = (g.findtext("device0") or "").strip()
                right = (g.findtext("device1") or "").strip()
                slots.append({
                    "level": int(level[-1]),
                    "group": group,
                    "left": left or None,
                    "right": right or None,
                })
        presets[pr.tag] = slots
    return {"devices": devices, "presets": presets}


def export_params(game_data: pathlib.Path) -> tuple[dict, dict]:
    import yaml
    structure_raw = yaml.safe_load(decode_dvpl(
        (game_data / "hangarParamsStructureConfig.yaml.dvpl").read_bytes()).decode("utf-8"))
    format_raw = yaml.safe_load(decode_dvpl(
        (game_data / "hangarParamsConfig.yaml.dvpl").read_bytes()).decode("utf-8"))["keyedArchive"]

    def unwrap(node):
        if isinstance(node, dict) and set(node) == {"keyedArchive"}:
            return {k: unwrap(v) for k, v in node["keyedArchive"].items()}
        if isinstance(node, dict):
            return {k: unwrap(v) for k, v in node.items()}
        if isinstance(node, list):
            return [unwrap(x) for x in node]
        return node

    structure = unwrap(structure_raw)
    fmt = {}
    for key, node in (format_raw or {}).items():
        inner = unwrap(node.get("keyedArchive", {})) if isinstance(node, dict) else {}
        # 预览客户端的数值参数多一层 `numeric.keyedArchive`（11.21 无此层）；不剥掉
        # 会让 units/digits/biggerIsBetter 全部读空（42 个模块耐久参数曾因此只有空条目）。
        if isinstance(inner, dict) and isinstance(inner.get("numeric"), dict):
            inner = unwrap(inner["numeric"])
        entry = {}
        if "units" in inner and isinstance(inner["units"], dict) and "string" in inner["units"]:
            entry["units"] = inner["units"]["string"]
        if "digits" in inner and isinstance(inner["digits"], dict):
            entry["digits"] = inner["digits"].get("int32")
        if "biggerIsBetter" in inner and isinstance(inner["biggerIsBetter"], dict):
            entry["biggerIsBetter"] = inner["biggerIsBetter"].get("bool", True)
        fmt[key] = entry
    return {"types": structure.get("types"), "mainSections": structure.get("mainSections"),
            "sections": structure.get("sections")}, fmt


def export_strings(game_data: pathlib.Path, langs: list[str], param_keys: list[str]) -> dict:
    out = {}
    for lang in langs:
        text = decode_dvpl((game_data / f"Strings/{lang}.yaml.dvpl").read_bytes()).decode("utf-8")
        import yaml
        table = yaml.safe_load(text) or {}
        entries = {}
        for key in param_keys:
            k = f"tank_descr_param_{key}"
            if k in table:
                entries[key] = table[k]
        for uk in [k for k in table if k.startswith("unit_measure_short_")]:
            entries[uk] = table[uk]
        out[lang] = entries
    return out


def export_camouflage(game_data: pathlib.Path) -> dict:
    """涂装隐蔽加成（客户端 camouflages.xml）：按涂装类别的逐车型系数。

    权威 = `XML/item_defs/vehicles/common/camouflages.xml` 的
    `<invisibility><categories>`；配置器的“涂装”开关用 `free` 类别（标准免费涂装，
    与 disposable/common-camo-credits 等常规涂装同值）。
    """
    root = load_dvpl_xml(game_data / "XML/item_defs/vehicles/common/camouflages.xml.dvpl")
    container = root.find("invisibility/categories")
    if container is None:
        raise RuntimeError("camouflages.xml 缺少 <invisibility><categories>")
    categories: dict[str, dict[str, float]] = {}
    for category in container:
        if not isinstance(category.tag, str):
            continue
        values = {}
        for child in category:
            if not isinstance(child.tag, str):
                continue
            raw = (child.text or "").strip()
            if not raw:
                continue
            values[child.tag] = float(raw)
        if values:
            categories[category.tag] = values
    if not categories:
        raise RuntimeError("camouflages.xml 的涂装类别为空")
    return {"defaultCategory": "free", "categories": categories}


# 图标来源（权威 = 客户端 UI 样式表的 bg-sprite 映射，不按文件名猜）：
#   - 装备/消耗品/补给：InventoryBigStyles（inventory 面板）+ 事件面板样式
#   - 装备：OptionalDevicesItemImage.style（装备页专用，含新版 supply 树图标）
#   - Normal 档兜底：少数物品只在 Normal 样式登记（如 desertPower）
ICON_STYLE_MAPS = (
    "UI/Styles/Lobby/Inventory/InventoryBigStyles.yaml.dvpl",
    "UI/Styles/Lobby/Inventory/Event/InventoryBigStyles.yaml.dvpl",
    "UI/Screens3/Lobby/Inventory/OptionalDevices/OptionalDevicesItemImage.style.yaml.dvpl",
    "UI/Styles/Lobby/Inventory/InventoryNormalStyles.yaml.dvpl",
    "UI/Styles/Lobby/Inventory/Event/InventoryNormalStyles.yaml.dvpl",
)
ICON_KINDS = (
    ("devices", "OptionalDevices"),
    ("consumables", "Consumables"),
    ("provisions", "Provisions"),
)
MODULE_ICON_KINDS = ("chassis", "engine", "gun", "hull", "turret")


def parse_icon_style_map(game_data: pathlib.Path) -> dict[str, str]:
    """`<class>`（= 物品 XML 的 <icon>）→ bg-sprite 路径。"""
    import re
    pattern = re.compile(
        r'\.(?:equipment_item|optional_device_item)\.([A-Za-z0-9_\-]+)[^\n]*\n[^\n]*\n'
        r'\s+properties:\s*\n\s+bg-sprite:\s*"([^"]+)"')
    mapping: dict[str, str] = {}
    for rel in ICON_STYLE_MAPS:
        p = game_data / rel
        if not p.exists():
            continue
        text = decode_dvpl(p.read_bytes()).decode("utf-8")
        for cls, sprite in pattern.findall(text):
            mapping.setdefault(cls, sprite)
    if not mapping:
        raise RuntimeError("客户端 UI 样式表里没有找到任何 icon 映射")
    return mapping


def _sprite_frames(txt: str) -> list[tuple[int, int, int, int]]:
    lines = [line.strip() for line in txt.splitlines() if line.strip()]
    count = int(lines[3].split()[0])
    frames = []
    for line in lines[4:4 + count]:
        parts = line.split()
        frames.append((int(parts[0]), int(parts[1]), int(parts[2]), int(parts[3])))
    return frames


def _load_sprite(game_data: pathlib.Path, bases: list[str], cache: dict) -> "object":
    """按候选基名取图标：`<base>.txt.dvpl`（图集裁剪）优先，其次 `<base>.packed.webp.dvpl`（整图）。"""
    import io as _io
    from PIL import Image
    for base in bases:
        txt = game_data / f"{base}.txt.dvpl"
        if txt.exists():
            text = decode_dvpl(txt.read_bytes()).decode("utf-8")
            lines = [line.strip() for line in text.splitlines() if line.strip()]
            atlas = pathlib.PurePosixPath(base).parent / lines[1]
            atlas_path = game_data / f"{atlas}.dvpl"
            key = str(atlas_path)
            if key not in cache:
                cache[key] = Image.open(_io.BytesIO(decode_dvpl(atlas_path.read_bytes()))).convert("RGBA")
            image = cache[key]
            x, y, w, h = _sprite_frames(text)[0]
            if x + w > image.width or y + h > image.height:
                raise RuntimeError(f"icon frame out of atlas: {base} {x},{y},{w},{h} > {image.size}")
            return image.crop((x, y, x + w, y + h)).copy()
        packed = game_data / f"{base}.packed.webp.dvpl"
        if packed.exists():
            return Image.open(_io.BytesIO(decode_dvpl(packed.read_bytes()))).convert("RGBA")
    raise FileNotFoundError(f"icon sprite not found: {bases}")


def _normalize_icon(image: "object", canvas: int = 96, pad: int = 6,
                    crop_alpha: bool = True, alpha_threshold: int = 0) -> "object":
    """统一视觉尺寸：按比例缩放后居中放进固定方形画布。

    默认裁到 alpha 内容框（物品/模块/弹种图集帧：内容 34~83px 不等）。乘员技能
    技能图标用 `alpha_threshold=10`：这是为了贴近 BlitzKit 技能图标的可见内容
    效果而采用的 alpha 阈值裁切（BlitzKit 的 `sharp().trim()` 合并颜色与 alpha，
    不是同一算法）；否则 alpha>0 的帧外光晕会把实心主体拉偏。
    """
    from PIL import Image
    if crop_alpha:
        alpha = image.getchannel("A")
        if alpha_threshold:
            alpha = alpha.point(lambda value: 255 if value > alpha_threshold else 0)
        bbox = alpha.getbbox()
        if bbox:
            image = image.crop(bbox)
    inner = canvas - 2 * pad
    scale = min(inner / image.width, inner / image.height)
    size = (max(1, round(image.width * scale)), max(1, round(image.height * scale)))
    resized = image.resize(size, Image.LANCZOS)
    out = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    # 不能把 RGBA 自己当 paste mask：那会把边缘 alpha 再乘一次（alpha²），
    # 半透明抗锯齿边被吃掉后内容框只剩偏一侧的实心区，视觉上就“没居中”。
    # alpha_composite 对透明画布做正常 source-over，保留完整 alpha 与居中位置。
    out.alpha_composite(resized, ((canvas - size[0]) // 2, (canvas - size[1]) // 2))
    return out


def export_icons(game_data: pathlib.Path, out_dir: pathlib.Path, devices: dict,
                 consumables: list[dict], provisions: list[dict]) -> dict:
    """装备/消耗品/补给图标 + 模块类型图标（客户端图集裁剪；缺件记录在 manifest.missing）。

    只导出配置器会展示的物品（装备全部 + common 消耗品/补给）；事件模式图标不在本期消费面。
    """
    mapping = parse_icon_style_map(game_data)
    cache: dict = {}
    manifest: dict = {"modules": {}, "items": {}, "missing": []}
    missing: list[dict] = []

    # 图标目录由导出器全权生成：旧一代残留（如早期 vehicle*.webp）不进入新 manifest，
    # 先清空避免“包里有图但 manifest 没有”的歧义。
    import shutil
    icons_root = out_dir / "icons"
    if icons_root.exists():
        shutil.rmtree(icons_root)

    for kind, sub in ICON_KINDS:
        if kind == "devices":
            items = devices["devices"]
        elif kind == "consumables":
            items = [item for item in consumables if item["mode"] == "common"]
        else:
            items = [item for item in provisions if item["mode"] == "common"]
        exported: dict[str, dict] = {}
        for item in items:
            icon = item.get("icon")
            if not icon or icon in exported:
                continue
            bases: list[str] = []
            sprite = mapping.get(icon)
            if sprite:
                rel = sprite.replace("~res:/", "")
                bases.append(rel.rsplit(".", 1)[0] if rel.endswith((".txt", ".psd", ".png")) else rel)
            bases.append(f"Gfx/UI/InventoryIcons/Big/{sub}/{icon}")
            try:
                image = _load_sprite(game_data, bases, cache)
            except FileNotFoundError:
                missing.append({"kind": kind, "stem": item["stem"], "icon": icon})
                continue
            normalized = _normalize_icon(image)
            dest = out_dir / "icons" / "items" / kind / f"{icon}.webp"
            dest.parent.mkdir(parents=True, exist_ok=True)
            normalized.save(dest, "WEBP", lossless=True)
            exported[icon] = {"file": f"items/{kind}/{icon}.webp", "w": normalized.width, "h": normalized.height}
        manifest["items"][kind] = exported

    module_dir = game_data / "Gfx/UI/ModulesTechTree"
    for kind in MODULE_ICON_KINDS:
        src = module_dir / f"vehicle{kind.capitalize()}.packed.webp.dvpl"
        raw = decode_dvpl(src.read_bytes())
        if not raw.startswith((b"RIFF", b"\x89PNG")):
            raise RuntimeError(f"unexpected module icon payload: {src.name} head={raw[:8].hex()}")
        import io as _io
        from PIL import Image
        normalized = _normalize_icon(Image.open(_io.BytesIO(raw)).convert("RGBA"))
        dest = out_dir / "icons" / "modules" / f"{kind}.webp"
        dest.parent.mkdir(parents=True, exist_ok=True)
        normalized.save(dest, "WEBP", lossless=True)
        manifest["modules"][kind] = f"modules/{kind}.webp"

    manifest["missing"] = missing
    return manifest


def export_skills(game_data: pathlib.Path, out_dir: pathlib.Path,
                  tables_by_lang: dict[str, dict]) -> dict:
    """乘员技能：客户端 avatar.xml 的 class 归属/效果参数/图标 + Strings 文案。

    effect/tip 保留客户端原文（含 %(...) 参数占位）；`effects` 是 skill 元素的数值子项
    （per-level 小数，如 camouflageFactorPerLevel=0.03），前端/引擎不得另写常量。
    """
    root = load_dvpl_xml(game_data / "XML/item_defs/tankmen/avatar.xml.dvpl")
    skill_nodes = {el.tag: el for el in (root.find("skills") or []) if isinstance(el.tag, str)}
    class_map: dict[str, list[str]] = {}
    for cls in root.find("skillsByClasses") or []:
        if isinstance(cls.tag, str):
            class_map[cls.tag] = (cls.text or "").split()

    manifest: dict[str, dict] = {}
    missing: list[dict] = []
    for skill_id in SKILL_IDS:
        node = skill_nodes.get(skill_id)
        if node is None:
            missing.append({"kind": "skill-definition", "id": skill_id})
            continue
        icon_name = None
        for icon in node.findall("icon"):
            icon_name = ftext(icon, "name")
            if icon_name:
                break
        if not icon_name:
            missing.append({"kind": "skill-icon", "id": skill_id})
            continue
        base = icon_name.replace("~res:/", "")
        try:
            image = _load_sprite(game_data, [base], {})
        except FileNotFoundError:
            missing.append({"kind": "skill-icon", "id": skill_id})
            continue
        # 技能 packed 图比逻辑图标大，且边缘有近乎透明的光晕；按 alpha>0 取框会
        # 把光晕算进去，实心主体偏侧。这里用 alpha>10 的可见内容框（近似 BlitzKit
        # 的可见效果；sharp().trim() 合并颜色与 alpha，二者不是同一算法）。
        image = _normalize_icon(image, alpha_threshold=10)
        dest = out_dir / "icons" / "skills" / f"{skill_id}.webp"
        dest.parent.mkdir(parents=True, exist_ok=True)
        image.save(dest, "WEBP", lossless=True)

        effects: dict[str, object] = {}
        skill_type = None
        for child in node:
            if not isinstance(child.tag, str):
                continue
            if child.tag in ("userString", "effectDescription", "tipDescription", "featuresDescription"):
                continue
            if child.tag == "icon":
                continue
            raw = (child.text or "").strip()
            if child.tag == "type":
                skill_type = raw or None
                continue
            try:
                effects[child.tag] = float(raw)
            except ValueError:
                if raw:
                    effects[child.tag] = raw
        entry: dict[str, object] = {
            "icon": f"skills/{skill_id}.webp",
            "effects": effects,
        }
        if skill_type:
            entry["type"] = skill_type
        classes = [cls for cls, ids in class_map.items() if skill_id in ids]
        if classes:
            entry["classes"] = classes
        for lang, table in tables_by_lang.items():
            prefix = f"#crew_skills:{skill_id}/"
            for field in ("name", "effect", "tip", "features"):
                value = table.get(prefix + field)
                if value:
                    entry[f"{field}_{lang}"] = value.strip()
        manifest[skill_id] = entry
    return {"skills": manifest, "classes": class_map, "missing": missing}

def export_shell_icons(game_data: pathlib.Path, out_dir: pathlib.Path) -> dict:
    """弹种图标（AmmoSlot.yaml 的 iconPath → ammunition/normal/<type>_m）。"""
    manifest: dict[str, str] = {}
    missing: list[dict] = []
    cache: dict = {}
    for shell_type in SHELL_TYPES:
        try:
            image = _load_sprite(
                game_data, [f"Gfx/Shared/tank-supply/ammunition/normal/{shell_type}_m"], cache)
        except FileNotFoundError:
            missing.append({"kind": "shell-icon", "type": shell_type})
            continue
        image = _normalize_icon(image)
        dest = out_dir / "icons" / "shells" / f"{shell_type}.webp"
        dest.parent.mkdir(parents=True, exist_ok=True)
        image.save(dest, "WEBP", lossless=True)
        manifest[shell_type] = f"shells/{shell_type}.webp"
    return {"shells": manifest, "missing": missing}


def collect_vehicle_edges(nation: str, stem: str, root: ET.Element, comp: Components,
                          radio_shared: dict[str, ET.Element] | None,
                          fuel_shared: dict[str, ET.Element] | None) -> list[dict]:
    """整车研发边投影：本车模块 `<unlocks><vehicle>` → 研发图的车辆边。

    与 modules/*.json 同一物化口径（内联模块 + Components 共享解析），来源模块 kind
    取容器语义（chassis/turret/gun/engine/radio/fuelTank），与研发图统计
    （client-research-tech-tree-loadout.md §3.2）同构；`cost`/`costRaw` 均保留原值。
    """
    edges: list[dict] = []

    def scan(module_el: ET.Element, shared: ET.Element | None, src_kind: str, module_stem: str) -> None:
        u = module_el.find("unlocks")
        if u is None:
            return
        for target in u:
            if not isinstance(target.tag, str) or target.tag != "vehicle":
                continue
            cost_raw = (target.findtext("cost") or "").strip()
            edges.append({
                "from": stem,
                "to": (target.text or "").strip(),
                "cost": parse_cost(cost_raw),
                "costRaw": cost_raw,
                "sourceModule": module_stem,
                "sourceKind": src_kind,
            })

    # turrets 容器标签是字面 "turrets0"（与 export_module_kind 同一事实）；
    # radios/fuelTanks 的共享定义不在 Components 内，由调用方传入。
    containers = (("chassis", "chassis", None, None), ("turrets", "turrets0", None, None),
                  ("engines", "engines", None, None),
                  ("radios", "radios", "radios", radio_shared),
                  ("fuelTanks", "fuelTanks", "fuelTanks", fuel_shared))
    for comp_kind, container_tag, map_kind, shared_map in containers:
        container = last_child(root, container_tag)
        if container is None:
            continue
        for entry in container:
            if not isinstance(entry.tag, str):
                continue
            shared = shared_map.get(entry.tag) if shared_map is not None else comp.shared_def(comp_kind, entry.tag)
            scan(entry, shared, comp_kind.rstrip("s") if comp_kind != "chassis" else "chassis", entry.tag)
            if comp_kind == "turrets":
                guns_el = entry.find("guns")
                if guns_el is not None:
                    for g in guns_el:
                        if isinstance(g.tag, str):
                            scan(g, comp.shared_def("guns", g.tag), "gun", g.tag)
    return edges


def parse_list_facts(list_root: ET.Element) -> dict[str, dict]:
    """list.xml 条目 → 科技树节点事实（id/level/price/tags）。"""
    facts = {}
    for v in list_root:
        price_raw = (v.findtext("price") or "").strip()
        facts[v.tag] = {
            "listId": int(v.findtext("id")),
            "tier": int(v.findtext("level") or 0),
            "price": int(price_raw) if price_raw.isdigit() else 0,
            "priceRaw": price_raw,
            "tags": (v.findtext("tags") or "").split(),
        }
    return facts


def export_tech_tree(game_data: pathlib.Path, write, version: str, by_key: dict,
                     extra: dict | None, langs_unused: None = None) -> dict:
    """科技树布局 + 车辆研发边（--with-research-graph）。

    数据源（每国）：
    - `Configs/TechTree/{nation}_tree.yaml`：`tanks[stem].position = [列, 行]`（列≠tier，
      other 树列 1..7 而 tier 4..10；tier 一律读 list.xml `<level>`）+ `premium_rows`；
    - `XML/item_defs/vehicles/{nation}/list.xml`：tankId（bridge 对号）/tier/price/tags；
    - 逐车 XML `<unlocks><vehicle>`：整车研发边（collect_vehicle_edges）。

    `--tech-tree-extra`（11.22 XI 预览，tools/fetch_t11_techtree.py 产物）：为预览 yaml
    新增的 XI 节点补 position；其中 list 命中（BZ-79 / Object 432U）的为 preview 节点
    （tankId 来自 preview bridge），未实装的 6 个登记位进 `pending`（只有 stem + 坐标，
    不造名称/价格）。fail-closed：主树节点缺 list.xml 或 bridge 条目、研发边目标无法
    解析为车辆时直接报错。

    输出 `tech_tree/{nation}.json`：{meta, nodes, pending, edges}。
    """
    import yaml as _yaml

    extra_trees = (extra or {}).get("trees") or {}
    extra_lists = (extra or {}).get("list") or {}
    extra_version = (extra or {}).get("meta", {}).get("gameVersion")
    totals = {"nations": 0, "nodes": 0, "pending": 0, "edges": 0}
    list_facts_all: dict[str, dict] = {}
    cross_targets: set[str] = set()

    for nation in NATIONS:
        tree_p = game_data / f"Configs/TechTree/{nation}_tree.yaml.dvpl"
        tree = _yaml.safe_load(decode_dvpl(tree_p.read_bytes()).decode("utf-8")) or {}
        positions: dict[str, list[int]] = {}
        for stem, v in (tree.get("tanks") or {}).items():
            position = v.get("position") if isinstance(v, dict) else None
            if not position:
                raise RuntimeError(f"{nation}_tree.yaml entry without position: {stem}")
            positions[str(stem)] = [int(position[0]), int(position[1])]
        premium_rows = tree.get("premium_rows")

        list_root = load_dvpl_xml(game_data / f"XML/item_defs/vehicles/{nation}/list.xml.dvpl")
        facts = parse_list_facts(list_root)
        list_facts_all[nation] = facts

        nodes: list[dict] = []
        pending: list[dict] = []
        preview_stems: list[str] = []
        for stem, (x, y) in sorted(positions.items()):
            f = facts.get(stem)
            if f is None:
                raise RuntimeError(f"{nation}: tree node without list.xml entry: {stem}")
            tank_id = by_key.get((nation, stem))
            if tank_id is None:
                raise RuntimeError(f"{nation}: tree node not in tank bridge: {stem}")
            nodes.append({"stem": stem, "tankId": tank_id, **f, "x": x, "y": y})
        # XI 预览增量：主 yaml 没有的 stem 才来自预览树（主数据集坐标优先，预览不改写）。
        extra_tree = extra_trees.get(nation) or {}
        extra_list = extra_lists.get(nation) or {}
        for stem, v in sorted((extra_tree.get("tanks") or {}).items()):
            x, y = v["position"]
            if stem in positions:
                if positions[stem] != [x, y]:
                    print(f"   [tech-tree] {nation}/{stem}: preview position {[x, y]} differs "
                          f"from main {positions[stem]}（主数据集优先）")
                continue
            f = extra_list.get(stem)
            if f is not None:
                nodes.append({"stem": stem, "tankId": f["tankId"], "listId": f["listId"],
                              "tier": f["level"], "price": f["price"], "priceRaw": f["priceRaw"],
                              "tags": f["tags"], "x": x, "y": y, "preview": True})
                preview_stems.append(stem)
            else:
                # 科技树已登记但无车辆定义（超测展示位）：只有坐标，不猜名称/价格。
                pending.append({"stem": stem, "x": x, "y": y})

        # 整车研发边：全量 list 车辆（含 bridge 外的收藏/活动车，其 <unlocks> 一并覆盖）。
        comp = Components(game_data / f"XML/item_defs/vehicles/{nation}/components")
        _radio_ids, radio_shared = load_component(game_data / f"XML/item_defs/vehicles/{nation}/components", "radios")
        _fuel_ids, fuel_shared = load_component(game_data / f"XML/item_defs/vehicles/{nation}/components", "fuelTanks")
        edges: list[dict] = []
        for v in list_root:
            stem = v.tag
            p = game_data / f"XML/item_defs/vehicles/{nation}/{stem}.xml.dvpl"
            if not p.exists():
                continue
            edges.extend(collect_vehicle_edges(nation, stem, load_dvpl_xml(p), comp,
                                               radio_shared, fuel_shared))
        for edge in edges:
            if ":" in edge["to"]:
                cross_targets.add(edge["to"])
            elif edge["to"] not in facts:
                raise RuntimeError(f"{nation}: vehicle unlock target not in list.xml: "
                                   f"{edge['from']} -> {edge['to']}")

        file_meta = {"nation": nation, "gameVersion": version, "count": len(nodes),
                     "premiumRows": premium_rows, "edges": len(edges)}
        if pending:
            file_meta["pendingCount"] = len(pending)
        if preview_stems:
            file_meta["preview"] = {"gameVersion": extra_version, "tanks": sorted(preview_stems)}
        payload = {"meta": file_meta, "nodes": nodes, "pending": pending, "edges": edges}
        write(f"tech_tree/{nation}.json", payload)
        totals["nations"] += 1
        totals["nodes"] += len(nodes)
        totals["pending"] += len(pending)
        totals["edges"] += len(edges)
        print(f"   [tech-tree] {nation}: nodes={len(nodes)} pending={len(pending)} "
              f"edges={len(edges)} premium_rows={premium_rows}")

    # 跨国籍目标（`nation:stem`，导出规则：保留全名）：目标国 list.xml 可解析才放行。
    for target in sorted(cross_targets):
        t_nation, t_stem = target.split(":", 1)
        if t_stem not in list_facts_all.get(t_nation, {}):
            raise RuntimeError(f"cross-nation unlock target not in list.xml: {target}")
    return totals


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="data/research")
    ap.add_argument("--langs", default="en,zh-Hans")
    ap.add_argument("--with-research-graph", action="store_true",
                    help="同时导出 tech_tree/{nation}.json（科技树坐标 + 整车研发边）")
    ap.add_argument("--tech-tree-extra", default=None,
                    help="XI 预览增量 JSON（tools/fetch_t11_techtree.py 产物），与 "
                         "--with-research-graph 连用合并 XI 节点")
    ap.add_argument("--game-data", default=None,
                    help="客户端 Data 根目录（默认自动探测；预览客户端可显式指定）")
    ap.add_argument("--bridge", default=None,
                    help="tank-id 映射 JSON（默认 data/tank_id_bridge.json；"
                         "预览增量可给只含新增车的映射）")
    ap.add_argument("--modules-only", action="store_true",
                    help="只导出逐车 modules/*.json（预览客户端增量；不改装备/参数/图标）")
    ap.add_argument("--items-only", action="store_true",
                    help="只刷新全局 consumables/provisions/consumables_default（预览客户端"
                         "增量；不改车辆模块）")
    ap.add_argument("--merge", action="store_true",
                    help="与 --modules-only 连用：把本次导出的车并入既有 modules/*.json，"
                         "保留其余车")
    args = ap.parse_args()
    if args.merge and not args.modules_only:
        ap.error("--merge 只能与 --modules-only 连用")
    if args.items_only and (args.modules_only or args.with_research_graph or args.merge):
        ap.error("--items-only 不能与 --modules-only/--with-research-graph/--merge 连用")
    langs = [x.strip() for x in args.langs.split(",") if x.strip()]
    out_root = REPO_ROOT / args.out
    if not args.modules_only and not args.items_only:
        # 防回退门禁：data/research 是 SSOT 快照，XI 预览合并（--modules-only --merge）
        # 的成果不能被一次 11.21 全量导出静默冲掉（真实事故：740 → 738）。
        prev_meta_p = out_root / "meta.json"
        if prev_meta_p.exists():
            prev_preview = (json.loads(prev_meta_p.read_text(encoding="utf-8"))
                            .get("preview") or [])
            if prev_preview:
                raise RuntimeError(
                    f"{prev_meta_p} 已含 XI 预览合并（{len(prev_preview)} 辆）；全量导出会丢失它们。"
                    f"正确顺序：python tools/fetch_t11_techtree.py --data-root <root> 重建最小预览根 → "
                    f"全量导出到新目录后复制 tech_tree/ 增量 → 重跑 --modules-only --merge"
                    f"（docs/game-data-sources.md §2.7）。")
    out_root.mkdir(parents=True, exist_ok=True)
    game_data = pathlib.Path(args.game_data) if args.game_data else default_game_data()
    bridge_path = pathlib.Path(args.bridge) if args.bridge else REPO_ROOT / "data/tank_id_bridge.json"
    version = decode_dvpl((game_data / "version.txt.dvpl").read_bytes()).decode("utf-8").strip()
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    # XI 升级属性定义（modifier/script/params/icon/title）。11.21 现役客户端没有这个
    # 文件且没有 XI；只有遇到带 <upgradeProgression> 的车时才要求它存在（fail-closed）。
    upgrade_attr_rel = "XML/item_defs/vehicles/common/tank_upgrade_attributes.xml.dvpl"
    upgrade_attr_path = game_data / upgrade_attr_rel
    upgrade_attribute_definitions: dict[str, dict] | None = None
    upgrade_attributes_sha256: str | None = None
    if upgrade_attr_path.exists():
        attr_raw, attr_root = load_dvpl_xml_bytes(upgrade_attr_path)
        upgrade_attribute_definitions = parse_upgrade_attributes(attr_root)
        upgrade_attributes_sha256 = hashlib.sha256(attr_raw).hexdigest()
    # tank_id -> 顶级总 HP（车体 + 顶炮塔 + MAX_HEALTH 节点）；Rust tank_cache 消费。
    top_hp_by_tank: dict[int, int] = {}

    if args.tech_tree_extra and not args.with_research_graph:
        ap.error("--tech-tree-extra 只能与 --with-research-graph 连用")
    if args.with_research_graph and args.modules_only:
        ap.error("--with-research-graph 需要全量导出（不与 --modules-only 连用）")

    names_by_lang = {lang: Names(game_data, lang=lang) for lang in langs}
    en_names = names_by_lang[langs[0]]
    tables_by_lang = {}
    for lang in langs:
        import yaml as _y
        tables_by_lang[lang] = _y.safe_load(decode_dvpl(
            (game_data / f"Strings/{lang}.yaml.dvpl").read_bytes()).decode("utf-8")) or {}
        # 基础 Strings 之后补运行时覆盖层（只补缺键，基础表优先）
        for key, value in load_localization_overlay(lang).items():
            tables_by_lang[lang].setdefault(key, value)

    if args.items_only:
        write_item_catalogs(game_data, out_root, version, tables_by_lang, generated_at)
        return

    # 逐车 `<extras>` 只保留客户端 item 目录声明为 isVehicleAbility 的标签，
    # 避免把 alteredSpotDuration / burstMode 之外的普通 extras 误报成技能。
    ability_stems = vehicle_ability_stems(game_data, tables_by_lang)

    # ---------- 逐车模块 ----------
    # 模型碰撞盒 / 炮塔挂点：复用提取链的 Extractor（与 models.pb 同源同语义）。
    ex = Extractor(game_data)
    bridge = json.loads(bridge_path.read_text(encoding="utf-8"))
    by_key = {(v["nation"], v["stem"]): int(tid) for tid, v in bridge.items()}
    skipped_not_in_bridge: dict[str, int] = {}
    tank_counts = {}
    total_tanks = 0
    exported_keys: set[tuple[str, str]] = set()
    # 预览增量：bridge 只给新增车 → 只遍历这些国别（缺目录/文件的国别不猜、不写空）。
    nations = NATIONS
    if args.modules_only:
        nations = [n for n in NATIONS if any(v["nation"] == n for v in bridge.values())]
    for nation in nations:
        comp = Components(game_data / f"XML/item_defs/vehicles/{nation}/components")
        radio_ids, radio_shared = load_component(game_data / f"XML/item_defs/vehicles/{nation}/components", "radios")
        fuel_ids, fuel_shared = load_component(game_data / f"XML/item_defs/vehicles/{nation}/components", "fuelTanks")
        list_root = load_dvpl_xml(game_data / f"XML/item_defs/vehicles/{nation}/list.xml.dvpl")
        existing_meta: dict = {}
        if args.merge:
            mp = out_root / "modules" / f"{nation}.json"
            if mp.exists():
                existing = json.loads(mp.read_text(encoding="utf-8"))
                existing_meta = existing.get("meta") or {}
                tanks = dict(existing.get("tanks") or {})
            else:
                tanks = {}
        else:
            tanks = {}
        exported_here = 0
        skipped = 0
        for v in list_root:
            stem = v.tag
            tid = by_key.get((nation, stem))
            if tid is None:
                skipped += 1
                continue
            p = game_data / f"XML/item_defs/vehicles/{nation}/{stem}.xml.dvpl"
            if not p.exists():
                skipped += 1
                continue
            xml_raw, root = load_dvpl_xml_bytes(p)
            hull = root.find("hull")
            list_el = v
            name_key = ftext(list_el, "userString")
            tank = {
                "tankId": tid,
                "name": en_names.display(nation, name_key) or stem,
                "tier": fint(list_el, "level") or 0,
                "tags": (ftext(list_el, "tags") or "").split(),
                "consumableSlots": fint(root, "consumableSlots"),
                "provisionSlots": fint(root, "provisionSlots"),
                "optDevicePreset": ftext(root, "optDevicePreset"),
                "speedLimits": {
                    "forward": ffloat(root.find("speedLimits"), "forward"),
                    "backward": ffloat(root.find("speedLimits"), "backward"),
                },
                "invisibility": {
                    "still": ffloat(root.find("invisibility"), "still"),
                    "moving": ffloat(root.find("invisibility"), "moving"),
                    "firePenalty": ffloat(root.find("invisibility"), "firePenalty"),
                },
                "hull": {
                    "hp": fint(hull, "maxHealth") if hull is not None else None,
                    "weight": ffloat(hull, "weight") if hull is not None else None,
                },
                # 是否有可旋转炮塔：客户端 `<hull><fakeTurrets>` 元素**存在** ⇔ 无炮塔
                # （无炮塔车的“炮塔”是随火炮转动的固定战斗室，游戏把这类车标为
                # hasTurret=false，并用 gun <rotationSpeed> 作为转向速度）。
                # 738/738 与 BI 11.21 hasTurret 逐辆一致；缺 <hull> 时留 null（不猜有/无）。
                "hasTurret": (hull.find("fakeTurrets") is None) if hull is not None else None,
                # <crew> 角色表（子标签名；子标签有文本 = 兼职，如 commander 兼 gunner）。
                # 乘员角色加成（BK：有该角色 ×1.1，无 ×1.05）的判定输入。
                "crewRoles": [c.tag for c in (root.find("crew") or []) if isinstance(c.tag, str)],
                "chassis": export_module_kind(root, "chassis", comp, names_by_lang, tables_by_lang, nation),
                "turrets": export_module_kind(root, "turrets", comp, names_by_lang, tables_by_lang, nation),
                "engines": export_module_kind(root, "engines", comp, names_by_lang, tables_by_lang, nation),
                "radios": export_module_kind(root, "radios", comp, names_by_lang, tables_by_lang, nation,
                                             ids_map=radio_ids, shared_map=radio_shared),
                "fuelTanks": export_module_kind(root, "fuelTanks", comp, names_by_lang, tables_by_lang, nation,
                                                ids_map=fuel_ids, shared_map=fuel_shared),
            }
            # XI 车辆能力：客户端 `<extras>` 子标签中由 item 目录声明为
            # isVehicleAbility 的项；WotbTools 机制展示依赖这个绑定。
            extras = root.find("extras")
            if extras is not None:
                abilities = [child.tag for child in extras
                             if isinstance(child.tag, str) and child.tag in ability_stems]
                if abilities:
                    tank["vehicleAbilities"] = abilities
                    tank.setdefault("gameVersion", version)
            if tank["radios"] is not None:
                for m in tank["radios"]:
                    sh = radio_shared.get(m["stem"])
                    if sh is not None:
                        m["stats"]["distance"] = m["stats"]["distance"] if m["stats"]["distance"] is not None else ffloat(sh, "distance")
                        m["stats"]["weight"] = m["stats"]["weight"] if m["stats"]["weight"] is not None else ffloat(sh, "weight")
            if tank["fuelTanks"] is not None:
                for m in tank["fuelTanks"]:
                    sh = fuel_shared.get(m["stem"])
                    if sh is not None and m["stats"]["weight"] is None:
                        m["stats"]["weight"] = ffloat(sh, "weight")
            # XI 升级树（客户端 <upgradeProgression>）：节点/备选组/属性定义 + 来源
            # provenance。WotbTools 的 projectTankUpgrades 以此投影顶级配置；没有该块
            # 的普通车不写字段，保持原模块 JSON 形状。
            upgrade_el = root.find("upgradeProgression")
            if upgrade_el is not None:
                if upgrade_attribute_definitions is None:
                    raise RuntimeError(
                        f"{nation}/{stem} 含 upgradeProgression，但缺少 "
                        f"{upgrade_attr_rel}（fail-closed）")
                progression = parse_upgrade_progression(
                    upgrade_el,
                    upgrade_attribute_definitions,
                    version,
                    f"client:XML/item_defs/vehicles/{nation}/{stem}.xml.dvpl",
                    hashlib.sha256(xml_raw).hexdigest(),
                    "client:XML/item_defs/vehicles/common/tank_upgrade_attributes.xml.dvpl",
                    upgrade_attributes_sha256 or "",
                )
                tank["gameVersion"] = version
                tank["upgradeProgression"] = progression
                hp_top = upgrade_top_hp(tank, progression)
                if hp_top is not None:
                    top_hp_by_tank[tid] = hp_top
            # 车辆模型碰撞盒（尺寸参数）：车体/底盘盒在车辆级，炮塔盒挂在逐炮塔上
            # （随所选炮塔变）。turretOrigin 是 XML <turretPositions> 的 [x, 上, 前]，
            # 与包围盒坐标 [x, 前, 上] 的换算在前端 compute 中完成（见 §5 公式表）。
            try:
                model = ex.extract_model(nation, stem)
            except Exception as exc:  # noqa: BLE001 - 单辆车缺模型不应中断整国导出
                model = None
                print(f"   [bbox] {nation}/{stem} 跳过：{type(exc).__name__}: {exc}")
            if model:
                tank["turretOrigin"] = model.get("turret_origin")
                # 炮塔初始姿态（仅 4 辆意系 AT-SPG 非零）：pitch 计入有效俯仰上下限
                # （BI 11.21 同口径：Minotauro 18/7 + pitch 3 → 上 15 / 下 10）。
                tank["initialTurretRotation"] = model.get("initial_turret_rotation")
                tank["bbox"] = {
                    "hull": model.get("hull_bbox"),
                    "chassis": model.get("chassis_bbox"),
                }
                src_turrets = model.get("turrets") or []
                if len(src_turrets) == len(tank["turrets"]):
                    for dst, src in zip(tank["turrets"], src_turrets):
                        dst["stats"]["bbox"] = src.get("bbox")
                else:
                    # 顺序无法对号 = 不着色/不给错盒（fail-closed：尺寸整组隐藏）
                    print(f"   [bbox] {nation}/{stem} 炮塔数不匹配（{len(src_turrets)} vs {len(tank['turrets'])}），尺寸留空")
            if args.modules_only:
                # 预览增量数据来源逐车留痕：模块文件的 meta.gameVersion 仍描述主数据集。
                tank["gameVersion"] = version
            tanks[stem] = tank
            exported_here += 1
            exported_keys.add((nation, stem))
        skipped_not_in_bridge[nation] = skipped
        tank_counts[nation] = len(tanks)
        total_tanks += len(tanks)
        preview_ids = sorted(
            v["tankId"] for v in tanks.values() if v.get("gameVersion"))
        if args.modules_only and args.merge and exported_here == 0:
            continue
        file_meta = {"nation": nation, "gameVersion": version, "count": len(tanks)}
        if args.merge and existing_meta:
            file_meta["gameVersion"] = existing_meta.get("gameVersion") or version
            versions = sorted({v["gameVersion"] for v in tanks.values() if v.get("gameVersion")})
            if versions:
                file_meta["preview"] = {"gameVersions": versions, "tanks": preview_ids}
        payload = {"meta": file_meta, "tanks": tanks}
        (out_root / "modules").mkdir(parents=True, exist_ok=True)
        (out_root / "modules" / f"{nation}.json").write_text(
            json.dumps(payload, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")

    # 顶级 HP 旁路表：Rust `TankResolver::from_blitzkit` 读它写 `tank_cache.hp_top`
    # （资产包本身只带 tank_cache 的 `hp_top`，不依赖此文件）。预览 merge 保留既有条目。
    top_hp_path = out_root.parent / "upgrade_top_hp.json"
    merged_top_hp: dict[int, int] = {}
    if args.merge and top_hp_path.exists():
        previous = json.loads(top_hp_path.read_text(encoding="utf-8"))
        merged_top_hp.update({
            int(k): int(v) for k, v in (previous.get("tanks") or {}).items()
        })
    merged_top_hp.update(top_hp_by_tank)
    if merged_top_hp:
        top_hp_path.write_text(json.dumps({
            "meta": {"gameVersion": version, "count": len(merged_top_hp)},
            "tanks": {str(k): merged_top_hp[k] for k in sorted(merged_top_hp)},
        }, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        print(f"   [upgrade] top HP: {len(merged_top_hp)} tanks -> {top_hp_path}")

    if args.modules_only:
        # bridge 是这次要导出的权威清单；任何一个条目没落到输出都必须失败，
        # 不能因为拼错 stem/国别而静默成功。
        requested = {(v["nation"], v["stem"]) for v in bridge.values()}
        missing = sorted(requested - exported_keys)
        if missing:
            raise RuntimeError(f"bridge entries not exported: {missing}")
        # 预览增量只改 modules/*.json；meta.json 保留主数据集版本，追加预览车清单，
        # 并重算全量文件 sha256（资产包 manifest 的逐文件校验会读它）。
        meta_path = out_root / "meta.json"
        if args.merge and meta_path.exists():
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            meta["previewGeneratedAt"] = generated_at
        else:
            meta = {"gameVersion": version, "generatedAt": generated_at, "langs": langs}
        counts = meta.setdefault("counts", {})
        by_nation = dict(counts.get("tanksByNation") or {})
        by_nation.update(tank_counts)
        counts["tanks"] = sum(by_nation.values())
        counts["tanksByNation"] = by_nation
        if (out_root / "tech_tree").exists():
            tech = {"nations": 0, "nodes": 0, "pending": 0, "edges": 0}
            for tp in sorted((out_root / "tech_tree").glob("*.json")):
                d = json.loads(tp.read_text(encoding="utf-8"))
                tech["nations"] += 1
                tech["nodes"] += len(d.get("nodes") or [])
                tech["pending"] += len(d.get("pending") or [])
                tech["edges"] += len(d.get("edges") or [])
            counts["techTree"] = tech
        preview = []
        for mp in sorted((out_root / "modules").glob("*.json")):
            data = json.loads(mp.read_text(encoding="utf-8"))
            for tank in (data.get("tanks") or {}).values():
                gv = tank.get("gameVersion")
                if gv:
                    preview.append({"tankId": tank["tankId"], "nation": mp.stem,
                                    "gameVersion": gv})
        meta["preview"] = sorted(preview, key=lambda x: x["tankId"])
        files = {}
        for p in sorted(out_root.rglob("*")):
            if not p.is_file() or p.name == "meta.json":
                continue
            rel = p.relative_to(out_root).as_posix()
            files[rel] = {"sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
                          "bytes": p.stat().st_size}
        meta["files"] = files
        meta_path.write_text(json.dumps(meta, ensure_ascii=False, separators=(",", ":")),
                             encoding="utf-8")
        print(f"game={version} modules-only preview tanks={len(preview)}")
        for item in preview:
            print(f"  {item['nation']}: {item['tankId']}")
        print(f"out={out_root}")
        return

    # ---------- 装备 / 物品 / 默认表 / 参数 ----------
    devices = export_devices(game_data, names_by_lang, tables_by_lang)
    consumables = export_items(game_data, "consumables", tables_by_lang)
    provisions = export_items(game_data, "provisions", tables_by_lang)

    import yaml as _yaml
    defaults_raw = _yaml.safe_load(decode_dvpl(
        (game_data / "XML/item_defs/vehicles/common/consumablesDefault.yaml.dvpl").read_bytes()).decode("utf-8"))
    by_tier, overrides = {}, {}
    for k, v in (defaults_raw or {}).items():
        if isinstance(k, int) or (isinstance(k, str) and k.isdigit()):
            by_tier[int(k)] = v
        else:
            overrides[str(k)] = v

    structure, fmt = export_params(game_data)
    param_keys = sorted(fmt.keys())

    strings = export_strings(game_data, langs, param_keys)
    camouflage = export_camouflage(game_data)
    icons = export_icons(game_data, out_root, devices, consumables, provisions)
    skill_icons = export_skills(game_data, out_root, tables_by_lang)
    shell_icons = export_shell_icons(game_data, out_root)
    icons["skills"] = skill_icons["skills"]
    icons["shells"] = shell_icons["shells"]
    icons["missing"].extend(skill_icons["missing"])
    icons["missing"].extend(shell_icons["missing"])

    # ---------- 可选：研发图 / 科技树 ----------
    files: dict[str, dict] = {}

    def write(rel: str, obj) -> None:
        p = out_root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(obj, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")

    tech_tree_totals = None
    if args.with_research_graph:
        extra = None
        if args.tech_tree_extra:
            extra = json.loads((REPO_ROOT / args.tech_tree_extra).read_text(encoding="utf-8"))
        tech_tree_totals = export_tech_tree(game_data, write, version, by_key, extra)

    write("devices.json", {"meta": {"gameVersion": version, "count": len(devices["devices"]),
                                    "presets": len(devices["presets"])}, **devices})
    write("consumables.json", {"meta": {"gameVersion": version, "count": len(consumables)}, "items": consumables})
    write("provisions.json", {"meta": {"gameVersion": version, "count": len(provisions)}, "items": provisions})
    write("consumables_default.json", {"meta": {"gameVersion": version},
                                       "byTier": by_tier, "overrides": overrides})
    write("params/structure.json", {"meta": {"gameVersion": version}, **structure})
    write("params/format.json", {"meta": {"gameVersion": version}, "params": fmt})
    for lang in langs:
        write(f"strings/{lang}/research.json", {"meta": {"gameVersion": version, "lang": lang},
                                                "entries": strings[lang]})
    write("camouflage.json", {"meta": {"gameVersion": version}, **camouflage})
    write("skills.json", {"meta": {"gameVersion": version, "count": len(skill_icons["skills"])},
                          "classes": skill_icons["classes"], "items": skill_icons["skills"]})
    write("icons/manifest.json", {"meta": {"gameVersion": version}, **icons})

    # 逐文件 sha256：覆盖全部产出的数据与图标（meta.json 自身除外）
    for p in sorted(out_root.rglob("*")):
        if not p.is_file() or p.name == "meta.json":
            continue
        rel = p.relative_to(out_root).as_posix()
        files[rel] = {"sha256": hashlib.sha256(p.read_bytes()).hexdigest(), "bytes": p.stat().st_size}

    counts = {
        "tanks": total_tanks,
        "tanksByNation": tank_counts,
        "devices": len(devices["devices"]),
        "devicePresets": len(devices["presets"]),
        "consumables": len(consumables),
        "provisions": len(provisions),
        "consumablesCommon": sum(1 for x in consumables if x["mode"] == "common"),
        "provisionsCommon": sum(1 for x in provisions if x["mode"] == "common"),
        "skills": len(skill_icons["skills"]),
        "shellIcons": len(shell_icons["shells"]),
    }
    if tech_tree_totals is not None:
        counts["techTree"] = tech_tree_totals
    write("meta.json", {
        "gameVersion": version,
        "generatedAt": generated_at,
        "langs": langs,
        "counts": counts,
        "files": files,
    })
    print(f"game={version}")
    print(f"tanks={total_tanks} byNation={tank_counts}")
    print(f"devices={len(devices['devices'])} presets={len(devices['presets'])}")
    print(f"consumables={len(consumables)} (common {sum(1 for x in consumables if x['mode'] == 'common')})")
    print(f"provisions={len(provisions)} (common {sum(1 for x in provisions if x['mode'] == 'common')})")
    print(f"params={len(fmt)} strings({','.join(langs)}) icons={sum(len(v) for v in icons['items'].values())}+{len(icons['modules'])}")
    print(f"out={out_root}")


if __name__ == "__main__":
    main()
