#!/usr/bin/env python3
"""从 11.22 tier XI 超测预览 ZIP（公开 CDN，见 docs/game-data-sources.md §2.7）按需提取
科技树与车辆清单事实，产出 `--tech-tree-extra` 增量 JSON 供
`tools/export_research_loadout.py --with-research-graph` 合并 XI 节点。

完整包约 20 GB，本工具**不做全量下载**：ZIP central directory 支持 Range 定位，
只取下列小文件的字节区间并解压：

- `Data/Configs/TechTree/{nation}_tree.yaml.dvpl`（9 国：XI 节点 position / premium_rows）
- `Data/XML/item_defs/vehicles/{nation}/list.xml.dvpl`（preview bridge 中各国车辆的
  list 事实：id/level/price/tags/userString）

预览车的 tankId 用 `data/preview_tank_id_bridge.json` 对号（主 bridge 只含 11.21 的
738 辆；预览 bridge 由 11.22 包 list.xml 的 `(id<<8)|国家低位` 派生）。
输出（默认 `data/research_t11_preview/tech_tree_extra.json`）：

    {
      "meta": {"source": "...", "gameVersion": "...", "fetchedAt": "...", "tanks": [...]},
      "trees": {"<nation>": {"premiumRows": int, "tanks": {"<stem>": {"position": [x, y]}}}},
      "list":  {"<nation>": {"<stem>": {"tankId": int, "listId": int, "level": int,
                                         "price": int, "priceRaw": str, "tags": str,
                                         "userString": str}}}
    }

fail-closed：任何定位/解压/DVPL/yaml 解析失败直接报错退出，不写部分产物。
只读远端与本地文件；不执行包内任何内容。
"""
from __future__ import annotations

import argparse
import http.client
import json
import pathlib
import struct
import sys
import urllib.request
import zlib
import xml.etree.ElementTree as ET
from datetime import datetime, timezone

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "tools" / "wotbtools"))
from wotb_sc2 import decode_dvpl  # noqa: E402

DEFAULT_URL = "http://cdn.static.wotb.app/win7/WotBlitz_11.4.0.130_t11p2.zip"
REPO_ROOT = pathlib.Path(__file__).resolve().parents[1]
NATIONS = ["ussr", "germany", "usa", "china", "france", "uk", "japan", "other", "european"]
DEFAULT_EXPECTED_MARKER = "11.22_supertest_tier11_5"
CHUNK = 1 << 20

ZIP_LOCAL_SIG = 0x04034B50
ZIP_EOCD_SIG = 0x06054B50
ZIP64_EOCD_LOCATOR_SIG = 0x07064B50
ZIP64_EOCD_SIG = 0x06064B50


class RangeReader:
    """ZIP 成员随机读取：按需 `Range` 请求字节区间，聚合小请求降低往返次数。"""

    def __init__(self, url: str) -> None:
        self.url = url
        req = urllib.request.Request(url, method="HEAD")
        with urllib.request.urlopen(req, timeout=60) as resp:
            if resp.status != 200:
                raise RuntimeError(f"HEAD {resp.status}")
            length = resp.headers.get("Content-Length")
            ranges = (resp.headers.get("Accept-Ranges") or "").lower()
            if not length or "bytes" not in ranges:
                raise RuntimeError("server does not advertise Content-Length + Accept-Ranges")
            self.length = int(length)
        self._fetched = 0

    def fetch(self, start: int, end: int) -> bytes:
        """闭开区间 [start, end)。CDN 偶发 503/429/超时：指数退避重试。"""
        if start < 0 or end > self.length or start >= end:
            raise RuntimeError(f"bad range [{start}, {end}) of {self.length}")
        import time

        last_error: Exception | None = None
        for attempt in range(6):
            if attempt:
                time.sleep(min(2 ** attempt, 30))
            req = urllib.request.Request(self.url, headers={"Range": f"bytes={start}-{end - 1}"})
            try:
                with urllib.request.urlopen(req, timeout=120) as resp:
                    if resp.status not in (200, 206):
                        raise RuntimeError(f"range GET {resp.status}")
                    data = resp.read()
            except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError,
                    http.client.IncompleteRead, ConnectionError, OSError) as exc:
                status = getattr(exc, "code", None)
                if isinstance(exc, urllib.error.HTTPError) and status not in (503, 429, 500, 502):
                    raise
                last_error = exc
                continue
            self._fetched += len(data)
            if len(data) != end - start:
                raise RuntimeError(f"short read: {len(data)} != {end - start}")
            return data
        raise RuntimeError(f"range GET [{start}, {end}) failed after retries: {last_error}")

    def fetch_chunks(self, start: int, end: int) -> bytes:
        parts = []
        pos = start
        while pos < end:
            stop = min(pos + CHUNK, end)
            parts.append(self.fetch(pos, stop))
            pos = stop
        return b"".join(parts)


