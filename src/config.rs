use serde::{Serialize, Deserialize};
use postcard::{from_bytes, to_slice};
use uom::si::{
    electric_potential::volt,
    electric_current::ampere,
};
use crate::{
    channels::{CHANNELS, Channels},
    command_parser::CenterPoint,
    pid,
    steinhart_hart,
};

/// Just for encoding/decoding, actual state resides in ChannelState
#[derive(Serialize, Deserialize)]
pub struct Config {
    channels: [ChannelConfig; CHANNELS],
}

impl Config {
    pub fn new(channels: &mut Channels) -> Self {
        Config {
            channels: [
                ChannelConfig::new(channels, 0),
                ChannelConfig::new(channels, 1),
            ],
        }
    }

    pub fn encode<'a>(&self, buffer: &'a mut [u8]) -> Result<&'a mut [u8], postcard::Error> {
        to_slice(self, buffer)
    }

    pub fn decode(buffer: &[u8]) -> Result<Self, postcard::Error> {
        from_bytes(buffer)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    center: CenterPoint,
    pid: pid::Parameters,
    pid_target: f32,
    sh: steinhart_hart::Parameters,
    pwm: PwmLimits,
}

impl ChannelConfig {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let pwm = PwmLimits::new(channels, channel);
        let state = channels.channel_state(channel);
        ChannelConfig {
            center: state.center.clone(),
            pid: state.pid.parameters.clone(),
            pid_target: state.pid.target as f32,
            sh: state.sh.clone(),
            pwm,
        }
    }

}

#[derive(Clone, Serialize, Deserialize)]
struct PwmLimits {
    max_v: f32,
    max_i_pos: f32,
    max_i_neg: f32,
}

impl PwmLimits {
    pub fn new(channels: &mut Channels, channel: usize) -> Self {
        let (max_v, _) = channels.get_max_v(channel);
        let (max_i_pos, _) = channels.get_max_i_pos(channel);
        let (max_i_neg, _) = channels.get_max_i_neg(channel);
        PwmLimits {
            max_v: max_v.get::<volt>() as f32,
            max_i_pos: max_i_pos.get::<ampere>() as f32,
            max_i_neg: max_i_neg.get::<ampere>() as f32,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fit_eeprom() {
        let channel_config = ChannelConfig {
            center: CenterPoint::Override(1.5),
            pid: pid::Parameters::default(),
            pid_target: 93.7,
            sh: steinhart_hart::Parameters::default(),
            pwm: PwmLimits {
                max_v: 1.65,
                max_i_pos: 2.1,
                max_i_neg: 2.25,
            },
        };
        let config = Config {
            channels: [
                channel_config.clone(),
                channel_config.clone(),
            ],
        };

        const EEPROM_SIZE: usize = 0x80;
        let mut buffer = [0; EEPROM_SIZE];
        let buffer = config.encode(&mut buffer).unwrap();
        assert!(buffer.len() <= EEPROM_SIZE);
    }
}
