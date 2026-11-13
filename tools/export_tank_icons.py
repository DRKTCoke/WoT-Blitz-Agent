#!/usr/bin/env python3
"""用本机 WoT Blitz 客户端导出坦克封面图/图标（替代 BlitzKit `tanks/{id}/icons/big.webp`）。

**不替换运行期数据源**：产物写到 `data/cache/local_tank_icons/<tank_id>.webp`，与 BlitzKit 的
`data/cache/tank_images/<tank_id>.webp` 并存，可用 `tools/compare_tank_icons.py` 或肉眼对照。

客户端来源（实测路径与形态）：

  * `Data/Gfx/UI/BigTankIcons/<name>.packed.webp.dvpl`（2312 个文件，含 `@2x` 与 `_skinN` 变体）
    —— DVPL 壳里就是**裸 webp**（RIFF/WEBP），**剥壳即得，无需重编码**。
    分辨率：基础 256×128 RGBA / `@2x` 512×256。
  * `Data/Gfx/UI/BattleScreenHUD/SmallTankIcons/<name>.packed.webp.dvpl`（1474 个）128×32，作兜底。

**匹配策略：客户端声明源优先、精确等值——不做部分串匹配**（2026-10-10 重写）：
按下列**有序**候选查名（全部来自客户端自己的声明）：

  1. `iconPath` —— **逐车参数文件** `3d/Tanks/Parameters/<nation>/<stem>.yaml.dvpl` 的
     `bigIconPath: "~res:/Gfx/UI/BigTankIcons/<名>"`（755 份逐车文件全含）：按**整文件名
     （含国家标签）精确解析**——`ussr-IS_2` 必须命中苏联图标、不得归一化撞上 `china-IS2`；
     声明名带资源变体后缀（实测唯一一例：`ussr-KV_1s_BP.china`）而磁盘缺该名时，去尾后缀
     再精确一次；
  2. 备选链路（声明缺失时才出场）：`registry` —— `camouflages.yaml` 皮肤注册表的
     `previewWith: "<nation>:<stem>"` + `iconBig` 配对 → `short`/`full` —— `list.xml` 的
     `shortUserString`/`userString` 字面键取 `Strings` ∪ 运行时本地化覆盖层 → `stem` 及去
     `XxNN_`/`Xy_`/`WH_` 前缀变体 → 人工别名表。弱来源带**唯一性护栏**（被 >1 辆车声明的
     键弃用——`T34_hvy` 的全名 "T34" 会撞他车）。

覆盖实测（2026-10-10）：**735/735，全部由 `iconPath` 命中**（`match_src` 逐车可审）。
选择优先级（同一键内）：**大图基础 > 大图皮肤变体 > 小图标**；`_export_status.json` 的
`grade`（big / big-skin / small）与 `match_src` 逐条可查。

用法：
    python tools/export_tank_icons.py --all                    # 全量 735 辆（大图基础档）
    python tools/export_tank_icons.py --all --use-2x           # 用 @2x（512×256）
    python tools/export_tank_icons.py --tank 9489 --tank 7169
    python tools/export_tank_icons.py --all --allow-small      # 允许退到 SmallTankIcons（128×32）
    python tools/export_tank_icons.py --list                   # 只打印解析结果（不写盘）

进度：原子更新 `<out>/_export_status.json`（total/done/missing/finished/results），与其他导出器同约定。
"""

from __future__ import annotations

import argparse
import concurrent.futures
import glob
import json
import os
import pathlib
import re
import sys
import time

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

# 图标文件名里的国家标签（实测出现的全集；`britsh` 是客户端数据里的错拼，须一并接受）
ICON_TAGS = ("germany", "ussr", "usa", "british", "britsh", "china", "japan", "france",
             "european", "other")
# list.xml 所在目录名（= pb 的 nation 值；图标文件名里的标签另有一套，见 ICON_TAGS）
NATION_DIRS = ("ussr", "germany", "usa", "china", "france", "uk", "japan", "other", "european")
# 模型名的子部件前缀：`Ch01_Type59` / `GB10_Black_Prince` / `Oth08_WH_Vindicator` / `S04_Lago-I`
STEM_PREFIX_RE = re.compile(r"^[A-Za-z]+[0-9]+_")
# 少数车的前缀不带数字（`Ch_WZ-112v2` → 图标 `china-WZ-112v2`、`GB_Mark_I` → `britsh-Mark_I`）
STEM_PREFIX_NODIGIT_RE = re.compile(r"^[A-Za-z]{1,3}_")
# 少数车多带一层无意义前缀（实测 `Oth08_WH_Vindicator` → 图标 `other-Oth08_Vindicator`）
EXTRA_PREFIX_RE = re.compile(r"^WH_")
NOISE_INFIX = ("WH_",)

