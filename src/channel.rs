use crate::{
    ad5680,
    channel_state::ChannelState,
    pins::{ChannelPins, ChannelPinSet},
};

/// Marker type for the first channel
pub struct Channel0;

/// Marker type for the second channel
pub struct Channel1;


pub struct Channel<C: ChannelPins> {
    pub state: ChannelState,
    /// for `i_set`
    pub dac: ad5680::Dac<C::DacSpi, C::DacSync>,
    /// 1 / Volts
    pub dac_factor: f64,
    pub shdn: C::Shdn,
    /// stm32f4 integrated adc
    pub adc: C::Adc,
    pub vref_pin: C::VRefPin,
    pub itec_pin: C::ItecPin,
    /// feedback from `dac` output
    pub dac_feedback_pin: C::DacFeedbackPin,
    pub tec_u_meas_pin: C::TecUMeasPin,
}

impl<C: ChannelPins> Channel<C> {
    pub fn new(mut pins: ChannelPinSet<C>) -> Self {
        let state = ChannelState::default();
        let mut dac = ad5680::Dac::new(pins.dac_spi, pins.dac_sync);
        let _ = dac.set(0);
        // power up TEC
        let _ = pins.shdn.set_high();
        // sensible dummy preset. calibrate_i_set() must be used.
        let dac_factor = ad5680::MAX_VALUE as f64 / 5.0;

        Channel {
            state,
            dac, dac_factor,
            shdn: pins.shdn,
            adc: pins.adc,
            vref_pin: pins.vref_pin,
            itec_pin: pins.itec_pin,
            dac_feedback_pin: pins.dac_feedback_pin,
            tec_u_meas_pin: pins.tec_u_meas_pin,
        }
    }
}
