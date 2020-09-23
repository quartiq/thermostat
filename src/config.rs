use serde::{Serialize, Deserialize};
use serde_cbor::Serializer;
use serde_cbor::ser::SliceWrite;
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

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, serde_cbor::Error> {
        let writer = SliceWrite::new(buffer);
        let mut ser = Serializer::new(writer);
        self.serialize(&mut ser)?;
        let writer = ser.into_inner();
        let size = writer.bytes_written();
        Ok(size)
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
