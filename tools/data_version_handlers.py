#!/usr/bin/env python3
"""数据版本 apply 的 handler 注册表；每个 handler 只负责一个数据面的增量/复用。

维护约定：
- 所有转换调用原导出器/原 CLI，不在 Python 里重写 GLB、地图、pb 编码语义。
- handler 只写自己声明的 pack 路径；未变文件由 pack_incremental.overlay_* 按 sha256 复用。
- 目标集合内不再产出的文件必须删除（removed tank/map、丢失的 suspension/tread/icon）。
"""
from __future__ import annotations

import json
import os
import pathlib
import shutil
import subprocess
import sys
from dataclasses import dataclass, field

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"
sys.path.insert(0, str(TOOLS))

import pack_incremental as P
from plan_data_version import CORE_PLANES
ASSET_PLANES = {"vehicle-xml", "vehicle-params", "vehicle-model", "tank-icons"}
RESEARCH_PLANES = {"vehicle-xml", "vehicle-roster", "shared-xml", "techtree", "strings"}


@dataclass
class Context:
    base: pathlib.Path
    out: pathlib.Path
    game_data: pathlib.Path
    plan: dict
    bridge: pathlib.Path
    map_index: pathlib.Path
    work: pathlib.Path
    agent_bin: pathlib.Path
    render_dir: pathlib.Path = ROOT / "release/overhead-bake"
    gun_mask: pathlib.Path | None = None
    dry_run: bool = False
    core_online: bool = False
    jobs: int = 1
    report: dict = field(default_factory=lambda: {"handlers": {}})

    @property
    def active(self) -> set[str]:
        return {name for name, data in self.plan["planes"].items()
                if data.get("added") or data.get("changed") or data.get("removed")}

    def log(self, handler: str, message: str) -> None:
        print(f"[{handler}] {message}", flush=True)


def _run(ctx: Context, cmd: list[str], cwd: pathlib.Path | None = None) -> None:
    printable = " ".join(str(c) for c in cmd)
    if ctx.dry_run:
        print(f"  [dry-run] (cd {cwd or ROOT}) {printable}", flush=True)
        return
    print(f"  + (cd {cwd or ROOT}) {printable}", flush=True)
    rendered = [str(c) for c in cmd]
    # -I 会忽略 PYTHONIOENCODING；用 -X utf8 让导出器的中文/上标 print 在 GBK 控制台不崩。
    if len(rendered) >= 2 and rendered[0] == sys.executable and rendered[1] == "-I":
        rendered = [rendered[0], "-X", "utf8", *rendered[1:]]
    subprocess.run(rendered, cwd=cwd or ROOT, check=True)


def _tank_ids(plan: dict, kinds: set[str], *, removed_only: bool = False) -> list[int]:
    ids = []
    for tid, item in (plan.get("tanks") or {}).items():
        if not (set(item.get("kinds") or []) & kinds):
            continue
        files = item.get("files") or {}
        if removed_only:
            if files.get("removed") and not files.get("added") and not files.get("changed"):
                ids.append(int(tid))
            continue
        if files.get("added") or files.get("changed") or files.get("removed"):
            ids.append(int(tid))
    return sorted(ids)


class ResearchHandler:
    name = "research"
    planes = RESEARCH_PLANES

    def active(self, ctx: Context) -> bool:
        return bool(ctx.active & self.planes)

    def run(self, ctx: Context) -> None:
        if not self.active(ctx):
            return
        sync = [sys.executable, "-I", str(TOOLS / "sync_preview_research.py"),
                "--game-data", str(ctx.game_data), "--bridge", str(ctx.bridge),
                "--base", str(ctx.base / "research"), "--out", str(ctx.out / "research")]
        _run(ctx, sync)
        if ctx.dry_run:
            ctx.report["handlers"][self.name] = {"dryRun": True}
            return
        report_path = ctx.out / "research.sync.json"
        report = json.loads(report_path.read_text(encoding="utf-8"))
        changed = report["updated"] + report["new"]
        for name in ("research.sync.json", "upgrade_top_hp.json"):
            p = ctx.out / name
            if p.is_file():
                p.unlink()
        ctx.report["handlers"][self.name] = {"updated": len(changed), "files": changed}


