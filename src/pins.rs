use stm32_eth::hal::{
    adc::Adc,
    gpio::{
        AF5, Alternate, AlternateOD, Analog, Floating, Input,
        gpioa::*,
        gpiob::*,
        gpioc::*,
        gpioe::*,
        gpiof::*,
        gpiog::*,
        GpioExt,
        Output, PushPull,
    },
    hal::{self, blocking::spi::Transfer, digital::v2::OutputPin},
    i2c::I2c,
    // otg_fs::USB,
    rcc::Clocks,
    pwm::{self, PwmChannels},
    spi::{Spi, NoMiso},
    stm32::{
        ADC1,
        GPIOA, GPIOB, GPIOC, GPIOD, GPIOE, GPIOF, GPIOG,
        I2C1,
        OTG_FS_GLOBAL, OTG_FS_DEVICE, OTG_FS_PWRCLK,
        SPI2, SPI4, SPI5,
        TIM1, TIM3,
    },
    time::U32Ext,
};
use eeprom24x::{self, Eeprom24x};
use stm32_eth::EthPins;
use crate::{
    // channel::{Channel0, Channel1},
    leds::Leds,
};

pub type Eeprom = Eeprom24x<
    I2c<I2C1, (
        PB8<AlternateOD<stm32_eth::hal::gpio::AF4>>,
        PB9<AlternateOD<stm32_eth::hal::gpio::AF4>>
    )>,
    eeprom24x::page_size::B8,
    eeprom24x::addr_size::OneByte
>;

pub type EthernetPins = EthPins<
    PA1<Input<Floating>>,
    PA2<Input<Floating>>,
    PC1<Input<Floating>>,
    PA7<Input<Floating>>,
    PB11<Input<Floating>>,
    PG13<Input<Floating>>,
    PB13<Input<Floating>>,
    PC4<Input<Floating>>,
    PC5<Input<Floating>>,
 >;

pub trait ChannelPins {
    type DacSpi: Transfer<u8>;
    type DacSync: OutputPin;
    type Shdn: OutputPin;
    type VRefPin;
    type ItecPin;
    type DacFeedbackPin;
    type TecUMeasPin;
}

/*
impl ChannelPins for Channel0 {
    type DacSpi = Dac0Spi;
    type DacSync = PE4<Output<PushPull>>;
    type Shdn = PE10<Output<PushPull>>;
    type VRefPin = PA0<Analog>;
    type ItecPin = PA6<Analog>;
    type DacFeedbackPin = PA4<Analog>;
    type TecUMeasPin = PC2<Analog>;
}

impl ChannelPins for Channel1 {
    type DacSpi = Dac1Spi;
    type DacSync = PF6<Output<PushPull>>;
    type Shdn = PE15<Output<PushPull>>;
    type VRefPin = PA3<Analog>;
    type ItecPin = PB0<Analog>;
    type DacFeedbackPin = PA5<Analog>;
    type TecUMeasPin = PC3<Analog>;
}
*/

/// SPI peripheral used for communication with the ADC
pub type AdcSpi = Spi<SPI2, (PB10<Alternate<AF5>>, PB14<Alternate<AF5>>, PB15<Alternate<AF5>>)>;
pub type AdcNss = PB12<Output<PushPull>>;
type Dac0Spi = Spi<SPI4, (PE2<Alternate<AF5>>, NoMiso, PE6<Alternate<AF5>>)>;
type Dac1Spi = Spi<SPI5, (PF7<Alternate<AF5>>, NoMiso, PF9<Alternate<AF5>>)>;
pub type PinsAdc = Adc<ADC1>;

pub struct ChannelPinSet<C: ChannelPins> {
    pub dac_spi: C::DacSpi,
    pub dac_sync: C::DacSync,
    pub shdn: C::Shdn,
    pub vref_pin: C::VRefPin,
    pub itec_pin: C::ItecPin,
    pub dac_feedback_pin: C::DacFeedbackPin,
    pub tec_u_meas_pin: C::TecUMeasPin,
}

pub struct Pins {
    // pub adc_spi: AdcSpi,
    pub adc_nss: AdcNss,
    pub pins_adc: PinsAdc,
    pub pwm: PwmPins,
    //pub channel0: ChannelPinSet<Channel0>,
    //pub channel1: ChannelPinSet<Channel1>,
}

