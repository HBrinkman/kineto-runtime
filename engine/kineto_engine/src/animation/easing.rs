/// Evaluate CSS cubic-bezier(x1,y1,x2,y2) at `t` ∈ [0,1].
/// Mirrors `cubicBezierEase` in `easing.dart`.
pub fn cubic_bezier_ease(t: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let u = solve_bezier_x(t, x1, x2);
    cubic_bezier_1d(u, 0.0, y1, y2, 1.0)
}

fn solve_bezier_x(x: f64, x1: f64, x2: f64) -> f64 {
    let mut u = x;
    for _ in 0..8 {
        let dx = cubic_bezier_1d(u, 0.0, x1, x2, 1.0) - x;
        if dx.abs() < 1e-7 {
            return u;
        }
        let d = cubic_bezier_derivative(u, 0.0, x1, x2, 1.0);
        if d.abs() < 1e-14 {
            break;
        }
        u -= dx / d;
    }
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        let val = cubic_bezier_1d(mid, 0.0, x1, x2, 1.0);
        if (val - x).abs() < 1e-7 {
            return mid;
        }
        if val < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

fn cubic_bezier_1d(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let mt = 1.0 - t;
    mt * mt * mt * p0 + 3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t * p3
}

fn cubic_bezier_derivative(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let mt = 1.0 - t;
    3.0 * (mt * mt * (p1 - p0) + 2.0 * mt * t * (p2 - p1) + t * t * (p3 - p2))
}

/// Smooth-step (no overshoot). Mirrors `autoBezierEase` in `easing.dart`.
pub fn auto_bezier_ease(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// The 6 MVP easing presets. Ordinals match the Dart `EasingPreset` enum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EasingPreset {
    Linear = 0,
    Ease = 1,
    EaseIn = 2,
    EaseOut = 3,
    EaseInOut = 4,
    EasyEase = 5,
}

impl EasingPreset {
    /// CSS cubic-bezier handles: (x1, y1, x2, y2).
    fn handles(self) -> (f64, f64, f64, f64) {
        match self {
            EasingPreset::Linear => (0.0, 0.0, 1.0, 1.0),
            EasingPreset::Ease => (0.25, 0.1, 0.25, 1.0),
            EasingPreset::EaseIn => (0.42, 0.0, 1.0, 1.0),
            EasingPreset::EaseOut => (0.0, 0.0, 0.58, 1.0),
            EasingPreset::EaseInOut => (0.42, 0.0, 0.58, 1.0),
            EasingPreset::EasyEase => (0.33, 0.0, 0.67, 1.0),
        }
    }

    pub fn apply(self, t: f64) -> f64 {
        if self == EasingPreset::Linear {
            return t;
        }
        let (x1, y1, x2, y2) = self.handles();
        cubic_bezier_ease(t, x1, y1, x2, y2)
    }

    /// Map the FlatBuffers `easing` byte (Dart `EasingPreset` ordinal) to a preset.
    pub fn from_kvk_ordinal(ordinal: u8) -> Self {
        match ordinal {
            0 => EasingPreset::Linear,
            1 => EasingPreset::Ease,
            2 => EasingPreset::EaseIn,
            3 => EasingPreset::EaseOut,
            4 => EasingPreset::EaseInOut,
            5 => EasingPreset::EasyEase,
            _ => EasingPreset::Linear,
        }
    }
}