class VehicleAssetsHandler:
    name = "vehicle-assets"
    planes = ASSET_PLANES

    def active(self, ctx: Context) -> bool:
        return bool(ctx.active & self.planes)

    def run(self, ctx: Context) -> None:
        if not self.active(ctx):
            return
        ids = _tank_ids(ctx.plan, {"vehicle-model", "vehicle-params", "tank-icons"})
        removed = _tank_ids(ctx.plan, {"vehicle-model", "vehicle-params", "tank-icons",
                                       "vehicle-xml"}, removed_only=True)
        ids = [t for t in ids if t not in set(removed)]
        work = ctx.work / "assets"
        models = work / "models"
        suspension = work / "suspension"
        tread = work / "tread"
        icons = work / "icons"
        if not ctx.dry_run:
            for d in (models, suspension, tread, icons):
                d.mkdir(parents=True, exist_ok=True)
        # crops 必须基于完整图标集重算；先复制 base 图标（不硬链接，避免导出器截断写穿）。
        if not ctx.dry_run and (ctx.base / "tank_images").is_dir():
            for src in (ctx.base / "tank_images").glob("*.webp"):
                shutil.copy2(src, icons / src.name)
        # 删除车先从图标工作集移除，避免 crops 保留旧条目。
        for tid in removed:
            for p in (icons / f"{tid}.webp",):
                p.unlink(missing_ok=True)

        model_ids = [t for t in ids
                     if "vehicle-model" in (ctx.plan["tanks"].get(str(t), {}).get("kinds") or [])]
        param_ids = [t for t in ids
                     if "vehicle-params" in (ctx.plan["tanks"].get(str(t), {}).get("kinds") or [])]
        icon_ids = [t for t in ids
                    if "tank-icons" in (ctx.plan["tanks"].get(str(t), {}).get("kinds") or [])]

        base_pb = ctx.base / "data/tanks.pb"
        if model_ids:
            cmd = [sys.executable, "-I", str(TOOLS / "export_tank_glb.py"),
                   "--game-data", str(ctx.game_data), "--pb", str(base_pb),
                   "--out", str(models), "--jobs", str(ctx.jobs)]
            for tid in model_ids:
                cmd += ["--tank", str(tid)]
            _run(ctx, cmd)
        if param_ids:
            cmd = [sys.executable, "-I", str(TOOLS / "export_tank_suspension.py"),
                   "--game-data", str(ctx.game_data), "--tanks-pb", str(base_pb),
                   "--out", str(suspension)]
            for tid in param_ids:
                cmd += ["--tank", str(tid)]
            _run(ctx, cmd)
            cmd = [sys.executable, "-I", str(TOOLS / "export_tank_tread.py"),
                   "--game-data", str(ctx.game_data), "--tanks-pb", str(base_pb),
                   "--out", str(tread), "--textures-out", str(work / "tread_textures")]
            for tid in param_ids:
                cmd += ["--tank", str(tid)]
            _run(ctx, cmd)
        if icon_ids or removed:
            cmd = [sys.executable, "-I", str(TOOLS / "export_tank_icons.py"),
                   "--game-data", str(ctx.game_data), "--pb", str(base_pb),
                   "--out", str(icons), "--jobs", str(ctx.jobs)]
            for tid in icon_ids:
                cmd += ["--tank", str(tid)]
            if icon_ids:
                _run(ctx, cmd)
            _run(ctx, [sys.executable, "-I", str(TOOLS / "export_tank_icons.py"),
                       "--emit-crops", "--out", str(icons),
                       "--crops-out", str(work / "crops.json")])

        if ctx.dry_run:
            ctx.report["handlers"][self.name] = {
                "dryRun": True, "tanks": ids, "removed": removed}
            return
        report = {"updated": 0, "reused": 0, "deleted": 0, "tanks": ids}
        for tid in ids:
            src = models / str(tid)
            if src.is_dir():
                P.sync_tree(ctx.out, f"glb/{tid}", src, report)
            else:
                # 模型导出失败时不得留下旧 GLB。
                if (ctx.out / "glb" / str(tid)).exists() and "vehicle-model" in (
                        ctx.plan["tanks"].get(str(tid), {}).get("kinds") or []):
                    raise SystemExit(f"缺少模型导出: {src}")
            for family, rel in (("suspension", f"suspension/{tid}.json"),
                                ("tread", f"tread/{tid}.json")):
                src_file = work / family / f"{tid}.json"
                if src_file.is_file():
                    action = P.overlay_file(ctx.out, rel, src_file)
                    report[action] = report.get(action, 0) + 1
                else:
                    if P.delete_path(ctx.out, rel):
                        report["deleted"] += 1
            icon = icons / f"{tid}.webp"
            if icon.is_file():
                action = P.overlay_file(ctx.out, f"tank_images/{tid}.webp", icon)
                report[action] = report.get(action, 0) + 1
            elif P.delete_path(ctx.out, f"tank_images/{tid}.webp"):
                report["deleted"] += 1
        # 全量纹理表：tread 导出器总是重写；按哈希覆盖。
        tex = work / "tread_textures"
        if tex.is_dir():
            for src in sorted(tex.glob("*.webp")):
                action = P.overlay_file(ctx.out, f"tread/textures/{src.name}", src)
                report[action] = report.get(action, 0) + 1
            if (tex / "textures.json").is_file():
                action = P.overlay_file(ctx.out, "tread/textures.json", tex / "textures.json")
                report[action] = report.get(action, 0) + 1
        crops = work / "crops.json"
        if crops.is_file():
            action = P.overlay_file(ctx.out, "tank_images/crops.json", crops)
            report[action] = report.get(action, 0) + 1
        # 删车：所有逐车资产。
        for tid in removed:
            for rel in (f"glb/{tid}", f"suspension/{tid}.json", f"tread/{tid}.json",
                        f"tank_images/{tid}.webp", f"tank/{tid}.json", f"game_data/{tid}.json"):
                if P.delete_path(ctx.out, rel):
                    report["deleted"] += 1
        ctx.report["handlers"][self.name] = report
        # 供 core dump 读取新 GLB。
        ctx.report.setdefault("_model_stage", {})["models"] = str(models)
        ctx.report["_model_stage"]["ids"] = model_ids


