use core::fmt;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Volts(pub f64);

impl fmt::Display for Volts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}V", self.0)
    }
}
