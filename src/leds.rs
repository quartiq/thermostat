use stm32_eth::hal::{
    gpio::{
        gpiod::{PD9, PD10, PD11},
        Output, PushPull,
    },
    hal::digital::v2::OutputPin,
};

pub struct Leds {
    /// Red LED L1
    pub r1: Led<PD9<Output<PushPull>>>,
    /// Green LED L3
    pub g3: Led<PD10<Output<PushPull>>>,
    /// Green LED L4
    pub g4: Led<PD11<Output<PushPull>>>,
}

impl Leds {
    pub fn new<M1, M2, M3>(r1: PD9<M1>, g3: PD10<M2>, g4: PD11<M3>) -> Self {
        Leds {
            r1: Led::new(r1.into_push_pull_output()),
            g3: Led::new(g3.into_push_pull_output()),
            g4: Led::new(g4.into_push_pull_output()),
        }
    }
}

pub struct Led<P> {
    pin: P,
}

impl<P: OutputPin> Led<P> {
    pub fn new(pin: P) -> Self {
        Led { pin }
    }

    pub fn on(&mut self) {
        let _ = self.pin.set_high();
    }

    pub fn off(&mut self) {
        let _ = self.pin.set_low();
    }
}
