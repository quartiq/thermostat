

// Biquadratic (BiQuad) Infinite Impulse Response (IIR) Filter.
// Uses the implementation from: https://hackmd.io/IACbwcOTSt6Adj3_F9bKuw?view


/// Generic vector for integer IIR filter.
/// This struct is used to hold the x/y input/output data vector or the b/a coefficient
/// vector.
pub type Vec5 = [f64; 5];



/// Main IIR struct holds coefficient vector and a shift value which defines the fixed point position
#[derive(Debug, Copy, Clone)]
pub struct Iir {
    pub ba: Vec5,   // b and a coeffitients can be changed. [b0,b1,b2,a1,a2]
    pub xy: Vec5,   // x and y internal filter states       [x0,x1,y0,y1,y2]
    pub target: f64,        // tagret value. Is subtracted from the filter output.
    pub ylim: [f64; 2]      // lower and uppe y limit
}

impl Iir {

    pub fn new() -> Iir {
        Iir{
            ba: [0.1, 0.0, 0.0, 0.0, 0.0],        // default to only proportional feedback
            xy: [0.0, 0.0, 0.0, 0.0, 0.0],
            target: 0.0,
            ylim: [-0.7, 0.7]
        }
    }

    /// Filter tick. Takes a new input sample and time delta and returns a new output sample.
    pub fn tick(&mut self, x0: f64) -> f64 {

        // shift in x0
        self.xy.copy_within(0..4, 1);
        self.xy[0] = -x0;       // invert to stay consistent with previous implementation

        let y0 = 0.0;
        let y_ = self.xy
            .iter()
            .zip(&self.ba)
            .map(|(x, a)| *x as f64 * *a as f64)
            .fold(y0, |y, xa| y + xa);
        let y_ = y_ + self.target;       // add target
        let y_ = max(self.ylim[0], min(self.ylim[1], y_));      //limit
        self.xy[2] = y_;
        y_
    }
}



pub fn max<T>(x: T, y: T) -> T
where
    T: PartialOrd,
{
    if x > y {
        x
    } else {
        y
    }
}


pub fn min<T>(x: T, y: T) -> T
where
    T: PartialOrd,
{
    if x < y {
        x
    } else {
        y
    }
}
