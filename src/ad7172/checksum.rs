#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum ChecksumMode {
    Off = 0b00,
    /// Seems much less reliable than `Crc`
    Xor = 0b01,
    Crc = 0b10,
}

impl From<u8> for ChecksumMode {
    fn from(x: u8) -> Self {
        match x {
            0 => ChecksumMode::Off,
            1 => ChecksumMode::Xor,
            _ => ChecksumMode::Crc,
        }
    }
}

pub struct Checksum {
    mode: ChecksumMode,
    state: u8,
}

impl Checksum {
    pub fn new(mode: ChecksumMode) -> Self {
        Checksum { mode, state: 0 }
    }

    pub fn feed(&mut self, input: u8) {
        match self.mode {
            ChecksumMode::Off => {},
            ChecksumMode::Xor => self.state ^= input,
            ChecksumMode::Crc => {
                for i in 0..8 {
                    let input_mask = 0x80 >> i;
                    self.state = (self.state << 1) ^
                        if ((self.state & 0x80) != 0) != ((input & input_mask) != 0) {
                            0x07 /* x8 + x2 + x + 1 */
                        } else {
                            0
                        };
                }
            }
        }
    }

    pub fn result(&self) -> Option<u8> {
        match self.mode {
            ChecksumMode::Off => None,
            _ => Some(self.state)
        }
    }
}
