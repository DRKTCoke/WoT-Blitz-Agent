#!/usr/bin/env python3
"""按 change_plan 应用数据版本更新（pack 级复用 + research 全平面同步）。

用法：
    python tools/plan_data_version.py --base <old Data> --new <new Data> --out plan.json \
        --base-pack-manifest base/manifest.json --new-pack-manifest new/manifest.json --strict
    python tools/apply_data_version.py --base-pack release/asset_pack_x --out-pack /tmp/new \
        --game-data <new Data> --bridge data/preview_tank_id_bridge.json \
        --plan plan.json [--allow-manual]

行为：
1. 用硬链接克隆 base pack（`research/**` 例外：复制，避免写入共享 inode 污染基座）；
2. 若 plan 命中 research 相关平面（vehicle-xml/shared-xml/techtree/strings），调用
   `sync_preview_research.py` 对 out/research 做全平面同步；
3. `--refresh-manifest` 重算 manifest（需要 `--data-version-dir` 提供 data_version.json）；
4. 覆盖门禁：plan 里出现 vehicle-params/vehicle-model/maps 等尚未自动 apply 的平面时
   fail-closed，并打印需要运行的完整重建命令；只有显式 `--allow-manual` 才继续。
5. 输出 `<out>.apply.json`，列出复用的 pack 文件数、research 变更、待人工平面。

设计约束：不猜测/不统计；plan 未命中的平面一律按基座复用；写入前断链由 sync 工具负责。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"
RESEARCH_PLANES = {"vehicle-xml", "shared-xml", "techtree", "strings"}
MANUAL_PLANES = {"vehicle-params", "vehicle-model", "tank-icons", "maps"}


def clone_pack(base: pathlib.Path, out: pathlib.Path) -> tuple[int, int]:
    if out.exists() and any(out.iterdir()):
        raise SystemExit(f"输出 pack 非空: {out}")
    linked = copied = 0
    for src in sorted(base.rglob("*")):
        if not src.is_file():
            continue
        rel = src.relative_to(base)
        if rel.name in {"manifest.json", "version.json", "delta.json"}:
            continue  # 发布元数据由新 release 重新 stamp，不复用旧硬链接
        dst = out / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        if rel.parts and rel.parts[0] == "research":
            shutil.copy2(src, dst)
            copied += 1
            continue
        try:
            os.link(src, dst)
            linked += 1
        except OSError:
            shutil.copy2(src, dst)
            copied += 1
    return linked, copied


def run(cmd: list[str]) -> None:
    print("+ " + " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=ROOT, check=True)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base-pack", required=True)
    ap.add_argument("--out-pack", required=True)
    ap.add_argument("--game-data", required=True)
    ap.add_argument("--bridge", default="data/preview_tank_id_bridge.json")
    ap.add_argument("--plan", required=True)
    ap.add_argument("--data-version-dir", default=None,
                    help="含 data_version.json 的目录（--refresh-manifest 用；缺省取 base pack 的 data/）")
    ap.add_argument("--allow-manual", action="store_true")
    args = ap.parse_args()

    base = pathlib.Path(args.base_pack)
    out = pathlib.Path(args.out_pack)
    plan = json.loads(pathlib.Path(args.plan).read_text(encoding="utf-8"))
    active = {name for name, data in plan["planes"].items()
              if data.get("added") or data.get("changed") or data.get("removed")}
    manual = sorted(active & MANUAL_PLANES)
    if manual and not args.allow_manual:
        print("需要人工/完整重建的数据面:", manual, file=sys.stderr)
        print("这些平面不能用 research-only 增量安全更新；先跑完整重建链，"
              "或显式 --allow-manual 以只应用 research 部分。", file=sys.stderr)
        raise SystemExit(2)

    linked, copied = clone_pack(base, out)
    print(f"pack clone: linked={linked} copied={copied} -> {out}")

    changed = []
    if active & RESEARCH_PLANES:
        sync = [sys.executable, "-I", str(TOOLS / "sync_preview_research.py"),
                "--game-data", str(pathlib.Path(args.game_data).resolve()),
                "--bridge", str(pathlib.Path(args.bridge).resolve()),
                "--base", str((base / "research").resolve()),
                "--out", str((out / "research").resolve())]
        run(sync)
        report_path = out / "research.sync.json"
        report = json.loads(report_path.read_text(encoding="utf-8"))
        changed = report["updated"] + report["new"]
        # sync 的旁路文件落在 pack 根（research 的 parent）：不属于包内容，refresh 前移除。
        for name in ("research.sync.json", "upgrade_top_hp.json"):
            path = out / name
            if path.is_file():
                path.unlink()

    dv_dir = pathlib.Path(args.data_version_dir) if args.data_version_dir else base / "data"
    if not (dv_dir / "data_version.json").is_file():
        raise SystemExit(f"缺少 data_version.json: {dv_dir}")
    # refresh manifest 需要 --data 指向含 data_version.json 的目录；data_version 本身
    # 若在 base pack 内，会与 out pack 的 data/data_version.json 不一致，先同步到 out。
    dv_doc = json.loads((dv_dir / "data_version.json").read_text(encoding="utf-8"))
    versions = plan.get("versions") or {}
    dv_doc["base_game_version"] = versions.get("base")
    dv_doc["new_game_version"] = versions.get("new")
    dv_doc["source_plan_sha256"] = hashlib.sha256(
        pathlib.Path(args.plan).read_bytes()).hexdigest()
    dv_doc["changed_planes"] = sorted(active)
    dv_doc["coverage"] = plan.get("coverage")
    target_dv = out / "data/data_version.json"
    target_dv.parent.mkdir(parents=True, exist_ok=True)
    if target_dv.exists():
        target_dv.unlink()
    target_dv.write_text(json.dumps(dv_doc, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    # 用一个隔离目录提供 data_version，避免 refresh 读 base pack 的旧文件。
    meta_dir = out.parent / (out.name + ".data_version")
    meta_dir.mkdir(parents=True, exist_ok=True)
    (meta_dir / "data_version.json").write_text(target_dv.read_text(encoding="utf-8"),
                                                encoding="utf-8")
    run([sys.executable, "-I", str(ROOT / "scripts/export_asset_pack.py"),
         "--out", str(out), "--refresh-manifest", "--data", str(meta_dir)])

    report = {
        "basePack": str(base), "outPack": str(out), "plan": args.plan,
        "activePlanes": sorted(active), "manualPlanes": manual,
        "linked": linked, "copied": copied, "researchChanged": changed,
    }
    report_path = out.with_name(out.name + ".apply.json")
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"apply -> {report_path}: researchChanged={len(changed)} manual={manual}")


if __name__ == "__main__":
    main()