# 字符串条目（转义感知的值捕获）
_STRING_ENTRY_RE = re.compile(
    r'"#?([A-Za-z0-9_\-]+_vehicles:[A-Za-z0-9_.\-]+)"\s*:\s*"((?:[^"\\]|\\.)*)"')
# YAML 双引号串转义（客户端文件真实出现：`\xe4`→ä、`\u00A0`→NBSP、`\"`）
_ESC_SUB_RE = re.compile(r"\\(x[0-9a-fA-F]{2}|u[0-9a-fA-F]{4}|U[0-9a-fA-F]{8}|n|t|\"|\\|/)")

# 人工核对过的别名：模型名（tanks.pb field32）→ 客户端图标**文件名**。
#
# 客户端图标名有时用内部代号/缩写/另一套拼法，声明源拼不出来（`R63_ST_IBD`、`Renault_D1`…）。
# 这张表**只收实测确认是同一辆车的**，宁可缺失也不猜——错挂别的车的图标比缺图更糟。
# 每条依据写在行尾，便于复核。2026-10-10 接入逐车 iconPath 后 735/735 全部由声明源命中，
# 本表休眠为兜底（仅在声明源缺失的车上才会出场）。
ICON_ALIAS = {
    # uk：客户端用 Medium_N / Cruiser_N / Churchill_GC 等短名，模型名走 GB 编号
    "GB01_Medium_Mark_I": "britsh-Medium_I.packed.webp.dvpl",
    "GB05_Vickers_Medium_Mk_II": "britsh-Medium_II.packed.webp.dvpl",
    "GB06_Vickers_Medium_Mk_III": "britsh-Medium_III.packed.webp.dvpl",
    "GB58_Cruiser_Mk_III": "britsh-Cruiser_III.packed.webp.dvpl",
    "GB59_Cruiser_Mk_IV": "britsh-Cruiser_IV.packed.webp.dvpl",
    "GB40_Gun_Carrier_Churchill": "britsh-Churchill_GC.packed.webp.dvpl",     # GC = Gun Carrier
    "GB68_Matilda_Black_Prince": "britsh-MatildaBP.packed.webp.dvpl",         # BP = Black Prince
    "GB24_Centurion_Mk3": "britsh-Centurion_7-1.packed.webp.dvpl",            # 显示名即 Centurion Mk. 7/1
    "GB116_Harry_Hopkins": "british-GB116_Harry_Hopkins_I.packed.webp.dvpl",  # 仅多尾部 "I"
    # usa
    "T1_Cunningham": "usa-T1.packed.webp.dvpl",
    "M2_med": "usa-M2_MT.packed.webp.dvpl",                                   # MT = Medium Tank
    "T2_med": "usa-T2_MT.packed.webp.dvpl",
    "T7_Combat_Car": "usa-T7-cc.packed.webp.dvpl",                            # cc = combat car
    # ussr
    "ST_I": "ussr-ST-1.packed.webp.dvpl",       # 声明源即 ussr-ST-1；曾误挂 ussr-R63_ST_IBD
                                                # （那是 T-2020 / 20993 的档），2026-10-10 审计纠正
    "R132_T100LT": "ussr-R132_VNII_100LT.packed.webp.dvpl",
    "R116_ISU122C_Berlin": "ussr-ISU122_Berlin.packed.webp.dvpl",
    # germany
    "E50_Ausf_M": "germany-E-50M.packed.webp.dvpl",
    "Pro_Ag_A": "germany-Leopard_PT_A.packed.webp.dvpl",                      # Leopard Prototyp A
    "G114_Rheinmetall_Scorpion": "germany-Skorpion.packed.webp.dvpl",
    "G_VK3502_BP": "germany-G_VK3502H_BP.packed.webp.dvpl",
    "H39_captured": "germany-PzKpfw38H_735.packed.webp.dvpl",                 # 显示名 Pz.Kpfw. 38H 735 (f)
    "JagdTiger_SdKfz_185": "germany-JagdTiger8_8.packed.webp.dvpl",           # 8,8 cm Pak 43 Jagdtiger
    "JagdTiger_SdKfz_185_Snowstorm": "germany-JagdTiger8_8_Snowstorm.packed.webp.dvpl",
    "S01_Frankentank": "Frankentank_event.packed.webp.dvpl",                  # 万圣节 Tankenstein；文件名无国家标签
    # france
    "D1": "france-Renault_D1.packed.webp.dvpl",
    "F73_M4A1_Revalorise": "france-Revalorise.packed.webp.dvpl",
    "AMX_M4_1945": "france-AMX_M4-45.packed.webp.dvpl",                       # AMX M4 mle. 45
    "F68_AMX_Chasseur_de_char_46": "france-CDC.packed.webp.dvpl",             # CDC = Chasseur de chars
    "Ch23_112": "china-112_event.packed.webp.dvpl",                           # 显示名 "112 Glacial" = 活动版 112
    "PzVI_GuP": "japan-Tiger_I_GuP.packed.webp.dvpl",                         # PzVI = Tiger I；GuP 联动车都在 japan 标签下
}


