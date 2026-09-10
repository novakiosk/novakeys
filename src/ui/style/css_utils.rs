use crate::constants::geometry::{MIN_KEY_HEIGHT, ROW_GAP, VERTICAL_PADDING_TOTAL};

/// Divide usable height between rows, keeping the minimum touch-key height.
pub fn cal_geometry_unit(length: i32, count: i32) -> i32 {
    if count <= 0 {
        return MIN_KEY_HEIGHT;
    }

    let total_row_gaps = ROW_GAP * (count - 1).max(0);
    let available_height = length - VERTICAL_PADDING_TOTAL - total_row_gaps;

    (available_height / count).max(MIN_KEY_HEIGHT)
}
