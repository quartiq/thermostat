use lexical_core::Float;

/// Steinhart-Hart equation parameters
#[derive(Clone, Debug)]
pub struct Parameters {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    /// Parallel resistance
    ///
    /// Not truly part of the equation but required to calculate
    /// resistance from voltage.
    pub parallel_r: f64,
}

impl Parameters {
    /// Perform the voltage to temperature conversion.
    ///
    /// Result unit: Kelvin
    ///
    /// TODO: verify
    pub fn get_temperature(&self, voltage: f64) -> f64 {
        let r = self.parallel_r * voltage;
        let ln_r = r.abs().ln();
        let inv_temp = self.a +
            self.b * ln_r +
            self.c * ln_r * ln_r * ln_r;
        1.0 / inv_temp
    }
}
