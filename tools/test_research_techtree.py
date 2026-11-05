"""科技树导出护栏（standalone，无 pytest 依赖）。

校验 `data/research/tech_tree/{nation}.json`（`export_research_loadout.py
--with-research-graph` 产物）的结构不变量与研发图统计（census
client-research-tech-tree-loadout.md §3.2 / §4.2），以及 XI 预览合并
（tools/fetch_t11_techtree.py → --tech-tree-extra）的落盘事实：

- 节点/研发边总数与 census 一致（308 = 304 主树 + 4 辆预览定义车；314 条整车边）；
- 每个定义节点能对号 modules/{nation}.json（前端联表的完整性边界）；
- 列↔tier 映射按列内一致性成立（other 树列 1..7 → tier 4..10）；
- pending 只有 stem/x/y（未实装展示位不造名称/价格）；
- 每条边 cost 三态合法（xp / tokens / raw），跨国籍目标带 `nation:` 前缀。

用法：python tools/test_research_techtree.py
"""
import json
import pathlib
import collections

ROOT = pathlib.Path(__file__).resolve().parent.parent
TT = ROOT / "data/research/tech_tree"
NATIONS = ["ussr", "germany", "usa", "china", "france", "uk", "japan", "other", "european"]
# census 的整车边行（vehicle）；逐来源模块 kind 的拆分是导出的派生事实，
# 只校验自洽（各 kind 之和 = 总数），不用未记录的数字硬编码。
# 2026-11-05 t11p2：XI 预览定义车 4 辆（BZ-79/432U/XM69/KR-1），登记位 pending 4 个；
# 研发边 314 = 310（t11b 基线）+ 4（t11p2 逐车重算净增：china+1 usa+1 ussr+2）。
CENSUS_VEHICLE_EDGES = 314
CENSUS_NODES = 308
CENSUS_PENDING = 4
# 科技树上有节点的预览定义车；A179_Black_Rock 是收藏车（tier 11）但**不在树上**，
# 单独校验它只存在于 modules 而无 tree 节点（防"树外车混进节点"回归）。
PREVIEW_TANKS = {
    "Ch67_BZ_79": ("china", 15153),
    "R230_Object_432U": ("ussr", 27905),
    "A183_XM69_Hacker": ("usa", 30497),
    "R228_KR_1": ("ussr", 28161),
}
PREVIEW_OFF_TREE = {"A179_Black_Rock": ("usa", 30241)}
PENDING_STEMS = {"G189_Hirschkafer", "G187_Taschenratte", "J77_Ho_Ri_4", "F135_AS_XX_40_t"}


