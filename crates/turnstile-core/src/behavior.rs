//! Behavior analysis (v2).
//!
//! Derive a human-likeness score in `[0, 100]` (higher = more human-like) from
//! interaction telemetry: mouse movement, click cadence, keystroke timing, and
//! mouse path geometry.
//!
//! Four orthogonal signals are averaged:
//!
//! 1. **Timing CV** (v1): coefficient of variation of mouse speed, click
//!    intervals, and key intervals. Scripts tend toward constant speed and
//!    metronomic timing → CV near 0. Humans are irregular → high CV.
//!
//! 2. **Angular jitter** (v2): mean absolute angular change between consecutive
//!    mouse-movement segments. AI-controlled cursors move in unnaturally smooth
//!    arcs with near-identical direction vectors → low jitter. Human hands
//!    produce micro-tremors → high jitter.
//!
//! 3. **Straightness** (v2): path length ÷ direct end-to-end distance. A
//!    deterministic algorithm drawing a perfect straight line (or a smooth
//!    Bézier) yields a ratio near 1.0. A human wobbles → ratio > 1.5.
//!
//! 4. **Click coherence** (v2): fraction of recent mouse samples that land
//!    within a proximity radius of the click point. AI tooling ("click element")
//!    teleports the cursor directly onto the target — the surrounding mouse
//!    trace is nowhere near it. A human gradually approaches → high coherence.
//!
//! All functions are pure — deterministic over the input events — so they
//! build for both native and wasm32 and unit-test without mocks.

/// Minimum mouse samples required for geometry-based signals.
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
    /// Click positions as `(x, y)` pairs, in chronological order.
    pub click_positions: Vec<(f64, f64)>,
}

/// CV at or above this is treated as "clearly human" for timing signals.
const HUMAN_CV: f64 = 0.5;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Human-likeness score in `[0, 100]`, or `None` if there's too little signal
/// to judge (caller should then omit `behavior_score` from the request).
pub fn score(input: &BehaviorInput) -> Option<u32> {
    let mut signals: Vec<f64> = Vec::new();

    // 1. Timing CV
    if let Some(s) = timing_cv_score(input) {
        signals.push(s);
    }
    // 2. Angular jitter
    if let Some(s) = angular_jitter(&input.mouse) {
        signals.push(s);
    }
    // 3. Straightness / curvature
    if let Some(s) = straightness_score(&input.mouse) {
        signals.push(s);
    }
    // 4. Click teleport coherence
    if let Some(s) = click_coherence(&input.mouse, &input.click_positions) {
        signals.push(s);
    }

    if signals.is_empty() {
        return None;
    }
    let avg = signals.iter().sum::<f64>() / signals.len() as f64;
    Some((avg * 100.0).round().clamp(0.0, 100.0) as u32)
}

// ---------------------------------------------------------------------------
// Signal 1: Timing CV (v1, preserved)
// ---------------------------------------------------------------------------

fn timing_cv_score(input: &BehaviorInput) -> Option<f64> {
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
    let avg = cvs.iter().sum::<f64>() / cvs.len() as f64;
    // Normalize: CV >= HUMAN_CV → 1.0
    Some((avg / HUMAN_CV).min(1.0))
}

fn mouse_speed_cv(mouse: &[MouseSample]) -> Option<f64> {
    if mouse.len() < MIN_MOUSE_SAMPLES {
        return None;
    }
    let speeds: Vec<f64> = mouse
        .windows(2)
        .map(|w| speed_between(&w[0], &w[1]))
        .collect();
    if speeds.iter().sum::<f64>() <= 0.0 {
        return None;
    }
    Some(cv_of(&speeds))
}

fn speed_between(a: &MouseSample, b: &MouseSample) -> f64 {
    let dt = (b.t_ms - a.t_ms).max(1.0);
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dx.hypot(dy) / dt
}

// ---------------------------------------------------------------------------
// Signal 2: Angular jitter (v2)
// ---------------------------------------------------------------------------

