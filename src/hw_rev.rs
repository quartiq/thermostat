use serde::Serialize;

use crate::{
    pins::HWRevPins,
    command_handler::JsonBuffer,
};

#[derive(Serialize, Copy, Clone)]
pub struct HWRev {
    pub major: u8,
    pub minor: u8,
}

#[derive(Serialize, Clone)]
pub struct HWSettings {
    pub fan_k_a: f32,
    pub fan_k_b: f32,
    pub fan_k_c: f32,
    pub min_fan_pwm: f32,
    pub max_fan_pwm: f32,
    pub fan_pwm_freq_hz: u32,
    pub fan_available: bool,
    pub fan_pwm_recommended: bool,
}

#[derive(Serialize, Clone)]
struct HWSummary<'a> {
    rev: &'a HWRev,
    settings: &'a HWSettings,
}

impl HWRev {
    pub fn detect_hw_rev(hwrev_pins: &HWRevPins) -> Self {
        let (h0, h1, h2, h3) = (hwrev_pins.hwrev0.is_high(), hwrev_pins.hwrev1.is_high(),
                                hwrev_pins.hwrev2.is_high(), hwrev_pins.hwrev3.is_high());
        match (h0, h1, h2, h3) {
            (true, true, true, false) => HWRev { major: 1, minor: 0 },
            (true, false, false, false) => HWRev { major: 2, minor: 0 },
            (false, true, false, false) => HWRev { major: 2, minor: 2 },
            (_, _, _, _) => HWRev { major: 0, minor: 0 }
        }
    }

    pub fn settings(&self) -> HWSettings {
        match (self.major, self.minor) {
            (2, 2) => HWSettings {
                fan_k_a: 1.0,
                fan_k_b: 0.0,
                fan_k_c: 0.0,
                // below this value motor's autostart feature may fail,
                // according to internal experiments
                min_fan_pwm: 0.04,
                max_fan_pwm: 1.0,
                // According to `SUNON DC Brushless Fan & Blower(255-E)` catalogue p.36-37
                // model MF35101V1-1000U-G99 doesn't have a PWM wire, but we'll follow their others models'
                // recommended frequency, as it is said by the Thermostat's schematics that we can
                // use PWM, but not stated at which frequency
                fan_pwm_freq_hz: 25_000,
                fan_available: true,
                // see https://github.com/sinara-hw/Thermostat/issues/115 and
                // https://git.m-labs.hk/M-Labs/thermostat/issues/69#issuecomment-6464 for explanation
                fan_pwm_recommended: false,
            },
            (_, _) => HWSettings {
                fan_k_a: 0.0,
                fan_k_b: 0.0,
                fan_k_c: 0.0,
                min_fan_pwm: 0.0,
                max_fan_pwm: 0.0,
                fan_pwm_freq_hz: 0,
                fan_available: false,
                fan_pwm_recommended: false,
            }
        }
    }

    pub fn summary(&self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        let settings = self.settings();
        let summary = HWSummary { rev: self, settings: &settings };
        serde_json_core::to_vec(&summary)
    }
}