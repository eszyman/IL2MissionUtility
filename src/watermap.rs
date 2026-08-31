//! Packed Korea terrain mask (`assets/combined_terrain.bin`).
//!
//! File layout: `WMAP` + width:u32 LE + height:u32 LE + width*height bytes.
//! Pixel (0,0) is the north-west corner of the mission square.
//!
//! Each byte is a bitfield:
//! - water: `packed & 1 != 0`
//! - road:  `packed & 2 != 0` (reserved for later)
//! - open:  `packed & 4 != 0`

use crate::geo::{MAP_MAX, MAP_MIN};

pub const FLAG_WATER: u8 = 1;
pub const FLAG_ROAD: u8 = 2;
pub const FLAG_OPEN: u8 = 4;

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
        match MAP.get_or_init(|| Self::from_bytes(include_bytes!("../assets/combined_terrain.bin"))) {
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
        b & FLAG_OPEN != 0 && b & FLAG_WATER == 0
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
        assert!(!map.is_open_cell(1, 1), "water+open should not host ground");
        assert!(map.is_water_cell(1, 1));
        assert!(!map.is_water_cell(2, 0));
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
            map.grid.iter().any(|b| b & FLAG_OPEN != 0 && b & FLAG_WATER == 0),
            "terrain mask should include dry open ground"
        );
    }
}
