//! Two-point Korea map projection (lat/lon → XPos/ZPos).
//!
//! A line that varies `XPos` runs north–south, so **XPos is north** and
//! **ZPos is east**.

pub const MAP_MIN: f64 = 0.0;
pub const MAP_MAX: f64 = 499_200.0;

/// Seoul Reference
pub const REF_LAT: f64 = 37.5665;
pub const REF_LON: f64 = 126.9780;
pub const REF_X: f64 = 107_177.0;
pub const REF_Z: f64 = 286_221.0;

/// Sinuiju Reference
pub const CAL_LAT: f64 = 40.1006;
pub const CAL_LON: f64 = 124.3981;
pub const CAL_X: f64 = 389_918.0;
pub const CAL_Z: f64 = 61_245.0;

fn scale_x_per_lat() -> f64 {
    (CAL_X - REF_X) / (CAL_LAT - REF_LAT)
}

fn scale_z_per_lon_at(lat: f64) -> f64 {
    let dlon = CAL_LON - REF_LON;
    let raw = (CAL_Z - REF_Z) / dlon;
    // Fit the meridional scale at Sinuiju so both control points match, then
    // vary with cosine of latitude.
    let k = raw / CAL_LAT.to_radians().cos();
    k * lat.to_radians().cos()
}

/// Map X/Z from WGS84 for drawing front lines, using a simple spherical projection.
pub fn latlon_to_xz(lat: f64, lon: f64) -> (f64, f64) {
    let x = REF_X + (lat - REF_LAT) * scale_x_per_lat();
    let z = REF_Z + (lon - REF_LON) * scale_z_per_lon_at(lat);
    (x, z)
}

pub fn on_map(x: f64, z: f64) -> bool {
    (MAP_MIN..=MAP_MAX).contains(&x) && (MAP_MIN..=MAP_MAX).contains(&z)
}

/// Vertices for a constant-latitude line (east–west: X fixed, Z varies).
pub fn parallel_line(lat: f64, step: f64) -> Vec<(f64, f64)> {
    let (x, _) = latlon_to_xz(lat, REF_LON);
    let mut out = Vec::new();
    let mut z = MAP_MIN;
    while z <= MAP_MAX + 0.5 {
        if on_map(x, z) {
            out.push((x, z));
        }
        z += step;
    }
    out
}

pub fn parallel_38_xz() -> Vec<(f64, f64)> {
    parallel_line(38.0, 5_000.0)
}

/// Preview-only city. We use exact game coordinates (X/Z) so visual labels
/// line up perfectly with the map image, bypassing cartographic projection errors.
#[derive(Clone, Copy, Debug)]
pub struct RefCity {
    pub name: &'static str,
    pub x: f64,
    pub z: f64,
    pub dprk: bool,
    /// Draw the label to the west of the marker (avoids pile-ups).
    pub label_left: bool,
}

pub const MAJOR_CITIES: &[RefCity] = &[
    // Hardcoded exact coordinates provided by user
    RefCity { name: "Seoul", x: 106815.0, z: 284406.0, dprk: false, label_left: false },
    RefCity { name: "Suwon", x: 74954.0, z: 289050.0, dprk: false, label_left: false },
    RefCity { name: "Sinuiju", x: 389078.0, z: 62514.0, dprk: true, label_left: false },
    RefCity { name: "Hamhung", x: 367981.0, z: 330062.0, dprk: true, label_left: false },
    RefCity { name: "Pyongyang", x: 268356.0, z: 177438.0, dprk: true, label_left: false },
    RefCity { name: "Inchon", x: 96170.0, z: 255800.0, dprk: false, label_left: true },
    RefCity { name: "Chunchon", x: 142827.0, z: 350759.0, dprk: false, label_left: false },
    RefCity { name: "Wonsan", x: 284808.0, z: 323252.0, dprk: true, label_left: false },
    RefCity { name: "Haeju", x: 159390.0, z: 174199.0, dprk: true, label_left: true },
    RefCity { name: "Chaeryong", x: 199432.0, z: 165872.0, dprk: true, label_left: false },
    RefCity { name: "Wonju", x: 84048.0, z: 372189.0, dprk: false, label_left: false },
    RefCity { name: "Kaesong", x: 151881.0, z: 247830.0, dprk: true, label_left: true },
    RefCity { name: "Kangnung", x: 131100.0, z: 454167.0, dprk: false, label_left: true },
    RefCity { name: "Chinnampo", x: 235687.0, z: 147139.0, dprk: true, label_left: true },
    RefCity { name: "Chongbong-ni", x: 173484.0, z: 423784.0, dprk: false, label_left: true },    
	RefCity { name: "Hanjon-dong", x: 231311.4, z: 247023.41, dprk: true, label_left: true },
];

