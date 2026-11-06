#!/usr/bin/env python3
"""XI `upgradeProgression` 导出契约的独立回归测试（无 pytest 依赖）。

覆盖：
1. `tank_upgrade_attributes.xml` 的属性定义解析（modifier/script/内联 params）；
2. 车辆 `<upgradeProgression>` → WotbTools `UpgradeProgression` 形状
   （节点 id/techName/type/attributes、alternativeNodes→choices、provenance）；
3. 顶级总 HP = 车体 + 顶炮塔 + 所有 MAX_HEALTH 节点；
4. 若本地存在预览构建产物（`.t11p2/research_work`），核对 5 辆 XI 的真实值。

用法：python tools/test_upgrade_progression.py
"""

from __future__ import annotations

import json
import pathlib
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import export_research_loadout as E  # noqa: E402

ATTRIBUTES_XML = """<root><attributes>
  <attribute>
    <attributeName>ATTRIBUTE_NAME_MAX_HEALTH</attributeName>
    <modifier>MODIFIER_ADD</modifier>
    <script>MaxHealthUpgrade</script>
    <icon>~res:/icon_hp</icon>
    <title>TankUpgrade/Attribute/MaxHealth</title>
  </attribute>
  <attribute>
    <attributeName>ATTRIBUTE_NAME_FORWARD_MAX_SPEED</attributeName>
    <modifier>MODIFIER_ADD</modifier>
    <script>SpeedLimitUpgrade<params><type>forward</type></params></script>
    <icon>~res:/icon_speed</icon>
    <title>TankUpgrade/Attribute/MaxSpeed</title>
  </attribute>
  <attribute>
    <attributeName>SAS_ALLOW_ACTIVATE_ON_HIT</attributeName>
    <modifier>MODIFIER_BOOLEAN</modifier>
  </attribute>
</attributes></root>"""

PROGRESSION_XML = """<upgradeProgression>
  <matrix><rows>2</rows><columns>2</columns></matrix>
  <alternativeMechanicGroups>
    <alternativeMechanicGroup>
      <techName>tier11_choice</techName>
      <displayName>Group/Name</displayName>
      <description>Group/Desc</description>
      <icon>~res:/group</icon>
      <alternativeNodes>
        <node>alt_allow</node>
        <node>alt_other</node>
      </alternativeNodes>
    </alternativeMechanicGroup>
  </alternativeMechanicGroups>
  <nodes>
    <node>
      <id>1</id><techName>tier11_hp_01</techName><nodeType>NODETYPE_COMMON</nodeType>
      <attributes><attribute>
        <attributeName>ATTRIBUTE_NAME_MAX_HEALTH</attributeName><value>100</value>
      </attribute></attributes>
    </node>
    <node>
      <id>2</id><techName>tier11_speed_01</techName><nodeType>NODETYPE_COMMON</nodeType>
      <attributes><attribute>
        <attributeName>ATTRIBUTE_NAME_FORWARD_MAX_SPEED</attributeName><value>2</value>
      </attribute></attributes>
    </node>
    <node>
      <id>3</id><techName>alt_allow</techName><nodeType>NODETYPE_ALTERNATIVE_MECHANIC</nodeType>
      <displayName>Alt/Allow</displayName>
      <description>Alt/Allow/Desc</description>
      <icon>~res:/alt_allow</icon>
      <attributes><attribute>
        <attributeName>SAS_ALLOW_ACTIVATE_ON_HIT</attributeName><value>true</value>
      </attribute></attributes>
    </node>
    <node>
      <id>4</id><techName>alt_other</techName><nodeType>NODETYPE_ALTERNATIVE_MECHANIC</nodeType>
      <attributes><attribute>
        <attributeName>ATTRIBUTE_NAME_MAX_HEALTH</attributeName><value>50</value>
      </attribute></attributes>
    </node>
  </nodes>
</upgradeProgression>"""


