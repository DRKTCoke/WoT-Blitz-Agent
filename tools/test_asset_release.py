"""asset_release.py 的契约测试（standalone，无 pytest 依赖）。

覆盖：content_digest 的内容稳定性、manifest 完备性 fail-closed、dataset id 与目录名
一致性、catalog 排序/default、delta 的 added/modified/removed。

用法：python tools/test_asset_release.py
"""
import hashlib
import json
import os
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

import asset_release as ar  # noqa: E402


def _write_pack(
    root: pathlib.Path,
    files: dict[str, bytes],
    *,
    generated: str = "2026-11-01T00:00:00+00:00",
    game_version: str = "release/11.21.0",
    tank_count: int = 2,
    preview: dict | None = None,
    upstream_commit: str = "a" * 40,
    worktree_dirty: bool = False,
) -> pathlib.Path:
    root.mkdir(parents=True, exist_ok=True)
    entries = []
    for rel, data in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        entries.append(
            {"path": rel, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        )
    data_version = {
        "game_version": game_version,
        "tank_cache_updated_at": generated,
        "game_data_updated_at": generated,
        "tank_count": tank_count,
        "game_data_files": tank_count,
    }
    if preview is not None:
        data_version["preview"] = preview
    manifest = {
        "version": 1,
        "generated": generated,
        "upstream_commit": upstream_commit,
        "worktree_dirty": worktree_dirty,
        "data_version": data_version,
        "files": sorted(entries, key=lambda item: item["path"]),
    }
    (root / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    return root


def _stamp(root: pathlib.Path, dataset_id: str, **kwargs) -> dict:
    version = ar.build_version(
        root,
        dataset_id=dataset_id,
        game_version=kwargs.pop("game_version", None),
        variant=kwargs.pop("variant", None),
        label=kwargs.pop("label", None),
        release=kwargs.pop("release", None),
        min_client=kwargs.pop("min_client", None),
        notes=kwargs.pop("notes", None),
        status=kwargs.pop("status", "active"),
    )
    ar.write_json(root / "version.json", version)
    return version


BASE_FILES = {"data/a.json": b"alpha", "map/x.webp": b"webp-bytes"}


def test_verify_pack_checks_content_for_preflight():
    """dataset.sh link 的切换前体检：verify --pack 对坏包 fail-closed。"""
    import argparse

    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "11.21.0", BASE_FILES)
        _stamp(pack, "11.21.0", game_version="release/11.21.0")
        args = argparse.Namespace(root=None, pack=str(pack), catalog=None, full=False)
        assert ar.command_verify(args) == 0

        # manifest 登记的文件缺失 → 候选体检必须失败（在线 dataset 尚未切换）
        (pack / "data/a.json").unlink()
        try:
            ar.command_verify(argparse.Namespace(root=None, pack=str(pack), catalog=None, full=False))
            raise AssertionError("broken candidate must fail verify --pack")
        except ar.ReleaseError as exc:
            assert "data/a.json" in str(exc)


def test_stamp_writes_identity_and_digest():
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "11.21.0", BASE_FILES)
        version = _stamp(pack, "11.21.0", game_version="release/11.21.0")
        assert version["dataset_id"] == "11.21.0"
        assert version["game_version"] == "release/11.21.0"
        assert version["content_digest"].startswith("sha256:")
        assert version["manifest_sha256"] == hashlib.sha256(
            (pack / "manifest.json").read_bytes()
        ).hexdigest()
        assert version["tank_count"] == 2
        assert version["release"]
        verified = ar.verify_dataset(pack, full=True)
        assert verified["content_digest"] == version["content_digest"]


def test_digest_ignores_generated_provenance_and_stamp_time():
    with tempfile.TemporaryDirectory() as tmp:
        a = _write_pack(pathlib.Path(tmp) / "a", BASE_FILES, generated="2026-10-01T00:00:00+00:00")
        b = _write_pack(
            pathlib.Path(tmp) / "b",
            BASE_FILES,
            generated="2026-11-05T12:00:00+00:00",
            upstream_commit="b" * 40,
            worktree_dirty=True,
        )
        va = ar.build_version(
            a, dataset_id="11.21.0", game_version=None, variant=None, label=None,
            release=None, min_client=None, notes=None, status="active",
        )
        vb = ar.build_version(
            b, dataset_id="11.21.0", game_version=None, variant=None, label=None,
            release=None, min_client=None, notes=None, status="active",
        )
        assert va["content_digest"] == vb["content_digest"]
        assert va["manifest_sha256"] != vb["manifest_sha256"]


