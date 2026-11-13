//! Decides when the pointer is "going to the left edge", so the sidebar can show before the
//! pointer gets there: touching the edge always counts, and so does a quick move towards it.

/// Touching this close to the edge always shows the sidebar, in pixels.
pub const TOUCH: f64 = 10.0;
/// A quick flick towards the edge counts from this far away.
pub const FLICK_ZONE: f64 = 140.0;
/// A slower, deliberate move counts only close to the edge.
pub const NEAR_ZONE: f64 = 48.0;
/// Leftward speeds, in pixels per millisecond.
const FLICK_SPEED: f64 = 1.4;
const NEAR_SPEED: f64 = 0.45;
/// Samples further apart than this say nothing about speed, in microseconds.
const STALE_US: i64 = 120_000;

#[derive(Debug, Default, Clone, Copy)]
pub struct EdgeDetector {
    last: Option<(f64, i64)>,
}

impl EdgeDetector {
    /// Feeds a pointer position (x in pixels from the left edge of the window, time in
    /// microseconds) and says whether the sidebar should show.
    pub fn feed(&mut self, x: f64, time_us: i64) -> bool {
        let previous = self.last.replace((x, time_us));
        if x <= TOUCH {
            return true;
        }
        let Some((last_x, last_t)) = previous else {
            return false;
        };
        let dt = time_us - last_t;
        if dt <= 0 || dt > STALE_US {
            return false;
        }
        // Positive: towards the left edge.
        let speed = (last_x - x) / (dt as f64 / 1000.0);
        (x <= FLICK_ZONE && speed >= FLICK_SPEED) || (x <= NEAR_ZONE && speed >= NEAR_SPEED)
    }

    /// Forget the last position, e.g. when the pointer left the window.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(points: &[(f64, i64)]) -> bool {
        let mut d = EdgeDetector::default();
        points.iter().fold(false, |_, &(x, t)| d.feed(x, t))
    }

    #[test]
    fn touching_the_edge_shows_it() {
        assert!(run(&[(400.0, 0), (3.0, 500_000)]));
    }

    #[test]
    fn a_quick_move_to_the_side_shows_it_before_the_edge() {
        // 120 px in 50 ms is 2.4 px/ms: a flick.
        assert!(run(&[(220.0, 0), (100.0, 50_000)]));
    }

    #[test]
    fn a_slow_drift_does_not() {
        assert!(!run(&[(220.0, 0), (200.0, 50_000), (180.0, 100_000)]));
        // Slow and close, but below the deliberate-move speed.
        assert!(!run(&[(60.0, 0), (55.0, 50_000)]));
    }

    #[test]
    fn moving_away_from_the_edge_never_shows_it() {
        assert!(!run(&[(20.0, 0), (140.0, 30_000)]));
    }

    #[test]
    fn a_deliberate_move_close_to_the_edge_shows_it() {
        // 30 px in 50 ms is 0.6 px/ms, ending 40 px from the edge.
        assert!(run(&[(70.0, 0), (40.0, 50_000)]));
    }

    #[test]
    fn a_flick_far_from_the_edge_is_ignored() {
        assert!(!run(&[(900.0, 0), (700.0, 50_000)]));
    }

    #[test]
    fn old_samples_say_nothing_about_speed() {
        assert!(!run(&[(300.0, 0), (100.0, 400_000)]));
    }

    #[test]
    fn reset_forgets_the_last_position() {
        let mut d = EdgeDetector::default();
        d.feed(200.0, 0);
        d.reset();
        assert!(!d.feed(100.0, 10_000));
    }
}
