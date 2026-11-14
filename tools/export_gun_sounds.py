#!/usr/bin/env python3
"""导出客户端内置炮声资产（3D 回放炮声的资产面）。

数据来源（Steam 客户端 Data root，与 extract_vehicles.GAME_DIR_CANDIDATES 同源）：
  * `WwiseSound/SoundbanksInfo.json.dvpl`   Wwise 事件 → 媒体清单（事件名 = play_wpn_*_3p，
    媒体 = ReferencedStreamedFiles；MaxAttenuation=500m）
  * `WwiseSound/weapon.pck.dvpl` / `weapon_basic.pck.dvpl`   AKPK 容器，流式 .wem 本体

格式链：AKPK(.pck) → .wem（RIFF/WAVE + Wwise Vorbis, tag 0xFFFF）→ vgmstream-cli 解码
→ WAV → libsndfile(soundfile) 编码 OGG Vorbis → 资产包 `sound/wpn/`。

命名契约（前端 gunfireAudio.ts 按 index.json 消费，不猜文件名）：
  {class}_{layer}_{nn}.ogg    class = small|mid|main|large|huge|huge_extra|auto
                              layer = close|mid|far（客户端 Close/Mid/Far 距离分层）
  index.json                  { "<class>": { "<layer>": ["<file>", ...] } }
  （gun_class.json 由 tools/export_gun_class.py 负责，本工具不触碰）

已知边界（如实记录，不做猜测）：
  * Close/Mid/Far 的客户端内部切换阈值（Wwise HIRC RTPC）未取证——MVP 网页侧固定用
    close 层 + WebAudio 距离增益；index.json 仍带全部三层，后续取证可直接启用。
  * 1p（车内视角）变体不导出：回放 3D 全程外视角，3p 事件已覆盖。
  * 只导出 3p 事件引用的流式媒体；两 pck 的其余内容（UI/语音等）不在事件清单内，跳过。
"""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile

import soundfile as sf

TOOLS_DIR = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))
sys.path.insert(0, str(TOOLS_DIR / "wotbtools"))

from wotb_sc2 import decode_dvpl  # noqa: E402
from extract_vehicles import GAME_DIR_CANDIDATES  # noqa: E402

REPO_ROOT = TOOLS_DIR.parent
EVENT_RE_PREFIX = "play_wpn_"
# 事件闭集（= export_gun_class.py WPN_TAG 的 class 词表 + 客户端事件命名）
EVENT_CLASSES = ("small", "mid", "main", "large", "huge", "huge_extra", "auto")
LAYERS = ("close", "mid", "far")
VGMSTREAM_CANDIDATES = (
    pathlib.Path("D:/Class/Rust/tmp_bins/vgmstream/vgmstream-cli.exe"),
)


def game_data_root(value: str | None) -> pathlib.Path:
    if value:
        return pathlib.Path(value)
    for c in GAME_DIR_CANDIDATES:
        p = pathlib.Path(c)
        if p.is_dir():
            return p
    raise SystemExit(f"找不到客户端 Data root（候选: {GAME_DIR_CANDIDATES}），用 --game-data 指定")


def find_vgmstream(value: str | None) -> pathlib.Path:
    if value:
        p = pathlib.Path(value)
        if p.exists():
            return p
        raise SystemExit(f"--vgmstream 不存在: {p}")
    env = os.environ.get("VGMSTREAM_CLI")
    if env and pathlib.Path(env).exists():
        return pathlib.Path(env)
    for c in VGMSTREAM_CANDIDATES:
        if c.exists():
            return c
    raise SystemExit(
        "找不到 vgmstream-cli（.wem→WAV 解码器）。安装其一并传入：\n"
        "  * 下载 https://github.com/vgmstream/vgmstream/releases 的 vgmstream-win64.zip\n"
        "  * --vgmstream <path> 或环境变量 VGMSTREAM_CLI 指向 vgmstream-cli(.exe)"
    )


def parse_akpk_streams(pck: bytes) -> dict[int, tuple[int, int]]:
    """AKPK 流式媒体索引（eXpl0it3r/bnkextr pck.bt 契约）→ {id: (文件偏移, 长度)}。

    AKPK | headerLength | version | languagesLength | banksLength | streamsLength
    [externalsLength 当 headerLength != 16+前三者和]
    LANGUAGES{ count, {nameOffset,id}[], UTF16 names } → FILE_TABLE banks → FILE_TABLE streams
    FILE_ENTRY = {id, blockSize, length, startBlock, languageId}；绝对偏移 = startBlock*blockSize。
    """
    if pck[:4] != b"AKPK":
        raise ValueError("不是 AKPK 容器")
    header_len, _version, langs_len, banks_len, streams_len = struct.unpack_from("<5I", pck, 4)
    pos = 24
    if header_len != langs_len + banks_len + streams_len + 16:
        pos += 4  # externalsLength
    # LANGUAGES
    n_langs, = struct.unpack_from("<I", pck, pos); pos += 4
    pos += n_langs * 8 + (langs_len - 4 - n_langs * 8)
    # banks 表（跳过）
    n_banks, = struct.unpack_from("<I", pck, pos); pos += 4 + n_banks * 20
    # streams 表
    n_streams, = struct.unpack_from("<I", pck, pos); pos += 4
    out: dict[int, tuple[int, int]] = {}
    for _ in range(n_streams):
        fid, blk, ln, sb, _lang = struct.unpack_from("<5I", pck, pos); pos += 20
        out[fid] = (sb * blk, ln)
    return out


