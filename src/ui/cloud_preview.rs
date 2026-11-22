//! JPEG previews of the editor cloud list, captured on the Korea coast.
//!
//! `build.rs` fills `jpeg` from `assets/clouds`. Hours 7 through 19 use
//! `h07` … `h19` when those frames exist. Every other hour uses `night.jpg`
//! when that frame exists. Until a frame has been captured, the hour falls
//! back to the single picture of that cloud type.

include!(concat!(env!("OUT_DIR"), "/cloud_hours.rs"));

/// 8pm through 6am share one night frame. 7am through 7pm are hourly.
pub fn is_night_hour(hour: u32) -> bool {
    let hour = hour % 24;
    !(7..=19).contains(&hour)
}

#[cfg(test)]
mod tests {
    use super::is_night_hour;

    #[test]
    fn night_hours_share_one_frame_and_day_is_hourly() {
        for hour in [0, 1, 2, 3, 4, 5, 6, 20, 21, 22, 23] {
            assert!(is_night_hour(hour), "{hour}");
        }
        for hour in 7..=19 {
            assert!(!is_night_hour(hour), "{hour}");
        }
    }
}
