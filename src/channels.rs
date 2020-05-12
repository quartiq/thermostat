use crate::{
    ad7172,
    channel::{Channel, Channel0, Channel1},
    pins,
};

pub struct Channels {
    pub channel0: Channel<Channel0>,
    pub channel1: Channel<Channel1>,
    pub adc: ad7172::Adc<pins::AdcSpi, pins::AdcNss>,
    pub pwm: pins::PwmPins,
}

impl Channels {
    pub fn new(pins: pins::Pins) -> Self {
        let channel0 = Channel::new(pins.channel0);
        let channel1 = Channel::new(pins.channel1);
        let pwm = pins.pwm;

        let mut adc = ad7172::Adc::new(pins.adc_spi, pins.adc_nss).unwrap();
        // Feature not used
        adc.set_sync_enable(false).unwrap();
        // Setup channels
        adc.setup_channel(0, ad7172::Input::Ain0, ad7172::Input::Ain1).unwrap();
        adc.setup_channel(1, ad7172::Input::Ain2, ad7172::Input::Ain3).unwrap();
        adc.calibrate_offset().unwrap();

        Channels { channel0, channel1, adc, pwm }
    }
}
