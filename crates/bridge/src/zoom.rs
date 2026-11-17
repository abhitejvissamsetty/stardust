//! Page zoom. CEF measures zoom as a level, where each step multiplies the size by 1.2,
//! and people see a percentage. Chrome moves through a fixed list of percentages.

/// Chrome's zoom percentages.
pub const ZOOM_STEPS: [i32; 17] = [
    25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500,
];

/// The percentage for one of CEF's zoom levels.
pub fn percent_for_level(level: f64) -> i32 {
    (1.2f64.powf(level) * 100.0).round() as i32
}

/// CEF's zoom level for a percentage.
pub fn level_for_percent(percent: i32) -> f64 {
    (f64::from(percent.max(1)) / 100.0).ln() / 1.2f64.ln()
}

/// The next step up or down from the current percentage. At either end it stays put.
pub fn next_zoom_percent(current: i32, zoom_in: bool) -> i32 {
    if zoom_in {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|step| *step > current)
            .unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1])
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|step| *step < current)
            .unwrap_or(ZOOM_STEPS[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_through_chromes_list() {
        assert_eq!(next_zoom_percent(100, true), 110);
        assert_eq!(next_zoom_percent(100, false), 90);
        assert_eq!(next_zoom_percent(25, false), 25);
        assert_eq!(next_zoom_percent(500, true), 500);
    }

    #[test]
    fn an_odd_percentage_moves_to_the_nearest_step() {
        // CEF levels don't always land on a listed percentage.
        assert_eq!(next_zoom_percent(104, true), 110);
        assert_eq!(next_zoom_percent(104, false), 100);
    }

    #[test]
    fn levels_and_percentages_agree() {
        assert_eq!(percent_for_level(0.0), 100);
        assert_eq!(percent_for_level(1.0), 120);
        for step in ZOOM_STEPS {
            assert_eq!(percent_for_level(level_for_percent(step)), step);
        }
    }
}