def find_eocd(reader: RangeReader) -> tuple[int, int, int]:
    """返回 (central_dir_offset, central_dir_size, total_entries)，处理 ZIP64。"""
    tail_size = min(reader.length, 1 << 20)
    tail = reader.fetch(reader.length - tail_size, reader.length)
    eocd_pos = tail.rfind(struct.pack("<I", ZIP_EOCD_SIG))
    if eocd_pos < 0:
        raise RuntimeError("EOCD not found")
    abs_eocd = reader.length - tail_size + eocd_pos
    (_sig, _disk, _cd_disk, _entries_here, _total, cd_size, cd_offset, _comment) = struct.unpack(
        "<IHHHHIIH", tail[eocd_pos:eocd_pos + 22])
    # ZIP64：EOCD 字段全 0xFF 时经 locator 找 ZIP64 EOCD。
    if cd_offset == 0xFFFFFFFF or cd_size == 0xFFFFFFFF or _total == 0xFFFF:
        loc_pos = tail.rfind(struct.pack("<I", ZIP64_EOCD_LOCATOR_SIG), 0, eocd_pos)
        if loc_pos < 0:
            raise RuntimeError("ZIP64 EOCD locator not found")
        (z64_eocd_offset,) = struct.unpack("<Q", tail[loc_pos + 8:loc_pos + 16])
        head = reader.fetch(z64_eocd_offset, z64_eocd_offset + 56)
        if struct.unpack("<I", head[:4])[0] != ZIP64_EOCD_SIG:
            raise RuntimeError("ZIP64 EOCD signature mismatch")
        cd_size, cd_offset = struct.unpack("<QQ", head[40:56])
    return cd_offset, cd_size, _total


def parse_central_directory(reader: RangeReader, cd_offset: int, cd_size: int) -> dict[str, tuple[int, int]]:
    """central directory → {name: (local_header_offset, compressed_size)}。

    ZIP64 extra field (id 0x0001) 按 central entry 的字段落位补齐
    （未压缩大小 / 压缩大小 / local header offset 依序只放 0xFFFFFFFF 的项）。
    """
    blob = reader.fetch_chunks(cd_offset, cd_offset + cd_size)
    entries: dict[str, tuple[int, int]] = {}
    pos = 0
    sig = struct.pack("<I", 0x02014B50)
    while pos + 46 <= len(blob):
        if blob[pos:pos + 4] != sig:
            raise RuntimeError(f"central directory signature mismatch at {pos}")
        # 头部布局（46B）：sig(4) ver_made(2) ver_need(2) flags(2) method(2) time(2)
        # date(2) crc(4) comp(4) uncomp(4) name_len(2) extra_len(2) comment_len(2)
        # disk_start(2) int_attr(2) ext_attr(4) local_offset(4)。
        (_ver_made, _ver_need, _flags, _method, _mtime, _mdate,
         _crc, comp_size, uncomp_size, name_len, extra_len, comment_len,
         _disk_start, _int_attr, _ext_attr, local_offset) = struct.unpack(
            "<6H3I5H2I", blob[pos + 4:pos + 46])
        name = blob[pos + 46:pos + 46 + name_len].decode("utf-8")
        extra = blob[pos + 46 + name_len:pos + 46 + name_len + extra_len]
        # ZIP64 extra (id 0x0001)：只按字段落位补 0xFFFFFFFF 的项，顺序
        # 未压缩大小 / 压缩大小 / local header offset（ZIP 规范 §4.5.3）。
        epos = 0
        while epos + 4 <= len(extra):
            eid, esz = struct.unpack("<HH", extra[epos:epos + 4])
            payload = extra[epos + 4:epos + 4 + esz]
            vpos = 0
            if eid == 0x0001:
                if uncomp_size == 0xFFFFFFFF and vpos + 8 <= len(payload):
                    uncomp_size = struct.unpack("<Q", payload[vpos:vpos + 8])[0]
                    vpos += 8
                if comp_size == 0xFFFFFFFF and vpos + 8 <= len(payload):
                    comp_size = struct.unpack("<Q", payload[vpos:vpos + 8])[0]
                    vpos += 8
                if local_offset == 0xFFFFFFFF and vpos + 8 <= len(payload):
                    local_offset = struct.unpack("<Q", payload[vpos:vpos + 8])[0]
                    vpos += 8
            epos += 4 + esz
        entries[name] = (local_offset, comp_size)
        pos += 46 + name_len + extra_len + comment_len
    return entries


