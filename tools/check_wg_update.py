#!/usr/bin/env python3
"""检查 WG / Steam 客户端更新通道是否发布了新版本（如 11.22）或微更新。

检查面（对应"游戏客户端自己的更新方式"的全部数据通道）：

1. **Steam depot**（app 444200，Steam 版客户端的分发通道）：
   本机 `appmanifest_444200.acf` 的 buildid 与 444202（Windows 内容 depot）
   manifest，对比 api.steamcmd.net 公共分支——manifest gid 一致 = Windows 内容
   未变。注意：动态内容微更新**不走 Steam manifest**（见 3），buildid 前进而
   manifest 不变不排除有新内容。
2. **WG conf**：`conf/regions_<ver>.yaml`（HEAD 探测，200 = 服务端该版本配置
   就绪）。除 --target 外自动探测本机版本的 +1/+2 微版本——微版本发布前
   conf 会先行就绪（2026-11-06 `regions_11.21.1.yaml` 实例，与 11.21.0 逐字节
   一致，纯版本号占位）。
3. **SmartDLC 微更新**：`s<version>_<n>.yaml` 补丁清单，本机版本与 --target
   都探。命中即下载清单并 HEAD 其中引用的 dc_*.dvpm 与本地化 yaml——这类
   更新完全绕开 Steam manifest（2026-11-10 `s11.21.0_1` 实例：贴图/活动文案/
   新收藏车 Relic 注册文案，Steam Windows manifest 全程未动）。

只读探测，不落盘。网络异常按不可达（状态 0）记录，不中断整体探测；退出码
恒为 0，结论以文本输出为准。smartDlcUrls 解析兼容 flow/block 两种 YAML 列表
风格——客户端 regions_publish.yaml 实际为 block-style，早期 flow-only 正则使
本项检查长期假阴性（2026-11-11 修复，tools/test_check_wg_update.py 锁定）。

用法：
    python tools/check_wg_update.py [--target 11.22.0]
"""
from __future__ import annotations

import argparse
import datetime as _dt
import json
import pathlib
import re
import sys
import urllib.request

TOOLS_DIR = pathlib.Path(__file__).resolve().parent
REPO_ROOT = TOOLS_DIR.parent
sys.path.insert(0, str(TOOLS_DIR / "wotbtools"))

from wotb_sc2 import decode_dvpl  # noqa: E402

APP_ID = "444200"
WINDOWS_DEPOT = "444202"
STEAM_API = f"https://api.steamcmd.net/v1/info/{APP_ID}"
STEAM_LIBRARIES = (
    pathlib.Path("D:/SteamLibrary"),
    pathlib.Path("C:/Program Files (x86)/Steam"),
    pathlib.Path("C:/Program Files/Steam"),
)
CONF_HOSTS = (
    "http://dl-wotblitz-gc.wargaming.net/conf",
    "http://cdn.static.wotb.app/conf",
)
DLC_HOSTS = (
    "http://dl-wotblitz-gc.wargaming.net/dlc",
    "http://cdn.static.wotb.app/dlc",
)
DLC_LANGS = ("en", "ru", "zh-Hans")


def head(url: str) -> tuple[int, str | None, str | None]:
    """HEAD 探测。HTTPError 返回其状态码；网络层异常（瞬断/重置）重试一次后
    记 0=不可达——dl-wotblitz-gc 实测偶发瞬断，不得让单点探测中断整体。"""
    req = urllib.request.Request(url, method="HEAD", headers={"User-Agent": "curl/8"})
    for attempt in range(2):
        try:
            with urllib.request.urlopen(req, timeout=20) as r:
                return r.status, r.headers.get("Content-Length"), r.headers.get("Last-Modified")
        except urllib.error.HTTPError as e:
            return e.code, None, None
        except OSError:
            if attempt == 0:
                continue
    return 0, None, None


def get_text(url: str) -> str | None:
    req = urllib.request.Request(url, headers={"User-Agent": "curl/8"})
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.read().decode("utf-8", "replace")
    except OSError:
        return None


