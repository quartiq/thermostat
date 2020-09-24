use serde::{Serialize, Deserialize};
use postcard::{from_bytes, to_slice};
use crate::{
    channel_state::ChannelState,
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
                ChannelConfig::new(channels.channel_state(0usize)),
                ChannelConfig::new(channels.channel_state(1usize)),
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

#[derive(Serialize, Deserialize)]
pub struct ChannelConfig {
    center: CenterPoint,
    pid: pid::Parameters,
    pid_target: f64,
    sh: steinhart_hart::Parameters,
    // TODO: pwm limits
}

impl ChannelConfig {
    pub fn new(state: &ChannelState) -> Self {
        ChannelConfig {
            center: state.center.clone(),
            pid: state.pid.parameters.clone(),
            pid_target: state.pid.target,
            sh: state.sh.clone(),
        }
    }

}
