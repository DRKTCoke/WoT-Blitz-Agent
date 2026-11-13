#!/usr/bin/env python3
"""导出纯静态资产包（架构契约 §13；供对象存储/静态托管直传）。

把 Agent 资产端点的"文件选择逻辑"在打包时执行一次，产物是傻瓜静态主机
都能伺服的目录树——前端配置 `?assets=<部署域名>` 后按静态路径取用：

    <包根>/
    ├── index.json                      # 数字 id → key/space/display（前端一次性装载）
    ├── glb/{tank_id}/model.glb, collision.glb
    ├── tank_images/{id}.webp
    ├── tank_images/crops.json          # 逐车封面图内容框（展示层缩放定位；与 tank_images/ 1:1，
    │                                   #   源 data/cache/tank_icon_crops.json ← export_tank_icons --emit-crops）
    ├── suspension/{tank_id}.json       # 逐车悬挂（tools/export_tank_suspension.py，additive）
    ├── data/{tanks.pb, models.pb, tank_cache.json, data_version.json, shellKinds.json,
    │         gunMaskSlices.json}
    ├── research/**                     # 坦克百科配置器数据（data/research 非图标部分：全局目录/
    │                                   #   物品/技能/params/strings + 逐车 modules；图标留 WotbTools）
    └── map/{key}/
        ├── ground.webp                 # /api/playback/map 的高清底图答案
        │                               # ⚠️ 本脚本会用 client colormap 重写 ground.webp、
        │                               #   覆盖掉俯视重合成结果——打包后必须重跑
        │                               #   `python tools/composite_overhead.py --pack <pack> --all
        │                               #   --write --render-dir release/overhead-bake
        │                               #   --base-dir data/cache/maps`，再 `--refresh-manifest`
        ├── mini.webp                   # res=mini 的答案
        ├── terrain.u16.bin             # /api/playback/terrain 的答案
        ├── terrain.json                # X-Terrain-Meta 头的 sidecar 化 {size,zmax,zmin,span}
        ├── scenery.glb                 # 场景 GLB
        ├── ground.layers.json          # groundmeta 的答案
        └── ground/{cm,lm,tile0,tile1,mask0,mask1,hmap0,hmap1}.webp  # groundtex 的答案

用法：
    wotb-agent dump-map-index > map_index.json      # 需游戏客户端在场（注册表源）
    wotb-agent dump-shell-kinds --out data/shell_kinds.json   # 弹种反解表（须与 data/tanks.pb 同版）
    python scripts/export_asset_pack.py --map-index map_index.json
    # --out 缺省为仓库根 release/asset_pack；如显式指定请用绝对路径——
    # 相对路径按 cwd 解析，曾在 frontend/dist 下误落 1.3GB 副本。

仅打包已存在的文件（缺卷积贴图/场景的地图自动缩量）；manifest.json 附全量 sha256。
"""

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PROJECT_ROOT / "tools"))
from pack_incremental import overlay_bytes
from tank_image_overrides import apply_covers, load_catalog, matches_dataset

GROUND_LAYERS = ["cm", "lm", "tile0", "tile1", "mask0", "mask1", "hmap0", "hmap1"]


