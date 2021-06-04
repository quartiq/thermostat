use heapless::{consts::{U2, U1024}, Vec};
use serde::{Serialize, Serializer};
use smoltcp::time::Instant;
use stm32f4xx_hal::hal;
use uom::si::{
    f64::{ElectricCurrent, ElectricPotential, ElectricalResistance, Time},
    electric_potential::{millivolt, volt},
    electric_current::ampere,
    electrical_resistance::ohm,
    ratio::ratio,
    thermodynamic_temperature::degree_celsius,
};
use crate::{
    ad5680,
    ad7172,
    channel::{Channel, Channel0, Channel1},
    channel_state::ChannelState,
    command_parser::{CenterPoint, PwmPin},
    pins,
    steinhart_hart,
    iir_array,
};

pub const CHANNELS: usize = 2;
pub const R_SENSE: f64 = 0.05;
// DAC chip outputs 0-5v, which is then passed through a resistor dividor to provide 0-3v range
const DAC_OUT_V_MAX: f64 = 3.0;

// TODO: -pub
pub struct Channels {
    channel0: Channel<Channel0>,
    channel1: Channel<Channel1>,
    pub adc: ad7172::Adc<pins::AdcSpi, pins::AdcNss>,
    /// stm32f4 integrated adc
    pins_adc: pins::PinsAdc,
    pub pwm: pins::PwmPins,
    pub iirs: iir_array::IirMatrix,
}

