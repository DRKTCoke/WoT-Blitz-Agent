"""asset_store.py 的去重/硬链接/GC 契约测试（standalone）。

覆盖：
  * import 按 sha256 去重（同一内容多个路径 → 一个对象）
  * materialize 建立对象硬链接（同 inode）+ 复制 manifest/version 元数据
  * adopt 把已有 release 的重复文件原地改成对象硬链接
  * gc 只删除无 manifest 引用且无 release 硬链接的对象；dry-run 不落盘
  * stats 的唯一内容 / 逻辑总量 / 去重率

用法：python tools/test_asset_store.py
"""
import hashlib
import json
import os
import pathlib
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "tools"))

import asset_store as store  # noqa: E402


def _write_pack(root: pathlib.Path, files: dict[str, bytes], *, version: dict | None = None) -> pathlib.Path:
    root.mkdir(parents=True, exist_ok=True)
    entries = []
    for rel, data in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        entries.append({"path": rel, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    manifest = {
        "version": 1,
        "generated": "2026-11-01T00:00:00+00:00",
        "data_version": {"game_version": "release/11.21.0"},
        "files": sorted(entries, key=lambda item: item["path"]),
    }
    (root / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=1), encoding="utf-8")
    if version:
        (root / "version.json").write_text(json.dumps(version), encoding="utf-8")
    return root


def _same_inode(a: pathlib.Path, b: pathlib.Path) -> bool:
    sa, sb = a.stat(), b.stat()
    return (sa.st_ino, sa.st_dev) == (sb.st_ino, sb.st_dev)


def test_import_dedupes_identical_content_across_paths():
    with tempfile.TemporaryDirectory() as tmp:
        pack = _write_pack(pathlib.Path(tmp) / "pack", {"a.bin": b"same", "b.bin": b"same"})
        objects = pathlib.Path(tmp) / "objects"
        result = store.import_pack(pack, objects, copy=False, verify=True, dry_run=False)
        assert result["files"] == 2
        assert result["unique_objects"] == 1
        assert result["added"] == 1
        assert result["reused"] == 1
        assert result["logical_bytes"] == 8
        assert result["unique_bytes"] == 4
        assert len(list(store.iter_objects(objects))) == 1


def test_materialize_creates_hardlinks_and_metadata():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        pack = _write_pack(root / "pack", {"a.bin": b"same", "b.bin": b"same"}, version={"schema": 1, "dataset_id": "11.21.0"})
        objects = root / "objects"
        store.import_pack(pack, objects, copy=False, verify=True, dry_run=False)
        releases = root / "releases"
        out = releases / "r1"
        result = store.materialize(pack / "manifest.json", objects, out, metadata=pack, copy=False, force=False, dry_run=False)
        assert result["linked"] == 2
        assert (out / "manifest.json").is_file()
        assert (out / "version.json").is_file()
        sha = hashlib.sha256(b"same").hexdigest()
        obj = store.object_path(objects, sha)
        assert _same_inode(out / "a.bin", obj)
        assert _same_inode(out / "b.bin", obj)
        assert store.verify(objects, releases, full=True, links=True)["checked"] == 2


def test_stats_reports_dedupe_saving():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        pack = _write_pack(root / "pack", {"a.bin": b"same", "b.bin": b"same"})
        objects = root / "objects"
        store.import_pack(pack, objects, copy=False, verify=True, dry_run=False)
        out = root / "releases" / "r1"
        store.materialize(pack / "manifest.json", objects, out, metadata=pack, copy=False, force=False, dry_run=False)
        result = store.stats(objects, root / "releases")
        assert result["objects"] == 1
        assert result["unique_bytes"] == 4
        assert result["logical_bytes"] == 8
        assert abs(result["dedupe_saving"] - 0.5) < 1e-9


def test_adopt_relinks_existing_release_duplicates():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        release = _write_pack(root / "r1", {"a.bin": b"same", "b.bin": b"same"})
        assert not _same_inode(release / "a.bin", release / "b.bin")
        objects = root / "objects"
        result = store.adopt_release(release, objects, dry_run=False)
        assert result["files"] == 2
        assert len(list(store.iter_objects(objects))) == 1
        assert _same_inode(release / "a.bin", release / "b.bin")


def test_gc_removes_only_unreferenced_objects():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        objects = root / "objects"
        releases = root / "releases"
        old_pack = _write_pack(root / "old", {"old.bin": b"old"})
        store.import_pack(old_pack, objects, copy=False, verify=True, dry_run=False)
        store.materialize(old_pack / "manifest.json", objects, releases / "r1", metadata=old_pack, copy=False, force=False, dry_run=False)
        new_pack = _write_pack(root / "new", {"new.bin": b"new"})
        store.import_pack(new_pack, objects, copy=False, verify=True, dry_run=False)
        store.materialize(new_pack / "manifest.json", objects, releases / "r2", metadata=new_pack, copy=False, force=False, dry_run=False)
        assert len(list(store.iter_objects(objects))) == 2

        # r1 仍在：两个对象都有引用/硬链接，GC 不删
        assert store.gc(objects, releases, dry_run=True, min_age_seconds=0)["removed"] == 0
        import shutil
        shutil.rmtree(releases / "r1")
        shutil.rmtree(root / "old")   # 生产上 staging pack 也应在 gc 前清理
        dry = store.gc(objects, releases, dry_run=True, min_age_seconds=0)
        assert dry["removed"] == 1 and dry["freed_bytes"] == 3
        assert len(list(store.iter_objects(objects))) == 2
        real = store.gc(objects, releases, dry_run=False, min_age_seconds=0)
        assert real["removed"] == 1
        assert len(list(store.iter_objects(objects))) == 1
        assert (releases / "r2" / "new.bin").read_bytes() == b"new"


def test_incremental_import_uses_existing_objects_and_staged_changes():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        base = _write_pack(root / "base", {"a.bin": b"a"})
        objects = root / "objects"
        store.import_pack(base, objects, copy=False, verify=True, dry_run=False)
        full = _write_pack(root / "full", {"a.bin": b"a", "b.bin": b"b"})
        staging = root / "staging"
        (staging).mkdir()
        (staging / "b.bin").write_bytes(b"b")
        result = store.import_manifest(full / "manifest.json", staging, objects, copy=False, verify=True, dry_run=False)
        assert result["files"] == 2
        assert result["reused"] == 1
        assert result["added"] == 1
        assert len(list(store.iter_objects(objects))) == 2


def test_incremental_import_fails_when_entry_is_neither_object_nor_staged():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        full = _write_pack(root / "full", {"c.bin": b"c"})
        empty = root / "empty"
        empty.mkdir()
        try:
            store.import_manifest(full / "manifest.json", empty, root / "objects", copy=False, verify=True, dry_run=False)
        except store.StoreError as exc:
            assert "既不是已有对象" in str(exc)
        else:
            raise AssertionError("缺对象且缺源文件必须 fail-closed")


def test_dry_run_does_not_write():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        pack = _write_pack(root / "pack", {"a.bin": b"a"})
        objects = root / "objects"
        result = store.import_pack(pack, objects, copy=False, verify=True, dry_run=True)
        assert result["added"] == 1
        assert not objects.exists()
        store.import_pack(pack, objects, copy=False, verify=True, dry_run=False)
        out = root / "release"
        store.materialize(pack / "manifest.json", objects, out, metadata=pack, copy=False, force=False, dry_run=True)
        assert not (out / "a.bin").exists()


def test_materialize_rejects_non_empty_out_without_force():
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)
        pack = _write_pack(root / "pack", {"a.bin": b"a"})
        objects = root / "objects"
        store.import_pack(pack, objects, copy=False, verify=True, dry_run=False)
        out = root / "release"
        out.mkdir()
        (out / "keep").write_text("x", encoding="utf-8")
        try:
            store.materialize(pack / "manifest.json", objects, out, metadata=pack, copy=False, force=False, dry_run=False)
        except store.StoreError as exc:
            assert "非空" in str(exc)
        else:
            raise AssertionError("非空输出目录必须 fail-closed")


if __name__ == "__main__":
    fns = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for fn in fns:
        fn()
        print(f"PASS {fn.__name__}")
    print(f"{len(fns)} tests passed")