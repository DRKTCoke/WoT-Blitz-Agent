#!/usr/bin/env python3
"""`tools/apply_data_version.py` / `pack_incremental.py` 的独立回归测试。

覆盖：
1. 硬链接克隆只跳过根目录发布元数据，嵌套同名文件必须保留；
2. 未识别数据面 fail-closed；
3. dry-run 不写 pack，且按 handler 分发 vehicle/core/maps 命令。
"""
from __future__ import annotations

import json
import os
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
        [sys.executable, "-X", "utf8", "-I", str(TOOL), "--base-pack", str(base), "--out-pack", str(out),
         "--game-data", str(out.parent), "--plan", str(plan_path), *extra],
        cwd=ROOT, capture_output=True, encoding="utf-8",
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


def reviewed_covers(tmp: pathlib.Path) -> None:
    from PIL import Image
    import pack_incremental as P
    from export_tank_icons import emit_crops
    from tank_image_overrides import apply_covers, matches_dataset

    base = tmp / "raw-pack"
    raw = base / "tank_images"
    raw.mkdir(parents=True)
    out = tmp / "candidate"
    covers = tmp / "covers"
    covers.mkdir()
    original = raw / "1.webp"
    render = covers / "1.webp"
    im = Image.new("RGBA", (16, 8))
    im.paste("yellow", (1, 2, 5, 6))
    im.save(original, lossless=True)
    os.link(original, raw / "2.webp")  # Non-target with the same placeholder.
    im.paste("grey", (2, 3, 12, 8))
    im.save(render, lossless=True)
    emit_crops(raw, raw / "crops.json")
    base_crop = (raw / "crops.json").read_bytes()
    model = base / "glb/1/model.glb"
    config = base / "tank/1.json"
    for path, data in [(model, b"reviewed model"), (config, b"{}")]:
        path.parent.mkdir(parents=True)
        path.write_bytes(data)
    row = {"enabled": True, "sha256": P.sha256_file(render),
           "model_sha256": P.sha256_file(model), "tank_sha256": P.sha256_file(config)}
    catalog = {"dataset_id": "11.4.0.138", "preview_source": "reviewed.zip",
               "placeholder_sha256": P.sha256_file(original), "vehicles": {"1": row}}
    assert matches_dataset(catalog, None, "reviewed.zip")
    assert not matches_dataset(catalog, "another", "reviewed.zip")
    assert not matches_dataset(catalog, "11.4.0.138", "next-client.zip")
    assert matches_dataset(catalog, None, "reviewed.zip", "11.4.0.138-r5")
    assert not matches_dataset(catalog, None, "next-client.zip", "11.4.0.138")
    assert not matches_dataset(catalog, None, "another.zip")
    P.clone_base(base, out)

    def apply(eligible=True):
        return apply_covers(out, raw, eligible, catalog, tmp)["1"]

    assert apply() == "render"
    dest = out / "tank_images/1.webp"
    crops = out / "tank_images/crops.json"
    assert dest.read_bytes() == render.read_bytes()
    crop_hash = P.sha256_file(crops)
    assert json.loads(crops.read_text(encoding="utf-8"))["icons"]["1"]["w"] == 11
    assert (raw / "crops.json").read_bytes() == base_crop
    assert P.sha256_file(original) == catalog["placeholder_sha256"]
    assert os.path.samefile(raw / "2.webp", out / "tank_images/2.webp")
    assert apply() == "render" and P.sha256_file(crops) == crop_hash

    for key in ("model_sha256", "tank_sha256", "sha256"):
        saved = row[key]
        row[key] = "0" * 64
        assert apply().startswith("original")
        assert dest.read_bytes() == original.read_bytes()
        row[key] = saved
    row["enabled"] = False
    assert apply().startswith("original")
    row["enabled"] = True
    assert apply(False).startswith("original") and crops.read_bytes() == base_crop
    render.rename(render.with_suffix(".away"))
    assert apply().startswith("original") and dest.read_bytes() == original.read_bytes()
    render.with_suffix(".away").rename(render)
    assert apply() == "render"

    # The next client ships an official image; the base is already rendered.
    fresh = tmp / "new-client"
    fresh.mkdir()
    im.paste("olive", (4, 2, 15, 8))
    im.save(fresh / "1.webp", lossless=True)
    result = apply_covers(out, fresh, True, catalog, tmp)
    assert "source changed" in result["1"]
    assert dest.read_bytes() == (fresh / "1.webp").read_bytes()
    assert json.loads(crops.read_text(encoding="utf-8"))["icons"]["1"]["w"] == 14
    (fresh / "1.webp").unlink()
    try:
        apply_covers(out, fresh, True, catalog, tmp)
    except ValueError as exc:
        assert "original client icon unavailable" in str(exc)
    else:
        raise AssertionError("A missing raw input must not allow an inherited render to ship")


def source_transition(tmp: pathlib.Path) -> None:
    """A plan with zero vehicle changes can still transition away from a rendered base."""
    from PIL import Image
    from export_tank_icons import emit_crops
    from tank_image_overrides import CATALOG, load_catalog

    base = tmp / "previous-release"
    images = base / "tank_images"
    images.mkdir(parents=True)
    rows = load_catalog()["vehicles"]
    tid = next(iter(rows))
    name = f"{tid}.webp"
    approved = (CATALOG.parent / "covers" / name).read_bytes()
    (images / name).write_bytes(approved)
    emit_crops(images, images / "crops.json")
    (base / "version.json").write_text('{"dataset_id":"11.4.0.138-r5"}', encoding="utf-8")
    original_dir = tmp / "official-icons"
    original_dir.mkdir()
    Image.new("RGBA", (16, 8), "olive").save(original_dir / name, lossless=True)
    plan = _plan({})
    plan.update(new="https://cdn.example/new-client.zip",
                versions={"base": "11.4.0.138-r5", "new": "next"})
    candidate = tmp / "next-release"
    result = run_apply(base, candidate, plan, "--raw-tank-images", str(original_dir))
    assert result.returncode == 0, result.stderr
    assert (candidate / "tank_images" / name).read_bytes() == (original_dir / name).read_bytes()
    assert (images / name).read_bytes() == approved
    manifest = json.loads((candidate / "manifest.json").read_text(encoding="utf-8"))
    from pack_incremental import sha256_file
    file = next(f for f in manifest["files"] if f["path"] == f"tank_images/{name}")
    assert file["sha256"] == sha256_file(original_dir / name)
    # A published destination must survive both forms of the full pack CLI.
    for mode in [("--refresh-manifest",), ("--map-index", str(tmp / "unused.json"))]:
        command = [sys.executable, "-X", "utf8", "-I", str(ROOT / "scripts/export_asset_pack.py"),
                   "--out", str(base), *mode]
        error = subprocess.run(command, capture_output=True, encoding="utf-8")
        assert error.returncode == 2 and "已 stamp" in error.stderr, error.stderr
    assert (images / name).read_bytes() == approved


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        clone_keeps_nested_metadata(tmp)
        incremental_overlay(tmp)
        reviewed_covers(tmp)
        source_transition(tmp)
        dry_run_and_fail_closed(tmp)
    print("apply data version tests: PASS")


if __name__ == "__main__":
    main()
