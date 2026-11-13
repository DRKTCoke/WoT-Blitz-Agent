#!/usr/bin/env python3
"""XI 预览 research 增量同步（可复用构建入口，避免“只并 modules 漏 items”）。

用法：
    python tools/sync_preview_research.py --game-data .t11p2/preview_data \
        --bridge data/preview_tank_id_bridge.json \
        --base data/research --out .t11p2/research_work [--reset]

行为：
1. 以 `--base`（现役 11.21 research）为底；`--out` 不存在或 `--reset` 时整体复制。
2. 跑 `export_research_loadout.py --modules-only --merge`：刷新 bridge 预览车的
   modules（含 `upgradeProgression`/`gameVersion`）与 `upgrade_top_hp.json`。
3. 跑 `export_research_loadout.py --items-only`：刷新全局 consumables/provisions/
   consumables_default（预览客户端 `vehicleFilter` 的 tier 上限 + 新能力条目）。
4. 合并预览客户端 params 增量：新增 `modulesDumpSection`（7 类模块耐久 × 6 指标）
   与 42 个 format 条目；刻意不加入 `mainSections`，数据先落盘、面板暂不展示。
5. 校验：XI 升级树/provenance、hp_top、tier-11 物品可用性、42 个模块耐久参数、
   技术树 census、meta 计数；失败即非零退出。未变化的文件由第 1 步基座原样复用。
6. 输出 `<out>.sync.json`：逐文件 reused/updated/new 清单，供发布记录引用。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"
sys.path.insert(0, str(TOOLS))

import export_research_loadout as E  # noqa: E402

# t11r3（11.4.0.138_7408695 / 11.22_supertest_tier11_6）起 4 个登记位全部实装：
# J77_Ho_Ri_4（10593）、F135_AS_XX_40_t（23361）、G187_Taschenratte（30737）、
# G189_Hirschkafer（30993）——9 辆 XI、科技树 0 pending。
PREVIEW_TANKS = (15153, 27905, 28161, 30241, 30497, 10593, 23361, 30737, 30993)
EXPECTED_TT = {"nations": 9, "nodes": 312, "pending": 0, "edges": 318}
EXPECTED_ABILITIES = {
    15153: ["rechargeableNitro"],
    27905: ["combatFrenzy"],
    28161: ["rammingConfiguration"],
    30241: ["burstMode"],
    30497: ["steadyAimSystem"],
    10593: ["unguidedRocket"],
    23361: ["externalAmmoModule"],
    30737: ["auxilliaryWeapons"],
    30993: ["shellOverheat"],
}


def sha(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root: pathlib.Path) -> dict[str, str]:
    return {
        p.relative_to(root).as_posix(): sha(p)
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


def run_exporter(*args: str) -> None:
    cmd = [sys.executable, "-I", str(TOOLS / "export_research_loadout.py"), *args]
    print("+ " + " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=ROOT, check=True)


def _names_tables(game_data: pathlib.Path, langs: list[str]) -> tuple[dict, dict]:
    import yaml
    names = {lang: E.Names(game_data, lang=lang) for lang in langs}
    tables = {}
    for lang in langs:
        tables[lang] = yaml.safe_load(
            E.decode_dvpl((game_data / f"Strings/{lang}.yaml.dvpl").read_bytes()).decode("utf-8")
        ) or {}
        for key, value in E.load_localization_overlay(lang).items():
            tables[lang].setdefault(key, value)
    return names, tables


def _read_json(path: pathlib.Path):
    return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else None


def _write_json(path: pathlib.Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, separators=(",", ":")),
                    encoding="utf-8")


def sync_globals(out: pathlib.Path, game_data: pathlib.Path, langs: list[str]) -> dict:
    """devices / camouflage / skills / strings 全量对比；内容相同则原样复用。"""
    names, tables = _names_tables(game_data, langs)
    version = E.decode_dvpl((game_data / "version.txt.dvpl").read_bytes()).decode("utf-8").strip()
    changed: list[str] = []

    devices = E.export_devices(game_data, names, tables)
    dev_doc = _read_json(out / "devices.json") or {}
    if dev_doc.get("devices") != devices["devices"] or dev_doc.get("presets") != devices["presets"]:
        _write_json(out / "devices.json", {
            "meta": {"gameVersion": version, "count": len(devices["devices"]),
                     "presets": len(devices["presets"])}, **devices,
        })
        changed.append("devices.json")

    camo = E.export_camouflage(game_data)
    camo_doc = _read_json(out / "camouflage.json") or {}
    if (camo_doc.get("defaultCategory") != camo["defaultCategory"]
            or camo_doc.get("categories") != camo["categories"]):
        _write_json(out / "camouflage.json", {"meta": {"gameVersion": version}, **camo})
        changed.append("camouflage.json")

    skills = None
    skill_error = None
    try:
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            skills = E.export_skills(game_data, pathlib.Path(tmp), tables)
    except Exception as exc:  # noqa: BLE001 - 缺图标资源时保留基座并在报告中显式标记
        skill_error = f"{type(exc).__name__}: {exc}"
    if skills is not None:
        skill_doc = _read_json(out / "skills.json") or {}
        # 预览 Data 根若没带完整 avatar/技能定义，导出结果会是空；不能用空表覆盖现役
        # 20 项技能（fail-closed，保留基座并报告）。
        if not skills["skills"] and skill_doc.get("items"):
            skill_error = "preview skills empty; kept base"
        elif skill_doc.get("items") != skills["skills"] or skill_doc.get("classes") != skills["classes"]:
            _write_json(out / "skills.json", {
                "meta": {"gameVersion": version, "count": len(skills["skills"])},
                "classes": skills["classes"], "items": skills["skills"],
            })
            changed.append("skills.json")

    # params 已由 sync_params 合并；用最终键集导出参数名文案。
    fmt = (_read_json(out / "params/format.json") or {}).get("params") or {}
    strings = E.export_strings(game_data, langs, sorted(fmt))
    for lang in langs:
        path = out / f"strings/{lang}/research.json"
        doc = _read_json(path) or {"meta": {"gameVersion": version, "lang": lang}, "entries": {}}
        entries = dict(doc.get("entries") or {})
        merged = dict(entries)
        merged.update(strings[lang])
        if merged != entries:
            _write_json(path, {"meta": {"gameVersion": version, "lang": lang}, "entries": merged})
            changed.append(f"strings/{lang}/research.json")

    meta_path = out / "meta.json"
    if meta_path.exists():
        meta = _read_json(meta_path)
        files = meta.setdefault("files", {})
        # 即使某个文件内容相同/被保留，也刷新其 hash，避免上一轮空 skills 等写坏 meta。
        for name in ("devices.json", "camouflage.json", "skills.json"):
            path = out / name
            if path.is_file():
                data = path.read_bytes()
                files[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
        for name in changed:
            data = (out / name).read_bytes()
            files[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
        meta.setdefault("counts", {})["skills"] = len(
            (_read_json(out / "skills.json") or {}).get("items") or {})
        _write_json(meta_path, meta)
    return {"changed": changed, "skillIconError": skill_error}


def rule_matches(rule: dict, tank: dict) -> bool:
    if not rule:
        return False
    tiers = rule.get("tiers")
    if tiers and not (tiers["min"] <= tank["tier"] <= tiers["max"]):
        return False
    if "ids" in rule:
        if f"{tank['nation']}:{tank['stem']}" not in rule["ids"] and str(tank["tankId"]) not in rule["ids"]:
            return False
    if "nations" in rule and tank["nation"] not in rule["nations"]:
        return False
    if "extendedTags" in rule and not any(tag in tank["tags"] for tag in rule["extendedTags"]):
        return False
    return True


def available(item: dict, tank: dict) -> bool:
    flt = item.get("filter")
    if not flt:
        return True
    if any(rule_matches(rule, tank) for rule in flt.get("exclude") or []):
        return False
    include = flt.get("include") or []
    return any(rule_matches(rule, tank) for rule in include) if include else True


def sync_params(out: pathlib.Path, game_data: pathlib.Path) -> dict:
    """把预览客户端的参数增量并入 params/{structure,format}.json（只增不改）。

    `modulesDumpSection`（7 类部件 × 6 个耐久指标）刻意不加入 `mainSections`：
    数据先落盘，WotbTools 参数面板暂不展示。既有条目保持现役值（预览结构里
    `crew`/`gunCommonReloadTime` 的差异不由本函数删除）。
    """
    structure, fmt = E.export_params(game_data)
    version = E.decode_dvpl((game_data / "version.txt.dvpl").read_bytes()).decode("utf-8").strip()
    sp = out / "params/structure.json"
    fp = out / "params/format.json"
    sd = json.loads(sp.read_text(encoding="utf-8"))
    fd = json.loads(fp.read_text(encoding="utf-8"))
    sections = sd.setdefault("sections", [])
    known_sections = {s.get("section") for s in sections}
    added_sections = []
    for section in structure.get("sections", []):
        name = section.get("section")
        if name and name not in known_sections:
            sections.append(section)
            known_sections.add(name)
            added_sections.append(name)
    types = sd.setdefault("types", [])
    for name in structure.get("types", []):
        if name not in types:
            types.append(name)
    params = fd.setdefault("params", {})
    added_params = [key for key in fmt if key not in params]
    for key in added_params:
        params[key] = fmt[key]
    # previewGameVersion 只在真有增量时写——零增量的注记会造成"内容相同、
    # 字节不同"的假阳性（11.21.0_1 实例：基线 params 与客户端完全一致）。
    if added_sections or added_params:
        for doc in (sd, fd):
            doc.setdefault("meta", {})["previewGameVersion"] = version
        sp.write_text(json.dumps(sd, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
        fp.write_text(json.dumps(fd, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    meta_path = out / "meta.json"
    if meta_path.exists():
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        files = meta.setdefault("files", {})
        for name in ("params/structure.json", "params/format.json"):
            data = (out / name).read_bytes()
            files[name] = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
        prefixes = ("track", "engine", "gun", "ammobay", "turretRotator",
                    "surveyingDevice", "fuelTank")
        suffixes = ("MaxHealth", "RegenHealth", "HysteresisHealth",
                    "RegenPerSec", "BurnPerSec", "RepairTime")
        meta.setdefault("counts", {})["moduleDurabilityParams"] = sum(
            1 for key in params if key.startswith(prefixes) and key.endswith(suffixes))
        meta_path.write_text(json.dumps(meta, ensure_ascii=False, separators=(",", ":")),
                             encoding="utf-8")
    return {
        "newSections": added_sections,
        "newParams": len(added_params),
        "previewGameVersion": version,
    }


def tank_refs(modules: dict) -> dict[int, dict]:
    refs = {}
    for tank in modules.get("tanks", {}).values():
        tid = int(tank["tankId"])
        refs[tid] = {
            "tier": int(tank.get("tier") or 0),
            "nation": tank.get("nation", ""),
            "stem": tank.get("stem", ""),
            "tankId": tid,
            "tags": tank.get("tags") or [],
        }
    return refs


def validate(out: pathlib.Path, bridge_ids: set[int] | None = None,
             baseline_counts: dict | None = None) -> dict:
    """同步结果校验。t11 专属断言（upgradeProgression/abilities/312 节点 census/
    modulesDumpSection/42 耐久参数）仅当 bridge 含 t11 登记位时启用——其余 preview
    场景（如 11.21.0_1 的单车 Relic bridge）走通用校验 + 基线 census 派生期望。"""
    bridge_ids = bridge_ids or set()
    baseline_counts = baseline_counts or {}
    t11_ids = sorted(set(PREVIEW_TANKS) & bridge_ids)
    modules_dir = out / "modules"
    modules_by_tank: dict[int, dict] = {}
    module_nations: dict[str, dict] = {}
    for path in sorted(modules_dir.glob("*.json")):
        payload = json.loads(path.read_text(encoding="utf-8"))
        module_nations[path.stem] = payload
        for stem, tank in (payload.get("tanks") or {}).items():
            tank["stem"] = stem
            # 模块条目本身不带 nation（国别在外层文件名）；filter 求值需要它。
            tank["nation"] = path.stem
            modules_by_tank[int(tank["tankId"])] = tank

    # 旁路表只在 bridge 含强化树车（t11 XI）时存在且必查；收藏车 bridge
    #（如 11.21.0_1 的 Relic）无 upgradeProgression，导出器不产出，属正常形态。
    if t11_ids:
        sidecar_path = out.parent / "upgrade_top_hp.json"
        if not sidecar_path.is_file():
            raise SystemExit(f"缺少顶级 HP 旁路表: {sidecar_path}")
        sidecar = {int(k): int(v) for k, v in
                   json.loads(sidecar_path.read_text(encoding="utf-8"))["tanks"].items()}
        shells_path = out.parent / "upgrade_top_shells.json"
        if not shells_path.is_file():
            raise SystemExit(f"缺少顶级弹槽加成旁路表: {shells_path}")
        shell_sidecar = {int(k): v for k, v in
                         json.loads(shells_path.read_text(encoding="utf-8"))["tanks"].items()}
    else:
        sidecar, shell_sidecar = {}, {}

    problems: list[str] = []
    expected_hp: dict[int, int] = {}
    for tid in t11_ids:
        tank = modules_by_tank.get(tid)
        if tank is None:
            problems.append(f"modules 缺预览车 {tid}")
            continue
        progression = tank.get("upgradeProgression")
        if not progression:
            problems.append(f"{tid} 缺 upgradeProgression")
            continue
        if tank.get("gameVersion") != progression.get("provenance", {}).get("gameVersion"):
            problems.append(f"{tid} gameVersion 与 provenance 不一致")
        hp = E.upgrade_top_hp(tank, progression)
        if hp is None:
            problems.append(f"{tid} 无法计算顶级 HP")
        else:
            expected_hp[tid] = hp
        if sidecar.get(tid) != hp:
            problems.append(f"{tid} upgrade_top_hp {sidecar.get(tid)} != {hp}")
        # 逐弹槽点满加成旁路表（tank_cache `*_top` 的输入）：有弹槽属性的车必须在表。
        try:
            bonuses = E.upgrade_top_shell_bonuses(progression)
        except RuntimeError as exc:
            problems.append(f"{tid} upgrade_top_shell_bonuses: {exc}")
            bonuses = None
        if bonuses is not None and shell_sidecar.get(tid) != bonuses:
            problems.append(f"{tid} upgrade_top_shells {shell_sidecar.get(tid)} != {bonuses}")
        # 客户端数据自身异常：同节点同名属性（t11r3 Ho-Ri 4 的 SLOT_2 写了两遍）。
        # 数值按逐条叠加照原样保留，这里只告警，不当成导出缺陷、也不替客户端纠错。
        duplicates = (progression.get("provenance") or {}).get("attributeDuplicates")
        if duplicates:
            for dup in duplicates:
                print(f"   [warn] {tid} {dup['techName']} {dup['attribute']} "
                      f"重复 {dup['values']}（客户端数据异常，按逐条叠加计入）")
        if tank.get("vehicleAbilities") != EXPECTED_ABILITIES[tid]:
            problems.append(f"{tid} vehicleAbilities {tank.get('vehicleAbilities')} "
                            f"!= {EXPECTED_ABILITIES[tid]}")

    catalogs = {}
    for kind in ("consumables", "provisions"):
        payload = json.loads((out / f"{kind}.json").read_text(encoding="utf-8"))
        catalogs[kind] = payload["items"]
    if t11_ids:
        # XI 车需要 tier>10 可用项；基线数据集（无 XI）里 tier<=10 过滤是合法形态。
        for kind, items in catalogs.items():
            for item in items:
                for rule in (item.get("filter") or {}).get("include") or []:
                    tiers = rule.get("tiers")
                    if tiers and tiers.get("max") == 10:
                        problems.append(f"{kind}:{item['stem']} 仍限制 tier<=10")
                        break

    refs = tank_refs({"tanks": modules_by_tank})
    for tid in t11_ids:
        ref = refs.get(tid)
        if not ref:
            continue
        for kind, items in catalogs.items():
            pool = [
                item["stem"] for item in items
                if not item.get("isAbility") and item.get("mode") == "common"
                and available(item, ref)
            ]
            if len(pool) < 5:
                problems.append(f"{tid} {kind} 可用项过少: {pool}")

    params_doc = json.loads((out / "params/structure.json").read_text(encoding="utf-8"))
    params_fmt = json.loads((out / "params/format.json").read_text(encoding="utf-8"))["params"]
    prefixes = ("track", "engine", "gun", "ammobay", "turretRotator",
                "surveyingDevice", "fuelTank")
    suffixes = ("MaxHealth", "RegenHealth", "HysteresisHealth",
                "RegenPerSec", "BurnPerSec", "RepairTime")
    if t11_ids:
        # 预览客户端的参数增量断言（t11 链专属；基线客户端合并根不含这些增量）。
        if "modulesDumpSection" not in {s.get("section") for s in params_doc.get("sections", [])}:
            problems.append("params/structure.json 缺 modulesDumpSection")
        durability = [key for key in params_fmt
                      if key.startswith(prefixes) and key.endswith(suffixes)]
        if len(durability) != 42:
            problems.append(f"模块耐久参数 {len(durability)} != 42")

    meta = json.loads((out / "meta.json").read_text(encoding="utf-8"))
    counts = meta.get("counts") or {}
    if counts.get("consumables") != len(catalogs["consumables"]):
        problems.append("meta.counts.consumables 与文件不一致")
    if counts.get("provisions") != len(catalogs["provisions"]):
        problems.append("meta.counts.provisions 与文件不一致")
    # techTree census：t11 bridge 对 11.22 超测树（312 节点）；其余场景树应与基线一致。
    expected_tt = EXPECTED_TT if t11_ids else baseline_counts.get("techTree")
    if expected_tt is not None and counts.get("techTree") != expected_tt:
        problems.append(f"techTree census {counts.get('techTree')} != {expected_tt}")
    # 车数 = 基线车数（基座 meta）+ bridge 全部车型（t11r3 为 738+9；11.21.0_1 为 738+1）。
    expected_tanks = baseline_counts.get("tanks", 738) + len(bridge_ids)
    if counts.get("tanks") != expected_tanks:
        problems.append(f"counts.tanks {counts.get('tanks')} != {expected_tanks}")

    if problems:
        raise SystemExit("research sync 校验失败:\n" + "\n".join("  - " + p for p in problems))
    return {
        "previewHpTop": expected_hp,
        "consumables": len(catalogs["consumables"]),
        "provisions": len(catalogs["provisions"]),
        "previewTanks": len(t11_ids),
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base", default="data/research")
    ap.add_argument("--out", required=True)
    ap.add_argument("--game-data", required=True)
    ap.add_argument("--bridge", default="data/preview_tank_id_bridge.json")
    ap.add_argument("--reset", action="store_true",
                    help="用 --base 重建 --out（默认保留既有 out 做增量）")
    args = ap.parse_args()

    base = (ROOT / args.base).resolve() if not pathlib.Path(args.base).is_absolute() else pathlib.Path(args.base)
    out = (ROOT / args.out).resolve() if not pathlib.Path(args.out).is_absolute() else pathlib.Path(args.out)
    if not base.is_dir():
        raise SystemExit(f"--base 不存在: {base}")
    if args.reset or not out.exists():
        if out.exists():
            shutil.rmtree(out)
        shutil.copytree(base, out)
        print(f"baseline: {base} -> {out}", flush=True)
    before = snapshot(out)
    # 基线 census/车数：validate 的非 t11 期望值从这里派生（树/车数应只增 bridge 增量）。
    baseline_meta_path = out / "meta.json"
    baseline_counts = (json.loads(baseline_meta_path.read_text(encoding="utf-8"))
                       .get("counts") or {}) if baseline_meta_path.is_file() else {}
    bridge_path = pathlib.Path(args.bridge)
    bridge_ids = {int(k) for k in json.loads(
        (bridge_path if bridge_path.is_absolute() else ROOT / args.bridge)
        .read_text(encoding="utf-8"))}

    run_exporter("--game-data", str(args.game_data), "--bridge", str(ROOT / args.bridge),
                 "--modules-only", "--merge", "--out", str(out))
    run_exporter("--game-data", str(args.game_data), "--items-only", "--out", str(out))
    params_check = sync_params(out, pathlib.Path(args.game_data))
    globals_check = sync_globals(out, pathlib.Path(args.game_data), ["en", "zh-Hans"])
    checks = validate(out, bridge_ids=bridge_ids, baseline_counts=baseline_counts)
    checks["params"] = params_check
    checks["globals"] = globals_check
    checks["paramsDurability"] = json.loads(
        (out / "meta.json").read_text(encoding="utf-8")
    )["counts"].get("moduleDurabilityParams")

    after = snapshot(out)
    report = {
        "base": str(base),
        "out": str(out),
        "gameData": str(pathlib.Path(args.game_data).resolve()),
        "reused": sorted(p for p, h in after.items() if before.get(p) == h),
        "updated": sorted(p for p, h in after.items() if p in before and before[p] != h),
        "new": sorted(p for p in after if p not in before),
        "checks": checks,
    }
    report_path = out.with_name(out.name + ".sync.json")
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"reused={len(report['reused'])} updated={len(report['updated'])} new={len(report['new'])}")
    print(f"report -> {report_path}")


if __name__ == "__main__":
    main()
