//! Which press of a quick row of presses a press is: the rule that text areas
//! and rich editors each keep a copy of.

use super::NodeId;
use core::time::Duration;
use glassmere_scene::Point;

/// A press follows the last one, in the same row, when it comes strictly
/// under this long after it ...
const WITHIN: Duration = Duration::from_millis(400);
/// ... and strictly under this many logical pixels away from it on each axis.
const NEAR: f32 = 4.0;

/// The last press: when it was, where, on which node, and which of its row it
/// was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Press {
    pub when: Duration,
    pub point: Point,
    pub node: Option<NodeId>,
    pub count: u8,
}

/// Which of its row the press at `point`, `now`, on `node` is: 1, 2 or 3. A
/// press on another node starts a row, however near the last one it is.
pub(super) fn next(last: Option<Press>, now: Duration, point: Point, node: Option<NodeId>) -> u8 {
    match last {
        Some(last)
            if last.node == node
                && now.saturating_sub(last.when) < WITHIN
                && (last.point.x - point.x).abs() < NEAR
                && (last.point.y - point.y).abs() < NEAR =>
        {
            last.count % 3 + 1
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Point = Point { x: 50.0, y: 20.0 };
    const NODE: Option<NodeId> = Some(NodeId(7));

    fn press(when: u64, point: Point, count: u8) -> Option<Press> {
        Some(Press {
            when: Duration::from_millis(when),
            point,
            node: NODE,
            count,
        })
    }

    fn at(x: f32, y: f32) -> Point {
        Point { x, y }
    }

    #[test]
    fn a_first_press_is_the_first_of_its_row() {
        assert_eq!(next(None, Duration::from_millis(5), AT, NODE), 1);
    }

    #[test]
    fn a_press_just_inside_both_limits_is_the_next_of_the_row() {
        let last = press(1000, AT, 1);
        let now = Duration::from_millis(1399);
        assert_eq!(next(last, now, at(53.9, 23.9), NODE), 2);
        assert_eq!(next(last, now, at(46.1, 16.1), NODE), 2);
        assert_eq!(next(press(1000, AT, 2), now, AT, NODE), 3);
    }

    #[test]
    fn a_press_at_the_time_limit_starts_a_row() {
        let last = press(1000, AT, 1);
        assert_eq!(next(last, Duration::from_millis(1400), AT, NODE), 1);
        assert_eq!(next(last, Duration::from_millis(5000), AT, NODE), 1);
    }

    #[test]
    fn a_press_at_the_distance_limit_on_either_axis_starts_a_row() {
        let last = press(1000, AT, 1);
        let now = Duration::from_millis(1100);
        assert_eq!(next(last, now, at(54.0, 20.0), NODE), 1);
        assert_eq!(next(last, now, at(46.0, 20.0), NODE), 1);
        assert_eq!(next(last, now, at(50.0, 24.0), NODE), 1);
        assert_eq!(next(last, now, at(50.0, 16.0), NODE), 1);
        assert_eq!(next(last, now, at(120.0, 90.0), NODE), 1);
    }

    #[test]
    fn a_row_counts_one_two_three_and_starts_again() {
        let mut last = None;
        let mut counts = [0; 5];
        for (step, slot) in counts.iter_mut().enumerate() {
            let now = Duration::from_millis(step as u64 * 100);
            *slot = next(last, now, AT, NODE);
            last = Some(Press {
                when: now,
                point: AT,
                node: NODE,
                count: *slot,
            });
        }
        assert_eq!(counts, [1, 2, 3, 1, 2]);
    }

    #[test]
    fn a_press_on_another_node_starts_a_row_however_near_it_is() {
        let last = press(1000, AT, 1);
        let now = Duration::from_millis(1100);
        assert_eq!(next(last, now, AT, Some(NodeId(8))), 1);
        assert_eq!(next(last, now, AT, None), 1);
        assert_eq!(next(press(1000, AT, 2), now, AT, Some(NodeId(8))), 1);
        // And the row goes on from there.
        let moved = Some(Press {
            node: Some(NodeId(8)),
            ..last.unwrap()
        });
        assert_eq!(next(moved, now, AT, Some(NodeId(8))), 2);
    }

    #[test]
    fn a_clock_that_went_back_still_counts() {
        let last = press(1000, AT, 1);
        assert_eq!(next(last, Duration::from_millis(900), AT, NODE), 2);
        assert_eq!(next(last, Duration::ZERO, AT, NODE), 2);
    }
}
