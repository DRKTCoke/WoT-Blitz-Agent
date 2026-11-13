#!/usr/bin/env python3
"""extract_vehicles.Names 运行时覆盖层回退的独立单测（standalone，无 pytest 依赖）。

锁 2026-11-13 扩充后的契约：**dynloc 层（dynamicContentLocalizations）作为第三字符串源**
参与缺失键回退——11.21.0_1 的 Relic D3（Oth99_Relic）车辆/模块名键只在 dynloc
（`localizations/` 更新后仍 0 命中）。三条不变量：

1. 同键并存时 `localizations/` 优先（setdefault 首键赢，普通覆盖层先于 dynloc）；
2. dynloc 独有键可回退（Oth99_Relic → "Relic D3"）；
3. 非拉丁值仍被拒绝（dynloc 是客户端语言译名，中文不能灌进英文数据）。
"""
from __future__ import annotations

import os
import pathlib
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from extract_vehicles import Names  # noqa: E402


def _write_layer(tmp: pathlib.Path, layer: str, payload: str) -> None:
    root = tmp / "wotblitz" / "DAVAProject" / "cache" / layer
    root.mkdir(parents=True, exist_ok=True)
    (root / "zh-Hans.yaml").write_text(payload, encoding="utf-8")


def run(tmp: pathlib.Path) -> None:
    _write_layer(tmp, "localizations",
                 '"#other_vehicles:Shared_Key": "LocWins"\n')
    _write_layer(tmp, "dynamicContentLocalizations",
                 '"#other_vehicles:Oth99_Relic": "Relic D3"\n'
                 '"#other_vehicles:Shared_Key": "DynLoses"\n'
                 '"#other_vehicles:Oth99_Relic_Custom": "Relic Manta"\n'
                 '"#ussr_vehicles:КВ-1С": "КВ-1С"\n')

    saved = os.environ.get("LOCALAPPDATA")
    os.environ["LOCALAPPDATA"] = str(tmp)
    try:
        table = Names._load_overlay()
    finally:
        if saved is None:
            os.environ.pop("LOCALAPPDATA", None)
        else:
            os.environ["LOCALAPPDATA"] = saved

    assert table.get("other_vehicles:Oth99_Relic") == "Relic D3", \
        "dynloc 独有键必须回退（11.21.0_1 Relic）"
    assert table.get("other_vehicles:Oth99_Relic_Custom") == "Relic Manta"
    assert table.get("other_vehicles:Shared_Key") == "LocWins", \
        "localizations 层必须优先于 dynloc"
    assert "ussr_vehicles:КВ-1С" not in table, "非拉丁值必须被拒绝"


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        run(pathlib.Path(tmp))
    print("extract names dynloc tests: PASS")


if __name__ == "__main__":
    main()
