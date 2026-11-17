//! The part of a curve's outline that connects its endpoint symbols.

use super::{arrowhead_paths, Arrowhead, LineCap, Stroke};
use crate::geometry::{
    BezPath, Line, ParamCurve as _, ParamCurveArclen as _, ParamCurveNearest as _, PathEl, PathSeg,
    Point, Shape as _,
};
use std::borrow::Cow;

/// The line body, shortened where it first meets each endpoint symbol.
/// Keep using `path` for the object's fill and for [`arrowhead_paths`].
///
/// The contact is found on the placed head's actual filled outline, so a
/// hollow head does not have a line through its hole and a notched head
/// keeps its connection. Round and square caps finish at the contact,
/// rather than extending half an outline width into the head. The line's
/// original Bezier segments are kept. Paths without heads, closed paths
/// and invalid geometry are borrowed without any new allocation.
/// `width_scale` is the scale used to draw the outline, without changing
/// the symbols' own sizes.
pub fn arrowhead_shaft_path<'a>(
    path: &'a BezPath,
    stroke: &Stroke,
    width_scale: f64,
) -> Cow<'a, BezPath> {
    let (end, _) = stroke.end_head_drawn();
    if (stroke.start_arrow == Arrowhead::None && *end == Arrowhead::None)
        || !stroke.width.is_finite()
        || !path.is_finite()
        || !matches!(path.elements().first(), Some(PathEl::MoveTo(_)))
        || path
            .elements()
            .iter()
            .any(|el| matches!(el, PathEl::ClosePath))
    {
        return Cow::Borrowed(path);
    }
    let segments: Vec<_> = path.segments().collect();
    let lengths: Vec<_> = segments.iter().map(|seg| seg.arclen(1e-5)).collect();
    let total: f64 = lengths.iter().sum();
    if !total.is_finite() || total <= 1e-9 {
        return Cow::Borrowed(path);
    }
    let cap = if stroke.cap == LineCap::Butt || !width_scale.is_finite() {
        0.0
    } else {
        stroke.width.max(0.0) * width_scale.max(0.0) * 0.5
    };
    let mut from = 0.0_f64;
    let mut to = total;
    let mut count = 0;
    let mut first_end = None;
    let mut last_start = 0;
    let mut segment_starts = Vec::with_capacity(segments.len());
    let mut starts_subpath = true;
    for element in path.elements() {
        if matches!(element, PathEl::MoveTo(_)) {
            starts_subpath = true;
            if count > 0 {
                first_end.get_or_insert(count);
                last_start = count;
            }
        } else {
            segment_starts.push(starts_subpath);
            starts_subpath = false;
            count += 1;
        }
    }
    let first_end = first_end.unwrap_or(segments.len());
    let first_length: f64 = lengths.iter().take(first_end).sum();
    let last_offset: f64 = lengths.iter().take(last_start).sum();
    if stroke.start_arrow != Arrowhead::None {
        if let Some(head) = endpoint_head(path, stroke, true) {
            if let Some(last) = contact_distance(
                segments
                    .iter()
                    .zip(&lengths)
                    .take(first_end)
                    .map(|(s, n)| (*s, *n)),
                &head,
            ) {
                from = (last + cap).min(first_length);
            }
        }
    }
    if *end != Arrowhead::None {
        if let Some(head) = endpoint_head(path, stroke, false) {
            if let Some(last) = contact_distance(
                segments
                    .iter()
                    .zip(&lengths)
                    .skip(last_start)
                    .rev()
                    .map(|(s, n)| (s.reverse(), *n)),
                &head,
            ) {
                to = (total - last - cap).max(last_offset);
            }
        }
    }
    if from == 0.0 && to == total {
        return Cow::Borrowed(path);
    }
    if from >= to - 1e-9 {
        return Cow::Owned(BezPath::new());
    }
    let mut out = BezPath::new();
    let mut offset = 0.0;
    let mut previous: Option<Point> = None;
    for ((seg, length), starts_subpath) in segments.iter().zip(&lengths).zip(segment_starts) {
        if starts_subpath {
            previous = None;
        }
        let low = from.max(offset);
        let high = to.min(offset + length);
        if high > low + 1e-9 && *length > 1e-9 {
            let t0 = seg.inv_arclen(low - offset, 1e-5);
            let t1 = seg.inv_arclen(high - offset, 1e-5);
            let piece = seg.subsegment(t0..t1);
            if previous.is_none_or(|point| point.distance(piece.start()) > 1e-9) {
                out.move_to(piece.start());
            }
            out.push(piece.as_path_el());
            previous = Some(piece.end());
        }
        offset += length;
    }
    Cow::Owned(out)
}