impl Channels {
    pub fn new(pins: pins::Pins) -> Self {
        let mut adc = ad7172::Adc::new(pins.adc_spi, pins.adc_nss).unwrap();
        // Feature not used
        adc.set_sync_enable(false).unwrap();

        // Setup channels and start ADC
        adc.setup_channel(0, ad7172::Input::Ain2, ad7172::Input::Ain3).unwrap();
        let adc_calibration0 = adc.get_calibration(0)
            .expect("adc_calibration0");
        adc.setup_channel(1, ad7172::Input::Ain0, ad7172::Input::Ain1).unwrap();
        let adc_calibration1 = adc.get_calibration(1)
            .expect("adc_calibration1");
        adc.start_continuous_conversion().unwrap();

        let channel0 = Channel::new(pins.channel0, adc_calibration0);
        let channel1 = Channel::new(pins.channel1, adc_calibration1);
        let pins_adc = pins.pins_adc;
        let pwm = pins.pwm;
        let iirs = iir_array::IirMatrix::new();
        let mut channels = Channels { channel0, channel1, adc, pins_adc, pwm, iirs };
        for channel in 0..CHANNELS {
            channels.channel_state(channel).vref = channels.read_vref(channel);
            channels.calibrate_dac_value(channel);
            channels.set_i(channel, ElectricCurrent::new::<ampere>(0.0));
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
            let current = self.get_tec_i(channel.into());
            self.channel_state(channel).update(instant, data);
            match self.channel_state(channel).update_pid(current) {
                Some(pid_output) if self.channel_state(channel).pid_engaged => {
                    // Forward PID output to i_set DAC
                    self.set_i(channel.into(), ElectricCurrent::new::<ampere>(pid_output));
                    self.power_up(channel);
                }
                None if self.channel_state(channel).pid_engaged => {
                    self.power_down(channel);
                }
                _ => {}
            }

            let iir_output = self.channel_state(channel).update_iir();
            if self.channel_state(channel).iir_engaged {
                self.set_i(channel.into(), ElectricCurrent::new::<ampere>(iir_output.unwrap()));
                self.power_up(channel);
            }

            // only update iir matrix once (with channel0)
            if channel == 0 {
                self.iirs.tick(&mut self.channel0.state, &mut self.channel1.state);
            }

            // check if and which iir output goes to pwm
            let iir_out = self.channel_state(channel).matrix_engaged;
            if iir_out != 0 {
                self.set_i(channel.into(), ElectricCurrent::new::<ampere>(self.iirs.iirarray[iir_out-1].xy[2]));
                self.power_up(channel);
            }
            channel
        })
    }

    /// calculate the TEC i_set centerpoint
    pub fn get_center(&mut self, channel: usize) -> ElectricPotential {
        match self.channel_state(channel).center {
            CenterPoint::Vref => {
                let vref = self.read_vref(channel);
                self.channel_state(channel).vref = vref;
                vref
            },
            CenterPoint::Override(center_point) =>
                ElectricPotential::new::<volt>(center_point.into()),
        }
    }

    /// i_set DAC
    fn get_dac(&mut self, channel: usize) -> ElectricPotential {
        let voltage = self.channel_state(channel).dac_value;
        voltage
    }

    pub fn get_i(&mut self, channel: usize) -> ElectricCurrent {
        let center_point = self.get_center(channel);
        let r_sense = ElectricalResistance::new::<ohm>(R_SENSE);
        let voltage = self.get_dac(channel);
        let i_tec = (voltage - center_point) / (10.0 * r_sense);
        i_tec
    }

    /// i_set DAC
    fn set_dac(&mut self, channel: usize, voltage: ElectricPotential) -> ElectricPotential {
        let value = ((voltage / ElectricPotential::new::<volt>(DAC_OUT_V_MAX)).get::<ratio>() * (ad5680::MAX_VALUE as f64)) as u32 ;
        match channel {
            0 => self.channel0.dac.set(value).unwrap(),
            1 => self.channel1.dac.set(value).unwrap(),
            _ => unreachable!(),
        };
        self.channel_state(channel).dac_value = voltage;
        voltage
    }

    pub fn set_i(&mut self, channel: usize, i_tec: ElectricCurrent) -> ElectricCurrent {
        let vref_meas = match channel.into() {
            0 => self.channel0.vref_meas,
            1 => self.channel1.vref_meas,
            _ => unreachable!(),
        };
        let center_point = vref_meas;
        let r_sense = ElectricalResistance::new::<ohm>(R_SENSE);
        let voltage = i_tec * 10.0 * r_sense + center_point;
        let voltage = self.set_dac(channel, voltage);
        let i_tec = (voltage - center_point) / (10.0 * r_sense);
        i_tec
    }

    pub fn read_dac_feedback(&mut self, channel: usize) -> ElectricPotential {
        match channel {
            0 => {
                let sample = self.pins_adc.convert(
                    &self.channel0.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            1 => {
                let sample = self.pins_adc.convert(
                    &self.channel1.dac_feedback_pin,
                    stm32f4xx_hal::adc::config::SampleTime::Cycles_480
                );
                let mv = self.pins_adc.sample_to_millivolts(sample);
                ElectricPotential::new::<millivolt>(mv as f64)
            }
            _ => unreachable!(),
        }
    }

    pub fn read_dac_feedback_until_stable(&mut self, channel: usize, tolerance: ElectricPotential) -> ElectricPotential {
        let mut prev = self.read_dac_feedback(channel);
        loop {
            let current = self.read_dac_feedback(channel);
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

    /// Calibrates the DAC output to match vref of the MAX driver to reduce zero-current offset of the MAX driver output.
    ///
    /// The thermostat DAC applies a control voltage signal to the CTLI pin of MAX driver chip to control its output current.
    /// The CTLI input signal is centered around VREF of the MAX chip. Applying VREF to CTLI sets the output current to 0.
    ///
    /// This calibration routine measures the VREF voltage and the DAC output with the STM32 ADC, and uses a breadth-first
    /// search to find the DAC setting that will produce a DAC output voltage closest to VREF. This DAC output voltage will
    /// be stored and used in subsequent i_set routines to bias the current control signal to the measured VREF, reducing
    /// the offset error of the current control signal.
    ///
    /// The input offset of the STM32 ADC is eliminated by using the same ADC for the measurements, and by only using the
    /// difference in VREF and DAC output for the calibration.
    ///
    /// This routine should be called only once after boot, repeated reading of the vref signal and changing of the stored
    /// VREF measurement can introduce significant noise at the current output, degrading the stabilily performance of the
    /// thermostat.
    pub fn calibrate_dac_value(&mut self, channel: usize) {
        let samples = 50;
        let mut target_voltage = ElectricPotential::new::<volt>(0.0);
        for _ in 0..samples {
            target_voltage = target_voltage + self.get_center(channel);
        }
        target_voltage = target_voltage / samples as f64;
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

                    let vref = (value as f64 / ad5680::MAX_VALUE as f64) * ElectricPotential::new::<volt>(DAC_OUT_V_MAX);
                    match channel {
                        0 => self.channel0.vref_meas = vref,
                        1 => self.channel1.vref_meas = vref,
                        _ => unreachable!(),
                    }
                }

                prev_value = value;
            }
        }

        // Reset
        self.set_dac(channel, ElectricPotential::new::<volt>(0.0));
    }

    // power up TEC
    pub fn power_up<I: Into<usize>>(&mut self, channel: I) {
        match channel.into() {
            0 => self.channel0.power_up(),
            1 => self.channel1.power_up(),
            _ => unreachable!(),
        }
    }

    // power down TEC
    pub fn power_down<I: Into<usize>>(&mut self, channel: I) {
        match channel.into() {
            0 => self.channel0.power_down(),
            1 => self.channel1.power_down(),
            _ => unreachable!(),
        }
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

    pub fn get_max_v(&mut self, channel: usize) -> ElectricPotential {
        let max = 4.0 * ElectricPotential::new::<volt>(3.3);
        let duty = self.get_pwm(channel, PwmPin::MaxV);
        duty * max
    }

    pub fn get_max_i_pos(&mut self, channel: usize) -> (ElectricCurrent, ElectricCurrent) {
        let max = ElectricCurrent::new::<ampere>(3.0);
        let duty = self.get_pwm(channel, PwmPin::MaxIPos);
        (duty * max, max)
    }

    pub fn get_max_i_neg(&mut self, channel: usize) -> (ElectricCurrent, ElectricCurrent) {
        let max = ElectricCurrent::new::<ampere>(3.0);
        let duty = self.get_pwm(channel, PwmPin::MaxINeg);
        (duty * max, max)
    }

    // Get current passing through TEC
    pub fn get_tec_i(&mut self, channel: usize) -> ElectricCurrent {
        (self.read_itec(channel) - self.read_vref(channel)) / ElectricalResistance::new::<ohm>(0.4)
    }

    // Get voltage across TEC
    pub fn get_tec_v(&mut self, channel: usize) -> ElectricPotential {
        (self.read_tec_u_meas(channel) - ElectricPotential::new::<volt>(1.5)) * 4.0
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
        let max = 4.0 * ElectricPotential::new::<volt>(3.3);
        let duty = (max_v / max).get::<ratio>();
        let duty = self.set_pwm(channel, PwmPin::MaxV, duty);
        (duty * max, max)
    }

    pub fn set_max_i_pos(&mut self, channel: usize, max_i_pos: ElectricCurrent) -> (ElectricCurrent, ElectricCurrent) {
        let max = ElectricCurrent::new::<ampere>(3.0);
        let duty = (max_i_pos / max).get::<ratio>();
        let duty = self.set_pwm(channel, PwmPin::MaxIPos, duty);
        (duty * max, max)
    }

    pub fn set_max_i_neg(&mut self, channel: usize, max_i_neg: ElectricCurrent) -> (ElectricCurrent, ElectricCurrent) {
        let max = ElectricCurrent::new::<ampere>(3.0);
        let duty = (max_i_neg / max).get::<ratio>();
        let duty = self.set_pwm(channel, PwmPin::MaxINeg, duty);
        (duty * max, max)
    }

    fn report(&mut self, channel: usize) -> Report {
        let vref = self.channel_state(channel).vref;
        let i_set = self.get_i(channel);
        let i_tec = self.read_itec(channel);
        let tec_i = self.get_tec_i(channel);
        let dac_value = self.get_dac(channel);
        let state = self.channel_state(channel);
        let pid_output = state.pid.last_output.map(|last_output|
            ElectricCurrent::new::<ampere>(last_output)
        );
        Report {
            channel,
            time: state.get_adc_time(),
            interval: state.get_adc_interval(),
            adc: state.get_adc(),
            sens: state.get_sens(),
            temperature: state.get_temperature()
                .map(|temperature| temperature.get::<degree_celsius>()),
            pid_engaged: state.pid_engaged,
            i_set,
            vref,
            dac_value,
            dac_feedback: self.read_dac_feedback(channel),
            i_tec,
            tec_i,
            tec_u_meas: self.get_tec_v(channel),
            pid_output,
        }
    }

    pub fn reports_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut reports = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = reports.push(self.report(channel));
        }
        serde_json_core::to_vec(&reports)
    }

    pub fn pid_summaries_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.channel_state(channel).pid.summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }

    fn pwm_summary(&mut self, channel: usize) -> PwmSummary {
        PwmSummary {
            channel,
            center: CenterPointJson(self.channel_state(channel).center.clone()),
            i_set: (self.get_i(channel), ElectricCurrent::new::<ampere>(3.0)).into(),
            max_v: (self.get_max_v(channel), ElectricPotential::new::<volt>(5.0)).into(),
            max_i_pos: self.get_max_i_pos(channel).into(),
            max_i_neg: self.get_max_i_neg(channel).into(),
        }
    }

    pub fn pwm_summaries_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.pwm_summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }

    fn postfilter_summary(&mut self, channel: usize) -> PostFilterSummary {
        let rate = self.adc.get_postfilter(channel as u8).unwrap()
            .and_then(|filter| filter.output_rate());
        PostFilterSummary { channel, rate }
    }

    pub fn postfilter_summaries_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.postfilter_summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }

    fn steinhart_hart_summary(&mut self, channel: usize) -> SteinhartHartSummary {
        let params = self.channel_state(channel).sh.clone();
        SteinhartHartSummary { channel, params }
    }

    pub fn steinhart_hart_summaries_json(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let mut summaries = Vec::<_, U2>::new();
        for channel in 0..CHANNELS {
            let _ = summaries.push(self.steinhart_hart_summary(channel));
        }
        serde_json_core::to_vec(&summaries)
    }
}

