//! Input contracts shared by native and browser front ends.
use crate::{Machine, Soc, SocBus};

/// Stable load-kind numbers used by both browser ABIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LoadKind {
    Rom = 0,
    Bootloader = 1,
    Partitions = 2,
    App = 3,
    Symbols = 4,
    Flash = 5,
    Script = 6,
    CameraPicture = 7,
}

impl TryFrom<u32> for LoadKind {
    type Error = String;
    fn try_from(kind: u32) -> Result<Self, Self::Error> {
        match kind {
            0 => Ok(Self::Rom),
            1 => Ok(Self::Bootloader),
            2 => Ok(Self::Partitions),
            3 => Ok(Self::App),
            4 => Ok(Self::Symbols),
            5 => Ok(Self::Flash),
            6 => Ok(Self::Script),
            7 => Ok(Self::CameraPicture),
            _ => Err(format!("unknown load kind {kind}")),
        }
    }
}

impl<S: Soc> Machine<S> {
    pub fn load_input(&mut self, kind: LoadKind, data: &[u8]) -> Result<(), String> {
        match kind {
            LoadKind::Rom => self.load_rom(data),
            LoadKind::Bootloader | LoadKind::Flash => self.write_flash(0, data),
            LoadKind::Partitions => self.write_flash(0x8000, data),
            LoadKind::App => self.write_flash(0x10000, data),
            LoadKind::Symbols => self.add_symbols(data),
            LoadKind::Script => {
                let text = std::str::from_utf8(data).map_err(|_| "script is not UTF-8")?;
                self.load_script(text)
            }
            LoadKind::CameraPicture => {
                let picture = crate::picture::parse(data)?;
                self.bus.board().set_camera_picture(picture);
                Ok(())
            }
        }
    }

    /// Resolve a symbol first, then a hexadecimal address with an optional `0x` prefix.
    pub fn resolve_stub(&self, name: &str) -> Option<u32> {
        self.sym_addr(name).or_else(|| {
            u32::from_str_radix(name.strip_prefix("0x").unwrap_or(name), 16).ok()
        })
    }
}