def default_game_data() -> pathlib.Path:
    for c in GAME_DIR_CANDIDATES:
        p = pathlib.Path(c)
        if p.is_dir():
            return p
    return pathlib.Path(GAME_DIR_CANDIDATES[0])


def _unescape_yaml(s: str) -> str:
    """YAML 双引号串转义解码（`\\xe4`/`\\u00A0`/`\\"` 等在客户端文件里真实出现）。"""
    def sub(m):
        e = m.group(1)
        if e[0] in "xuU":
            try:
                return chr(int(e[1:], 16))
            except ValueError:
                return m.group(0)
        return {"n": "\n", "t": "\t", '"': '"', "\\": "\\", "/": "/"}.get(e, e)
    return _ESC_SUB_RE.sub(sub, s)


def strip_tag(name: str) -> str:
    """剥掉文件名开头的国家标签（`germany-`/`britsh-`/…），保留其余原文。"""
    low = name.lower()
    for tag in ICON_TAGS:
        for sep in ("-", "_"):
            if low.startswith(tag + sep):
                return name[len(tag) + 1:]
    return name


def normalize(name: str) -> str:
    """归一化比对键：去掉国家标签 → 只留小写字母数字（两侧同一套，命中即整键相等）。"""
    return re.sub(r"[^a-z0-9]", "", strip_tag(name).lower())


def icon_key(filename: str) -> str:
    """图标文件名 → 归一化键（剥掉 `@2x` 与 `.packed.webp.dvpl`）。"""
    n = filename.replace("@2x", "")
    if n.endswith(".packed.webp.dvpl"):
        n = n[: -len(".packed.webp.dvpl")]
    return normalize(n)


def base_name(filename: str) -> str:
    """图标文件 → 整文件名（去 `@2x` 与 `.packed.webp.dvpl`，含国家标签，原样大小写）。"""
    return filename.replace("@2x", "")[: -len(".packed.webp.dvpl")]


def _strip_extra(name: str) -> str:
    """剥离皮肤后缀与内部编号前缀，得到"纯车型名"（索引侧对称归一化用）。

    `germany-G88_Indien_Panzer_skin` → `Indien_Panzer`、`ussr-IS-4_skin` → `IS-4`。
    """
    n = re.sub(r"_skin\d*", "", name)
    n = strip_tag(n)              # 顺序要紧：先剥国家标签，内部编号才会落到串首被剥掉
    for rx in (STEM_PREFIX_RE, STEM_PREFIX_NODIGIT_RE):
        m = rx.match(n)
        if m:
            rest = n[m.end():]
            # 剥完至少还要剩两个词：`G88_Indien_Panzer` 可剥；`T34_hvy` → 只剩 `hvy`
            # （会被 T1_hvy 的候选键撞上，误挂 T34 Heavy 的图），不剥
            if len(re.findall(r"[A-Za-z0-9]+", rest)) >= 2:
                n = rx.sub("", n)
                break
    return EXTRA_PREFIX_RE.sub("", n)


def build_index(ui_dir: pathlib.Path) -> dict:
    """→ {"key": {归一化键: [条目]}, "name": {整文件名(去扩展/@2x): [条目]}}。

    * `key`：归一化键（大小写/下划线/国家标签差异全部吸收）——用于弱来源（stem/短名等）；
    * `name`：**整文件名精确表（含国家标签）**——用于 `iconPath` 这类客户端逐车声明：
      `~res:/Gfx/UI/BigTankIcons/ussr-IS_2` 必须命中 `ussr-IS_2.packed.webp.dvpl`，不能
      归一化后撞上同键的 `china-IS2`（跨车误挂，2026-10-10 实测）。

    每个文件名登记**精确键**与**派生键**（剥皮肤/内部编号）；派生键若与别车图标的精确键
    撞键且非同车则丢弃（防把 A 车图标挂到 B 车）。
    """
    index: dict = {}
    by_name: dict = {}
    derived: dict = {}
    for sub in ("BigTankIcons", "BattleScreenHUD/SmallTankIcons"):
        d = ui_dir / sub
        if not d.is_dir():
            continue
        for p in d.iterdir():
            n = p.name
            if not n.endswith(".packed.webp.dvpl"):
                continue
            key = icon_key(n)
            if not key:
                continue
            ent = {"file": n, "sub": sub, "x2": "@2x" in n,
                   "skin": bool(re.search(r"_skin\d*(@2x)?\.packed", n, re.I))}
            index.setdefault(key, []).append({**ent, "derived": False})
            by_name.setdefault(base_name(n), []).append({**ent, "derived": False})
            base = n.replace("@2x", "")[: -len(".packed.webp.dvpl")]
            dk = normalize(_strip_extra(base))
            if dk and dk != key:
                derived.setdefault(dk, []).append(ent)

    def same_vehicle(f: str) -> str:
        return normalize(_strip_extra(f.replace("@2x", "")[: -len(".packed.webp.dvpl")]))

    for dk, ents in derived.items():
        if dk in index and not all(same_vehicle(e["file"]) == dk for e in index[dk]):
            continue     # 撞上别车的精确键 → 丢弃，宁缺勿错
        index.setdefault(dk, []).extend({**e, "derived": True} for e in ents)
    return {"key": index, "name": by_name}


