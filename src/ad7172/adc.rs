use core::fmt;
use log::{info, warn};
use stm32f4xx_hal::hal::{
    blocking::spi::Transfer,
    digital::v2::OutputPin,
};
use uom::si::{
    f64::ElectricPotential,
    electric_potential::volt,
};
use super::{
    regs::{self, Register, RegisterData},
    checksum::{ChecksumMode, Checksum},
    Mode, Input, RefSource, PostFilter, DigitalFilterOrder,
};

/// AD7172-2 implementation
///
/// [Manual](https://www.analog.com/media/en/technical-documentation/data-sheets/AD7172-2.pdf)
pub struct Adc<SPI: Transfer<u8>, NSS: OutputPin> {
    spi: SPI,
    nss: NSS,
    checksum_mode: ChecksumMode,
}

impl<SPI: Transfer<u8, Error = E>, NSS: OutputPin, E: fmt::Debug> Adc<SPI, NSS> {
    pub fn new(spi: SPI, mut nss: NSS) -> Result<Self, SPI::Error> {
        let _ = nss.set_high();
        let mut adc = Adc {
            spi, nss,
            checksum_mode: ChecksumMode::Off,
        };
        adc.reset()?;
        adc.set_checksum_mode(ChecksumMode::Crc).unwrap();

        let mut retries = 0;
        let mut adc_id;
        loop {
            adc_id = adc.identify()?;
            if adc_id & 0xFFF0 == 0x00D0 {
                break;
            } else {
                retries += 1;
            }
        }
        info!("ADC id: {:04X} ({} retries)", adc_id, retries);

        let mut adc_mode = <regs::AdcMode as Register>::Data::empty();
        adc_mode.set_ref_en(true);
        adc_mode.set_mode(Mode::Standby);
        adc.write_reg(&regs::AdcMode, &mut adc_mode)?;

        Ok(adc)
    }

    /// `0x00DX` for AD7172-2
    pub fn identify(&mut self) -> Result<u16, SPI::Error> {
        self.read_reg(&regs::Id)
            .map(|id| id.id())
    }

    pub fn set_checksum_mode(&mut self, mode: ChecksumMode) -> Result<(), SPI::Error> {
        // Cannot use update_reg() here because checksum_mode is
        // updated between read_reg() and write_reg().
        let mut ifmode = self.read_reg(&regs::IfMode)?;
        ifmode.set_crc(mode);
        self.checksum_mode = mode;
        self.write_reg(&regs::IfMode, &mut ifmode)?;
        Ok(())
    }

    pub fn set_sync_enable(&mut self, enable: bool) -> Result<(), SPI::Error> {
        self.update_reg(&regs::GpioCon, |data| {
            data.set_sync_en(enable);
        })
    }

    pub fn setup_channel(
        &mut self, index: u8, in_pos: Input, in_neg: Input
    ) -> Result<(), SPI::Error> {
        self.update_reg(&regs::SetupCon { index }, |data| {
            data.set_bipolar(false);
            data.set_refbuf_pos(true);
            data.set_refbuf_neg(true);
            data.set_ainbuf_pos(true);
            data.set_ainbuf_neg(true);
            data.set_ref_sel(RefSource::External);
        })?;
        self.update_reg(&regs::FiltCon { index }, |data| {
            data.set_enh_filt_en(true);
            data.set_enh_filt(PostFilter::F16SPS);
            data.set_order(DigitalFilterOrder::Sinc5Sinc1);
            // output data rate: 10 Hz
            data.set_odr(0b10011);
        })?;
        self.update_reg(&regs::Channel { index }, |data| {
            data.set_setup(index);
            data.set_enabled(true);
            data.set_a_in_pos(in_pos);
            data.set_a_in_neg(in_neg);
        })?;
        Ok(())
    }

    pub fn get_calibration(&mut self, index: u8) -> Result<ChannelCalibration, SPI::Error> {
        let offset = self.read_reg(&regs::Offset { index })?.offset();
        let gain = self.read_reg(&regs::Gain { index })?.gain();
        let bipolar = self.read_reg(&regs::SetupCon { index })?.bipolar();
        Ok(ChannelCalibration { offset, gain, bipolar })
    }

    pub fn start_continuous_conversion(&mut self) -> Result<(), SPI::Error> {
        let mut adc_mode = <regs::AdcMode as Register>::Data::empty();
        adc_mode.set_ref_en(true);
        adc_mode.set_mode(Mode::ContinuousConversion);
        self.write_reg(&regs::AdcMode, &mut adc_mode)?;

        Ok(())
    }

    pub fn get_postfilter(&mut self, index: u8) -> Result<Option<PostFilter>, SPI::Error> {
        self.read_reg(&regs::FiltCon { index })
            .map(|data| {
                if data.enh_filt_en() {
                    Some(data.enh_filt())
                } else {
                    None
                }
            })
    }

