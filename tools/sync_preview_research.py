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
4. 校验：XI 升级树/provenance、hp_top、tier-11 物品可用性、技术树 census、
   meta 计数；失败即非零退出。未变化的文件由第 1 步的基座原样复用。
5. 输出 `<out>.sync.json`：逐文件 reused/updated/new 清单，供发布记录引用。
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

PREVIEW_TANKS = (15153, 27905, 28161, 30241, 30497)
EXPECTED_TT = {"nations": 9, "nodes": 308, "pending": 4, "edges": 314}


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


def validate(out: pathlib.Path) -> dict:
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

    sidecar_path = out.parent / "upgrade_top_hp.json"
    if not sidecar_path.is_file():
        raise SystemExit(f"缺少顶级 HP 旁路表: {sidecar_path}")
    sidecar = {int(k): int(v) for k, v in
               json.loads(sidecar_path.read_text(encoding="utf-8"))["tanks"].items()}

    problems: list[str] = []
    expected_hp: dict[int, int] = {}
    for tid in PREVIEW_TANKS:
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

    catalogs = {}
    for kind in ("consumables", "provisions"):
        payload = json.loads((out / f"{kind}.json").read_text(encoding="utf-8"))
        catalogs[kind] = payload["items"]
        for item in payload["items"]:
            for rule in (item.get("filter") or {}).get("include") or []:
                tiers = rule.get("tiers")
                if tiers and tiers.get("max") == 10:
                    problems.append(f"{kind}:{item['stem']} 仍限制 tier<=10")
                    break

    refs = tank_refs({"tanks": modules_by_tank})
    for tid in PREVIEW_TANKS:
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

    meta = json.loads((out / "meta.json").read_text(encoding="utf-8"))
    counts = meta.get("counts") or {}
    if counts.get("consumables") != len(catalogs["consumables"]):
        problems.append("meta.counts.consumables 与文件不一致")
    if counts.get("provisions") != len(catalogs["provisions"]):
        problems.append("meta.counts.provisions 与文件不一致")
    if counts.get("techTree") != EXPECTED_TT:
        problems.append(f"techTree census {counts.get('techTree')} != {EXPECTED_TT}")
    if counts.get("tanks") != 743:
        problems.append(f"counts.tanks {counts.get('tanks')} != 743")

    if problems:
        raise SystemExit("research sync 校验失败:\n" + "\n".join("  - " + p for p in problems))
    return {
        "previewHpTop": expected_hp,
        "consumables": len(catalogs["consumables"]),
        "provisions": len(catalogs["provisions"]),
        "previewTanks": len(PREVIEW_TANKS),
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

    run_exporter("--game-data", str(args.game_data), "--bridge", str(ROOT / args.bridge),
                 "--modules-only", "--merge", "--out", str(out))
    run_exporter("--game-data", str(args.game_data), "--items-only", "--out", str(out))
    checks = validate(out)

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
