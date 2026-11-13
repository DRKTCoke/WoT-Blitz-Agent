#!/usr/bin/env python3
"""从本机 WoT Blitz 客户端提取 `tanks.pb` / `models.pb` 的等价数据（同构 JSON）。

对照基准是 `src/wargaming/blitzkit.rs` 的 serde 输出形状（`TankFullData` / `TankModelInfo`），
`tools/compare_vehicle_data.py` 据此做逐字段 diff。

数据来源（全部读磁盘，**不读 `item_defs.zip`**——那是旧快照，869 条 vs 磁盘 881 条）：
  * `XML/item_defs/vehicles/<nation>/<stem>.xml.dvpl`  车辆定义（数值权威）
  * `XML/item_defs/vehicles/<nation>/list.xml.dvpl`    tier/类别/稀有度/本地化键
  * `XML/item_defs/vehicles/<nation>/components/*.xml.dvpl`  共享模块 + `<ids>` 局部 id 表
  * `Strings/en.yaml.dvpl`                             本地化显示名
  * `3d/Tanks/Parameters/<nation>/<stem>.yaml.dvpl`    碰撞盒 / maskSlice（models 侧）

tank_id 桥：`--emit-bridge` 从 `data/tanks.pb` 一次性导出
`{tank_id: {nation, stem, slug}}` → `data/tank_id_bridge.json`（738 条，入库后提取链
不再依赖 BlitzKit）。客户端磁盘上还有 ~146 个桥外的车辆 XML（开发/超测/未实装），
默认跳过、计数上报；`--all-local` 才提取它们（tank_id 记 -文件数序号，仅供对照）。

实现铁律（原文出自可行性报告 A，已删档见 git 历史；提取链口径见 docs/game-data-sources.md
§5.2b。违反会静默出错）：
  1. 重复 XML 标签 **last-wins**（`<armor_N>`/`<pitchLimits>`/`<transition>` 会重复）；
  2. 0/缺 = 省略语义：`<weight>`/`<gunPosition>`/`<hullPosition>`/全 0 bbox ⇔ pb 字段缺失；
     0 厚板不入 `plates`，但 `spaced` 保留（零厚板可带 vehicleDamageFactor）；
  3. 模块 id = `(局部 id << 8) | 国家低位`，国家低位 ussr=1 germany=17 usa=33 china=49
     france=65 uk=81 japan=97 other=113 european=129（模块局部 id 来自 components 的 `<ids>`，
     弹种局部 id 来自 shells.xml 条目的 `<id>`）；
  4. 弹夹/弹鼓判定**只认车辆内联** `<clip>`/`<pumpGunMode>`（共享定义里的 clip 不算，
     否则 6 辆车假弹夹）；`<burst>` 是表现参数，禁止用来判 is_burst；
  5. `mask` = yaml `maskSlice.<node>.planePosition[1]`（+Y 分量），`enabled:false` ⇒ 省略；
     同一项的完整记录（position/normal/axis/两个 slice mode/锥台参数）另存在每个 gun 的
     `mask_slice`（models.json → `data/gun_mask_slices.json`，viewer 按客户端原值建裁切面）；
  6. `pitchLimits` 首值=min 次值=max（不取负）；`extraPitchLimits` 的 front/back 为
     `min max range` 三值；`<transition>` 重复时 last-wins；
  7. `-180 180` 的 yawLimits ⇒ pb 省略（输出 null）；
  8. `caliber_factor`（pb 注释里的"口径系数"）实为炮定义的 `<rotationSpeed>`。

用法：
    python tools/extract_vehicles.py --emit-bridge              # 生成 data/tank_id_bridge.json
    python tools/extract_vehicles.py                            # 全量提取 → data/cache/local_pb/
    python tools/extract_vehicles.py --tank 9489 --tank 7169
    python tools/extract_vehicles.py --all-local                # 连桥外车辆一起提取
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import pathlib
import re
import struct
import sys
import time
import xml.etree.ElementTree as ET

TOOLS_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = TOOLS_DIR.parent
sys.path.insert(0, str(TOOLS_DIR / "wotbtools"))

from wotb_sc2 import decode_dvpl  # noqa: E402

GAME_DIR_CANDIDATES = [
    "D:/SteamLibrary/steamapps/common/World of Tanks Blitz/Data",
    "C:/Program Files (x86)/Steam/steamapps/common/World of Tanks Blitz/Data",
    "/mnt/c/Program Files (x86)/Steam/steamapps/common/World of Tanks Blitz/Data",
    "/mnt/d/SteamLibrary/steamapps/common/World of Tanks Blitz/Data",
]

# 模块 id 的国家低位（= 报告 A §4.2 的"国家序×16+1"）
NATION_LOW = {"ussr": 1, "germany": 17, "usa": 33, "china": 49, "france": 65,
              "uk": 81, "japan": 97, "other": 113, "european": 129}
CLASS_WORDS = ("lightTank", "mediumTank", "heavyTank", "AT-SPG")
# shells.xml `<kind>` 语义枚举 → BlitzKit tanks.pb field9 的 4 值闭集（0=AP/1=APCR/2=HEAT/3=HE）
KIND_ID = {"ARMOR_PIERCING": 0, "ARMOR_PIERCING_CR": 1, "HOLLOW_CHARGE": 2, "HIGH_EXPLOSIVE": 3}


def default_game_data() -> pathlib.Path:
    for c in GAME_DIR_CANDIDATES:
        p = pathlib.Path(c)
        if p.is_dir():
            return p
    return pathlib.Path(GAME_DIR_CANDIDATES[0])


def mid(nation: str, local: int) -> int:
    return (local << 8) | NATION_LOW[nation]


# ---------------------------------------------------------------------------
# protobuf（仅 --emit-bridge 用：tanks.pb field1/field11/field2/field32）
# ---------------------------------------------------------------------------
def _pb_varint(b: bytes, i: int) -> tuple[int, int]:
    r = s = 0
    while True:
        x = b[i]
        i += 1
        r |= (x & 0x7F) << s
        if not x & 0x80:
            return r, i
        s += 7


def _pb_fields(b: bytes) -> list[tuple[int, int, object]]:
    i, out = 0, []
    while i < len(b):
        key, i = _pb_varint(b, i)
        fn, wt = key >> 3, key & 7
        if wt == 0:
            v, i = _pb_varint(b, i)
            out.append((fn, wt, v))
        elif wt == 2:
            ln, i = _pb_varint(b, i)
            out.append((fn, wt, b[i:i + ln]))
            i += ln
        elif wt == 5:
            out.append((fn, wt, struct_unpack_f(b, i)))
            i += 4
        elif wt == 1:
            out.append((fn, wt, b[i:i + 8]))
            i += 8
        else:
            raise ValueError(f"wire {wt}")
    return out


def struct_unpack_f(b: bytes, i: int) -> float:
    return struct.unpack_from("<f", b, i)[0]


def emit_bridge_from_client(game_data: pathlib.Path, out_path: pathlib.Path) -> int:
    """**纯客户端**生成 tank_id 桥接表（不读 tanks.pb）。

    实测（2026-10-10）：`list.xml` 每个车辆条目的 `<id>` 是国家内局部 id，全局 tank_id =
    `(局部 id << 8) | 国家基数`（与 guns/shells 同一编码）——735/735 与 pb 导出的桥表
    stem/nation 逐条一致。`slug`（BlitzKit 专有）留空：全仓无消费者（文件定位一律用 stem）。
    """
    table: dict[str, dict] = {}
    for nation, low in NATION_LOW.items():
        p = game_data / "XML/item_defs/vehicles" / nation / "list.xml.dvpl"
        if not p.exists():
            continue
        txt = decode_dvpl(p.read_bytes()).decode("utf-8", "replace")
        # 逐行扫描：条目形如 `<Stem>` 换行 `<id>NN</id>`（不用跨行正则，避免转义层踩坑）
        lines = txt.splitlines()
        for i, ln in enumerate(lines):
            m = re.match(r"^[ \t]*<([A-Za-z0-9_.\-]+)>[ \t]*$", ln)
            if not m or i + 1 >= len(lines):
                continue
            m2 = re.match(r"^[ \t]*<id>(\d+)</id>[ \t]*$", lines[i + 1])
            if not m2:
                continue
            stem, lid = m.group(1), int(m2.group(1))
            table[str((lid << 8) | low)] = {"nation": nation, "stem": stem, "slug": ""}
    out_path.write_text(json.dumps(table, ensure_ascii=False, sort_keys=True, indent=1),
                        encoding="utf-8")
    print(f"桥接表（客户端 list.xml）{len(table)} 条 → {out_path}")
    return len(table)


def emit_bridge(pb_path: pathlib.Path, out_path: pathlib.Path) -> int:
    buf = pb_path.read_bytes()
    table: dict[str, dict] = {}
    for fn, wt, v in _pb_fields(buf):
        if fn != 1 or wt != 2:
            continue
        entry = _pb_fields(v)
        tid = next((x[2] for x in entry if x[0] == 1 and x[1] == 0), None)
        main = next((x[2] for x in entry if x[0] == 2 and x[1] == 2), None)
        if tid is None or main is None:
            continue
        nation = stem = slug = ""
        for f2, w2, v2 in _pb_fields(main):
            if f2 == 11 and w2 == 2:
                nation = v2.decode("utf-8", "replace")
            elif f2 == 32 and w2 == 2:
                stem = v2.decode("utf-8", "replace")
            elif f2 == 2 and w2 == 2:
                slug = v2.decode("utf-8", "replace")
        table[str(tid)] = {"nation": nation, "stem": stem, "slug": slug}
    out_path.write_text(json.dumps(table, ensure_ascii=False, sort_keys=True, indent=1),
                        encoding="utf-8")
    print(f"桥接表 {len(table)} 条 → {out_path}")
    return len(table)


# ---------------------------------------------------------------------------
# 基础设施：last-wins 标量、共享定义合并、本地化
# ---------------------------------------------------------------------------
def last_child(parent: ET.Element | None, tag: str) -> ET.Element | None:
    """直接子元素中该标签的**最后一个**（重复标签 last-wins）。"""
    if parent is None:
        return None
    found = None
    for ch in parent:
        if ch.tag == tag:
            found = ch
    return found


def last_text(parent: ET.Element | None, tag: str) -> str | None:
    ch = last_child(parent, tag)
    if ch is None:
        return None
    return (ch.text or "").strip() or None


def ffloat(parent: ET.Element | None, tag: str, default: float = 0.0) -> float:
    t = last_text(parent, tag)
    try:
        return float(t) if t is not None else default
    except ValueError:
        return default


def fint(parent: ET.Element | None, tag: str, default: int = 0) -> int:
    t = last_text(parent, tag)
    try:
        return int(float(t)) if t is not None else default
    except ValueError:
        return default


class DefResolver:
    """车辆内联条目 + components 共享定义的合并视图（内联覆盖共享，逐标签 last-wins）。"""

    def __init__(self, inline: ET.Element | None, shared: ET.Element | None):
        self.inline, self.shared = inline, shared

    def has_shared_marker(self) -> bool:
        return bool(self.inline is not None and (self.inline.text or "").strip() == "shared")

    def elem(self, tag: str) -> ET.Element | None:
        """合并后该标签的元素（内联优先，否则共享）。"""
        el = last_child(self.inline, tag) if self.inline is not None else None
        if el is not None:
            return el
        return last_child(self.shared, tag) if self.shared is not None else None

    def text(self, tag: str) -> str | None:
        el = self.elem(tag)
        return (el.text or "").strip() if el is not None else None

    def floats(self, tag: str, default=0.0) -> float:
        el = self.elem(tag)
        try:
            return float((el.text or "").strip()) if el is not None else default
        except ValueError:
            return default

    def ints(self, tag: str, default=0) -> int:
        el = self.elem(tag)
        try:
            return int(float((el.text or "").strip())) if el is not None else default
        except ValueError:
            return default

    def inline_elem(self, tag: str) -> ET.Element | None:
        return last_child(self.inline, tag) if self.inline is not None else None

    def user_key(self) -> str | None:
        """userString 的**原文键**（含段前缀，去 `#`）：`#gb_vehicles:_47mm_3pdrAP` →
        `gb_vehicles:_47mm_3pdrAP`。名字解析按原文键查（段前缀 ≠ pb 国家名，见 Names.display）。"""
        for tag in ("userString", "shortUserString"):
            t = self.text(tag)
            if t:
                return t[1:] if t.startswith("#") else t
        return None


class Components:
    """一个国家的 components/*.xml：`<ids>` 局部 id 表 + `<shared>` 定义。"""

    def __init__(self, comp_dir: pathlib.Path):
        self.ids: dict[str, dict[str, int]] = {}
        self.shared: dict[str, dict[str, ET.Element]] = {}
        for kind in ("guns", "shells", "engines", "chassis", "turrets", "radios", "fuelTanks"):
            self.ids[kind] = {}
            self.shared[kind] = {}
        for kind in ("guns", "engines", "chassis", "turrets"):
            p = comp_dir / f"{kind}.xml.dvpl"
            if not p.exists():
                continue
            root = ET.fromstring(decode_dvpl(p.read_bytes()))
            ids_el = last_child(root, "ids")
            if ids_el is not None:
                for ch in ids_el:
                    m = re.fullmatch(r"\d+", (ch.text or "").strip())
                    if m is not None:
                        self.ids[kind][ch.tag] = int(m.group(0))
            shared_el = last_child(root, "shared")
            if shared_el is not None:
                for ch in shared_el:
                    self.shared[kind][ch.tag] = ch
        # 弹种：顶层条目自带 <id>（无 ids/shared 分节）
        p = comp_dir / "shells.xml.dvpl"
        if p.exists():
            root = ET.fromstring(decode_dvpl(p.read_bytes()))
            self.shared["shells"] = {ch.tag: ch for ch in root
                                     if ch.tag not in ("icons", "nextAvailableId")
                                     and last_child(ch, "id") is not None}
            for name, el in self.shared["shells"].items():
                v = last_text(el, "id")
                if v is not None:
                    self.ids["shells"][name] = int(float(v))

    def local_id(self, kind: str, name: str) -> int | None:
        return self.ids.get(kind, {}).get(name)

    def shared_def(self, kind: str, name: str) -> ET.Element | None:
        return self.shared.get(kind, {}).get(name)


class Names:
    """`Strings/en.yaml` 的 `#<nation>_vehicles:<KEY>` → 显示名。

    **第二字符串源（2026-10-10）**：客户端运行时会从 CDN 下载**本地化覆盖层**到
    `%LOCALAPPDATA%/wotblitz/DAVAProject/cache/localizations/<lang>.yaml`（带 `.etag` 的
    HTTP 缓存）。随包的 `Data/Strings/*.yaml` 是构建时的基础快照，**不含**上线后新增的
    活动/BP 车与模块名——这正是"客户端能正常显示、而本地 Data 查不到"的原因。覆盖层只按
    客户端语言缓存（本机为 zh-Hans），但**专名跨语言同形**（Turbo/Magnate/Panlong…），
    故：仅作**缺失键的回退**、且只接受**拉丁值**（避免把中文译名灌进英文数据）。

    **第三字符串源（2026-11-13）**：SmartDLC 动态本地化
    `%LOCALAPPDATA%/wotblitz/DAVAProject/cache/dynamicContentLocalizations/<lang>.yaml`
    此前实测"不含车辆/模块名"——被 11.21.0_1 槽推翻：新收藏车 Oth99_Relic（Relic D3）
    的车辆/部件/炮/引擎名键**只在 dynloc**（`localizations/` 更新后仍 0 命中），随
    `s11.21.0_1` 的 `localizations/11.21.0_1_7405734/` 下发。同样只作缺失键回退、
    只收拉丁值（"Relic D3" 天然通过）。
    """

    # YAML 双引号串的转义（客户端文件里真实出现：`\xe4`→ä、`\u00A0`→NBSP、`\n`）
    _ESC = re.compile(r"\\(x[0-9a-fA-F]{2}|u[0-9a-fA-F]{4}|U[0-9a-fA-F]{8}|n|t|\"|\\|/)")

    # 运行时本地化覆盖层（客户端语言缓存；仅回退用，见类注释）。
    # dynloc 层排在普通覆盖层之后：后者已覆盖 738 辆现役车，dynloc 只补动态下发的新车。
    OVERLAY_DIRS = (
        "%LOCALAPPDATA%/wotblitz/DAVAProject/cache/localizations",
        "~/wotblitz/DAVAProject/cache/localizations",
        "%LOCALAPPDATA%/wotblitz/DAVAProject/cache/dynamicContentLocalizations",
        "~/wotblitz/DAVAProject/cache/dynamicContentLocalizations",
    )

    @classmethod
    def _unescape(cls, s: str) -> str:
        def sub(m):
            e = m.group(1)
            if e[0] in "xuU":
                try:
                    return chr(int(e[1:], 16))
                except ValueError:
                    return m.group(0)
            return {"n": "\n", "t": "\t", '"': '"', "\\": "\\", "/": "/"}.get(e, e)
        return cls._ESC.sub(sub, s)

    @staticmethod
    def _load_overlay() -> dict:
        """运行时本地化覆盖层 → {原文键: 值}（只收拉丁值）。缺失/不可读时返回空表。"""
        out: dict[str, str] = {}
        for d in Names.OVERLAY_DIRS:
            root = os.path.expandvars(d)
            if not os.path.isdir(root):
                continue
            for p in sorted(glob.glob(os.path.join(root, "*.yaml"))):
                try:
                    txt = open(p, encoding="utf-8", errors="replace").read()
                except OSError:
                    continue
                for m in re.finditer(
                        r'"#?([A-Za-z0-9_\-]+_vehicles:[A-Za-z0-9_.\-]+)"\s*:\s*"((?:[^"\\]|\\.)*)"',
                        txt):
                    v = Names._unescape(m.group(2))
                    # 只接受拉丁值：覆盖层是本机客户端语言的译名，中文值不能灌进英文数据
                    if v and all(ord(c) < 0x2E80 or c in "·—–'’" for c in v):
                        out.setdefault(m.group(1), v)
        return out

    def __init__(self, game_data: pathlib.Path, lang: str = "en"):
        p = game_data / "Strings" / f"{lang}.yaml.dvpl"
        self.map: dict[tuple[str, str], str] = {}
        if not p.exists():
            return
        txt = decode_dvpl(p.read_bytes()).decode("utf-8", "replace")
        self.bare: dict[str, str] = {}
        self.full: dict[str, str] = {}   # 原文键（含段前缀）→ 值，按客户端字面查
        for m in re.finditer(
                r'"#(\w+)_vehicles:([A-Za-z0-9_.\-]+)"\s*:\s*"((?:[^"\\]|\\.)*)"', txt):
            v = self._unescape(m.group(3))
            if v:
                self.map[(m.group(1), m.group(2))] = v
                self.bare.setdefault(m.group(2), v)
                self.full.setdefault(f"{m.group(1)}_vehicles:{m.group(2)}", v)
        # 运行时本地化覆盖层：**只作缺失键的回退**（随包 strings 优先，避免覆盖已知英文值）
        for k, v in self._load_overlay().items():
            prefix, key = k.split(":", 1)
            self.full.setdefault(k, v)
            self.map.setdefault((prefix, key), v)
            self.bare.setdefault(key, v)

    def _lookup(self, nation: str, key: str) -> str | None:
        return self.map.get((nation, key)) or self.bare.get(key)

    def display(self, nation: str, key: str | None) -> str | None:
        if not key:
            return None
        k = key[1:] if key.startswith("#") else key
        prefix = None
        if ":" in k:
            # 带段前缀的原文键：**按客户端字面查**优先——段前缀与 pb 国家名不一致
            # （uk 车/弹用 `gb_vehicles`），而裸键跨系会撞（`_47mm_3pdrAP` 在 usa 段是
            # 字面 "None"，uk 段才是 'QF AP Mk. IIIT'）。2026-10-10 实测修复。
            v = self.full.get(k)
            if v:
                return v
            prefix, k2 = k.split(":", 1)
            v = self.map.get((prefix, k2))
            if v:
                return v
            key = k2
        v = self._lookup(nation, key)
        if v:
            return v
        # 变体回退：客户端的重复弹/枪条目常用尾字母区分（`_75mm_M61AB` 无字符串、其基础
        # 条目 `_75mm_M61` 有）——**仅在基础键真实存在时才回退**，不发明名字。
        stripped = key
        while len(stripped) > 2 and stripped[-1].isascii() and stripped[-1].isupper():
            stripped = stripped[:-1]
            v = (self.full.get(prefix + ":" + stripped) if prefix else None) or \
                self._lookup(nation, stripped)
            if v:
                return v
        return None


# ---------------------------------------------------------------------------
# 装甲（plates/spaced）与俯仰
# ---------------------------------------------------------------------------
def parse_armor(armor_el: ET.Element | None) -> tuple[dict[str, float], list[int]]:
    """`<armor>` → (plates, spaced)。0 厚板不入 plates（last-wins：后值 0 会删掉前值）；
    带 vehicleDamageFactor 的板（含 0 厚）入 spaced。"""
    plates: dict[str, float] = {}
    spaced: list[int] = []
    if armor_el is None:
        return plates, spaced
    for ch in armor_el:
        m = re.fullmatch(r"armor_(\d+)", ch.tag)
        if not m:
            continue
        n = int(m.group(1))
        val_text = (ch.text or "").strip()
        try:
            val = float(val_text) if val_text else 0.0
        except ValueError:
            val = 0.0
        key = str(n)
        if val != 0.0:
            plates[key] = val
        else:
            plates.pop(key, None)  # last-wins：后值 0 覆盖前值非 0
        if last_child(ch, "vehicleDamageFactor") is not None and n not in spaced:
            spaced.append(n)
    return plates, spaced


def parse_pitch(gun: DefResolver) -> dict | None:
    """`<pitchLimits>` + `<extraPitchLimits>` → PitchLimitsInfo 形状（缺省省略键）。"""
    pl = gun.elem("pitchLimits")
    if pl is None:
        return None
    vals = (pl.text or "").split()
    if len(vals) < 2:
        return None
    out: dict = {"min": float(vals[0]), "max": float(vals[1])}
    ext = gun.elem("extraPitchLimits")
    if ext is not None:
        for side in ("front", "back"):
            el = last_child(ext, side)
            if el is not None:
                v = (el.text or "").split()
                if len(v) >= 3:
                    out[side] = {"min": float(v[0]), "max": float(v[1]), "range": float(v[2])}
        tr = last_child(ext, "transition")
        if tr is not None:
            try:
                out["transition"] = float((tr.text or "").strip())
            except ValueError:
                pass
    return out


def parse_vec_text(s: str | None) -> list[float] | None:
    if not s:
        return None
    v = [float(x) for x in s.split()]
    return v if len(v) == 3 else None


# ---------------------------------------------------------------------------
# 主提取
# ---------------------------------------------------------------------------
class Extractor:
    def __init__(self, game_data: pathlib.Path):
        self.gd = game_data
        self.veh_dir = game_data / "XML" / "item_defs" / "vehicles"
        self.params_dir = game_data / "3d" / "Tanks" / "Parameters"
        self.names = Names(game_data)
        self._comp: dict[str, Components] = {}
        self._list: dict[str, dict[str, ET.Element]] = {}
        self.failures: list[str] = []

    def components(self, nation: str) -> Components:
        if nation not in self._comp:
            self._comp[nation] = Components(self.veh_dir / nation / "components")
        return self._comp[nation]

    def list_entries(self, nation: str) -> dict[str, ET.Element]:
        if nation not in self._list:
            p = self.veh_dir / nation / "list.xml.dvpl"
            out: dict[str, ET.Element] = {}
            if p.exists():
                root = ET.fromstring(decode_dvpl(p.read_bytes()))
                for ch in root:
                    if ch.tag not in ("nextAvailableId",):
                        out[ch.tag] = ch
            self._list[nation] = out
        return self._list[nation]

    # -- 名字 -----------------------------------------------------------------
    def disp(self, nation: str, r: DefResolver) -> str | None:
        return self.names.display(nation, r.user_key())

    # -- tanks.json（TankFullData 形状）---------------------------------------
    def extract_tank(self, nation: str, stem: str) -> dict:
        low = NATION_LOW[nation]
        comp = self.components(nation)
        veh_path = self.veh_dir / nation / f"{stem}.xml.dvpl"
        root = ET.fromstring(decode_dvpl(veh_path.read_bytes()))
        list_el = self.list_entries(nation).get(stem)
        hull = last_child(root, "hull")

        # 稀有度：金价 ∧ (¬)collectible（list.xml）
        gold = False
        collectible = False
        tier = 0
        if list_el is not None:
            price = last_child(list_el, "price")
            gold = price is not None and last_child(price, "gold") is not None
            tags = (last_text(list_el, "tags") or "").split()
            collectible = "collectible" in tags
            tier = fint(list_el, "level")
            # 先精确 token 匹配（tags 里存在 lightTankArtefacts_User 这类**含车种词的复合
            # token**，子串匹配会把它误判成 lightTank——GB01_Medium_Mark_I 实测 tags 同时
            # 含 mediumTank 与 lightTankArtefacts_User）；再按 token 序子串兜底
            # （复合 token 如 mediumAT-SPG）。
            cls = (next((t for t in tags if t in CLASS_WORDS), None)
                   or next((w for t in tags for w in CLASS_WORDS if w in t), None))
        else:
            cls = None
        tank_type = cls or "lightTank"

        name = (self.names.display(nation, user_key_text(list_el, "shortUserString"))
                or self.names.display(nation, user_key_text(list_el, "userString"))
                or stem)  # pb 车名 = 短名（'Pz. IV G'），非全名

        turrets: list[dict] = []
        t0 = last_child(root, "turrets0")
        if t0 is not None:
            for t_el in t0:
                tname = t_el.tag
                tshared = comp.shared_def("turrets", tname)
                tr = DefResolver(t_el, tshared)
                tlocal = comp.local_id("turrets", tname)
                guns: list[dict] = []
                guns_el = tr.elem("guns")
                if guns_el is not None:
                    for g_el in guns_el:
                        gshared = comp.shared_def("guns", g_el.tag)
                        gr = DefResolver(g_el, gshared)
                        guns.append(self.extract_gun(nation, low, comp, gr, g_el))
                    # 保持 XML 文档序 = 游戏研发序（顶级 = 末位），与 tanks.pb 逐辆一致
                    # （2026-10-09 实测 735/735）。此前按 module_id 升序排序是错误启发式：
                    # 它把末位换成"最大模块 id"，与研发序顶级主炮在 91 辆车上不符。

                turrets.append({
                    "module_id": mid(nation, tlocal) if tlocal is not None else 0,
                    "name": self.disp(nation, tr) or tname,
                    "health": tr.ints("maxHealth"),
                    "weight": tr.floats("weight"),
                    "view_range": tr.floats("circularVisionRadius"),
                    "traverse_speed": tr.floats("rotationSpeed"),
                    "guns": guns,
                })

        engines: list[dict] = []
        e_el = last_child(root, "engines")
        if e_el is not None:
            for entry in e_el:
                er = DefResolver(entry, comp.shared_def("engines", entry.tag))
                engines.append({
                    "name": self.disp(nation, er) or entry.tag,
                    "power": er.floats("power"),
                    "fire_chance": er.floats("fireStartingChance"),
                })

        tracks: list[dict] = []
        c_el = last_child(root, "chassis")
        if c_el is not None:
            for entry in c_el:
                cr = DefResolver(entry, comp.shared_def("chassis", entry.tag))
                res = (cr.text("terrainResistance") or "").split()
                tracks.append({
                    "module_id": mid(nation, comp.local_id("chassis", entry.tag) or 0),
                    "name": self.disp(nation, cr) or entry.tag,
                    "weight": cr.floats("weight"),
                    "traverse_speed": cr.floats("rotationSpeed"),
                    "resistance_hard": float(res[0]) if res else None,
                    "resistance_medium": float(res[1]) if len(res) > 1 else None,
                })

        speed = last_child(root, "speedLimits")
        hp = fint(hull, "maxHealth")  # pb field10 = 车体 maxHealth（E-100 2200，非车体+炮塔之和）
        return {
            "tank_id": 0,  # 由调用方按桥接表填
            "dev_name": "",  # BlitzKit 的 slug，客户端无此概念，比较时跳过
            "model_name": stem,
            "name": name,
            "nation": nation,
            "tier": tier,
            "tank_type": tank_type,
            "hp": hp,
            "is_premium": bool(gold and not collectible),
            "is_collector": bool(gold and collectible),
            "speed_forward": ffloat(speed, "forward"),
            "speed_reverse": ffloat(speed, "backward"),
            "hull_traverse": 0.0,  # field27 与客户端非同量，不提取（比较时跳过）
            # field27 的真身：静止迷彩系数（车辆 XML `<invisibility><still>`；BK 同源）
            "camouflage_still": ffloat(last_child(root, "invisibility"), "still"),
            "weight": ffloat(hull, "weight"),
            "turrets": turrets,
            "engines": engines,
            "tracks": tracks,
        }

    # -- 单门炮 ---------------------------------------------------------------
    def extract_gun(self, nation: str, low: int, comp: Components,
                    gr: DefResolver, g_el: ET.Element) -> dict:
        # 弹夹/弹鼓判定**只认车辆内联**（铁律 4）
        inline_clip = gr.inline_elem("clip")
        inline_pump = gr.inline_elem("pumpGunMode")
        reload_time = gr.floats("reloadTime")
        # 连发组（PomPom）：`<burst count rate>` 是**时序**参数（组内弹间 60/rate），
        # 不是弹夹分类依据（铁律 4 不变：分类只看 clip/pumpGunMode）。与 BK 的
        # `GunDefinition.burst {count, interval}` 同义，供 DPM 的整匣循环公式使用。
        burst_el = gr.inline_elem("burst")
        if burst_el is None and gr.shared is not None:
            burst_el = last_child(gr.shared, "burst")
        burst_count = int(round(ffloat(burst_el, "count"))) if burst_el is not None else 0
        burst_rate = ffloat(burst_el, "rate") if burst_el is not None else 0.0
        burst_group = {
            "burst_group_count": burst_count if burst_count > 1 else 0,
            "burst_group_interval": (60.0 / burst_rate) if burst_count > 1 and burst_rate else 0.0,
        }
        if inline_pump is not None:
            times = [float(x) for x in (gr.text("pumpGunReloadTimes") or "").split()]
            clip = gr.inline_elem("clip")
            rate = ffloat(clip, "rate") if clip is not None else 0.0
            reload = {
                "reload": max(times) if times else reload_time,
                "is_burst": True, "is_drum": True,
                "burst_size": ffloat(clip, "count") if clip is not None else 0.0,
                "burst_interval": (60.0 / rate) if rate else 0.0,
                "burst_reloads": times,
                **burst_group,
            }
        elif inline_clip is not None:
            count = ffloat(inline_clip, "count")
            rate = ffloat(inline_clip, "rate")
            reload = {
                "reload": reload_time,
                "is_burst": True, "is_drum": False,
                "burst_size": count,
                "burst_interval": (60.0 / rate) if rate else 0.0,
                "burst_reloads": [],
                **burst_group,
            }
        else:
            reload = {"reload": reload_time, "is_burst": False, "is_drum": False,
                      "burst_size": 0.0, "burst_interval": 0.0, "burst_reloads": [],
                      **burst_group}

        shells: list[dict] = []
        # shots 的两处来源：**共享 gun 定义**的条目带弹道参数（speed/piercingPower/...），
        # **车辆内联**的 `<shots>` 决定这炮装哪些弹、顺序如何（条目是 `<shell>shared<price>` 引用，
        # 无弹道字段）。逐弹合并：键序取内联（缺则共享），弹道字段内联 last-wins 回退共享。
        inline_shots = gr.inline_elem("shots")
        shared_shots = last_child(gr.shared, "shots") if gr.shared is not None else None
        # 弹药列表的权威来源是**共享炮定义**的 <shots>——弹道/穿深（piercingPower）只在那里；
        # 车辆内联 <shots> 是价格/可用性叠加，个别车引用的弹种族与共享定义不同
        # （J24_Type_57 实测：内联 base 族在客户端无任何弹道数据 → 穿深 0；共享 A 族
        # =218/260/65 与 BlitzKit 一致）。按共享列表迭代，内联同标签条目合并覆盖其余字段。
        order = shared_shots if shared_shots is not None else inline_shots
        if order is not None:
            for shot in order:
                counterpart = last_child(inline_shots, shot.tag) if inline_shots is not None else None
                traj = DefResolver(counterpart, shot)
                shell_def = comp.shared_def("shells", shot.tag)
                s_el = shell_def
                sid = comp.local_id("shells", shot.tag)
                pp = (traj.text("piercingPower") or "").split()
                if not pp and s_el is not None:
                    # t11p2 起：客户端把弹道参数（piercingPower/speed/maxDistance）从炮共享
                    # <shots> 迁入 shells.xml 弹种条目本身（shots 变空标签）。shots 有值时
                    # 仍以 shots 为准（t11b 及以前语义），为空才回落弹种条目。
                    pp = (last_text(s_el, "piercingPower") or "").split()
                velocity = traj.floats("speed")
                if velocity == 0.0 and s_el is not None:
                    velocity = ffloat(s_el, "speed")
                range_ = traj.floats("maxDistance")
                if range_ == 0.0 and s_el is not None:
                    range_ = ffloat(s_el, "maxDistance")
                shells.append({
                    "id": mid(nation, sid) if sid is not None else 0,
                    "name": (self.names.display(nation, user_key_text(s_el, "userString"))
                             or shot.tag),
                    "shell_type": last_text(s_el, "icon") or "",
                    # field9 的客户端原语：`<kind>` 4 值闭集（icon 只是显示令牌，词表会漏项——
                    # atgm_heat 即此例）；两者实测 9 国 2093 发零矛盾
                    "shell_type_id": KIND_ID.get((last_text(s_el, "kind") or "").strip()),
                    "damage": 0.0,
                    "penetration": float(pp[0]) if pp else 0.0,
                    "module_damage": 0.0,
                    "velocity": velocity,
                    "range": range_,
                    "penetration_far": float(pp[1]) if len(pp) > 1 else 0.0,
                    "caliber": ffloat(s_el, "caliber"),
                    "normalization": ffloat(s_el, "normalizationAngle"),
                    "ricochet": ffloat(s_el, "ricochetAngle"),
                    "explosion_radius": ffloat(s_el, "explosionRadius"),
                    "_damage_armor": last_text(last_child(s_el, "damage"), "armor") if s_el is not None else None,
                    "_damage_devices": last_text(last_child(s_el, "damage"), "devices") if s_el is not None else None,
                })
        for s in shells:
            s["damage"] = float(s.pop("_damage_armor") or 0)
            s["module_damage"] = float(s.pop("_damage_devices") or 0)
        # 不排序：pb 的 shells/guns 顺序来自 BlitzKit 自身管线（与其 XML 文档序、id 序都不同），
        # 属非规范差异；对照器的键配对已消除顺序影响

        return {
            "module_id": mid(nation, comp.local_id("guns", g_el.tag) or 0),
            "name": self.disp(nation, gr) or g_el.tag,
            "caliber_factor": gr.floats("rotationSpeed"),  # pb"口径系数"实为炮 rotationSpeed
            "shell_count": gr.ints("level"),  # pb field9 = 炮的 <level>（"shell_count"是 Rust 侧误名；maxAmmo 在 pb field18，Rust 不读）
            "dispersion": gr.floats("shotDispersionRadius"),
            "aim_time": gr.floats("aimingTime"),
            "shells": shells,
            "reload": reload,
        }

    # -- models.json（TankModelInfo 形状）-------------------------------------
    def extract_model(self, nation: str, stem: str) -> dict:
        comp = self.components(nation)
        veh_path = self.veh_dir / nation / f"{stem}.xml.dvpl"
        root = ET.fromstring(decode_dvpl(veh_path.read_bytes()))
        hull = last_child(root, "hull")

        import yaml as pyyaml
        ypath = self.params_dir / nation / f"{stem}.yaml.dvpl"
        ydata = {}
        if ypath.exists():
            ydata = pyyaml.safe_load(decode_dvpl(ypath.read_bytes()).decode("utf-8", "replace")) or {}
        collision = ydata.get("collision") or {}
        mask_slice = ydata.get("maskSlice") or {}

        def bbox_of(key: str) -> dict | None:
            el = collision.get(key)
            if not isinstance(el, dict):
                return None
            bb = el.get("bbox")
            if not isinstance(bb, dict):
                return None
            mn, mx = bb.get("min"), bb.get("max")
            if not (isinstance(mn, list) and isinstance(mx, list)):
                return None
            # 注意：全 0 bbox **不**省略——实测 43 个炮塔的 pb 带全零 bbox
            return {"min": [float(x) for x in mn], "max": [float(x) for x in mx]}

        hull_plates, hull_spaced = parse_armor(last_child(hull, "armor"))
        turret_origin = parse_vec_text(last_text(last_child(hull, "turretPositions"), "turret"))

        turrets: list[dict] = []
        t0 = last_child(root, "turrets0")
        if t0 is not None:
            for t_el in t0:
                tr = DefResolver(t_el, comp.shared_def("turrets", t_el.tag))
                node = _node_from_model_path(tr, "Turret")
                guns_el = tr.elem("guns")
                guns: list[dict] = []
                if guns_el is not None:
                    for g_el in guns_el:
                        gr = DefResolver(g_el, comp.shared_def("guns", g_el.tag))
                        gnode = _node_from_model_path(gr, "Gun")
                        armor = gr.elem("armor")
                        gplates, gspaced = parse_armor(armor)
                        thick_el = last_child(armor, "gun") if armor is not None else None
                        thickness = float((thick_el.text or "0").strip()) if thick_el is not None else None
                        if thickness == 0.0:
                            thickness = None  # 0 厚度省略
                        ms = mask_slice.get(f"gun_{gnode:02d}")
                        mask = None
                        mask_slice_rec = None
                        if isinstance(ms, dict) and ms.get("enabled"):
                            pp = ms.get("planePosition")
                            if isinstance(pp, list) and len(pp) >= 2:
                                mask = float(pp[1])
                            # 完整客户端 maskSlice（viewer 裁切面按原值重建）：position/normal
                            # 必须完整，缺任一即不生成记录（fail-closed，不猜）。
                            pn = ms.get("planeNormal")
                            pa = ms.get("planeAxis")
                            if (isinstance(pp, list) and len(pp) >= 3
                                    and isinstance(pn, list) and len(pn) >= 3
                                    and isinstance(pa, list) and len(pa) >= 3):
                                mask_slice_rec = {
                                    "node": gnode,
                                    "position": [float(pp[0]), float(pp[1]), float(pp[2])],
                                    "normal": [float(pn[0]), float(pn[1]), float(pn[2])],
                                    "axis": [float(pa[0]), float(pa[1]), float(pa[2])],
                                    "plane_slice_mode": int(ms.get("planeSliceMode", 0)),
                                    "frustum_cone_slice_mode": int(ms.get("frustumConeSliceMode", 0)),
                                    "cone_base_radius": float(ms.get("frustumConeBaseRadius", 0.0)),
                                    "cone_angle_deg": float(ms.get("frustumConeAngleDegrees", 0.0)),
                                }
                        yl_text = tr.text("yawLimits")
                        yaw = None
                        if yl_text:
                            v = [float(x) for x in yl_text.split()]
                            if len(v) == 2 and not (v[0] == -180 and v[1] == 180):
                                yaw = {"min": v[0], "max": v[1]}
                        tlocal = comp.local_id("turrets", t_el.tag)
                        glocal = comp.local_id("guns", g_el.tag)
                        guns.append({
                            "gun_module_id": mid(nation, glocal) if glocal is not None else 0,
                            "model_node": gnode,
                            "thickness": thickness,
                            "mask": mask,
                            "mask_slice": mask_slice_rec,
                            "gun_spaced": gspaced,
                            "gun_plates": gplates,
                            "pitch_limits": parse_pitch(gr),
                        })
                turrets.append({
                    "module_id": mid(nation, tlocal) if tlocal is not None else 0,
                    "model_node": node,
                    "turret_spaced": parse_armor(tr.elem("armor"))[1],
                    "turret_plates": parse_armor(tr.elem("armor"))[0],
                    "bbox": bbox_of(f"turret_{node:02d}"),
                    "gun_origin": parse_vec_text(tr.text("gunPosition")),
                    "yaw_limits": yaw,
                    "guns": guns,
                })

        # 履带：逐底盘 leftTrack 厚度 + hullPosition 原点（C1 塌缩修复：
        # key = 底盘 module_id，多底盘车逐条保留，不再取第一条；origin 缺失
        # 保 None——pb 侧全 0 序列化为空消息，两边同形）
        tracks_model: dict = {}
        c_el = last_child(root, "chassis")
        if c_el is not None:
            for entry in c_el:
                cr = DefResolver(entry, comp.shared_def("chassis", entry.tag))
                armor = cr.elem("armor")
                lt = last_text(armor, "leftTrack") if armor is not None else None
                td: dict = {}
                if lt:
                    td["thickness"] = float(lt)
                hp_o = parse_vec_text(cr.text("hullPosition"))
                if hp_o and any(abs(v) > 1e-6 for v in hp_o):
                    td["origin"] = hp_o  # 全 0 省略（pb 侧零值序列化为空消息，两边同形）
                tracks_model[str(mid(nation, comp.local_id("chassis", entry.tag) or 0))] = td

        # 炮塔初始姿态：车辆 XML `<turretInitialRotation><yaw>/<pitch>/<roll>`（度）。
        # 2026-10-10 实测：与 BlitzKit models.pb field3 逐值一致（4 辆意系 AT-SPG 非零、其余
        # 无元素 = BK 的 None）；此前误判为"在 .sc2 节点变换内"——.sc2 的 TransformComponent
        # 全是单位阵，真源就是这一节。
        initial_rot = None
        tir = last_child(hull, "turretInitialRotation")
        if tir is not None:
            vals = {k: float((last_text(tir, k) or "0").strip() or 0) for k in ("yaw", "pitch", "roll")}
            if any(abs(v) > 1e-6 for v in vals.values()):
                initial_rot = vals

        return {
            "tank_id": 0,
            "hull_spaced": hull_spaced,
            "hull_plates": hull_plates,
            "hull_bbox": bbox_of("hull"),
            "chassis_bbox": bbox_of("chassis"),
            "tracks": tracks_model,
            "turret_origin": turret_origin,
            "initial_turret_rotation": initial_rot,
            "turrets": turrets,
        }


def user_key_text(list_el: ET.Element | None, tag: str) -> str | None:
    """userString 的**原文键**（保留段前缀，去前导 `#`）：`gb_vehicles:_47mm_3pdrAP`。

    段前缀与 pb 的国家名不一致（uk 车/弹常用 `gb_vehicles`），而裸键跨系会撞
    （`_47mm_3pdrAP` 在 usa 段的值是字面 "None"）——名字解析必须按原文键查。
    """
    t = last_text(list_el, tag) if list_el is not None else None
    if not t:
        return None
    return t[1:] if t.startswith("#") else t


def _key_of(list_el: ET.Element | None, tag: str) -> str | None:
    t = last_text(list_el, tag) if list_el is not None else None
    return t.split(":", 1)[1] if t and ":" in t else t


def _node_from_model_path(r: DefResolver, kind: str) -> int:
    """`models.undamaged` 路径里的 `Turret_02.model` / `Gun_04.model` → 2 / 4。"""
    models = r.elem("models")
    p = last_text(models, "undamaged") if models is not None else None
    if not p:
        return 0
    m = re.search(rf"{kind}_(\d+)\.model", p, re.IGNORECASE)
    return int(m.group(1)) if m else 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def _write_status(path: pathlib.Path, state: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    state["updated_at"] = time.strftime("%H:%M:%S")
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.replace(path)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game-data", type=pathlib.Path, default=None)
    ap.add_argument("--pb", type=pathlib.Path, default=REPO_ROOT / "data" / "tanks.pb")
    ap.add_argument("--bridge", type=pathlib.Path, default=REPO_ROOT / "data" / "tank_id_bridge.json")
    ap.add_argument("--out", type=pathlib.Path, default=pathlib.Path("data/cache/local_pb"))
    ap.add_argument("--tank", action="append", default=[])
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--all-local", action="store_true",
                    help="连桥外车辆（开发/超测/未实装）一起提取，tank_id 记负数序号")
    ap.add_argument("--emit-bridge", action="store_true",
                    help="导出 tank_id 桥接表（默认从 data/tanks.pb，加 --from-client 则纯客户端）")
    ap.add_argument("--from-client", action="store_true",
                    help="--emit-bridge 时改从客户端 list.xml 生成（不读 pb；slug 留空）")
    args = ap.parse_args()

    game_data = args.game_data or default_game_data()
    if args.emit_bridge:
        if args.from_client:
            emit_bridge_from_client(args.game_data or default_game_data(), args.bridge)
        else:
            emit_bridge(args.pb, args.bridge)
        return 0

    if not args.bridge.exists():
        print(f"!! 桥接表不存在: {args.bridge}（先跑 --emit-bridge）", file=sys.stderr)
        return 2
    bridge = json.loads(args.bridge.read_text(encoding="utf-8"))

    ex = Extractor(game_data)
    # 磁盘上的全部车辆 stem（按国家）
    disk: dict[str, list[str]] = {}
    for nation in NATION_LOW:
        d = ex.veh_dir / nation
        if d.is_dir():
            disk[nation] = sorted(p.stem.replace(".xml", "") for p in d.glob("*.xml.dvpl")
                                  if p.name != "list.xml.dvpl")

    targets: list[tuple[int, str, str]] = []
    if args.all or args.tank:
        want = {int(x) for x in args.tank} if args.tank else None
        for tid_s, info in sorted(bridge.items(), key=lambda kv: int(kv[0])):
            if want is not None and int(tid_s) not in want:
                continue
            targets.append((int(tid_s), info["nation"], info["stem"]))
    else:
        print("!! 需要 --tank <id> 或 --all", file=sys.stderr)
        return 2

    extra_by_stem: dict[tuple[str, str], int] = {}
    if args.all_local:
        bridged = {(info["nation"], info["stem"]) for info in bridge.values()}
        seq = -1
        for nation in NATION_LOW:
            for stem in disk.get(nation, []):
                if (nation, stem) not in bridged:
                    extra_by_stem[(nation, stem)] = seq
                    seq -= 1

    args.out.mkdir(parents=True, exist_ok=True)
    tanks_out: list[dict] = []
    models_out: list[dict] = []
    extra_out: list[dict] = []
    failures: list[dict] = []
    started = time.time()
    for i, (tid, nation, stem) in enumerate(targets):
        try:
            tk = ex.extract_tank(nation, stem)
            tk["tank_id"] = tid
            md = ex.extract_model(nation, stem)
            md["tank_id"] = tid
            tanks_out.append(tk)
            models_out.append(md)
        except Exception as e:  # noqa: BLE001
            failures.append({"tank_id": tid, "nation": nation, "stem": stem,
                             "error": f"{type(e).__name__}: {e}"})
        if (i + 1) % 100 == 0:
            print(f"  {i + 1}/{len(targets)}", flush=True)

    for (nation, stem), seq in sorted(extra_by_stem.items(), key=lambda kv: kv[1]):
        try:
            tk = ex.extract_tank(nation, stem)
            tk["tank_id"] = seq
            md = ex.extract_model(nation, stem)
            md["tank_id"] = seq
            tanks_out.append(tk)
            models_out.append(md)
            extra_out.append({"seq": seq, "nation": nation, "stem": stem})
        except Exception as e:  # noqa: BLE001
            failures.append({"tank_id": seq, "nation": nation, "stem": stem,
                             "error": f"{type(e).__name__}: {e}"})

    (args.out / "tanks.json").write_text(
        json.dumps(tanks_out, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    (args.out / "models.json").write_text(
        json.dumps(models_out, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    (args.out / "local_extra.json").write_text(
        json.dumps({"count": len(extra_out), "vehicles": extra_out}, ensure_ascii=False, indent=1),
        encoding="utf-8")
    if failures:
        (args.out / "failures.json").write_text(
            json.dumps(failures, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"完成：提取 {len(tanks_out)} 辆（桥外 {len(extra_out)}），失败 {len(failures)}，"
          f"耗时 {round(time.time() - started, 1)}s → {args.out}")
    for f in failures[:10]:
        print(f"   [fail] {f['tank_id']} {f['stem']}: {f['error']}")
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