def test_parser_roundtrip() -> None:
    attrs = E.parse_upgrade_attributes(ET.fromstring(ATTRIBUTES_XML))
    assert attrs["ATTRIBUTE_NAME_MAX_HEALTH"]["script"] == "MaxHealthUpgrade", attrs
    assert attrs["ATTRIBUTE_NAME_FORWARD_MAX_SPEED"]["params"] == {"type": "forward"}, attrs
    assert attrs["SAS_ALLOW_ACTIVATE_ON_HIT"]["modifier"] == "MODIFIER_BOOLEAN", attrs

    progression = E.parse_upgrade_progression(
        ET.fromstring(PROGRESSION_XML), attrs,
        "game-version", "client:src", "sha", "client:attrs", "attrs-sha",
    )
    assert [n["id"] for n in progression["nodes"]] == [1, 2, 3, 4], progression
    assert progression["nodes"][2]["attributes"] == [
        {"name": "SAS_ALLOW_ACTIVATE_ON_HIT", "value": True}
    ], progression
    # 客户端节点级展示元数据（有则保留；无则省略字段）
    assert progression["nodes"][2]["displayName"] == "Alt/Allow", progression["nodes"][2]
    assert progression["nodes"][2]["description"] == "Alt/Allow/Desc", progression["nodes"][2]
    assert progression["nodes"][2]["icon"] == "~res:/alt_allow", progression["nodes"][2]
    assert "displayName" not in progression["nodes"][0], progression["nodes"][0]
    assert progression["alternativeGroups"] == [{
        "id": "tier11_choice",
        "choices": [3, 4],
        "icon": "~res:/group",
        "displayName": "Group/Name",
        "description": "Group/Desc",
    }], progression
    # 只带本车节点实际引用的定义（与 WotbTools fixture 同口径）
    assert set(progression["attributeDefinitions"]) == {
        "ATTRIBUTE_NAME_MAX_HEALTH",
        "ATTRIBUTE_NAME_FORWARD_MAX_SPEED",
        "SAS_ALLOW_ACTIVATE_ON_HIT",
    }, progression["attributeDefinitions"]
    assert progression["provenance"] == {
        "gameVersion": "game-version",
        "source": "client:src",
        "sha256": "sha",
        "attributesSource": "client:attrs",
        "attributesSha256": "attrs-sha",
    }, progression["provenance"]

    tank = {
        "hull": {"hp": 1000},
        "turrets": [{"stats": {"maxHealth": 500}}],
    }
    # 顶级 = 所有非 alternative 节点 + 每个组首个 choice：100 + (alt_other 50 不计)
    assert E.upgrade_top_hp(tank, progression) == 1600


def test_real_preview_artifact() -> None:
    """本地预览合并产物存在时核对 5 辆 XI 的客户端导出值。"""
    modules_dir = ROOT / ".t11p2/research_work/modules"
    sidecar = ROOT / ".t11p2/upgrade_top_hp.json"
    if not modules_dir.is_dir() or not sidecar.is_file():
        print("skip real preview artifact (.t11p2/research_work missing)")
        return
    expected = {15153: 2850, 27905: 2200, 28161: 3100, 30241: 2900, 30497: 2200}
    found: dict[int, int] = {}
    for path in sorted(modules_dir.glob("*.json")):
        for tank in json.loads(path.read_text(encoding="utf-8")).get("tanks", {}).values():
            progression = tank.get("upgradeProgression")
            if not progression:
                continue
            assert tank["gameVersion"] == progression["provenance"]["gameVersion"], tank["tankId"]
            hp = E.upgrade_top_hp(tank, progression)
            if hp is not None:
                found[tank["tankId"]] = hp
    top_hp = json.loads(sidecar.read_text(encoding="utf-8"))["tanks"]
    assert {int(k): v for k, v in top_hp.items()} == expected, top_hp
    assert found == expected, found


def _rule_matches(rule: dict, tank: dict) -> bool:
    if not rule:
        return False
    tiers = rule.get("tiers")
    if tiers and not (tiers["min"] <= tank["tier"] <= tiers["max"]):
        return False
    if "ids" in rule:
        if f"{tank['nation']}:{tank['stem']}" not in rule["ids"] and str(tank["tankId"]) not in rule["ids"]:
            return False
    if "nations" in rule and tank["nation"] not in rule["nations"]:
        return False
    if "extendedTags" in rule and not any(tag in tank["tags"] for tag in rule["extendedTags"]):
        return False
    return True


def _available(item: dict, tank: dict) -> bool:
    flt = item.get("filter")
    if not flt:
        return True
    if any(_rule_matches(rule, tank) for rule in flt.get("exclude") or []):
        return False
    include = flt.get("include") or []
    return any(_rule_matches(rule, tank) for rule in include) if include else True