    pub fn set_postfilter(&mut self, index: u8, filter: Option<PostFilter>) -> Result<(), SPI::Error> {
        self.update_reg(&regs::FiltCon { index }, |data| {
            match filter {
                None => data.set_enh_filt_en(false),
                Some(filter) => {
                    data.set_enh_filt_en(true);
                    data.set_enh_filt(filter);
                }
            }
        })
    }

    /// Returns the channel the data is from
    pub fn data_ready(&mut self) -> Result<Option<u8>, SPI::Error> {
        self.read_reg(&regs::Status)
            .map(|status| {
                if status.ready() {
                    Some(status.channel())
                } else {
                    None
                }
            })
    }

    /// Get data
    pub fn read_data(&mut self) -> Result<u32, SPI::Error> {
        self.read_reg(&regs::Data)
            .map(|data| data.data())
    }

    fn read_reg<R: regs::Register>(&mut self, reg: &R) -> Result<R::Data, SPI::Error> {
        let mut reg_data = R::Data::empty();
        let address = 0x40 | reg.address();
        let mut checksum = Checksum::new(self.checksum_mode);
        checksum.feed(&[address]);
        let checksum_out = checksum.result();

        loop {
            let checksum_in = self.transfer(address, reg_data.as_mut(), checksum_out)?;

            checksum.feed(&reg_data);
            let checksum_expected = checksum.result();
            if checksum_expected == checksum_in {
                break;
            }
            // Retry
            warn!("read_reg {:02X}: checksum error: {:?}!={:?}, retrying", reg.address(), checksum_expected, checksum_in);
        }
        Ok(reg_data)
    }

    fn write_reg<R: regs::Register>(&mut self, reg: &R, reg_data: &mut R::Data) -> Result<(), SPI::Error> {
        loop {
            let address = reg.address();
            let mut checksum = Checksum::new(match self.checksum_mode {
                ChecksumMode::Off => ChecksumMode::Off,
                // write checksums are always crc
                ChecksumMode::Xor => ChecksumMode::Crc,
                ChecksumMode::Crc => ChecksumMode::Crc,
            });
            checksum.feed(&[address]);
            checksum.feed(&reg_data);
            let checksum_out = checksum.result();

            let mut data = reg_data.clone();
            self.transfer(address, data.as_mut(), checksum_out)?;

            // Verification
            let readback_data = self.read_reg(reg)?;
            if *readback_data == **reg_data {
                return Ok(());
            }
            warn!("write_reg {:02X}: readback error, {:?}!={:?}, retrying", address, &*readback_data, &**reg_data);
        }
    }

    fn update_reg<R, F, A>(&mut self, reg: &R, f: F) -> Result<A, SPI::Error>
    where
        R: regs::Register,
        F: FnOnce(&mut R::Data) -> A,
    {
        let mut reg_data = self.read_reg(reg)?;
        let result = f(&mut reg_data);
        self.write_reg(reg, &mut reg_data)?;
        Ok(result)
    }

    pub fn reset(&mut self) -> Result<(), SPI::Error> {
        let mut buf = [0xFFu8; 8];
        let _ = self.nss.set_low();
        let result = self.spi.transfer(&mut buf);
        let _ = self.nss.set_high();
        result?;
        Ok(())
    }

    fn transfer<'w>(&mut self, addr: u8, reg_data: &'w mut [u8], checksum: Option<u8>) -> Result<Option<u8>, SPI::Error> {
        let mut addr_buf = [addr];

        let _ = self.nss.set_low();
        let result = match self.spi.transfer(&mut addr_buf) {
            Ok(_) => self.spi.transfer(reg_data),
            Err(e) => Err(e),
        };
        let result = match (result, checksum) {
            (Ok(_), None) =>
                Ok(None),
            (Ok(_), Some(checksum_out)) => {
                let mut checksum_buf = [checksum_out; 1];
                match self.spi.transfer(&mut checksum_buf) {
                    Ok(_) => Ok(Some(checksum_buf[0])),
                    Err(e) => Err(e),
                }
            }
            (Err(e), _) =>
                Err(e),
        };
        let _ = self.nss.set_high();

        result
    }
}

#[derive(Debug, Clone)]
pub struct ChannelCalibration {
    offset: u32,
    gain: u32,
    bipolar: bool,
}

impl ChannelCalibration {
    pub fn convert_data(&self, data: u32) -> ElectricPotential {
        let data = if self.bipolar {
            (data as i32 - 0x80_0000) as f64
        } else {
            data as f64 / 2.0
        };
        let data = data / (self.gain as f64 / (0x40_0000 as f64));
        let data = data + (self.offset as i32 - 0x80_0000) as f64;
        let data = data / (2 << 23) as f64;

        const V_REF: f64 = 3.3;
        ElectricPotential::new::<volt>(data * V_REF / 0.75)
    }
}
