use esp_soc::board::{BoardEdge, BoardModel, VirtualCycle};

pub const PIN_AMOLED_SDIO0: u8 = 4;
pub const PIN_AMOLED_SDIO1: u8 = 5;
pub const PIN_AMOLED_SDIO2: u8 = 6;
pub const PIN_AMOLED_SDIO3: u8 = 7;
pub const PIN_AMOLED_SCLK: u8 = 11;
pub const PIN_AMOLED_CS: u8 = 12;
pub const PIN_AMOLED_TE: u8 = 13;
pub const PIN_AMOLED_I2C_SCL: u8 = 14;
pub const PIN_AMOLED_I2C_SDA: u8 = 15;
pub const PIN_AMOLED_TOUCH_INT: u8 = 21;

/// CO5300 controller and 368x448 RGB565 frame memory used by the Waveshare AMOLED V2 board.
pub struct Co5300 {
    pub frame: Vec<u16>,
    pub frames: u64,
    pub pixels_written: u64,
    x0: u16,
    x1: u16,
    y0: u16,
    y1: u16,
    x: u16,
    y: u16,
    pending: Option<u8>,
    pixel_hi: Option<u8>,
}

impl Co5300 {
    pub const WIDTH: usize = 368;
    pub const HEIGHT: usize = 448;
    pub const X_OFFSET: u16 = 0x10;

    pub fn new() -> Self {
        Self {
            frame: vec![0; Self::WIDTH * Self::HEIGHT],
            frames: 0,
            pixels_written: 0,
            x0: Self::X_OFFSET,
            x1: Self::X_OFFSET + Self::WIDTH as u16 - 1,
            y0: 0,
            y1: Self::HEIGHT as u16 - 1,
            x: 0,
            y: 0,
            pending: None,
            pixel_hi: None,
        }
    }

    pub fn transaction(&mut self, tx: &[u8]) {
        let data = if tx.len() >= 4 && matches!(tx[0], 0x02 | 0x32) {
            let command = tx[2];
            if command == 0x01 {
                *self = Self::new();
                return;
            }
            self.pending = Some(command);
            self.pixel_hi = None;
            if command == 0x2c { self.x = self.x0; self.y = self.y0; self.frames += 1; }
            &tx[4..]
        } else { tx };
        match self.pending {
            Some(0x2a) if data.len() >= 4 => {
                self.x0 = u16::from_be_bytes([data[0], data[1]]);
                self.x1 = u16::from_be_bytes([data[2], data[3]]);
                self.x = self.x0;
                self.pending = None;
            }
            Some(0x2b) if data.len() >= 4 => {
                self.y0 = u16::from_be_bytes([data[0], data[1]]);
                self.y1 = u16::from_be_bytes([data[2], data[3]]);
                self.y = self.y0;
                self.pending = None;
            }
            // ESP-IDF sends the command and its four parameter bytes in separate
            // SPI transfers while keeping chip select active. Preserve the command
            // after the header-only transfer so the next transfer sets the window.
            Some(0x2a | 0x2b) => {}
            Some(0x2c | 0x3c) => {
                for &byte in data {
                    match self.pixel_hi.take() {
                        None => self.pixel_hi = Some(byte),
                        Some(high) => self.write_pixel(u16::from_be_bytes([high, byte])),
                    }
                }
            }
            Some(_) => self.pending = None,
            None => {}
        }
    }

    fn write_pixel(&mut self, pixel: u16) {
        let framebuffer_x = self.x.checked_sub(Self::X_OFFSET).map(usize::from);
        if framebuffer_x.is_some_and(|x| x < Self::WIDTH) && (self.y as usize) < Self::HEIGHT {
            self.frame[self.y as usize * Self::WIDTH + framebuffer_x.expect("checked panel column must exist")] = pixel;
            self.pixels_written += 1;
        }
        if self.x >= self.x1 {
            self.x = self.x0;
            if self.y >= self.y1 { self.y = self.y0; } else { self.y += 1; }
        } else {
            self.x += 1;
        }
    }
}

