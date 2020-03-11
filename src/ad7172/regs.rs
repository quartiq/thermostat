use byteorder::{BigEndian, ByteOrder};
use bit_field::BitField;

use super::*;

pub trait Register {
    type Data: RegisterData;
    fn address(&self) -> u8;
}
pub trait RegisterData {
    fn empty() -> Self;
    fn as_mut(&mut self) -> &mut [u8];
}

macro_rules! def_reg {
    ($Reg: ident, $reg: ident, $addr: expr, $size: expr) => {
        /// AD7172 register
        pub struct $Reg;
        impl Register for $Reg {
            /// Register contents
            type Data = $reg::Data;
            /// Register address
            fn address(&self) -> u8 {
                $addr
            }
        }
        mod $reg {
            /// Register contents
            pub struct Data(pub [u8; $size]);
            impl super::RegisterData for Data {
                /// Generate zeroed register contents
                fn empty() -> Self {
                    Data([0; $size])
                }
                /// Borrow for SPI transfer
                fn as_mut(&mut self) -> &mut [u8] {
                    &mut self.0
                }
            }
        }
    };
    ($Reg: ident, u8, $reg: ident, $addr: expr, $size: expr) => {
        pub struct $Reg { pub index: u8, }
        impl Register for $Reg {
            type Data = $reg::Data;
            fn address(&self) -> u8 {
                $addr + self.index
            }
        }
        mod $reg {
            pub struct Data(pub [u8; $size]);
            impl super::RegisterData for Data {
                fn empty() -> Self {
                    Data([0; $size])
                }
                fn as_mut(&mut self) -> &mut [u8] {
                    &mut self.0
                }
            }
        }
    }
}

macro_rules! reg_bit {
    ($getter: ident, $byte: expr, $bit: expr, $doc: expr) => {
        #[allow(unused)]
        #[doc = $doc]
        pub fn $getter(&self) -> bool {
            self.0[$byte].get_bit($bit)
        }
    };
    ($getter: ident, $setter: ident, $byte: expr, $bit: expr, $doc: expr) => {
        #[allow(unused)]
        #[doc = $doc]
        pub fn $getter(&self) -> bool {
            self.0[$byte].get_bit($bit)
        }
        #[allow(unused)]
        #[doc = $doc]
        pub fn $setter(&mut self, value: bool) {
            self.0[$byte].set_bit($bit, value);
        }
    };
}

macro_rules! reg_bits {
    ($getter: ident, $byte: expr, $bits: expr, $doc: expr) => {
        #[allow(unused)]
        #[doc = $doc]
        pub fn $getter(&self) -> u8 {
            self.0[$byte].get_bits($bits)
        }
    };
    ($getter: ident, $setter: ident, $byte: expr, $bits: expr, $doc: expr) => {
        #[allow(unused)]
        #[doc = $doc]
        pub fn $getter(&self) -> u8 {
            self.0[$byte].get_bits($bits)
        }
        #[allow(unused)]
        #[doc = $doc]
        pub fn $setter(&mut self, value: u8) {
            self.0[$byte].set_bits($bits, value);
        }
    };
    ($getter: ident, $byte: expr, $bits: expr, $ty: ty, $doc: expr) => {
        #[allow(unused)]
        #[doc = $doc]
        pub fn $getter(&self) -> $ty {
            self.0[$byte].get_bits($bits) as $ty
        }
    };
    ($getter: ident, $setter: ident, $byte: expr, $bits: expr, $ty: ty, $doc: expr) => {
        #[allow(unused)]
        #[doc = $doc]
        pub fn $getter(&self) -> $ty {
            self.0[$byte].get_bits($bits).into()
        }
        #[allow(unused)]
        #[doc = $doc]
        pub fn $setter(&mut self, value: $ty) {
            self.0[$byte].set_bits($bits, value as u8);
        }
    };
}

def_reg!(Status, status, 0x00, 1);
impl status::Data {
    /// Is there new data to read?
    pub fn ready(&self) -> bool {
        ! self.not_ready()
    }

    reg_bit!(not_ready, 0, 7, "No data ready indicator");
    reg_bits!(channel, 0, 0..=1, "Channel for which data is ready");
    reg_bit!(adc_error, 0, 6, "ADC error");
    reg_bit!(crc_error, 0, 5, "SPI CRC error");
    reg_bit!(reg_error, 0,4, "Register error");
}

def_reg!(IfMode, if_mode, 0x02, 2);
impl if_mode::Data {
    reg_bits!(crc, set_crc, 1, 2..=3, ChecksumMode, "SPI checksum mode");
}

def_reg!(Data, data, 0x04, 3);
impl data::Data {
    pub fn data(&self) -> i32 {
        let raw =
            (u32::from(self.0[0]) << 16) |
            (u32::from(self.0[1]) << 8) |
            u32::from(self.0[2]);
        if raw & 0x80_0000 != 0 {
            ((raw & 0x7F_FFFF) | 0x8000_0000) as i32
        } else {
            raw as i32
        }
    }
}