def read_tank_table(pb_path: pathlib.Path) -> dict:
    """`data/tanks.pb` → {tank_id: {nation, stem}}（与 export_tank_glb 同口径，自包含）。"""
    import struct

    def varint(b: bytes, i: int):
        r = s = 0
        while True:
            x = b[i]
            i += 1
            r |= (x & 0x7F) << s
            if not x & 0x80:
                return r, i
            s += 7

    def fields(b: bytes):
        i, out = 0, []
        while i < len(b):
            key, i = varint(b, i)
            fn, wt = key >> 3, key & 7
            if wt == 0:
                v, i = varint(b, i)
                out.append((fn, wt, v))
            elif wt == 2:
                ln, i = varint(b, i)
                out.append((fn, wt, b[i:i + ln]))
                i += ln
            elif wt == 5:
                out.append((fn, wt, struct.unpack_from("<f", b, i)[0]))
                i += 4
            elif wt == 1:
                out.append((fn, wt, b[i:i + 8]))
                i += 8
            else:
                raise ValueError(f"wire {wt}")
        return out

    table = {}
    for fn, wt, v in fields(pb_path.read_bytes()):
        if fn != 1 or wt != 2:
            continue
        entry = fields(v)
        tid = next((x[2] for x in entry if x[0] == 1 and x[1] == 0), None)
        main = next((x[2] for x in entry if x[0] == 2 and x[1] == 2), None)
        if tid is None or main is None:
            continue
        nation = stem = ""
        for f2, w2, v2 in fields(main):
            if f2 == 11 and w2 == 2:
                nation = v2.decode("utf-8", "replace")
            elif f2 == 32 and w2 == 2:
                stem = v2.decode("utf-8", "replace")
        table[int(tid)] = {"nation": nation, "stem": stem}
    return table


