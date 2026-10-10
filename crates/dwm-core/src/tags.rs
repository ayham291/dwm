/// Number of workspace tags (classic dwm: 9).
pub const NUM_TAGS: u32 = 9;
/// Scratchpad tags live above the workspace tags (spterm, spcalc, …).
pub const NUM_SCRATCHPADS: u32 = 2;
pub const NUM_TOTAL_TAGS: u32 = NUM_TAGS + NUM_SCRATCHPADS;

/// Bitmask of tags a client or view belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TagMask(pub u32);

impl TagMask {
    pub const EMPTY: Self = Self(0);
    /// All nine workspace tags.
    pub const ALL_WORKSPACE: Self = Self((1 << NUM_TAGS) - 1);
    /// All tags including scratchpads.
    pub const ALL: Self = Self((1 << NUM_TOTAL_TAGS) - 1);
    /// Bits reserved for scratchpads.
    pub const SCRATCH_MASK: Self = Self(((1 << NUM_SCRATCHPADS) - 1) << NUM_TAGS);

    pub fn new(bits: u32) -> Self {
        Self(bits & Self::ALL.0)
    }

    pub fn bit(tag: u32) -> Self {
        debug_assert!(tag < NUM_TAGS);
        Self(1 << tag)
    }

    /// Scratchpad tag `i` (0 = spterm, 1 = spcalc, …).
    pub fn scratch(i: u32) -> Self {
        debug_assert!(i < NUM_SCRATCHPADS);
        Self(1 << (NUM_TAGS + i))
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn toggle(self, other: Self) -> Self {
        Self(self.0 ^ other.0)
    }

    pub fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn is_scratch(self) -> bool {
        Self::SCRATCH_MASK.contains(self)
    }

    /// Circular shift within the workspace tags (used by `shiftview`).
    pub fn rotate(self, delta: i32) -> Self {
        let n = NUM_TAGS as i32;
        let bits = self.0 & Self::ALL_WORKSPACE.0;
        if delta == 0 || bits == 0 {
            return Self(bits);
        }
        let k = ((delta % n) + n) % n;
        let k = k as u32;
        let rotated = (bits << k) | (bits >> (NUM_TAGS - k));
        Self(rotated & Self::ALL_WORKSPACE.0)
    }
}

pub fn tag_bit(tag: u32) -> TagMask {
    TagMask::bit(tag)
}

pub fn scratch_bit(i: u32) -> TagMask {
    TagMask::scratch(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_left_by_one() {
        let t = TagMask::bit(0);
        assert_eq!(t.rotate(1), TagMask::bit(1));
        assert_eq!(TagMask::bit(8).rotate(1), TagMask::bit(0));
    }

    #[test]
    fn rotate_right_by_one() {
        let t = TagMask::bit(0);
        assert_eq!(t.rotate(-1), TagMask::bit(8));
    }

    #[test]
    fn rotate_multi_bit() {
        let t = TagMask::bit(0).union(TagMask::bit(2));
        assert_eq!(t.rotate(1), TagMask::bit(1).union(TagMask::bit(3)));
    }

    #[test]
    fn scratch_tags_sit_above_workspace() {
        assert_eq!(TagMask::scratch(0).0, 1 << 9);
        assert_eq!(TagMask::scratch(1).0, 1 << 10);
        assert!(TagMask::scratch(0).is_scratch());
        assert!(!TagMask::bit(0).is_scratch());
    }
}
