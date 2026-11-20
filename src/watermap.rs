//! watermap.rs — packed Korea terrain mask (`assets/combined_terrain.bin`)
//!
//! File layout: `WMAP` + width:u32 LE + height:u32 LE + width×height
//! bytes. Pixel (0,0) is the north-west corner of the mission square
//! (`geo::MAP_MIN`/`MAP_MAX`). Each byte is a bitfield: water `& 1`,
//! road `& 2` (reserved), open `& 4`, woods `& 8`. `WaterMap` is a
//! historical alias for `TerrainMap`. It does not place units — ship and
//! ground code query `is_water_xz` / `is_open_xz` / `is_woods_xz` /
//! `is_land_xz`. A query is one byte. Nothing here opens an editor photograph.
//!
//! MapHelper writes water, road, and the coarse open bit. Woods, and the
//! open-bit corrections that follow from them, are baked offline from
//! `References/MapCapture` tiles:
//! `py -3 tools/mapcapture/bake_woods.py`
//! Run that again after MapHelper rebuilds the bin. The cell is about 111 m,
//! so a copse smaller than the cell can still read as open.
//!
//! ## Public API
//! * `FLAG_WATER` / `FLAG_ROAD` / `FLAG_OPEN` / `FLAG_WOODS`
//! * `struct TerrainMap` (`from_bytes`, `builtin`, cell / xz queries)
//! * `type WaterMap = TerrainMap`
//!
//! ## Used by
//! * ui.rs (Map) — `WaterMap::builtin()` for ship/ground preview
//! * mapshipping.rs — stay in water
//! * mapground.rs — armor/supply/artillery on dry open land; infantry on any dry land

use crate::geo::{MAP_MAX, MAP_MIN};

pub const FLAG_WATER: u8 = 1;
pub const FLAG_ROAD: u8 = 2;
pub const FLAG_OPEN: u8 = 4;
/// Olive woods on the editor map, baked by `tools/mapcapture/bake_woods.py`.
pub const FLAG_WOODS: u8 = 8;

pub struct TerrainMap {
    pub width: u32,
    pub height: u32,
    pub grid: Vec<u8>,
}

/// Historical name used by shipping placement.
pub type WaterMap = TerrainMap;

