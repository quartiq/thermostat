use stm32f4xx_hal::hal::digital::v2::OutputPin;
use smoltcp::time::Instant;
use crate::{
    ad7172,
    channel::{Channel, Channel0, Channel1},
    channel_state::ChannelState,
    pins,
    units::Volts,
};

pub const CHANNELS: usize = 2;

// TODO: -pub
pub struct Channels {
    pub channel0: Channel<Channel0>,
    pub channel1: Channel<Channel1>,
    pub adc: ad7172::Adc<pins::AdcSpi, pins::AdcNss>,
    pub tec_u_meas_adc: pins::TecUMeasAdc,
    pub pwm: pins::PwmPins,
}

impl Channels {
    pub fn new(pins: pins::Pins) -> Self {
        let channel0 = Channel::new(pins.channel0);
        let channel1 = Channel::new(pins.channel1);
        let tec_u_meas_adc = pins.tec_u_meas_adc;
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

        Channels { channel0, channel1, adc, tec_u_meas_adc, pwm }
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
        match channel {
            0 => {
                self.channel0.dac.set(voltage).unwrap();
                self.channel0.state.dac_value = voltage;
                self.channel0.shdn.set_high().unwrap();
            }
            1 => {
                self.channel1.dac.set(voltage).unwrap();
                self.channel1.state.dac_value = voltage;
                self.channel1.shdn.set_high().unwrap();
            }
            _ => unreachable!(),
        }
    }

    pub fn read_dac_feedback(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.channel0.adc.convert(
                    &self.channel0.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.channel0.adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.channel1.adc.convert(
                    &self.channel1.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.channel1.adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_itec(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.channel0.adc.convert(
                    &self.channel0.itec_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.channel0.adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.channel1.adc.convert(
                    &self.channel1.itec_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.channel1.adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_vref(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.channel0.adc.convert(
                    &self.channel0.vref_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.channel0.adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.channel1.adc.convert(
                    &self.channel1.vref_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.channel1.adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_tec_u_meas(&mut self, channel: usize) -> Volts {
        match channel {
            0 => {
                let sample = self.tec_u_meas_adc.convert(
                    &self.channel0.tec_u_meas_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.tec_u_meas_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            1 => {
                let sample = self.tec_u_meas_adc.convert(
                    &self.channel1.tec_u_meas_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.tec_u_meas_adc.sample_to_millivolts(sample);
                Volts(mv as f64 / 1000.0)
            }
            _ => unreachable!(),
        }
    }
}
