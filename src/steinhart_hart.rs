use num_traits::float::Float;

/// Steinhart-Hart equation parameters
#[derive(Clone, Debug)]
pub struct Parameters {
    pub t0: f64,
    pub b: f64,
    pub r0: f64,
}

impl Parameters {
    /// Perform the voltage to temperature conversion.
    ///
    /// Result unit: Kelvin
    ///
    /// TODO: verify
    pub fn get_temperature(&self, r: f64) -> f64 {
        let inv_temp = 1.0 / self.t0 + (r / self.r0).ln() / self.b;
        1.0 / inv_temp
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            t0: 0.001_4,
            b: 0.000_000_099,
            r0: 5_110.0,
        }
    }
}
