use stm32f4xx_hal::hal;
use smoltcp::time::Instant;
use uom::si::{
    f64::{ElectricCurrent, ElectricPotential, ElectricalResistance},
    electric_potential::{millivolt, volt},
    electric_current::ampere,
    electrical_resistance::ohm,
    ratio::ratio,
};
use log::info;
use crate::{
    ad5680,
    ad7172,
    channel::{Channel, Channel0, Channel1},
    channel_state::ChannelState,
    command_parser::PwmPin,
    pins,
};

pub const CHANNELS: usize = 2;
pub const R_SENSE: f64 = 0.05;

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
        let mut adc = ad7172::Adc::new(pins.adc_spi, pins.adc_nss).unwrap();
        // Feature not used
        adc.set_sync_enable(false).unwrap();

        // Setup channels and start ADC
        adc.setup_channel(0, ad7172::Input::Ain0, ad7172::Input::Ain1).unwrap();
        let adc_calibration0 = adc.get_calibration(0)
            .expect("adc_calibration0");
        adc.setup_channel(1, ad7172::Input::Ain2, ad7172::Input::Ain3).unwrap();
        let adc_calibration1 = adc.get_calibration(1)
            .expect("adc_calibration1");
        adc.start_continuous_conversion().unwrap();

        let mut channel0 = Channel::new(pins.channel0, adc_calibration0);
        let mut channel1 = Channel::new(pins.channel1, adc_calibration1);
        let pins_adc = pins.pins_adc;
        let pwm = pins.pwm;
        let mut channels = Channels { channel0, channel1, adc, pins_adc, pwm };
        for channel in 0..CHANNELS {
            channels.channel_state(channel).vref = channels.read_vref(channel);
            channels.calibrate_dac_value(channel);
        }
        channels
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
                state.update(instant, data);
                let pid_output = state.update_pid();

                if state.pid_engaged {
                    Some(pid_output)
                } else {
                    None
                }
            };
            if let Some(dac_value) = dac_value {
                // Forward PID output to i_set DAC
                // TODO:
                // self.set_dac(channel.into(), ElectricPotential::new::<volt>(dac_value));
            }

            channel
        })
    }

    /// i_set DAC
    fn get_dac(&mut self, channel: usize) -> (ElectricPotential, ElectricPotential) {
        let dac_factor = match channel.into() {
            0 => self.channel0.dac_factor,
            1 => self.channel1.dac_factor,
            _ => unreachable!(),
        };
        let voltage = self.channel_state(channel).dac_value;
        let max = ElectricPotential::new::<volt>(ad5680::MAX_VALUE as f64 / dac_factor);
        (voltage, max)
    }

    pub fn get_i(&mut self, channel: usize) -> (ElectricCurrent, ElectricCurrent) {
        let vref = self.channel_state(channel).vref;
        let r_sense = ElectricalResistance::new::<ohm>(R_SENSE);
        let (voltage, max) = self.get_dac(channel);
        let i_tec = (voltage - vref) / (10.0 * r_sense);
        let max = (max - vref) / (10.0 * r_sense);
        (i_tec, max)
    }

    /// i_set DAC
    fn set_dac(&mut self, channel: usize, voltage: ElectricPotential) -> (ElectricPotential, ElectricPotential) {
        let dac_factor = match channel.into() {
            0 => self.channel0.dac_factor,
            1 => self.channel1.dac_factor,
            _ => unreachable!(),
        };
        let value = (voltage.get::<volt>() * dac_factor) as u32;
        let value = match channel {
            0 => self.channel0.dac.set(value).unwrap(),
            1 => self.channel1.dac.set(value).unwrap(),
            _ => unreachable!(),
        };
        let voltage = ElectricPotential::new::<volt>(value as f64 / dac_factor);
        self.channel_state(channel).dac_value = voltage;
        let max = ElectricPotential::new::<volt>(ad5680::MAX_VALUE as f64 / dac_factor);
        (voltage, max)
    }

    pub fn set_i(&mut self, channel: usize, i_tec: ElectricCurrent) -> (ElectricCurrent, ElectricCurrent) {
        let vref = self.channel_state(channel).vref;
        let r_sense = ElectricalResistance::new::<ohm>(R_SENSE);
        let voltage = i_tec * 10.0 * r_sense + vref;
        let (voltage, max) = self.set_dac(channel, voltage);
        let i_tec = (voltage - vref) / (10.0 * r_sense);
        let max = (max - vref) / (10.0 * r_sense);
        (i_tec, max)
    }

    pub fn read_dac_feedback(&mut self, channel: usize) -> ElectricPotential {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                info!("dac0_fb: {}/{:03X}", mv, sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                info!("dac1_fb: {}/{:03X}", mv, sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_dac_feedback_until_stable(&mut self, channel: usize, tolerance: ElectricPotential) -> ElectricPotential {
        let mut prev = self.read_dac_feedback(channel);
        loop {
            let current = self.read_dac_feedback(channel);
            use num_traits::float::Float;
            if (current - prev).abs() < tolerance {
                return current;
            }
            prev = current;
        }
    }

    pub fn read_itec(&mut self, channel: usize) -> ElectricPotential {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.itec_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.itec_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            _ => unreachable!(),
        }
    }

    /// should be 1.5V
    pub fn read_vref(&mut self, channel: usize) -> ElectricPotential {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.vref_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.vref_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_tec_u_meas(&mut self, channel: usize) -> ElectricPotential {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.tec_u_meas_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.tec_u_meas_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            _ => unreachable!(),
        }
    }

    /// Calibrate the I_SET DAC using the DAC_FB ADC pin.
    ///
    /// These loops perform a width-first search for the DAC setting
    /// that will produce a `target_voltage`.
    pub fn calibrate_dac_value(&mut self, channel: usize) {
        let target_voltage = ElectricPotential::new::<volt>(2.5);
        let mut start_value = 1;
        let mut best_error = ElectricPotential::new::<volt>(100.0);

        for step in (0..18).rev() {
            let mut prev_value = start_value;
            for value in (start_value..=ad5680::MAX_VALUE).step_by(1 << step) {
                match channel {
                    0 => {
                        self.channel0.dac.set(value).unwrap();
                    }
                    1 => {
                        self.channel1.dac.set(value).unwrap();
                    }
                    _ => unreachable!(),
                }

                let dac_feedback = self.read_dac_feedback_until_stable(channel, ElectricPotential::new::<volt>(0.001));
                let error = target_voltage - dac_feedback;
                if error < ElectricPotential::new::<volt>(0.0) {
                    break;
                } else if error < best_error {
                    best_error = error;
                    start_value = prev_value;

                    let dac_factor = value as f64 / dac_feedback.get::<volt>();
                    match channel {
                        0 => self.channel0.dac_factor = dac_factor,
                        1 => self.channel1.dac_factor = dac_factor,
                        _ => unreachable!(),
                    }
                }

                prev_value = value;
            }
        }

        // Reset
        self.set_dac(channel, ElectricPotential::new::<volt>(0.0));
    }

    fn get_pwm(&self, channel: usize, pin: PwmPin) -> f64 {
        fn get<P: hal::PwmPin<Duty=u16>>(pin: &P) -> f64 {
            let duty = pin.get_duty();
            let max = pin.get_max_duty();
            duty as f64 / (max as f64)
        }
        match (channel, pin) {
            (_, PwmPin::ISet) =>
                panic!("i_set is no pwm pin"),
            (0, PwmPin::MaxIPos) =>
                get(&self.pwm.max_i_pos0),
            (0, PwmPin::MaxINeg) =>
                get(&self.pwm.max_i_neg0),
            (0, PwmPin::MaxV) =>
                get(&self.pwm.max_v0),
            (1, PwmPin::MaxIPos) =>
                get(&self.pwm.max_i_pos1),
            (1, PwmPin::MaxINeg) =>
                get(&self.pwm.max_i_neg1),
            (1, PwmPin::MaxV) =>
                get(&self.pwm.max_v1),
            _ =>
                unreachable!(),
        }
    }

    pub fn get_max_v(&mut self, channel: usize) -> (ElectricPotential, ElectricPotential) {
        let vref = self.channel_state(channel).vref;
        let duty = self.get_pwm(channel, PwmPin::MaxV);
        (duty * 4.0 * vref, 4.0 * vref)
    }

    pub fn get_max_i_pos(&mut self, channel: usize) -> (ElectricCurrent, ElectricCurrent) {
        let vref = self.channel_state(channel).vref;
        let scale = vref / ElectricPotential::new::<volt>(3.0) / ElectricCurrent::new::<ampere>(1.0);
        let duty = self.get_pwm(channel, PwmPin::MaxIPos);
        (duty / scale, 1.0 / scale)
    }

    pub fn get_max_i_neg(&mut self, channel: usize) -> (ElectricCurrent, ElectricCurrent) {
        let vref = self.channel_state(channel).vref;
        let scale = vref / ElectricPotential::new::<volt>(3.0) / ElectricCurrent::new::<ampere>(1.0);
        let duty = self.get_pwm(channel, PwmPin::MaxINeg);
        (duty / scale, 1.0 / scale)
    }

    fn set_pwm(&mut self, channel: usize, pin: PwmPin, duty: f64) -> f64 {
        fn set<P: hal::PwmPin<Duty=u16>>(pin: &mut P, duty: f64) -> f64 {
            let max = pin.get_max_duty();
            let value = ((duty * (max as f64)) as u16).min(max);
            pin.set_duty(value);
            value as f64 / (max as f64)
        }
        match (channel, pin) {
            (_, PwmPin::ISet) =>
                panic!("i_set is no pwm pin"),
            (0, PwmPin::MaxIPos) =>
                set(&mut self.pwm.max_i_pos0, duty),
            (0, PwmPin::MaxINeg) =>
                set(&mut self.pwm.max_i_neg0, duty),
            (0, PwmPin::MaxV) =>
                set(&mut self.pwm.max_v0, duty),
            (1, PwmPin::MaxIPos) =>
                set(&mut self.pwm.max_i_pos1, duty),
            (1, PwmPin::MaxINeg) =>
                set(&mut self.pwm.max_i_neg1, duty),
            (1, PwmPin::MaxV) =>
                set(&mut self.pwm.max_v1, duty),
            _ =>
                unreachable!(),
        }
    }

    pub fn set_max_v(&mut self, channel: usize, max_v: ElectricPotential) -> (ElectricPotential, ElectricPotential) {
        let vref = self.channel_state(channel).vref;
        let duty = (max_v / 4.0 / vref).get::<ratio>();
        let duty = self.set_pwm(channel, PwmPin::MaxV, duty);
        (duty * 4.0 * vref, 4.0 * vref)
    }

    pub fn set_max_i_pos(&mut self, channel: usize, max_i_pos: ElectricCurrent) -> (ElectricCurrent, ElectricCurrent) {
        let vref = self.channel_state(channel).vref;
        let scale = vref / ElectricPotential::new::<volt>(3.0) / ElectricCurrent::new::<ampere>(1.0);
        let duty = (max_i_pos * scale).get::<ratio>();
        let duty = self.set_pwm(channel, PwmPin::MaxIPos, duty);
        (duty / scale, 1.0 / scale)
    }

    pub fn set_max_i_neg(&mut self, channel: usize, max_i_neg: ElectricCurrent) -> (ElectricCurrent, ElectricCurrent) {
        let vref = self.channel_state(channel).vref;
        let scale = vref / ElectricPotential::new::<volt>(3.0) / ElectricCurrent::new::<ampere>(1.0);
        let duty = (max_i_neg * scale).get::<ratio>();
        let duty = self.set_pwm(channel, PwmPin::MaxINeg, duty);
        (duty / scale, 1.0 / scale)
    }
}
