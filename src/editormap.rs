//! Editor map tiles photographed from the mission editor.
//!
//! The stitched TIFF is the same picture at full resolution and is too large
//! to keep open. This module reads `References/MapCapture`: one PNG per view,
//! plus the calibration that places it on the Korea square. The Map tab draws
//! the tiles that intersect the current zoom. Ground placement does not read
//! them. Open ground and woods come from the packed terrain mask. Woods are
//! baked into that mask offline (`py -3 tools/mapcapture/bake_woods.py`),
//! using the same olive and beige test as `classify_rgb`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static INSTALLED: OnceLock<EditorMap> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover {
    /// Green vegetation on the editor map.
    Wood,
    /// Beige field or open ground.
    Field,
}

#[derive(Clone, Debug)]
pub struct TileRef {
    pub row: i32,
    pub col: i32,
    pub z_left: f64,
    pub x_top: f64,
    pub path: PathBuf,
}

pub struct EditorMap {
    pub mpp_z: f64,
    pub mpp_x: f64,
    pub tile_w: u32,
    pub tile_h: u32,
    tiles: Vec<TileRef>,
    buckets: HashMap<(i32, i32), Vec<usize>>,
}

pub fn install(map: EditorMap) {
    let _ = INSTALLED.set(map);
}

pub fn with_installed<R>(f: impl FnOnce(&EditorMap) -> R) -> Option<R> {
    INSTALLED.get().map(f)
}

fn bucket_key(m: f64) -> i32 {
    (m / 20_000.0).floor() as i32
}

impl EditorMap {
    pub fn load_default() -> Option<Self> {
        let mut dirs = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd.join("References").join("MapCapture"));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                dirs.push(dir.join("References").join("MapCapture"));
            }
        }
        dirs.into_iter().find_map(|dir| Self::load(&dir).ok())
    }

    pub fn load(dir: &Path) -> Result<Self, String> {
        let cal = std::fs::read_to_string(dir.join("calibration.json"))
            .map_err(|e| format!("calibration.json: {e}"))?;
        let mpp_z = json_number(&cal, "mpp_z").ok_or("calibration is missing mpp_z")?;
        let mpp_x = json_number(&cal, "mpp_x").ok_or("calibration is missing mpp_x")?;
        let viewport = json_array4(&cal, "viewport").ok_or("calibration is missing viewport")?;
        let tile_w = viewport[2] as u32;
        let tile_h = viewport[3] as u32;
        if mpp_z <= 0.0 || mpp_x <= 0.0 || tile_w == 0 || tile_h == 0 {
            return Err("calibration has an empty view".into());
        }
        let index = std::fs::read_to_string(dir.join("tiles").join("index.jsonl"))
            .map_err(|e| format!("tiles/index.jsonl: {e}"))?;
        let mut tiles = Vec::new();
        for line in index.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some(file) = json_string(line, "file") else {
                continue;
            };
            let Some(row) = json_number(line, "row") else {
                continue;
            };
            let Some(col) = json_number(line, "col") else {
                continue;
            };
            let Some(z_left) = json_number(line, "z_left") else {
                continue;
            };
            let Some(x_top) = json_number(line, "x_top") else {
                continue;
            };
            tiles.push(TileRef {
                row: row as i32,
                col: col as i32,
                z_left,
                x_top,
                path: dir.join("tiles").join(file),
            });
        }
        if tiles.is_empty() {
            return Err("the tile index is empty".into());
        }
        tiles.sort_by_key(|t| (t.row, t.col));
        let map = Self {
            mpp_z,
            mpp_x,
            tile_w,
            tile_h,
            tiles,
            buckets: HashMap::new(),
        };
        let mut buckets: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, tile) in map.tiles.iter().enumerate() {
            let (x0, x1, z0, z1) = map.tile_bounds(tile);
            let mut ix = bucket_key(x0);
            while ix <= bucket_key(x1) {
                let mut iz = bucket_key(z0);
                while iz <= bucket_key(z1) {
                    buckets.entry((ix, iz)).or_default().push(i);
                    iz += 1;
                }
                ix += 1;
            }
        }
        Ok(Self { buckets, ..map })
    }

    pub fn tile_bounds(&self, tile: &TileRef) -> (f64, f64, f64, f64) {
        let z1 = tile.z_left + f64::from(self.tile_w) * self.mpp_z;
        let x0 = tile.x_top - f64::from(self.tile_h) * self.mpp_x;
        (x0, tile.x_top, tile.z_left, z1)
    }

    /// Tiles that meet the world box `(x0, x1, z0, z1)`. The bucket index
    /// keeps a pan from walking every tile on the map.
    pub fn tiles_touching(&self, x0: f64, x1: f64, z0: f64, z1: f64) -> Vec<&TileRef> {
        let hits = |tile: &TileRef| {
            let (sx, nx, wz, ez) = self.tile_bounds(tile);
            nx > x0 && sx < x1 && ez > z0 && wz < z1
        };
        if self.buckets.is_empty() {
            return self.tiles.iter().filter(|tile| hits(tile)).collect();
        }
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut ix = bucket_key(x0);
        let ix1 = bucket_key(x1);
        let iz1 = bucket_key(z1);
        while ix <= ix1 {
            let mut iz = bucket_key(z0);
            while iz <= iz1 {
                if let Some(indexes) = self.buckets.get(&(ix, iz)) {
                    for &i in indexes {
                        if seen.insert(i) && hits(&self.tiles[i]) {
                            out.push(&self.tiles[i]);
                        }
                    }
                }
                iz += 1;
            }
            ix += 1;
        }
        out
    }

}

