//! Behavior analysis (v1).
//!
//! Derive a human-likeness score in `[0, 100]` (higher = more human-like) from
//! interaction telemetry: mouse movement, click cadence, and keystroke timing.
//!
//! The signal is *regularity*: real human interaction is noisy — mouse speed
//! varies, paths curve, click intervals drift. Scripts tend toward constant
//! speed, metronomic clicks, and uniform key timing. We measure the
//! coefficient of variation (CV = stddev/mean) of the relevant quantities and
//! map it: CV near 0 (perfectly regular) ⇒ bot-like; CV ≥ ~0.5 ⇒ clearly
//! human. CV is a deliberately simple v1 heuristic; richer trajectory physics
//! (acceleration profiles, Bézier curvature) can layer on later behind the
//! same [`score`] interface.
//!
//! All functions here are pure — deterministic over the input events — so they
//! build for both native and wasm32 and unit-test without mocks.

/// Minimum mouse samples required to extract a trustworthy speed CV.
const MIN_MOUSE_SAMPLES: usize = 5;

/// One recorded mouse position. `t_ms` is a monotonic timestamp in ms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseSample {
    pub x: f64,
    pub y: f64,
    pub t_ms: f64,
}

/// Interaction telemetry collected client-side.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BehaviorInput {
    /// Mouse move samples (oldest first).
    pub mouse: Vec<MouseSample>,
    /// Inter-click intervals in ms.
    pub click_intervals_ms: Vec<f64>,
    /// Inter-key intervals in ms (keydown→keydown).
    pub key_intervals_ms: Vec<f64>,
}

/// CV at or above this is treated as "clearly human" (score 100).
const HUMAN_CV: f64 = 0.5;

/// Human-liakeness score in `[0, 100]`, or `None` if there's too little signal
/// to judge (caller should then omit `behavior_score` from the request).
pub fn score(input: &BehaviorInput) -> Option<u32> {
    let mut cvs: Vec<f64> = Vec::new();
    if let Some(cv) = mouse_speed_cv(&input.mouse) {
        cvs.push(cv);
    }
    if input.click_intervals_ms.len() >= 2 {
        cvs.push(cv_of(&input.click_intervals_ms));
    }
    if input.key_intervals_ms.len() >= 2 {
        cvs.push(cv_of(&input.key_intervals_ms));
    }
    if cvs.is_empty() {
        return None;
    }
    let avg_cv = cvs.iter().sum::<f64>() / cvs.len() as f64;
    // Linear map: cv 0 → 0 (bot), cv HUMAN_CV → 100 (human), clamped.
    let mapped = (avg_cv / HUMAN_CV) * 100.0;
    Some(mapped.round().clamp(0.0, 100.0) as u32)
}

/// CV of per-segment mouse speeds. `None` if too few samples or no movement.
fn mouse_speed_cv(mouse: &[MouseSample]) -> Option<f64> {
    if mouse.len() < MIN_MOUSE_SAMPLES {
        return None;
    }
    let speeds: Vec<f64> = mouse
        .windows(2)
        .map(|w| speed_between(&w[0], &w[1]))
        .collect();
    if speeds.iter().sum::<f64>() <= 0.0 {
        return None; // no movement → CV is meaningless
    }
    Some(cv_of(&speeds))
}

fn speed_between(a: &MouseSample, b: &MouseSample) -> f64 {
    let dt = (b.t_ms - a.t_ms).max(1.0); // avoid div-by-zero / negative for ties
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dx.hypot(dy) / dt
}

/// Coefficient of variation (stddev / |mean|) of a non-empty sample. Returns 0
/// for degenerate input (fewer than 2 points, or zero mean).
fn cv_of(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    if mean.abs() <= f64::EPSILON {
        return 0.0;
    }
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    var.sqrt() / mean.abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mouse_line(n: usize) -> Vec<MouseSample> {
        // Constant speed, straight line → CV ≈ 0 (bot-like).
        (0..n)
            .map(|i| MouseSample {
                x: i as f64,
                y: 0.0,
                t_ms: i as f64 * 10.0,
            })
            .collect()
    }

    fn mouse_variable(n: usize) -> Vec<MouseSample> {
        // Accelerating + jittering → high speed CV (human-like).
        (0..n)
            .map(|i| MouseSample {
                x: (i * i) as f64 + (i % 3) as f64,
                y: (i % 2) as f64,
                t_ms: i as f64 * 10.0,
            })
            .collect()
    }

    #[test]
    fn empty_input_is_none() {
        assert_eq!(score(&BehaviorInput::default()), None);
    }

    #[test]
    fn too_few_samples_is_none() {
        let input = BehaviorInput {
            mouse: mouse_line(3),
            ..Default::default()
        };
        assert_eq!(score(&input), None);
    }

    #[test]
    fn metronomic_clicks_score_low() {
        // Perfectly regular intervals → CV 0 → score 0.
        let input = BehaviorInput {
            click_intervals_ms: vec![100.0, 100.0, 100.0, 100.0],
            ..Default::default()
        };
        assert_eq!(score(&input), Some(0));
    }

    #[test]
    fn variable_clicks_score_high() {
        // Highly variable intervals → high CV → score 100.
        let input = BehaviorInput {
            click_intervals_ms: vec![50.0, 500.0, 80.0, 900.0, 120.0],
            ..Default::default()
        };
        let s = score(&input).unwrap();
        assert!(s >= 90, "variable clicks should score high, got {s}");
    }

    #[test]
    fn linear_mouse_scores_low() {
        let input = BehaviorInput {
            mouse: mouse_line(10),
            ..Default::default()
        };
        let s = score(&input).unwrap();
        assert_eq!(s, 0, "constant-speed straight line is bot-like");
    }

    #[test]
    fn variable_mouse_scores_higher_than_linear() {
        let linear = score(&BehaviorInput {
            mouse: mouse_line(10),
            ..Default::default()
        })
        .unwrap();
        let human = score(&BehaviorInput {
            mouse: mouse_variable(10),
            ..Default::default()
        })
        .unwrap();
        assert!(
            human > linear,
            "variable mouse ({human}) should beat linear ({linear})"
        );
    }

    #[test]
    fn score_is_bounded_0_to_100() {
        // Absurdly variable input still clamps.
        let input = BehaviorInput {
            click_intervals_ms: vec![1.0, 1e9, 1.0, 1e9, 1.0],
            ..Default::default()
        };
        let s = score(&input).unwrap();
        assert!(s <= 100);
    }

    #[test]
    fn cv_of_degenerate_is_zero() {
        assert_eq!(cv_of(&[5.0]), 0.0);
        assert_eq!(cv_of(&[0.0, 0.0, 0.0]), 0.0);
    }
}