class CoreHandler:
    name = "core"
    planes = CORE_PLANES

    def active(self, ctx: Context) -> bool:
        return bool(ctx.active & self.planes)

    def _base_ids(self, ctx: Context) -> set[int]:
        path = ctx.base / "data/tank_cache.json"
        if not path.is_file():
            return set()
        return {int(k) for k in json.loads(path.read_text(encoding="utf-8"))}

    def run(self, ctx: Context) -> None:
        if not self.active(ctx):
            return
        if not ctx.dry_run and not ctx.agent_bin.is_file():
            raise SystemExit(f"缺少 agent 二进制 {ctx.agent_bin}；先 cargo build --release --bin wotb-agent")
        if not ctx.core_online and (ctx.active & {"vehicle-roster", "vehicle-components"}):
            raise SystemExit("roster/components 变化需要重建 pb；请准备新 pb 或加 --core-online")
        removed = _tank_ids(ctx.plan, {"vehicle-xml", "vehicle-params", "vehicle-model"},
                            removed_only=True)
        if removed and not ctx.core_online:
            raise SystemExit(f"tank 移除需要先更新 pb：{removed[:10]}；准备新 pb 或加 --core-online")
        work = ctx.work
        data = work / "data"
        if not ctx.dry_run:
            P.reverse_pack_to_data(ctx.base, work)
            # vehicle handler 产出的新 GLB 必须在 dump-tank-data 前进入 work cache。
            stage = (ctx.report.get("_model_stage") or {})
            models = pathlib.Path(stage.get("models", "")) if stage.get("models") else None
            for tid in stage.get("ids") or []:
                src = models / str(tid) if models else None
                if src and src.is_dir():
                    P.sync_tree(data / "cache/models", str(tid), src)
            # 新坦克/组件变化需要 pb 先更新；offline 且 base pb 缺 id 时 fail-closed。
            base_ids = self._base_ids(ctx)
            changed_ids = {int(t) for t in (ctx.plan.get("tanks") or {})}
            missing = sorted(changed_ids - base_ids)
            if missing and not ctx.core_online:
                raise SystemExit(
                    f"base pb 缺 tank id {missing[:10]}；先准备新 pb（--core-online 或更新 "
                    f"data/tanks.pb），否则 update-data 无法提取这些车")
        cmd = [str(ctx.agent_bin), "update-data", "--game-dir", str(ctx.game_data),
               "--output", "data/tanks.pb", "--tank-cache", "data/tank_cache.json",
               "--game-data", "data/game_data", "--force"]
        if not ctx.core_online:
            cmd.append("--offline")
        _run(ctx, cmd, cwd=work)
        _run(ctx, [str(ctx.agent_bin), "dump-tank-data", "--out", str(work / "tank_data")], cwd=work)
        _run(ctx, [str(ctx.agent_bin), "dump-shell-kinds", "--out", str(work / "shell_kinds.json")],
             cwd=work)
        if ctx.dry_run:
            ctx.report["handlers"][self.name] = {"dryRun": True, "work": str(work)}
            return

        report = {"updated": 0, "reused": 0, "deleted": 0}
        for rel, src in (
            ("data/tanks.pb", data / "tanks.pb"),
            ("data/models.pb", data / "models.pb"),
            ("data/tank_cache.json", data / "tank_cache.json"),
            ("data/data_version.json", data / "data_version.json"),
            ("data/shellKinds.json", work / "shell_kinds.json"),
        ):
            if src.is_file():
                action = P.overlay_file(ctx.out, rel, src)
                report[action] = report.get(action, 0) + 1
        # gun mask 必须与 models.pb 同批；pb 未变时安全复用 base 文件。
        gm = ctx.base / "data/gunMaskSlices.json"
        if P.same_bytes(gm, ctx.out / "data/gunMaskSlices.json"):
            report["reused"] += 1
        elif ctx.gun_mask and ctx.gun_mask.is_file():
            action = P.overlay_file(ctx.out, "data/gunMaskSlices.json", ctx.gun_mask)
            report[action] = report.get(action, 0) + 1
        elif gm.is_file() and P.sha256_file(data / "models.pb") == P.sha256_file(ctx.base / "data/models.pb"):
            action = P.overlay_file(ctx.out, "data/gunMaskSlices.json", gm)
            report[action] = report.get(action, 0) + 1
        else:
            raise SystemExit("models.pb 变化但未提供 gunMaskSlices；先跑 tools/emit_vehicle_pb.py "
                             "或传 --gun-mask-slices")
        if (data / "game_data").is_dir():
            for src in sorted((data / "game_data").glob("*.json")):
                action = P.overlay_file(ctx.out, f"game_data/{src.name}", src)
                report[action] = report.get(action, 0) + 1
        tank_data = work / "tank_data"
        if tank_data.is_dir():
            for src in sorted(tank_data.glob("*.json")):
                action = P.overlay_file(ctx.out, f"tank/{src.name}", src)
                report[action] = report.get(action, 0) + 1
        # tank_names 派生。
        cache = data / "tank_cache.json"
        if cache.is_file():
            names = {}
            for tid, info in json.loads(cache.read_text(encoding="utf-8")).items():
                name = (info or {}).get("name")
                if name:
                    names[str(tid)] = name
            action = P.overlay_bytes(ctx.out, "data/tank_names.json",
                                     json.dumps(names, ensure_ascii=False, sort_keys=True)
                                     .encode("utf-8"))
            report[action] = report.get(action, 0) + 1
        # 删除车：tank/game_data（资产由 vehicle handler 处理）。
        for tid in _tank_ids(ctx.plan, {"vehicle-xml", "vehicle-params", "vehicle-model"},
                             removed_only=True):
            for rel in (f"tank/{tid}.json", f"game_data/{tid}.json"):
                if P.delete_path(ctx.out, rel):
                    report["deleted"] += 1
        ctx.report["handlers"][self.name] = report


