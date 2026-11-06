#!/usr/bin/env python3
"""`tools/apply_data_version.py` / `pack_incremental.py` 的独立回归测试。

覆盖：
1. 硬链接克隆只跳过根目录发布元数据，嵌套同名文件必须保留；
2. 未识别数据面 fail-closed；
3. dry-run 不写 pack，且按 handler 分发 vehicle/core/maps 命令。
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools/apply_data_version.py"
sys.path.insert(0, str(ROOT / "tools"))


def _plan(planes: dict, tanks: dict | None = None, maps: dict | None = None) -> dict:
    return {
        "schema": 2, "planes": planes, "tanks": tanks or {}, "maps": maps or {},
        "wholeTable": [], "corePlanes": [], "mapRegistryChanged": False,
        "unresolvedTanks": [], "unresolvedMaps": [], "pins": {},
    }


def run_apply(base: pathlib.Path, out: pathlib.Path, plan: dict, *extra: str) -> subprocess.CompletedProcess:
    plan_path = out.parent / (out.name + ".plan.json")
    plan_path.write_text(json.dumps(plan), encoding="utf-8")
    return subprocess.run(
        [sys.executable, "-I", str(TOOL), "--base-pack", str(base), "--out-pack", str(out),
         "--game-data", str(out.parent), "--plan", str(plan_path), *extra],
        cwd=ROOT, capture_output=True, text=True,
    )


def clone_keeps_nested_metadata(tmp: pathlib.Path) -> None:
    import pack_incremental as P

    base, out = tmp / "clone_base", tmp / "clone_out"
    (base / "nested").mkdir(parents=True)
    (base / "version.json").write_text("root", encoding="utf-8")
    (base / "nested/version.json").write_text("nested", encoding="utf-8")
    (base / "data").mkdir()
    (base / "data/x.json").write_text("x", encoding="utf-8")
    linked, copied = P.clone_base(base, out)
    assert not (out / "version.json").exists(), "root release metadata must be skipped"
    assert (out / "nested/version.json").read_text(encoding="utf-8") == "nested"
    assert linked == 2 and copied == 0, (linked, copied)


def incremental_overlay(tmp: pathlib.Path) -> None:
    import pack_incremental as P

    base, out = tmp / "ov_base", tmp / "ov_out"
    (base / "tank").mkdir(parents=True)
    (base / "tank/1.json").write_text("old", encoding="utf-8")
    (base / "tank/2.json").write_text("stale", encoding="utf-8")
    P.clone_base(base, out)
    inode = (out / "tank/1.json").stat().st_ino
    assert P.overlay_bytes(out, "tank/1.json", b"old") == "reused"
    assert (out / "tank/1.json").stat().st_ino == inode, "unchanged must keep inode"
    assert P.overlay_bytes(out, "tank/1.json", b"new") == "updated"
    assert (base / "tank/1.json").read_text(encoding="utf-8") == "old", "base must not be written"
    src = tmp / "src_tank"
    src.mkdir()
    (src / "1.json").write_text("new2", encoding="utf-8")
    report: dict[str, int] = {}
    P.sync_tree(out, "tank", src, report)
    assert (out / "tank/1.json").read_text(encoding="utf-8") == "new2"
    assert not (out / "tank/2.json").exists(), "stale item must be deleted"
    assert report.get("deleted") == 1, report


def dry_run_and_fail_closed(tmp: pathlib.Path) -> None:
    base = tmp / "pack"
    (base / "data").mkdir(parents=True)
    (base / "data/data_version.json").write_text("{}", encoding="utf-8")
    (base / "data/tanks.pb").write_bytes(b"")
    (base / "data/tank_cache.json").write_text("{}", encoding="utf-8")
    out = tmp / "out"

    icons = _plan({"tank-icons": {"added": 1, "changed": 0, "removed": 0}},
                  {"1": {"nation": "usa", "stem": "A1", "kinds": ["tank-icons"],
                         "files": {"added": ["Gfx/UI/BigTankIcons/A1.packed.webp.dvpl"]}}})
    result = run_apply(base, out, icons, "--dry-run", "--only", "vehicle-assets")
    assert result.returncode == 0, result.stderr
    assert "export_tank_icons.py" in result.stdout, result.stdout
    assert not out.exists(), "dry-run must not create the pack"

    unmapped = _plan({"unmapped": {"added": 1, "changed": 0, "removed": 0}})
    result = run_apply(base, out, unmapped)
    assert result.returncode == 2, (result.returncode, result.stderr)
    assert "未识别数据面" in result.stderr, result.stderr
    assert not out.exists(), "must fail before clone"

    registry = _plan({"maps": {"added": 1, "changed": 0, "removed": 0}})
    registry["mapRegistryChanged"] = True
    result = run_apply(base, out, registry)
    assert result.returncode != 0, "map registry change must fail closed"


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        clone_keeps_nested_metadata(tmp)
        incremental_overlay(tmp)
        dry_run_and_fail_closed(tmp)
    print("apply data version tests: PASS")


if __name__ == "__main__":
    main()
