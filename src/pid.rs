use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Parameters {
    pub kp: f64,
    pub ki: f64,
    pub kd: f64,
    pub output_min: f64,
    pub output_max: f64,
    pub integral_min: f64,
    pub integral_max: f64
}

impl Default for Parameters {
    fn default() -> Self {
        Parameters {
            kp: 1.5,
            ki: 0.1,
            kd: 150.0,
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

    pub fn update(&mut self, input: f64) -> f64 {
        // error
        let error = self.target - input;

        // partial
        let p = self.parameters.kp * error;

        //integral
        self.integral += error;
        if self.integral < self.parameters.integral_min {
            self.integral = self.parameters.integral_min;
        }
        if self.integral > self.parameters.integral_max {
            self.integral = self.parameters.integral_max;
        }
        let i = self.parameters.ki * self.integral;

        // derivative
        let d = match self.last_input {
            None => 0.0,
            Some(last_input) => self.parameters.kd * (last_input - input)
        };
        self.last_input = Some(input);

        // output
        let mut output = p + i + d;
        if output < self.parameters.output_min {
            output = self.parameters.output_min;
        }
        if output > self.parameters.output_max {
            output = self.parameters.output_max;
        }
        self.last_output = Some(output);
        output
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.last_input = None;
    }
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
        const TARGET: f64 = 1234.56;
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
            let output = pid.update(values[next_t]);
            // Overwrite oldest with previous temperature + output
            values[next_t] = values[t] + output;
            t = next_t;
            total_t += 1;
        }
    }
}
