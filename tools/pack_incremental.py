#!/usr/bin/env python3
"""增量 pack 原语：硬链接复用、断链覆盖、目标集合孤儿删除。

设计约束：
- 只有逐文件 sha256 相同才复用；不按时间戳/统计阈值判断。
- 任何写入都先写同目录临时文件再 os.replace，绝不截断硬链接共享 inode。
- clone 只跳过根目录发布元数据（manifest/version/delta）与 overhead/ 渲染中间件。
"""
from __future__ import annotations

import hashlib
import os
import pathlib
import shutil

ROOT = pathlib.Path(__file__).resolve().parents[1]
ROOT_METADATA = {"manifest.json", "version.json", "delta.json"}


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def same_bytes(a: pathlib.Path, b: pathlib.Path) -> bool:
    return a.is_file() and b.is_file() and a.stat().st_size == b.stat().st_size \
        and sha256_file(a) == sha256_file(b)


def clone_base(base: pathlib.Path, out: pathlib.Path,
               copy_prefixes: tuple[str, ...] = ("research",)) -> tuple[int, int]:
    """硬链接克隆 base pack；copy_prefixes 下的文件复制（下游会原地写）。"""
    if out.exists() and any(out.iterdir()):
        raise SystemExit(f"输出 pack 非空: {out}")
    linked = copied = 0
    for src in sorted(base.rglob("*")):
        if not src.is_file():
            continue
        rel = src.relative_to(base)
        if len(rel.parts) == 1 and rel.name in ROOT_METADATA:
            continue
        if rel.parts and rel.parts[0] == "overhead":
            continue
        dst = out / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        if rel.parts and rel.parts[0] in copy_prefixes:
            shutil.copy2(src, dst)
            copied += 1
            continue
        try:
            os.link(src, dst)
            linked += 1
        except OSError:
            shutil.copy2(src, dst)
            copied += 1
    return linked, copied


def _write_bytes_atomic(dst: pathlib.Path, data: bytes) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    tmp = dst.with_name(dst.name + f".tmp-{os.getpid()}")
    tmp.write_bytes(data)
    os.replace(tmp, dst)


def overlay_bytes(out: pathlib.Path, rel: str, data: bytes) -> str:
    dst = out / rel
    if dst.is_file() and dst.stat().st_size == len(data) and dst.read_bytes() == data:
        return "reused"
    _write_bytes_atomic(dst, data)
    return "updated"


def overlay_file(out: pathlib.Path, rel: str, src: pathlib.Path) -> str:
    dst = out / rel
    if same_bytes(dst, src):
        return "reused"
    dst.parent.mkdir(parents=True, exist_ok=True)
    tmp = dst.with_name(dst.name + f".tmp-{os.getpid()}")
    shutil.copy2(src, tmp)
    os.replace(tmp, dst)
    return "updated"


def force_copy(out: pathlib.Path, rel: str, src: pathlib.Path) -> None:
    """无条件断链复制（供 composite 这类原地写工具使用）。"""
    dst = out / rel
    dst.parent.mkdir(parents=True, exist_ok=True)
    tmp = dst.with_name(dst.name + f".tmp-{os.getpid()}")
    shutil.copy2(src, tmp)
    os.replace(tmp, dst)


def delete_path(out: pathlib.Path, rel: str) -> bool:
    p = out / rel
    if p.is_symlink() or p.is_file():
        p.unlink()
        return True
    if p.is_dir():
        shutil.rmtree(p)
        return True
    return False


def sync_tree(out: pathlib.Path, rel: str, src_dir: pathlib.Path,
              report: dict[str, int] | None = None) -> None:
    """把一个逐项目录（glb/{id}/ 等）同步到 out/rel：新增/变化覆盖，源端不存在的旧文件删除。"""
    dst_dir = out / rel
    keep: set[str] = set()
    if src_dir.is_dir():
        for src in sorted(src_dir.rglob("*")):
            if not src.is_file():
                continue
            child = src.relative_to(src_dir).as_posix()
            keep.add(child)
            action = overlay_file(out, f"{rel}/{child}", src)
            if report is not None:
                report[action] = report.get(action, 0) + 1
    if dst_dir.is_dir():
        for dst in sorted(dst_dir.rglob("*"), reverse=True):
            if dst.is_file() and dst.relative_to(dst_dir).as_posix() not in keep:
                dst.unlink()
                if report is not None:
                    report["deleted"] = report.get("deleted", 0) + 1
        for dst in sorted(dst_dir.rglob("*"), reverse=True):
            if dst.is_dir() and not any(dst.iterdir()):
                dst.rmdir()


def copy_writable_tree(src: pathlib.Path, dst: pathlib.Path) -> None:
    """复制小目录（会被下游工具原地写），避免硬链接写穿。"""
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(src, dst)


def hardlink_tree(src: pathlib.Path, dst: pathlib.Path) -> None:
    """只读大目录用硬链接；写者必须先 os.replace 目标文件。"""
    if dst.exists():
        shutil.rmtree(dst)
    for s in sorted(src.rglob("*")):
        if not s.is_file():
            continue
        d = dst / s.relative_to(src)
        d.parent.mkdir(parents=True, exist_ok=True)
        try:
            os.link(s, d)
        except OSError:
            shutil.copy2(s, d)


def reverse_pack_to_data(pack: pathlib.Path, work: pathlib.Path) -> dict[str, int]:
    """从 base pack 反向物化 core 所需的 work/data（只读件硬链接，写入件复制）。"""
    data = work / "data"
    data.mkdir(parents=True, exist_ok=True)
    counts = {"copied": 0, "linked": 0}
    # 核心整表/小型 JSON：下游会写，复制。
    for name in ("tanks.pb", "models.pb", "tank_cache.json", "data_version.json"):
        src = pack / "data" / name
        if src.is_file():
            shutil.copy2(src, data / name)
            counts["copied"] += 1
    for src, dst in ((pack / "data/shellKinds.json", data / "shell_kinds.json"),
                     (pack / "data/gunMaskSlices.json", data / "gun_mask_slices.json")):
        if src.is_file():
            shutil.copy2(src, dst)
            counts["copied"] += 1
    # tank_cache 重建的旁路输入（tank_resolver 的 *_top 字段源，t11r3 起）：
    # 缺失时 Rust 侧静默空表 → 重建的 tank_cache 丢 *_top 字段（§5.1 混合版本陷阱）。
    # base 无 XI 车时本就不存在，复制是条件性的。
    for name in ("upgrade_top_hp.json", "upgrade_top_shells.json"):
        src = pack / "data" / name
        if src.is_file():
            shutil.copy2(src, data / name)
            counts["copied"] += 1
    # game_data 会被 update-data --force 写；复制（743 个小 JSON）。
    if (pack / "game_data").is_dir():
        copy_writable_tree(pack / "game_data", data / "game_data")
        counts["copied"] += sum(1 for p in (data / "game_data").rglob("*") if p.is_file())
    # 模型 GLB 只读：硬链接，变化项由 handler 用 os.replace 覆盖。
    if (pack / "glb").is_dir():
        hardlink_tree(pack / "glb", data / "cache/models")
        counts["linked"] += sum(1 for p in (data / "cache/models").rglob("*") if p.is_file())
    return counts
