use embedded_hal::{
    blocking::spi::Transfer,
    digital::v2::OutputPin,
};
use stm32f4xx_hal::{
    gpio::{
        AF5, Alternate,
        gpioa::*,
        gpiob::*,
        gpioc::*,
        gpioe::*,
        gpiof::*,
        gpiog::*,
        GpioExt,
        Output, PushPull,
        Speed::VeryHigh,
    },
    rcc::Clocks,
    pwm::{self, PwmChannels},
    spi::{self, Spi, NoMiso},
    stm32::{GPIOA, GPIOB, GPIOC, GPIOE, GPIOF, GPIOG, SPI2, SPI4, SPI5, TIM1, TIM3},
    time::{U32Ext, Hertz, MegaHertz},
};


/// SPI peripheral used for communication with the ADC
type AdcSpi = Spi<SPI2, (PB10<Alternate<AF5>>, PB14<Alternate<AF5>>, PB15<Alternate<AF5>>)>;
type Dac0Spi = Spi<SPI4, (PE2<Alternate<AF5>>, NoMiso, PE6<Alternate<AF5>>)>;
type Dac1Spi = Spi<SPI5, (PF7<Alternate<AF5>>, NoMiso, PF9<Alternate<AF5>>)>;

pub struct Pins {
    pub adc_spi: AdcSpi,
    pub adc_nss: PB12<Output<PushPull>>,
    pub pwm: PwmPins,
    pub dac0_spi: Dac0Spi,
    pub dac0_sync: PE4<Output<PushPull>>,
    pub dac1_spi: Dac1Spi,
    pub dac1_sync: PF6<Output<PushPull>>,
}

impl Pins {
    /// Setup GPIO pins and configure MCU peripherals
    pub fn setup(
        clocks: Clocks,
        tim1: TIM1,
        tim3: TIM3,
        gpioa: GPIOA, gpiob: GPIOB, gpioc: GPIOC, gpioe: GPIOE, gpiof: GPIOF, gpiog: GPIOG,
        spi2: SPI2, spi4: SPI4, spi5: SPI5
    ) -> Self {
        let gpioa = gpioa.split();
        let gpiob = gpiob.split();
        let gpioc = gpioc.split();
        let gpioe = gpioe.split();
        let gpiof = gpiof.split();
        let gpiog = gpiog.split();

        Self::setup_ethernet(
            gpioa.pa1, gpioa.pa2, gpioc.pc1, gpioa.pa7,
            gpioc.pc4, gpioc.pc5, gpiob.pb11, gpiog.pg13,
            gpiob.pb13
        );
        let adc_spi = Self::setup_spi_adc(clocks, spi2, gpiob.pb10, gpiob.pb14, gpiob.pb15);
        let adc_nss = gpiob.pb12.into_push_pull_output();

        let (dac0_spi, dac0_sync) = Self::setup_dac0(
            clocks, spi4,
            gpioe.pe2, gpioe.pe4, gpioe.pe6
        );
        let (dac1_spi, dac1_sync) = Self::setup_dac1(
            clocks, spi5,
            gpiof.pf7, gpiof.pf6, gpiof.pf9
        );

        let pwm = PwmPins::setup(
            clocks, tim1, tim3,
            gpioc.pc6, gpioc.pc7,
            gpioe.pe9, gpioe.pe11,
            gpioe.pe13, gpioe.pe14
        );

        Pins {
            adc_spi, adc_nss,
            pwm,
            dac0_spi, dac0_sync,
            dac1_spi, dac1_sync,
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

    fn setup_dac0<M1, M2, M3>(
        clocks: Clocks, spi4: SPI4,
        sclk: PE2<M1>, sync: PE4<M2>, sdin: PE6<M3>
    ) -> (Dac0Spi, PE4<Output<PushPull>>) {
        let sclk = sclk.into_alternate_af5();
        let sdin = sdin.into_alternate_af5();
        let spi = Spi::spi4(
            spi4,
            (sclk, NoMiso, sdin),
            crate::ad5680::SPI_MODE,
            crate::ad5680::SPI_CLOCK.into(),
            clocks
        );
        let sync = sync.into_push_pull_output();

        (spi, sync)
    }

    fn setup_dac1<M1, M2, M3>(
        clocks: Clocks, spi5: SPI5,
        sclk: PF7<M1>, sync: PF6<M2>, sdin: PF9<M3>
    ) -> (Dac1Spi, PF6<Output<PushPull>>) {
        let sclk = sclk.into_alternate_af5();
        let sdin = sdin.into_alternate_af5();
        let spi = Spi::spi5(
            spi5,
            (sclk, NoMiso, sdin),
            crate::ad5680::SPI_MODE,
            crate::ad5680::SPI_CLOCK.into(),
            clocks
        );
        let sync = sync.into_push_pull_output();

        (spi, sync)
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

pub struct PwmPins {
    pub max_v0: PwmChannels<TIM3, pwm::C1>,
    pub max_v1: PwmChannels<TIM3, pwm::C2>,
    pub max_i_pos0: PwmChannels<TIM1, pwm::C1>,
    pub max_i_pos1: PwmChannels<TIM1, pwm::C2>,
    pub max_i_neg0: PwmChannels<TIM1, pwm::C3>,
    pub max_i_neg1: PwmChannels<TIM1, pwm::C4>,
}

impl PwmPins {
    fn setup<M1, M2, M3, M4, M5, M6>(
        clocks: Clocks,
        tim1: TIM1,
        tim3: TIM3,
        max_v0: PC6<M1>,
        max_v1: PC7<M2>,
        max_i_pos0: PE9<M3>,
        max_i_pos1: PE11<M4>,
        max_i_neg0: PE13<M5>,
        max_i_neg1: PE14<M6>,
    ) -> PwmPins {
        let freq = 20u32.khz();

        let channels = (
            max_v0.into_alternate_af2(),
            max_v1.into_alternate_af2(),
        );
        let (max_v0, max_v1) = pwm::tim3(tim3, channels, clocks, freq);

        let channels = (
            max_i_pos0.into_alternate_af1(),
            max_i_pos1.into_alternate_af1(),
            max_i_neg0.into_alternate_af1(),
            max_i_neg1.into_alternate_af1(),
        );
        let (max_i_pos0, max_i_pos1, max_i_neg0, max_i_neg1) =
            pwm::tim1(tim1, channels, clocks, freq);

        PwmPins {
            max_v0, max_v1,
            max_i_pos0, max_i_pos1,
            max_i_neg0, max_i_neg1,
        }
    }
}