def local_state() -> dict:
    out: dict = {}
    version = None
    for lib in STEAM_LIBRARIES:
        data = lib / "steamapps" / "common" / "World of Tanks Blitz" / "Data"
        vp = data / "version.txt.dvpl"
        if vp.exists():
            version = decode_dvpl(vp.read_bytes()).decode("utf-8", "replace").strip()
            out["game_dir"] = data
            break
    if version:
        out["version"] = version
        out["short_version"] = ".".join(version.split(".")[:3])
    for lib in STEAM_LIBRARIES:
        acf = lib / "steamapps" / "appmanifest_444200.acf"
        if acf.exists():
            txt = acf.read_text(encoding="utf-8", errors="replace")
            m = re.search(r'"buildid"\s+"(\d+)"', txt)
            out["local_buildid"] = m.group(1) if m else None
            seg = txt[txt.find('"InstalledDepots"'):]
            mm = re.search(rf'"{WINDOWS_DEPOT}"\s*{{\s*"manifest"\s+"(\d+)"', seg)
            out["local_windows_manifest"] = mm.group(1) if mm else None
            break
    return out


def steam_cdn_state() -> dict:
    req = urllib.request.Request(STEAM_API, headers={"User-Agent": "curl/8"})
    d = json.loads(urllib.request.urlopen(req, timeout=30).read())["data"][APP_ID]
    out: dict = {}
    br = d.get("depots", {}).get("branches", {}).get("public", {})
    out["public_buildid"] = br.get("buildid")
    out["public_timeupdated"] = br.get("timeupdated")
    man = d.get("depots", {}).get(WINDOWS_DEPOT, {}).get("manifests", {}).get("public", {})
    out["cdn_windows_manifest"] = man.get("gid")
    out["cdn_windows_size"] = man.get("size")
    return out


def bump_micro(short_version: str, delta: int) -> str:
    a, b, c = (int(x) for x in short_version.split(".")[:3])
    return f"{a}.{b}.{c + delta}"


def smartdlc_urls(txt: str) -> list[str]:
    """提取 regions_publish.yaml 的 smartDlcUrls；兼容 flow `[...]` 与 block
    `- "url"` 两种列表风格。block 分支只认紧随键后的连续列表项行，首个非列表
    行即结束——不得把后续键的引号值（如 public: "true"）当 host。"""
    m = re.search(r"smartDlcUrls\s*:\s*\[(.*?)\]", txt, re.S)
    if m:
        return re.findall(r'"([^"]+)"', m.group(1))
    m = re.search(r"smartDlcUrls\s*:", txt)
    if not m:
        return []
    hosts: list[str] = []
    for line in txt[m.end():].splitlines():
        if not line.strip():
            continue
        item = re.match(r"\s+-\s+\"([^\"]+)\"\s*$", line)
        if not item:
            break
        hosts.append(item.group(1))
    return hosts


def probe_conf(versions: list[str]) -> dict[str, list[tuple[int, str | None, str | None]]]:
    """逐版本 HEAD conf/regions_<ver>.yaml（两个 host），按版本聚合结果。"""
    return {ver: [head(f"{host}/regions_{ver}.yaml") for host in CONF_HOSTS] for ver in versions}


