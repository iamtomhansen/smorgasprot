//! Smoothly animated scalar used for the flatten, butterfly, and explode sliders.
//! Exponential easing is frame-rate independent, so it behaves the same at 30 or 144 FPS.

/// Easing rate in 1/s (≈ 18 % of the remaining distance per frame at 60 FPS).
const RATE_PER_S: f32 = 12.0;
/// Distance below which the value snaps to its target.
const SNAP_EPS: f32 = 1e-3;

/// A displayed `value` that eases toward a `target`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Tween {
    pub value: f32,
    pub target: f32,
}

impl Tween {
    pub const fn new(value: f32) -> Self {
        Self { value, target: value }
    }

    /// Starts an eased transition toward `target` (preset buttons).
    pub fn animate_to(&mut self, target: f32) {
        self.target = target;
    }

    /// Jumps immediately to `value` (direct slider drags).
    pub fn snap_to(&mut self, value: f32) {
        self.value = value;
        self.target = value;
    }

    /// Advances by `dt` seconds; returns `true` while still moving.
    pub fn step(&mut self, dt: f32) -> bool {
        let remaining = self.target - self.value;
        if remaining.abs() < SNAP_EPS {
            self.value = self.target;
            return false;
        }
        self.value += remaining * (1.0 - (-RATE_PER_S * dt).exp());
        true
    }
}
