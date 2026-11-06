#!/usr/bin/env python3
"""地图增量物化的纯函数回归：cache → pack/map/<key> 的优先级与 terrain sidecar。"""
from __future__ import annotations

import json
import pathlib
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

import data_version_handlers as H  # noqa: E402


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = pathlib.Path(tmp)
        out, cache = tmp / "pack", tmp / "cache"
        key, space = "karelia", "17_karelia_ka"
        (out / "map" / key).mkdir(parents=True)
        (out / "map" / key / "terrain.u16.bin").write_bytes(b"terrain")
        (out / "map" / key / "terrain.json").write_text(
            json.dumps({"size": 512, "zmax": 1, "zmin": 0, "span": 1,
                        "terrainLayout": 2}), encoding="utf-8")
        cache.mkdir()
        for name in ("glb", "ground.webp", "minimap.webp", "lighting.json", "ibl.webp",
                     "ground.layers.json", "ground.cm.webp"):
            (cache / f"{space}.{name}").write_bytes(name.encode())
        (cache / f"{space}.json").write_text(
            json.dumps({"worldBounds": {"min": [0, 0, 0], "max": [100, 200, 30]}}),
            encoding="utf-8")
        (cache / key).mkdir()
        (cache / key / "destructibles.json").write_text("{}", encoding="utf-8")
        report: dict[str, int] = {}
        H._materialize_map(out, key, space, cache, report)
        assert (out / "map" / key / "scenery.glb").read_bytes() == b"glb"
        assert (out / "map" / key / "destructibles.json").read_text() == "{}"
        assert (out / "map" / key / "ground/cm.webp").read_bytes() == b"ground.cm.webp"
        side = json.loads((out / "map" / key / "terrain.json").read_text(encoding="utf-8"))
        assert side["span"] == 200 and side["zmax"] == 30, side
        assert side["terrainBinSha256"] == H.P.sha256_file(out / "map" / key / "terrain.u16.bin")
    print("map materialize tests: PASS")


if __name__ == "__main__":
    main()
