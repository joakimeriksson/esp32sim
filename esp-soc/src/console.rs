//! Names shared by native and browser console selectors.
use crate::Console;

impl Console {
    /// USB-CDC, UART0 and UART1/2 occupy bits 0, 1 and 2 respectively.
    pub fn parse_mask(name: &str) -> Option<u32> {
        Some(match name {
            "usb" => 1,
            "uart" | "uart0" => 2,
            "both" => 3,
            "all" => 7,
            "none" => 0,
            _ => return None,
        })
    }
}