/// Build each end independently: an absent or degenerate start must not
/// shift the end symbol into the start's slot.
fn endpoint_head(path: &BezPath, stroke: &Stroke, start: bool) -> Option<BezPath> {
    let mut one_end = stroke.clone();
    one_end.share_arrowheads = false;
    if start {
        one_end.end_arrow = Arrowhead::None;
    } else {
        let (head, placement) = stroke.end_head_drawn();
        one_end.end_arrow = head.clone();
        one_end.end_head = *placement;
        one_end.start_arrow = Arrowhead::None;
    }
    arrowhead_paths(path, &one_end)
        .into_iter()
        .next()
        .filter(BezPath::is_finite)
}

/// The farthest contact in the endpoint's first passage through the head.
/// Only the head boundary is flattened; intersections split the original
/// curve into exact Bezier pieces before the winding test. Once the curve
/// leaves the symbol's bounds, a later loop through it is part of the body.
fn contact_distance(segments: impl Iterator<Item = (PathSeg, f64)>, head: &BezPath) -> Option<f64> {
    let mut edges = Vec::new();
    let mut first = None;
    let mut previous = None;
    kurbo::flatten(
        head.elements().iter().copied(),
        0.0025,
        &mut |el| match el {
            PathEl::MoveTo(point) => {
                first = Some(point);
                previous = Some(point);
            }
            PathEl::LineTo(point) => {
                if let Some(last) = previous {
                    edges.push(Line::new(last, point));
                }
                previous = Some(point);
            }
            PathEl::ClosePath => {
                if let (Some(last), Some(start)) = (previous, first) {
                    edges.push(Line::new(last, start));
                }
                previous = first;
            }
            _ => {}
        },
    );
    let mut segments = segments.peekable();
    let endpoint = segments.peek()?.0.start();
    let bounds = head.bounding_box().union_pt(endpoint).inflate(1e-7, 1e-7);
    let bounds_edges = [
        Line::new((bounds.x0, bounds.y0), (bounds.x1, bounds.y0)),
        Line::new((bounds.x1, bounds.y0), (bounds.x1, bounds.y1)),
        Line::new((bounds.x1, bounds.y1), (bounds.x0, bounds.y1)),
        Line::new((bounds.x0, bounds.y1), (bounds.x0, bounds.y0)),
    ];
    let mut last_contact = None;
    let mut offset = 0.0;
    for (seg, length) in segments {
        let mut cuts: Vec<_> = edges
            .iter()
            .chain(&bounds_edges)
            .flat_map(|edge| seg.intersect_line(*edge))
            .map(|hit| hit.segment_t)
            .filter(|t| t.is_finite() && *t > 1e-9 && *t < 1.0 - 1e-9)
            .collect();
        cuts.extend([0.0, 1.0]);
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        for pair in cuts.windows(2) {
            let middle = seg.eval((pair[0] + pair[1]) * 0.5);
            if !bounds.contains(middle) {
                return last_contact;
            }
            // A rotated head can have its contour exactly on the line.
            // A boundary contact still joins the symbol and line body.
            let on_boundary = edges
                .iter()
                .any(|edge| edge.nearest(middle, 1e-9).distance_sq <= 1e-14);
            if head.winding(middle) == 0 && !on_boundary {
                continue;
            }
            let end = offset + seg.subsegment(0.0..pair[1]).arclen(1e-5);
            last_contact = Some(end);
        }
        offset += length;
    }
    last_contact
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Affine, Rect};
    use crate::style::ArrowheadPlacement;

    fn line(length: f64) -> BezPath {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((length, 0.0));
        path
    }

    #[test]
    fn every_preset_shortens_the_body_at_both_ends() {
        let path = line(100.0);
        for head in Arrowhead::ALL.iter().skip(1) {
            for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
                let stroke = Stroke {
                    width: 2.0,
                    cap,
                    start_arrow: head.clone(),
                    end_arrow: head.clone(),
                    ..Default::default()
                };
                let body = arrowhead_shaft_path(&path, &stroke, 1.0);
                let bounds = body.bounding_box();
                assert!(
                    bounds.x0 > 0.0 && bounds.x1 < 100.0,
                    "{head:?} {cap:?}: {bounds:?}"
                );
                assert!(body.is_finite());
            }
        }
    }

    #[test]
    fn hollow_heads_stop_the_body_at_the_back_rim_and_caps_finish_there() {
        let path = line(100.0);
        for (head, back) in [
            (Arrowhead::CircleOpen, 100.0 - 6.4),
            (Arrowhead::SquareOpen, 100.0 - 6.4),
            (Arrowhead::DiamondOpen, 100.0 - 8.8),
            (Arrowhead::TriangleOpen, 100.0 - 8.0),
        ] {
            for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
                let stroke = Stroke {
                    width: 2.0,
                    end_arrow: head.clone(),
                    cap,
                    ..Default::default()
                };
                let body = arrowhead_shaft_path(&path, &stroke, 1.0);
                let cap_reach = if cap == LineCap::Butt { 0.0 } else { 1.0 };
                assert!(
                    (body.bounding_box().x1 + cap_reach - back).abs() < 0.01,
                    "{head:?} {cap:?}"
                );
            }
        }
    }

    #[test]
    fn notched_heads_keep_the_connection_and_short_lines_can_have_no_body() {
        let path = line(100.0);
        let stroke = Stroke {
            width: 2.0,
            end_arrow: Arrowhead::Arrow,
            ..Default::default()
        };
        assert!((arrowhead_shaft_path(&path, &stroke, 1.0).bounding_box().x1 - 94.4).abs() < 1e-6);
        for head in [
            Arrowhead::Triangle,
            Arrowhead::Circle,
            Arrowhead::CircleOpen,
        ] {
            let stroke = Stroke {
                start_arrow: head.clone(),
                end_arrow: head.clone(),
                ..stroke.clone()
            };
            assert!(
                arrowhead_shaft_path(&line(0.01), &stroke, 1.0)
                    .elements()
                    .is_empty(),
                "{head:?}"
            );
        }
    }

    #[test]
    fn curve_pieces_and_separate_subpaths_survive_shortening() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.curve_to((10.0, 10.0), (20.0, 0.0), (30.0, 0.0));
        path.move_to((70.0, 0.0));
        path.quad_to((80.0, 0.0), (100.0, 0.0));
        let stroke = Stroke {
            start_arrow: Arrowhead::Triangle,
            end_arrow: Arrowhead::CircleOpen,
            ..Default::default()
        };
        let body = arrowhead_shaft_path(&path, &stroke, 1.0);
        assert_eq!(
            body.elements()
                .iter()
                .filter(|el| matches!(el, PathEl::MoveTo(_)))
                .count(),
            2
        );
        assert!(body
            .elements()
            .iter()
            .any(|el| matches!(el, PathEl::CurveTo(..))));
        assert!(body
            .elements()
            .iter()
            .any(|el| matches!(el, PathEl::QuadTo(..))));
        assert!(body.bounding_box().x0 > 0.0 && body.bounding_box().x1 < 100.0);

        // Touching subpaths still have separate caps and dash origins.
        let mut touching = line(30.0);
        touching.move_to((30.0, 0.0));
        touching.line_to((100.0, 0.0));
        let body = arrowhead_shaft_path(&touching, &stroke, 1.0);
        assert_eq!(
            body.elements()
                .iter()
                .filter(|el| matches!(el, PathEl::MoveTo(_)))
                .count(),
            2
        );
    }

    #[test]
    fn custom_head_placement_sets_the_contact_in_page_space() {
        let path = line(100.0);
        let custom = Arrowhead::Custom {
            path: Rect::new(-1.0, -0.5, 0.0, 0.5).to_path(0.01),
            name: "Block".into(),
        };
        let stroke = Stroke {
            width: 2.0,
            end_arrow: custom,
            ..Default::default()
        };
        let base = arrowhead_shaft_path(&path, &stroke, 1.0).bounding_box().x1;
        assert!((base - 92.0).abs() < 1e-6);
        let mut placed = stroke.clone();
        placed.end_head = ArrowheadPlacement {
            size: 200.0,
            offset: -3.0,
            ..Default::default()
        };
        assert!((arrowhead_shaft_path(&path, &placed, 1.0).bounding_box().x1 - 81.0).abs() < 1e-6);
        placed.end_head = ArrowheadPlacement {
            rotation: 90.0,
            ..Default::default()
        };
        assert!((arrowhead_shaft_path(&path, &placed, 1.0).bounding_box().x1 - 96.0).abs() < 0.01);
        placed.end_head = ArrowheadPlacement {
            mirror_h: true,
            offset: -5.0,
            ..Default::default()
        };
        assert!((arrowhead_shaft_path(&path, &placed, 1.0).bounding_box().x1 - 95.0).abs() < 1e-6);
        placed.end_head = ArrowheadPlacement {
            offset: 20.0,
            ..Default::default()
        };
        assert!(matches!(
            arrowhead_shaft_path(&path, &placed, 1.0),
            Cow::Borrowed(_)
        ));
        let moved = Affine::translate((10.0, 20.0)) * path;
        let body = arrowhead_shaft_path(&moved, &stroke, 1.0);
        assert!((body.bounding_box().x1 - 102.0).abs() < 1e-6);
    }

    #[test]
    fn scaled_caps_stop_at_the_same_contact() {
        let path = line(100.0);
        for cap in [LineCap::Round, LineCap::Square] {
            let stroke = Stroke {
                width: 2.0,
                cap,
                end_arrow: Arrowhead::Triangle,
                ..Default::default()
            };
            let body = arrowhead_shaft_path(&path, &stroke, 3.0);
            assert!((body.bounding_box().x1 + 3.0 - 92.8).abs() < 1e-6);
        }
    }

    #[test]
    fn short_endpoint_subpaths_do_not_trim_their_neighbours() {
        let mut path = line(0.25);
        path.move_to((20.0, 0.0));
        path.line_to((30.0, 0.0));
        path.move_to((50.0, 0.0));
        path.line_to((50.25, 0.0));
        let stroke = Stroke {
            width: 2.0,
            cap: LineCap::Square,
            start_arrow: Arrowhead::Triangle,
            end_arrow: Arrowhead::Triangle,
            ..Default::default()
        };
        let body = arrowhead_shaft_path(&path, &stroke, 1.0);
        assert_eq!(
            body.elements(),
            &[
                PathEl::MoveTo((20.0, 0.0).into()),
                PathEl::LineTo((30.0, 0.0).into())
            ]
        );
    }

    #[test]
    fn a_degenerate_start_does_not_consume_the_end_head() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((0.0, 0.0));
        path.line_to((100.0, 0.0));
        let stroke = Stroke {
            width: 2.0,
            start_arrow: Arrowhead::Triangle,
            end_arrow: Arrowhead::Triangle,
            ..Default::default()
        };
        let body = arrowhead_shaft_path(&path, &stroke, 1.0);
        assert!(body.bounding_box().x0.abs() < 1e-6);
        assert!((body.bounding_box().x1 - 92.8).abs() < 1e-6);
    }

    #[test]
    fn a_loop_back_into_the_head_keeps_the_distant_body() {
        let mut path = line(20.0);
        path.line_to((20.0, 20.0));
        path.line_to((2.0, 20.0));
        path.line_to((2.0, 0.0));
        path.line_to((50.0, 0.0));
        let stroke = Stroke {
            width: 2.0,
            start_arrow: Arrowhead::Triangle,
            ..Default::default()
        };
        let body = arrowhead_shaft_path(&path, &stroke, 1.0);
        assert!((body.bounding_box().y1 - 20.0).abs() < 1e-6);
        assert_eq!(
            body.elements()
                .iter()
                .filter(|el| matches!(el, PathEl::LineTo(_)))
                .count(),
            5
        );
        assert!((body.segments().next().expect("first segment").start().x - 7.2).abs() < 1e-6);
    }

    #[test]
    fn unchanged_closed_hairline_and_invalid_paths_remain_safe() {
        let path = line(100.0);
        assert!(matches!(
            arrowhead_shaft_path(&path, &Stroke::default(), 1.0),
            Cow::Borrowed(_)
        ));
        let stroke = Stroke {
            width: Stroke::HAIRLINE,
            end_arrow: Arrowhead::Arrow,
            ..Default::default()
        };
        assert!(arrowhead_shaft_path(&path, &stroke, 1.0).bounding_box().x1 < 100.0);
        let closed = Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.01);
        assert!(matches!(
            arrowhead_shaft_path(&closed, &stroke, 1.0),
            Cow::Borrowed(_)
        ));
        let invalid = line(f64::NAN);
        assert!(matches!(
            arrowhead_shaft_path(&invalid, &stroke, 1.0),
            Cow::Borrowed(_)
        ));
        let invalid_width = Stroke {
            width: f64::INFINITY,
            ..stroke
        };
        assert!(matches!(
            arrowhead_shaft_path(&path, &invalid_width, 1.0),
            Cow::Borrowed(_)
        ));
    }
}
