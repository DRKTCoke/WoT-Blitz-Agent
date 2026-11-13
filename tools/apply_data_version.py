#!/usr/bin/env python3
"""按 change_plan 增量应用数据版本更新：只重导变化项，其余硬链接复用。

用法：
    python tools/plan_data_version.py --base <old Data> --new <new Data> --out plan.json \
        --base-pack-manifest base/manifest.json --new-pack-manifest new/manifest.json --strict
    python tools/apply_data_version.py --base-pack release/asset_pack_x --out-pack /tmp/new \
        --game-data <new Data> --bridge data/preview_tank_id_bridge.json \
        --plan plan.json [--dry-run] [--only research,vehicle-assets,core,maps]

行为：
1. 用 pack_incremental.clone_base 硬链接克隆 base pack（research/** 复制，防止写穿）；
2. 按 plan 选择 handler：
   - research：sync_preview_research.py（modules/items/params/devices/camo/skills/strings）
   - vehicle-assets：逐车 GLB/悬挂/履带/图标，只导出 plan.tanks 中变化的 id
   - core：work/data 反向物化后按原 update-data/dump-tank-data 整表重算，只覆盖变化字节
   - maps：按变化 space 重导 GLB/ground/光照/IBL，2D/变体整表重跑（用户选定）
3. 生成 data_version 溯源并重算 manifest；
4. 输出 `<out>.apply.json`，逐 handler 记录 reused/updated/deleted。

设计约束：不猜测/不统计；未识别平面 fail-closed；任何写入先断链。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"
sys.path.insert(0, str(TOOLS))

import pack_incremental as P
from tank_image_overrides import apply_covers, load_catalog, matches_dataset
from data_version_handlers import (CoreHandler, MapsHandler, ResearchHandler,
                                   VehicleAssetsHandler, Context)
from plan_data_version import NON_DATA_PLANES
HANDLERS = (ResearchHandler(), VehicleAssetsHandler(), CoreHandler(), MapsHandler())


def _agent_bin(value: str | None) -> pathlib.Path:
    if value:
        return pathlib.Path(value).resolve()
    for name in ("wotb-agent.exe", "wotb-agent"):
        p = ROOT / "target" / "release" / name
        if p.is_file():
            return p
    return ROOT / "target" / "release" / ("wotb-agent.exe" if sys.platform == "win32"
                                           else "wotb-agent")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base-pack", required=True)
    ap.add_argument("--out-pack", required=True)
    ap.add_argument("--game-data", required=True)
    ap.add_argument("--bridge", default="data/preview_tank_id_bridge.json")
    ap.add_argument("--map-index", default="map_index.json")
    ap.add_argument("--plan", required=True)
    ap.add_argument("--data-version-dir", default=None)
    ap.add_argument("--dataset-id", help="WG 源 dataset ID；否则使用 plan 中的新版本/来源")
    ap.add_argument("--raw-tank-images", type=pathlib.Path,
                    help="WG 原始封面目录；缺省时按新 Data 重新导出覆盖目标")
    ap.add_argument("--agent-bin", default=None)
    ap.add_argument("--core-online", action="store_true",
                    help="让 update-data 下载/更新 pb；默认 offline，使用 base pb")
    ap.add_argument("--pb-dir", default=None,
                    help="预构建 pb 目录（tanks.pb+models.pb，含新车 roster）：core 物化 "
                         "work/data 后先覆盖再跑 update-data --offline；vehicle-assets 逐车"
                         "导出也读它（新车不在 base pb）。models.pb 变化须同批 --gun-mask-slices")
    ap.add_argument("--gun-mask-slices", default=None,
                    help="models.pb 变化时提供同批 gun_mask_slices.json")
    ap.add_argument("--render-dir", default="release/overhead-bake",
                    help="地图俯视渲染缓存目录")
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--only", default=None,
                    help="逗号分隔的 handler（research,vehicle-assets,core,maps）")
    ap.add_argument("--keep-work", action="store_true", help="保留 .work staging 供排障")
    args = ap.parse_args()

    base = pathlib.Path(args.base_pack).resolve()
    out = pathlib.Path(args.out_pack).resolve()
    plan_path = pathlib.Path(args.plan).resolve()
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    plan["_plan_path"] = str(plan_path)
    active = {name for name, data in plan["planes"].items()
              if data.get("added") or data.get("changed") or data.get("removed")}
    if "unmapped" in active:
        print("plan 含未识别数据面变化，拒绝 apply：", file=sys.stderr)
        for rel in (plan.get("unmappedChanged") or [])[:10]:
            print(f"  {rel}", file=sys.stderr)
        raise SystemExit(2)
    if plan.get("mapRegistryChanged"):
        raise SystemExit("maps.yaml/Strings 引起地图注册表变化：请走完整地图重建链")
    if plan.get("unresolvedTanks") or plan.get("unresolvedMaps"):
        raise SystemExit(f"plan 有未解析 tank/map：{plan.get('unresolvedTanks')[:5]} "
                         f"{plan.get('unresolvedMaps')[:5]}")
    known = set().union(*(h.planes for h in HANDLERS))
    unknown = active - known - NON_DATA_PLANES
    if unknown:
        raise SystemExit(f"没有 handler 的数据面: {sorted(unknown)}")

    selected = None
    if args.only:
        selected = {x.strip() for x in args.only.split(",") if x.strip()}
        unknown_handlers = selected - {h.name for h in HANDLERS}
        if unknown_handlers:
            raise SystemExit(f"未知 handler: {sorted(unknown_handlers)}")

    work = out.with_name(out.name + ".work")
    if not args.dry_run:
        if out.exists() and any(out.iterdir()):
            raise SystemExit(f"输出 pack 非空: {out}")
        if work.exists():
            shutil.rmtree(work)  # 上一次失败留下的 staging；不参与发布
        work.mkdir(parents=True)
        linked, copied = P.clone_base(base, out)
        print(f"pack clone: linked={linked} copied={copied} -> {out}")
        ctx = Context(base=base, out=out, game_data=pathlib.Path(args.game_data).resolve(),
                      plan=plan, bridge=pathlib.Path(args.bridge).resolve(),
                      map_index=pathlib.Path(args.map_index).resolve(), work=work,
                      agent_bin=_agent_bin(args.agent_bin), core_online=args.core_online,
                      render_dir=pathlib.Path(args.render_dir).resolve(),
                      gun_mask=pathlib.Path(args.gun_mask_slices).resolve()
                      if args.gun_mask_slices else None,
                      pb_dir=pathlib.Path(args.pb_dir).resolve()
                      if args.pb_dir else None,
                      jobs=max(1, args.jobs))
    else:
        ctx = Context(base=base, out=out, game_data=pathlib.Path(args.game_data).resolve(),
                      plan=plan, bridge=pathlib.Path(args.bridge).resolve(),
                      map_index=pathlib.Path(args.map_index).resolve(), work=work,
                      agent_bin=_agent_bin(args.agent_bin), core_online=args.core_online,
                      render_dir=pathlib.Path(args.render_dir).resolve(),
                      gun_mask=pathlib.Path(args.gun_mask_slices).resolve()
                      if args.gun_mask_slices else None,
                      pb_dir=pathlib.Path(args.pb_dir).resolve()
                      if args.pb_dir else None,
                      jobs=max(1, args.jobs), dry_run=True)

    for handler in HANDLERS:
        if not handler.active(ctx):
            continue
        if selected is not None and handler.name not in selected:
            continue
        print(f"=== {handler.name} ===", flush=True)
        handler.run(ctx)

    if args.dry_run:
        print(json.dumps({"dryRun": True, "active": sorted(active),
                          "handlers": ctx.report["handlers"]}, ensure_ascii=False, indent=1))
        return

    # data_version / manifest
    dv_dir = pathlib.Path(args.data_version_dir).resolve() if args.data_version_dir else None
    if dv_dir and (dv_dir / "data_version.json").is_file():
        source = dv_dir / "data_version.json"
    else:
        source = work / "data/data_version.json"
    if not source.is_file():
        source = base / "data/data_version.json"
    dv = json.loads(source.read_text(encoding="utf-8")) if source.is_file() else {}
    versions = plan.get("versions") or {}
    catalog = load_catalog()
    # An incremental plan may be based on a stamped custom pack. Re-read WG icons even if
    # no tank-icons plane changed: an inherited render must not silently outlive its rule.
    # The plan can inventory a local Data directory or the actual remote preview ZIP.
    source_input = plan.get("new") or ""
    preview_source = source_input if source_input.startswith(("https://", "http://")) else None
    if dv_dir and source == dv_dir / "data_version.json":
        preview_source = (dv.get("preview") or {}).get("source") or preview_source
    eligible = matches_dataset(catalog, args.dataset_id, preview_source, versions.get("new"))
    present = [tid for tid in catalog["vehicles"] if (out / "tank_images" / f"{tid}.webp").is_file()]
    if present:
        # Only special-case builds with a reviewed dataset or a base carrying our actual art.
        # On ordinary updates the vehicle handler has already installed the original icons.
        base_meta = base / "data/data_version.json"
        base_dv = json.loads(base_meta.read_text(encoding="utf-8")) if base_meta.is_file() else {}
        base_source = (base_dv.get("preview") or {}).get("source")
        base_eligible = matches_dataset(catalog, None, base_source, versions.get("base"))
        inherited = any(
            (base / "tank_images" / f"{tid}.webp").is_file()
            and P.sha256_file(base / "tank_images" / f"{tid}.webp") == catalog["vehicles"][tid]["sha256"]
            for tid in present)
        if eligible or base_eligible or inherited:
            raw = args.raw_tank_images
            if raw is None:
                raw = work / "raw-tank-images"
                command = [sys.executable, "-X", "utf8", "-I", str(TOOLS / "export_tank_icons.py"),
                           "--game-data", str(ctx.game_data), "--pb", str(out / "data/tanks.pb"),
                           "--out", str(raw), "--jobs", str(ctx.jobs)]
                for tid in present:
                    command.extend(["--tank", tid])
                subprocess.run(command, cwd=ROOT, check=True)
            ctx.report["handlers"]["tank-image-overrides"] = apply_covers(out, raw, eligible, catalog)
    dv.update({
        "base_game_version": versions.get("base"),
        "new_game_version": versions.get("new"),
        "source_plan_sha256": hashlib.sha256(plan_path.read_bytes()).hexdigest(),
        "changed_planes": sorted(active),
        "coverage": plan.get("coverage"),
        "scope": {
            "tanks": sorted(int(k) for k in (plan.get("tanks") or {})),
            "maps": sorted((plan.get("maps") or {}).keys()),
            "wholeTable": plan.get("wholeTable") or [],
        },
        "inputs": plan.get("pins") or {},
    })
    P.overlay_bytes(out, "data/data_version.json",
                    (json.dumps(dv, ensure_ascii=False, indent=1) + "\n").encode("utf-8"))
    meta_dir = out.with_name(out.name + ".manifest-data")
    meta_dir.mkdir(parents=True, exist_ok=True)
    (meta_dir / "data_version.json").write_text(
        json.dumps(dv, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print("+ manifest refresh", flush=True)
    subprocess.run([sys.executable, "-X", "utf8", "-I",
                    str(ROOT / "scripts/export_asset_pack.py"),
                    "--out", str(out), "--refresh-manifest", "--data", str(meta_dir)],
                   cwd=ROOT, check=True)

    report = {
        "basePack": str(base), "outPack": str(out), "plan": str(plan_path),
        "activePlanes": sorted(active), "handlers": ctx.report["handlers"],
        "work": str(work),
    }
    report_path = out.with_name(out.name + ".apply.json")
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n",
                           encoding="utf-8")
    if not args.keep_work:
        shutil.rmtree(work, ignore_errors=True)
    print(f"apply -> {report_path}: handlers={list(ctx.report['handlers'])}")


if __name__ == "__main__":
    main()
