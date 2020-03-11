use stm32f4xx_hal::{
    gpio::{
        AF5, Alternate,
        gpioa::{PA1, PA2, PA7},
        gpiob::{PB10, PB11, PB12, PB13, PB14, PB15},
        gpioc::{PC1, PC4, PC5},
        gpiog::{PG13},
        GpioExt,
        Output, PushPull,
        Speed::VeryHigh,
    },
    rcc::Clocks,
    spi::Spi,
    stm32::{GPIOA, GPIOB, GPIOC, GPIOG, SPI2},
};


/// SPI peripheral used for communication with the ADC
type AdcSpi = Spi<SPI2, (PB10<Alternate<AF5>>, PB14<Alternate<AF5>>, PB15<Alternate<AF5>>)>;

pub struct Pins {
    pub adc_spi: AdcSpi,
    pub adc_nss: PB12<Output<PushPull>>,
}

impl Pins {
    /// Setup GPIO pins and configure MCU peripherals
    pub fn setup(clocks: Clocks, gpioa: GPIOA, gpiob: GPIOB, gpioc: GPIOC, gpiog: GPIOG, spi2: SPI2) -> Self {
        let gpioa = gpioa.split();
        let gpiob = gpiob.split();
        let gpioc = gpioc.split();
        let gpiog = gpiog.split();

        Self::setup_ethernet(
            gpioa.pa1, gpioa.pa2, gpioc.pc1, gpioa.pa7,
            gpioc.pc4, gpioc.pc5, gpiob.pb11, gpiog.pg13,
            gpiob.pb13
        );
        let adc_spi = Self::setup_spi_adc(clocks, spi2, gpiob.pb10, gpiob.pb14, gpiob.pb15);
        let adc_nss = gpiob.pb12.into_push_pull_output();
        Pins {
            adc_spi,
            adc_nss,
        }
    }

    /// Configure the GPIO pins for SPI operation, and initialize SPI
    fn setup_spi_adc<M1, M2, M3>(
        clocks: Clocks,
        spi2: SPI2,
        sck: PB10<M1>,
        miso: PB14<M2>,
        mosi: PB15<M3>,
    ) -> AdcSpi
    {
        let sck = sck.into_alternate_af5();
        let miso = miso.into_alternate_af5();
        let mosi = mosi.into_alternate_af5();
        Spi::spi2(
            spi2,
            (sck, miso, mosi),
            crate::ad7172::SPI_MODE,
            crate::ad7172::SPI_CLOCK.into(),
            clocks
        )
    }

    /// Configure the GPIO pins for Ethernet operation
    fn setup_ethernet<M1, M2, M3, M4, M5, M6, M7, M8, M9>(
        pa1: PA1<M1>, pa2: PA2<M2>, pc1: PC1<M3>, pa7: PA7<M4>,
        pc4: PC4<M5>, pc5: PC5<M6>, pb11: PB11<M7>, pg13: PG13<M8>,
        pb13: PB13<M9>
    ) {
        // PA1 RMII Reference Clock - SB13 ON
        pa1.into_alternate_af11().set_speed(VeryHigh);
        // PA2 RMII MDIO - SB160 ON
        pa2.into_alternate_af11().set_speed(VeryHigh);
        // PC1 RMII MDC - SB164 ON
        pc1.into_alternate_af11().set_speed(VeryHigh);
        // PA7 RMII RX Data Valid D11 JP6 ON
        pa7.into_alternate_af11().set_speed(VeryHigh);
        // PC4 RMII RXD0 - SB178 ON
        pc4.into_alternate_af11().set_speed(VeryHigh);
        // PC5 RMII RXD1 - SB181 ON
        pc5.into_alternate_af11().set_speed(VeryHigh);
        // PB11 RMII TX Enable - SB183 ON
        pb11.into_alternate_af11().set_speed(VeryHigh);
        // PG13 RXII TXD0 - SB182 ON
        pg13.into_alternate_af11().set_speed(VeryHigh);
        // PB13 RMII TXD1 I2S_A_CK JP7 ON
        pb13.into_alternate_af11().set_speed(VeryHigh);
    }
}