def build_icon_context(game_data: pathlib.Path, lang: str) -> dict:
    """**声明源**上下文（全部来自客户端自己的声明；缺失时相应字段为空，不猜）：

    * `registry`：`camouflages.yaml` 皮肤注册表（块内 `iconBig` 在前、`previewWith` 在后，
      必须整块收集后配对）——客户端自己引用图标名的地方，含 BP/联动车；
    * `list_keys`：每国 `list.xml` 里车辆的 `shortUserString`/`userString` **字面键**（含段前缀）；
    * `strings`：`Data/Strings/<lang>.yaml` ∪ **运行时本地化覆盖层**（客户端从 CDN 下载并缓存在
      `%LOCALAPPDATA%/wotblitz/DAVAProject/cache/localizations/`；只收拉丁值，避免混入译名）。
    """
    ctx = {"declared": {}, "registry": {}, "list_keys": {}, "strings": {}}
    # **最高优先级：车辆自己的图标声明**——`3d/Tanks/Parameters/<nation>/<stem>.yaml.dvpl`
    # 的 `bigIconPath` / `smallIconPath`（客户端自己引用图标名的地方，逐车一份；
    # 实测 764 份参数文件中 755 份为逐车文件且全含此字段，另 9 份为国家级默认）
    params_dir = game_data / "3d" / "Tanks" / "Parameters"
    if params_dir.is_dir():
        for nat_dir in params_dir.iterdir():
            if not nat_dir.is_dir():
                continue
            for f in nat_dir.glob("*.yaml.dvpl"):
                try:
                    txt = decode_dvpl(f.read_bytes()).decode("utf-8", "replace")
                except Exception:
                    continue
                big = re.search(r"bigIconPath:\s*\"~res:/Gfx/UI/BigTankIcons/([^\"]+)\"", txt)
                small = re.search(r'smallIconPath:\s*"~res:/Gfx/UI/BattleScreenHUD/SmallTankIcons/([^"]+)"', txt)
                if big or small:
                    ctx["declared"][(nat_dir.name, f.name[: -len(".yaml.dvpl")])] = (
                        big.group(1) if big else None, small.group(1) if small else None)
    cam_path = game_data / "camouflages.yaml.dvpl"
    if cam_path.exists():
        cam = decode_dvpl(cam_path.read_bytes()).decode("utf-8", "replace")
        pv = icon = None
        for line in cam.splitlines() + ["END:"]:
            if line and not line[0].isspace():
                if pv and icon:
                    ctx["registry"][pv] = icon
                pv = icon = None
                continue
            m = re.match(r"\s+previewWith:\s*\"([a-z]+):([^\"]+)\"", line)
            if m:
                pv = (m.group(1), m.group(2).strip())
            m2 = re.match(r"\s+iconBig:\s*\"([^\"]+)\"", line)
            if m2:
                icon = m2.group(1).split("/")[-1].rsplit(".", 1)[0]
    for nation in NATION_DIRS:
        p = game_data / "XML/item_defs/vehicles" / nation / "list.xml.dvpl"
        if not p.exists():
            continue
        text = decode_dvpl(p.read_bytes()).decode("utf-8", "replace")
        cur = None
        for line in text.splitlines():
            m = re.match(r"\s*<([A-Za-z0-9_.\-]+)>$", line)
            if m:
                cur = m.group(1)
                continue
            m2 = re.match(r"\s*<(shortUserString|userString)>#?([^<]+)</", line)
            if m2 and cur:
                ctx["list_keys"].setdefault(cur, {})[m2.group(1)] = m2.group(2)
    string_sources = [game_data / "Strings" / f"{lang}.yaml.dvpl"]
    string_sources += [pathlib.Path(p) for p in sorted(glob.glob(os.path.expandvars(
        "%LOCALAPPDATA%/wotblitz/DAVAProject/cache/localizations/*.yaml")))]
    for p in string_sources:
        if not p.exists():
            continue
        try:
            txt = decode_dvpl(p.read_bytes()).decode("utf-8", "replace")
        except Exception:
            txt = p.read_text(encoding="utf-8", errors="replace")
        for m in _STRING_ENTRY_RE.finditer(txt):
            v = _unescape_yaml(m.group(2))
            # 覆盖层是客户端语言的译名：只收拉丁值（专名跨语言同形）
            if v and v != "None" and all(ord(c) < 0x2E80 or c in "·—–'’" for c in v):
                ctx["strings"].setdefault(m.group(1), v)
    return ctx


def ambiguous_keys(table: dict, ctx: dict) -> set:
    """被 >1 辆车**声明**的候选键（弱来源会因此跨车误挂：`T34_hvy` 的全名 "T34" → `t34`
    会撞上中式 T-34 的图标；`Ch08_Type97_Chi_Ha` 的短名 → `chiha` 会撞日本 Chi-Ha）。

    只对**弱来源**（short/full/stem_name/stem_noise）生效；`stem`/`stem_prefix` 是车辆
    自身的模型名，视为强声明（历史匹配已 40+ 辆验证）。
    """
    claims: dict = {}
    for tid, t in table.items():
        for _src, k in declared_candidates(t["stem"], t["nation"], ctx):
            claims.setdefault(k, set()).add(tid)   # 含强来源：他车的模型名同样构成"被声明"
    return {k for k, v in claims.items() if len(v) > 1}


def declared_candidates(stem: str, nation: str, ctx: dict, ambiguous: set | None = None) -> list:
    """声明源候选键（有序）：registry > short > full > stem > stem_prefix。

    每项都是 `(来源标签, 归一化键)`；命中即**整键相等**——不做部分串匹配。
    """
    out = []
    decl = ctx["declared"].get((nation, stem))
    if decl and decl[0]:
        out.append(("iconPath", decl[0]))      # 整文件名（含国家标签）——按精确表解析
        # 声明名可带资源变体后缀（实测唯一一例：`ussr-KV_1s_BP.china`，磁盘上是
        # `ussr-KV_1s_BP`）——变体名缺失时按**去尾后缀**精确回退，仍不做部分串匹配
        if "." in decl[0]:
            out.append(("iconPath", decl[0].rsplit(".", 1)[0]))
    icon = ctx["registry"].get((nation, stem))
    if icon:
        out.append(("registry", normalize(icon)))
    for tag, field in (("short", "shortUserString"), ("full", "userString")):
        k = (ctx["list_keys"].get(stem) or {}).get(field)
        if k:
            v = ctx["strings"].get(k)
            if v:
                out.append((tag, normalize(v)))
    out.append(("stem", normalize(stem)))
    for rx in (STEM_PREFIX_RE, STEM_PREFIX_NODIGIT_RE):
        short = rx.sub("", stem)
        if short != stem:
            out.append(("stem_prefix", normalize(short)))
    stripped = stem
    for noise in NOISE_INFIX:
        if noise in stripped:
            stripped = stripped.replace(noise, "")
    if stripped != stem:
        out.append(("stem_noise", normalize(stripped)))
    # stem + 显示名里多出的词（两段都是客户端声明名的**确定性拼接**，非模糊匹配）：
    # `M48A1` 的显示名 "M48A1 Patton" → 图标 `usa-M48A1_Patton`；
    # `GB81_FV4004` + "FV4004 Conway" → 图标 `british-GB81_FV4004_Conway`。
    disp = display_name(stem, ctx)
    extra = [t for t in re.split(r"[^A-Za-z0-9]+", disp or "")
             if t and normalize(t) not in normalize(stem)]
    if extra:
        out.append(("stem_name", normalize(stem + "_" + "_".join(extra))))
    seen, uniq = set(), []
    for src, k in out:
        if not k or k in seen:
            continue
        if ambiguous and k in ambiguous and src not in ("iconPath", "stem", "stem_prefix"):
            continue      # 弱来源的歧义键 → 弃用（宁缺勿错）
        seen.add(k)
        uniq.append((src, k))
    return uniq


