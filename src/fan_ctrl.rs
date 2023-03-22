use num_traits::Float;
use serde::Serialize;
use stm32f4xx_hal::{
    pwm::{self, PwmChannels},
    pac::TIM8,
};

use crate::{
    hw_rev::HWSettings,
    command_handler::JsonBuffer,
};

pub type FanPin = PwmChannels<TIM8, pwm::C4>;

// as stated in the schematics
const MAX_TEC_I: f32 = 3.0;

const MAX_USER_FAN_PWM: f32 = 100.0;
const MIN_USER_FAN_PWM: f32 = 1.0;


pub struct FanCtrl {
    fan: Option<FanPin>,
    fan_auto: bool,
    pwm_enabled: bool,
    k_a: f32,
    k_b: f32,
    k_c: f32,
    abs_max_tec_i: f32,
    hw_settings: HWSettings,
}

impl FanCtrl {
    pub fn new(fan: Option<FanPin>, hw_settings: HWSettings) -> Self {
        let mut fan_ctrl = FanCtrl {
            fan,
            // do not enable auto mode by default,
            // but allow to turn it at the user's own risk
            fan_auto: hw_settings.fan_pwm_recommended,
            pwm_enabled: false,
            k_a: hw_settings.fan_k_a,
            k_b: hw_settings.fan_k_b,
            k_c: hw_settings.fan_k_c,
            abs_max_tec_i: 0f32,
            hw_settings,
        };
        if fan_ctrl.fan_auto {
            fan_ctrl.enable_pwm();
        }
        fan_ctrl
    }

    pub fn cycle(&mut self, abs_max_tec_i: f32) {
        self.abs_max_tec_i = abs_max_tec_i;
        if self.fan_auto && self.hw_settings.fan_available {
            let scaled_current = self.abs_max_tec_i / MAX_TEC_I;
            // do not limit upper bound, as it will be limited in the set_pwm()
            let pwm = (MAX_USER_FAN_PWM * (scaled_current * (scaled_current * self.k_a + self.k_b) + self.k_c)) as u32;
            self.set_pwm(pwm);
        }
    }

    pub fn summary(&mut self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        if self.hw_settings.fan_available {
            let summary = FanSummary {
                fan_pwm: self.get_pwm(),
                abs_max_tec_i: self.abs_max_tec_i,
                auto_mode: self.fan_auto,
                k_a: self.k_a,
                k_b: self.k_b,
                k_c: self.k_c,
            };
            serde_json_core::to_vec(&summary)
        } else {
            let summary: Option<()> = None;
            serde_json_core::to_vec(&summary)
        }
    }

    pub fn set_auto_mode(&mut self, fan_auto: bool) {
        self.fan_auto = fan_auto;
    }

    pub fn set_curve(&mut self, k_a: f32, k_b: f32, k_c: f32) {
        self.k_a = k_a;
        self.k_b = k_b;
        self.k_c = k_c;
    }

    pub fn restore_defaults(&mut self) {
        self.set_curve(self.hw_settings.fan_k_a,
                       self.hw_settings.fan_k_b,
                       self.hw_settings.fan_k_c);
    }

    pub fn set_pwm(&mut self, fan_pwm: u32) -> f32 {
        if self.fan.is_none() || (!self.pwm_enabled && !self.enable_pwm())  {
            return 0f32;
        }
        let fan = self.fan.as_mut().unwrap();
        let fan_pwm = fan_pwm.min(MAX_USER_FAN_PWM as u32).max(MIN_USER_FAN_PWM as u32);
        let duty = scale_number(fan_pwm as f32, self.hw_settings.min_fan_pwm, self.hw_settings.max_fan_pwm, MIN_USER_FAN_PWM, MAX_USER_FAN_PWM);
        let max = fan.get_max_duty();
        let value = ((duty * (max as f32)) as u16).min(max);
        fan.set_duty(value);
        value as f32 / (max as f32)
    }

    pub fn fan_pwm_recommended(&self) -> bool {
        self.hw_settings.fan_pwm_recommended
    }

    pub fn fan_available(&self) -> bool {
        self.hw_settings.fan_available
    }

    fn get_pwm(&self) -> u32 {
        if let Some(fan) = &self.fan {
            let duty = fan.get_duty();
            let max = fan.get_max_duty();
            scale_number(duty as f32 / (max as f32), MIN_USER_FAN_PWM, MAX_USER_FAN_PWM, self.hw_settings.min_fan_pwm, self.hw_settings.max_fan_pwm).round() as u32
        } else { 0 }
    }

    fn enable_pwm(&mut self) -> bool {
        if self.fan.is_some() && self.hw_settings.fan_available {
            let fan = self.fan.as_mut().unwrap();
            fan.set_duty(0);
            fan.enable();
            self.pwm_enabled = true;
            true
        } else {
            false
        }
    }
}


fn scale_number(unscaled: f32, to_min: f32, to_max: f32, from_min: f32, from_max: f32) -> f32 {
    (to_max - to_min) * (unscaled - from_min) / (from_max - from_min) + to_min
}

#[derive(Serialize)]
pub struct FanSummary {
    fan_pwm: u32,
    abs_max_tec_i: f32,
    auto_mode: bool,
    k_a: f32,
    k_b: f32,
    k_c: f32,
}
