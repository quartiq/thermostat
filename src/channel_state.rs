use smoltcp::time::Instant;
use crate::{ad5680, ad7172, pid, steinhart_hart as sh};


pub struct ChannelState {
    pub adc_data: Option<i32>,
    pub adc_time: Instant,
    pub dac_value: u32,
    pub pid_enabled: bool,
    pub pid: pid::Controller,
    pub sh: sh::Parameters,
}

impl Default for ChannelState {
    fn default() -> Self {
        ChannelState {
            adc_data: None,
            adc_time: Instant::from_secs(0),
            dac_value: 0,
            pid_enabled: false,
            pid: pid::Controller::new(pid::Parameters::default()),
            sh: sh::Parameters::default(),
        }
    }
}

impl ChannelState {
    /// Update PID state on ADC input, calculate new DAC output
    pub fn update_adc(&mut self, now: Instant, adc_data: i32) {
        self.adc_data = Some(adc_data);
        self.adc_time = now;

        // Update PID controller
        let input = (adc_data as f64) / (ad7172::MAX_VALUE as f64);
        let temperature = self.sh.get_temperature(input);
        let output = self.pid.update(temperature);
        self.dac_value = (output * (ad5680::MAX_VALUE as f64)) as u32;
    }
}

