use postcard::{from_bytes, to_slice};
use serde::{Serialize, Deserialize};
use stm32f4xx_hal::i2c;
use uom::si::{
    electric_potential::volt,
    electric_current::ampere,
    electrical_resistance::ohm,
    f64::{ElectricCurrent, ElectricPotential, ElectricalResistance, ThermodynamicTemperature},
    thermodynamic_temperature::degree_celsius,
};
use crate::{
    channels::{CHANNELS, Channels},
    command_parser::CenterPoint,
    EEPROM_SIZE, EEPROM_PAGE_SIZE,
    pid,
    pins,
    steinhart_hart,
};

#[derive(Debug)]
pub enum Error {
    Eeprom(eeprom24x::Error<i2c::Error>),
    Encode(postcard::Error),
}

impl From<eeprom24x::Error<i2c::Error>> for Error {
    fn from(e: eeprom24x::Error<i2c::Error>) -> Self {
        Error::Eeprom(e)
    }
}

impl From<postcard::Error> for Error {
    fn from(e: postcard::Error) -> Self {
        Error::Encode(e)
    }
}

/// Just for encoding/decoding, actual state resides in ChannelState
#[derive(Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChannelConfig {
    center: CenterPoint,
    pid: pid::Parameters,
    pid_target: f32,
    sh: SteinhartHartConfig,
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
            sh: (&state.sh).into(),
            pwm,
        }
    }

    pub fn apply(&self, channels: &mut Channels, channel: usize) {
        let state = channels.channel_state(channel);
        state.center = self.center.clone();
        state.pid.parameters = self.pid.clone();
        state.pid.target = self.pid_target.into();
        state.sh = (&self.sh).into();
        self.pwm.apply(channels, channel);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SteinhartHartConfig {
    t0: f32,
    r0: f32,
    b: f32,
}

impl From<&steinhart_hart::Parameters> for SteinhartHartConfig {
    fn from(sh: &steinhart_hart::Parameters) -> Self {
        SteinhartHartConfig {
            t0: sh.t0.get::<degree_celsius>() as f32,
            r0: sh.r0.get::<ohm>() as f32,
            b: sh.b as f32,
        }
    }
}

impl Into<steinhart_hart::Parameters> for &SteinhartHartConfig {
    fn into(self) -> steinhart_hart::Parameters {
        steinhart_hart::Parameters {
            t0: ThermodynamicTemperature::new::<degree_celsius>(self.t0.into()),
            r0: ElectricalResistance::new::<ohm>(self.r0.into()),
            b: self.b.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

    pub fn apply(&self, channels: &mut Channels, channel: usize) {
        channels.set_max_v(channel, ElectricPotential::new::<volt>(self.max_v.into()));
        channels.set_max_i_pos(channel, ElectricCurrent::new::<ampere>(self.max_i_pos.into()));
        channels.set_max_i_neg(channel, ElectricCurrent::new::<ampere>(self.max_i_neg.into()));
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
            sh: (&steinhart_hart::Parameters::default()).into(),
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

        let mut buffer = [0; EEPROM_SIZE];
        let buffer = config.encode(&mut buffer).unwrap();
        assert!(buffer.len() <= EEPROM_SIZE);
    }

    #[test]
    fn test_encode_decode() {
        let channel_config = ChannelConfig {
            center: CenterPoint::Override(1.5),
            pid: pid::Parameters::default(),
            pid_target: 93.7,
            sh: (&steinhart_hart::Parameters::default()).into(),
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

        let mut buffer = [0; EEPROM_SIZE];
        config.encode(&mut buffer).unwrap();
        let decoded = Config::decode(&buffer).unwrap();
        assert_eq!(decoded, config);
    }
}