/// Mean absolute angular change (radians) between consecutive movement vectors.
///
/// AI-controlled cursors follow smooth arcs: consecutive direction vectors are
/// nearly identical → tiny angular changes. Humans produce micro-tremors: each
/// pair of vectors has measurable angular deviation.
///
/// Returns `[0.0, 1.0]` (0 = perfectly smooth bot, 1 = jittery human).
fn angular_jitter(mouse: &[MouseSample]) -> Option<f64> {
    if mouse.len() < MIN_MOUSE_SAMPLES {
        return None;
    }
    let mut total: f64 = 0.0;
    let mut count: usize = 0;
    for i in 2..mouse.len() {
        let dx1 = mouse[i - 1].x - mouse[i - 2].x;
        let dy1 = mouse[i - 1].y - mouse[i - 2].y;
        let dx2 = mouse[i].x - mouse[i - 1].x;
        let dy2 = mouse[i].y - mouse[i - 1].y;
        let len1 = dx1.hypot(dy1);
        let len2 = dx2.hypot(dy2);
        if len1 < 0.5 || len2 < 0.5 {
            continue; // skip stationary / tiny moves
        }
        let cos = ((dx1 * dx2 + dy1 * dy2) / (len1 * len2)).clamp(-1.0, 1.0);
        total += cos.acos();
        count += 1;
    }
    if count == 0 {
        return None;
    }
    let avg_rad = total / count as f64;
    // Calibration: human avg ≈ 0.3-0.8 rad; bot avg ≈ 0.02-0.1 rad.
    // Map 0 rad → 0.0, 0.5 rad → 1.0.
    Some((avg_rad / 0.5).min(1.0))
}

// ---------------------------------------------------------------------------
// Signal 3: Straightness / curvature (v2)
// ---------------------------------------------------------------------------

/// Path length ÷ direct end-to-end distance.
///
/// 1.0 = perfect straight line (bot-like). > 1.5 = distinctly curved (human).
/// Returns `[0.0, 1.0]` (0 = straight bot, 1 = curvy human).
fn straightness_score(mouse: &[MouseSample]) -> Option<f64> {
    if mouse.len() < 3 {
        return None;
    }
    let first = &mouse[0];
    let last = &mouse[mouse.len() - 1];
    let direct = (last.x - first.x).hypot(last.y - first.y);
    if direct < 1.0 {
        return None; // didn't move enough
    }
    let path: f64 = mouse
        .windows(2)
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .sum();
    if path < 1.0 {
        return None;
    }
    let ratio = path / direct;
    // ratio 1.0 → 0.0; ratio 3.0+ → 1.0
    Some(((ratio - 1.0) / 2.0).min(1.0))
}

// ---------------------------------------------------------------------------
// Signal 4: Click coherence / teleport detection (v2)
// ---------------------------------------------------------------------------

/// Fraction of the last N mouse samples that lie within `PROXIMITY_PX` of
/// the most recent click position.
///
/// AI click tooling puts the cursor exactly on the element; the preceding
/// mouse trace (if any) was elsewhere. A human gradually approaches the target
/// → most recent samples are nearby.
///
/// Returns `[0.0, 1.0]` (0 = teleport, 1 = coherent approach).
const CLICK_PROXIMITY_PX: f64 = 60.0;
const RECENT_SAMPLES: usize = 10;

fn click_coherence(
    mouse: &[MouseSample],
    click_positions: &[(f64, f64)],
) -> Option<f64> {
    if click_positions.is_empty() || mouse.len() < 3 {
        return None;
    }
    // Evaluate the last click against the last RECENT_SAMPLES mouse positions.
    let (cx, cy) = click_positions[click_positions.len() - 1];
    let recent: Vec<&MouseSample> = mouse.iter().rev().take(RECENT_SAMPLES).collect();
    if recent.is_empty() {
        return None;
    }
    let close = recent
        .iter()
        .filter(|s| (s.x - cx).hypot(s.y - cy) < CLICK_PROXIMITY_PX)
        .count();
    Some(close as f64 / recent.len() as f64)
}

// ---------------------------------------------------------------------------
// Shared: coefficient of variation
// ---------------------------------------------------------------------------

