use crate::pins::{ChannelPins, ChannelPinSet};
use crate::channel_state::ChannelState;
use crate::ad5680;

/// Marker type for the first channel
pub struct Channel0;

/// Marker type for the second channel
pub struct Channel1;


pub struct Channel<C: ChannelPins> {
    pub state: ChannelState,
    pub dac: ad5680::Dac<C::DacSpi, C::DacSync>,
    pub shdn: C::Shdn,
    pub ref_adc: C::RefAdc,
    pub ref_pin: C::RefPin,
}

impl<C: ChannelPins> Channel<C> {
    pub fn new(pins: ChannelPinSet<C>) -> Self {
        let state = ChannelState::default();
        let mut dac = ad5680::Dac::new(pins.dac_spi, pins.dac_sync);
        let _ = dac.set(0);

        Channel {
            state,
            dac,
            shdn: pins.shdn,
            ref_adc: pins.ref_adc,
            ref_pin: pins.ref_pin,
        }
    }
}
