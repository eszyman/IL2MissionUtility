"""Bake editor-map woods into assets/combined_terrain.bin.

Place and the map pan never open these photographs. This script runs once,
offline, and writes two bits on the existing mask:

  bit 1  water   left as MapHelper wrote it
  bit 2  road    left as MapHelper wrote it
  bit 4  open    cleared on photographed woods, set on photographed fields
  bit 8  woods   olive ground on the editor tiles

The colour test is the same one as editormap::classify_rgb. A cell is about
111 m. It becomes woods when at least 30% of the samples in it are olive and
olive is at least as common as beige. It becomes open when at least 45% are
beige and beige is more than twice the olive. Anything less clear (contours,
towns, gray sea, a skipped tile) keeps the open bit MapHelper already stored.
Water cells are never marked woods or open.

Re-run this after MapHelper rebuilds the bin. MapHelper does not write bit 8.

    py -3 tools/mapcapture/bake_woods.py
    py -3 tools/mapcapture/bake_woods.py --dry-run
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
MASK_PATH = ROOT / "assets" / "combined_terrain.bin"
CAPTURE_DIR = ROOT / "References" / "MapCapture"

MAP_MAX = 499_200.0
FLAG_WATER = 1
FLAG_OPEN = 4
FLAG_WOODS = 8

# Every other editor pixel is about 20 m. The mask cell is about 111 m.
SAMPLE_STEP = 2
MIN_SAMPLES = 12
WOOD_NUMER = 30  # wood * 100 >= seen * WOOD_NUMER
FIELD_NUMER = 45

# Keep in step with editormap::classify_rgb and its unit test.
CLASSIFY_CASES = (
    (222, 207, 181, "field"),
    (190, 190, 150, "wood"),
    (180, 190, 140, "wood"),
    (40, 30, 20, None),
    (150, 155, 170, None),
)


def classify_name(r: int, g: int, b: int) -> str | None:
    if r >= 205 and g >= 185 and b >= 160 and r + 8 >= g and r - b <= 60:
        return "field"
    if g + 18 >= r and g > b + 25 and 140 <= g < 215 and b < 165:
        return "wood"
    return None


def check_classifier() -> None:
    for r, g, b, expect in CLASSIFY_CASES:
        got = classify_name(r, g, b)
        if got != expect:
            raise SystemExit(
                f"classify({r},{g},{b}) is {got}, editormap::classify_rgb expects {expect}"
            )


def load_mask(path: Path) -> tuple[int, int, np.ndarray]:
    data = path.read_bytes()
    if len(data) < 12 or data[:4] != b"WMAP":
        raise SystemExit(f"{path} is not a WMAP terrain mask")
    width = int.from_bytes(data[4:8], "little")
    height = int.from_bytes(data[8:12], "little")
    need = width * height
    if width <= 0 or height <= 0 or len(data) < 12 + need:
        raise SystemExit(f"{path} is truncated")
    grid = np.frombuffer(data[12 : 12 + need], dtype=np.uint8).copy()
    return width, height, grid


def load_tiles(capture_dir: Path) -> tuple[float, float, list[dict]]:
    cal_path = capture_dir / "calibration.json"
    index_path = capture_dir / "tiles" / "index.jsonl"
    if not cal_path.is_file() or not index_path.is_file():
        raise SystemExit(
            "editor photographs are not in References/MapCapture "
            "(need calibration.json and tiles/index.jsonl)"
        )
    cal = json.loads(cal_path.read_text(encoding="utf-8"))
    mpp_z = float(cal["mpp_z"])
    mpp_x = float(cal["mpp_x"])
    if mpp_z <= 0 or mpp_x <= 0:
        raise SystemExit("calibration.json has no meters-per-pixel")
    tiles: dict[tuple[int, int], dict] = {}
    for line in index_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        rec = json.loads(line)
        tiles[(int(rec["row"]), int(rec["col"]))] = rec
    if not tiles:
        raise SystemExit("tiles/index.jsonl is empty")
    ordered = [tiles[key] for key in sorted(tiles)]
    return mpp_z, mpp_x, ordered


def accumulate_tile(
    image: np.ndarray,
    z_left: float,
    x_top: float,
    mpp_z: float,
    mpp_x: float,
    width: int,
    height: int,
    wood: np.ndarray,
    field: np.ndarray,
    seen: np.ndarray,
) -> None:
    step = SAMPLE_STEP
    small = image[::step, ::step]
    rows, cols = small.shape[:2]
    if rows == 0 or cols == 0:
        return
    row_m = (np.arange(rows, dtype=np.float64) * step) + (step * 0.5)
    col_m = (np.arange(cols, dtype=np.float64) * step) + (step * 0.5)
    xx = x_top - row_m * mpp_x
    zz = z_left + col_m * mpp_z
    r = small[:, :, 0].astype(np.int16)
    g = small[:, :, 1].astype(np.int16)
    b = small[:, :, 2].astype(np.int16)
    is_field = (r >= 205) & (g >= 185) & (b >= 160) & (r + 8 >= g) & (r - b <= 60)
    is_wood = (~is_field) & (g + 18 >= r) & (g > b + 25) & (g >= 140) & (g < 215) & (b < 165)
    gx = np.floor(zz[None, :] / MAP_MAX * width).astype(np.int32)
    gy = np.floor((MAP_MAX - xx[:, None]) / MAP_MAX * height).astype(np.int32)
    inside = (xx[:, None] >= 0.0) & (xx[:, None] < MAP_MAX) & (zz[None, :] >= 0.0) & (zz[None, :] < MAP_MAX)
    inside &= (gx >= 0) & (gx < width) & (gy >= 0) & (gy < height)
    if not np.any(inside):
        return
    idx = gy * width + gx
    np.add.at(seen, idx[inside], 1)
    wood_hit = inside & is_wood
    field_hit = inside & is_field
    if np.any(wood_hit):
        np.add.at(wood, idx[wood_hit], 1)
    if np.any(field_hit):
        np.add.at(field, idx[field_hit], 1)


def apply_votes(
    grid: np.ndarray, wood: np.ndarray, field: np.ndarray, seen: np.ndarray
) -> dict[str, int]:
    water = (grid & FLAG_WATER) != 0
    enough = seen >= MIN_SAMPLES
    wood_win = enough & (wood * 100 >= seen * WOOD_NUMER) & (wood >= field) & ~water
    field_win = (
        enough
        & ~wood_win
        & (field * 100 >= seen * FIELD_NUMER)
        & (field > wood * 2)
        & ~water
    )
    old_open = (grid & FLAG_OPEN) != 0
    woods_bit = np.uint8(FLAG_WOODS)
    open_bit = np.uint8(FLAG_OPEN)
    new = grid.copy()
    new &= ~woods_bit
    new[wood_win] = (new[wood_win] | woods_bit) & ~open_bit
    new[field_win] = (new[field_win] | open_bit) & ~woods_bit
    # Water and road bits are not in the masks above, so they stay.
    new_open = (new & FLAG_OPEN) != 0
    return {
        "grid": new,
        "woods": int(np.count_nonzero(wood_win)),
        "fields": int(np.count_nonzero(field_win)),
        "open_cleared": int(np.count_nonzero(old_open & ~new_open & ~water)),
        "open_set": int(np.count_nonzero(~old_open & new_open & ~water)),
        "water": int(np.count_nonzero(water)),
        "seen_cells": int(np.count_nonzero(seen >= MIN_SAMPLES)),
    }


def cell_center(index: int, width: int, height: int) -> tuple[float, float]:
    gy, gx = divmod(int(index), width)
    z = (gx + 0.5) / width * MAP_MAX
    x = MAP_MAX - (gy + 0.5) / height * MAP_MAX
    return x, z


def anchor(mask: np.ndarray, votes: np.ndarray, seen: np.ndarray, width: int, height: int) -> tuple[float, float, float]:
    score = np.zeros(mask.shape[0], dtype=np.float64)
    np.divide(votes, seen, out=score, where=seen >= MIN_SAMPLES)
    score = np.where(mask, score, 0.0)
    # Stay off the map edge, where a tile can cover only part of a cell.
    gy, gx = np.divmod(np.arange(mask.shape[0]), width)
    edge = (gx < 8) | (gy < 8) | (gx >= width - 8) | (gy >= height - 8)
    score[edge] = 0.0
    index = int(np.argmax(score))
    x, z = cell_center(index, width, height)
    return x, z, float(score[index])


def main() -> None:
    parser = argparse.ArgumentParser(description="Bake editor-map woods into combined_terrain.bin")
    parser.add_argument("--dry-run", action="store_true", help="count woods and fields, do not write the bin")
    parser.add_argument("--mask", type=Path, default=MASK_PATH)
    parser.add_argument("--capture", type=Path, default=CAPTURE_DIR)
    args = parser.parse_args()

    check_classifier()
    if not args.mask.is_file():
        raise SystemExit(f"missing terrain mask: {args.mask}")
    width, height, grid = load_mask(args.mask)
    mpp_z, mpp_x, tiles = load_tiles(args.capture)
    wood = np.zeros(width * height, dtype=np.uint32)
    field = np.zeros(width * height, dtype=np.uint32)
    seen = np.zeros(width * height, dtype=np.uint32)

    started = time.time()
    used = 0
    missing = 0
    for n, rec in enumerate(tiles, start=1):
        path = args.capture / "tiles" / rec["file"]
        if not path.is_file():
            missing += 1
            continue
        image = np.asarray(Image.open(path).convert("RGB"))
        accumulate_tile(
            image,
            float(rec["z_left"]),
            float(rec["x_top"]),
            mpp_z,
            mpp_x,
            width,
            height,
            wood,
            field,
            seen,
        )
        used += 1
        if n % 200 == 0 or n == len(tiles):
            print(f"{n}/{len(tiles)} tiles", flush=True)
    if used == 0:
        raise SystemExit("no editor tiles could be read; the mask was not changed")

    stats = apply_votes(grid, wood, field, seen)
    new = stats.pop("grid")
    woods_mask = (new & FLAG_WOODS) != 0
    field_mask = ((new & FLAG_OPEN) != 0) & ((new & FLAG_WATER) == 0) & ~woods_mask
    # Field anchor: cells this bake set open, so the fraction is the beige vote.
    field_votes = field.copy()
    field_votes[~field_mask] = 0
    wood_x, wood_z, wood_frac = anchor(woods_mask, wood, seen, width, height)
    field_x, field_z, field_frac = anchor(field_mask, field_votes, seen, width, height)

    print(
        f"cells {width}x{height}  tiles {used}  missing {missing}  "
        f"sampled {stats['seen_cells']}  woods {stats['woods']}  fields {stats['fields']}"
    )
    print(
        f"open cleared {stats['open_cleared']}  open set {stats['open_set']}  "
        f"water {stats['water']} (unchanged)"
    )
    print(f"anchor woods  x {wood_x:.1f}  z {wood_z:.1f}  olive {wood_frac:.3f}")
    print(f"anchor field  x {field_x:.1f}  z {field_z:.1f}  beige {field_frac:.3f}")
    print(f"elapsed {time.time() - started:.0f}s")

    if args.dry_run:
        print("dry run: combined_terrain.bin was not written")
        return
    out = new.tobytes()
    if len(out) != width * height:
        raise SystemExit("refusing to write a mask whose size changed")
    blob = b"WMAP" + width.to_bytes(4, "little") + height.to_bytes(4, "little") + out
    tmp = args.mask.with_suffix(".bin.tmp")
    tmp.write_bytes(blob)
    tmp.replace(args.mask)
    print(f"wrote {args.mask}")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(1)