#[derive(Clone, Copy, Debug)]
pub struct RefWaterway {
    pub name: &'static str,
    pub x: f64,
    pub z: f64,
}
//  X
//  |
//  |
//  |
//  +-------- Z

pub const MAJOR_WATERWAYS: &[RefWaterway] = &[
    RefWaterway { name: "Yellow Sea", x: 100_000.0, z: 60_000.0 },
    RefWaterway { name: "Sea of Japan", x: 250_000.0, z: 430_000.0 },
    // Off Map -- RefWaterway { name: "Korea Strait", x: 20_000.0, z: 250_000.0 },
    // Retaining a dedicated label for the Yalu River near Sinuiju
    RefWaterway { name: "Yalu River", x: 405_000.0, z: 90_000.0 }, //*
	RefWaterway { name: "Ch'ongch'on River", x: 336_000.0, z: 170_000.0 },
	RefWaterway { name: "Taedong River", x: 252_000.0, z: 162_000.0 }, //*
	RefWaterway { name: "Yesong River", x: 165_000.0, z: 230_000.0 },
	RefWaterway { name: "Imjin River", x: 150_000.0, z: 275_000.0 }, //*
	RefWaterway { name: "Han River", x: 130_000.0, z: 250_000.0 }, //*
	RefWaterway { name: "Bukhan River", x: 125_000.0, z: 325_000.0 }, //*
	RefWaterway { name: "Soyang River", x: 158_000.0, z: 380_000.0 }, 
];

/// Lower Yalu (Amnok), mouth at Korea Bay up toward Chosan.
pub const YALU_LATLON: &[(f64, f64)] = &[
    (39.80, 124.28), (39.86, 124.32), (39.95, 124.36),
    (40.10, 124.40), (40.20, 124.53), (40.28, 124.65),
    (40.36, 124.80), (40.46, 124.96), (40.54, 125.15),
    (40.62, 125.38), (40.70, 125.58), (40.82, 125.80),
];

pub fn yalu_river_xz() -> Vec<(f64, f64)> {
    YALU_LATLON
        .iter()
        .map(|&(lat, lon)| latlon_to_xz(lat, lon))
        .collect()
}

pub fn yalu_x_at_z(z: f64) -> Option<f64> {
    let pts = yalu_river_xz();
    if pts.len() < 2 {
        return None;
    }
    if z <= pts[0].1 {
        return Some(pts[0].0);
    }
    if let Some(last) = pts.last() {
        if z >= last.1 {
            return Some(last.0);
        }
    }
    for w in pts.windows(2) {
        let (x0, z0) = w[0];
        let (x1, z1) = w[1];
        let lo = z0.min(z1);
        let hi = z0.max(z1);
        if z >= lo && z <= hi {
            let denom = z1 - z0;
            if denom.abs() < 1e-6 {
                return Some(x0.max(x1));
            }
            let t = ((z - z0) / denom).clamp(0.0, 1.0);
            return Some(x0 + (x1 - x0) * t);
        }
    }
    Some(pts.last().unwrap().0)
}

pub fn cities_on_map() -> Vec<(&'static RefCity, f64, f64)> {
    MAJOR_CITIES
        .iter()
        .filter(|c| on_map(c.x, c.z))
        .map(|c| (c, c.x, c.z))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_and_anchor_match_exactly() {
        let (x, z) = latlon_to_xz(REF_LAT, REF_LON);
        assert!((x - REF_X).abs() < 0.001);
        assert!((z - REF_Z).abs() < 0.001);
        
        let (x2, z2) = latlon_to_xz(CAL_LAT, CAL_LON);
        assert!((x2 - CAL_X).abs() < 0.001);
        assert!((z2 - CAL_Z).abs() < 0.001);
    }

    #[test]
    fn major_cities_split_north_south_and_stay_on_map() {
        let cities = cities_on_map();
        assert!(cities.len() >= 12, "expected most listed cities on the Korea map");
        let seoul = cities.iter().find(|(c, ..)| c.name == "Seoul").unwrap();
        assert!(!seoul.0.dprk);
        let pyongyang = cities.iter().find(|(c, ..)| c.name == "Pyongyang").unwrap();
        assert!(pyongyang.0.dprk);
    }

    #[test]
    fn yalu_northing_is_defined_across_the_map_easting() {
        let mut z = MAP_MIN;
        while z <= MAP_MAX {
            assert!(
                yalu_x_at_z(z).is_some(),
                "Yalu northing must be defined at z={z} so south AoI cannot cross into China"
            );
            z += 25_000.0;
        }
    }
}