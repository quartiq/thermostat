use serde::{Serialize, Deserialize};
use uom::si::{
    electric_potential::volt,
    electric_current::ampere,
    f64::{ElectricCurrent, ElectricPotential},
};
use crate::{
    ad7172::PostFilter,
    channels::Channels,
    command_parser::CenterPoint,
    pid,
    steinhart_hart,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelConfig {
    center: CenterPoint,
    pid: pid::Parameters,
    pid_target: f32,
    pid_engaged: bool,
    sh: steinhart_hart::Parameters,
    pwm: PwmLimits,
    /// uses variant `PostFilter::Invalid` instead of `None` to save space
    adc_postfilter: PostFilter,
}

impl ChannelConfig {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let pwm = PwmLimits::new(channels, channel);

        let adc_postfilter = channels.adc.get_postfilter(channel as u8)
            .unwrap()
            .unwrap_or(PostFilter::Invalid);

        let state = channels.channel_state(channel);
        ChannelConfig {
            center: state.center.clone(),
            pid: state.pid.parameters.clone(),
            pid_target: state.pid.target as f32,
            pid_engaged: state.pid_engaged,
            sh: state.sh.clone(),
            pwm,
            adc_postfilter,
        }
    }

    pub fn apply(&self, channels: &mut Channels, channel: usize) {
        let state = channels.channel_state(channel);
        state.center = self.center.clone();
        state.pid.parameters = self.pid.clone();
        state.pid.target = self.pid_target.into();
        state.pid_engaged = self.pid_engaged;
        state.sh = self.sh.clone();

        self.pwm.apply(channels, channel);

        let adc_postfilter = match self.adc_postfilter {
            PostFilter::Invalid => None,
            adc_postfilter => Some(adc_postfilter),
        };
        let _ = channels.adc.set_postfilter(channel as u8, adc_postfilter);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct PwmLimits {
    max_v: f64,
    max_i_pos: f64,
    max_i_neg: f64,
}

impl PwmLimits {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let (max_v, _) = channels.get_max_v(channel);
        let (max_i_pos, _) = channels.get_max_i_pos(channel);
        let (max_i_neg, _) = channels.get_max_i_neg(channel);
        PwmLimits {
            max_v: max_v.get::<volt>(),
            max_i_pos: max_i_pos.get::<ampere>(),
            max_i_neg: max_i_neg.get::<ampere>(),
        }
    }

    pub fn apply(&self, channels: &mut Channels, channel: usize) {
        channels.set_max_v(channel, ElectricPotential::new::<volt>(self.max_v));
        channels.set_max_i_pos(channel, ElectricCurrent::new::<ampere>(self.max_i_pos));
        channels.set_max_i_neg(channel, ElectricCurrent::new::<ampere>(self.max_i_neg));
    }
}
