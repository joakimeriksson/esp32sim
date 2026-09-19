//! CPU interrupt-line state shared by the C3 matrix and the C6 PLIC/INTPRI front ends.

#[derive(Default)]
pub struct Lines {
    pub enable: u32,
    pub int_type: u32,
    pub pri: [u32; 32],
    pub thresh: u32,
    pub edge_pending: u32,
    pub level: u32,
    prev: u32,
}

impl Lines {
    /// Route asserted peripheral sources, latching only rising edges on edge-triggered lines.
    pub fn update(&mut self, map: &[u32], status: &[u32]) {
        let mut lines = 0;
        for (source, &line) in map.iter().enumerate() {
            if status[source / 32] & (1 << (source % 32)) != 0 && line != 0 {
                lines |= 1 << line;
            }
        }
        self.edge_pending |= lines & !self.prev & self.int_type;
        self.prev = lines;
        self.level = lines & !self.int_type;
    }

    /// Highest nonzero priority at or above the threshold; the lower line wins a tie.
    pub fn pending(&self) -> Option<u32> {
        let pending = (self.level | self.edge_pending) & self.enable & !1;
        if pending == 0 { return None; }
        let (mut best, mut best_pri) = (None, 0);
        for line in 1..32 {
            let pri = self.pri[line];
            if pending & (1 << line) != 0 && pri >= self.thresh && pri > best_pri {
                best = Some(line as u32);
                best_pri = pri;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::Lines;

    #[test]
    fn edge_clear_requires_a_new_rising_edge_while_levels_follow_sources() {
        let mut lines = Lines { int_type: 1 << 3, ..Lines::default() };
        let map = [0, 3, 5];
        lines.update(&map, &[0b111]);
        assert_eq!(lines.edge_pending, 1 << 3);
        assert_eq!(lines.level, 1 << 5);
        lines.edge_pending = 0;
        lines.update(&map, &[0b111]);
        assert_eq!(lines.edge_pending, 0);
        lines.update(&map, &[0]);
        assert_eq!(lines.level, 0);
        lines.update(&map, &[0b010]);
        assert_eq!(lines.edge_pending, 1 << 3);
        lines.update(&map, &[0]);
        assert_eq!(lines.edge_pending, 1 << 3);
    }

    #[test]
    fn priority_threshold_enable_and_ties_select_one_line() {
        let mut lines = Lines { level: u32::MAX, enable: u32::MAX, thresh: 2, ..Lines::default() };
        lines.pri[0] = 15; // Line zero is never delivered.
        lines.pri[4] = 2;
        lines.pri[7] = 2;
        assert_eq!(lines.pending(), Some(4)); // Equal to the threshold is enabled.
        lines.pri[7] = 3;
        assert_eq!(lines.pending(), Some(7));
        lines.enable &= !(1 << 7);
        assert_eq!(lines.pending(), Some(4));
        lines.thresh = 3;
        assert_eq!(lines.pending(), None);
        lines.thresh = 0;
        lines.pri[4] = 0;
        assert_eq!(lines.pending(), None); // Priority zero stays disabled.
    }
}
