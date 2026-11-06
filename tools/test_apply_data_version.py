#!/usr/bin/env python3
"""`tools/apply_data_version.py` 的独立回归测试（无 pytest 依赖）。

覆盖：
1. 硬链接克隆只跳过根目录发布元数据，嵌套同名文件必须保留；
2. plan 含 vehicle-xml 的非 research 输出时 fail-closed（不得只同步 research 就发布）；
3. plan 含 unmapped 变化时 fail-closed，且在克隆前退出。
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools/apply_data_version.py"


def run_apply(base: pathlib.Path, out: pathlib.Path, plan: dict) -> subprocess.CompletedProcess:
    plan_path = out.parent / (out.name + ".plan.json")
    plan_path.write_text(json.dumps(plan), encoding="utf-8")
    return subprocess.run(
        [sys.executable, "-I", str(TOOL), "--base-pack", str(base),
         "--out-pack", str(out), "--game-data", str(out.parent),
         "--plan", str(plan_path)],
        cwd=ROOT, capture_output=True, text=True,
    )


def clone_keeps_nested_metadata(tmp: pathlib.Path) -> None:
    sys.path.insert(0, str(ROOT / "tools"))
    import apply_data_version as A

    base, out = tmp / "clone_base", tmp / "clone_out"
    (base / "nested").mkdir(parents=True)
    (base / "version.json").write_text("root", encoding="utf-8")
    (base / "nested/version.json").write_text("nested", encoding="utf-8")
    (base / "data").mkdir()
    (base / "data/x.json").write_text("x", encoding="utf-8")
    linked, copied = A.clone_pack(base, out)
    assert not (out / "version.json").exists(), "root release metadata must be skipped"
    assert (out / "nested/version.json").read_text(encoding="utf-8") == "nested"
    assert linked == 2 and copied == 0, (linked, copied)


def fail_closed_cases(tmp: pathlib.Path) -> None:
    base = tmp / "pack"
    base.mkdir()
    (base / "data").mkdir()
    (base / "data/data_version.json").write_text("{}", encoding="utf-8")

    vehicle_xml = {"planes": {"vehicle-xml": {"added": 1, "changed": 0, "removed": 0}}}
    result = run_apply(base, tmp / "out_vehicle", vehicle_xml)
    assert result.returncode == 2, (result.returncode, result.stderr)
    assert "vehicle-xml" in result.stderr and "完整重建" in result.stderr, result.stderr
    assert not (tmp / "out_vehicle").exists(), "must fail before clone"

    unmapped = {"planes": {"unmapped": {"added": 1, "changed": 0, "removed": 0}},
                "unmappedChanged": ["Unknown/blob.bin"]}
    result = run_apply(base, tmp / "out_unmapped", unmapped)
    assert result.returncode == 2, (result.returncode, result.stderr)
    assert "未识别数据面" in result.stderr, result.stderr
    assert not (tmp / "out_unmapped").exists(), "must fail before clone"


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        clone_keeps_nested_metadata(tmp)
        fail_closed_cases(tmp)
    print("apply data version tests: PASS")


if __name__ == "__main__":
    main()
