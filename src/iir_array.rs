

use crate::{
    iir_float
};



// IIR array. In/outputs can be freely reconfigured.

pub type Iirs = [iir_float::Iir; 8];




pub struct IirMatrix {
    pub array: Iirs,
}

impl IirMatrix {

    pub fn new() -> IirMatrix {
        IirMatrix{
            array: [iir_float::Iir::new(); 8]
        }
    }

    /// Time tick. Updates all IIRs and states in order.
    pub fn tick(&mut self) {
        self.array[0].tick(0.0);
    }
}
