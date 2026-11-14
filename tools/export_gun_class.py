#!/usr/bin/env python3
"""导出 炮 → 炮声等级 映射表（3D 回放炮声的数据面）。

数据来源（与 extract_vehicles.py 同一权威链，读同一 Data root）：
  * `XML/item_defs/vehicles/<nation>/<stem>.xml.dvpl`  每门炮 `<effects>wpn_shot_*</effects>`
  * `XML/item_defs/vehicles/<nation>/components/guns.xml.dvpl`  炮模块 local id

语义口径：炮声等级由**客户端文件级声明**给出（炮元素级 `<effects>`，内联优先、
共享 guns.xml 回退——DefResolver 同一合并规则），不做任何按口径/弹速的统计推断。
标签词表（2026-11-14 全量 738 车实测）：`wpn_shot_{small,mid,main,large,huge,
huge_extra,auto}` × 可选 `_mb` 后缀（同一等级的变体标记，网页侧同事件复用）；
其余 tag（如彩蛋车 `wpn_Oth50_STT`）无对应 Wwise 事件 → 不映射、报告。

前端消费（WotbTools gunfireAudio.ts）：vehicle 的 `gun_local`（configs[].gun_local /
回放 comp 透传）→ `gun_class.json[tank_id][gun_local]` → tag → `play_wpn_<class>_3p`。

输出（资产包内、随包发布）：
  release/asset_pack/sound/wpn/gun_class.json
    { "<tank_id>": { "<gun_local>": "<raw effects tag>" } }
"""
from __future__ import annotations

import argparse
import collections
import json
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

TOOLS_DIR = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

from extract_vehicles import (  # noqa: E402
    Components,
    DefResolver,
    default_game_data,
    last_child,
)
from wotb_sc2 import decode_dvpl  # noqa: E402

REPO_ROOT = TOOLS_DIR.parent
# 事件词表 = SoundbanksInfo 的 play_wpn_{cls}_3p 闭集（export_gun_sounds.py 同表）
WPN_TAG = re.compile(r"^wpn_shot_(?P<cls>small|mid|main|large|huge|huge_extra|auto)(?:_mb)?$")


def export(game_data: pathlib.Path, out_path: pathlib.Path) -> int:
    bridge = json.loads((REPO_ROOT / "data" / "tank_id_bridge.json").read_text(encoding="utf-8"))
    veh_dir = game_data / "XML" / "item_defs" / "vehicles"

    table: dict[str, dict[str, str]] = {}
    unmapped_tags: collections.Counter[str] = collections.Counter()
    comps: dict[str, Components] = {}
    missing_xml: list[str] = []
    tanks_with_guns = 0
    tanks_all_mapped = 0
    slots = 0
    mapped_slots = 0

    for tid, info in bridge.items():
        nation, stem = info["nation"], info["stem"]
        veh_path = veh_dir / nation / f"{stem}.xml.dvpl"
        if not veh_path.exists():
            missing_xml.append(f"{tid} {nation}/{stem}")
            continue
        if nation not in comps:
            comps[nation] = Components(veh_dir / nation / "components")
        comp = comps[nation]

        root = ET.fromstring(decode_dvpl(veh_path.read_bytes()))
        tank_map: dict[str, str] = {}
        slot_n = 0
        slot_mapped = 0
        t0 = last_child(root, "turrets0")
        if t0 is not None:
            for t_el in t0:
                tr = DefResolver(t_el, comp.shared_def("turrets", t_el.tag))
                guns_el = tr.elem("guns")
                if guns_el is None:
                    continue
                for g_el in guns_el:
                    gr = DefResolver(g_el, comp.shared_def("guns", g_el.tag))
                    tag = (gr.text("effects") or "").strip()
                    if not tag:
                        continue
                    lid = comp.local_id("guns", g_el.tag)
                    if lid is None:
                        continue
                    slot_n += 1
                    if WPN_TAG.match(tag):
                        slot_mapped += 1
                        tank_map[str(lid)] = tag
                    else:
                        unmapped_tags[tag] += 1
        slots += slot_n
        mapped_slots += slot_mapped
        if slot_n:
            tanks_with_guns += 1
            if slot_mapped == slot_n:
                tanks_all_mapped += 1
        if tank_map:
            table[str(tid)] = tank_map

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(table, ensure_ascii=False, sort_keys=True, indent=1),
                        encoding="utf-8")
    print(f"gun_class.json: {len(table)} 车 / {mapped_slots} 炮映射 → {out_path}")
    print(f"  带炮槽位的车: {tanks_with_guns}（全部映射: {tanks_all_mapped}）/ 桥接 {len(bridge)}")
    print(f"  炮槽位（local id + effects 双全）: {slots}，已映射: {mapped_slots}")
    if unmapped_tags:
        print("  UNMAPPED TAGS（fail-closed 报告，不入表）:")
        for tag, n in unmapped_tags.most_common():
            print(f"    {tag}: {n}")
    if missing_xml:
        print(f"  缺车辆 XML: {len(missing_xml)}（前 5）: {missing_xml[:5]}")
    return 0


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game-data", help="客户端 Data 根（默认按 extract_vehicles.GAME_DIR_CANDIDATES）")
    ap.add_argument("--out", default=str(REPO_ROOT / "release/asset_pack/sound/wpn/gun_class.json"))
    args = ap.parse_args()
    sys.exit(export(default_game_data() if not args.game_data else pathlib.Path(args.game_data),
                    pathlib.Path(args.out)))


if __name__ == "__main__":
    main()
