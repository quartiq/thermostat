use serde::{Serialize, Deserialize};
use uom::si::{
    f64::Time,
    time::second,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameters {
    /// Gain coefficient for proportional term
    pub kp: f32,
    /// Gain coefficient for integral term
    pub ki: f32,
    /// Gain coefficient for derivative term
    pub kd: f32,
    /// Output limit minimum
    pub output_min: f32,
    /// Output limit maximum
    pub output_max: f32,
    /// Integral clipping minimum
    pub integral_min: f32,
    /// Integral clipping maximum
    pub integral_max: f32
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            kp: 1.5,
            ki: 1.0,
            kd: 1.5,
            output_min: 0.0,
            output_max: 2.0,
            integral_min: -10.0,
            integral_max: 10.0,
        }
    }
}

#[derive(Clone)]
pub struct Controller {
    pub parameters: Parameters,
    pub target: f64,
    integral: f64,
    last_input: Option<f64>,
    pub last_output: Option<f64>,
}

impl Controller {
    pub const fn new(parameters: Parameters) -> Controller {
        Controller {
            parameters: parameters,
            target: 0.0,
            last_input: None,
            integral: 0.0,
            last_output: None,
        }
    }

    pub fn update(&mut self, input: f64, time_delta: Time) -> f64 {
        let time_delta = time_delta.get::<second>();

        // error
        let error = self.target - input;

        // proportional
        let p = f64::from(self.parameters.kp) * error;

        // integral
        if let Some(last_output_val) = self.last_output {
            // anti integral windup
            if last_output_val < self.parameters.output_max.into() && last_output_val > self.parameters.output_min.into() {
                self.integral += error * time_delta;    
            }
        }
        if self.integral < self.parameters.integral_min.into() {
            self.integral = self.parameters.integral_min.into();
        }
        if self.integral > self.parameters.integral_max.into() {
            self.integral = self.parameters.integral_max.into();
        }
        let i = self.integral * f64::from(self.parameters.ki);

        // derivative
        let d = match self.last_input {
            None =>
                0.0,
            Some(last_input) =>
                f64::from(self.parameters.kd) * (input - last_input) / time_delta,
        };
        self.last_input = Some(input);

        // output
        let mut output = p + i + d;
        if output < self.parameters.output_min.into() {
            output = self.parameters.output_min.into();
        }
        if output > self.parameters.output_max.into() {
            output = self.parameters.output_max.into();
        }
        self.last_output = Some(output);
        output
    }

    pub fn summary(&self, channel: usize) -> Summary {
        Summary {
            channel,
            parameters: self.parameters.clone(),
            target: self.target,
            integral: self.integral,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Summary {
    channel: usize,
    parameters: Parameters,
    target: f64,
    integral: f64,
}

#[cfg(test)]
mod test {
    use super::*;

    const PARAMETERS: Parameters = Parameters {
        kp: 0.055,
        ki: 0.005,
        kd: 0.04,
        output_min: -10.0,
        output_max: 10.0,
        integral_min: -100.0,
        integral_max: 100.0,
    };

    #[test]
    fn test_controller() {
        const DEFAULT: f64 = 0.0;
        const TARGET: f64 = -1234.56;
        const ERROR: f64 = 0.01;
        const DELAY: usize = 10;

        let mut pid = Controller::new(PARAMETERS.clone());
        pid.target = TARGET;

        let mut values = [DEFAULT; DELAY];
        let mut t = 0;
        let mut total_t = 0;
        let target = (TARGET - ERROR)..=(TARGET + ERROR);
        while !values.iter().all(|value| target.contains(value)) {
            let next_t = (t + 1) % DELAY;
            // Feed the oldest temperature
            let output = pid.update(values[next_t], Time::new::<second>(1.0));
            // Overwrite oldest with previous temperature - output
            values[next_t] = values[t] - output;
            t = next_t;
            total_t += 1;
        }
    }
}
