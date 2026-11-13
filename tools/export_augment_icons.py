#!/usr/bin/env python3
"""导出 XI 强化树（UpgradeProgression）图标：客户端 @2x 贴图 → alpha 裁边 webp。

数据面事实（docs/game-data-sources.md §2.7）：
- 强化节点/属性图标 = `Data/Gfx/Lobby/icons/upgrades/<icon>.packed.webp.dvpl`
  （客户端 `tank_upgrade_attributes.xml` 的 `<icon>` 与组 `<icon>` 引用）；
- 车辆机制图标 = `Data/Gfx/Lobby/icons/tank-mechanics/<kebab>/icon_<kebab>_m...`
  （与 `vehicleAbilities` 的 stem 对号：burstMode → burst-mode）。

消费方 = WotbTools `common/research/icons/upgrades/`（`TankAugmentTree.vue` 按
`<icon 基名>.webp` 解析；缺失时回退 `mech_<abilityStem>.webp`，再无则空图）。
CSS 固定显示尺寸（38/46/32px + object-fit: contain），所以这里只保证**原图宽高比**
与内容边界：取 @2x 整图，按 alpha 可见内容裁边，无损 webp 落盘。

输入（二选一，优先 ZIP）：
- `--zip-url <URL>`（默认 t11r3 预览包）——ZIP64 central directory + HTTP Range 只取
  需要的图标成员，不下载整包；
- `--data-root <Data 根>`——已解包的客户端数据目录。

用法：
    python tools/export_augment_icons.py --pack release/asset_pack_t11r3_r2 \
        --out /path/to/WotbTools/common/research/icons/upgrades
    python tools/export_augment_icons.py --pack <pack> --out <dir> --check
"""
from __future__ import annotations

import argparse
import io
import json
import pathlib
import re
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT / "tools"))

DEFAULT_ZIP = "http://cdn.static.wotb.app/win7/WotBlitz_11.4.0.138_t11r3.zip"
UPGRADE_DIR = "Gfx/Lobby/icons/upgrades"
MECHANICS_DIR = "Gfx/Lobby/icons/tank-mechanics"


def kebab(stem: str) -> str:
    """burstMode → burst-mode（客户端 tank-mechanics 目录命名）。"""
    return re.sub(r"(?<!^)(?=[A-Z])", "-", stem).lower()


# `vehicleAbilities` 的 stem 来自物品定义 `<script>` 名；客户端 UI 的图标目录按
# `eGameplayMechanics` 枚举名命名（Data/UI/Screens3/Lobby/Common/TankMechanics/
# TankMechanicsOverlay.yaml 的 `iconPath` 映射）。两处拼写不同的条目集中登记，
# 其余走 kebab(stem) 直配；表外未命中即 fail-closed（不猜目录）。
MECHANIC_ICON_ALIASES = {
    # 物品 `<script>AuxilliaryWeapons</script>`（客户端自身拼写）→ 字符串
    # `TankMechanics/AuxiliaryWeapons` + 目录 auxiliary-weapons。
    "auxilliaryWeapons": "auxiliary-weapons",
    # 物品 `<script>ExternalAmmoModule<...>` → 字符串
    # `TankMechanics/ExternalAmmunitionSupply` + 目录 external-ammunition-supply。
    "externalAmmoModule": "external-ammunition-supply",
}


def collect_requirements(pack: pathlib.Path) -> tuple[set[str], dict[str, str]]:
    """从包的 research/modules/*.json 收集所需图标。

    返回 (属性/组图标基名, {abilityStem: 机制图标基名})——后者落盘名固定
    `mech_<abilityStem>.webp`（消费方按 stem 拼名，不按贴图名）。
    """
    icons: set[str] = set()
    mechanics: dict[str, str] = {}
    modules_dir = pack / "research" / "modules"
    if not modules_dir.is_dir():
        raise SystemExit(f"缺少 research/modules: {modules_dir}")
    for path in sorted(modules_dir.glob("*.json")):
        payload = json.loads(path.read_text(encoding="utf-8"))
        for tank in (payload.get("tanks") or {}).values():
            progression = tank.get("upgradeProgression")
            if not progression:
                continue
            for defn in (progression.get("attributeDefinitions") or {}).values():
                raw = defn.get("icon") if isinstance(defn, dict) else None
                if isinstance(raw, str) and raw:
                    icons.add(raw.split("/")[-1])
            for group in progression.get("alternativeGroups") or []:
                raw = group.get("icon")
                if isinstance(raw, str) and raw:
                    icons.add(raw.split("/")[-1])
            for ability in tank.get("vehicleAbilities") or []:
                if isinstance(ability, str) and ability:
                    mechanics.setdefault(ability, f"mech_{ability}")
    return icons, mechanics


def _dvpl_decode(raw: bytes) -> bytes:
    from wotb_sc2 import decode_dvpl
    return decode_dvpl(raw)


def _crop_alpha(data: bytes) -> tuple[bytes, tuple[int, int], tuple[int, int]]:
    """按 alpha 可见内容裁边 → 无损 webp 字节（保留原宽高比）。"""
    from PIL import Image
    image = Image.open(io.BytesIO(data)).convert("RGBA")
    bbox = image.getchannel("A").getbbox()
    if bbox:
        image = image.crop(bbox)
    buf = io.BytesIO()
    image.save(buf, "WEBP", lossless=True, exact=True)
    return buf.getvalue(), image.size, bbox or (0, 0, 0, 0)


