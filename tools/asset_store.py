"""资产包的内容寻址对象库（content-addressed store）+ 硬链接 release 树。

背景：`releases/<release-id>/` 是可变的完整逻辑树；发布时用 `cp -al` 从上一
release 硬链接复制，未变文件共享 inode。但这条链只对**同一 lineage**有效：
不同 dataset 从不同 base 构建、或包内同一内容出现在多个路径时，仍会重复占盘。

本工具把内容收敛到 `objects/<sha[:2]>/<sha256>`（每个唯一内容一个 inode），
release 树只是指向对象的硬链接。Caddy 仍按普通静态文件伺服，无需动态服务。

  objects/<sha[:2]>/<sha256>     # 唯一内容，文件名即 sha256
  releases/<release-id>/...      # 硬链接到 objects；目录真实存在
  datasets/<dataset-id> -> ../releases/<release-id>
  current -> datasets/<default-id>

命令：
  import      把 pack 的全部内容登记进 objects（默认硬链接，跨盘用 --copy）
  materialize 按 manifest 从 objects 构建 release 树（硬链接 + 拷贝少量元数据）
  adopt       把已有 release 树原地改成对象硬链接（迁移/去重）
  gc          删除未被任何保留 release 引用、且没有 release 硬链接的对象
  stats       输出唯一内容 / 逻辑总量 / 去重率
  verify      校验 release 的每个 manifest 条目都有对象且大小一致

约束：
  * manifest 的逐文件 sha256 仍是完整性权威；对象名只是内容身份。
  * import 默认硬链接（同文件系统零拷贝）；跨盘 staging 用 --copy 写入 objects。
  * materialize/adopt 需要 hardlink；跨盘会 fail-closed，提示先把 objects 放到同盘。
  * gc 只删“无 manifest 引用 且 st_nlink == 1”的对象，避免误删仍在使用的硬链接。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sys
import time
from pathlib import Path, PurePosixPath

OBJECT_SHARD = 2
METADATA_FILES = ("version.json", "delta.json")


class StoreError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_manifest(path: Path) -> dict:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise StoreError(f"缺少 manifest: {path}") from None
    except json.JSONDecodeError as exc:
        raise StoreError(f"manifest JSON 解析失败: {path}: {exc}") from None
    if not isinstance(manifest, dict) or manifest.get("version") != 1:
        raise StoreError(f"manifest schema 不支持: {path}")
    files = manifest.get("files")
    if not isinstance(files, list):
        raise StoreError(f"manifest.files 必须是数组: {path}")
    seen: set[str] = set()
    for entry in files:
        if not isinstance(entry, dict):
            raise StoreError(f"manifest.files 条目必须是对象: {path}")
        rel = entry.get("path")
        sha = entry.get("sha256")
        if not isinstance(rel, str) or not rel:
            raise StoreError(f"manifest.files 条目缺 path: {path}")
        pure = PurePosixPath(rel)
        if pure.is_absolute() or ".." in pure.parts or "\\" in rel:
            raise StoreError(f"manifest.files 非法 path: {rel!r}")
        if rel in seen:
            raise StoreError(f"manifest.files 重复 path: {rel}")
        seen.add(rel)
        if not isinstance(sha, str) or len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha):
            raise StoreError(f"manifest.files 非法 sha256: {rel}")
    return manifest


def object_path(objects: Path, sha: str) -> Path:
    return objects / sha[:OBJECT_SHARD] / sha


def ensure_shard(objects: Path, sha: str) -> Path:
    shard = objects / sha[:OBJECT_SHARD]
    shard.mkdir(parents=True, exist_ok=True)
    return shard


def link_or_copy(source: Path, target: Path, *, copy: bool) -> None:
    """原子写入 target；默认 hardlink，--copy 时复制。"""
    target.parent.mkdir(parents=True, exist_ok=True)
    tmp = target.with_name(f".tmp-{target.name}-{os.getpid()}")
    try:
        if tmp.exists() or tmp.is_symlink():
            tmp.unlink()
        if copy:
            shutil.copy2(source, tmp)
        else:
            os.link(source, tmp)
        os.replace(tmp, target)
    finally:
        if tmp.exists() or tmp.is_symlink():
            try:
                tmp.unlink()
            except OSError:
                pass


def import_manifest(manifest_path: Path, files_root: Path, objects: Path, *, copy: bool, verify: bool, dry_run: bool) -> dict:
    """按 manifest 登记内容。

    对象已存在 → 跳过（不要求源文件存在）；对象不存在 → 从 files_root 导入；
    两者都没有 → fail-closed 并列出缺项。这样既支持完整 pack，也支持“只上传变更文件”的
    增量 staging（manifest 仍是完整的，未变内容命中已有对象）。
    """
    manifest = load_manifest(manifest_path)
    added = 0
    reused = 0
    logical = 0
    unique: set[str] = set()
    missing: list[str] = []
    for entry in manifest["files"]:
        rel, sha, size = entry["path"], entry["sha256"], entry.get("bytes")
        logical += int(size or 0)
        unique.add(sha)
        obj = object_path(objects, sha)
        if obj.is_file():
            if isinstance(size, int) and obj.stat().st_size != size:
                raise StoreError(f"对象大小与 manifest 不符: {obj}（{obj.stat().st_size} vs {size}）")
            reused += 1
            continue
        source = files_root / rel
        if not source.is_file():
            missing.append(rel)
            continue
        actual_size = source.stat().st_size
        if isinstance(size, int) and size != actual_size:
            raise StoreError(f"文件大小不符: {rel}（manifest {size}，实际 {actual_size}）")
        if verify:
            actual = sha256_file(source)
            if actual != sha:
                raise StoreError(f"文件 sha256 不符: {rel}（期望 {sha}，实际 {actual}）")
        added += 1
        if not dry_run:
            link_or_copy(source, obj, copy=copy)
    if missing:
        sample = ", ".join(missing[:8])
        raise StoreError(f"{len(missing)} 个 manifest 条目既不是已有对象、也没有源文件: {sample}")
    unique_bytes = sum(object_path(objects, sha).stat().st_size for sha in unique if object_path(objects, sha).is_file())
    return {
        "files": len(manifest["files"]),
        "logical_bytes": logical,
        "unique_objects": len(unique),
        "unique_bytes": unique_bytes,
        "added": added,
        "reused": reused,
        "missing": missing,
        "dry_run": dry_run,
    }


def import_pack(pack: Path, objects: Path, *, copy: bool, verify: bool, dry_run: bool) -> dict:
    return import_manifest(pack / "manifest.json", pack, objects, copy=copy, verify=verify, dry_run=dry_run)


def materialize(manifest_path: Path, objects: Path, out: Path, *, metadata: Path | None, copy: bool, force: bool, dry_run: bool) -> dict:
    manifest = load_manifest(manifest_path)
    if out.exists() and any(out.iterdir()):
        if not force:
            raise StoreError(f"输出目录非空（拒绝覆盖）: {out}")
        shutil.rmtree(out)
    out.mkdir(parents=True, exist_ok=True)
    linked = 0
    copied = 0
    for entry in manifest["files"]:
        rel, sha, size = entry["path"], entry["sha256"], entry.get("bytes")
        obj = object_path(objects, sha)
        if not obj.is_file():
            raise StoreError(f"缺少对象: {sha}（{rel}）；先运行 import")
        if isinstance(size, int) and obj.stat().st_size != size:
            raise StoreError(f"对象大小与 manifest 不符: {sha}")
        target = out / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists():
            raise StoreError(f"输出文件已存在: {target}")
        if dry_run:
            linked += 1
            continue
        if copy:
            shutil.copy2(obj, target)
            copied += 1
        else:
            try:
                os.link(obj, target)
            except OSError as exc:
                raise StoreError(f"硬链接失败（objects 与 release 必须在同一文件系统；可 --copy）: {target}: {exc}") from None
            linked += 1
    # manifest 自身 + 元数据（不属于内容清单）
    if not dry_run:
        shutil.copy2(manifest_path, out / "manifest.json")
        if metadata:
            for name in METADATA_FILES:
                src = metadata / name
                if src.is_file():
                    shutil.copy2(src, out / name)
    return {"files": len(manifest["files"]), "linked": linked, "copied": copied, "dry_run": dry_run}


def adopt_release(release: Path, objects: Path, *, dry_run: bool) -> dict:
    manifest = load_manifest(release / "manifest.json")
    adopted = 0
    skipped = 0
    for entry in manifest["files"]:
        rel, sha = entry["path"], entry["sha256"]
        size = entry.get("bytes")
        source = release / rel
        if not source.is_file():
            raise StoreError(f"release 缺文件: {source}")
        actual_size = source.stat().st_size
        if isinstance(size, int) and size != actual_size:
            raise StoreError(f"release 文件大小与 manifest 不符: {rel}（{actual_size} vs {size}）")
        obj = object_path(objects, sha)
        if obj.is_file():
            if isinstance(size, int) and obj.stat().st_size != size:
                raise StoreError(f"对象大小与 manifest 不符: {obj}")
        else:
            if dry_run:
                adopted += 1
                continue
            link_or_copy(source, obj, copy=False)
        if source.stat().st_ino == obj.stat().st_ino and source.stat().st_dev == obj.stat().st_dev:
            skipped += 1
            continue
        adopted += 1
        if dry_run:
            continue
        tmp = source.with_name(f".{source.name}.adopt-{os.getpid()}")
        if tmp.exists() or tmp.is_symlink():
            tmp.unlink()
        try:
            os.link(obj, tmp)
            os.replace(tmp, source)
        except OSError as exc:
            raise StoreError(f"adopt 硬链接失败: {source}: {exc}") from None
        finally:
            if tmp.exists() or tmp.is_symlink():
                try:
                    tmp.unlink()
                except OSError:
                    pass
    return {"files": len(manifest["files"]), "adopted": adopted, "already_linked": skipped, "dry_run": dry_run}


def referenced_objects(releases: Path) -> set[str]:
    refs: set[str] = set()
    if not releases.is_dir():
        return refs
    for release in releases.iterdir():
        manifest = release / "manifest.json"
        if release.is_dir() and manifest.is_file():
            for entry in load_manifest(manifest)["files"]:
                refs.add(entry["sha256"])
    return refs


def iter_objects(objects: Path):
    if not objects.is_dir():
        return
    for shard in objects.iterdir():
        if not shard.is_dir():
            continue
        for obj in shard.iterdir():
            if obj.is_file() and not obj.name.startswith("."):
                yield obj


def gc(objects: Path, releases: Path, *, dry_run: bool, min_age_seconds: int) -> dict:
    refs = referenced_objects(releases)
    now = time.time()
    scanned = 0
    removed = 0
    freed = 0
    for obj in iter_objects(objects):
        scanned += 1
        sha = obj.name
        if sha in refs:
            continue
        st = obj.stat()
        if st.st_nlink > 1:
            continue  # 仍被某个 release 目录硬链接（manifest 可能丢失/未写完）
        if now - st.st_mtime < min_age_seconds:
            continue
        removed += 1
        freed += st.st_size
        if not dry_run:
            obj.unlink()
    return {"scanned": scanned, "referenced": len(refs), "removed": removed, "freed_bytes": freed, "dry_run": dry_run}


def stats(objects: Path, releases: Path | None) -> dict:
    unique_bytes = 0
    object_count = 0
    for obj in iter_objects(objects):
        object_count += 1
        unique_bytes += obj.stat().st_size
    result = {"objects": object_count, "unique_bytes": unique_bytes}
    if releases:
        logical = 0
        release_count = 0
        for release in releases.iterdir():
            manifest = release / "manifest.json"
            if release.is_dir() and manifest.is_file():
                release_count += 1
                logical += sum(int(entry.get("bytes") or 0) for entry in load_manifest(manifest)["files"])
        result["releases"] = release_count
        result["logical_bytes"] = logical
        result["dedupe_saving"] = (1 - unique_bytes / logical) if logical else 0.0
    return result


def verify(objects: Path, releases: Path, *, full: bool, links: bool) -> dict:
    hashed: dict[str, str] = {}
    checked = 0
    for release in releases.iterdir():
        manifest_path = release / "manifest.json"
        if not release.is_dir() or not manifest_path.is_file():
            continue
        for entry in load_manifest(manifest_path)["files"]:
            rel, sha = entry["path"], entry["sha256"]
            obj = object_path(objects, sha)
            if not obj.is_file():
                raise StoreError(f"缺对象: {sha}（{release.name}/{rel}）")
            size = entry.get("bytes")
            if isinstance(size, int) and obj.stat().st_size != size:
                raise StoreError(f"对象大小不符: {sha}（{release.name}/{rel}）")
            if full and sha not in hashed:
                actual = sha256_file(obj)
                if actual != sha:
                    raise StoreError(f"对象内容与文件名不符: {obj}")
                hashed[sha] = actual
            if links:
                target = release / rel
                if not target.is_file():
                    raise StoreError(f"release 缺文件: {target}")
                a, b = target.stat(), obj.stat()
                if (a.st_ino, a.st_dev) != (b.st_ino, b.st_dev):
                    raise StoreError(f"release 文件不是对象硬链接: {target}")
            checked += 1
    return {"checked": checked, "hashed": len(hashed)}


def human_bytes(n: int) -> str:
    return f"{n / 1e6:.1f} MB"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="资产内容寻址对象库")
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("import", help="把 pack 内容登记进 objects")
    p.add_argument("--pack", help="完整 pack（内部有 manifest.json）")
    p.add_argument("--manifest", help="完整 manifest（配合 --files 做变更文件 staging）")
    p.add_argument("--files", default=None, help="--manifest 时的源文件根；缺省为 manifest 所在目录")
    p.add_argument("--objects", required=True)
    p.add_argument("--copy", action="store_true", help="跨盘 staging：复制而不是硬链接")
    p.add_argument("--verify", action="store_true", help="逐个文件重算 sha256（慢）")
    p.add_argument("--dry-run", action="store_true")
    p.set_defaults(func=command_import)

    p = sub.add_parser("materialize", help="按 manifest 从 objects 构建 release 树")
    p.add_argument("--manifest", required=True)
    p.add_argument("--objects", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--metadata", default=None, help="提供 version.json / delta.json 的目录")
    p.add_argument("--copy", action="store_true", help="复制而不是硬链接（不省空间，仅兼容）")
    p.add_argument("--force", action="store_true", help="输出目录非空时清空")
    p.add_argument("--dry-run", action="store_true")
    p.set_defaults(func=command_materialize)

    p = sub.add_parser("adopt", help="把现有 release 树原地改成对象硬链接")
    p.add_argument("--releases", required=True)
    p.add_argument("--objects", required=True)
    p.add_argument("--dry-run", action="store_true")
    p.set_defaults(func=command_adopt)

    p = sub.add_parser("gc", help="清理未被 release 引用的对象")
    p.add_argument("--objects", required=True)
    p.add_argument("--releases", required=True)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--min-age-seconds", type=int, default=3600)
    p.set_defaults(func=command_gc)

    p = sub.add_parser("stats", help="唯一内容 / 逻辑总量 / 去重率")
    p.add_argument("--objects", required=True)
    p.add_argument("--releases", default=None)
    p.set_defaults(func=command_stats)

    p = sub.add_parser("verify", help="校验 release 条目与对象")
    p.add_argument("--objects", required=True)
    p.add_argument("--releases", required=True)
    p.add_argument("--full", action="store_true", help="逐个唯一对象重算 sha256")
    p.add_argument("--links", action="store_true", help="要求 release 文件与对象 inode 相同")
    p.set_defaults(func=command_verify)

    return parser


def command_import(args):
    objects = Path(args.objects).resolve()
    if bool(args.pack) == bool(args.manifest):
        raise StoreError("import 需要 --pack 或 --manifest 之一")
    if args.pack:
        result = import_pack(Path(args.pack).resolve(), objects,
                             copy=args.copy, verify=args.verify, dry_run=args.dry_run)
    else:
        manifest = Path(args.manifest).resolve()
        files_root = Path(args.files).resolve() if args.files else manifest.parent
        result = import_manifest(manifest, files_root, objects,
                                 copy=args.copy, verify=args.verify, dry_run=args.dry_run)
    print(f"import: files={result['files']} logical={human_bytes(result['logical_bytes'])} "
          f"unique={result['unique_objects']} objects ({human_bytes(result['unique_bytes'])}) "
          f"added={result['added']} reused={result['reused']} dry_run={result['dry_run']}")
    return 0


def command_materialize(args):
    result = materialize(Path(args.manifest).resolve(), Path(args.objects).resolve(),
                         Path(args.out).resolve(), metadata=Path(args.metadata).resolve() if args.metadata else None,
                         copy=args.copy, force=args.force, dry_run=args.dry_run)
    print(f"materialize: files={result['files']} linked={result['linked']} copied={result['copied']} "
          f"dry_run={result['dry_run']}")
    return 0


def command_adopt(args):
    releases = Path(args.releases).resolve()
    objects = Path(args.objects).resolve()
    total = 0
    for release in sorted(releases.iterdir()):
        if not (release / "manifest.json").is_file():
            continue
        result = adopt_release(release, objects, dry_run=args.dry_run)
        total += result["adopted"]
        print(f"adopt {release.name}: adopted={result['adopted']} already_linked={result['already_linked']}")
    print(f"adopt total: {total} files dry_run={args.dry_run}")
    return 0


def command_gc(args):
    result = gc(Path(args.objects).resolve(), Path(args.releases).resolve(),
                dry_run=args.dry_run, min_age_seconds=args.min_age_seconds)
    print(f"gc: scanned={result['scanned']} referenced={result['referenced']} removed={result['removed']} "
          f"freed={human_bytes(result['freed_bytes'])} dry_run={result['dry_run']}")
    return 0


def command_stats(args):
    result = stats(Path(args.objects).resolve(), Path(args.releases).resolve() if args.releases else None)
    print(f"stats: objects={result['objects']} unique={human_bytes(result['unique_bytes'])}")
    if "logical_bytes" in result:
        print(f"       releases={result['releases']} logical={human_bytes(result['logical_bytes'])} "
              f"dedupe={result['dedupe_saving'] * 100:.1f}%")
    return 0


def command_verify(args):
    result = verify(Path(args.objects).resolve(), Path(args.releases).resolve(),
                    full=args.full, links=args.links)
    print(f"verify: PASS（{result['checked']} 条，hashed={result['hashed']}）")
    return 0


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return args.func(args)
    except StoreError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())