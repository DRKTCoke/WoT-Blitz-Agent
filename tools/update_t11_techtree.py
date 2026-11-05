#!/usr/bin/env python3
"""科技树 XI 预览的**外科手术式**增量更新（t11p2 起，替代整树重建）。

为什么不再整树重建：研发边是全树性质，但两包 central directory 的 CRC 对比证明
"基线预览包 → 新预览包"只有少数车辆定义 XML 变化（t11b→t11p2 为 94 changed + 3 added，
其余 780+ 辆逐字节相同）——未变车辆的边按定义不变。因此只对**变化车辆 + 预览车**逐车
重算 `collect_vehicle_edges` 并替换，再把预览包新实装的 XI 从 `pending` 转正为
`preview` 节点。某辆前缀车若在新包新增了指向新 XI 的研发边，会被逐车替换自动捕获。

输入：
- `--game-data`：`tools/fetch_t11_preview_assets.py` 的根（含全部共享组件/list/树
  yaml、变化车辆 XML、预览车 XML）；
- `--changed-xml`：CRC 对比得出的"本版相对基线有变化"的车辆定义 XML 路径清单
  （JSON 数组，zip 内路径；预览车新增的 3 辆也在其中）；
- `--tech-tree-extra`：`tools/fetch_t11_techtree.py` 产物（XI 登记位坐标 + list 事实）。

更新规则：
- 变化车辆的边：删 `from == stem` 的旧边，追加新边（其余边不动）；
- 新实装 XI（extra list 命中 + 树上有坐标、且当前还是 pending）：转正为
  `preview: true` 节点（tankId/listId/tier/price/tags/x/y），移出 pending；
- 未实装登记位维持 pending（只有坐标）；`A179_Black_Rock` 这类**树外**车
  （tier 11 收藏车、不在 `{nation}_tree.yaml`）不进树也不进 pending，仅存在于
  modules——本工具校验这一点；
- `meta.gameVersion`（主数据集版本）不动；`meta.edges/count/pendingCount` 与
  `meta.preview.{gameVersion,tanks}` 按更新后事实重写。

fail-closed：变化 XML 缺文件、研发边目标无法解析到树上节点、桥 tankId 缺失、
最终 pending 集合与预览登记位不符——一律报错退出，不写部分产物。

用法：
    python tools/update_t11_techtree.py --game-data <preview Data root> \
        --changed-xml .t11p2/changed_vehicle_xml.json
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from export_research_loadout import (  # noqa: E402
    Components,
    collect_vehicle_edges,
    load_component,
    load_dvpl_xml,
)

NATIONS = ["ussr", "germany", "usa", "china", "france", "uk", "japan", "other", "european"]


def parse_vehicle_path(rel: str) -> tuple[str, str]:
    """`app/Data/XML/item_defs/vehicles/{nation}/{stem}.xml.dvpl` → (nation, stem)。"""
    marker = "xml/item_defs/vehicles/"
    low = rel.lower()
    i = low.index(marker) + len(marker)
    rest = rel[i:]
    nation, _, filename = rest.partition("/")
    if not nation or not filename.lower().endswith(".xml.dvpl"):
        raise RuntimeError(f"不是车辆定义路径: {rel}")
    return nation, filename[: -len(".xml.dvpl")]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game-data", required=True)
    ap.add_argument("--changed-xml", required=True)
    ap.add_argument("--research", default="data/research")
    ap.add_argument("--base-bridge", default="data/tank_id_bridge.json")
    ap.add_argument("--preview-bridge", default="data/preview_tank_id_bridge.json")
    ap.add_argument("--tech-tree-extra", default="data/research_t11_preview/tech_tree_extra.json")
    args = ap.parse_args()

    game_data = pathlib.Path(args.game_data)
    research = REPO_ROOT / args.research
    tt_dir = research / "tech_tree"
    extra = json.loads((REPO_ROOT / args.tech_tree_extra).read_text(encoding="utf-8"))
    changed_xml = json.loads((REPO_ROOT / args.changed_xml).read_text(encoding="utf-8"))
    bridges: dict[str, dict] = {}
    for path in (args.base_bridge, args.preview_bridge):
        for tid, v in json.loads((REPO_ROOT / path).read_text(encoding="utf-8")).items():
            bridges[v["stem"]] = {"tankId": int(tid), "nation": v["nation"]}

    trees = {n: json.loads((tt_dir / f"{n}.json").read_text(encoding="utf-8")) for n in NATIONS}
    node_index = {n: {x["stem"] for x in d["nodes"]} for n, d in trees.items()}
    # 基线自检：meta 计数必须与数据一致（不一致说明上游数据已坏，不是本工具该修的）
    for nation, tt in trees.items():
        meta = tt["meta"]
        if meta["count"] != len(tt["nodes"]) or meta.get("pendingCount", 0) != len(tt["pending"]) \
                or meta["edges"] != len(tt["edges"]):
            raise RuntimeError(
                f"{nation}: 现有 tech_tree meta 计数与数据不符（上游数据已坏）")

    # ---------- ① 新实装 XI：pending → preview 节点（先于边校验：新边目标才可见） ----------
    node_moves: list[str] = []
    dirty: set[str] = set()
    extra_version = extra.get("meta", {}).get("gameVersion")
    for nation in NATIONS:
        tree_positions = (extra.get("trees") or {}).get(nation, {}).get("tanks") or {}
        list_facts = (extra.get("list") or {}).get(nation) or {}
        if not tree_positions and not list_facts:
            continue
        tt = trees[nation]
        pending_stems = {x["stem"] for x in tt["pending"]}
        preview_tanks: list[str] = []
        for stem, pos in sorted(tree_positions.items()):
            x, y = pos["position"]
            if stem in node_index[nation]:
                # extra 的树是整棵树（含主树车）：主数据集坐标优先，只顺带校验漂移
                node = next(v for v in tt["nodes"] if v["stem"] == stem)
                if [node["x"], node["y"]] != [x, y]:
                    print(f"[warn] {nation}/{stem}: 预览坐标 {[x, y]} 与现有 "
                          f"{[node['x'], node['y']]} 不同（主数据集优先，不改写）")
                if node.get("preview"):
                    preview_tanks.append(stem)
                continue
            facts = list_facts.get(stem)
            if facts is None:
                # 未实装登记位：必须在 pending 中（只有坐标）
                if stem not in pending_stems:
                    raise RuntimeError(f"{nation}/{stem}: 未实装登记位缺 pending 条目")
                item = next(v for v in tt["pending"] if v["stem"] == stem)
                if [item["x"], item["y"]] != [x, y]:
                    raise RuntimeError(f"{nation}/{stem}: pending 坐标与预览树不符")
                continue
            tank_id = facts.get("tankId")
            if tank_id is None:
                raise RuntimeError(f"{nation}/{stem}: 预览 list 事实缺 tankId（bridge 未对号）")
            if stem not in pending_stems:
                raise RuntimeError(f"{nation}/{stem}: 新实装 XI 应处于 pending，实际不在")
            tt["pending"] = [v for v in tt["pending"] if v["stem"] != stem]
            tt["nodes"].append({
                "stem": stem, "tankId": tank_id, "listId": facts["listId"],
                "tier": facts["level"], "price": facts["price"], "priceRaw": facts["priceRaw"],
                "tags": facts["tags"], "x": x, "y": y, "preview": True,
            })
            node_index[nation].add(stem)
            preview_tanks.append(stem)
            node_moves.append(f"{nation}/{stem}")
        # 树外预览车（如 A179_Black_Rock）：不得混进 nodes/pending
        for stem, facts in list_facts.items():
            if stem not in tree_positions and (stem in node_index[nation]
                                               or any(v["stem"] == stem for v in tt["pending"])):
                raise RuntimeError(f"{nation}/{stem}: 树外预览车不应出现在树数据中")
        if preview_tanks:
            prev = tt["meta"].get("preview")
            new_stems = sorted(set(preview_tanks) - set((prev or {}).get("tanks") or []))
            if new_stems:
                prev = tt["meta"].setdefault("preview", {})
                prev["gameVersion"] = extra_version
                prev["tanks"] = sorted(set(preview_tanks) | set(prev.get("tanks") or []))
                dirty.add(nation)

    # ---------- ② 变化车辆 + 预览车：逐车重算研发边并替换 ----------
    edge_stats: dict[str, dict] = {}
    seen: set[tuple[str, str]] = set()
    for rel in changed_xml:
        nation, stem = parse_vehicle_path(rel)
        if (nation, stem) in seen:
            continue
        seen.add((nation, stem))
        if nation not in NATIONS:
            raise RuntimeError(f"未知国别: {nation} ({rel})")
        p = game_data / rel[rel.lower().index("data/") + len("data/"):]
        if not p.exists():
            raise RuntimeError(f"变化车辆 XML 不在 game_data 根中（先拉取）: {rel}")
        comp_dir = game_data / f"XML/item_defs/vehicles/{nation}/components"
        comp = Components(comp_dir)
        _, radio_shared = load_component(comp_dir, "radios")
        _, fuel_shared = load_component(comp_dir, "fuelTanks")
        new_edges = collect_vehicle_edges(
            nation, stem, load_dvpl_xml(p), comp, radio_shared, fuel_shared)
        for edge in new_edges:
            target = edge["to"]
            if ":" in target:
                t_nation, t_stem = target.split(":", 1)
                if t_stem not in node_index.get(t_nation, set()):
                    raise RuntimeError(f"{nation}/{stem}: 跨国籍研发目标不在树上: {target}")
            elif target not in node_index[nation]:
                raise RuntimeError(f"{nation}/{stem}: 研发目标不在树上: {target}")
        tt = trees[nation]
        old_for_stem = [e for e in tt["edges"] if e["from"] == stem]
        if old_for_stem != new_edges:
            tt["edges"] = [e for e in tt["edges"] if e["from"] != stem] + new_edges
            dirty.add(nation)
            edge_stats.setdefault(nation, {"vehicles": 0, "edges": 0})
            edge_stats[nation]["vehicles"] += 1
            edge_stats[nation]["edges"] += len(new_edges) - len(old_for_stem)
        # 该车是预览定义车（已转正节点）时顺带校验 tankId 一致
        if stem in node_index[nation]:
            node = next(x for x in tt["nodes"] if x["stem"] == stem)
            expect = bridges.get(stem, {}).get("tankId")
            if expect is not None and node.get("tankId") != expect:
                raise RuntimeError(f"{nation}/{stem}: 节点 tankId {node.get('tankId')} != bridge {expect}")

    # ---------- 3. 落盘（只写 dirty；计数随变更重算） ----------
    for nation in NATIONS:
        if nation not in dirty:
            continue
        tt = trees[nation]
        meta = tt["meta"]
        meta["count"] = len(tt["nodes"])
        if tt["pending"]:
            meta["pendingCount"] = len(tt["pending"])
        else:
            meta.pop("pendingCount", None)
        meta["edges"] = len(tt["edges"])
        (tt_dir / f"{nation}.json").write_text(
            json.dumps(tt, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
        print(f"[updated] {nation}: nodes={meta['count']} pending={len(tt['pending'])} "
              f"edges={meta['edges']}")

    print(f"edge-updated vehicles: 共 {len(seen)} 辆逐车重算（含未产生差异的）")
    for nation, s in sorted(edge_stats.items()):
        print(f"  {nation}: {s['vehicles']} 辆边的定义有变化（净 {s['edges']:+d} 条）")
    print(f"pending → preview 节点: {node_moves or '（无）'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
