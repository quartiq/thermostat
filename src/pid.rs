use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameters {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub output_min: f32,
    pub output_max: f32,
    pub integral_min: f32,
    pub integral_max: f32
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
        let error = input - self.target;

        // partial
        let p = f64::from(self.parameters.kp) * error;

        // integral
        self.integral += f64::from(self.parameters.ki) * error;
        if self.integral < self.parameters.integral_min.into() {
            self.integral = self.parameters.integral_min.into();
        }
        if self.integral > self.parameters.integral_max.into() {
            self.integral = self.parameters.integral_max.into();
        }
        let i = self.integral;

        // derivative
        let d = match self.last_input {
            None => 0.0,
            Some(last_input) => f64::from(self.parameters.kd) * (input - last_input),
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

type JsonBuffer = heapless::Vec<u8, heapless::consts::U360>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Summary {
    channel: usize,
    parameters: Parameters,
    target: f64,
    integral: f64,
}

impl Summary {
    pub fn to_json(&self) -> Result<JsonBuffer, serde_json_core::ser::Error> {
        serde_json_core::to_vec(self)
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
            let output = pid.update(values[next_t]);
            // Overwrite oldest with previous temperature - output
            values[next_t] = values[t] - output;
            t = next_t;
            total_t += 1;
        }
    }

    #[test]
    fn summary_to_json() {
        let mut pid = Controller::new(PARAMETERS.clone());
        pid.target = 30.0 / 1.1;
        let buf = pid.summary(0).to_json().unwrap();
        assert_eq!(buf[0], b'{');
        assert_eq!(buf[buf.len() - 1], b'}');
    }
}