def display_name(stem: str, ctx: dict) -> str:
    """车辆的显示名（list.xml 短名优先；供 status/审计显示，也用于启发式尾档）。"""
    keys = ctx["list_keys"].get(stem) or {}
    for field in ("shortUserString", "userString"):
        v = ctx["strings"].get(keys.get(field, ""))
        if v:
            return v
    return ""


def pick_icon(index: dict, stem: str, display: str, use_2x: bool, allow_small: bool,
              declared: list | None = None) -> dict | None:
    """按优先级挑一张：**声明源候选**（精确整键）→ 手工别名表 → 启发式（stem 变体）。

    同一键内排序：大图档位优先（128×32 小图只作兜底）→ 精确键优于派生键 → 基础档优于皮肤
    变体 → 与请求档位（1x/@2x）越接近越好。
    """
    by_name = index.get("name") or {}
    # 声明源：**全部候选都收集**（同一辆车可能同时命中"基础档"与"皮肤档"——注册表条目多为
    # 皮肤），随后按统一排序（非皮肤优先）挑，`match_src` 记录**获胜项**的来源。
    key_index = index.get("key") or index
    got = []      # [(entry, 来源标签)]
    for src, k in (declared or []):
        table = by_name if src == "iconPath" else key_index
        got += [(e, src) for e in table.get(k, [])]
    if stem in ICON_ALIAS:
        # 别名**始终参与竞争**（手核表：确认为同车的基础档图标）——声明源可能只命中共车的
        # 皮肤派生键或小图标（19985 / 64561 实测），此时别名的基础档必须能赢。
        got += [(e, "alias") for e in key_index.get(icon_key(ICON_ALIAS[stem]), [])]
    if not got:
        for k in candidate_keys(stem):
            got += [(e, "heuristic") for e in key_index.get(k, [])]
        if not allow_small and not any(e[0]["sub"] == "BigTankIcons" for e in got):
            got = []
    if not got:
        return None
    prio = {"BigTankIcons": 0, "BattleScreenHUD/SmallTankIcons": 1}
    want_x2 = 1 if use_2x else 0
    # 来源优先级：**基础名（stem/short/full）> 去前缀短名 > 注册表风格档 > 别名/启发式**。
    # 注册表条目多为皮肤/风格（`Maus_Skin`），只有基础图不存在时才应中选。
    src_rank = {"iconPath": 0, "stem": 1, "short": 1, "full": 1, "stem_prefix": 2,
                "stem_name": 3, "stem_noise": 4, "alias": 5, "registry": 6, "heuristic": 7}
    ranked = sorted(
        got,
        key=lambda p: (prio[p[0]["sub"]] if allow_small else 0,   # ① 大图档位
                       int(bool(p[0].get("derived"))),            # ② 精确键优于皮肤/编号派生键
                       int(p[0]["skin"]),                         # ③ 基础档优于皮肤档
                       src_rank.get(p[1], 9),                     # ④ 来源：基础名 > 注册表风格
                       abs(int(p[0]["x2"]) - want_x2)),
    )
    if not allow_small:
        big = [p for p in ranked if p[0]["sub"] == "BigTankIcons"]
        if not big:
            return None
        ranked = big
    out = dict(ranked[0][0])
    out["match_src"] = ranked[0][1]
    return out


def candidate_keys(stem: str) -> list:
    """启发式尾档（**仅在声明源与别名表都未命中时**使用）：模型名与去前缀短名的归一化键。"""
    outs = [stem]
    for rx in (STEM_PREFIX_RE, STEM_PREFIX_NODIGIT_RE):
        short = rx.sub("", stem)
        if short != stem:
            outs.append(short)
    for s in list(outs):
        s2 = EXTRA_PREFIX_RE.sub("", s)
        if s2 != s:
            outs.append(s2)
        for noise in NOISE_INFIX:
            if noise in s:
                outs.append(s.replace(noise, ""))
    seen, keys = set(), []
    for s in outs:
        k = normalize(s)
        if k and k not in seen:
            seen.add(k)
            keys.append(k)
    return keys


