//! Range and step rules of the numeric settings rows, shared by the pointer
//! (a click or drag on the rail) and typed entry (a number in the value
//! column), so both land on the values the slider itself can show.

use super::catalog::ValueKind;
use crate::menu_widgets::NumberFormat;

impl ValueKind {
    /// The cvar text for `raw` on this slider: rounded to the nearest step
    /// counted from the minimum, then clamped to the range. `None` for rows
    /// that are not sliders.
    pub(super) fn snapped(self, raw: f64) -> Option<String> {
        match self {
            Self::Integer { min, max, step } => {
                let step = step.max(1);
                let steps = ((raw - min as f64) / step as f64).round();
                let value = min.saturating_add((steps as i64).saturating_mul(step));
                Some(value.clamp(min, max).to_string())
            }
            Self::Float { min, max, step } => {
                let value = if step > 0.0 {
                    ((raw - min) / step).round() * step + min
                } else {
                    raw
                };
                // Steps such as 0.05 accumulate binary noise; keep only the
                // decimals the step has so the value reads as typed.
                let scale = 10f64.powi(decimals(step));
                let value = ((value * scale).round() / scale).clamp(min, max);
                Some(value.to_string())
            }
            _ => None,
        }
    }

    /// The characters typed entry accepts for this slider.
    pub(super) fn number_format(self) -> Option<NumberFormat> {
        match self {
            Self::Integer { min, .. } => Some(NumberFormat {
                fraction: false,
                negative: min < 0,
            }),
            Self::Float { min, .. } => Some(NumberFormat {
                fraction: true,
                negative: min < 0.0,
            }),
            _ => None,
        }
    }
}

/// Decimal places of `step` (0.05 has 2), at most 6.
fn decimals(step: f64) -> i32 {
    (0..6)
        .find(|places| {
            let shifted = step * 10f64.powi(*places);
            (shifted - shifted.round()).abs() < 1e-6
        })
        .unwrap_or(6)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: ValueKind = ValueKind::Integer {
        min: 0,
        max: 2000,
        step: 25,
    };
    const FOV: ValueKind = ValueKind::Float {
        min: 70.0,
        max: 130.0,
        step: 5.0,
    };
    const VOLUME: ValueKind = ValueKind::Float {
        min: 0.0,
        max: 1.0,
        step: 0.05,
    };
    const SENSITIVITY: ValueKind = ValueKind::Float {
        min: 0.1,
        max: 20.0,
        step: 0.25,
    };

    #[test]
    fn integers_round_to_the_step_and_clamp() {
        assert_eq!(FPS.snapped(144.0).as_deref(), Some("150"));
        assert_eq!(FPS.snapped(137.0).as_deref(), Some("125"));
        assert_eq!(FPS.snapped(5000.0).as_deref(), Some("2000"));
        assert_eq!(FPS.snapped(-3.0).as_deref(), Some("0"));
        assert_eq!(FPS.snapped(1e300).as_deref(), Some("2000"));
    }

    #[test]
    fn floats_round_to_the_step_and_clamp() {
        assert_eq!(FOV.snapped(103.0).as_deref(), Some("105"));
        assert_eq!(FOV.snapped(97.4).as_deref(), Some("95"));
        assert_eq!(FOV.snapped(10.0).as_deref(), Some("70"));
        assert_eq!(FOV.snapped(400.0).as_deref(), Some("130"));
    }

    #[test]
    fn fractional_steps_read_without_binary_noise() {
        assert_eq!(VOLUME.snapped(0.7).as_deref(), Some("0.7"));
        assert_eq!(VOLUME.snapped(0.33).as_deref(), Some("0.35"));
        assert_eq!(VOLUME.snapped(1.0).as_deref(), Some("1"));
    }

    #[test]
    fn steps_count_from_the_minimum() {
        // The slider shows 0.1, 0.35, 0.6, ...; typing lands on one of them.
        assert_eq!(SENSITIVITY.snapped(5.0).as_deref(), Some("5.1"));
        assert_eq!(SENSITIVITY.snapped(0.0).as_deref(), Some("0.1"));
        assert_eq!(SENSITIVITY.snapped(25.0).as_deref(), Some("20"));
    }

    #[test]
    fn non_sliders_have_no_number() {
        assert_eq!(ValueKind::Bool.snapped(1.0), None);
        assert_eq!(ValueKind::Text.number_format(), None);
        assert_eq!(
            FOV.number_format(),
            Some(NumberFormat {
                fraction: true,
                negative: false,
            })
        );
        assert_eq!(
            FPS.number_format(),
            Some(NumberFormat {
                fraction: false,
                negative: false,
            })
        );
    }
}