def collect_event_media(game_data: pathlib.Path) -> dict[str, list[tuple[int, str]]]:
    """SoundbanksInfo → {class: [(media_id, wem_path)]}（仅 play_wpn_<class>_3p 事件）。"""
    sbi_path = game_data / "WwiseSound" / "SoundbanksInfo.json.dvpl"
    d = json.loads(decode_dvpl(sbi_path.read_bytes()).decode("utf-8"))["SoundBanksInfo"]
    events: dict[str, dict[int, str]] = {}
    for b in d.get("SoundBanks", []):
        for ev in (b.get("IncludedEvents") or []):
            name = ev.get("Name", "")
            if not name.startswith(EVENT_RE_PREFIX) or not name.endswith("_3p"):
                continue
            cls = name[len(EVENT_RE_PREFIX):-len("_3p")]
            if cls not in EVENT_CLASSES:
                print(f"  跳过未列入闭集的事件: {name}")
                continue
            slot = events.setdefault(cls, {})
            for m in (ev.get("ReferencedStreamedFiles") or []):
                slot[int(m["Id"])] = m.get("Path", "")
    return {c: sorted(m.items()) for c, m in events.items()}


def wem_meta(path: str) -> tuple[str, str]:
    """`SFX\\Weapons_new\\Wpn_arcade\\05_Large\\Mid\\blitz_..._03_xxx.wem` → (layer, nn)。"""
    parts = path.replace("\\\\", "\\").split("\\")
    # SFX\Weapons_new\Wpn_arcade\05_Large\Mid\<file> → 层=Mid（下标 4）
    layer = parts[4].lower() if len(parts) > 5 else ""
    if layer not in LAYERS:
        raise ValueError(f"未知距离层: {path}")
    stem = parts[-1].rsplit(".", 1)[0]
    nn = stem.split("_")[-2]
    if not nn.isdigit():
        raise ValueError(f"变体序号解析失败: {path}")
    return layer, nn


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game-data", help="客户端 Data 根（默认按 GAME_DIR_CANDIDATES）")
    ap.add_argument("--out", default=str(REPO_ROOT / "release/asset_pack/sound/wpn"))
    ap.add_argument("--vgmstream", help="vgmstream-cli 路径（默认候选+VGMSTREAM_CLI）")
    args = ap.parse_args()

    gd = game_data_root(args.game_data)
    vgm = find_vgmstream(args.vgmstream)
    out_dir = pathlib.Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)

    # 1) 事件 → 媒体
    events = collect_event_media(gd)
    n_media = sum(len(v) for v in events.values())
    print(f"事件闭集 {len(events)} 类，3p 流式媒体 {n_media} 个")

    # 2) AKPK 媒体池（weapon.pck + weapon_basic.pck；同名 id 先到先得——3p 全命中即可）
    pool: dict[int, bytes] = {}
    for name in ("weapon.pck", "weapon_basic.pck"):
        p = gd / "WwiseSound" / f"{name}.dvpl"
        if not p.exists():
            print(f"  警告: 缺 {name}（3p 媒体可能不全）")
            continue
        raw = decode_dvpl(p.read_bytes())
        streams = parse_akpk_streams(raw)
        for fid, (off, ln) in streams.items():
            pool.setdefault(fid, raw[off:off + ln])
        print(f"  {name}: {len(streams)} 流式媒体")
    missing = [(cid, fid) for cid, items in events.items() for fid, _ in items if fid not in pool]
    if missing:
        raise SystemExit(f"FAIL-CLOSED: {len(missing)} 个 3p 媒体不在任何 pck: {missing[:8]}")

    # 3) 逐媒体转换（wem → wav(vgmstream) → ogg(libsndfile)）
    index: dict[str, dict[str, list[str]]] = {c: {} for c in events}
    total_bytes = 0
    n_done = 0
    for cid, items in events.items():
        for fid, path in items:
            layer, nn = wem_meta(path)
            wem = pool[fid]
            if not (wem[:4] == b"RIFF" and wem[8:12] == b"WAVE"):
                raise SystemExit(f"FAIL-CLOSED: 媒体不是 WAVE 容器: {path}")
            fname = f"{cid}_{layer}_{nn}.ogg"
            dst = out_dir / fname
            if dst.exists():
                print(f"  重名冲突（同名不同媒体）: {fname}")
                raise SystemExit("FAIL-CLOSED: 命名契约要求 (class,layer,nn) 唯一")
            with tempfile.TemporaryDirectory() as td:
                wem_path = pathlib.Path(td) / "in.wem"
                wav_path = pathlib.Path(td) / "out.wav"
                wem_path.write_bytes(wem)
                r = subprocess.run([str(vgm), "-o", str(wav_path), str(wem_path)],
                                   capture_output=True, text=True)
                if r.returncode != 0 or not wav_path.exists():
                    raise SystemExit(f"FAIL-CLOSED: vgmstream 解码失败 {path}\n{r.stderr[-800:]}")
                data, sr = sf.read(wav_path, dtype="float32", always_2d=True)
            sf.write(dst, data, sr, format="OGG", subtype="VORBIS")
            index[cid].setdefault(layer, []).append(fname)
            total_bytes += dst.stat().st_size
            n_done += 1

    # 4) index.json（重写式：本目录 *.ogg + index.json 由本工具独占）
    (out_dir / "index.json").write_text(
        json.dumps(index, ensure_ascii=False, sort_keys=True, indent=1), encoding="utf-8")
    print(f"导出 {n_done} 个 OGG（{total_bytes / 1e6:.1f} MB）→ {out_dir}")
    for cid in sorted(index):
        layers = {k: len(v) for k, v in sorted(index[cid].items())}
        print(f"  {cid}: {layers}")


if __name__ == "__main__":
    main()
