//! terrain.rs — measured Korea terrain heights on a 100 m lattice
//!
//! The editor's heightmap is packed in the game archives, so heights are
//! measured: T-34 probes on the lattice X = 100·i, Z = 100·j are snapped
//! with the editor's **set to ground** and read back (`heightprobe`). This
//! module only stores and answers height queries; it does not parse
//! `.Group` files or place units. Unmeasured ground returns `None` so
//! callers can keep their old Y and warn.
//!
//! Storage is sparse by tile (224 × 224 nodes, the probe-file tile), heights
//! as i16 decimetres. Tile rows count north → south (`tile_row` 0 holds the
//! largest X) to match the `HG100_Trr_cc` probe file names. Extra measured
//! points (snapped mission units, off-lattice probes) are kept as a list and
//! win over the lattice within `POINT_RADIUS_M`.
//!
//! ## Public API
//! * `STEP_M`, `LATTICE_N`, `TILE`, `TILES_PER_SIDE`, `KOREA_MAP_ID`
//! * `DEFAULT_GROUND_MARGIN_M`, `DEFAULT_PARKED_PLANE_MARGIN_M`, `ground_margin_m`,
//!   `parked_plane_margin_m`
//! * `struct HeightStore` — `new`, `node` / `set_node`, `add_point`,
//!   `height_at`, `lookup` (→ `TerrainHeight`), `measured_nodes`, `tile_measured`, `to_bytes` /
//!   `from_bytes`, `load` / `save`, `merge_from`, `builtin`, `open_store`
//! * `tile_of_node`, `node_of_tile`, `tile_node_range`, `default_store_path`
//!
//! ## Used by
//! * heightprobe.rs — probe tiles and ingest of snapped files
//! * heighthelper.rs — per-machine stores merged after a split harvest
//! * ui/map.rs — Map › Terrain: store status, coverage and relief layers, height readout

#![allow(dead_code)] // wired into export and the Map tab in later phases

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::geo::MAP_MAX;

/// Lattice spacing (metres).
pub const STEP_M: f64 = 100.0;
/// Nodes per side: 0 ..= 499_200 m.
pub const LATTICE_N: usize = (MAP_MAX / STEP_M) as usize + 1;
/// Nodes per tile side (≤ 50,176 probes per file, proven in the editor).
pub const TILE: usize = 224;
pub const TILES_PER_SIDE: usize = LATTICE_N.div_ceil(TILE);
/// Map identity stored with the heights; a different map needs its own store.
pub const KOREA_MAP_ID: &str = r"graphics\LANDSCAPE_Korea";
/// Ground-unit lift over the interpolated height. On the roughest 3 × 3 km
/// measured (136–994 m) a 100 m lattice + 20 m left 0.3% of points below
/// ground and none more than 8 m; units placed high settle onto the ground.
pub const DEFAULT_GROUND_MARGIN_M: f64 = 20.0;
/// Parked planes sit on flat airfields and must not drop far.
pub const DEFAULT_PARKED_PLANE_MARGIN_M: f64 = 1.0;
/// A measured point this close to a query answers it directly.
pub const POINT_RADIUS_M: f64 = 30.0;
/// Lattice strides tried by `lookup`, finest first (100 / 200 / 400 / 800 m).
const LOOKUP_STRIDES: [usize; 4] = [1, 2, 4, 8];

/// A looked-up ground height. `spacing_m` is the lattice spacing that
/// answered (0 = a measured point within `POINT_RADIUS_M`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainHeight {
    pub y: f64,
    pub spacing_m: f64,
}

/// Lift to add to a ground unit so it never starts underground, from the
/// measured interpolation error per spacing (roughest 3 × 3 km tested:
/// 100 m + 20 m → 0.3% below, none > 8 m; 200 m + 30 m → 3% below). `None`
/// above 200 m: that is an estimate only, not safe to place units on.
pub fn ground_margin_m(spacing_m: f64) -> Option<f64> {
    if spacing_m <= 0.0 {
        Some(5.0)
    } else if spacing_m <= STEP_M {
        Some(DEFAULT_GROUND_MARGIN_M)
    } else if spacing_m <= 2.0 * STEP_M {
        Some(30.0)
    } else {
        None
    }
}