def _export_one(payload: tuple) -> dict:
    tid, stem, display, declared, game_data, out_root, use_2x, allow_small, index = payload
    ent = pick_icon(index, stem, display, use_2x, allow_small, declared)
    if ent is None:
        return {"tank_id": tid, "stem": stem, "status": "no-icon"}
    src_path = pathlib.Path(game_data) / "Gfx/UI" / ent["sub"] / ent["file"]
    if not src_path.exists():
        return {"tank_id": tid, "stem": stem, "status": "file-missing", "src": str(src_path)}
    data = decode_dvpl(src_path.read_bytes())
    if data[:4] != b"RIFF" or data[8:12] != b"WEBP":
        return {"tank_id": tid, "stem": stem, "status": "not-webp", "src": str(src_path)}
    out = pathlib.Path(out_root)
    out.mkdir(parents=True, exist_ok=True)
    (out / f"{tid}.webp").write_bytes(data)
    grade = ("small" if ent["sub"].endswith("SmallTankIcons")
             else "big-skin" if ent["skin"] else "big")
    return {"tank_id": tid, "stem": stem, "status": "ok", "src": ent["file"],
            "tier": "2x" if ent["x2"] else "1x", "small": ent["sub"].endswith("SmallTankIcons"),
            "skin": ent["skin"], "grade": grade, "match_src": ent.get("match_src"),
            "bytes": len(data)}


def _write_status(path: pathlib.Path, state: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)   # 首次写入时输出目录可能尚未创建
    state["updated_at"] = time.strftime("%H:%M:%S")
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(state, ensure_ascii=False, indent=1), encoding="utf-8")
    tmp.replace(path)