impl Pins {
    /// Setup GPIO pins and configure MCU peripherals
    pub fn setup(
        clocks: Clocks,
        tim1: TIM1, tim3: TIM3,
        gpioa: GPIOA, gpiob: GPIOB, gpioc: GPIOC, gpiod: GPIOD, gpioe: GPIOE, gpiof: GPIOF, gpiog: GPIOG,
        i2c1: I2C1,
        spi2: SPI2, spi4: SPI4, spi5: SPI5,
        adc1: ADC1,
        otg_fs_global: OTG_FS_GLOBAL, otg_fs_device: OTG_FS_DEVICE, otg_fs_pwrclk: OTG_FS_PWRCLK,
    ) -> (Self, Leds, Eeprom, EthernetPins) {
        let gpioa = gpioa.split();
        let gpiob = gpiob.split();
        let gpioc = gpioc.split();
        let gpiod = gpiod.split();
        let gpioe = gpioe.split();
        let gpiof = gpiof.split();
        let gpiog = gpiog.split();

        // let adc_spi = Self::setup_spi_adc(clocks, spi2, gpiob.pb10, gpiob.pb14, gpiob.pb15);
        let adc_nss = gpiob.pb12.into_push_pull_output();

        let pins_adc = Adc::adc1(adc1, true, Default::default());

        let pwm = PwmPins::setup(
            clocks, tim1, tim3,
            gpioc.pc6, gpioc.pc7,
            gpioe.pe9, gpioe.pe11,
            gpioe.pe13, gpioe.pe14
        );

        /*
        let (dac0_spi, dac0_sync) = Self::setup_dac0(
            clocks, spi4,
            gpioe.pe2, gpioe.pe4, gpioe.pe6
        );
        let mut shdn0 = gpioe.pe10.into_push_pull_output();
        let _ = shdn0.set_low();
        let vref0_pin = gpioa.pa0.into_analog();
        let itec0_pin = gpioa.pa6.into_analog();
        let dac_feedback0_pin = gpioa.pa4.into_analog();
        let tec_u_meas0_pin = gpioc.pc2.into_analog();
        let channel0 = ChannelPinSet {
            dac_spi: dac0_spi,
            dac_sync: dac0_sync,
            shdn: shdn0,
            vref_pin: vref0_pin,
            itec_pin: itec0_pin,
            dac_feedback_pin: dac_feedback0_pin,
            tec_u_meas_pin: tec_u_meas0_pin,
        };

        let (dac1_spi, dac1_sync) = Self::setup_dac1(
            clocks, spi5,
            gpiof.pf7, gpiof.pf6, gpiof.pf9
        );
        let mut shdn1 = gpioe.pe15.into_push_pull_output();
        let _ = shdn1.set_low();
        let vref1_pin = gpioa.pa3.into_analog();
        let itec1_pin = gpiob.pb0.into_analog();
        let dac_feedback1_pin = gpioa.pa5.into_analog();
        let tec_u_meas1_pin = gpioc.pc3.into_analog();
        let channel1 = ChannelPinSet {
            dac_spi: dac1_spi,
            dac_sync: dac1_sync,
            shdn: shdn1,
            vref_pin: vref1_pin,
            itec_pin: itec1_pin,
            dac_feedback_pin: dac_feedback1_pin,
            tec_u_meas_pin: tec_u_meas1_pin,
        };
        */

        let pins = Pins {
            // adc_spi,
            adc_nss,
            pins_adc,
            pwm,
            //channel0,
            //channel1,
        };

        let leds = Leds::new(gpiod.pd9, gpiod.pd10.into_push_pull_output(), gpiod.pd11.into_push_pull_output());

        let eeprom_scl = gpiob.pb8.into_alternate_af4().set_open_drain();
        let eeprom_sda = gpiob.pb9.into_alternate_af4().set_open_drain();
        let eeprom_i2c = I2c::i2c1(i2c1, (eeprom_scl, eeprom_sda), 400.khz(), clocks);
        let eeprom = Eeprom24x::new_24x02(eeprom_i2c, eeprom24x::SlaveAddr::default());

        let eth_pins = EthPins {
            ref_clk: gpioa.pa1,
            md_io: gpioa.pa2,
            md_clk: gpioc.pc1,
            crs: gpioa.pa7,
            tx_en: gpiob.pb11,
            tx_d0: gpiog.pg13,
            tx_d1: gpiob.pb13,
            rx_d0: gpioc.pc4,
            rx_d1: gpioc.pc5,
        };

        /*
        let usb = USB {
            usb_global: otg_fs_global,
            usb_device: otg_fs_device,
            usb_pwrclk: otg_fs_pwrclk,
            pin_dm: gpioa.pa11.into_alternate_af10(),
            pin_dp: gpioa.pa12.into_alternate_af10(),
            // hclk: clocks.hclk(),
        };
        */
        (pins, leds, eeprom, eth_pins)
    }
/*
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
    ) -> (Dac0Spi, <Channel0 as ChannelPins>::DacSync) {
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
    ) -> (Dac1Spi, <Channel1 as ChannelPins>::DacSync) {
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
*/
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

        fn init_pwm_pin<P: hal::PwmPin<Duty=u16>>(pin: &mut P) {
            pin.set_duty(0);
            pin.enable();
        }
        let channels = (
            max_v0.into_alternate_af2(),
            max_v1.into_alternate_af2(),
        );
        let (mut max_v0, mut max_v1) = pwm::tim3(tim3, channels, clocks, freq);
        init_pwm_pin(&mut max_v0);
        init_pwm_pin(&mut max_v1);

        let channels = (
            max_i_pos0.into_alternate_af1(),
            max_i_pos1.into_alternate_af1(),
            max_i_neg0.into_alternate_af1(),
            max_i_neg1.into_alternate_af1(),
        );
        let (mut max_i_pos0, mut max_i_pos1, mut max_i_neg0, mut max_i_neg1) =
            pwm::tim1(tim1, channels, clocks, freq);
        init_pwm_pin(&mut max_i_pos0);
        init_pwm_pin(&mut max_i_neg0);
        init_pwm_pin(&mut max_i_pos1);
        init_pwm_pin(&mut max_i_neg1);

        PwmPins {
            max_v0, max_v1,
            max_i_pos0, max_i_pos1,
            max_i_neg0, max_i_neg1,
        }
    }
}