/// Lift for a plane starting on the ground (runway / parking). Airfields are
/// flat, and a plane dropped from height can break, so the lift stays small
/// and coarse spacings are refused.
pub fn parked_plane_margin_m(spacing_m: f64) -> Option<f64> {
    if spacing_m <= 0.0 {
        Some(0.5)
    } else if spacing_m <= STEP_M {
        Some(DEFAULT_PARKED_PLANE_MARGIN_M)
    } else if spacing_m <= 2.0 * STEP_M {
        Some(3.0)
    } else {
        None
    }
}

const UNKNOWN: i16 = i16::MIN;
const MAGIC: &[u8; 4] = b"HGT1";

/// Tile holding lattice node (i, j): (row north → south, column west → east).
pub fn tile_of_node(i: usize, j: usize) -> (usize, usize) {
    ((LATTICE_N - 1 - i) / TILE, j / TILE)
}

/// Lattice node of a tile cell (r, c), r counting north → south inside the tile.
pub fn node_of_tile(ti: usize, tj: usize, r: usize, c: usize) -> (usize, usize) {
    (LATTICE_N - 1 - (ti * TILE + r), tj * TILE + c)
}

/// Inclusive node ranges (i_lo, i_hi, j_lo, j_hi) covered by a tile.
pub fn tile_node_range(ti: usize, tj: usize) -> (usize, usize, usize, usize) {
    let i_hi = LATTICE_N - 1 - ti * TILE;
    let i_lo = i_hi.saturating_sub(TILE - 1);
    let j_lo = tj * TILE;
    let j_hi = (j_lo + TILE - 1).min(LATTICE_N - 1);
    (i_lo, i_hi, j_lo, j_hi)
}

/// Korea heights compiled into the utility (`assets/korea_100m.hgt`).
pub fn builtin_bytes() -> &'static [u8] {
    include_bytes!("../assets/korea_100m.hgt")
}

fn store_for_korea(bytes: &[u8], from: &str) -> Result<HeightStore, String> {
    let store = HeightStore::from_bytes(bytes).map_err(|e| format!("{from}: {e}"))?;
    if store.map_id != KOREA_MAP_ID {
        return Err(format!("{from} holds heights for {}, not {KOREA_MAP_ID}", store.map_id));
    }
    Ok(store)
}

/// The height store shipped with this build.
pub fn builtin() -> Result<HeightStore, String> {
    store_for_korea(builtin_bytes(), "built-in height store")
}

/// Baked heights, with `path` merged on top when that file exists and is newer.
///
/// A missing file is the baked store alone. Bytes identical to the baked
/// store are parsed once. HeightHelper still writes its own shard files;
/// this is only how the utility reads a store.
pub fn open_store(path: &Path) -> Result<HeightStore, String> {
    let baked = builtin_bytes();
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            store_for_korea(baked, "built-in height store")
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
        Ok(disk) if disk.as_slice() == baked => store_for_korea(&disk, "built-in height store"),
        Ok(disk) => {
            let mut store = store_for_korea(baked, "built-in height store")?;
            let extra = store_for_korea(&disk, &path.display().to_string())?;
            store
                .merge_from(&extra)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(store)
        }
    }
}

/// `%APPDATA%\IL2MissionUtility\terrain\korea_100m.hgt` (next to the exe if
/// `APPDATA` is unset). Extra measurements live here and are merged over
/// [`builtin`] by [`open_store`].
pub fn default_store_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok()?.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("IL2MissionUtility").join("terrain").join("korea_100m.hgt")
}