def_reg!(GpioCon, gpio_con, 0x06, 2);
impl gpio_con::Data {
    reg_bit!(sync_en, set_sync_en, 0, 3, "Enables the SYNC/ERROR pin as a sync input");
}

def_reg!(Id, id, 0x07, 2);
impl id::Data {
    pub fn id(&self) -> u16 {
        BigEndian::read_u16(&self.0)
    }
}

def_reg!(Channel, u8, channel, 0x10, 2);
impl channel::Data {
    reg_bit!(enabled, set_enabled, 0, 7, "Channel enabled");
    reg_bits!(setup, set_setup, 0, 4..=5, "Setup number");

    /// Which input is connected to positive input of this channel
    #[allow(unused)]
    pub fn a_in_pos(&self) -> Input {
        ((self.0[0].get_bits(0..=1) << 3) |
         self.0[1].get_bits(5..=7)).into()
    }
    /// Set which input is connected to positive input of this channel
    #[allow(unused)]
    pub fn set_a_in_pos(&mut self, value: Input) {
        let value = value as u8;
        self.0[0].set_bits(0..=1, value >> 3);
        self.0[1].set_bits(5..=7, value & 0x7);
    }
    reg_bits!(a_in_neg, set_a_in_neg, 1, 0..=4, Input,
              "Which input is connected to negative input of this channel");

    // const PROPS: &'static [Property<Self>] = &[
    //     Property::named("enable")
    //         .readable(&|self_: &Self| self_.enabled().into())
    //         .writebale(&|self_: &mut Self, value| self_.set_enabled(value != 0)),
    //     Property::named("setup")
    //         .readable(&|self_: &Self| self_.0[0].get_bits(4..=5).into())
    //         .writeable(&|self_: &mut Self, value| {
    //             self_.0[0].set_bits(4..=5, value as u8);
    //         }),
    // ];

    // pub fn props() -> &'static [Property<Self>] {
    //     Self::PROPS
    // }
}

def_reg!(SetupCon, u8, setup_con, 0x20, 2);
impl setup_con::Data {
    reg_bit!(bipolar, set_bipolar, 0, 4, "Unipolar (`false`) or bipolar (`true`) coded output");
    reg_bit!(refbuf_pos, set_refbuf_pos, 0, 3, "Enable REF+ input buffer");
    reg_bit!(refbuf_neg, set_refbuf_neg, 0, 2, "Enable REF- input buffer");
    reg_bit!(ainbuf_pos, set_ainbuf_pos, 0, 1, "Enable AIN+ input buffer");
    reg_bit!(ainbuf_neg, set_ainbuf_neg, 0, 0, "Enable AIN- input buffer");
    reg_bit!(burnout_en, 1, 7, "enables a 10 µA current source on the positive analog input selected and a 10 µA current sink on the negative analog input selected");
    reg_bits!(ref_sel, set_ref_sel, 1, 4..=5, RefSource, "Select reference source for conversion");
}

def_reg!(FiltCon, u8, filt_con, 0x28, 2);
impl filt_con::Data {
    reg_bit!(sinc3_map, 0, 7, "If set, mapping of filter register changes to directly program the decimation rate of the sinc3 filter");
    reg_bit!(enh_filt_en, set_enh_filt_en, 0, 3, "Enable postfilters for enhanced 50Hz and 60Hz rejection");
    reg_bits!(enh_filt, set_enh_filt, 0, 0..=2, PostFilter, "Select postfilters for enhanced 50Hz and 60Hz rejection");
    reg_bits!(order, set_order, 1, 5..=6, DigitalFilterOrder, "order of the digital filter that processes the modulator data");
    reg_bits!(odr, set_odr, 1, 0..=4, "Output data rate");
}

def_reg!(Offset, u8, offset, 0x30, 3);
impl offset::Data {
    #[allow(unused)]
    pub fn offset(&self) -> u32 {
        (u32::from(self.0[0]) << 16) |
        (u32::from(self.0[1]) << 8) |
        u32::from(self.0[2])
    }
    #[allow(unused)]
    pub fn set_offset(&mut self, value: u32) {
        self.0[0] = (value >> 16) as u8;
        self.0[1] = (value >> 8) as u8;
        self.0[2] = value as u8;
    }
}

def_reg!(Gain, u8, gain, 0x38, 3);
impl gain::Data {
    #[allow(unused)]
    pub fn gain(&self) -> u32 {
        (u32::from(self.0[0]) << 16) |
        (u32::from(self.0[1]) << 8) |
        u32::from(self.0[2])
    }
    #[allow(unused)]
    pub fn set_gain(&mut self, value: u32) {
        self.0[0] = (value >> 16) as u8;
        self.0[1] = (value >> 8) as u8;
        self.0[2] = value as u8;
    }
}