def test_digest_changes_with_content():
    with tempfile.TemporaryDirectory() as tmp:
        a = _write_pack(pathlib.Path(tmp) / "a", BASE_FILES)
        b = _write_pack(pathlib.Path(tmp) / "b", {**BASE_FILES, "data/a.json": b"beta"})
        va = ar.build_version(
            a, dataset_id="11.21.0", game_version=None, variant=None, label=None,
            release=None, min_client=None, notes=None, status="active",
        )
        vb = ar.build_version(
            b, dataset_id="11.21.0", game_version=None, variant=None, label=None,
            release=None, min_client=None, notes=None, status="active",
        )
        assert va["content_digest"] != vb["content_digest"]


def test_stamp_rejects_unlisted_content_file():
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "11.21.0", BASE_FILES)
        (pack / "extra.txt").write_text("not in manifest", encoding="utf-8")
        try:
            ar.build_version(
                pack, dataset_id="11.21.0", game_version=None, variant=None, label=None,
                release=None, min_client=None, notes=None, status="active",
            )
        except ar.ReleaseError as exc:
            assert "未登记" in str(exc)
        else:
            raise AssertionError("未登记的文件必须 fail-closed")


def test_stamp_ignores_release_metadata_and_overhead():
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "11.21.0", BASE_FILES)
        (pack / "overhead").mkdir()
        (pack / "overhead" / "render.webp").write_bytes(b"intermediate")
        (pack / "delta.json").write_text("{}", encoding="utf-8")
        version = _stamp(pack, "11.21.0", game_version="release/11.21.0")
        assert version["content_digest"].startswith("sha256:")


def test_stamp_rejects_missing_manifest_file():
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "11.21.0", BASE_FILES)
        (pack / "data/a.json").unlink()
        try:
            ar.build_version(
                pack, dataset_id="11.21.0", game_version=None, variant=None, label=None,
                release=None, min_client=None, notes=None, status="active",
            )
        except ar.ReleaseError as exc:
            assert "不存在" in str(exc)
        else:
            raise AssertionError("缺文件必须 fail-closed")


def test_stamp_accepts_preview_variant():
    preview = {"client_version": "11.22_supertest_tier11_4", "tanks": [15153, 27905]}
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(
            pathlib.Path(tmp) / "11.21.0-t11-preview",
            BASE_FILES,
            game_version="release/11.21.0+t11-preview",
            tank_count=740,
            preview=preview,
        )
        version = _stamp(
            pack, "11.21.0-t11-preview", game_version="release/11.21.0", variant="t11-preview"
        )
        assert version["game_version"] == "release/11.21.0"
        assert version["variant"] == "t11-preview"
        assert version["preview"] == preview
        assert version["tank_count"] == 740


def test_stamp_rejects_mismatched_game_version():
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "11.21.0", BASE_FILES, game_version="release/11.21.0")
        try:
            ar.build_version(
                pack, dataset_id="11.21.0", game_version="release/11.22.0", variant=None,
                label=None, release=None, min_client=None, notes=None, status="active",
            )
        except ar.ReleaseError as exc:
            assert "game_version" in str(exc)
        else:
            raise AssertionError("game_version 不一致必须 fail-closed")


