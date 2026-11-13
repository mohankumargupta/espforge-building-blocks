use crate::Context;

/// `control_parameters`
const KP: f32 = 0.04;
const KI: f32 = 0.001;
const KD: f32 = 0.1;
/// `default_target_temperature: 150.0`
const TARGET_CENTI: i32 = 15_000;
/// `output_averaging_samples: 5`
const AVERAGING: usize = 5;
/// The SCD4x measures every five seconds, which sets the loop period.
const PERIOD_S: f32 = 5.0;
/// Clamping the integral term at the output limits is the anti-windup.
const OUTPUT_MIN: f32 = 0.0;
const OUTPUT_MAX: f32 = 1.0;

/// One PID state: error history and the clamped integral.
struct Pid {
    integral: f32,
    previous_measurement: Option<f32>,
    window: [f32; AVERAGING],
    filled: usize,
}

impl Pid {
    const fn new() -> Self {
        Self {
            integral: 0.0,
            previous_measurement: None,
            window: [0.0; AVERAGING],
            filled: 0,
        }
    }

    /// Returns the averaged duty cycle in 0.0..=1.0.
    fn update(&mut self, measurement: f32, target: f32) -> f32 {
        let error = target - measurement;

        // Accumulate, then clamp the *integral* so it cannot wind up while the
        // output is saturated. Clamping the output alone is not enough.
        self.integral += KI * error * PERIOD_S;
        let max_integral = (OUTPUT_MAX - KP * error).clamp(OUTPUT_MIN, OUTPUT_MAX);
        self.integral = self.integral.clamp(-max_integral, max_integral);

        // Derivative on the measurement, not the error: a setpoint step then
        // does not produce a derivative kick.
        let derivative = match self.previous_measurement {
            Some(previous) => -(measurement - previous) / PERIOD_S,
            None => 0.0,
        };
        self.previous_measurement = Some(measurement);

        let output = (KP * error + self.integral + KD * derivative)
            .clamp(OUTPUT_MIN, OUTPUT_MAX);

        self.window[self.filled % AVERAGING] = output;
        self.filled += 1;
        let used = self.filled.min(AVERAGING);
        self.window[..used].iter().sum::<f32>() / used as f32
    }
}

pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "pid-climate: kp 0.04, ki 0.001, kd 0.1, target 150.0 C (no output)",
    );

    if device!(ctx, air).start().is_err() {
        ctx.logger.warn("pid-climate: sensor did not start");
    }
}

pub fn forever(ctx: &mut Context) {
    // The integral has to survive between ticks: `forever` is called repeatedly.
    static mut PID: Pid = Pid::new();

    let reading = match device!(ctx, air).read() {
        Ok(reading) => reading,
        Err(_) => {
            ctx.logger.warn("pid-climate: sensor read failed");
            ctx.delay.delay_ms(5_000);
            return;
        }
    };

    let measurement = f32::from(reading.temperature_centi_c) / 100.0;
    let target = TARGET_CENTI as f32 / 100.0;

    // SAFETY: `forever` is the only user of PID, and the scheduler does not
    // re-enter it. A driver would hold this in the climate entity instead.
    let duty = unsafe { PID.update(measurement, target) };

    ctx.logger.info(format_args!(
        "reflow: {measurement:.1} C -> {target:.1} C, duty {:.1}% (proposed output)",
        duty * 100.0
    ));

    ctx.delay.delay_ms(5_000);
}