/// One editor-map pixel. Contours, the grid, and type are left unclassified.
pub fn classify_rgb(r: u8, g: u8, b: u8) -> Option<Cover> {
    if r >= 205 && g >= 185 && b >= 160 && r as i16 + 8 >= g as i16 && r as i16 - b as i16 <= 60 {
        return Some(Cover::Field);
    }
    if g as i16 + 18 >= r as i16 && g as i16 > b as i16 + 25 && (140..215).contains(&g) && b < 165 {
        return Some(Cover::Wood);
    }
    None
}

fn json_number(text: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{key}\"");
    let i = text.find(&pat)?;
    let rest = text[i + pat.len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let end = rest
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e' || c == 'E' || c == '+')
        })
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn json_string(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let i = text.find(&pat)?;
    let rest = text[i + pat.len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn json_array4(text: &str, key: &str) -> Option<[f64; 4]> {
    let pat = format!("\"{key}\"");
    let i = text.find(&pat)?;
    let rest = text[i + pat.len()..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('[')?;
    let end = rest.find(']')?;
    let mut nums = rest[..end]
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok());
    Some([nums.next()?, nums.next()?, nums.next()?, nums.next()?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beige_is_field_and_olive_is_wood() {
        assert_eq!(classify_rgb(222, 207, 181), Some(Cover::Field));
        assert_eq!(classify_rgb(190, 190, 150), Some(Cover::Wood));
        assert_eq!(classify_rgb(180, 190, 140), Some(Cover::Wood));
        assert_eq!(classify_rgb(40, 30, 20), None);
        assert_eq!(classify_rgb(150, 155, 170), None);
    }

    #[test]
    fn tile_bounds_and_overlap_query() {
        let map = EditorMap {
            mpp_z: 10.0,
            mpp_x: 10.0,
            tile_w: 100,
            tile_h: 50,
            tiles: vec![TileRef {
                row: 0,
                col: 1,
                z_left: 1_000.0,
                x_top: 5_000.0,
                path: PathBuf::new(),
            }],
            buckets: HashMap::new(),
        };
        let (x0, x1, z0, z1) = map.tile_bounds(&map.tiles[0]);
        assert_eq!((x0, x1, z0, z1), (4_500.0, 5_000.0, 1_000.0, 2_000.0));
        assert!(map.tiles_touching(4_600.0, 4_700.0, 1_100.0, 1_200.0).len() == 1);
        assert!(map.tiles_touching(100.0, 200.0, 100.0, 200.0).is_empty());
    }

    #[test]
    fn index_numbers_round_trip() {
        let line =
            r#"{"row": 2, "col": 4, "file": "r0002_c0004.png", "z_left": 12.5, "x_top": 80.0}"#;
        assert_eq!(json_number(line, "row"), Some(2.0));
        assert_eq!(json_number(line, "z_left"), Some(12.5));
        assert_eq!(
            json_string(line, "file").as_deref(),
            Some("r0002_c0004.png")
        );
        let cal = "\"viewport\": [\n  1,\n  2,\n  1435,\n  901\n]";
        assert_eq!(
            json_array4(cal, "viewport"),
            Some([1.0, 2.0, 1435.0, 901.0])
        );
    }
}