impl Default for Co5300 { fn default() -> Self { Self::new() } }


/// Waveshare ESP32-S3-Touch-AMOLED-1.8 V2 with the CO5300 and CST820 device set.
pub struct WaveshareAmoled18V2 {
    pub gpio_events: u64,
    pub panel: Co5300,
    pub touch_state: std::sync::Arc<std::sync::Mutex<crate::i2c::TouchState>>,
    cycle: VirtualCycle,
    next_te_cycle: Option<VirtualCycle>,
    te_level: bool,
    touch_irq_level: bool,
    /// At most two touch IRQ transitions: the first observable edge and a coalesced final level.
    pending_touch_irq: Option<(VirtualCycle, bool)>,
    pending_touch_final: Option<bool>,
    edges: Vec<BoardEdge>,
}

impl WaveshareAmoled18V2 {
    const APPROXIMATE_TE_HALF_PERIOD: VirtualCycle = crate::periph::CPU_HZ / 120;

    pub fn new() -> Self {
        Self {
            gpio_events: 0,
            panel: Co5300::new(),
            touch_state: Default::default(),
            cycle: 0,
            next_te_cycle: Some(Self::APPROXIMATE_TE_HALF_PERIOD),
            te_level: true,
            touch_irq_level: true,
            pending_touch_irq: None,
            pending_touch_final: None,
            edges: Vec::new(),
        }
    }

    fn queue_touch(&mut self, cycle: VirtualCycle, x: u16, y: u16, down: bool) {
        let mut touch = self.touch_state.lock().expect("AMOLED touch state mutex poisoned");
        touch.x = x.min(Co5300::WIDTH as u16 - 1);
        touch.y = y.min(Co5300::HEIGHT as u16 - 1);
        if down { touch.down = true; touch.seen = false; touch.release_pending = false; }
        else if touch.seen { touch.down = false; }
        else { touch.release_pending = true; }
        drop(touch);
        let level = !down;
        let queued_level = self.pending_touch_final.or(self.pending_touch_irq.map(|(_, level)| level)).unwrap_or(self.touch_irq_level);
        if level != queued_level {
            match self.pending_touch_irq {
                Some((_, first_level)) if level == first_level => self.pending_touch_final = None,
                Some(_) => self.pending_touch_final = Some(level),
                None => self.pending_touch_irq = Some((cycle.max(self.cycle).saturating_add(1), level)),
            }
        }
    }
}

impl Default for WaveshareAmoled18V2 { fn default() -> Self { Self::new() } }

