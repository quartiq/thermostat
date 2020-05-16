use core::{
    fmt,
    ops::Div,
};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Volts(pub f64);

impl fmt::Display for Volts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}V", self.0)
    }
}

impl Div<Ohms> for Volts {
    type Output = Amps;
    fn div(self, rhs: Ohms) -> Amps {
        Amps(self.0 / rhs.0)
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Amps(pub f64);

impl fmt::Display for Amps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}A", self.0)
    }
}
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Ohms(pub f64);

impl fmt::Display for Ohms {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}Ω", self.0)
    }
}