def read_member(reader: RangeReader, entries: dict[str, tuple[int, int]], name: str) -> bytes:
    offset, comp_size = entries[name]
    header = reader.fetch_chunks(offset, offset + 30)
    if struct.unpack("<I", header[:4])[0] != ZIP_LOCAL_SIG:
        raise RuntimeError(f"local header signature mismatch for {name}")
    name_len, extra_len = struct.unpack("<HH", header[26:30])
    data_start = offset + 30 + name_len + extra_len
    raw = reader.fetch_chunks(data_start, data_start + comp_size)
    method = struct.unpack("<H", header[8:10])[0]
    if method == 0:
        data = raw
    elif method == 8:
        data = zlib.decompress(raw, -15)
    else:
        raise RuntimeError(f"unsupported compression method {method} for {name}")
    return data


def parse_tree_yaml(text: str) -> tuple[dict[str, list[int]], int | None]:
    import yaml

    data = yaml.safe_load(text) or {}
    tanks = {}
    for stem, v in (data.get("tanks") or {}).items():
        position = v.get("position") if isinstance(v, dict) else None
        if not position:
            raise RuntimeError(f"tree entry without position: {stem}")
        tanks[str(stem)] = {"position": [int(position[0]), int(position[1])]}
    premium_rows = data.get("premium_rows")
    return tanks, (int(premium_rows) if premium_rows is not None else None)