/// What [`HeightStore::merge_from`] kept from the incoming store.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MergeReport {
    pub added: usize,
    pub same: usize,
    pub points_added: usize,
    pub points_same: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredPoint {
    pub x: f64,
    pub z: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeightStore {
    pub map_id: String,
    tiles: HashMap<(u16, u16), Vec<i16>>,
    points: Vec<MeasuredPoint>,
}

impl HeightStore {
    pub fn new(map_id: &str) -> Self {
        Self { map_id: map_id.to_string(), tiles: HashMap::new(), points: Vec::new() }
    }

    fn slot(i: usize, j: usize) -> Option<((u16, u16), usize)> {
        if i >= LATTICE_N || j >= LATTICE_N {
            return None;
        }
        let (ti, tj) = tile_of_node(i, j);
        let r = (LATTICE_N - 1 - i) % TILE;
        let c = j % TILE;
        Some(((ti as u16, tj as u16), r * TILE + c))
    }

    /// Measured height of lattice node (i, j).
    pub fn node(&self, i: usize, j: usize) -> Option<f64> {
        let (key, k) = Self::slot(i, j)?;
        let v = *self.tiles.get(&key)?.get(k)?;
        (v != UNKNOWN).then(|| f64::from(v) / 10.0)
    }

    pub fn set_node(&mut self, i: usize, j: usize, y: f64) {
        let Some((key, k)) = Self::slot(i, j) else { return };
        let dm = (y * 10.0).round().clamp(f64::from(i16::MIN + 1), f64::from(i16::MAX)) as i16;
        self.tiles.entry(key).or_insert_with(|| vec![UNKNOWN; TILE * TILE])[k] = dm;
    }

    /// Keep an off-lattice measurement (replaces an earlier one within 1 m).
    pub fn add_point(&mut self, x: f64, z: f64, y: f64) {
        if let Some(p) = self.points.iter_mut().find(|p| (p.x - x).hypot(p.z - z) < 1.0) {
            p.y = y;
        } else {
            self.points.push(MeasuredPoint { x, z, y });
        }
    }

    pub fn points(&self) -> &[MeasuredPoint] {
        &self.points
    }

    /// Ground height at world (x, z), or `None` where nothing is measured.
    pub fn height_at(&self, x: f64, z: f64) -> Option<f64> {
        self.lookup(x, z).map(|h| h.y)
    }

    /// Ground height plus how it was measured.
    ///
    /// A measured point within `POINT_RADIUS_M` answers directly (highest if
    /// several). Otherwise the finest fully measured lattice cell around
    /// (x, z) is interpolated bilinearly: 100 m, then the 200 / 400 / 800 m
    /// cells the coarser probe passes fill. With no full cell, the highest
    /// measured corner of the 100 m cell is used — a unit placed high
    /// settles, one placed low ends up underground.
    pub fn lookup(&self, x: f64, z: f64) -> Option<TerrainHeight> {
        let near = self
            .points
            .iter()
            .filter(|p| (p.x - x).hypot(p.z - z) <= POINT_RADIUS_M)
            .map(|p| p.y)
            .reduce(f64::max);
        if let Some(y) = near {
            return Some(TerrainHeight { y, spacing_m: 0.0 });
        }
        if !(0.0..=MAP_MAX).contains(&x) || !(0.0..=MAP_MAX).contains(&z) {
            return None;
        }
        let (fi, fj) = (x / STEP_M, z / STEP_M);
        for k in LOOKUP_STRIDES {
            let i0 = ((fi / k as f64).floor() as usize * k).min(LATTICE_N - 1 - k);
            let j0 = ((fj / k as f64).floor() as usize * k).min(LATTICE_N - 1 - k);
            let q = [self.node(i0, j0), self.node(i0, j0 + k), self.node(i0 + k, j0), self.node(i0 + k, j0 + k)];
            if let [Some(a), Some(b), Some(c), Some(d)] = q {
                let (ti, tj) = ((fi - i0 as f64) / k as f64, (fj - j0 as f64) / k as f64);
                let y = a * (1.0 - ti) * (1.0 - tj) + b * (1.0 - ti) * tj + c * ti * (1.0 - tj) + d * ti * tj;
                return Some(TerrainHeight { y, spacing_m: k as f64 * STEP_M });
            }
        }
        let i0 = (fi.floor() as usize).min(LATTICE_N - 2);
        let j0 = (fj.floor() as usize).min(LATTICE_N - 2);
        [self.node(i0, j0), self.node(i0, j0 + 1), self.node(i0 + 1, j0), self.node(i0 + 1, j0 + 1)]
            .into_iter()
            .flatten()
            .reduce(f64::max)
            .map(|y| TerrainHeight { y, spacing_m: STEP_M })
    }

    pub fn measured_nodes(&self) -> usize {
        self.tiles.values().map(|t| t.iter().filter(|&&v| v != UNKNOWN).count()).sum()
    }

    /// Measured nodes inside tile (ti, tj).
    pub fn tile_measured(&self, ti: usize, tj: usize) -> usize {
        self.tiles
            .get(&(ti as u16, tj as u16))
            .map_or(0, |t| t.iter().filter(|&&v| v != UNKNOWN).count())
    }

    /// `HGT1`, map id, tiles (row, col, TILE² i16), then measured points.
    /// All integers little-endian.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.tiles.len() * TILE * TILE * 2);
        out.extend_from_slice(MAGIC);
        for v in [STEP_M as u32, LATTICE_N as u32, TILE as u32, self.map_id.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(self.map_id.as_bytes());
        let mut keys: Vec<_> = self.tiles.keys().copied().collect();
        keys.sort_unstable();
        out.extend_from_slice(&(keys.len() as u32).to_le_bytes());
        for key in keys {
            out.extend_from_slice(&key.0.to_le_bytes());
            out.extend_from_slice(&key.1.to_le_bytes());
            for v in &self.tiles[&key] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.points.len() as u32).to_le_bytes());
        for p in &self.points {
            for v in [p.x, p.z, p.y] {
                out.extend_from_slice(&(v as f32).to_le_bytes());
            }
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut r = Reader { b: bytes, at: 0 };
        if r.take(4)? != MAGIC {
            return Err("not a terrain height store (missing HGT1 header)".into());
        }
        let (step, n, tile, id_len) = (r.u32()?, r.u32()?, r.u32()?, r.u32()? as usize);
        if step != STEP_M as u32 || n != LATTICE_N as u32 || tile != TILE as u32 {
            return Err(format!("height store lattice {step} m / {n} / {tile} does not match this build"));
        }
        let map_id = String::from_utf8(r.take(id_len)?.to_vec()).map_err(|_| "bad map id".to_string())?;
        let mut store = Self::new(&map_id);
        for _ in 0..r.u32()? {
            let key = (r.u16()?, r.u16()?);
            let cells = r.take(TILE * TILE * 2)?;
            let tile = cells.as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes(*c)).collect();
            store.tiles.insert(key, tile);
        }
        for _ in 0..r.u32()? {
            let (x, z, y) = (r.f32()?, r.f32()?, r.f32()?);
            store.points.push(MeasuredPoint { x: x.into(), z: z.into(), y: y.into() });
        }
        Ok(store)
    }

    /// Load `path`, or an empty store for `map_id` if the file does not exist.
    pub fn load(path: &Path, map_id: &str) -> Result<Self, String> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let store = Self::from_bytes(&bytes)?;
                if store.map_id != map_id {
                    return Err(format!("{} holds heights for {}, not {map_id}", path.display(), store.map_id));
                }
                Ok(store)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::new(map_id)),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    /// Fold `other` into this store. Cells measured in only one store are kept.
    /// The same decimetre in both is kept. Two different heights is an error,
    /// and nothing is written when that happens, so a split harvest can be
    /// joined only when the shards do not disagree.
    pub fn merge_from(&mut self, other: &Self) -> Result<MergeReport, String> {
        if self.map_id != other.map_id {
            return Err(format!("cannot merge heights for {} into {}", other.map_id, self.map_id));
        }
        let mut rep = MergeReport::default();
        let mut fills: Vec<((u16, u16), usize, i16)> = Vec::new();
        for (&key, cells) in &other.tiles {
            if cells.len() != TILE * TILE {
                return Err(format!("tile {}/{} has a bad length", key.0, key.1));
            }
            let dest = self.tiles.get(&key);
            for (k, &src) in cells.iter().enumerate() {
                if src == UNKNOWN {
                    continue;
                }
                match dest.and_then(|d| d.get(k)).copied().unwrap_or(UNKNOWN) {
                    UNKNOWN => {
                        fills.push((key, k, src));
                        rep.added += 1;
                    }
                    have if have == src => rep.same += 1,
                    have => {
                        let (i, j) = node_of_tile(key.0 as usize, key.1 as usize, k / TILE, k % TILE);
                        return Err(format!(
                            "node ({i},{j}) is {:.1} m in one store and {:.1} m in another",
                            f64::from(have) / 10.0,
                            f64::from(src) / 10.0
                        ));
                    }
                }
            }
        }
        let mut extra_points = Vec::new();
        for p in &other.points {
            if let Some(have) = self.points.iter().find(|q| (q.x - p.x).hypot(q.z - p.z) < 1.0) {
                if (have.y - p.y).abs() > 0.05 {
                    return Err(format!(
                        "point ({:.1}, {:.1}) is {:.1} m in one store and {:.1} m in another",
                        have.x, have.z, have.y, p.y
                    ));
                }
                rep.points_same += 1;
            } else {
                extra_points.push(*p);
                rep.points_added += 1;
            }
        }
        for (key, k, src) in fills {
            let dest = self.tiles.entry(key).or_insert_with(|| vec![UNKNOWN; TILE * TILE]);
            dest[k] = src;
        }
        self.points.extend(extra_points);
        Ok(rep)
    }

    /// Write via a temp file + rename so a crash never leaves half a store.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = path.with_extension("hgt.tmp");
        std::fs::write(&tmp, self.to_bytes()).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len()).ok_or("height store is truncated")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lattice_constants_match_probe_files() {
        assert_eq!(LATTICE_N, 4993);
        assert_eq!(TILES_PER_SIDE, 23);
        // HG100_T05_16 spans X 364,900–387,200 and Z 358,400–380,700 (tiles.csv).
        assert_eq!(tile_node_range(5, 16), (3649, 3872, 3584, 3807));
        assert_eq!(tile_of_node(3649, 3584), (5, 16));
        assert_eq!(tile_of_node(3872, 3807), (5, 16));
        assert_eq!(node_of_tile(5, 16, 0, 0), (3872, 3584));
    }

    #[test]
    fn node_round_trip_and_unknown() {
        let mut s = HeightStore::new(KOREA_MAP_ID);
        assert_eq!(s.node(10, 20), None);
        s.set_node(10, 20, 123.46);
        assert_eq!(s.node(10, 20), Some(123.5));
        assert_eq!(s.node(10, 21), None);
        assert_eq!(s.measured_nodes(), 1);
        let (ti, tj) = tile_of_node(10, 20);
        assert_eq!(s.tile_measured(ti, tj), 1);
    }

    #[test]
    fn height_at_interpolates_between_four_nodes() {
        let mut s = HeightStore::new(KOREA_MAP_ID);
        s.set_node(1000, 2000, 100.0);
        s.set_node(1000, 2001, 200.0);
        s.set_node(1001, 2000, 300.0);
        s.set_node(1001, 2001, 400.0);
        assert_eq!(s.height_at(100_000.0, 200_000.0), Some(100.0));
        assert_eq!(s.height_at(100_050.0, 200_050.0), Some(250.0));
        assert_eq!(s.height_at(100_000.0, 200_050.0), Some(150.0));
    }

    #[test]
    fn partial_cell_uses_highest_known_and_empty_is_none() {
        let mut s = HeightStore::new(KOREA_MAP_ID);
        s.set_node(1000, 2000, 100.0);
        s.set_node(1001, 2001, 180.0);
        assert_eq!(s.height_at(100_050.0, 200_050.0), Some(180.0));
        assert_eq!(s.height_at(300_050.0, 300_050.0), None);
        assert_eq!(s.height_at(-5.0, 100.0), None);
    }

    #[test]
    fn lookup_falls_back_to_coarser_passes() {
        let mut s = HeightStore::new(KOREA_MAP_ID);
        // an 800 m survey cell (stride 8) around (100,000 / 200,000)
        for (i, j, y) in [(1000, 2000, 100.0), (1000, 2008, 180.0), (1008, 2000, 260.0), (1008, 2008, 340.0)] {
            s.set_node(i, j, y);
        }
        let h = s.lookup(100_400.0, 200_400.0).unwrap();
        assert_eq!(h, TerrainHeight { y: 220.0, spacing_m: 800.0 });
        assert_eq!(ground_margin_m(h.spacing_m), None, "800 m is an estimate only");
        // a 200 m base cell inside it wins once measured
        for (i, j) in [(1004, 2004), (1004, 2006), (1006, 2004), (1006, 2006)] {
            s.set_node(i, j, 500.0);
        }
        let h = s.lookup(100_500.0, 200_500.0).unwrap();
        assert_eq!(h, TerrainHeight { y: 500.0, spacing_m: 200.0 });
        assert_eq!(ground_margin_m(h.spacing_m), Some(30.0));
        assert_eq!(ground_margin_m(100.0), Some(DEFAULT_GROUND_MARGIN_M));
        assert_eq!(ground_margin_m(0.0), Some(5.0));
    }

    #[test]
    fn measured_point_wins_nearby() {
        let mut s = HeightStore::new(KOREA_MAP_ID);
        for (i, j) in [(1000, 2000), (1000, 2001), (1001, 2000), (1001, 2001)] {
            s.set_node(i, j, 100.0);
        }
        s.add_point(100_040.0, 200_040.0, 131.0);
        assert_eq!(s.height_at(100_050.0, 200_050.0), Some(131.0));
        assert_eq!(s.height_at(100_090.0, 200_090.0), Some(100.0));
        s.add_point(100_040.3, 200_040.0, 132.0);
        assert_eq!(s.points().len(), 1);
    }

    #[test]
    fn bytes_round_trip() {
        let mut s = HeightStore::new(KOREA_MAP_ID);
        s.set_node(0, 0, -3.2);
        s.set_node(4992, 4992, 2500.0);
        s.set_node(3700, 3600, 512.3);
        s.add_point(1.5, 2.5, 3.5);
        let back = HeightStore::from_bytes(&s.to_bytes()).unwrap();
        assert_eq!(back, s);
        assert!(HeightStore::from_bytes(b"NOPE").is_err());
        assert!(HeightStore::from_bytes(&s.to_bytes()[..40]).is_err());
    }

    #[test]
    fn save_load_and_map_mismatch() {
        let dir = std::env::temp_dir().join(format!("il2_terrain_test_{}", std::process::id()));
        let path = dir.join("store.hgt");
        let mut s = HeightStore::new(KOREA_MAP_ID);
        s.set_node(5, 5, 42.0);
        s.save(&path).unwrap();
        assert_eq!(HeightStore::load(&path, KOREA_MAP_ID).unwrap(), s);
        assert!(HeightStore::load(&path, "graphics\\LANDSCAPE_Other").is_err());
        let missing = HeightStore::load(&dir.join("none.hgt"), KOREA_MAP_ID).unwrap();
        assert_eq!(missing.measured_nodes(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_joins_shards_and_rejects_disagreement() {
        let mut a = HeightStore::new(KOREA_MAP_ID);
        let mut b = HeightStore::new(KOREA_MAP_ID);
        a.set_node(10, 20, 100.0);
        b.set_node(11, 20, 250.0);
        b.add_point(15.0, 25.0, 8.0);
        let rep = a.merge_from(&b).unwrap();
        assert_eq!(rep, MergeReport { added: 1, same: 0, points_added: 1, points_same: 0 });
        assert_eq!(a.node(10, 20), Some(100.0));
        assert_eq!(a.node(11, 20), Some(250.0));
        assert_eq!(a.points().len(), 1);

        let mut again = HeightStore::new(KOREA_MAP_ID);
        again.set_node(10, 20, 100.0);
        let rep = a.merge_from(&again).unwrap();
        assert_eq!(rep.same, 1);
        assert_eq!(a.measured_nodes(), 2);

        let mut clash = HeightStore::new(KOREA_MAP_ID);
        clash.set_node(10, 20, 180.0);
        let err = a.merge_from(&clash).unwrap_err();
        assert!(err.contains("node (10,20)"), "{err}");
        assert_eq!(a.node(10, 20), Some(100.0), "a failed merge leaves the store as it was");

        let mut other = HeightStore::new(r"graphics\LANDSCAPE_Other");
        assert!(a.merge_from(&other).is_err());
        other.map_id = KOREA_MAP_ID.to_string();
        other.add_point(15.0, 25.0, 40.0);
        assert!(a.merge_from(&other).unwrap_err().contains("point"));
    }

    #[test]
    fn builtin_store_is_measured_and_disk_merges_over_it() {
        let baked = builtin().expect("assets/korea_100m.hgt");
        assert!(
            baked.measured_nodes() > 100_000,
            "baked store should carry the harvested land heights"
        );
        let dir = std::env::temp_dir().join(format!("il2_builtin_merge_{}", std::process::id()));
        let path = dir.join("extra.hgt");
        let mut extra = HeightStore::new(KOREA_MAP_ID);
        extra.add_point(12.5, 34.5, 56.5);
        extra.save(&path).unwrap();
        let opened = open_store(&path).unwrap();
        assert!(opened.measured_nodes() >= baked.measured_nodes());
        assert!(opened.points().iter().any(|p| (p.y - 56.5).abs() < 0.05));
        let missing = open_store(&dir.join("none.hgt")).unwrap();
        assert_eq!(missing.measured_nodes(), baked.measured_nodes());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
