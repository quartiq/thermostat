#[derive(Clone, Copy)]
pub struct Parameters {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub output_min: f32,
    pub output_max: f32,
    pub integral_min: f32,
    pub integral_max: f32
}

#[derive(Clone)]
pub struct Controller {
    parameters: Parameters,
    target: f32,
    integral: f32,
    last_input: Option<f32>
}

impl Controller {
    pub const fn new(parameters: Parameters) -> Controller {
        Controller {
            parameters: parameters,
            target: 0.0,
            last_input: None,
            integral: 0.0
        }
    }

    pub fn update(&mut self, input: f32) -> f32 {
        let error = self.target - input;

        let p = self.parameters.kp * error;

        self.integral += error;
        if self.integral < self.parameters.integral_min {
            self.integral = self.parameters.integral_min;
        }
        if self.integral > self.parameters.integral_max {
            self.integral = self.parameters.integral_max;
        }
        let i = self.parameters.ki * self.integral;

        let d = match self.last_input {
            None => 0.0,
            Some(last_input) => self.parameters.kd * (last_input - input)
        };
        self.last_input = Some(input);

        let mut output = p + i + d;
        if output < self.parameters.output_min {
            output = self.parameters.output_min;
        }
        if output > self.parameters.output_max {
            output = self.parameters.output_max;
        }
        output
    }

    pub fn get_target(&self) -> f32 {
        self.target
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn get_parameters(&self) -> &Parameters {
        &self.parameters
    }

    pub fn update_parameters<F: FnOnce(&mut Parameters)>(&mut self, f: F) {
        f(&mut self.parameters);
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
        const DEFAULT: f32 = 0.0;
        const TARGET: f32 = 1234.56;
        const ERROR: f32 = 0.01;
        const DELAY: usize = 10;

        let mut pid = Controller::new(PARAMETERS.clone());
        pid.set_target(TARGET);

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
        dbg!(values[t], total_t);
    }
}