def _first(paths: list[pathlib.Path]) -> pathlib.Path | None:
    return next((p for p in paths if p.is_file()), None)


def _copy_first(out: pathlib.Path, rel: str, srcs: list[pathlib.Path],
                report: dict[str, int], *, force: bool = False) -> bool:
    src = _first(srcs)
    if src is None:
        return False
    if force:
        P.force_copy(out, rel, src)
        report["updated"] = report.get("updated", 0) + 1
    else:
        action = P.overlay_file(out, rel, src)
        report[action] = report.get(action, 0) + 1
    return True


def _refresh_terrain_json(out: pathlib.Path, key: str, space: str, cache: pathlib.Path,
                          report: dict[str, int]) -> None:
    """沿用打包器口径，用已有 terrain bin + 新 sidecar 的世界尺度刷新 terrain.json。"""
    mdir = out / "map" / key
    bin_path = mdir / "terrain.u16.bin"
    sidecar = cache / f"{space}.json"
    if not bin_path.is_file() or not sidecar.is_file():
        return
    doc = json.loads(sidecar.read_text(encoding="utf-8"))
    b = doc.get("worldBounds") or {}
    mn, mx = b.get("min"), b.get("max")
    if not (isinstance(mn, list) and isinstance(mx, list) and len(mn) >= 3 and len(mx) >= 3):
        return
    side: dict = {}
    terrain_json = mdir / "terrain.json"
    if terrain_json.is_file():
        try:
            side = json.loads(terrain_json.read_text(encoding="utf-8"))
        except Exception:
            side = {}
    span = max(float(mx[0]) - float(mn[0]), float(mx[1]) - float(mn[1]))
    side.update({"size": 512, "zmax": round(float(mx[2]), 1),
                 "zmin": round(float(mn[2]), 1), "span": round(span, 1),
                 "terrainBinSha256": P.sha256_file(bin_path)})
    action = P.overlay_bytes(out, f"map/{key}/terrain.json",
                             json.dumps(side, ensure_ascii=False).encode("utf-8"))
    report[action] = report.get(action, 0) + 1


