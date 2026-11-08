//! Blend trees (issue #111).
//!
//! A blend state used to export as an animation state with an empty clip id,
//! so it posed nothing at all on device while the editor blended. This mirrors
//! `blend_evaluator.dart`: the same weights, so the two produce the same pose
//! rather than two readings of the same idea.

use crate::format;

/// One clip and how much of it reaches the pose.
#[derive(Debug, Clone, PartialEq)]
pub struct BlendWeight {
    pub clip_id: String,
    pub weight: f64,
}

/// Linear 1D blend along the sorted threshold axis.
///
/// Below the first threshold or above the last, the end clip takes the whole
/// pose rather than fading out into nothing.
pub fn evaluate_1d(points: &[(String, f64)], value: f64) -> Vec<BlendWeight> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));

    let first = &sorted[0];
    if value <= first.1 {
        return vec![BlendWeight {
            clip_id: first.0.clone(),
            weight: 1.0,
        }];
    }
    let last = &sorted[sorted.len() - 1];
    if value >= last.1 {
        return vec![BlendWeight {
            clip_id: last.0.clone(),
            weight: 1.0,
        }];
    }

    for pair in sorted.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if value >= a.1 && value <= b.1 {
            let range = b.1 - a.1;
            // Two points at the same threshold split evenly: there is no
            // gradient to read a position off.
            let t = if range > 0.0 { (value - a.1) / range } else { 0.5 };
            return vec![
                BlendWeight {
                    clip_id: a.0.clone(),
                    weight: 1.0 - t,
                },
                BlendWeight {
                    clip_id: b.0.clone(),
                    weight: t,
                },
            ];
        }
    }
    Vec::new()
}

/// Barycentric 2D blend over the nearest three points.
pub fn evaluate_2d(points: &[(String, f64, f64)], x: f64, y: f64) -> Vec<BlendWeight> {
    if points.is_empty() {
        return Vec::new();
    }
    if points.len() == 1 {
        return vec![BlendWeight {
            clip_id: points[0].0.clone(),
            weight: 1.0,
        }];
    }
    if points.len() == 2 {
        // Two points make a line, not a triangle, so this degrades to the 1D
        // case projected onto it.
        let (a, b) = (&points[0], &points[1]);
        let (ax, ay) = (b.1 - a.1, b.2 - a.2);
        let len2 = ax * ax + ay * ay;
        if len2 <= 0.0 {
            return vec![BlendWeight {
                clip_id: a.0.clone(),
                weight: 1.0,
            }];
        }
        let t = (((x - a.1) * ax + (y - a.2) * ay) / len2).clamp(0.0, 1.0);
        return vec![
            BlendWeight {
                clip_id: a.0.clone(),
                weight: 1.0 - t,
            },
            BlendWeight {
                clip_id: b.0.clone(),
                weight: t,
            },
        ];
    }

    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| {
        let da = (a.1 - x).powi(2) + (a.2 - y).powi(2);
        let db = (b.1 - x).powi(2) + (b.2 - y).powi(2);
        da.total_cmp(&db)
    });
    let (p0, p1, p2) = (&sorted[0], &sorted[1], &sorted[2]);

    let denom = (p1.2 - p2.2) * (p0.1 - p2.1) + (p2.1 - p1.1) * (p0.2 - p2.2);
    if denom.abs() < 1e-10 {
        // Three points on a line have no interior to interpolate across.
        return vec![BlendWeight {
            clip_id: p0.0.clone(),
            weight: 1.0,
        }];
    }

    let w0 = ((p1.2 - p2.2) * (x - p2.1) + (p2.1 - p1.1) * (y - p2.2)) / denom;
    let w1 = ((p2.2 - p0.2) * (x - p2.1) + (p0.1 - p2.1) * (y - p2.2)) / denom;
    let w2 = 1.0 - w0 - w1;

    // Outside the triangle the barycentric weights go negative; clamping and
    // renormalising pins the pose to the nearest edge instead of inverting it.
    let (c0, c1, c2) = (
        w0.clamp(0.0, 1.0),
        w1.clamp(0.0, 1.0),
        w2.clamp(0.0, 1.0),
    );
    let sum = c0 + c1 + c2;
    if sum <= 0.0 {
        return vec![BlendWeight {
            clip_id: p0.0.clone(),
            weight: 1.0,
        }];
    }

    vec![
        BlendWeight {
            clip_id: p0.0.clone(),
            weight: c0 / sum,
        },
        BlendWeight {
            clip_id: p1.0.clone(),
            weight: c1 / sum,
        },
        BlendWeight {
            clip_id: p2.0.clone(),
            weight: c2 / sum,
        },
    ]
}

/// The clips and weights a state resolves to, given the machine's inputs.
///
/// An animation state answers with its own clip at full weight, so a caller
/// never has to ask which kind it is holding.
pub fn weights_for_state(
    state: format::SMState<'_>,
    number: &dyn Fn(&str) -> Option<f64>,
) -> Vec<BlendWeight> {
    let points = state.blend_points().unwrap_or_default();
    match state.kind() {
        format::generated::kineto::SMStateKind::Blend1D => {
            let value = state.input_id_x().and_then(number).unwrap_or(0.0);
            let axis: Vec<(String, f64)> = points
                .iter()
                .map(|p| (p.clip_id().to_owned(), p.x() as f64))
                .collect();
            evaluate_1d(&axis, value)
        }
        format::generated::kineto::SMStateKind::Blend2D => {
            let x = state.input_id_x().and_then(number).unwrap_or(0.0);
            let y = state.input_id_y().and_then(number).unwrap_or(0.0);
            let plane: Vec<(String, f64, f64)> = points
                .iter()
                .map(|p| (p.clip_id().to_owned(), p.x() as f64, p.y() as f64))
                .collect();
            evaluate_2d(&plane, x, y)
        }
        // Animation, and any kind a newer file carries that this build does
        // not know: the clip it names is the honest answer.
        _ => {
            let clip_id = state.clip_id();
            if clip_id.is_empty() {
                Vec::new()
            } else {
                vec![BlendWeight {
                    clip_id: clip_id.to_owned(),
                    weight: 1.0,
                }]
            }
        }
    }
}