def test_tech_tree_export():
    assert {p.stem for p in TT.glob("*.json")} == set(NATIONS), "9 国文件齐全"
    nodes_total = pending_total = edges_total = 0
    kinds = collections.Counter()
    problems = []

    for nation in NATIONS:
        data = json.loads((TT / f"{nation}.json").read_text(encoding="utf-8"))
        meta = data["meta"]
        nodes = data["nodes"]
        pending = data["pending"]
        edges = data["edges"]
        nodes_total += len(nodes)
        pending_total += len(pending)
        edges_total += len(edges)
        if meta["nation"] != nation:
            problems.append(f"{nation}: meta.nation mismatch")
        if meta["count"] != len(nodes) or meta["edges"] != len(edges):
            problems.append(f"{nation}: meta 计数与数据不符")

        # 模块联表完整性：每个定义节点的 tankId 都能在 modules/{nation}.json 对号
        # （modules 的键是 stem、值里只有 tankId；节点 stem 即键）
        modules = json.loads((ROOT / f"data/research/modules/{nation}.json").read_text(encoding="utf-8"))
        module_tanks = modules["tanks"]
        for node in nodes:
            for key in ("stem", "tankId", "listId", "tier", "x", "y", "price", "priceRaw", "tags"):
                if key not in node:
                    problems.append(f"{nation}/{node.get('stem')}: 缺字段 {key}")
            if node["stem"] not in module_tanks:
                problems.append(f"{nation}/{node['stem']}: 不在 modules/{nation}.json（前端联表会断）")
            elif module_tanks[node["stem"]]["tankId"] != node["tankId"]:
                problems.append(f"{nation}/{node['stem']}: tankId 与 modules 不一致")
            if not (isinstance(node["x"], int) and isinstance(node["y"], int)):
                problems.append(f"{nation}/{node['stem']}: 坐标不是整数")

        # 列 ↔ tier 一致性（other 树例外本身也应满足"列内 tier 唯一"）
        by_col = collections.defaultdict(set)
        for node in nodes:
            by_col[node["x"]].add(node["tier"])
        for col, tiers in by_col.items():
            if len(tiers) != 1:
                problems.append(f"{nation}: 列 {col} 的 tier 不唯一 {sorted(tiers)}")

        # pending：只有 stem/x/y
        for item in pending:
            extra = set(item) - {"stem", "x", "y"}
            if extra:
                problems.append(f"{nation}/pending/{item['stem']}: 不该有字段 {sorted(extra)}")

        # 边：cost 三态 + 来源模块 kind
        for edge in edges:
            if edge.get("kind") == "vehicle" or "kind" in edge:
                problems.append(f"{nation}: 边不应带 kind 字段（sourceKind 才是来源）")
            cost = edge.get("cost") or {}
            if not (set(cost) & {"xp", "tokens", "raw"}) or len(cost) != 1:
                problems.append(f"{nation}: 非法 cost {cost}")
            if ":" in edge.get("to", "") and edge["to"].split(":", 1)[0] not in NATIONS:
                problems.append(f"{nation}: 跨国籍目标国别未知 {edge['to']}")
            kinds[edge.get("sourceKind")] += 1

    # census 对拍（tech_tree 只含整车边：from=车 → to=车）
    if nodes_total != CENSUS_NODES:
        problems.append(f"nodes {nodes_total} != census {CENSUS_NODES}")
    if pending_total != CENSUS_PENDING:
        problems.append(f"pending {pending_total} != census {CENSUS_PENDING}")
    if edges_total != CENSUS_VEHICLE_EDGES:
        problems.append(f"edges {edges_total} != census {CENSUS_VEHICLE_EDGES}")
    if sum(kinds.values()) != edges_total:
        problems.append(f"sourceKind 之和 {sum(kinds.values())} != edges {edges_total}")

    # XI 预览事实
    for stem, (nation, tank_id) in PREVIEW_TANKS.items():
        data = json.loads((TT / f"{nation}.json").read_text(encoding="utf-8"))
        node = next((n for n in data["nodes"] if n["stem"] == stem), None)
        if node is None:
            problems.append(f"{nation}: 预览定义车 {stem} 缺节点")
        elif not (node.get("preview") and node["tankId"] == tank_id and node["tier"] == 11 and node["x"] == 11):
            problems.append(f"{nation}/{stem}: 预览节点事实不符 {node}")
    pending_all = set()
    for nation in NATIONS:
        data = json.loads((TT / f"{nation}.json").read_text(encoding="utf-8"))
        pending_all |= {item["stem"] for item in data["pending"]}
    if pending_all != PENDING_STEMS:
        problems.append(f"pending stems {sorted(pending_all)} != 预览登记位")
    # 树外预览车（收藏 tier 11）：必须在 modules 里，且不得出现在任何树上
    for stem, (nation, tank_id) in PREVIEW_OFF_TREE.items():
        modules = json.loads((ROOT / f"data/research/modules/{nation}.json").read_text(encoding="utf-8"))
        if stem not in modules.get("tanks", {}):
            problems.append(f"{nation}: 树外预览车 {stem} 缺 modules 条目")
        data = json.loads((TT / f"{nation}.json").read_text(encoding="utf-8"))
        if any(n["stem"] == stem for n in data["nodes"]):
            problems.append(f"{nation}: 树外预览车 {stem} 不应有 tree 节点")
        if any(item["stem"] == stem for item in data["pending"]):
            problems.append(f"{nation}: 树外预览车 {stem} 不应在 pending")

    assert not problems, "tech_tree 导出问题:\n" + "\n".join(problems)


if __name__ == "__main__":
    test_tech_tree_export()
    print("[ok] test_tech_tree_export")
    print("全部通过")