impl BoardModel for WaveshareAmoled18V2 {
    fn name(&self) -> &'static str { "waveshare-amoled18-v2" }
    fn gpio_changes(&mut self, changes: &[(u8, bool)]) { self.gpio_events += changes.len() as u64; }
    fn gpio_events(&self) -> u64 { self.gpio_events }
    fn spi_transfer(&mut self, host: u8, tx: &[u8], rx_len: usize) -> Vec<u8> {
        if host == 2 { self.panel.transaction(tx); }
        vec![0xff; rx_len]
    }
    fn i2c_devices(&mut self) -> Vec<(u8, u8, Box<dyn crate::i2c::I2cDevice>)> {
        use crate::i2c::*;
        vec![
            (0, 0x15, Box::new(Cst820::new(self.touch_state.clone()))),
            (0, 0x20, Box::new(Tca9554::register_ram_stub())),
            (0, 0x34, Box::new(Reg8Device::new("axp2101-pmic-initialization-stub", &[(0x03, 0x4a)]))),
            (0, 0x51, Box::new(Reg8Device::new("pcf85063a-rtc-initialization-stub", &[]))),
            (0, 0x6b, Box::new(Reg8Device::new("qmi8658-imu-initialization-stub", &[(0x00, 0x05)]))),
        ]
    }
    fn display(&self) -> Option<(u32, u32, Vec<u16>, u64)> {
        Some((Co5300::WIDTH as u32, Co5300::HEIGHT as u32, self.panel.frame.clone(), self.panel.pixels_written))
    }
    fn display_version(&self) -> u64 { self.panel.pixels_written }
    fn display_frames(&self) -> u64 { self.panel.frames }
    fn display_quiet_push(&self) -> bool { true }
    fn input_levels(&self) -> Vec<(u8, bool)> {
        vec![(PIN_AMOLED_TE, self.te_level), (PIN_AMOLED_TOUCH_INT, self.touch_irq_level)]
    }
    fn touch(&mut self, x: u16, y: u16, down: bool) { self.queue_touch(self.cycle, x, y, down); }
    fn touch_at(&mut self, cycle: VirtualCycle, x: u16, y: u16, down: bool) { self.queue_touch(cycle, x, y, down); }
    fn next_deadline(&self) -> Option<VirtualCycle> {
        match (self.next_te_cycle, self.pending_touch_irq) {
            (Some(te), Some((touch, _))) => Some(te.min(touch)),
            (Some(te), None) => Some(te),
            (None, Some((touch, _))) => Some(touch),
            (None, None) => None,
        }
    }
    fn advance_to(&mut self, cycle: VirtualCycle) {
        assert!(cycle >= self.cycle, "board time moved backwards from {} to {}", self.cycle, cycle);
        while self.next_deadline().is_some_and(|deadline| deadline <= cycle) {
            let deadline = self.next_deadline().expect("due board transition must have a deadline");
            if self.next_te_cycle == Some(deadline) {
                self.te_level = !self.te_level;
                self.edges.push(BoardEdge { cycle: deadline, pin: PIN_AMOLED_TE, level: self.te_level });
                self.next_te_cycle = deadline.checked_add(Self::APPROXIMATE_TE_HALF_PERIOD);
            }
            if self.pending_touch_irq.is_some_and(|(touch_cycle, _)| touch_cycle == deadline) {
                let (_, level) = self.pending_touch_irq.take().expect("due touch interrupt must remain pending");
                self.touch_irq_level = level;
                self.edges.push(BoardEdge { cycle: deadline, pin: PIN_AMOLED_TOUCH_INT, level });
                if self.pending_touch_final.is_some_and(|final_level| final_level != level) {
                    let final_level = self.pending_touch_final.take().expect("different final touch level must remain pending");
                    self.pending_touch_irq = Some((deadline.saturating_add(1), final_level));
                } else {
                    self.pending_touch_final = None;
                }
            }
        }
        self.cycle = cycle;
    }
    fn take_edges(&mut self) -> Vec<BoardEdge> { std::mem::take(&mut self.edges) }
}


#[cfg(test)]
mod amoled_tests {
    use super::*;

    fn cpu_transfer(command: u8, data: &[u8]) -> Vec<u8> {
        let mut spi = esp_periph::gpspi::GpSpi::new();
        spi.write(0x10, (1 << 31) | (1 << 30) | (1 << 27));
        spi.write(0x14, 23 << 27);
        spi.write(0x18, (7 << 28) | 0x02);
        spi.write(0x04, (command as u32) << 16);
        spi.write(0x1c, data.len() as u32 * 8 - 1);
        for (index, &byte) in data.iter().enumerate() {
            spi.w[index / 4] |= u32::from(byte) << (8 * (index % 4));
        }
        spi.write(0, 1 << 24);
        spi.take_transfer().expect("CPU GP-SPI transfer must be ready").tx
    }

    fn dma_transfer(command: u8, data: &[u8]) -> Vec<u8> {
        let mut spi = esp_periph::gpspi::GpSpi::new();
        spi.write(0x30, 1 << 28);
        spi.write(0x10, (1 << 31) | (1 << 30) | (1 << 27));
        spi.write(0x14, 23 << 27);
        spi.write(0x18, (7 << 28) | 0x32);
        spi.write(0x04, (command as u32) << 16);
        spi.write(0x1c, data.len() as u32 * 8 - 1);
        spi.write(0, 1 << 24);
        spi.complete_dma_tx(data);
        spi.take_transfer().expect("DMA GP-SPI transfer must be ready").tx
    }