def _materialize_map(out: pathlib.Path, key: str, space: str, cache: pathlib.Path,
                     report: dict[str, int]) -> bool:
    """把 cache 中一个 space/key 的产物按打包器优先级覆盖到 pack/map/<key>/。

    返回是否写入了 ground（只有写入时才需要 composite，且写前已断链）。
    """
    mdir = out / "map" / key
    # ground 必须断链，composite 会原地写。
    ground = _copy_first(out, f"map/{key}/ground.webp",
                         [ROOT / "data/maps" / f"{key}.webp",
                          ROOT / "data/maps" / f"{key}.png",
                          ROOT / "data/maps" / f"{key}.jpg",
                          ROOT / "data/maps" / f"{key}.jpeg",
                          cache / f"{space}.ground.webp"], report, force=True)
    _copy_first(out, f"map/{key}/mini.webp", [cache / f"{space}.minimap.webp"], report)
    _copy_first(out, f"map/{key}/scenery.glb", [cache / f"{space}.glb"], report)
    _copy_first(out, f"map/{key}/destructibles.json",
                [cache / key / "destructibles.json"], report)
    _copy_first(out, f"map/{key}/lighting.json", [cache / f"{space}.lighting.json"], report)
    _copy_first(out, f"map/{key}/ibl.webp", [cache / f"{space}.ibl.webp"], report)
    _copy_first(out, f"map/{key}/ground.layers.json",
                [cache / f"{space}.ground.layers.json"], report)
    for layer in ("cm", "lm", "tile0", "tile1", "mask0", "mask1", "hmap0", "hmap1"):
        _copy_first(out, f"map/{key}/ground/{layer}.webp",
                    [cache / f"{space}.ground.{layer}.webp"], report)
    _refresh_terrain_json(out, key, space, cache, report)
    return ground