class ZipSource:
    def __init__(self, url: str) -> None:
        from fetch_t11_techtree import RangeReader, find_eocd, parse_central_directory
        self.reader = RangeReader(url)
        off, size, _ = find_eocd(self.reader)
        self.entries = parse_central_directory(self.reader, off, size)

    def get(self, relbase: str) -> bytes | None:
        """按 `Gfx/...` 相对路径取解码后的 webp；@2x 优先（消费方 CSS 会缩放）。"""
        for variant in (f"{relbase}@2x.packed.webp.dvpl", f"{relbase}.packed.webp.dvpl"):
            member = f"app/Data/{variant}"
            if member in self.entries:
                from fetch_t11_techtree import read_member
                raw = read_member(self.reader, {member: self.entries[member]}, member)
                return _dvpl_decode(raw)
        return None

    def close(self) -> None:
        return None


class DataRootSource:
    def __init__(self, root: pathlib.Path) -> None:
        self.root = root

    def get(self, relbase: str) -> bytes | None:
        for variant in (f"{relbase}@2x.packed.webp.dvpl", f"{relbase}.packed.webp.dvpl"):
            path = self.root / variant
            if path.is_file():
                return _dvpl_decode(path.read_bytes())
        return None


def mechanic_bases(stem: str) -> list[str]:
    """机制图标的候选基名（`_m` 中号优先，其次卡片图）。"""
    kb = MECHANIC_ICON_ALIASES.get(stem) or kebab(stem)
    snake = stem.lower()
    out = [f"{MECHANICS_DIR}/{kb}/icon_{kb}_m", f"{MECHANICS_DIR}/{kb}/icon_{snake}_m"]
    out += [f"{MECHANICS_DIR}/{kb}/icon_{kb.replace('-', '_')}_m",
            f"{MECHANICS_DIR}/{kb}/icon_{kb.replace('-', '_')}_card",
            f"{MECHANICS_DIR}/{kb}/icon_{kb}_card"]
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--pack", type=pathlib.Path, required=True,
                    help="已构建的资产包（读 research/modules/*.json 的图标引用）")
    ap.add_argument("--out", type=pathlib.Path, required=True,
                    help="图标输出目录（WotbTools common/research/icons/upgrades）")
    ap.add_argument("--zip-url", default=DEFAULT_ZIP)
    ap.add_argument("--data-root", type=pathlib.Path, default=None,
                    help="改为从已解包的 Data 根读取（不用 ZIP）")
    ap.add_argument("--check", action="store_true",
                    help="只检查缺失，不写文件")
    ap.add_argument("--overwrite", action="store_true",
                    help="同名图标已存在时也重写（默认只补缺失；避免无谓改动已验收的旧图）")
    ap.add_argument("--prune", action="store_true",
                    help="删除本次未被任何 XI 车引用的图标（默认保留）")
    args = ap.parse_args()

    icons, mechanics = collect_requirements(args.pack)
    print(f"required: {len(icons)} attribute/group icons, {len(mechanics)} mechanics")

    source = DataRootSource(args.data_root) if args.data_root else ZipSource(args.zip_url)
    args.out.mkdir(parents=True, exist_ok=True)

    written = skipped = missing = 0
    for icon in sorted(icons):
        data = source.get(f"{UPGRADE_DIR}/{icon}")
        if data is None:
            print(f"  MISSING in client: {UPGRADE_DIR}/{icon}")
            missing += 1
            continue
        webp, size, bbox = _crop_alpha(data)
        dst = args.out / f"{icon}.webp"
        if dst.is_file() and not args.overwrite:
            skipped += 1
            continue
        if args.check:
            print(f"  would update {icon}.webp ({size[0]}x{size[1]} from {bbox})")
            continue
        dst.write_bytes(webp)
        written += 1
        print(f"  {icon}.webp ({size[0]}x{size[1]})")

    for stem, out_name in sorted(mechanics.items()):
        data = None
        for base in mechanic_bases(stem):
            data = source.get(base)
            if data is not None:
                print(f"  mechanic {stem} <- {base}")
                break
        if data is None:
            print(f"  MISSING mechanic sprite: {stem}")
            missing += 1
            continue
        webp, size, _ = _crop_alpha(data)
        dst = args.out / f"{out_name}.webp"
        if dst.is_file() and not args.overwrite:
            skipped += 1
            continue
        if args.check:
            print(f"  would update {out_name}.webp ({size[0]}x{size[1]})")
            continue
        dst.write_bytes(webp)
        written += 1
        print(f"  {out_name}.webp ({size[0]}x{size[1]})")

    if args.prune and not args.check:
        keep = {f"{icon}.webp" for icon in icons} | {f"{name}.webp" for name in mechanics.values()}
        # 机制类只保留本次引用的 mech_*；其余（t11_augment_*、line_connect 等 UI 件）不动
        for path in sorted(args.out.glob("*.webp")):
            if path.name.startswith("mech_") and path.name not in keep:
                path.unlink()
                print(f"  pruned {path.name}")

    print(f"written={written} unchanged={skipped} missing={missing}")
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
