use smoltcp::time::Instant;
use uom::si::{
    f64::{
        ElectricPotential,
        ElectricalResistance,
        ThermodynamicTemperature,
    },
    electric_potential::volt,
    electrical_resistance::ohm,
    temperature_interval::kelvin,
};
use crate::{
    ad7172,
    pid,
    steinhart_hart as sh,
};

const R_INNER: f64 = 2.0 * 5100.0;
const VREF_SENS: f64 = 3.3 / 2.0;

pub struct ChannelState {
    pub adc_data: Option<u32>,
    pub adc_calibration: ad7172::ChannelCalibration,
    pub adc_time: Instant,
    /// VREF for the TEC (1.5V)
    pub vref: ElectricPotential,
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
            // can be initialized later with Channels.read_vref()
            vref: ElectricPotential::new::<volt>(3.3 / 2.0),
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
    pub fn update_pid(&mut self) {
        // Update PID controller
        // self.pid.update(self.get_temperature().unwrap().get::<kelvin>())
        // TODO: add output field
    }

    pub fn get_adc(&self) -> Option<ElectricPotential> {
        Some(self.adc_calibration.convert_data(self.adc_data?))
    }

    /// Get `SENS[01]` input resistance
    pub fn get_sens(&self) -> Option<ElectricalResistance> {
        let r_inner = ElectricalResistance::new::<ohm>(R_INNER);
        let vref = ElectricPotential::new::<volt>(VREF_SENS);
        let adc_input = self.get_adc()?;
        let r = r_inner * adc_input / (vref - adc_input);
        Some(r)
    }

    pub fn get_temperature(&self) -> Option<ThermodynamicTemperature> {
        let r = self.get_sens()?;
        let temperature = self.sh.get_temperature(r);
        Some(temperature)
    }
}