impl TerrainMap {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 12 {
            return Err("combined_terrain.bin is too short".into());
        }
        if &bytes[0..4] != b"WMAP" {
            return Err("combined_terrain.bin is missing the WMAP header".into());
        }
        let width = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let height = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if width == 0 || height == 0 {
            return Err("combined_terrain.bin has a zero-sized grid".into());
        }
        let need = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| "combined_terrain.bin dimensions overflow".to_string())?;
        if bytes.len() < 12 + need {
            return Err("combined_terrain.bin grid is truncated".into());
        }
        Ok(Self {
            width,
            height,
            grid: bytes[12..12 + need].to_vec(),
        })
    }

    pub fn builtin() -> Result<&'static Self, String> {
        use std::sync::OnceLock;
        static MAP: OnceLock<Result<TerrainMap, String>> = OnceLock::new();
        match MAP.get_or_init(|| Self::from_bytes(include_bytes!("../assets/combined_terrain.bin")))
        {
            Ok(map) => Ok(map),
            Err(err) => Err(err.clone()),
        }
    }

    #[inline]
    pub fn cell(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.grid[(y * self.width + x) as usize]
    }

    #[inline]
    pub fn is_water_cell(&self, x: u32, y: u32) -> bool {
        self.cell(x, y) & FLAG_WATER != 0
    }

    #[inline]
    pub fn is_open_cell(&self, x: u32, y: u32) -> bool {
        let b = self.cell(x, y);
        b & FLAG_OPEN != 0 && b & FLAG_WATER == 0 && b & FLAG_WOODS == 0
    }

    /// Photographed woods. Water wins, so a wet cell is never woods.
    #[inline]
    pub fn is_woods_cell(&self, x: u32, y: u32) -> bool {
        let b = self.cell(x, y);
        b & FLAG_WOODS != 0 && b & FLAG_WATER == 0
    }

    /// Dry cell (not water). Forests, hills, and clearings all count.
    #[inline]
    pub fn is_land_cell(&self, x: u32, y: u32) -> bool {
        self.cell(x, y) & FLAG_WATER == 0
    }

    #[inline]
    pub fn is_road_cell(&self, x: u32, y: u32) -> bool {
        self.cell(x, y) & FLAG_ROAD != 0
    }

    pub fn world_to_cell(&self, x: f64, z: f64) -> Option<(u32, u32)> {
        let span = MAP_MAX - MAP_MIN;
        if span <= 0.0 {
            return None;
        }
        let u = (z - MAP_MIN) / span;
        let v = (MAP_MAX - x) / span;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let gx = (u * self.width as f64).floor() as u32;
        let gy = (v * self.height as f64).floor() as u32;
        Some((gx.min(self.width - 1), gy.min(self.height - 1)))
    }

    pub fn is_water_xz(&self, x: f64, z: f64) -> bool {
        self.world_to_cell(x, z)
            .is_some_and(|(gx, gy)| self.is_water_cell(gx, gy))
    }

    pub fn is_open_xz(&self, x: f64, z: f64) -> bool {
        self.world_to_cell(x, z)
            .is_some_and(|(gx, gy)| self.is_open_cell(gx, gy))
    }

    pub fn is_woods_xz(&self, x: f64, z: f64) -> bool {
        self.world_to_cell(x, z)
            .is_some_and(|(gx, gy)| self.is_woods_cell(gx, gy))
    }

    pub fn is_land_xz(&self, x: f64, z: f64) -> bool {
        self.world_to_cell(x, z)
            .is_some_and(|(gx, gy)| self.is_land_cell(gx, gy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed(width: u32, height: u32, cells: &[u8]) -> Vec<u8> {
        let mut out = Vec::from(*b"WMAP");
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(cells);
        out
    }

    #[test]
    fn from_bytes_reads_header_and_bitflags() {
        let map = TerrainMap::from_bytes(&packed(2, 2, &[0, 1, 4, 5])).unwrap();
        assert_eq!(map.width, 2);
        assert_eq!(map.height, 2);
        assert!(!map.is_water_cell(0, 0));
        assert!(map.is_water_cell(1, 0));
        assert!(map.is_open_cell(0, 1));
        assert!(!map.is_woods_cell(0, 1));
        assert!(!map.is_open_cell(1, 1), "water+open should not host ground");
        assert!(map.is_water_cell(1, 1));
        assert!(map.is_land_cell(0, 0), "unmarked dry cell is land");
        assert!(map.is_land_cell(0, 1));
        assert!(!map.is_land_cell(1, 0));
        assert!(!map.is_land_cell(1, 1));
        assert!(!map.is_water_cell(2, 0));
    }

    #[test]
    fn woods_bit_closes_open_ground() {
        // open, woods, open+woods, water+woods
        let map = TerrainMap::from_bytes(&packed(2, 2, &[4, 8, 12, 9])).unwrap();
        assert!(map.is_open_cell(0, 0));
        assert!(!map.is_woods_cell(0, 0));
        assert!(map.is_woods_cell(1, 0));
        assert!(!map.is_open_cell(1, 0));
        assert!(
            map.is_woods_cell(0, 1) && !map.is_open_cell(0, 1),
            "woods closes a cell even when the open bit is still set"
        );
        assert!(!map.is_woods_cell(1, 1), "water is not woods");
        assert!(!map.is_open_cell(1, 1));
        assert!(map.is_water_cell(1, 1));
    }

    #[test]
    fn world_to_cell_maps_nw_and_se() {
        let map = TerrainMap::from_bytes(&packed(4, 4, &[0; 16])).unwrap();
        let (gx, gy) = map.world_to_cell(MAP_MAX - 1.0, MAP_MIN + 1.0).unwrap();
        assert_eq!((gx, gy), (0, 0));
        let (gx, gy) = map.world_to_cell(MAP_MIN + 1.0, MAP_MAX - 1.0).unwrap();
        assert_eq!((gx, gy), (3, 3));
    }

    #[test]
    fn builtin_mask_loads_with_water_and_open() {
        let map = TerrainMap::builtin().expect("combined_terrain.bin");
        assert!(map.width >= 64 && map.height >= 64);
        assert_eq!(map.grid.len(), map.width as usize * map.height as usize);
        assert!(
            map.grid.iter().any(|b| b & FLAG_WATER != 0),
            "terrain mask should include water"
        );
        assert!(
            map.grid
                .iter()
                .any(|b| b & FLAG_OPEN != 0 && b & FLAG_WATER == 0 && b & FLAG_WOODS == 0),
            "terrain mask should include dry open ground"
        );
        assert!(
            map.grid.iter().any(|b| b & FLAG_WOODS != 0),
            "terrain mask should include baked woods"
        );
        assert!(
            map.grid
                .iter()
                .all(|b| b & FLAG_WOODS == 0 || b & FLAG_WATER == 0),
            "water stays water, not woods"
        );
        assert!(
            map.grid
                .iter()
                .all(|b| b & FLAG_WOODS == 0 || b & FLAG_OPEN == 0),
            "a woods cell is not open"
        );
        let woods_at = map
            .grid
            .iter()
            .position(|b| b & FLAG_WOODS != 0)
            .expect("woods cell");
        let gx = (woods_at as u32) % map.width;
        let gy = (woods_at as u32) / map.width;
        let span = MAP_MAX - MAP_MIN;
        let z = (f64::from(gx) + 0.5) / f64::from(map.width) * span;
        let x = MAP_MAX - (f64::from(gy) + 0.5) / f64::from(map.height) * span;
        assert!(map.is_woods_xz(x, z));
        assert!(!map.is_open_xz(x, z));
        assert!(map.is_land_xz(x, z));
        // Cell centers from the baked editor tiles: an olive hill, and a beige valley.
        assert!(map.is_woods_xz(410_673.28, 136_372.52));
        assert!(!map.is_open_xz(410_673.28, 136_372.52));
        assert!(map.is_open_xz(398_066.11, 226_843.50));
        assert!(!map.is_woods_xz(398_066.11, 226_843.50));
        assert!(!map.is_water_xz(398_066.11, 226_843.50));
    }
}
