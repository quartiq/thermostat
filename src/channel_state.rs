use smoltcp::time::Instant;
use uom::si::{
    f64::ElectricPotential,
    electric_potential::volt,
};
use crate::{
    ad7172,
    pid,
    steinhart_hart as sh,
};


pub struct ChannelState {
    pub adc_data: Option<u32>,
    pub adc_calibration: ad7172::ChannelCalibration,
    pub adc_time: Instant,
    pub dac_value: ElectricPotential,
    pub pid_engaged: bool,
    pub pid: pid::Controller,
    pub sh: sh::Parameters,
}

impl ChannelState {
    pub fn new(adc_calibration: ad7172::ChannelCalibration) -> Self {
        ChannelState {
            adc_data: None,
            adc_calibration,
            adc_time: Instant::from_secs(0),
            dac_value: ElectricPotential::new::<volt>(0.0),
            pid_engaged: false,
            pid: pid::Controller::new(pid::Parameters::default()),
            sh: sh::Parameters::default(),
        }
    }

    pub fn update(&mut self, now: Instant, adc_data: u32) {
        self.adc_data = Some(adc_data);
        self.adc_time = now;
    }

    /// Update PID state on ADC input, calculate new DAC output
    pub fn update_pid(&mut self) -> f64 {
        // Update PID controller
        self.pid.update(self.get_temperature().unwrap())
    }

    pub fn get_adc(&self) -> Option<ElectricPotential> {
        let volts = self.adc_calibration.convert_data(self.adc_data?);
        Some(ElectricPotential::new::<volt>(volts))
    }

    pub fn get_temperature(&self) -> Option<f64> {
        let r = self.get_adc()?.get::<volt>();
        let temperature = self.sh.get_temperature(r);
        Some(temperature)
    }
}
