#!/usr/bin/env python3
"""`tools/plan_data_version.py` 的独立回归测试（无 pytest 依赖）。

覆盖：
1. 路径 → 数据面分类（车辆 XML/Parameters、shared XML、techtree、strings、maps、ui）；
2. 目录 vs ZIP 的 CRC32+size 回退比较；
3. strict 模式：未知变化 fail-closed，已知变化通过。
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools/plan_data_version.py"


def classify_cases() -> None:
    sys.path.insert(0, str(ROOT / "tools"))
    import plan_data_version as P
    cases = {
        "XML/item_defs/vehicles/usa/A1.xml.dvpl": "vehicle-xml",
        "3d/Tanks/Parameters/usa/A1.yaml.dvpl": "vehicle-params",
        "XML/item_defs/vehicles/common/vehicle.xml.dvpl": "shared-xml",
        "Configs/TechTree/usa_tree.yaml.dvpl": "techtree",
        "Strings/en.yaml.dvpl": "strings",
        "maps/foo.bin": "maps",
        "Gfx/UI/icon.png.dvpl": "ui",
        "Unknown/blob.bin": "unmapped",
    }
    for path, expected in cases.items():
        got, _detail = P.classify(path)
        assert got == expected, (path, got, expected)


def write_tree(root: pathlib.Path, files: dict[str, str]) -> None:
    for rel, text in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")


def run_plan(base: pathlib.Path, new: pathlib.Path, out: pathlib.Path, strict: bool) -> subprocess.CompletedProcess:
    cmd = [sys.executable, "-I", str(TOOL), "--base", str(base), "--new", str(new),
           "--out", str(out)]
    if strict:
        cmd.append("--strict")
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)


def main() -> None:
    classify_cases()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        base, new = tmp / "base", tmp / "new"
        common = {
            "XML/item_defs/vehicles/usa/A1.xml.dvpl": "old",
            "XML/item_defs/vehicles/common/vehicle.xml.dvpl": "old",
            "Configs/TechTree/usa_tree.yaml.dvpl": "old",
            "maps/x.bin": "old",
            "Gfx/UI/icon.png.dvpl": "old",
        }
        write_tree(base, common)
        write_tree(new, {**common,
                         "XML/item_defs/vehicles/usa/A1.xml.dvpl": "new",
                         "XML/item_defs/vehicles/usa/A2.xml.dvpl": "new"})
        plan_path = tmp / "plan.json"
        result = run_plan(base, new, plan_path, strict=True)
        assert result.returncode == 0, result.stderr
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        assert plan["planes"]["vehicle-xml"]["added"] == 1, plan
        assert plan["planes"]["vehicle-xml"]["changed"] == 1, plan
        assert plan["counts"]["unmapped"] == 0, plan

        # 目录 vs ZIP：同内容不能误报 changed。
        zip_path = tmp / "new.zip"
        with zipfile.ZipFile(zip_path, "w") as zf:
            for path in new.rglob("*"):
                if path.is_file():
                    zf.write(path, "app/Data/" + path.relative_to(new).as_posix())
        zip_plan = tmp / "zip_plan.json"
        result = run_plan(base, zip_path, zip_plan, strict=True)
        assert result.returncode == 0, result.stderr
        parsed = json.loads(zip_plan.read_text(encoding="utf-8"))
        assert parsed["planes"]["vehicle-xml"]["added"] == 1, parsed
        assert parsed["planes"]["vehicle-xml"]["changed"] == 1, parsed
        assert parsed["counts"]["changed"] == 1, parsed

        # 未知变化 strict 必须失败。
        (new / "Unknown").mkdir()
        (new / "Unknown/blob.bin").write_text("new", encoding="utf-8")
        result = run_plan(base, new, tmp / "bad.json", strict=True)
        assert result.returncode != 0, "unmapped change must fail strict"
    print("plan data version tests: PASS")


if __name__ == "__main__":
    main()
