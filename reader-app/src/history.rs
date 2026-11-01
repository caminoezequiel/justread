//! Back/forward history of jumps (links, TOC entries, search hits), like a
//! web browser's: following a jump records where you were; going back
//! returns there and keeps the place you left for going forward again.

use std::collections::VecDeque;

use epub_core::Position;

/// At most this many places are kept for going back; older ones are dropped.
pub const LIMIT: usize = 50;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    /// Oldest first. A `VecDeque` is a ring buffer: pushing and popping at
    /// either end is cheap, so dropping the oldest entry doesn't shift the
    /// whole list the way `Vec::remove(0)` would.
    back: VecDeque<Position>,
    /// Most recent last.
    forward: Vec<Position>,
}

impl History {
    /// A jump is leaving `from`. Starts a new path, so forward is cleared.
    pub fn record(&mut self, from: Position) {
        self.forward.clear();
        if self.back.back() == Some(&from) {
            return;
        }
        if self.back.len() == LIMIT {
            self.back.pop_front();
        }
        self.back.push_back(from);
    }

    /// Where to go back to from `current`, if anywhere.
    pub fn back(&mut self, current: Position) -> Option<Position> {
        let to = self.back.pop_back()?;
        self.forward.push(current);
        Some(to)
    }

    /// Where to go forward to from `current`, if anywhere.
    pub fn forward(&mut self, current: Position) -> Option<Position> {
        let to = self.forward.pop()?;
        self.back.push_back(current);
        Some(to)
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(spine_index: usize) -> Position {
        Position {
            spine_index,
            fraction: 0.5,
        }
    }

    #[test]
    fn back_and_forward_retrace_the_jumps() {
        let mut h = History::default();
        assert!(!h.can_go_back() && !h.can_go_forward());
        h.record(at(1)); // 1 → 2
        h.record(at(2)); // 2 → 3
        assert_eq!(h.back(at(3)), Some(at(2)));
        assert_eq!(h.back(at(2)), Some(at(1)));
        assert_eq!(h.back(at(1)), None);
        assert_eq!(h.forward(at(1)), Some(at(2)));
        assert_eq!(h.forward(at(2)), Some(at(3)));
        assert_eq!(h.forward(at(3)), None);
    }

    #[test]
    fn a_new_jump_clears_forward() {
        let mut h = History::default();
        h.record(at(1));
        h.back(at(2));
        assert!(h.can_go_forward());
        h.record(at(1));
        assert!(!h.can_go_forward());
    }

    #[test]
    fn the_same_place_is_recorded_once() {
        let mut h = History::default();
        h.record(at(1));
        h.record(at(1));
        assert_eq!(h.back(at(2)), Some(at(1)));
        assert!(!h.can_go_back());
    }

    #[test]
    fn oldest_entries_are_dropped_past_the_limit() {
        let mut h = History::default();
        for i in 0..LIMIT + 5 {
            h.record(at(i));
        }
        let mut went_back = 0;
        let mut last = None;
        while let Some(p) = h.back(at(999)) {
            went_back += 1;
            last = Some(p);
        }
        assert_eq!(went_back, LIMIT);
        assert_eq!(last, Some(at(5)), "the first five were dropped");
    }
}
