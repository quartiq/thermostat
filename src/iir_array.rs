

use uom::si::{
    thermodynamic_temperature::degree_celsius,
};

use crate::{
    iir_float,
    channel_state::ChannelState,
};

const ARRSIZE: usize = 8;




pub type Iirs = [iir_float::Iir; ARRSIZE];
pub type StateVec  = [State; ARRSIZE];


// Single internal state with information about where to update from.
#[derive(Debug, Copy, Clone)]
pub struct State {
    pub source: usize,  // 0~constval 1~temp0  2~temp1  3~iir0  4~iir1  etc.
    pub constval : f64,
    pub val: f64
}

impl State {
    pub fn new() -> Self{
        State{
            source: 0,
            constval: 0.0,
            val:0.0
        }
    }
}


// IIR matrix with variable target and input source for each iir. In/outputs can be freely reconfigured.
pub struct IirMatrix {
    pub iirarray: Iirs,
    pub inputs: StateVec,
    pub targets: StateVec
}

impl IirMatrix {

    pub fn new() -> IirMatrix {
        IirMatrix{
            iirarray: [iir_float::Iir::new(); ARRSIZE],
            inputs: [State::new(); ARRSIZE],
            targets: [State::new(); ARRSIZE]
        }
    }

    /// Time tick. Updates all IIRs and states in order.
    pub fn tick(&mut self, channel0: &mut ChannelState, channel1: &mut ChannelState ) {
        for i in 0..ARRSIZE{
            if self.inputs[i].source == 0 {
                self.inputs[i].val = self.inputs[i].constval;
            }
            else if self.inputs[i].source == 1 {
                self.inputs[i].val = channel0.get_temperature().unwrap()
                    .get::<degree_celsius>();
            }
        }

    }
}
