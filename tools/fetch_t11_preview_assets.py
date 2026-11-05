#!/usr/bin/env python3
"""从 11.22 tier XI 超测预览 ZIP 按需物化一个可跑提取链的 Data 根（**最小变更范围**）。

范围控制（2026-11-05 t11p2 起）：对基线预览 ZIP 做两包 central directory 的 **CRC 对比**，
只拉"真正变化的成员"，而不是整包子集——

- **全量共享元数据**（都很小）：version / Strings(en,zh-Hans) / 9 国科技树 yaml /
  9 国 `list.xml` + 共享组件 / `camouflages.yaml` / `avatar.xml`；
- **每辆预览车（preview bridge）**：车辆 XML / Parameters / 主模型 .sc2+.scg / 贴图 /
  碰撞体 / 封面图标（数值/GLB/图标/悬挂/履带的输入）；
- **`--xml-list` 指定的车辆定义 XML**：CRC 对比得出的"本版相对基线有变化"的车辆，
  供科技树研发边的**逐车重算**（其余车辆 XML 逐字节未变，边按定义不变）；
- 3d/Tanks/Common 共享贴图（GLB 跨目录引用兜底）。

依然不做全量下载：ZIP64 central directory + HTTP Range 并发只取选中成员。
落盘布局 = zip 内 `app/Data/` 之后的部分（`version.txt.dvpl`、`XML/...`、`3d/...`），
与 `--game-data` 参数语义一致。fail-closed：成员缺失/解压失败/bridge 未命中即报错，
不写部分产物；只读远端与本地文件，不执行包内任何内容。

用法：
    python tools/fetch_t11_preview_assets.py --out <data-root> \
        [--xml-list .t11p2/changed_vehicle_xml.json] [--url ...]
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
from datetime import datetime, timezone

REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from fetch_t11_techtree import (  # noqa: E402
    DEFAULT_URL,
    RangeReader,
    find_eocd,
    parse_central_directory,
    read_member,
)
from wotb_sc2 import decode_dvpl  # noqa: E402

SKIP_MARKERS = ("/customization/", "/animations/", "/gfx/particles/", "/3d/fx/")


def select_members(entries: dict[str, tuple[int, int]],
                   bridge: dict[str, dict],
                   xml_list: list[str]) -> tuple[set[str], dict[str, list[str]]]:
    """返回 (选中的 zip 成员, {选中原因: 成员列表})，原因只用于 manifest 审计。"""
    selected: set[str] = set()
    reasons: dict[str, list[str]] = {}

    def take(name: str, reason: str) -> None:
        if name in selected:
            return
        selected.add(name)
        reasons.setdefault(reason, []).append(name)

    known = set(entries)
    for path in xml_list:
        if path not in known:
            raise RuntimeError(f"--xml-list 条目不在 zip 内: {path}")
        take(path, "vehicle-xml-list")

    for name in entries:
        low = name.lower()
        if low.endswith("data/version.txt.dvpl"):
            take(name, "version")
        elif low.endswith(("strings/en.yaml.dvpl", "strings/zh-hans.yaml.dvpl")):
            take(name, "strings")
        elif low.endswith("camouflages.yaml.dvpl"):
            take(name, "camouflages-registry")
        elif low.endswith("xml/item_defs/tankmen/avatar.xml.dvpl"):
            take(name, "avatar")
        elif "/configs/techtree/" in low and low.endswith("_tree.yaml.dvpl"):
            take(name, "techtree")
        elif "/xml/item_defs/vehicles/" in low and low.endswith(".xml.dvpl"):
            # list + 共享组件：研发边/模块联表/提取链的公共输入（全量 ~70 文件）。
            # 车辆**定义** XML 不在此处——只按 bridge 车 + --xml-list 显式拉取
            # （t11b↔t11p2 的 CRC 对比证明其余车辆逐字节未变）。
            if low.endswith("/list.xml.dvpl") or "/components/" in low:
                take(name, "vehicle-shared-xml")
        elif low.startswith("app/data/3d/tanks/common/"):
            # 共享贴图/履带印贴图（~1.6MB）：GLB 材质可能跨目录引用 Common/images。
            take(name, "tanks-common")

    by_tank: dict[str, list[str]] = {}
    for tid, v in sorted(bridge.items(), key=lambda kv: int(kv[0])):
        nation, stem = v["nation"], v["stem"]
        stem_low = stem.lower()
        tag_low = f"-{stem_low}"
        n_model = n_icon = 0
        for name in entries:
            low = name.lower()
            if "/3d/tanks/parameters/" in low and stem_low in low and low.endswith(".dvpl"):
                take(name, f"tank:{tid}:parameters")
            elif low.startswith("app/data/3d/tanks/") and stem_low in low \
                    and not any(m in low for m in SKIP_MARKERS) and low.endswith(".dvpl"):
                take(name, f"tank:{tid}:model")
                n_model += 1
            elif ("/gfx/ui/bigtankicons/" in low or "/battlescreenhud/smalltankicons/" in low) \
                    and tag_low in low and low.endswith(".dvpl"):
                take(name, "ui-icons")
                n_icon += 1
        if n_model == 0:
            raise RuntimeError(f"bridge tank {tid} ({nation}/{stem}): no model assets matched")
        if n_icon == 0:
            raise RuntimeError(f"bridge tank {tid} ({nation}/{stem}): no UI icons matched")
        by_tank[tid] = [nation, stem, n_model, n_icon]
    return selected, reasons


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--url", default=DEFAULT_URL)
    ap.add_argument("--out", required=True, help="目标 Data 根（必须不存在或为空）")
    ap.add_argument("--preview-bridge", default="data/preview_tank_id_bridge.json")
    ap.add_argument("--xml-list", default=None,
                    help="额外拉取的车辆定义 XML 路径清单（JSON 数组，zip 内完整路径）；"
                         "用于 CRC 对比得出的“本版相对基线有变化”的车辆，缺省只拉 bridge 车")
    ap.add_argument("--jobs", type=int, default=12, help="并发 Range 拉取线程数")
    args = ap.parse_args()

    bridge = json.loads((REPO_ROOT / args.preview_bridge).read_text(encoding="utf-8"))
    xml_list = json.loads((REPO_ROOT / args.xml_list).read_text(encoding="utf-8")) \
        if args.xml_list else []
    out = pathlib.Path(args.out)
    if out.exists() and any(out.iterdir()):
        raise RuntimeError(f"输出目录非空: {out}（换新目录，避免混入旧文件）")

    reader = RangeReader(args.url)
    print(f"zip size = {reader.length} bytes")
    cd_offset, cd_size, _total = find_eocd(reader)
    entries = parse_central_directory(reader, cd_offset, cd_size)
    print(f"central directory entries: {len(entries)}")
    selected, reasons = select_members(entries, bridge, xml_list)

    # bridge 每辆车都必须有 XML / Parameters（模型/图标命中数已在 select_members 判零）。
    for tid, v in sorted(bridge.items(), key=lambda kv: int(kv[0])):
        nation, stem = v["nation"], v["stem"]
        for label, suffix in (
            ("xml", f"xml/item_defs/vehicles/{nation}/{stem}.xml.dvpl".lower()),
            ("parameters", f"3d/tanks/parameters/{nation}/{stem}.yaml.dvpl".lower()),
        ):
            if not any(n.lower().endswith(suffix) for n in selected):
                raise RuntimeError(f"bridge tank {tid} ({nation}/{stem}): missing {label}")

    total_bytes = 0
    written = 0
    ordered = sorted(selected)

    def fetch_one(name: str) -> tuple[str, bytes]:
        return name, read_member(reader, entries, name)

    # 选中成员彼此独立：并发 Range 拉取（每成员 header+data 各一次请求），
    # 顺序写盘保持输出确定性。CDN 偶发限流由 RangeReader 的重试兜底。
    from concurrent.futures import ThreadPoolExecutor

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for done, (name, payload) in enumerate(pool.map(fetch_one, ordered), start=1):
            marker = name.lower().index("data/")
            rel = pathlib.Path(name[marker + len("data/"):])
            if any(part == ".." for part in rel.parts):
                raise RuntimeError(f"unsafe member path: {name}")
            dest = out / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(payload)
            total_bytes += len(payload)
            written += 1
            if done % 100 == 0:
                print(f"  {done}/{len(ordered)}", flush=True)

    # 锚定数据来源：version.txt 必须可解且带 t11 标记（与 techtree 工具同口径）。
    vname = next(n for n in selected if n.lower().endswith("data/version.txt.dvpl"))
    version_txt = decode_dvpl(read_member(reader, entries, vname)).decode("utf-8").strip()
    if "11.22_supertest_tier11" not in version_txt:
        raise RuntimeError(f"unexpected version.txt: {version_txt!r}")

    manifest = {
        "schema": 1,
        "source": args.url,
        "gameVersion": version_txt,
        "fetchedAt": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "zipSize": reader.length,
        "files": written,
        "bytes": total_bytes,
        "fetchedBytes": reader._fetched,
        "tanks": sorted(int(tid) for tid in bridge),
        "reasons": {k: len(v) for k, v in sorted(reasons.items())},
    }
    manifest_path = out.parent / f"{out.name}.manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"written {written} files / {total_bytes/1e6:.1f} MB -> {out}")
    print(f"version: {version_txt}")
    print(f"fetched {reader._fetched/1e6:.1f} MB via Range requests")
    print(f"manifest: {manifest_path}")


if __name__ == "__main__":
    main()
