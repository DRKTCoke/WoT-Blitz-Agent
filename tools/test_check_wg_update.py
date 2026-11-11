"""check_wg_update.py 的契约测试（standalone，无 pytest 依赖）。

背景（2026-11-11）：smartDlcUrls 解析曾只认 flow-style `[...]`，而客户端
regions_publish.yaml 实际为 block-style `- "url"` 行——SmartDLC 检查自写下起
长期假阴性（永远报"无补丁清单"，实际 2026-11-10 已有 s11.21.0_1 微更新）。
本文件锁定两种风格的解析行为与微版本推进，防止回归。

用法：python tools/test_check_wg_update.py
"""
from __future__ import annotations

import importlib.util
import pathlib

_TOOLS = pathlib.Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("check_wg_update", _TOOLS / "check_wg_update.py")
_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_mod)

BLOCK_STYLE = (
    "Common:\n"
    "    smartDlcUrls:\n"
    '        - "http://dl-wotblitz-gc.wargaming.net/dlc"\n'
    '        - "http://cdn.static.wotb.app/dlc"\n'
    "    cdnSelection:\n"
    "        useNewCdnSelectionAlgorithm: true\n"
    "Regions:\n"
)


def test_smartdlc_urls_block_style():
    # 客户端 regions_publish.yaml 的真实格式（block-style）
    assert _mod.smartdlc_urls(BLOCK_STYLE) == [
        "http://dl-wotblitz-gc.wargaming.net/dlc",
        "http://cdn.static.wotb.app/dlc",
    ]


def test_smartdlc_urls_block_stops_at_sibling_key():
    # 回归：曾把后续键的引号值（public: "true"）当 host，拼出 true/s11.21.0_1.yaml
    txt = 'Common:\n    smartDlcUrls:\n        public: "true"\n'
    assert _mod.smartdlc_urls(txt) == []


def test_smartdlc_urls_flow_style():
    txt = 'smartDlcUrls: ["http://a.example/dlc", "http://b.example/dlc"]\n'
    assert _mod.smartdlc_urls(txt) == ["http://a.example/dlc", "http://b.example/dlc"]


def test_smartdlc_urls_missing_key():
    assert _mod.smartdlc_urls('Common:\n    public: "true"\n') == []


def test_bump_micro():
    assert _mod.bump_micro("11.21.0", 0) == "11.21.0"
    assert _mod.bump_micro("11.21.0", 1) == "11.21.1"
    assert _mod.bump_micro("11.21.0", 2) == "11.21.2"


if __name__ == "__main__":
    fns = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for fn in fns:
        fn()
        print(f"PASS {fn.__name__}")
    print(f"{len(fns)} tests passed")