    #[test]
    fn combined_gpspi_parameter_and_color_phases_update_panel_pixels() {
        let mut panel = Co5300::new();
        panel.transaction(&cpu_transfer(0x2a, &[0, 0x11, 0, 0x12]));
        panel.transaction(&cpu_transfer(0x2b, &[0, 3, 0, 3]));
        panel.transaction(&dma_transfer(0x2c, &[0xf8, 0, 0x07, 0xe0]));
        assert_eq!(panel.frame[3 * Co5300::WIDTH + 1], 0xf800);
        assert_eq!(panel.frame[3 * Co5300::WIDTH + 2], 0x07e0);
        assert_eq!(panel.pixels_written, 2);
        assert_eq!(panel.frames, 1);
    }

    #[test]
    fn split_gpspi_commands_and_parameters_place_partial_updates_in_the_window() {
        let mut panel = Co5300::new();
        // The IDF LCD SPI driver sends these headers separately from the parameters.
        panel.transaction(&[0x02, 0, 0x2a, 0]);
        panel.transaction(&[0, 0x60, 0, 0x61]);
        panel.transaction(&[0x02, 0, 0x2b, 0]);
        panel.transaction(&[0, 140, 0, 141]);
        panel.transaction(&[0x32, 0, 0x2c, 0]);
        panel.transaction(&[0xf8, 0, 0x07, 0xe0]);
        panel.transaction(&[0x32, 0, 0x3c, 0]);
        panel.transaction(&[0, 0x1f, 0xff, 0xff]);
        assert_eq!(&panel.frame[140 * Co5300::WIDTH + 80..140 * Co5300::WIDTH + 82], &[0xf800, 0x07e0]);
        assert_eq!(&panel.frame[141 * Co5300::WIDTH + 80..141 * Co5300::WIDTH + 82], &[0x001f, 0xffff]);
        assert_eq!(panel.pixels_written, 4);
        assert_eq!(panel.frame[0], 0);
    }

    #[test]
    fn physical_x_offset_maps_the_full_panel_width_to_the_framebuffer() {
        let mut panel = Co5300::new();
        let x1 = Co5300::X_OFFSET + Co5300::WIDTH as u16 - 1;
        panel.transaction(&cpu_transfer(0x2a, &[0, Co5300::X_OFFSET as u8, (x1 >> 8) as u8, x1 as u8]));
        panel.transaction(&cpu_transfer(0x2b, &[0, 0, 0, 0]));
        let mut pixels = Vec::with_capacity(Co5300::WIDTH * 2);
        for pixel in 1..=Co5300::WIDTH as u16 { pixels.extend_from_slice(&pixel.to_be_bytes()); }
        panel.transaction(&dma_transfer(0x2c, &pixels));
        assert_eq!(panel.pixels_written, Co5300::WIDTH as u64);
        assert_eq!(panel.frame[0], 1);
        assert_eq!(panel.frame[Co5300::WIDTH - 1], Co5300::WIDTH as u16);
        assert_eq!(&panel.frame[..Co5300::WIDTH], &(1..=Co5300::WIDTH as u16).collect::<Vec<_>>());
    }

    #[test]
    fn software_reset_clears_the_panel_parser_window_and_framebuffer() {
        let mut panel = Co5300::new();
        panel.transaction(&cpu_transfer(0x2a, &[0, 0x11, 0, 0x11]));
        panel.transaction(&cpu_transfer(0x2b, &[0, 3, 0, 3]));
        panel.transaction(&dma_transfer(0x2c, &[0xf8, 0]));
        assert_eq!(panel.frame[3 * Co5300::WIDTH + 1], 0xf800);

        panel.transaction(&[0x02, 0, 0x01, 0]);
        assert!(panel.frame.iter().all(|&pixel| pixel == 0));
        assert_eq!((panel.x0, panel.x1, panel.y0, panel.y1),
                   (Co5300::X_OFFSET, Co5300::X_OFFSET + Co5300::WIDTH as u16 - 1, 0, Co5300::HEIGHT as u16 - 1));
        assert_eq!((panel.pending, panel.pixel_hi, panel.frames, panel.pixels_written), (None, None, 0, 0));
    }

