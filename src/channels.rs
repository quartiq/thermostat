use smoltcp::time::Instant;
use log::info;
use crate::{
    ad5680,
    ad7172,
    channel::{Channel, Channel0, Channel1},
    channel_state::ChannelState,
    pins,
    units::Volts,
};

pub const CHANNELS: usize = 2;

// TODO: -pub
pub struct Channels {
    channel0: Channel<Channel0>,
    channel1: Channel<Channel1>,
    pub adc: ad7172::Adc<pins::AdcSpi, pins::AdcNss>,
    /// stm32f4 integrated adc
    pins_adc: pins::PinsAdc,
    pub pwm: pins::PwmPins,
}

impl Channels {
    pub fn new(pins: pins::Pins) -> Self {
        let channel0 = Channel::new(pins.channel0);
        let channel1 = Channel::new(pins.channel1);
        let pins_adc = pins.pins_adc;
        let pwm = pins.pwm;

        let mut adc = ad7172::Adc::new(pins.adc_spi, pins.adc_nss).unwrap();
        // Feature not used
        adc.set_sync_enable(false).unwrap();

        // Calibrate ADC channels individually
        adc.disable_all_channels().unwrap();

        adc.setup_channel(0, ad7172::Input::Ain0, ad7172::Input::Ain1).unwrap();
        adc.calibrate().unwrap();
        adc.disable_channel(0).unwrap();

        adc.setup_channel(1, ad7172::Input::Ain2, ad7172::Input::Ain3).unwrap();
        adc.calibrate().unwrap();
        adc.disable_channel(1).unwrap();

        // Setup channels and start ADC
        adc.setup_channel(0, ad7172::Input::Ain0, ad7172::Input::Ain1).unwrap();
        adc.setup_channel(1, ad7172::Input::Ain2, ad7172::Input::Ain3).unwrap();
        adc.start_continuous_conversion().unwrap();

        Channels { channel0, channel1, adc, pins_adc, pwm }
    }

    pub fn channel_state<I: Into<usize>>(&mut self, channel: I) -> &mut ChannelState {
        match channel.into() {
            0 => &mut self.channel0.state,
            1 => &mut self.channel1.state,
            _ => unreachable!(),
        }
    }

    /// ADC input + PID processing
    pub fn poll_adc(&mut self, instant: Instant) -> Option<u8> {
        self.adc.data_ready().unwrap().map(|channel| {
            let data = self.adc.read_data().unwrap();

            let dac_value = {
                let state = self.channel_state(channel);
                let pid_output = state.update_pid(instant, data);

                if state.pid_engaged {
                    Some(pid_output)
                } else {
                    None
                }
            };
            if let Some(dac_value) = dac_value {
                // Forward PID output to i_set DAC
                self.set_dac(channel.into(), Volts(dac_value));
            }

            channel
        })
    }

    /// i_set DAC
    pub fn set_dac(&mut self, channel: usize, voltage: Volts) {
        let dac_factor = match channel.into() {
            0 => self.channel0.dac_factor,
            1 => self.channel1.dac_factor,
            _ => unreachable!(),
        };
        let value = (voltage.0 * dac_factor) as u32;
        match channel {
            0 => {
                self.channel0.dac.set(value).unwrap();
                self.channel0.state.dac_value = voltage;
            }
            1 => {
                self.channel1.dac.set(value).unwrap();
                self.channel1.state.dac_value = voltage;
            }
            _ => unreachable!(),
        }
    }

    pub fn read_dac_feedback(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                info!("dac0_fb: {}/{:03X}", mv, sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                info!("dac1_fb: {}/{:03X}", mv, sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_dac_feedback_until_stable(&mut self, channel: usize, tolerance: f64) -> Volts {
        let mut prev = self.read_dac_feedback(channel);
        loop {
            let current = self.read_dac_feedback(channel);
            use num_traits::float::Float;
            if (current - prev).0.abs() < tolerance {
                return current;
            }
            prev = current;
        }
    }

    pub fn read_itec(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.itec_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.itec_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }

    /// should be 1.5V
    pub fn read_vref(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.vref_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.vref_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_tec_u_meas(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.tec_u_meas_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.tec_u_meas_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }

    /// for i_set
    pub fn calibrate_dac_value(&mut self, channel: usize) {
        let vref = self.read_vref(channel);
        let value = self.calibrate_dac_value_for_voltage(channel, vref);
        info!("best dac value for {}: {}", vref, value);

        let dac_factor = value as f64 / vref.0;
        match channel {
            0 => self.channel0.dac_factor = dac_factor,
            1 => self.channel1.dac_factor = dac_factor,
            _ => unreachable!(),
        }
    }

    fn calibrate_dac_value_for_voltage(&mut self, channel: usize, voltage: Volts) -> u32 {
        let mut best_value = 0;
        let mut best_error = Volts(100.0);

        for step in (1..=12).rev() {
            for value in (best_value..=ad5680::MAX_VALUE).step_by(2usize.pow(step)) {
                match channel {
                    0 => {
                        self.channel0.dac.set(value).unwrap();
                        // self.channel0.shdn.set_high().unwrap();
                    }
                    1 => {
                        self.channel1.dac.set(value).unwrap();
                        // self.channel1.shdn.set_high().unwrap();
                    }
                    _ => unreachable!(),
                }

                let dac_feedback = self.read_dac_feedback_until_stable(channel, 0.001);
                let error = voltage - dac_feedback;
                if error < Volts(0.0) {
                    break;
                } else if error < best_error {
                    best_value = value;
                    best_error = error;
                }
            }
        }

        self.set_dac(channel, Volts(0.0));
        best_value
    }
}