def sha256(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def copy_first(srcs: list[Path], dst: Path) -> bool:
    """按优先级拷贝首个存在的来源（镜像服务端回退链）；返回是否命中。"""
    for s in srcs:
        if s.is_file():
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(s, dst)
            return True
    return False


def git_provenance() -> dict:
    """包的来源：生成它的 upstream commit，以及生成时工作区是否干净。

    `worktree_dirty=True` 的包**不等于**该 commit——消费方据此判断能否用包内的
    sha256 去对照某个 commit 复现。取不到 git（非仓库/无 git）时返回空 dict，
    不因此中断打包（旧行为）。
    """
    try:
        sha = subprocess.run(["git", "rev-parse", "HEAD"], cwd=PROJECT_ROOT,
                             capture_output=True, text=True, check=True).stdout.strip()
        dirty = bool(subprocess.run(["git", "status", "--porcelain"], cwd=PROJECT_ROOT,
                                    capture_output=True, text=True, check=True).stdout.strip())
    except Exception:
        return {}
    return {"upstream_commit": sha, "worktree_dirty": dirty}


def refresh_manifest(out: Path, data: Path) -> int:
    """按包目录**现有内容**重算 manifest.json（不重新打包）。

    为什么需要：`tools/composite_overhead.py --write` 在打包器之后改写
    `map/*/ground.webp`，manifest 因此记的是"合成前"哈希（data-inventory §2.1 入档）。
    上传器（tools/upload_asset_pack_cos.py）的跳过判定以**桶内 manifest 的 sha256**
    为准——条目与包内容失配会导致无谓重传（36 张 ground）。
    上传前跑一次本模式即让 manifest 与包内容自洽。
    """
    if not out.is_dir():
        print(f"!! 包目录不存在: {out}", file=sys.stderr)
        return 2
    provenance = git_provenance()
    data_version = None
    dv_path = data / "data_version.json"
    if dv_path.is_file():
        try:
            data_version = json.loads(dv_path.read_text(encoding="utf-8"))
        except Exception:
            data_version = None
    entries: list[dict] = []
    # `overhead/` = frontend/scripts/bake-ground-overhead.mjs 落进包目录的**中间渲染件**
    # （36×2 个文件 ≈ 2.4 GB），不属于发布内容：打包器从不生成它，但重算模式若把它登记进
    # manifest，manifest 驱动的差分上传会把 2.4 GB 推上 COS（2026-10-09 事故复现路径）。
    skip_dirs = {out / "overhead"}
    for f in sorted(out.rglob("*")):
        if any(d in f.parents for d in skip_dirs):
            continue
        # manifest 不登记自己；version.json / delta.json 是发布元数据（asset_release.py
        # 在 manifest 之后写），也不能进内容清单——否则重算一次 manifest 就会把
        # content_digest / manifest_sha256 打乱（dataset 版本管理契约）。
        if not f.is_file() or f.name in {"manifest.json", "version.json", "delta.json"}:
            continue
        rel = f.relative_to(out).as_posix()
        entries.append({"path": rel, "bytes": f.stat().st_size, "sha256": sha256(f)})
    manifest = {
        "version": 1,
        "generated": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        **provenance,
        "data_version": data_version,
        "files": entries,
    }
    # Candidate directories can share hard links with an immutable release.
    overlay_bytes(out, "manifest.json", json.dumps(manifest, ensure_ascii=False, indent=1).encode("utf-8"))
    mb = sum(e["bytes"] for e in entries) / 1e6
    print(f"manifest.json 已按包内容重算: {len(entries)} 文件 / {mb:.1f}MB → {out / 'manifest.json'}")
    return 0


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, default=PROJECT_ROOT / "release" / "asset_pack")
    ap.add_argument("--data", type=Path, default=PROJECT_ROOT / "data")
    ap.add_argument("--map-index", type=Path, default=None,
                    help="wotb-agent dump-map-index 的 JSON 输出（--refresh-manifest 时不需要）")
    ap.add_argument("--refresh-manifest", action="store_true",
                    help="不重新打包：按 --out 现有内容重算 manifest.json 的逐文件 sha256")
    ap.add_argument("--dataset-id", help="WG 原图所属 dataset；与稍后 stamp 的公开路径独立")
    ap.add_argument("--raw-tank-images", type=Path,
                    help="增量候选包的 WG 原始封面目录（与 --refresh-manifest、--dataset-id 一起使用）")
    args = ap.parse_args()

    if (args.out / "version.json").exists():
        ap.error("已 stamp 的包不可变；请使用单独的未发布候选目录")
    if args.refresh_manifest:
        if bool(args.dataset_id) != bool(args.raw_tank_images):
            ap.error("增量覆盖须同时指定 --dataset-id 和 --raw-tank-images")
        if args.raw_tank_images:
            catalog = load_catalog()
            version_path = args.out / "data/data_version.json"
            source = None
            if version_path.is_file():
                version = json.loads(version_path.read_text(encoding="utf-8"))
                source = (version.get("preview") or {}).get("source")
            apply_covers(args.out, args.raw_tank_images, matches_dataset(catalog, args.dataset_id, source), catalog)
        raise SystemExit(refresh_manifest(args.out, args.data))
    if args.raw_tank_images:
        ap.error("全量打包从 --data/cache/tank_images 读取原图")
    if args.map_index is None:
        ap.error("--map-index 必填（除非使用 --refresh-manifest）")

    maps = json.loads(args.map_index.read_text(encoding="utf-8"))
    data = args.data
    cache_maps = data / "cache" / "maps"
    terrain_cache = data / "cache" / "terrain"
    out = args.out
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    files: list[Path] = []

    def cp(src: Path, rel: str) -> bool:
        dst = out / rel
        if src.is_file():
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
            files.append(dst)
            return True
        return False

    # ---- 坦克 GLB（data/cache/models/{id}/，四级回退的最终答案已落盘） ----
    glb_n = 0
    models_dir = data / "cache" / "models"
    if models_dir.is_dir():
        for f in sorted(models_dir.rglob("*.glb")):
            if cp(f, f"glb/{f.parent.name}/{f.name}"):
                glb_n += 1

    # ---- 坦克封面 ----
    img_n = 0
    img_dir = data / "cache" / "tank_images"
    if img_dir.is_dir():
        for f in sorted(img_dir.rglob("*.webp")):
            if cp(f, f"tank_images/{f.name}"):
                img_n += 1

    # 封面图内容框（展示层缩放定位，视觉对齐旧紧裁口径；与 tank_images/ 1:1，
    # 图标重导后须重跑 `export_tank_icons.py --emit-crops`，缺源/数量失配即拒绝打包）
    crops = data / "cache" / "tank_icon_crops.json"
    if not crops.is_file():
        ap.error(f"缺少 {crops} —— 先运行 `python tools/export_tank_icons.py --emit-crops`")
    crops_doc = json.loads(crops.read_text(encoding="utf-8"))
    crops_n = len(crops_doc.get("icons", {}))
    if crops_n != img_n:
        ap.error(f"tank_icon_crops 与包内图标数不一致：{crops_n} vs {img_n}"
                 " —— 图标重导后须重跑 `--emit-crops`")
    shutil.copyfile(crops, out / "tank_images" / "crops.json")
    files.append(out / "tank_images" / "crops.json")

    # ---- 射击复现数据前提：碰撞盒 JSON（game_data/）+ per-tank 数据（tank/，dump-tank-data 产物） ----
    gd = data / "game_data"
    if gd.is_dir():
        for f in sorted(gd.rglob("*.json")):
            cp(f, f"game_data/{f.relative_to(gd).as_posix()}")
    td = data / "tank_data"
    if td.is_dir():
        for f in sorted(td.rglob("*.json")):
            cp(f, f"tank/{f.name}")

    # ---- 配置器数据面（data/research；图标留在 WotbTools common，其余全部随资产包） ----
    # 逐车模块与物品/技能/参数/文案都是配置器数据，随资产包更新；新坦克无需 WotbTools
    # 发版。缺目录即拒绝打包，避免发布“配置器不可用”的包（fail-closed）。
    research_n = 0
    rd = data / "research"
    if not rd.is_dir() or not (rd / "modules").is_dir():
        ap.error("缺少 data/research —— 先运行 "
                 "`python tools/export_research_loadout.py --out data/research`")
    for f in sorted(rd.rglob("*")):
        if not f.is_file():
            continue
        rel = f.relative_to(rd)
        if rel.parts and rel.parts[0] == "icons":
            continue  # 全局图标集留在 WotbTools common/research/icons
        if cp(f, f"research/{rel.as_posix()}"):
            research_n += 1

    # ---- 逐车悬挂参数（tools/export_tank_suspension.py 产物；additive） ----
    # 客户端 `Parameters/*.yaml` 的 suspension 块（逐轮行程 + 履带静止折线 + 弯曲/铺放系数 +
    # tank 侧 textureScale），供 3D 回放做逐轮贴地/履带形变/花纹滚动。语义与未定项见
    # docs/tank-suspension-client-re.md；缺文件的 tank 由前端回落整台刚体（fail-closed）。
    sus_n = 0
    sd = data / "tank_suspension"
    if sd.is_dir():
        for f in sorted(sd.glob("*.json")):
            if cp(f, f"suspension/{f.name}"):
                sus_n += 1

    # ---- 逐车地面履带印（tools/export_tank_tread.py 产物；additive） ----
    # 客户端 `Parameters/*.yaml` 的 treadParameters 块（印记条带宽/间距 + 贴图引用）与
    # 解码好的 `tread/textures/*.webp`（客户端 DDS 无损转码），供 3D 回放画碾压痕。
    # 无 treadParameters 块的 tank 不产 json（数据面口径见 docs/data-inventory.md 履带印行）。
    tread_n = 0
    td = data / "tank_tread"
    if td.is_dir():
        for f in sorted(td.glob("*.json")):
            if cp(f, f"tread/{f.name}"):
                tread_n += 1
    tt = data / "tank_tread_textures"
    if tt.is_dir():
        for f in sorted(tt.glob("*.webp")):
            if cp(f, f"tread/textures/{f.name}"):
                tread_n += 1
        # alpha 归一化表放 **tread/textures.json**（与前端 `/tread/textures.json` 的取用
        # 路径一致；2026-10-11 首传曾误放 tread/textures/ 下 ⇒ 生产 404、归一化静默失效）
        meta = tt / "textures.json"
        if meta.is_file() and cp(meta, "tread/textures.json"):
            tread_n += 1

    # ---- 核心数据（前端展示名/弹种表的自足来源） ----
    core_n = 0
    for name in ["tanks.pb", "models.pb", "tank_cache.json", "data_version.json"]:
        if cp(data / name, f"data/{name}"):
            core_n += 1

    # 精简展示名表（id → 名称）：客户端路径 tank_name 的自足来源（wotbagent 数据，不依赖消费方 tankopedia）
    tc = data / "tank_cache.json"
    if tc.is_file():
        names = {}
        for tid, info in json.loads(tc.read_text(encoding="utf-8")).items():
            name = (info or {}).get("name")
            if name:
                names[str(tid)] = name
        (out / "data" / "tank_names.json").write_text(
            json.dumps(names, ensure_ascii=False, sort_keys=True), encoding="utf-8")
        files.append(out / "data" / "tank_names.json")
        core_n += 1

    # 弹种反解表（wotb-agent dump-shell-kinds 产物，tanks.pb 全量展开）。2026-10-15 起随包
    # 分发：此前内嵌消费方前端构建，是唯一绕过 COS 的数据项（data-inventory §2.3，已退役）。
    # 缺源即拒绝打包——静默缺表会让消费方弹种判定退回无注入语义且无人察觉（fail-closed）。
    sk = data / "shell_kinds.json"
    if not sk.is_file():
        ap.error(f"缺少 {sk} —— 先运行 `wotb-agent dump-shell-kinds --out data/shell_kinds.json`"
                 "（须与打包所用的 tanks.pb 同一次刷新产出）")
    shutil.copyfile(sk, out / "data" / "shellKinds.json")
    files.append(out / "data" / "shellKinds.json")
    core_n += 1

    # 客户端 maskSlice 完整记录（emit_vehicle_pb 产物）：逐车逐炮位的裁切平面原值，
    # 装甲查看器按客户端 position/normal/planeSliceMode 重建炮管裁切面。缺源即拒绝
    # 打包——静默缺表会让 viewer 回退到旧的“标量 + 轴对齐”近似（与客户端不一致且无人察觉）。
    gm = data / "gun_mask_slices.json"
    if not gm.is_file():
        ap.error(f"缺少 {gm} —— 先运行 `python tools/emit_vehicle_pb.py`"
                 "（从客户端 maskSlice 生成，须与 models.pb 同批刷新）")
    shutil.copyfile(gm, out / "data" / "gunMaskSlices.json")
    files.append(out / "data" / "gunMaskSlices.json")
    core_n += 1

    # ---- 地图（按注册表逐张物化；对齐四个端点的文件选择语义） ----
    map_n = 0
    with_terrain = 0
    missing_scale = []
    seen_spaces: dict[str, str] = {}   # space → 首个物化 key（变体图共享资产）
    for e in maps:
        key, space = e["key"], e["space"]
        if space in seen_spaces:
            continue  # 同 space 变体：资产只物化一份，index 多 id 指向同一目录
        seen_spaces[space] = key
        mid = str(e["map_id"])
        mdir = out / "map" / key
        got = []

        # map?id= 高清底图：手动覆盖（data/maps/<name|key>.{webp,png,jpg,jpeg}）
        # → data/cache/maps/{space}.ground.webp
        overrides = [data / "maps" / f"{s}.{ext}"
                     for s in (e.get("_param", key), key)
                     for ext in ("webp", "png", "jpg", "jpeg")]
        if copy_first(overrides + [cache_maps / f"{space}.ground.webp"],
                      mdir / "ground.webp"):
            got.append("ground")

        # res=mini：提取缓存 {space}.minimap.webp（客户端提取/随包兜底为运行时链，包外）
        if (cache_maps / f"{space}.minimap.webp").is_file():
            cp(cache_maps / f"{space}.minimap.webp", f"map/{key}/mini.webp")
            got.append("mini")

        # terrain：手动覆盖 data/maps/{key}.heightmap.u16.bin（契约 = corner 口径镜像数据）
        #          → 缓存 data/cache/terrain/{key}.hm.c2.u16.bin（文件名自描述布局；
        #            修正前的旧名 {key}.hm.u16.bin 是离散翻转布局，仅无 c2 时兜底且不写
        #            terrainLayout——前端读取边界按旧布局自行平移一列补正）；
        # 尺度 = sidecar data/cache/maps/{space}.json 的 worldBounds（缺尺度不打包 terrain
        # ——与服务端"错标尺度比 404 更糟"同一裁决），sidecar 物化为 terrain.json
        scale_src = cache_maps / f"{space}.json"
        scale = None
        if scale_src.is_file():
            v = json.loads(scale_src.read_text(encoding="utf-8"))
            b = v.get("worldBounds") or {}
            mn, mx = b.get("min"), b.get("max")
            if isinstance(mn, list) and isinstance(mx, list) and len(mn) >= 3 and len(mx) >= 3:
                # span = 水平世界跨度 max(dx,dy)——与服务端 terrain_scale（map_assets.rs）
                # 同式；前端 sampleHeight/rebuildGround 以它做水平米制换算。
                # 勿写 zmax-zmin（那是垂直高度差）：错值会把地形压成 span×span 小块
                span_h = max(float(mx[0]) - float(mn[0]), float(mx[1]) - float(mn[1]))
                scale = {"size": 512,
                         "zmax": round(float(mx[2]), 1),
                         "zmin": round(float(mn[2]), 1),
                         "span": round(span_h, 1)}
        terrain_srcs = [data / "maps" / f"{key}.heightmap.u16.bin",
                        terrain_cache / f"{key}.hm.c2.u16.bin",
                        terrain_cache / f"{key}.hm.u16.bin"]
        hit = copy_first(terrain_srcs, mdir / "terrain.u16.bin")
        terrain_src = next((s for s in terrain_srcs if s.is_file()), None) if hit else None
        if hit and scale:
            # terrain.json = X-Terrain-Meta 头的 sidecar 化：顶层必须是前端消费的
            # size/zmax/zmin/span（worldBounds 等导出器原字段保留在子对象）
            got.append("terrain")
            with_terrain += 1
        if hit:
            side = {}
            if scale_src.is_file():
                try:
                    side = json.loads(scale_src.read_text(encoding="utf-8"))
                except Exception:
                    side = {}
            side.update(scale or {})
            # terrainLayout 必须与字节一致（WotbTools PR #615 decodeTerrainField：2 原样用、
            # 缺省按旧布局平移一列、未知值 fail-closed 回退 2D）——错标比缺省更糟
            if terrain_src.name.endswith(".hm.u16.bin"):
                print(f"  ⚠ terrain {key}: 只有旧布局缓存（{terrain_src.name}），"
                      f"terrain.json 不写 terrainLayout——请重跑 fetch-terrain --force")
            else:
                side["terrainLayout"] = 2
            # bin 的 sha256（配对锚点）：terrain.json（no-cache）与 terrain.u16.bin
            # （max-age=3600）是两个独立请求，发布切换期可能错配（旧 JSON+新 bin = 双重
            # 补正、新 JSON+缓存旧 bin = 漏补正）；前端拿该 hash 做缓存破坏 URL 即可保证
            # 配对。与 manifest.json 的逐文件 sha256 同源
            side["terrainBinSha256"] = hashlib.sha256(
                (mdir / "terrain.u16.bin").read_bytes()).hexdigest()
            # 让位掩码（cover）与贴地结构脚印（structs）均已撤销（2026-10-11 用户裁示：
            # 只对齐客户端）——清掉历史字段，terrain.json 只承载客户端已有的元数据
            for legacy in ("cover", "coverStats", "structs", "structsStats"):
                side.pop(legacy, None)
            (out / f"map/{key}/terrain.json").write_text(
                json.dumps(side, ensure_ascii=False), encoding="utf-8")
            files.append(out / f"map/{key}/terrain.json")
        elif hit:
            missing_scale.append(key)
            (mdir / "terrain.u16.bin").unlink(missing_ok=True)

        # scenery：data/cache/maps/{space}.glb
        if cp(cache_maps / f"{space}.glb", f"map/{key}/scenery.glb"):
            got.append("scenery")

        # 可破坏物清单（tools/export_map_destructibles.py 产物，按 key 目录）：
        # 回放 destructible_events 的 (cell, slot) 联表目标（逆向总集 §5.4 公式）
        if cp(cache_maps / key / "destructibles.json", f"map/{key}/destructibles.json"):
            got.append("destructibles")

        # 逐图光照参数（tools/export_map_lighting.py：太阳方向/色/强度 + 环境色 + 雾 + IBL
        # 引用）——3D 侧方向光与半球环境光的来源；缺失时消费端回落硬编码灯光，不阻断）
        if cp(cache_maps / f"{space}.lighting.json", f"map/{key}/lighting.json"):
            got.append("lighting")

        # 逐图 IBL 环境贴图（tools/export_map_ibl.py：客户端 specular 立方图 → 等距柱状投影）
        # ——动态物/坦克的环境反射来源；缺失时消费端不设 scene.environment（不阻断）
        if cp(cache_maps / f"{space}.ibl.webp", f"map/{key}/ibl.webp"):
            got.append("ibl")

        # groundmeta + groundtex：{space}.ground.layers.json + {space}.ground.{layer}.webp
        if cp(cache_maps / f"{space}.ground.layers.json", f"map/{key}/ground.layers.json"):
            got.append("layers")
        for k in GROUND_LAYERS:
            cp(cache_maps / f"{space}.ground.{k}.webp", f"map/{key}/ground/{k}.webp")

        if got:
            map_n += 1
            files.extend(p for p in mdir.rglob("*") if p.is_file())

    # ---- 包的来源（upstream commit + 生成时的数据版本清单） ----
    provenance = git_provenance()
    data_version = None
    dv_path = data / "data_version.json"
    if dv_path.is_file():
        try:
            data_version = json.loads(dv_path.read_text(encoding="utf-8"))
        except Exception:
            data_version = None

    catalog = load_catalog()
    preview_source = ((data_version or {}).get("preview") or {}).get("source")
    if matches_dataset(catalog, args.dataset_id, preview_source):
        apply_covers(out, img_dir, True, catalog)

    # ---- index.json（前端一次性装载：数字 id → key） ----
    generated = datetime.now(timezone.utc).isoformat(timespec="seconds")
    index = {
        "version": 1,
        "generated": generated,
        **provenance,
        "maps": {str(e["map_id"]): {"key": seen_spaces.get(e["space"], e["key"]), "display": e["display"]} for e in maps},
    }
    (out / "index.json").write_text(json.dumps(index, ensure_ascii=False, indent=1),
                                    encoding="utf-8")
    files.append(out / "index.json")

    # ---- manifest（sha256，部署前校验包完整性） ----
    # 按路径去重（map 段 cp 与 rglob 会重复登记同一文件）
    uniq: dict[str, dict] = {}
    for f in sorted(files):
        rel = str(f.relative_to(out)).replace("\\", "/")
        uniq.setdefault(rel, {"path": rel, "bytes": f.stat().st_size, "sha256": sha256(f)})
    manifest_files = [uniq[k] for k in sorted(uniq)]
    manifest = {
        "version": 1,
        "generated": generated,
        **provenance,
        "data_version": data_version,
        "files": manifest_files,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1),
                                       encoding="utf-8")

    n_files = len(manifest["files"])
    mb = sum(f["bytes"] for f in manifest["files"]) / 1e6
    print(f"资产包已导出: {out}")
    print(f"  GLB {glb_n} 个文件 / 封面 {img_n} 张 / 核心数据 {core_n} 个 / 地图 {map_n} 张"
          f"（含地形 {with_terrain}）/ 悬挂 {sus_n} 辆 / 配置器数据 {research_n} 件")
    print(f"  共 {n_files} 文件, {mb:.1f}MB（manifest.json 含逐文件 sha256）")
    if missing_scale:
        print(f"  ⚠ 缺地形尺度（sidecar worldBounds），terrain 未打包: {', '.join(missing_scale)}")
    print("  部署：整目录上传到静态主机/对象存储根路径；CORS 允许页面来源 GET/HEAD。")


if __name__ == "__main__":
    main()
