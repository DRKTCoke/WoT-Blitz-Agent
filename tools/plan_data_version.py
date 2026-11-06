#!/usr/bin/env python3
"""数据版本全量差异计划：对比“默认版本（上一个游戏客户端）”与“新客户端”。

用法：
    python tools/plan_data_version.py \
        --base /path/to/old/Data --new /path/to/new/Data \
        --out change_plan.json [--strict]

也支持 ZIP（本地文件或 http(s) URL，读取 central directory，不下载内容）：
    python tools/plan_data_version.py --base old.zip --new https://.../new.zip ...

产物 `change_plan.json`：
- 全量 inventory（目录 = sha256；ZIP = 成员 CRC32 + size）；
- 按 plane 分类的 added / changed / removed；
- 变化车辆 stem、变化地图键候选；
- `unmappedChanged`：没有导出规则的数据文件（strict 下失败）；
- 可选 pack manifest 覆盖检查：`missingOutput` / `orphanOutput`。

设计原则：分类来自路径语义（plane registry），不做内容统计；未识别的变化 fail-closed
（strict），避免“新版本某个数据面静默漏导”。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sys
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"
sys.path.insert(0, str(TOOLS))

# 路径 → 数据面。顺序敏感：先具体后通用。
PLANES: list[tuple[str, re.Pattern[str], tuple[str, ...]]] = [
    ("vehicle-xml", re.compile(r"^XML/item_defs/vehicles/(?!common/)([^/]+)/([^/]+)\.xml\.dvpl$"),
     ("tank/", "game_data/", "data/tanks.pb", "data/tank_cache.json", "research/modules/",
      "research/tech_tree/")),
    ("vehicle-params", re.compile(r"^3d/Tanks/Parameters/([^/]+)/([^/]+)\.yaml\.dvpl$"),
     ("game_data/", "glb/", "suspension/", "tread/")),
    ("vehicle-model", re.compile(r"^3d/Tanks/(?!Parameters/)(?:[^/]+/)?([^/]+)"),
     ("glb/", "tank_images/")),
    ("shared-xml", re.compile(r"^XML/item_defs/vehicles/common/"),
     ("research/",)),
    ("techtree", re.compile(r"^Configs/TechTree/.*_tree\.yaml\.dvpl$"),
     ("research/tech_tree/",)),
    ("strings", re.compile(r"^Strings/(?:en|zh-Hans)\.yaml\.dvpl$"),
     ("research/strings/",)),
    ("tank-icons", re.compile(r"^Gfx/UI/(?:BigTankIcons|BattleScreenHUD/SmallTankIcons)/"),
     ("tank_images/", "cover/")),
    ("maps", re.compile(r"^(?:maps/|3d/Maps/)"),
     ("map/", "map2d/")),
    ("ui", re.compile(r"^(?:Gfx/|UI/|Audio/|Shaders/|Fonts/|HtmlDocuments/|Cinema/|"
                      r"Videos/|Sounds/|Packs/|DAVAProject/)"),
     ()),
]
NON_DATA_PLANES = {"ui"}


def digest_dir(root: pathlib.Path) -> dict[str, dict]:
    import zlib
    out: dict[str, dict] = {}
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(root).as_posix()
        data = path.read_bytes()
        out[rel] = {
            "sha256": hashlib.sha256(data).hexdigest(),
            "crc32": f"{zlib.crc32(data) & 0xFFFFFFFF:08x}",
            "size": len(data),
        }
    return out


def digest_zip(path_or_url: str) -> dict[str, dict]:
    if path_or_url.startswith(("http://", "https://")):
        from fetch_t11_techtree import RangeReader, find_eocd, parse_central_directory
        reader = RangeReader(path_or_url)
        cd_off, cd_size, _total = find_eocd(reader)
        entries = parse_central_directory(reader, cd_off, cd_size, with_meta=True)
        return {
            _norm_zip_name(name): {"sha256": None, "crc32": f"{meta[2]:08x}", "size": meta[3]}
            for name, meta in entries.items()
        }
    out = {}
    with zipfile.ZipFile(path_or_url) as zf:
        for info in zf.infolist():
            if info.is_dir():
                continue
            out[_norm_zip_name(info.filename)] = {
                "sha256": None, "crc32": f"{info.CRC:08x}", "size": info.file_size,
            }
    return out


def _norm_zip_name(name: str) -> str:
    for prefix in ("app/Data/", "Data/"):
        if name.startswith(prefix):
            return name[len(prefix):]
    return name


def inventory(path: str) -> dict[str, tuple[str, int]]:
    p = pathlib.Path(path)
    if p.is_dir():
        return digest_dir(p)
    return digest_zip(path)


def classify(rel: str) -> tuple[str, dict]:
    for plane, pattern, outputs in PLANES:
        match = pattern.match(rel)
        if match:
            detail: dict = {}
            groups = match.groups()
            if plane == "vehicle-xml" and len(groups) >= 2:
                detail = {"nation": groups[0], "stem": groups[1]}
            elif plane == "vehicle-params" and len(groups) >= 2:
                detail = {"nation": groups[0], "stem": groups[1]}
            elif plane == "vehicle-model" and groups:
                detail = {"stem": groups[0]}
            return plane, detail
    return "unmapped", {}


def same_entry(a: dict, b: dict) -> bool:
    """同格式优先 sha256；目录 vs ZIP 时退回 CRC32 + 未压缩大小。"""
    if a.get("sha256") and b.get("sha256"):
        return a["sha256"] == b["sha256"]
    return a["crc32"] == b["crc32"] and a["size"] == b["size"]


def _pack_manifest(path: str | None) -> dict[str, str] | None:
    if not path:
        return None
    return {entry["path"]: entry["sha256"]
            for entry in json.loads(pathlib.Path(path).read_text(encoding="utf-8"))["files"]}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base", required=True, help="默认版本 Data 根或 ZIP/URL")
    ap.add_argument("--new", required=True, help="新版本 Data 根或 ZIP/URL")
    ap.add_argument("--base-version", default=None, help="默认版本号（写入 plan/data_version 溯源）")
    ap.add_argument("--new-version", default=None, help="新版本号")
    ap.add_argument("--out", required=True, help="change_plan.json 输出路径")
    ap.add_argument("--base-pack-manifest", default=None)
    ap.add_argument("--new-pack-manifest", default=None)
    ap.add_argument("--strict", action="store_true",
                    help="unmapped/missing-output/orphan-output 非零退出")
    args = ap.parse_args()

    base = inventory(args.base)
    new = inventory(args.new)
    added = sorted(set(new) - set(base))
    removed = sorted(set(base) - set(new))
    changed = sorted(p for p in set(base) & set(new) if not same_entry(base[p], new[p]))

    planes: dict[str, dict] = {}
    unmapped: list[str] = []
    vehicle_stems: dict[str, dict[str, list[str]]] = {}
    for kind, paths in (("added", added), ("changed", changed), ("removed", removed)):
        for rel in paths:
            plane, detail = classify(rel)
            entry = planes.setdefault(plane, {
                "added": [], "changed": [], "removed": [],
                "outputPrefixes": next((o for n, _p, o in PLANES if n == plane), ()),
            })
            entry[kind].append(rel)
            if plane == "unmapped":
                unmapped.append(rel)
            if "stem" in detail:
                bucket = vehicle_stems.setdefault(detail["stem"], {"nation": detail.get("nation"),
                                                                   "added": [], "changed": [],
                                                                   "removed": []})
                bucket[kind].append(rel)

    base_manifest = _pack_manifest(args.base_pack_manifest)
    new_manifest = _pack_manifest(args.new_pack_manifest)
    coverage = None
    if base_manifest is not None and new_manifest is not None:
        changed_outputs = {p for p, sha in new_manifest.items() if base_manifest.get(p) != sha}
        missing = []
        for plane, data in planes.items():
            if plane in NON_DATA_PLANES or plane == "unmapped" or not data["outputPrefixes"]:
                continue
            if not (data["added"] or data["changed"] or data["removed"]):
                continue
            if not any(any(out.startswith(prefix) for prefix in data["outputPrefixes"])
                       for out in changed_outputs):
                missing.append({"plane": plane, "outputPrefixes": list(data["outputPrefixes"]),
                                "changedSources": data["added"] + data["changed"] + data["removed"][:5]})
        # 输出变化必须能追溯到某个数据面（派生索引/manifest 除外）。
        derived = ("manifest.json", "version.json", "delta.json", "index.json",
                   "map2d/index.json", "cover/index.json")
        source_planes = {p for p, d in planes.items()
                         if p not in NON_DATA_PLANES and p != "unmapped"
                         and (d["added"] or d["changed"] or d["removed"])}
        all_prefixes = [prefix for plane, data in planes.items()
                        if plane in source_planes for prefix in data["outputPrefixes"]]
        orphan = [out for out in sorted(changed_outputs)
                  if out not in derived and not any(out.startswith(p) for p in all_prefixes)]
        coverage = {
            "changedOutputs": len(changed_outputs),
            "missingOutput": missing,
            "orphanOutput": orphan,
        }

    plan = {
        "schema": 1,
        "base": args.base,
        "new": args.new,
        "versions": {"base": args.base_version, "new": args.new_version},
        "counts": {
            "added": len(added), "changed": len(changed), "removed": len(removed),
            "unmapped": len(unmapped),
        },
        "planes": {
            name: {
                "added": len(data["added"]),
                "changed": len(data["changed"]),
                "removed": len(data["removed"]),
                "outputPrefixes": list(data["outputPrefixes"]),
                "files": {k: data[k] for k in ("added", "changed", "removed") if data[k]},
            }
            for name, data in sorted(planes.items())
        },
        "vehicles": vehicle_stems,
        "unmappedChanged": sorted(unmapped),
        "coverage": coverage,
        "limitations": [],
    }
    pathlib.Path(args.out).write_text(json.dumps(plan, ensure_ascii=False, indent=1) + "\n",
                                      encoding="utf-8")
    print(f"plan -> {args.out}: added={len(added)} changed={len(changed)} "
          f"removed={len(removed)} unmapped={len(unmapped)}")
    for name, data in plan["planes"].items():
        print(f"  {name:16s} +{data['added']:4d} ~{data['changed']:4d} -{data['removed']:4d}")
    if coverage:
        print(f"  coverage: changedOutputs={coverage['changedOutputs']} "
              f"missingOutput={len(coverage['missingOutput'])} "
              f"orphanOutput={len(coverage['orphanOutput'])}")
    if args.strict:
        problems = []
        if unmapped:
            problems.append(f"unmapped changed files: {unmapped[:10]}")
        if coverage:
            if coverage["missingOutput"]:
                problems.append(f"missing output for planes: {[m['plane'] for m in coverage['missingOutput']]}")
            if coverage["orphanOutput"]:
                problems.append(f"output changed without source: {coverage['orphanOutput'][:10]}")
        if problems:
            raise SystemExit("data version plan strict failure:\n  - " + "\n  - ".join(problems))


if __name__ == "__main__":
    main()
