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
        "version.txt.dvpl": "version",
        "Gfx/UI/icon.png.dvpl": "ui",
        "Unknown/blob.bin": "unmapped",
    }
    for path, expected in cases.items():
        got, _detail = P.classify(path)
        assert got == expected, (path, got, expected)
    assert P.classify("XML/item_defs/vehicles/usa/components/gun.xml.dvpl")[0] == "vehicle-components"
    assert P.classify("XML/item_defs/vehicles/usa/list.xml.dvpl")[0] == "vehicle-roster"
    model = P.classify("3d/Tanks/usa/A1.sc2.dvpl")[1]
    assert model == {"nation": "usa", "stem": "A1"}, model
    mp = P.classify("3d/Maps/17_karelia_ka/17_karelia_ka.sc2.dvpl")[1]
    assert mp == {"space": "17_karelia_ka"}, mp
    mk = P.classify("maps/karelia/x.bin")[1]
    assert mk == {"key": "karelia"}, mk


def write_tree(root: pathlib.Path, files: dict[str, str]) -> None:
    for rel, text in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")


def run_plan(base: pathlib.Path, new: pathlib.Path, out: pathlib.Path, strict: bool,
             base_manifest: pathlib.Path | None = None,
             new_manifest: pathlib.Path | None = None,
             extra: list[str] | None = None) -> subprocess.CompletedProcess:
    cmd = [sys.executable, "-I", str(TOOL), "--base", str(base), "--new", str(new),
           "--out", str(out)]
    if strict:
        cmd.append("--strict")
    if base_manifest:
        cmd += ["--base-pack-manifest", str(base_manifest)]
    if new_manifest:
        cmd += ["--new-pack-manifest", str(new_manifest)]
    if extra:
        cmd += extra
    return subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)


def write_manifest(path: pathlib.Path, files: dict[str, str]) -> None:
    path.write_text(json.dumps({"files": [{"path": p, "sha256": h}
                                          for p, h in files.items()]}), encoding="utf-8")


def coverage_derived_outputs(tmp: pathlib.Path, extra: list[str]) -> None:
    """派生 data/ 文件不算 orphan；真正无源的输出仍要失败。"""
    base, new = tmp / "cov_base", tmp / "cov_new"
    source = "XML/item_defs/vehicles/usa/A1.xml.dvpl"
    write_tree(base, {source: "old"})
    write_tree(new, {source: "new"})
    manifest_files = {
        "data/models.pb", "data/data_version.json", "data/tanks.pb",
        "data/tank_names.json", "data/shellKinds.json",
        "data/gunMaskSlices.json", "tank/1.json",
    }
    base_manifest, new_manifest = tmp / "base_manifest.json", tmp / "new_manifest.json"
    write_manifest(base_manifest, {p: "old" for p in manifest_files})
    write_manifest(new_manifest, {p: "new" for p in manifest_files})
    plan_path = tmp / "coverage_plan.json"
    result = run_plan(base, new, plan_path, strict=True,
                      base_manifest=base_manifest, new_manifest=new_manifest, extra=extra)
    assert result.returncode == 0, result.stderr
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    assert plan["coverage"]["orphanOutput"] == [], plan["coverage"]

    # 无源输出必须被 strict 拒绝。
    write_manifest(new_manifest, {**{p: "new" for p in manifest_files},
                                  "other/extra.bin": "new"})
    result = run_plan(base, new, tmp / "coverage_bad.json", strict=True,
                      base_manifest=base_manifest, new_manifest=new_manifest, extra=extra)
    assert result.returncode != 0, "orphan output must fail strict"


def main() -> None:
    classify_cases()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        base, new = tmp / "base", tmp / "new"
        bridge = tmp / "bridge.json"
        bridge.write_text(json.dumps({
            "1": {"nation": "usa", "stem": "A1"},
            "2": {"nation": "usa", "stem": "A2"},
        }), encoding="utf-8")
        map_index = tmp / "map_index.json"
        map_index.write_text(json.dumps([
            {"display": "X", "key": "x", "map_id": 1, "space": "x_space",
             "minimap_dir": "x"},
        ]), encoding="utf-8")
        extra = ["--tank-bridge", str(bridge), "--preview-bridge", str(bridge),
                 "--map-index", str(map_index)]
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
                         "XML/item_defs/vehicles/usa/A2.xml.dvpl": "new",
                         "3d/Maps/x_space/x_space.sc2.dvpl": "new-map"})
        plan_path = tmp / "plan.json"
        result = run_plan(base, new, plan_path, strict=True, extra=extra)
        assert result.returncode == 0, result.stderr
        plan = json.loads(plan_path.read_text(encoding="utf-8"))
        assert plan["planes"]["vehicle-xml"]["added"] == 1, plan
        assert plan["planes"]["vehicle-xml"]["changed"] == 1, plan
        assert plan["counts"]["unmapped"] == 0, plan
        assert plan["tanks"]["1"]["stem"] == "A1", plan["tanks"]
        assert plan["tanks"]["2"]["stem"] == "A2", plan["tanks"]
        assert plan["wholeTable"], "vehicle-xml must require core whole-table"
        assert plan["maps"]["x_space"]["keys"] == ["x"], plan["maps"]
        assert plan["maps"]["x_space"]["map_ids"] == [1], plan["maps"]

        # 目录 vs ZIP：同内容不能误报 changed。
        zip_path = tmp / "new.zip"
        with zipfile.ZipFile(zip_path, "w") as zf:
            for path in new.rglob("*"):
                if path.is_file():
                    zf.write(path, "app/Data/" + path.relative_to(new).as_posix())
        zip_plan = tmp / "zip_plan.json"
        result = run_plan(base, zip_path, zip_plan, strict=True, extra=extra)
        assert result.returncode == 0, result.stderr
        parsed = json.loads(zip_plan.read_text(encoding="utf-8"))
        assert parsed["planes"]["vehicle-xml"]["added"] == 1, parsed
        assert parsed["planes"]["vehicle-xml"]["changed"] == 1, parsed
        assert parsed["counts"]["changed"] == 1, parsed

        coverage_derived_outputs(tmp, extra)

        # 未知变化 strict 必须失败。
        (new / "Unknown").mkdir()
        (new / "Unknown/blob.bin").write_text("new", encoding="utf-8")
        result = run_plan(base, new, tmp / "bad.json", strict=True, extra=extra)
        assert result.returncode != 0, "unmapped change must fail strict"
    print("plan data version tests: PASS")


if __name__ == "__main__":
    main()
