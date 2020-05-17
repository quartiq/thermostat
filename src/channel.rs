use crate::{
    ad5680,
    channel_state::ChannelState,
    pins::{ChannelPins, ChannelPinSet},
    units::Volts,
};

/// Marker type for the first channel
pub struct Channel0;

/// Marker type for the second channel
pub struct Channel1;


pub struct Channel<C: ChannelPins> {
    pub state: ChannelState,
    /// for `i_set`
    pub dac: ad5680::Dac<C::DacSpi, C::DacSync>,
    pub shdn: C::Shdn,
    /// stm32f4 integrated adc
    pub adc: C::Adc,
    pub itec_pin: C::ItecPin,
    /// feedback from `dac` output
    pub dac_feedback_pin: C::DacFeedbackPin,
}

impl<C: ChannelPins> Channel<C> {
    pub fn new(pins: ChannelPinSet<C>) -> Self {
        let state = ChannelState::default();
        let mut dac = ad5680::Dac::new(pins.dac_spi, pins.dac_sync);
        let _ = dac.set(Volts(0.0));

        Channel {
            state,
            dac,
            shdn: pins.shdn,
            adc: pins.adc,
            itec_pin: pins.itec_pin,
            dac_feedback_pin: pins.dac_feedback_pin,
        }
    }
}