type JsonBuffer = Vec<u8, U1024>;

#[derive(Serialize)]
pub struct Report {
    channel: usize,
    time: Time,
    interval: Time,
    adc: Option<ElectricPotential>,
    sens: Option<ElectricalResistance>,
    temperature: Option<f64>,
    pid_engaged: bool,
    i_set: ElectricCurrent,
    vref: ElectricPotential,
    dac_value: ElectricPotential,
    dac_feedback: ElectricPotential,
    i_tec: ElectricPotential,
    tec_i: ElectricCurrent,
    tec_u_meas: ElectricPotential,
    pid_output: Option<ElectricCurrent>,
}

pub struct CenterPointJson(CenterPoint);

// used in JSON encoding, not for config
impl Serialize for CenterPointJson {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.0 {
            CenterPoint::Vref =>
                serializer.serialize_str("vref"),
            CenterPoint::Override(vref) =>
                serializer.serialize_f32(vref),
        }
    }
}

#[derive(Serialize)]
pub struct PwmSummaryField<T: Serialize> {
    value: T,
    max: T,
}

impl<T: Serialize> From<(T, T)> for PwmSummaryField<T> {
    fn from((value, max): (T, T)) -> Self {
        PwmSummaryField { value, max }
    }
}

#[derive(Serialize)]
pub struct PwmSummary {
    channel: usize,
    center: CenterPointJson,
    i_set: PwmSummaryField<ElectricCurrent>,
    max_v: PwmSummaryField<ElectricPotential>,
    max_i_pos: PwmSummaryField<ElectricCurrent>,
    max_i_neg: PwmSummaryField<ElectricCurrent>,
}

#[derive(Serialize)]
pub struct PostFilterSummary {
    channel: usize,
    rate: Option<f32>,
}

#[derive(Serialize)]
pub struct SteinhartHartSummary {
    channel: usize,
    params: steinhart_hart::Parameters,
}
