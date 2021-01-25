use stm32f4xx_hal::hal::digital::v2::OutputPin;
use uom::si::{
    f64::ElectricPotential,
    electric_potential::volt,
};
use crate::{
    ad5680,
    ad7172,
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
    /// Measured vref of MAX driver chip
    pub vref_meas: ElectricPotential,
    pub shdn: C::Shdn,
    pub vref_pin: C::VRefPin,
    pub itec_pin: C::ItecPin,
    /// feedback from `dac` output
    pub dac_feedback_pin: C::DacFeedbackPin,
    pub tec_u_meas_pin: C::TecUMeasPin,
}

impl<C: ChannelPins> Channel<C> {
    pub fn new(pins: ChannelPinSet<C>, adc_calibration: ad7172::ChannelCalibration) -> Self {
        let state = ChannelState::new(adc_calibration);
        let mut dac = ad5680::Dac::new(pins.dac_spi, pins.dac_sync);
        let _ = dac.set(0);
        // sensible dummy preset taken from datasheet. calibrate_dac_value() should be used to override this value.
        let vref_meas = ElectricPotential::new::<volt>(1.5);

        Channel {
            state,
            dac, vref_meas,
            shdn: pins.shdn,
            vref_pin: pins.vref_pin,
            itec_pin: pins.itec_pin,
            dac_feedback_pin: pins.dac_feedback_pin,
            tec_u_meas_pin: pins.tec_u_meas_pin,
        }
    }

    // power up TEC
    pub fn power_up(&mut self) {
        let _ = self.shdn.set_high();
    }

    // power down TEC
    pub fn power_down(&mut self) {
        let _ = self.shdn.set_low();
    }
}