def probe_smartdlc(short_versions: list[str]) -> list[dict]:
    """探测 s<ver>_<n>.yaml（两 host × 各版本 × n=1..3）；命中即 GET 清单并
    HEAD 引用的 dc_*.dvpm 与本地化 yaml，让"补丁里有什么"一眼可见。"""
    urls: list[str] = []
    seen: set[str] = set()
    for host in DLC_HOSTS:
        base = host.rstrip("/")
        for ver in short_versions:
            for n in (1, 2, 3):
                u = f"{base}/s{ver}_{n}.yaml"
                if u not in seen:
                    seen.add(u)
                    urls.append(u)
    out: list[dict] = []
    for u in urls:
        status, size, modified = head(u)
        if status != 200:
            continue
        # 清单内引用相对 DLC 根（s*.yaml 的所在目录），如 <host>/dlc/<ref>
        dlc_base = u.rsplit("/", 1)[0]
        entry: dict = {"url": u, "size": size, "modified": modified, "refs": []}
        txt = get_text(u)
        if txt:
            entry["manifest"] = "  ".join(line.strip() for line in txt.splitlines() if line.strip())
            refs: list[str] = []
            for line in txt.splitlines():
                key, _, val = line.strip().partition(":")
                val = val.strip()
                if key in ("astc", "dx11") and val:
                    refs.append(val)
                elif key == "dynamicContentLocalizationsDir" and val:
                    refs.extend(f"{val}/{lang}.yaml" for lang in DLC_LANGS)
            for ref in refs:
                s2, sz2, mod2 = head(f"{dlc_base}/{ref}")
                entry["refs"].append((ref, s2, sz2, mod2))
        out.append(entry)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--target", default="11.22.0", help="探测的正式版本号（conf/regions_<ver>.yaml）")
    args = ap.parse_args()

    local = local_state()
    print("== 本机客户端 ==")
    print(f"  version:      {local.get('version')}")
    print(f"  buildid:      {local.get('local_buildid')}")
    print(f"  win manifest: {local.get('local_windows_manifest')}")

    print("== Steam depot 公共分支 ==")
    cdn = steam_cdn_state()
    print(f"  buildid:      {cdn.get('public_buildid')}")
    tu = cdn.get("public_timeupdated")
    if tu and str(tu).isdigit():
        ts = _dt.datetime.fromtimestamp(int(tu), _dt.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
        print(f"  入库时间:     {ts}")
    print(f"  win manifest: {cdn.get('cdn_windows_manifest')} (size={cdn.get('cdn_windows_size')})")
    same = local.get("local_windows_manifest") == cdn.get("cdn_windows_manifest")
    buildid_moved = bool(
        local.get("local_buildid") and cdn.get("public_buildid")
        and local["local_buildid"] != cdn["public_buildid"]
    )
    if same:
        note = "（buildid 有入库但 Windows 内容未变；动态内容不走 manifest，见 SmartDLC 段）" if buildid_moved else ""
        print(f"  → Windows 内容与本机一致：Steam 通道暂无新客户端数据{note}")
    else:
        print("  → Windows manifest 已前进：Steam 通道有新客户端数据可拉")

    # conf：target + 现役版本 + 微版本 +1/+2（微版本发布前 conf 先行就绪）
    conf_versions = [args.target]
    if local.get("short_version"):
        base = local["short_version"]
        conf_versions += [base, bump_micro(base, 1), bump_micro(base, 2)]
    deduped: list[str] = []
    for v in conf_versions:
        if v not in deduped:
            deduped.append(v)
    conf_versions = deduped
    print("== WG conf ==")
    conf = probe_conf(conf_versions)
    for ver in conf_versions:
        for host, (status, size, modified) in zip(CONF_HOSTS, conf[ver]):
            note = f" ({size} B, {modified})" if status == 200 else (" (不可达)" if status == 0 else "")
            print(f"  {host}/regions_{ver}.yaml: {status}{note}")
    conf_roles: dict[str, str] = {}
    for ver in conf_versions:
        if any(status == 200 for status, _, _ in conf[ver]):
            conf_roles[ver] = (
                "现役配置" if ver == local.get("short_version")
                else ("目标版本" if ver == args.target else "微版本预备")
            )
    if conf_roles:
        print(f"  → conf 就绪: {', '.join(f'{v}（{r}）' for v, r in conf_roles.items())}")

    print("== SmartDLC 微更新 ==")
    dlc_versions: list[str] = []
    if local.get("short_version") and local["short_version"] not in dlc_versions:
        dlc_versions.append(local["short_version"])
    if args.target not in dlc_versions:
        dlc_versions.append(args.target)
    entries = probe_smartdlc(dlc_versions)
    if entries:
        for e in entries:
            mod = f", {e['modified']}" if e.get("modified") else ""
            print(f"  200 {e['url']} ({e.get('size')} B{mod})")
            if e.get("manifest"):
                print(f"      清单: {e['manifest']}")
            for ref, s2, sz2, mod2 in e["refs"]:
                rnote = f"{sz2} B, {mod2}" if s2 == 200 else str(s2)
                print(f"      引用: {ref} → {rnote}")
    else:
        print(f"  无补丁清单（探测版本: {', '.join(dlc_versions)}）")

    print("== 结论 ==")
    if not same:
        print("  → 有全量客户端更新：Steam Windows manifest 已前进，随 Steam 推送即可拉取。")
    elif entries:
        print(f"  → 无全量客户端更新，但检测到 {len(entries)} 份 SmartDLC 动态内容微更新")
        print("    （贴图/活动文案/收藏车注册；该通道绕开 Steam manifest，不影响解析器数据面）。")
    if conf_roles:
        print(f"  → conf 就绪: {', '.join(f'{v}（{r}）' for v, r in conf_roles.items())}——对应版本发布准备中或已上线。")
    if same and not entries and not conf_roles:
        print(f"  更新通道暂无 {args.target} 的可下载数据，也无微更新信号。")
        print("  建议间隔复查本脚本；或等 Steam 客户端推送更新后重跑。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