def parse_list(text: str) -> dict[str, dict]:
    out = {}
    for v in ET.fromstring(text):
        price_raw = (v.findtext("price") or "").strip()
        out[v.tag] = {
            "listId": int(v.findtext("id")),
            "level": int(v.findtext("level") or 0),
            "price": int(price_raw) if price_raw.isdigit() else 0,
            "priceRaw": price_raw,
            "tags": (v.findtext("tags") or "").split(),
            "userString": v.findtext("userString"),
        }
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--url", default=DEFAULT_URL)
    ap.add_argument("--out", default="data/research_t11_preview/tech_tree_extra.json")
    ap.add_argument("--preview-bridge", default="data/preview_tank_id_bridge.json")
    ap.add_argument("--data-root", default=None,
                    help="同时物化最小预览 Data 根（--modules-only --merge 的 --game-data "
                         "入参；含 version/Strings/预览车所在国别的 XML+components+Parameters）")
    ap.add_argument("--expected-marker", default=DEFAULT_EXPECTED_MARKER,
                    help=f"version.txt 必含的构建标记（fail-closed）；默认 {DEFAULT_EXPECTED_MARKER}")
    args = ap.parse_args()

    bridge = json.loads((REPO_ROOT / args.preview_bridge).read_text(encoding="utf-8"))
    by_nation_stem = {(v["nation"], v["stem"]): int(tid) for tid, v in bridge.items()}
    preview_nations = sorted({v["nation"] for v in bridge.values()})

    reader = RangeReader(args.url)
    print(f"zip size = {reader.length} bytes")
    cd_offset, cd_size, _total = find_eocd(reader)
    print(f"central directory at {cd_offset} (+{cd_size} bytes)")
    entries = parse_central_directory(reader, cd_offset, cd_size)
    print(f"central directory entries: {len(entries)}")

    wanted: dict[str, str] = {}
    data_root_members: set[str] = set()
    if args.data_root:
        # 最小 Data 根成员（zip 内任意前缀，按尾部/包含路径匹配）：version、Strings、
        # 预览车所在国别的 list + components、每辆预览车的车辆 XML 与 Parameters yaml。
        for name in entries:
            lowered = name.lower()
            if lowered.endswith("data/version.txt.dvpl"):
                data_root_members.add(name)
                continue
            if lowered.endswith(("strings/en.yaml.dvpl", "strings/zh-hans.yaml.dvpl")):
                data_root_members.add(name)
                continue
            for nation in preview_nations:
                comp_dir = f"xml/item_defs/vehicles/{nation}/components/"
                if lowered.endswith(f"xml/item_defs/vehicles/{nation}/list.xml.dvpl"):
                    data_root_members.add(name)
                elif comp_dir in lowered and lowered.endswith(".xml.dvpl") \
                        and "/" not in lowered[lowered.index(comp_dir) + len(comp_dir):]:
                    data_root_members.add(name)
            for nation in preview_nations:
                for tid, v in bridge.items():
                    if v["nation"] != nation:
                        continue
                    stem = v["stem"]
                    if lowered.endswith(f"xml/item_defs/vehicles/{nation}/{stem}.xml.dvpl".lower()):
                        data_root_members.add(name)
                    if f"3d/tanks/parameters/{nation}/{stem}" in lowered and lowered.endswith(".dvpl"):
                        data_root_members.add(name)
        print(f"data-root members: {len(data_root_members)}")
    for name in entries:
        # 独立版布局可能带目录前缀（app/Data/... 或 Data/...）：按尾部路径匹配。
        lowered = name.lower()
        for nation in NATIONS:
            if lowered.endswith(f"configs/techtree/{nation}_tree.yaml.dvpl"):
                wanted[name] = f"tree:{nation}"
        for nation in preview_nations:
            if lowered.endswith(f"xml/item_defs/vehicles/{nation}/list.xml.dvpl"):
                wanted[name] = f"list:{nation}"
    missing = [n for n in NATIONS if not any(k.endswith(f"tree:{n}") for k in wanted.values())]
    if missing:
        raise RuntimeError(f"tree yaml not found in zip for nations: {missing}")
    print(f"selected {len(wanted)} members")

    trees: dict[str, dict] = {}
    lists: dict[str, dict] = {}
    version_txt = None
    for name, role in sorted(wanted.items()):
        kind, nation = role.split(":", 1)
        data = read_member(reader, entries, name)
        text = decode_dvpl(data).decode("utf-8")
        if kind == "tree":
            tanks, premium_rows = parse_tree_yaml(text)
            trees[nation] = {"premiumRows": premium_rows, "tanks": tanks}
            print(f"  {name}: {len(tanks)} nodes, premium_rows={premium_rows}")
        else:
            lists[nation] = parse_list(text)
            print(f"  {name}: {len(lists[nation])} entries")
            # 顺手核验版本文件，锚定数据来源（fail-closed：构建标记不符就拒绝）
            vname = next((n for n in entries if n.lower().endswith("data/version.txt.dvpl")), None)
            if version_txt is None and vname:
                version_txt = decode_dvpl(read_member(reader, entries, vname)).decode("utf-8").strip()
    if version_txt is None:
        raise RuntimeError("Data/version.txt.dvpl not found in zip")
    if args.expected_marker not in version_txt:
        raise RuntimeError(f"unexpected version.txt: {version_txt!r} (want marker {args.expected_marker!r})")

    if args.data_root:
        root = pathlib.Path(args.data_root)
        if root.exists() and any(root.iterdir()):
            raise RuntimeError(f"data root not empty: {root}（换新目录，避免混入旧文件）")
        written = 0
        for name in sorted(data_root_members):
            # 条目形如 app/Data/<rest>（standalone 布局）：去掉到 Data/ 的前缀再落盘。
            marker = name.lower().index("data/")
            rel = pathlib.Path(name[marker + len("data/"):])
            dest = root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(read_member(reader, entries, name))
            written += 1
        print(f"data root written: {written} files -> {root}")
        probe = root / "version.txt.dvpl"
        if not probe.exists():
            raise RuntimeError(f"data root missing version.txt.dvpl: {probe}")

    # 预览定义车（list 命中）必须能在 preview bridge 对号 tankId，否则这条链路断了。
    # list.xml 全量是 11.21 现役车 + 预览 XI 定义车；bridge 只含后者，其余条目
    # 不是本工具的产物职责，丢弃。bridge 的每一条都必须命中，缺一即 fail-closed。
    for nation, items in lists.items():
        for stem, facts in items.items():
            facts["tankId"] = by_nation_stem.get((nation, stem))
    lists = {n: {s: f for s, f in items.items() if f["tankId"] is not None} for n, items in lists.items()}
    kept = [(n, s) for n, items in lists.items() for s in items]
    if len(kept) != len(bridge):
        raise RuntimeError(f"expected exactly {len(bridge)} preview-defined vehicles, got: {kept}")
    print(f"preview-defined vehicles kept: {kept}")

    out = {
        "meta": {
            "source": args.url,
            "gameVersion": version_txt,
            "fetchedAt": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "zipSize": reader.length,
            "tanks": sorted(by_nation_stem.values()),
        },
        "trees": trees,
        "list": lists,
    }
    out_path = REPO_ROOT / args.out
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(out, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    print(f"fetched {reader._fetched} bytes via Range requests")
    print(f"out={out_path}")


if __name__ == "__main__":
    main()