class MapsHandler:
    name = "maps"
    planes = {"maps"}

    def active(self, ctx: Context) -> bool:
        return bool(ctx.active & self.planes)

    def run(self, ctx: Context) -> None:
        if not self.active(ctx):
            return
        if ctx.plan.get("mapRegistryChanged"):
            raise SystemExit("maps.yaml/Strings 引起地图注册表变化：请走完整地图重建链")
        registry = json.loads(ctx.map_index.read_text(encoding="utf-8"))
        by_space: dict[str, list[dict]] = {}
        for row in registry:
            by_space.setdefault(row["space"], []).append(row)
        work_maps = ctx.work / "maps"
        if not ctx.dry_run:
            work_maps.mkdir(parents=True, exist_ok=True)
        report = {"updated": 0, "reused": 0, "deleted": 0}
        # 全表 base key：变体重建会删除所有非 base 目录，不能只保护变化 space。
        root = ctx.base if ctx.dry_run else ctx.out
        base_by_space: dict[str, str] = {}
        for space, rows in by_space.items():
            existing = [r["key"] for r in rows if (root / "map" / r["key"]).is_dir()]
            if existing:
                base_by_space[space] = existing[0]
        changed_spaces = set(ctx.plan.get("maps") or {})
        ground_by_space: dict[str, bool] = {}
        for space in sorted(changed_spaces):
            rows = by_space.get(space) or []
            if not rows or space not in base_by_space:
                raise SystemExit(f"地图 {space}: pack 中没有已物化 key")
            base_key = base_by_space[space]
            cmd = [sys.executable, "-I", str(TOOLS / "export_map_glb.py"),
                   "--map", base_key, "--game-data", str(ctx.game_data),
                   "--output-dir", str(work_maps), "--jobs", str(ctx.jobs)]
            _run(ctx, cmd)
            _run(ctx, [sys.executable, "-I", str(TOOLS / "export_map_lighting.py"),
                       "--map", base_key, "--game-data", str(ctx.game_data),
                       "--out-dir", str(work_maps)])
            _run(ctx, [sys.executable, "-I", str(TOOLS / "export_map_ibl.py"),
                       "--map", base_key, "--game-data", str(ctx.game_data),
                       "--out-dir", str(work_maps)])
            if ctx.dry_run:
                continue
            ground_by_space[space] = _materialize_map(ctx.out, base_key, space, work_maps,
                                                       report)

        if not ctx.dry_run:
            # 未变化 space 的 destructibles 直接复用 base pack（variant 目录优先），
            # 避免为了整表变体重建而重导全客户端。
            for space, rows in by_space.items():
                if space in changed_spaces:
                    continue
                base_key = base_by_space.get(space)
                for row in rows:
                    src = ctx.out / "map" / row["key"] / "destructibles.json"
                    if not src.is_file() and base_key:
                        src = ctx.out / "map" / base_key / "destructibles.json"
                    if src.is_file():
                        dst = work_maps / row["key"] / "destructibles.json"
                        dst.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(src, dst)
        # 变化 space：重导该 space 全部 key 的 destructibles，供整表变体重建。
        cmd = [sys.executable, "-I", str(TOOLS / "export_map_destructibles.py"),
               "--game-data", str(ctx.game_data), "--output-dir", str(work_maps)]
        for row in registry:
            if row["space"] in changed_spaces:
                cmd += ["--map", row["key"]]
        if any(row["space"] in changed_spaces for row in registry):
            _run(ctx, cmd)

        if ctx.dry_run:
            ctx.report["handlers"][self.name] = {"dryRun": True,
                                                 "spaces": sorted(ctx.plan.get("maps") or {})}
            return
        # ground 合成：render 缺失即失败，不能把 raw ground 当作成品发布。
        for space in sorted(ctx.plan.get("maps") or {}):
            if not ground_by_space.get(space):
                continue  # 只改了 scenery/光照等，ground 未写入，不能对共享 inode 再合成
            key = base_by_space[space]
            render = ctx.render_dir / f"{key}.rgba"
            if not render.is_file():
                raise SystemExit(f"地图 {key}: 缺 overhead render {render}，先重新烘焙")
            _run(ctx, [sys.executable, "-I", str(TOOLS / "composite_overhead.py"),
                       "--pack", str(ctx.out), "--map", key,
                       "--render-dir", str(ctx.render_dir),
                       "--base-dir", str(work_maps), "--map-index", str(ctx.map_index),
                       "--write"])
        # materialize 会原地写 index.json；先断链，避免写穿 base pack。
        if (ctx.out / "index.json").is_file():
            P.force_copy(ctx.out, "index.json", ctx.out / "index.json")
        # 变体整表重跑：先删除所有非 base 目录，再由原工具重建。
        for d in sorted((ctx.out / "map").iterdir(), reverse=True):
            if d.is_dir() and d.name not in set(base_by_space.values()):
                shutil.rmtree(d)
                report["deleted"] += 1
        _run(ctx, [sys.executable, "-I", str(TOOLS / "materialize_destructible_variants.py"),
                   "--pack", str(ctx.out), "--map-index", str(ctx.map_index),
                   "--cache-maps", str(work_maps)])
        # 2D/封面整表重跑：清掉旧 key 集，避免 stale webp 让 preflight/索引不一致。
        for sub in ("map2d", "cover"):
            d = ctx.out / sub
            if d.is_dir():
                for p in d.iterdir():
                    if p.is_file():
                        p.unlink()
                        report["deleted"] += 1
        _run(ctx, [sys.executable, "-I", str(TOOLS / "export_2d_basemaps.py"),
                   "--pack", str(ctx.out), "--out", str(ctx.work / "2d"),
                   "--map2d-dir", str(ctx.out / "map2d"),
                   "--manifest", str(ctx.work / "2d-manifest.json")])
        ctx.report["handlers"][self.name] = report
