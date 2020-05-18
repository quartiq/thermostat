use core::{
    fmt,
    ops::{Add, Div, Neg, Sub},
};

macro_rules! impl_add_sub {
    ($Type: ident) => {
        impl Add<$Type> for $Type {
            type Output = $Type;
            fn add(self, rhs: $Type) -> $Type {
                $Type(self.0 + rhs.0)
            }
        }

        impl Sub<$Type> for $Type {
            type Output = $Type;
            fn sub(self, rhs: $Type) -> $Type {
                $Type(self.0 - rhs.0)
            }
        }

        impl Neg for $Type {
            type Output = $Type;
            fn neg(self) -> $Type {
                $Type(-self.0)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Volts(pub f64);
impl_add_sub!(Volts);

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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Amps(pub f64);
impl_add_sub!(Amps);

impl fmt::Display for Amps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}A", self.0)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Ohms(pub f64);
impl_add_sub!(Ohms);

impl fmt::Display for Ohms {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}Ω", self.0)
    }
}
