use lexical_core::Float;

/// Steinhart-Hart equation parameters
#[derive(Clone, Debug)]
pub struct Parameters {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    /// Parallel resistance
    ///
    /// Not truly part of the equation but required to calculate
    /// resistance from voltage.
    pub parallel_r: f32,
}

impl Parameters {
    /// Perform the voltage to temperature conversion.
    ///
    /// Result unit: Kelvin
    ///
    /// TODO: verify
    pub fn get_temperature(&self, voltage: f32) -> f32 {
        let r = self.parallel_r * voltage;
        let ln_r = r.abs().ln();
        let inv_temp = self.a +
            self.b * ln_r +
            self.c * ln_r * ln_r * ln_r;
        1.0 / inv_temp
    }
}
