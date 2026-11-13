"""Materialize reviewed covers in an unpublished pack, using an independent raw WG export."""
import json
from pathlib import Path

from export_tank_icons import emit_crops
from pack_incremental import overlay_file, sha256_file


CATALOG = Path(__file__).resolve().parents[1] / "data/tank_image_overrides/manifest.json"


def matches_dataset(catalog: dict, dataset_id: str | None, preview_source: str | None,
                    inferred_version: str | None = None) -> bool:
    # --dataset-id names a client source; inferred plan versions may instead name public
    # release routes. The exact preview archive identifies the client across route changes.
    if dataset_id is not None and dataset_id != catalog["dataset_id"]:
        return False
    if preview_source is not None:
        return preview_source == catalog["preview_source"]
    return (dataset_id or inferred_version) == catalog["dataset_id"]


def apply_covers(pack: Path, raw_icons: Path, eligible: bool, catalog: dict,
                 catalog_dir: Path = CATALOG.parent) -> dict[str, str]:
    """Compare the *raw* export every time, including when a base pack already has custom art."""
    if (pack / "version.json").exists():
        raise ValueError(f"already stamped; clone to an unpublished candidate first: {pack}")
    icons = pack / "tank_images"
    if raw_icons.resolve() == icons.resolve():
        raise ValueError("original client icons must be separate from the staged output")
    crops = icons / "crops.json"
    declared = json.loads(crops.read_text(encoding="utf-8"))["icons"]
    if set(declared) != {path.stem for path in icons.glob("*.webp") if path.stem.isdecimal()}:
        raise ValueError(f"incomplete candidate icon set for {crops}")
    decisions = {}
    for tid, row in catalog["vehicles"].items():
        if not tid.isascii() or not tid.isdecimal() or str(int(tid)) != tid:
            raise ValueError(f"invalid vehicle ID in {CATALOG}: {tid!r}")
        filename = f"{tid}.webp"
        if not (icons / filename).is_file():
            continue  # Removed/absent vehicle: never introduce an icon into the pack.
        source = raw_icons / filename
        if not source.is_file():
            raise ValueError(f"original client icon unavailable for {tid}: {source}")
        raw_hash = sha256_file(source)
        if raw_hash == row["sha256"]:
            raise ValueError(f"{source} is the reviewed render; supply the original client icon")
        chosen = source
        if not eligible or not row["enabled"]:
            status = "original (out of scope or retired)"
        elif raw_hash != catalog["placeholder_sha256"]:
            status = f"original (source changed; review {raw_hash})"
        else:
            model = pack / "glb" / tid / "model.glb"
            config = pack / "tank" / f"{tid}.json"
            replacement = catalog_dir / "covers" / filename
            if not model.is_file() or sha256_file(model) != row["model_sha256"]:
                status = "original (model changed or missing; review)"
            elif not config.is_file() or sha256_file(config) != row["tank_sha256"]:
                status = "original (tank config changed or missing; review)"
            elif not replacement.is_file() or sha256_file(replacement) != row["sha256"]:
                status = "original (reviewed image missing or digest mismatch)"
            else:
                chosen = replacement
                status = "render"
        overlay_file(icons, filename, chosen)
        decisions[tid] = status
        print(f"  cover {tid}: {status}")
    if decisions and emit_crops(icons, crops) != 0:
        raise ValueError(f"cannot calculate cover bounds in {icons}")
    return decisions


def load_catalog() -> dict:
    return json.loads(CATALOG.read_text(encoding="utf-8"))