def test_xi_item_catalogs() -> None:
    """XI 的消耗品/补给过滤器必须覆盖 tier 11；isVehicleAbility 不进装配池。"""
    root = ROOT / ".t11p2/research_work"
    if not (root / "consumables.json").is_file() or not (root / "provisions.json").is_file():
        print("skip XI item catalogs (.t11p2/research_work missing)")
        return
    catalogs = {
        kind: json.loads((root / f"{kind}.json").read_text(encoding="utf-8"))["items"]
        for kind in ("consumables", "provisions")
    }
    # 预览客户端的公共物品过滤器已扩到 11（11.21 现役仍为 10）。
    extinguisher = next(x for x in catalogs["consumables"] if x["stem"] == "autoExtinguisher")
    assert extinguisher["filter"]["include"][0]["tiers"]["max"] == 11, extinguisher
    # XI 自身能力（isVehicleAbility）不是机库装配项。
    for kind, stem in (
        ("consumables", "rechargeableNitro"),
        ("provisions", "steadyAimSystem"),
        ("provisions", "combatFrenzy"),
        ("provisions", "burstMode"),
        ("provisions", "rammingConfiguration"),
    ):
        item = next(x for x in catalogs[kind] if x["stem"] == stem)
        assert item["isAbility"] is True, (kind, stem, item)

    tanks = [
        {"tier": 11, "nation": "usa", "stem": "A183_XM69_Hacker", "tankId": 30497,
         "tags": ["mediumTank"]},
        {"tier": 11, "nation": "china", "stem": "Ch67_BZ_79", "tankId": 15153,
         "tags": ["heavyTank"]},
    ]
    for tank in tanks:
        for kind in ("consumables", "provisions"):
            available = [
                item["stem"] for item in catalogs[kind]
                if not item.get("isAbility")
                and item.get("mode") == "common"
                and _available(item, tank)
            ]
            assert len(available) >= 5, (tank["stem"], kind, available)
            assert "rechargeableNitro" not in available, available
            assert "steadyAimSystem" not in available, available


def test_module_durability_params() -> None:
    """预览 params 增量：modulesDumpSection 7×6 参数，且不进入 mainSections。"""
    root = ROOT / ".t11p2/research_work"
    structure_path = root / "params/structure.json"
    format_path = root / "params/format.json"
    if not structure_path.is_file() or not format_path.is_file():
        print("skip module durability params (.t11p2/research_work missing)")
        return
    structure = json.loads(structure_path.read_text(encoding="utf-8"))
    sections = {s.get("section"): s for s in structure["sections"]}
    assert "modulesDumpSection" in sections, sections.keys()
    assert "modulesDumpSection" not in structure.get("mainSections", []), structure.get("mainSections")
    subsections = [s.get("subsection") for s in sections["modulesDumpSection"]["content"]]
    assert subsections == [
        "trackDevice", "engineDevice", "gunDevice", "ammobayDevice",
        "turretRotatorDevice", "surveyingDeviceDevice", "fuelTankDevice",
    ], subsections
    fmt = json.loads(format_path.read_text(encoding="utf-8"))["params"]
    prefixes = ("track", "engine", "gun", "ammobay", "turretRotator",
                "surveyingDevice", "fuelTank")
    suffixes = ("MaxHealth", "RegenHealth", "HysteresisHealth",
                "RegenPerSec", "BurnPerSec", "RepairTime")
    keys = [key for key in fmt if key.startswith(prefixes) and key.endswith(suffixes)]
    assert len(keys) == 42, (len(keys), keys)
    assert fmt["engineBurnPerSec"] == {
        "units": "UNIT_PER_SECOND", "digits": 1, "biggerIsBetter": False,
    }, fmt["engineBurnPerSec"]
    assert fmt["engineRepairTime"] == {
        "units": "SECOND", "digits": 2, "biggerIsBetter": False,
    }, fmt["engineRepairTime"]
    assert fmt["engineMaxHealth"] == {}, fmt["engineMaxHealth"]


def main() -> None:
    test_parser_roundtrip()
    test_real_preview_artifact()
    test_xi_item_catalogs()
    test_module_durability_params()
    print("upgrade progression tests: PASS")


if __name__ == "__main__":
    main()
