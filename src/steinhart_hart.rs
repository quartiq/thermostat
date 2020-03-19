use num_traits::float::Float;

/// Steinhart-Hart equation parameters
#[derive(Clone, Debug)]
pub struct Parameters {
    pub t0: f64,
    pub r: f64,
    pub r0: f64,

    r_fact: f64,
}

impl Parameters {
    /// Update the cached r_fact
    pub fn update(&mut self) {
        self.r_fact = (self.r / self.r0).ln();
    }

    /// Perform the voltage to temperature conversion.
    ///
    /// Result unit: Kelvin
    ///
    /// TODO: verify
    pub fn get_temperature(&self, b: f64) -> f64 {
        let inv_temp = 1.0 / self.t0 + self.r_fact / b;
        1.0 / inv_temp
    }
}

impl Default for Parameters {
    fn default() -> Self {
        let mut p = Parameters {
            t0: 0.001_4,
            r: 0.000_000_099,
            r0: 5_110.0,
            r_fact: 0.0,
        };
        p.update();
        p
    }
}
