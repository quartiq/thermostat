use stm32f4xx_hal::{
    hal::{
        blocking::spi::Transfer,
        digital::v2::OutputPin,
    },
    time::MegaHertz,
    spi,
};

/// SPI Mode 1
pub const SPI_MODE: spi::Mode = spi::Mode {
    polarity: spi::Polarity::IdleLow,
    phase: spi::Phase::CaptureOnSecondTransition,
};
/// 30 MHz
pub const SPI_CLOCK: MegaHertz = MegaHertz(30);

pub const MAX_VALUE: u32 = 0x3FFFF;

/// [AD5680](https://www.analog.com/media/en/technical-documentation/data-sheets/AD5680.pdf) DAC
pub struct Dac<SPI: Transfer<u8>, S: OutputPin> {
    spi: SPI,
    sync: S,
}

impl<SPI: Transfer<u8>, S: OutputPin> Dac<SPI, S> {
    pub fn new(spi: SPI, mut sync: S) -> Self {
        let _ = sync.set_high();
        
        Dac {
            spi,
            sync,
        }
    }

    fn write(&mut self, mut buf: [u8; 3]) -> Result<(), SPI::Error> {
        let _ = self.sync.set_low();
        let result = self.spi.transfer(&mut buf);
        let _ = self.sync.set_high();

        result.map(|_| ())
    }

    /// value: `0..0x20_000`
    pub fn set(&mut self, value: u32) -> Result<(), SPI::Error> {
        let buf = [
            (value >> 14) as u8,
            (value >> 6) as u8,
            (value << 2) as u8,
        ];
        self.write(buf)
    }
}
