use smoltcp::time::Instant;
use crate::{
    ad7172,
    pid,
    steinhart_hart as sh,
    units::Volts,
};


pub struct ChannelState {
    pub adc_data: Option<u32>,
    pub adc_time: Instant,
    pub dac_value: Volts,
    pub pid_engaged: bool,
    pub pid: pid::Controller,
    pub sh: sh::Parameters,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState {
            adc_data: None,
            adc_time: Instant::from_secs(0),
            dac_value: Volts(0.0),
            pid_engaged: false,
            pid: pid::Controller::new(pid::Parameters::default()),
            sh: sh::Parameters::default(),
        }
    }
}

impl ChannelState {
    /// Update PID state on ADC input, calculate new DAC output
    pub fn update_pid(&mut self, now: Instant, adc_data: u32) -> f64 {
        self.adc_data = Some(adc_data);
        self.adc_time = now;

        // Update PID controller
        let input = (adc_data as f64) / (ad7172::MAX_VALUE as f64);
        let temperature = self.sh.get_temperature(input);
        self.pid.update(temperature)
    }
}