def emit_crops(out_dir: pathlib.Path, crops_out: pathlib.Path) -> int:
    """按内容边界（alpha>0 紧裁矩形）计算逐车**展示框**并落盘（版本化 JSON）。

    用途：客户端图标是 256×128（或 128×128）原始画布、坦克斜向绘制且锚定在左上、
    右下留白约一半——直接 contain 展示会比以前（BlitzKit 逐车紧裁版）显得小。
    本数据供消费方在**展示层**按框缩放定位（不改图标字节），视觉对齐旧紧裁口径。
    与 `out_dir` 内图标 **1:1 对应**（打包器按数量校验，fail-closed）；图标重导后
    必须重跑本模式，否则展示框与字节脱版。
    """
    from PIL import Image

    icons: dict[str, dict] = {}
    blank: list[str] = []
    for p in sorted(out_dir.glob("*.webp")):
        if not p.stem.isdigit():
            continue
        with Image.open(p) as im:
            canvas_w, canvas_h = im.size
            # Opaque WebP decodes as RGB; every canvas pixel belongs to its content box.
            bbox = im.getchannel("A").getbbox() if "A" in im.getbands() else (0, 0, canvas_w, canvas_h)
        if bbox is None:
            blank.append(p.stem)
            continue
        l, t, r, b = bbox
        icons[p.stem] = {
            "canvas_w": canvas_w,
            "canvas_h": canvas_h,
            "x": l,
            "y": t,
            "w": r - l,
            "h": b - t,
        }
    if blank:
        print(f"!! 全透明图标无法定展示框: {blank[:10]}（共 {len(blank)}）", file=sys.stderr)
        return 2
    doc = {
        "version": 1,
        "note": "逐车封面图内容框（alpha>0 紧裁，canvas 像素坐标）：展示层据此缩放定位；"
                "与同目录图标 1:1，图标重导后须重跑 --emit-crops",
        "icons": icons,
    }
    crops_out.parent.mkdir(parents=True, exist_ok=True)
    tmp = crops_out.with_suffix(".json.tmp")
    tmp.write_text(
        json.dumps(doc, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
        encoding="utf-8")
    tmp.replace(crops_out)
    print(f"内容框 {len(icons)} 辆 → {crops_out}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game-data", type=pathlib.Path, default=None)
    ap.add_argument("--pb", type=pathlib.Path, default=REPO_ROOT / "data" / "tanks.pb")
    ap.add_argument("--out", type=pathlib.Path, default=pathlib.Path("data/cache/local_tank_icons"))
    ap.add_argument("--crops-out", type=pathlib.Path,
                    default=REPO_ROOT / "data" / "cache" / "tank_icon_crops.json")
    ap.add_argument("--emit-crops", action="store_true",
                    help="只按 --out 现有图标重算内容框（展示框），不导出图标")
    ap.add_argument("--tank", action="append", default=[])
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--use-2x", action="store_true", help="优先用 @2x（512×256）")
    ap.add_argument("--allow-small", action="store_true", help="允许退到 SmallTankIcons（128×32）")
    ap.add_argument("--jobs", type=int, default=8)
    ap.add_argument("--lang", default="en", help="取本地化名的 Strings 语言（图标名依赖英文名）")
    ap.add_argument("--list", action="store_true", help="只打印解析结果，不写盘")
    args = ap.parse_args()

    if args.emit_crops:
        return emit_crops(args.out, args.crops_out)

    game_data = args.game_data or default_game_data()
    ui = game_data / "Gfx/UI"
    if not ui.is_dir():
        print(f"!! 客户端 UI 目录不存在: {ui}（用 --game-data 指定）", file=sys.stderr)
        return 2
    table = read_tank_table(args.pb)
    index = build_index(ui)
    key_index = index["key"]
    ctx = build_icon_context(game_data, args.lang)
    if args.all:
        targets = [(tid, t["stem"], t["nation"]) for tid, t in sorted(table.items())]
    elif args.tank:
        targets = [(int(x), table[int(x)]["stem"], table[int(x)]["nation"]) for x in args.tank]
    else:
        print("!! 需要 --tank <id> 或 --all", file=sys.stderr)
        return 2
    # 别名表自检：目标必须在客户端索引里真的存在。写错文件名立刻炸——否则别名会静默退化成缺图。
    # 只校验**本次导出目标**的别名（部分客户端根/预览根没有全库图标；--all 时等价于全表校验）。
    target_stems = {stem for _, stem, _ in targets}
    bad_alias = [(s, f) for s, f in ICON_ALIAS.items()
                 if s in target_stems and icon_key(f) not in key_index]
    if bad_alias:
        for s, f in bad_alias:
            print(f"!! ICON_ALIAS 目标不存在于客户端: {s} -> {f}", file=sys.stderr)
        return 2
    ambiguous = ambiguous_keys(table, ctx)
    n_x2 = sum(1 for v in key_index.values() for e in v if e["x2"])
    print(f"图标索引：归一化键 {len(key_index)} 个 / 整文件名 {len(index['name'])} 个"
          f"（@2x {n_x2}）｜声明源：逐车 iconPath {len(ctx['declared'])} 辆、"
          f"注册表 {len(ctx['registry'])} 键、list.xml {len(ctx['list_keys'])} 辆、"
          f"字符串 {len(ctx['strings'])} 条")

    def declared_for(stem: str, nation: str) -> list:
        return declared_candidates(stem, nation, ctx, ambiguous)

    if args.list:
        for tid, stem, nation in targets:
            ent = pick_icon(index, stem, display_name(stem, ctx), args.use_2x, args.allow_small,
                            declared_for(stem, nation))
            name = ent["file"] if ent else "未命中"
            src = f"[{ent.get('match_src')}]" if ent else ""
            print(f"{tid}\t{stem}\t{name}\t{src}")
        return 0

    payloads = [(tid, stem, display_name(stem, ctx), declared_for(stem, nation),
                 str(game_data), str(args.out), args.use_2x, args.allow_small, index)
                for tid, stem, nation in targets]
    state = {"total": len(targets), "done": 0, "missing": 0, "finished": False, "results": []}
    started = time.time()
    status = args.out / "_export_status.json"
    _write_status(status, state)
    with concurrent.futures.ProcessPoolExecutor(max_workers=args.jobs) as pool:
        for r in pool.map(_export_one, payloads):
            if r["status"] == "ok":
                state["done"] += 1
            else:
                state["missing"] += 1
            state["results"].append(r)
            if (state["done"] + state["missing"]) % 100 == 0:
                state["elapsed_sec"] = round(time.time() - started, 1)
                _write_status(status, state)
    state["finished"] = True
    state["elapsed_sec"] = round(time.time() - started, 1)
    _write_status(status, state)
    ok = [r for r in state["results"] if r["status"] == "ok"]
    small = sum(1 for r in ok if r.get("small"))
    skinned = sum(1 for r in ok if r.get("grade") == "big-skin")
    by_src = {}
    for r in ok:
        by_src[r.get("match_src")] = by_src.get(r.get("match_src"), 0) + 1
    print(f"完成：导出 {state['done']}/{state['total']}（大图皮肤变体 {skinned}，小图标兜底 {small}）"
          f"，未命中 {state['missing']}，耗时 {state['elapsed_sec']}s")
    print("匹配来源分布:", by_src)
    miss = [r for r in state["results"] if r["status"] != "ok"]
    if miss:
        print("未命中清单（客户端无该车图标）:")
        for r in miss[:30]:
            print(f"   {r['tank_id']} {r['stem']} ({r['status']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
