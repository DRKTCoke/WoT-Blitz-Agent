#!/usr/bin/env python3
"""`export_augment_icons.py` 的独立回归测试（无 pytest 依赖，不触网）。

覆盖：
1. 机制目录名映射（kebab 大小写转换 + 两处客户端拼写别名）；
2. 从包 `research/modules/*.json` 收集图标引用的语义（属性定义 + 备选组 + 机制 stem）；
3. alpha 裁边 + 无损 webp 输出（合成图，验证尺寸与"同输入同字节"幂等性）。
"""

from __future__ import annotations

import io
import json
import pathlib
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import export_augment_icons as A  # noqa: E402


def test_kebab_and_aliases() -> None:
    assert A.kebab("burstMode") == "burst-mode"
    assert A.kebab("steadyAimSystem") == "steady-aim-system"
    # 客户端 item `<script>` 拼写与 UI 目录名不一致的两处
    assert A.MECHANIC_ICON_ALIASES["auxilliaryWeapons"] == "auxiliary-weapons"
    assert A.MECHANIC_ICON_ALIASES["externalAmmoModule"] == "external-ammunition-supply"
    # 别名的首个候选必须落在对应目录
    assert A.mechanic_bases("auxilliaryWeapons")[0].endswith(
        "tank-mechanics/auxiliary-weapons/icon_auxiliary-weapons_m")
    assert A.mechanic_bases("externalAmmoModule")[0].endswith(
        "tank-mechanics/external-ammunition-supply/icon_external-ammunition-supply_m")


def test_collect_requirements() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        pack = pathlib.Path(tmp)
        modules = pack / "research" / "modules"
        modules.mkdir(parents=True)
        (modules / "japan.json").write_text(json.dumps({
            "tanks": {
                "J77_Ho_Ri_4": {
                    "tankId": 10593,
                    "vehicleAbilities": ["unguidedRocket"],
                    "upgradeProgression": {
                        "attributeDefinitions": {
                            "A": {"icon": "~res:/Gfx/Lobby/icons/upgrades/icon_a"},
                            "B": {"icon": "~res:/Gfx/Lobby/icons/upgrades/icon_a"},
                            "C": {},  # 无 icon 的属性不得产生条目
                        },
                        "alternativeGroups": [{"icon": "~res:/Gfx/Lobby/icons/upgrades/icon_g"}],
                        "nodes": [],
                    },
                },
            },
        }), encoding="utf-8")
        icons, mechanics = A.collect_requirements(pack)
        assert icons == {"icon_a", "icon_g"}, icons
        assert mechanics == {"unguidedRocket": "mech_unguidedRocket"}, mechanics


def test_crop_alpha_lossless() -> None:
    from PIL import Image
    canvas = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
    canvas.paste((200, 10, 10, 255), (8, 16, 40, 48))  # 内容 32x32，偏移 (8,16)
    raw = io.BytesIO()
    canvas.save(raw, "WEBP", lossless=True, exact=True)

    webp, size, bbox = A._crop_alpha(raw.getvalue())
    assert bbox == (8, 16, 40, 48), bbox
    assert size == (32, 32), size
    again = A._crop_alpha(raw.getvalue())[0]
    assert again == webp, "同输入必须同字节（幂等，避免无谓改包）"
    assert Image.open(io.BytesIO(webp)).size == (32, 32)


def main() -> int:
    for name, fn in sorted(globals().items()):
        if name.startswith("test_") and callable(fn):
            fn()
            print(f"[ok] {name}")
    print("augment icon tests: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
