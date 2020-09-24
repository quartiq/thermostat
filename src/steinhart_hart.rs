use num_traits::float::Float;
use uom::si::{
    f64::{
        ElectricalResistance,
        ThermodynamicTemperature,
    },
    electrical_resistance::ohm,
    ratio::ratio,
    thermodynamic_temperature::{degree_celsius, kelvin},
};

/// Steinhart-Hart equation parameters
#[derive(Clone, Debug)]
pub struct Parameters {
    /// Base temperature
    pub t0: ThermodynamicTemperature,
    /// Base resistance
    pub r0: ElectricalResistance,
    /// Beta
    pub b: f64,
}

impl Parameters {
    /// Perform the voltage to temperature conversion.
    ///
    /// Result unit: Kelvin
    ///
    /// TODO: verify
    pub fn get_temperature(&self, r: ElectricalResistance) -> ThermodynamicTemperature {
        let inv_temp = 1.0 / self.t0.get::<kelvin>() + (r / self.r0).get::<ratio>().ln() / self.b;
        ThermodynamicTemperature::new::<kelvin>(1.0 / inv_temp)
    }
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            t0: ThermodynamicTemperature::new::<degree_celsius>(25.0),
            r0: ElectricalResistance::new::<ohm>(10_000.0),
            b: 3800.0,
        }
    }
}