def test_catalog_orders_versions_and_keeps_default():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp) / "datasets"
        for dataset_id, game_version, files in (
            ("11.21.0", "release/11.21.0", BASE_FILES),
            ("11.21.0-t11-preview", "release/11.21.0+t11-preview", {**BASE_FILES, "tank/1.json": b"preview"}),
            ("11.22.0", "release/11.22.0", {**BASE_FILES, "data/a.json": b"alpha2"}),
        ):
            preview = {"client_version": "x"} if dataset_id.endswith("preview") else None
            pack = _write_pack(root / dataset_id, files, game_version=game_version, preview=preview)
            _stamp(
                pack, dataset_id,
                game_version="release/11.21.0" if dataset_id.endswith("preview") else None,
                variant="t11-preview" if dataset_id.endswith("preview") else None,
            )
        out = pathlib.Path(tmp) / "versions.json"
        args = type("Args", (), {"root": str(root), "out": str(out), "default": "11.21.0", "full": False, "dry_run": False})()
        assert ar.command_catalog(args) == 0
        catalog = json.loads(out.read_text(encoding="utf-8"))
        assert [item["id"] for item in catalog["datasets"]] == [
            "11.22.0", "11.21.0", "11.21.0-t11-preview"
        ]
        assert catalog["default"] == "11.21.0"
        assert ar.command_verify(type("Args", (), {"root": str(root), "catalog": str(out), "full": False})()) == 0


def test_catalog_rejects_dataset_id_directory_mismatch():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp) / "datasets"
        pack = _write_pack(root / "11.21.0", BASE_FILES)
        version = _stamp(pack, "11.21.0")
        version["dataset_id"] = "11.21.0-other"
        ar.write_json(pack / "version.json", version)
        try:
            ar.collect_datasets(root, full=False)
        except ar.ReleaseError as exc:
            assert "目录名不一致" in str(exc)
        else:
            raise AssertionError("dataset_id 与目录名不一致必须 fail-closed")


def test_delta_reports_added_modified_removed():
    with tempfile.TemporaryDirectory() as tmp:
        old = _write_pack(pathlib.Path(tmp) / "old", BASE_FILES)
        _stamp(old, "11.21.0", release="20261101-aaaaaaaa")
        new = _write_pack(
            pathlib.Path(tmp) / "new",
            {"data/a.json": b"beta", "map/y.webp": b"new-map"},
            generated="2026-11-05T00:00:00+00:00",
        )
        _stamp(new, "11.21.0", release="20261105-bbbbbbbb")
        out = pathlib.Path(tmp) / "delta.json"
        args = type("Args", (), {"old": str(old), "new": str(new), "out": str(out), "base_release": None})()
        assert ar.command_delta(args) == 0
        delta = json.loads(out.read_text(encoding="utf-8"))
        assert delta["base_release"] == "20261101-aaaaaaaa"
        assert delta["release"] == "20261105-bbbbbbbb"
        assert {(c["path"], c["change"]) for c in delta["changes"]} == {
            ("data/a.json", "modified"),
            ("map/x.webp", "removed"),
            ("map/y.webp", "added"),
        }
        assert delta["dataset_id"] == "11.21.0"


def test_stamp_dry_run_does_not_write_version():
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "11.21.0", BASE_FILES)
        args = type("Args", (), {
            "pack": str(pack), "dataset_id": "11.21.0", "game_version": None, "variant": None,
            "label": None, "release": None, "min_client": None, "notes": None,
            "status": "active", "dry_run": True,
        })()
        assert ar.command_stamp(args) == 0
        assert not (pack / "version.json").exists()


def test_write_json_does_not_truncate_hardlinked_sibling():
    # 2026-11-07 r8 事故回归：cp -al 硬链接克隆后原地 write_text 会截断共享 inode，
    # 把上一 release 的同名文件一并改坏；write_json 必须 temp + os.replace 换 inode。
    with tempfile.TemporaryDirectory() as tmp:
        prev = pathlib.Path(tmp) / "prev"
        nxt = pathlib.Path(tmp) / "next"
        prev.mkdir()
        nxt.mkdir()
        original = '{"schema": 1, "release": "prev"}\n'
        (prev / "version.json").write_text(original, encoding="utf-8")
        os.link(prev / "version.json", nxt / "version.json")
        assert (prev / "version.json").stat().st_nlink == 2
        ar.write_json(nxt / "version.json", {"schema": 1, "release": "next"})
        assert (prev / "version.json").read_text(encoding="utf-8") == original
        assert json.loads((nxt / "version.json").read_text(encoding="utf-8"))["release"] == "next"
        assert (prev / "version.json").stat().st_nlink == 1
        assert not (nxt / "version.json.tmp").exists()


if __name__ == "__main__":
    fns = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for fn in fns:
        fn()
        print(f"PASS {fn.__name__}")
    print(f"{len(fns)} tests passed")