    #[test]
    fn board_exposes_the_v2_device_addresses_on_i2c0() {
        let mut board = WaveshareAmoled18V2::new();
        let addresses: Vec<_> = board.i2c_devices().into_iter().map(|(bus, address, _)| (bus, address)).collect();
        assert_eq!(addresses, [(0, 0x15), (0, 0x20), (0, 0x34), (0, 0x51), (0, 0x6b)]);
    }

    #[test]
    fn board_name_selects_only_the_v2_model() {
        assert_eq!(crate::board::make_board("waveshare-amoled18-v2").unwrap().name(), "waveshare-amoled18-v2");
        assert_eq!(crate::board::make_board("amoled18-v2").unwrap().name(), "waveshare-amoled18-v2");
        assert!(crate::board::make_board("waveshare-amoled18").is_none());
    }

    #[test]
    fn approximate_te_signal_preserves_edge_timestamps() {
        let mut board = WaveshareAmoled18V2::new();
        let half_period = WaveshareAmoled18V2::APPROXIMATE_TE_HALF_PERIOD;
        assert_eq!(board.next_deadline(), Some(half_period));
        board.advance_to(half_period - 1);
        assert!(board.take_edges().is_empty());
        board.advance_to(half_period);
        assert_eq!(board.take_edges(), [BoardEdge { cycle: half_period, pin: PIN_AMOLED_TE, level: false }]);
        board.advance_to(half_period * 2);
        assert_eq!(board.take_edges(), [BoardEdge { cycle: half_period * 2, pin: PIN_AMOLED_TE, level: true }]);
    }

    #[test]
    fn touch_signal_preserves_active_low_edge_timestamps() {
        let mut board = WaveshareAmoled18V2::new();
        board.touch(100, 200, true);
        board.touch(100, 200, false);
        board.advance_to(2);
        assert_eq!(board.take_edges(), [
            BoardEdge { cycle: 1, pin: PIN_AMOLED_TOUCH_INT, level: false },
            BoardEdge { cycle: 2, pin: PIN_AMOLED_TOUCH_INT, level: true },
        ]);
    }

    #[test]
    fn touch_burst_keeps_the_first_edge_and_coalesces_to_the_final_level() {
        let mut board = WaveshareAmoled18V2::new();
        for index in 0..1000 { board.touch(100, 200, index % 2 == 0); }
        assert_eq!(board.pending_touch_irq, Some((1, false)));
        assert_eq!(board.pending_touch_final, Some(true));
        board.advance_to(2);
        assert_eq!(board.take_edges(), [
            BoardEdge { cycle: 1, pin: PIN_AMOLED_TOUCH_INT, level: false },
            BoardEdge { cycle: 2, pin: PIN_AMOLED_TOUCH_INT, level: true },
        ]);
        assert_eq!(board.input_levels(), [(PIN_AMOLED_TE, true), (PIN_AMOLED_TOUCH_INT, true)]);
    }

    #[test]
    fn te_deadline_overflow_disables_future_transitions() {
        let mut board = WaveshareAmoled18V2::new();
        board.cycle = VirtualCycle::MAX - 1;
        board.next_te_cycle = Some(VirtualCycle::MAX);
        board.advance_to(VirtualCycle::MAX);
        assert_eq!(board.take_edges(), [BoardEdge { cycle: VirtualCycle::MAX, pin: PIN_AMOLED_TE, level: false }]);
        assert_eq!(board.next_deadline(), None);
    }

}
