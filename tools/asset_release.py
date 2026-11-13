"""资产包的**按游戏版本**发布元数据（dataset 版本管理）。

数据面按游戏版本维护，对外暴露 `datasets/<dataset_id>/`；同一 dataset 的重新导出
生成新的内部 release，dataset_id 不变。本工具不搬运字节，只负责：

  stamp     为一份已打包的 asset pack 写不可变的 `version.json`
  catalog   扫描 datasets 根目录，汇总 `versions.json`（前端版本选择器的数据源）
  verify    交叉校验 dataset 的 version.json / manifest.json / 逐文件 sha256
  delta     同一 dataset 前后两个 release 的差异清单（`delta.json`）

设计约束（见 docs/game-data-sources.md §5.2）：
  * `content_digest` 只由**内容**决定：manifest 的 path→sha256 集合 + 语义化的
    data_version；排除 generated / upstream_commit / worktree_dirty 与重新提取的
    时间戳，因此同内容重建得到同一个 digest。
  * `manifest.json`、`version.json`、`delta.json`、`overhead/` 是发布元数据/中间件，
    不进入 content_digest；除此之外 manifest 未登记的文件视为发布内容不完整，fail-closed。
  * dataset_id 只允许小写字母、数字、`.`、`-`、`_`（均为 URL 安全字符；`_` 用于
    客户端 SmartDLC 槽位命名，如 `11.21.0_1`），禁止 `+` / `/`。

用法：
  python tools/asset_release.py stamp --pack release/asset_pack \
      --dataset-id 11.21.0 --game-version release/11.21.0
  python tools/asset_release.py catalog --root /srv/wotb-assets/datasets \
      --out /srv/wotb-assets/versions.json --default 11.21.0
  python tools/asset_release.py verify --root /srv/wotb-assets/datasets --full
  python tools/asset_release.py delta --old <prev-release> --new <release> \
      --out <release>/delta.json
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

MANIFEST_SCHEMA = 1
CATALOG_SCHEMA = 1
CONTRACT = "asset-pack-v1"

# 发布元数据 / 中间件：不属于客户端内容，允许不在 manifest 中。
NON_CONTENT_FILES = {"manifest.json", "version.json", "delta.json"}
NON_CONTENT_PREFIXES = ("overhead/",)

# `_` 用于客户端 SmartDLC 槽位命名（如 11.21.0_1，对应 s11.21.0_1.yaml）；
# 允许的字符均为 URL 安全字符，可直接进 /v/<id>/ 路径。
DATASET_ID_RE = re.compile(r"^[a-z0-9](?:[a-z0-9._-]{0,62}[a-z0-9])?$")
STATUSES = {"active", "deprecated", "revoked"}


class ReleaseError(RuntimeError):
    """发布元数据不满足契约；调用方应中止发布。"""


def now_utc() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def write_json(path: Path, value: object) -> None:
    # 原子替换（temp + os.replace）：release 树常用 cp -al 硬链接克隆，原地 write_text
    # 会截断共享 inode，把上一 release 的同名元文件（version.json/delta.json）一并改坏
    # （2026-11-07 r8 发布时 r7 元数据污染事故）。os.replace 换 inode，旧链接不受影响。
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(json.dumps(value, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    os.replace(tmp, path)


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise ReleaseError(f"缺少文件: {path}") from None
    except json.JSONDecodeError as exc:
        raise ReleaseError(f"JSON 解析失败: {path}: {exc}") from None
    if not isinstance(value, dict):
        raise ReleaseError(f"JSON 顶层必须是对象: {path}")
    return value


def validate_dataset_id(dataset_id: str) -> None:
    if not DATASET_ID_RE.match(dataset_id):
        raise ReleaseError(
            f"dataset_id 非法: {dataset_id!r}（只允许小写字母/数字/./-/_，1-64 字符，"
            "必须首尾为字母或数字）"
        )


def load_manifest(pack: Path) -> dict:
    manifest_path = pack / "manifest.json"
    manifest = load_json(manifest_path)
    if manifest.get("version") != MANIFEST_SCHEMA:
        raise ReleaseError(
            f"manifest schema 不支持: {manifest.get('version')!r}（期望 {MANIFEST_SCHEMA}）"
        )
    files = manifest.get("files")
    if not isinstance(files, list):
        raise ReleaseError("manifest.files 必须是数组")
    seen: set[str] = set()
    for entry in files:
        if not isinstance(entry, dict):
            raise ReleaseError("manifest.files 条目必须是对象")
        rel = entry.get("path")
        digest = entry.get("sha256")
        if not isinstance(rel, str) or not rel:
            raise ReleaseError("manifest.files 条目缺 path")
        if rel in seen:
            raise ReleaseError(f"manifest.files 重复 path: {rel}")
        seen.add(rel)
        pure = PurePosixPath(rel)
        if pure.is_absolute() or ".." in pure.parts or "\\" in rel:
            raise ReleaseError(f"manifest.files 非法 path: {rel!r}")
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise ReleaseError(f"manifest.files 条目 sha256 非法: {rel}")
    return manifest


def manifest_path_set(manifest: dict) -> set[str]:
    return {entry["path"] for entry in manifest["files"]}


def is_non_content(rel: str) -> bool:
    return rel in NON_CONTENT_FILES or any(rel.startswith(prefix) for prefix in NON_CONTENT_PREFIXES)


def validate_pack_content(pack: Path, manifest: dict, *, full: bool) -> None:
    """manifest 登记的每个文件都存在且大小/哈希一致；未登记的内容文件 fail-closed。"""
    listed = manifest_path_set(manifest)
    for entry in manifest["files"]:
        rel = entry["path"]
        path = pack / rel
        if not path.is_file():
            raise ReleaseError(f"manifest 登记的文件不存在: {rel}")
        size = path.stat().st_size
        if isinstance(entry.get("bytes"), int) and entry["bytes"] != size:
            raise ReleaseError(f"文件大小不符: {rel}（manifest {entry['bytes']}，实际 {size}）")
        if full:
            actual = sha256_file(path)
            if actual != entry["sha256"]:
                raise ReleaseError(f"文件 sha256 不符: {rel}")

    extras: list[str] = []
    for path in pack.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(pack).as_posix()
        if rel in listed or is_non_content(rel):
            continue
        extras.append(rel)
    if extras:
        sample = ", ".join(sorted(extras)[:8])
        raise ReleaseError(
            f"存在 {len(extras)} 个 manifest 未登记的内容文件（发布包不完整）: {sample}"
        )


def semantic_data_version(manifest: dict) -> dict:
    """参与 content_digest 的语义字段；排除每次提取都会变化的时间戳。"""
    raw = manifest.get("data_version")
    if not isinstance(raw, dict):
        return {}
    out = {key: raw[key] for key in ("game_version", "tank_count", "game_data_files") if key in raw}
    if isinstance(raw.get("preview"), dict):
        out["preview"] = raw["preview"]
    return out


def content_digest(pack: Path, manifest: dict) -> str:
    entries = sorted(
        ({"path": entry["path"], "sha256": entry["sha256"]} for entry in manifest["files"]),
        key=lambda item: item["path"],
    )
    payload = {
        "contract": {"layout": CONTRACT, "manifest_schema": manifest["version"]},
        "data_version": semantic_data_version(manifest),
        "files": entries,
    }
    return "sha256:" + sha256_bytes(canonical_json(payload).encode("utf-8"))


def manifest_sha256(pack: Path) -> str:
    return sha256_file(pack / "manifest.json")


def default_release_id(generated: str, digest: str) -> str:
    try:
        day = datetime.fromisoformat(generated).astimezone(timezone.utc).strftime("%Y%m%d")
    except (TypeError, ValueError):
        day = datetime.now(timezone.utc).strftime("%Y%m%d")
    return f"{day}-{digest.removeprefix('sha256:')[:8]}"


def build_version(
    pack: Path,
    *,
    dataset_id: str,
    game_version: str | None,
    variant: str | None,
    label: str | None,
    release: str | None,
    min_client: str | None,
    notes: str | None,
    status: str,
) -> dict:
    validate_dataset_id(dataset_id)
    if status not in STATUSES:
        raise ReleaseError(f"status 非法: {status!r}（可选 {sorted(STATUSES)}）")
    manifest = load_manifest(pack)
    validate_pack_content(pack, manifest, full=True)

    dv = manifest.get("data_version") if isinstance(manifest.get("data_version"), dict) else {}
    manifest_game_version = dv.get("game_version")
    if not isinstance(manifest_game_version, str) or not manifest_game_version:
        raise ReleaseError("manifest.data_version.game_version 缺失")
    base = game_version or manifest_game_version
    expected_suffix = f"+{variant}" if variant else ""
    if manifest_game_version != base and manifest_game_version != base + expected_suffix:
        raise ReleaseError(
            "game_version 与 manifest 不一致: "
            f"manifest={manifest_game_version!r}，本次 base={base!r}，variant={variant!r}"
        )

    digest = content_digest(pack, manifest)
    generated = manifest.get("generated")
    if not isinstance(generated, str) or not generated:
        generated = now_utc()
    release_id = release or default_release_id(generated, digest)
    if not re.fullmatch(r"[0-9A-Za-z][0-9A-Za-z._-]{0,79}", release_id):
        raise ReleaseError(f"release 非法: {release_id!r}")

    preview = dv.get("preview") if isinstance(dv.get("preview"), dict) else None
    return {
        "schema": 1,
        "dataset_id": dataset_id,
        "game_version": base,
        "variant": variant,
        "label": label or dataset_id,
        "release": release_id,
        "generated": generated,
        "stamped_at": now_utc(),
        "content_digest": digest,
        "manifest_sha256": manifest_sha256(pack),
        "tank_count": dv.get("tank_count"),
        "preview": preview,
        "min_client": min_client,
        "status": status,
        "notes": notes or "",
    }


def load_version(pack: Path) -> dict:
    version = load_json(pack / "version.json")
    if version.get("schema") != 1:
        raise ReleaseError(f"{pack}/version.json schema 不支持: {version.get('schema')!r}")
    dataset_id = version.get("dataset_id")
    if not isinstance(dataset_id, str):
        raise ReleaseError(f"{pack}/version.json 缺 dataset_id")
    validate_dataset_id(dataset_id)
    for key in ("game_version", "release", "generated", "content_digest", "manifest_sha256"):
        if not isinstance(version.get(key), str) or not version[key]:
            raise ReleaseError(f"{pack}/version.json 缺 {key}")
    if version.get("status") not in STATUSES:
        raise ReleaseError(f"{pack}/version.json status 非法: {version.get('status')!r}")
    return version


def verify_dataset(pack: Path, *, full: bool) -> dict:
    version = load_version(pack)
    manifest = load_manifest(pack)
    validate_pack_content(pack, manifest, full=full)
    actual_manifest = manifest_sha256(pack)
    if actual_manifest != version["manifest_sha256"]:
        raise ReleaseError(
            f"{pack.name}: manifest_sha256 不符（version {version['manifest_sha256']}，实际 {actual_manifest}）"
        )
    actual_digest = content_digest(pack, manifest)
    if actual_digest != version["content_digest"]:
        raise ReleaseError(
            f"{pack.name}: content_digest 不符（version {version['content_digest']}，实际 {actual_digest}）"
        )
    return version


def version_sort_key(game_version: str) -> tuple:
    """把 `release/11.21.0` / `11.21.0` 变成可比较的数字元组；非数字段按字符串。"""
    value = game_version.rsplit("/", 1)[-1]
    parts = []
    for token in value.split("."):
        parts.append((0, int(token)) if token.isdigit() else (1, token))
    return tuple(parts)


def collect_datasets(root: Path, *, full: bool) -> list[dict]:
    if not root.is_dir():
        raise ReleaseError(f"datasets 根目录不存在: {root}")
    datasets: list[dict] = []
    aliases = {}
    for child in sorted(root.iterdir(), key=lambda p: p.name):
        if child.name.startswith("."):
            continue
        if child.is_symlink():
            # Only the literal sibling form denotes a retired public route. A normal
            # ../releases/<release> link must still match its stamped dataset ID.
            target = os.readlink(child)
            if target == Path(target).name:
                validate_dataset_id(child.name)
                validate_dataset_id(target)
                aliases[child.name] = target
                continue
            if not (child / "version.json").is_file():
                raise ReleaseError(f"dataset 链接缺少 version.json: {child} -> {target}")
        if not child.is_dir():
            continue
        if not (child / "version.json").is_file():
            continue
        version = verify_dataset(child, full=full)
        if version["dataset_id"] != child.name:
            raise ReleaseError(
                f"dataset_id 与目录名不一致: {version['dataset_id']!r} vs {child.name!r}"
            )
        datasets.append(
            {
                "id": version["dataset_id"],
                "label": version.get("label") or version["dataset_id"],
                "game_version": version["game_version"],
                "variant": version.get("variant"),
                "release": version["release"],
                "tank_count": version.get("tank_count"),
                "generated": version["generated"],
                "content_digest": version["content_digest"],
                "manifest_sha256": version["manifest_sha256"],
                "min_client": version.get("min_client"),
                "status": version["status"],
                "notes": version.get("notes") or "",
            }
        )
    canonical_ids = {item["id"] for item in datasets}
    for alias, target in aliases.items():
        if target not in canonical_ids:
            raise ReleaseError(f"dataset 别名 {alias!r} 必须直接指向已验证的 dataset: {target!r}")
    if not datasets:
        raise ReleaseError(f"{root} 下没有任何含 version.json 的 dataset")

    # 游戏版本降序；同一版本 base（variant 为空）排在变体前。
    datasets.sort(key=lambda item: item.get("variant") or "")
    datasets.sort(key=lambda item: version_sort_key(item["game_version"]), reverse=True)
    return datasets


def command_stamp(args: argparse.Namespace) -> int:
    pack = Path(args.pack).resolve()
    version = build_version(
        pack,
        dataset_id=args.dataset_id,
        game_version=args.game_version,
        variant=args.variant,
        label=args.label,
        release=args.release,
        min_client=args.min_client,
        notes=args.notes,
        status=args.status,
    )
    if args.dry_run:
        print(json.dumps(version, ensure_ascii=False, indent=1))
        return 0
    write_json(pack / "version.json", version)
    print(
        f"version.json 已写入: {pack / 'version.json'}\n"
        f"  dataset={version['dataset_id']} release={version['release']} "
        f"digest={version['content_digest']} files={len(load_manifest(pack)['files'])}"
    )
    return 0


def command_catalog(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    datasets = collect_datasets(root, full=args.full)
    ids = [item["id"] for item in datasets]
    if len(ids) != len(set(ids)):
        raise ReleaseError(f"dataset id 冲突: {ids}")

    existing_default = None
    out = Path(args.out).resolve() if args.out else root.parent / "versions.json"
    if out.is_file():
        try:
            existing_default = load_json(out).get("default")
        except ReleaseError:
            existing_default = None
    default = args.default or existing_default
    if default is None or default not in ids:
        default = ids[0]
    if default not in ids:
        raise ReleaseError(f"default 不在 dataset 列表: {default}")

    catalog = {
        "schema": CATALOG_SCHEMA,
        "updated": now_utc(),
        "default": default,
        "datasets": datasets,
    }
    if args.dry_run:
        print(json.dumps(catalog, ensure_ascii=False, indent=1))
        return 0
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_name(out.name + ".tmp")
    write_json(tmp, catalog)
    os.replace(tmp, out)
    print(f"versions.json 已写入: {out}（{len(datasets)} 个 dataset，default={default}）")
    return 0


def command_verify(args: argparse.Namespace) -> int:
    if getattr(args, "pack", None):
        # 单 release 内容校验（dataset.sh link 切换在线 symlink 之前的候选体检）。
        v = verify_dataset(Path(args.pack).resolve(), full=args.full)
        print(f"verify: PASS（{v['dataset_id']} / {v['release']}，tanks={v['tank_count']}，full={args.full}）")
        return 0
    if not args.root:
        raise ReleaseError("verify 需要 --pack <release 目录> 或 --root <datasets 目录>")
    root = Path(args.root).resolve()
    datasets = collect_datasets(root, full=args.full)
    if args.catalog:
        catalog = load_json(Path(args.catalog).resolve())
        if catalog.get("schema") != CATALOG_SCHEMA:
            raise ReleaseError(f"catalog schema 不支持: {catalog.get('schema')!r}")
        by_id = {item["id"]: item for item in catalog.get("datasets", [])}
        if set(by_id) != {item["id"] for item in datasets}:
            raise ReleaseError(
                f"catalog 与磁盘 dataset 不一致: catalog={sorted(by_id)} disk={sorted(item['id'] for item in datasets)}"
            )
        for item in datasets:
            entry = by_id[item["id"]]
            for key in ("content_digest", "manifest_sha256", "release", "game_version"):
                if entry.get(key) != item[key]:
                    raise ReleaseError(f"{item['id']}: catalog.{key} 与 version.json 不一致")
        if catalog.get("default") not in by_id:
            raise ReleaseError(f"catalog.default 不在列表: {catalog.get('default')!r}")
    print(f"verify: PASS（{len(datasets)} 个 dataset，full={args.full}）")
    return 0


def command_delta(args: argparse.Namespace) -> int:
    old = Path(args.old).resolve()
    new = Path(args.new).resolve()
    old_manifest = load_manifest(old)
    new_manifest = load_manifest(new)
    old_files = {entry["path"]: entry["sha256"] for entry in old_manifest["files"]}
    new_files = {entry["path"]: entry["sha256"] for entry in new_manifest["files"]}

    changes = []
    for rel in sorted(set(old_files) | set(new_files)):
        if rel not in old_files:
            changes.append({"path": rel, "change": "added"})
        elif rel not in new_files:
            changes.append({"path": rel, "change": "removed"})
        elif old_files[rel] != new_files[rel]:
            changes.append({"path": rel, "change": "modified"})

    new_version = load_version(new)
    old_version = None
    if (old / "version.json").is_file():
        old_version = load_version(old)
        if old_version["dataset_id"] != new_version["dataset_id"]:
            # dataset 改名/换通道是合法动作，但必须显式声明并留痕（默认仍 fail-closed）。
            if not getattr(args, "allow_dataset_change", False):
                raise ReleaseError(
                    f"delta 两侧 dataset 不同: {old_version['dataset_id']} vs {new_version['dataset_id']}"
                    "（确属通道改名/迁移时加 --allow-dataset-change）"
                )

    delta = {
        "schema": 1,
        "dataset_id": new_version["dataset_id"],
        "release": new_version["release"],
        "base_release": old_version["release"] if old_version else args.base_release,
        "generated": new_manifest.get("generated"),
        "changes": changes,
    }
    if old_version is not None and old_version["dataset_id"] != new_version["dataset_id"]:
        delta["dataset_changed_from"] = old_version["dataset_id"]
    out = Path(args.out).resolve() if args.out else new / "delta.json"
    write_json(out, delta)
    print(f"delta.json 已写入: {out}（{len(changes)} 处变更）")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="资产包按游戏版本的 dataset 发布元数据")
    sub = parser.add_subparsers(dest="command", required=True)

    stamp = sub.add_parser("stamp", help="为已打包的 asset pack 写 version.json")
    stamp.add_argument("--pack", required=True)
    stamp.add_argument("--dataset-id", required=True)
    stamp.add_argument("--game-version", default=None)
    stamp.add_argument("--variant", default=None)
    stamp.add_argument("--label", default=None)
    stamp.add_argument("--release", default=None)
    stamp.add_argument("--min-client", default=None)
    stamp.add_argument("--notes", default=None)
    stamp.add_argument("--status", default="active", choices=sorted(STATUSES))
    stamp.add_argument("--dry-run", action="store_true")
    stamp.set_defaults(func=command_stamp)

    catalog = sub.add_parser("catalog", help="扫描 datasets 根目录生成 versions.json")
    catalog.add_argument("--root", required=True)
    catalog.add_argument("--out", default=None)
    catalog.add_argument("--default", default=None)
    catalog.add_argument("--full", action="store_true", help="逐文件校验 sha256（慢）")
    catalog.add_argument("--dry-run", action="store_true")
    catalog.set_defaults(func=command_catalog)

    verify = sub.add_parser("verify", help="校验 dataset 元数据与内容")
    verify.add_argument("--root", default=None)
    verify.add_argument("--pack", default=None,
                        help="校验单个 release 目录（version.json + manifest 全量体检）")
    verify.add_argument("--catalog", default=None)
    verify.add_argument("--full", action="store_true", help="逐文件校验 sha256（慢）")
    verify.set_defaults(func=command_verify)

    delta = sub.add_parser("delta", help="同一 dataset 两个 release 的差异")
    delta.add_argument("--old", required=True)
    delta.add_argument("--new", required=True)
    delta.add_argument("--out", default=None)
    delta.add_argument("--base-release", default=None)
    delta.add_argument("--allow-dataset-change", action="store_true",
                       help="允许两侧 dataset_id 不同（通道改名/迁移），delta.json 记 dataset_changed_from")
    delta.set_defaults(func=command_delta)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except ReleaseError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