/// Coefficient of variation (stddev / |mean|). Returns 0 for degenerate input.
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    /// Constant-speed straight line → 0 angular jitter, 0 curvature.
    fn mouse_line(n: usize) -> Vec<MouseSample> {
        (0..n)
            .map(|i| MouseSample {
                x: i as f64 * 10.0,
                y: 0.0,
                t_ms: i as f64 * 10.0,
            })
            .collect()
    }

    /// Accelerating + jittering → high speed CV, moderate jitter.
    fn mouse_variable(n: usize) -> Vec<MouseSample> {
        (0..n)
            .map(|i| MouseSample {
                x: (i * i) as f64 + (i % 3) as f64,
                y: (i % 2) as f64 * 5.0,
                t_ms: i as f64 * 10.0,
            })
            .collect()
    }

    /// Zigzag path → high angular jitter, high curvature.
    fn mouse_zigzag(n: usize) -> Vec<MouseSample> {
        (0..n)
            .map(|i| MouseSample {
                x: i as f64 * 10.0,
                y: if i % 2 == 0 { 0.0 } else { 20.0 },
                t_ms: i as f64 * 10.0,
            })
            .collect()
    }

    // -- score tests (v1 preserved) --

    #[test]
    fn empty_input_is_none() {
        assert_eq!(score(&BehaviorInput::default()), None);
    }

    #[test]
    fn metronomic_clicks_score_low() {
        let input = BehaviorInput {
            click_intervals_ms: vec![100.0, 100.0, 100.0, 100.0],
            ..Default::default()
        };
        let s = score(&input).unwrap();
        assert!(s <= 30, "metronomic clicks should score low, got {s}");
    }

    #[test]
    fn linear_mouse_scores_low() {
        // Straight line + constant speed → timing CV ≈ 0, jitter ≈ 0,
        // curvature ≈ 0 → overall low.
        let input = BehaviorInput {
            mouse: mouse_line(10),
            ..Default::default()
        };
        let s = score(&input).unwrap();
        assert!(s <= 20, "linear mouse is bot-like, got {s}");
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

    // -- angular jitter tests --

    #[test]
    fn straight_line_jitter_near_zero() {
        let j = angular_jitter(&mouse_line(10)).unwrap();
        assert!(j < 0.1, "straight line should have near-zero jitter, got {j}");
    }

    #[test]
    fn zigzag_jitter_is_high() {
        let j = angular_jitter(&mouse_zigzag(10)).unwrap();
        assert!(j > 0.8, "zigzag should have high jitter, got {j}");
    }

    // -- straightness tests --

    #[test]
    fn straight_line_curvature_near_zero() {
        let s = straightness_score(&mouse_line(10)).unwrap();
        assert!(s < 0.05, "straight line curvature near 0, got {s}");
    }

    #[test]
    fn zigzag_curvature_is_high() {
        let s = straightness_score(&mouse_zigzag(10)).unwrap();
        assert!(s > 0.5, "zigzag should be curvy, got {s}");
    }

    // -- click coherence tests --

    #[test]
    fn click_far_from_mouse_is_low_coherence() {
        // Mouse moves along X axis at Y=0. Click is at Y=200 — teleport.
        let mouse = mouse_line(10);
        let clicks = vec![(50.0, 200.0)];
        let c = click_coherence(&mouse, &clicks).unwrap();
        assert!(c < 0.3, "click far from mouse trace, got {c}");
    }

    #[test]
    fn click_near_mouse_is_high_coherence() {
        let mouse = mouse_line(10);
        // Click at the endpoint — human would have approached here.
        let clicks = vec![(90.0, 2.0)];
        let c = click_coherence(&mouse, &clicks).unwrap();
        assert!(c > 0.5, "click near mouse trace, got {c}");
    }

    // -- bounds --

    #[test]
    fn score_is_bounded_0_to_100() {
        let input = BehaviorInput {
            mouse: mouse_zigzag(20),
            click_intervals_ms: vec![1.0, 1e9, 1.0, 1e9],
            key_intervals_ms: vec![1.0, 1e9],
            ..Default::default()
        };
        let s = score(&input).unwrap();
        assert!(s <= 100);
    }
}
