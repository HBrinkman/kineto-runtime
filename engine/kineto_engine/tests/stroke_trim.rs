//! Stroke trim (issue #158): which span of an outline a trimmed stroke draws.
//!
//! The windows are checked against `test/fixtures/stroke_trim_windows.txt`,
//! the same table the editor's Dart and Swift tests read, so the runtime and
//! the editor canvas cannot disagree about where a trim cuts.

use kineto_engine::render::{trim_windows, TrimWindow};

const FIXTURE: &str = include_str!("../../../test/fixtures/stroke_trim_windows.txt");

fn parse_expectation(text: &str) -> Option<Vec<TrimWindow>> {
    match text {
        "full" => None,
        "none" => Some(Vec::new()),
        spans => Some(
            spans
                .split_whitespace()
                .map(|span| {
                    let (from, to) = span.split_once(':').expect("span is from:to");
                    TrimWindow {
                        from: from.parse().expect("from parses"),
                        to: to.parse().expect("to parses"),
                    }
                })
                .collect(),
        ),
    }
}

#[test]
fn trim_windows_match_the_shared_fixture() {
    let mut checked = 0;
    for line in FIXTURE.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (input, expected) = line.split_once("=>").expect("line has =>");
        let fields: Vec<&str> = input.split_whitespace().collect();
        let [start, end, offset, kind] = fields[..] else {
            panic!("bad fixture line: {line}");
        };
        let actual = trim_windows(
            start.parse().unwrap(),
            end.parse().unwrap(),
            offset.parse().unwrap(),
            kind == "closed",
        );
        let expected = parse_expectation(expected.trim());
        match (&actual, &expected) {
            (None, None) => {}
            (Some(a), Some(e)) if a.len() == e.len() => {
                for (a, e) in a.iter().zip(e) {
                    assert!(
                        (a.from - e.from).abs() < 1e-9 && (a.to - e.to).abs() < 1e-9,
                        "{line}: got {actual:?}"
                    );
                }
            }
            _ => panic!("{line}: got {actual:?}, expected {expected:?}"),
        }
        checked += 1;
    }
    assert!(checked >= 10, "the fixture should carry the cases");
}

#[cfg(feature = "vello-render")]
mod outline {
    use kineto_engine::render::{
        trim_outline, RenderCap, RenderColor, RenderJoin, RenderStroke, RenderStrokeAlign,
    };
    use vello::kurbo::{BezPath, PathEl, Rect, Shape};

    fn stroke(start: f64, end: f64, offset: f64) -> RenderStroke {
        RenderStroke {
            color: RenderColor {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            width: 2.0,
            cap: RenderCap::Butt,
            join: RenderJoin::Miter,
            miter_limit: 4.0,
            dash_array: Vec::new(),
            dash_offset: 0.0,
            align: RenderStrokeAlign::Center,
            trim_start: start,
            trim_end: end,
            trim_offset: offset,
        }
    }

    fn length(path: &BezPath) -> f64 {
        path.segments()
            .map(|s| vello::kurbo::ParamCurveArclen::arclen(&s, 1e-6))
            .sum()
    }

    fn moves(path: &BezPath) -> usize {
        path.elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count()
    }

    fn square() -> BezPath {
        Rect::new(0.0, 0.0, 20.0, 20.0).to_path(0.1)
    }

    #[test]
    fn a_neutral_trim_leaves_the_outline_alone() {
        assert!(trim_outline(&square(), &stroke(0.0, 1.0, 0.4)).is_none());
    }

    #[test]
    fn start_and_end_cut_the_visible_span() {
        let trimmed = trim_outline(&square(), &stroke(0.0, 0.5, 0.0)).expect("trimmed");
        assert!((length(&trimmed) - 40.0).abs() < 1e-3);
        // From the top-left corner, clockwise: the top edge, then the right.
        let first = trimmed.elements()[0];
        assert_eq!(first, PathEl::MoveTo((0.0, 0.0).into()));
        let end = trimmed.segments().last().unwrap();
        let end = vello::kurbo::ParamCurve::end(&end);
        assert!((end.x - 20.0).abs() < 1e-3 && (end.y - 20.0).abs() < 1e-3);
    }

    #[test]
    fn offset_slides_the_span_through_the_seam_as_one_piece() {
        let trimmed = trim_outline(&square(), &stroke(0.5, 1.0, 0.25)).expect("trimmed");
        assert!((length(&trimmed) - 40.0).abs() < 1e-3);
        assert_eq!(moves(&trimmed), 1, "a closed outline wraps without a break");
        // 0.75 of the 80-unit perimeter is past the top, right and bottom
        // edges: the span starts at the bottom-left corner.
        let PathEl::MoveTo(start) = trimmed.elements()[0] else {
            panic!("starts with a move");
        };
        assert!((start.x - 0.0).abs() < 1e-3 && (start.y - 20.0).abs() < 1e-3);
    }

    #[test]
    fn an_open_path_wraps_into_two_pieces() {
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((100.0, 0.0));
        let trimmed = trim_outline(&line, &stroke(0.5, 1.0, 0.25)).expect("trimmed");
        assert_eq!(moves(&trimmed), 2);
        assert!((length(&trimmed) - 50.0).abs() < 1e-3);
    }

    #[test]
    fn an_empty_span_draws_nothing() {
        let trimmed = trim_outline(&square(), &stroke(0.3, 0.3, 0.0)).expect("trimmed");
        assert!(trimmed.elements().is_empty());
    }
}
