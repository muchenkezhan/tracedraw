//! Closed, filled outlines for the built-in endpoint symbols.

use super::Arrowhead;
use crate::geometry::{Affine, BezPath, Circle, Shape as _};

fn polygon(points: &[(f64, f64)], size: f64) -> BezPath {
    let mut path = BezPath::new();
    let mut points = points.iter();
    if let Some(&(x, y)) = points.next() {
        path.move_to((x * size, y * size));
        for &(x, y) in points {
            path.line_to((x * size, y * size));
        }
        path.close_path();
    }
    path
}

fn append(path: &mut BezPath, part: BezPath) {
    path.extend(part.elements().iter().copied());
}

fn chevron(tip: f64, length: f64, height: f64, thickness: f64, size: f64) -> BezPath {
    polygon(
        &[
            (tip, 0.0),
            (tip - length, height),
            (tip - length - thickness, height - thickness),
            (tip - thickness * 1.4, 0.0),
            (tip - length - thickness, -height + thickness),
            (tip - length, -height),
        ],
        size,
    )
}

fn tail(tip: f64, size: f64) -> BezPath {
    polygon(
        &[
            (tip, 0.5),
            (tip - 0.6, 0.0),
            (tip, -0.5),
            (tip - 0.2, -0.5),
            (tip - 0.8, 0.0),
            (tip - 0.2, 0.5),
        ],
        size,
    )
}

pub(super) fn outline(kind: &Arrowhead, size: f64, w: f64) -> Option<BezPath> {
    let mut p = BezPath::new();
    match kind {
        Arrowhead::None => return None,
        Arrowhead::Custom { path: outline, .. } => {
            p = Affine::scale(size) * outline.clone();
        }
        // Keep the original presets' geometry and sizing unchanged.
        Arrowhead::Arrow => {
            p.move_to((0.0, 0.0));
            p.line_to((-size, size * 0.4));
            p.line_to((-size * 0.7, 0.0));
            p.line_to((-size, -size * 0.4));
            p.close_path();
        }
        Arrowhead::OpenArrow => {
            let t = w.max(0.2);
            p.move_to((0.0, 0.0));
            p.line_to((-size, size * 0.45));
            p.line_to((-size - t, size * 0.45 - t));
            p.line_to((-t * 1.4, 0.0));
            p.line_to((-size - t, -size * 0.45 + t));
            p.line_to((-size, -size * 0.45));
            p.close_path();
        }
        Arrowhead::Circle => {
            let r = size * 0.35;
            p = Circle::new((-r, 0.0), r).to_path(0.01);
        }
        Arrowhead::Square => {
            let h = size * 0.35;
            let cx = -h;
            p.move_to((cx + h, h));
            p.line_to((cx - h, h));
            p.line_to((cx - h, -h));
            p.line_to((cx + h, -h));
            p.close_path();
        }
        Arrowhead::Bar => {
            let t = w.max(0.2);
            p.move_to((0.0, size * 0.5));
            p.line_to((-t, size * 0.5));
            p.line_to((-t, -size * 0.5));
            p.line_to((0.0, -size * 0.5));
            p.close_path();
        }
        Arrowhead::Diamond => {
            let h = size * 0.5;
            p.move_to((0.0, 0.0));
            p.line_to((-h, h * 0.6));
            p.line_to((-h * 2.0, 0.0));
            p.line_to((-h, -h * 0.6));
            p.close_path();
        }
        Arrowhead::Triangle => {
            p = polygon(&[(0.0, 0.0), (-0.9, 0.5), (-0.9, -0.5)], size);
        }
        Arrowhead::NarrowTriangle => {
            p = polygon(&[(0.0, 0.0), (-1.4, 0.325), (-1.4, -0.325)], size);
        }
        Arrowhead::WideTriangle => {
            p = polygon(&[(0.0, 0.0), (-0.65, 0.55), (-0.65, -0.55)], size);
        }
        Arrowhead::Stealth => {
            p = polygon(
                &[(0.0, 0.0), (-0.9, 0.55), (-0.4, 0.0), (-0.9, -0.55)],
                size,
            );
        }
        Arrowhead::LongStealth => {
            p = polygon(
                &[(0.0, 0.0), (-1.45, 0.4), (-1.1, 0.0), (-1.45, -0.4)],
                size,
            );
        }
        Arrowhead::DoubleArrow => {
            for tip in [0.0, -0.8] {
                append(
                    &mut p,
                    polygon(&[(tip, 0.0), (tip - 0.65, 0.45), (tip - 0.65, -0.45)], size),
                );
            }
        }
        Arrowhead::TripleArrow => {
            for tip in [0.0, -0.55, -1.1] {
                append(
                    &mut p,
                    polygon(&[(tip, 0.0), (tip - 0.42, 0.4), (tip - 0.42, -0.4)], size),
                );
            }
        }
        Arrowhead::Chevron => {
            p = chevron(0.0, 0.7, 0.5, 0.18, size);
        }
        Arrowhead::DoubleChevron => {
            for tip in [0.0, -0.75] {
                append(&mut p, chevron(tip, 0.55, 0.45, 0.15, size));
            }
        }
        Arrowhead::HalfArrowLeft | Arrowhead::HalfArrowRight => {
            // Positive y is left when looking along the outward direction.
            p = polygon(
                &[
                    (0.0, 0.0),
                    (-1.0, 0.55),
                    (-0.65, 0.06),
                    (-1.0, 0.06),
                    (-1.0, -0.06),
                    (-0.12, -0.06),
                ],
                size,
            );
            if matches!(kind, Arrowhead::HalfArrowRight) {
                p = Affine::scale_non_uniform(1.0, -1.0) * p;
            }
        }
        Arrowhead::Harpoon => {
            p.move_to((0.0, 0.0));
            p.line_to((-0.9, 0.48));
            p.line_to((-0.6, 0.08));
            p.curve_to((-1.05, 0.12), (-1.08, -0.45), (-0.65, -0.5));
            p.line_to((-0.65, -0.32));
            p.curve_to((-0.83, -0.28), (-0.82, -0.08), (-0.56, -0.09));
            p.line_to((-0.65, -0.25));
            p.close_path();
            p = Affine::scale(size) * p;
        }
        Arrowhead::ReverseArrow => {
            p = polygon(&[(0.0, 0.45), (-1.1, 0.0), (0.0, -0.45), (-0.3, 0.0)], size);
        }
        Arrowhead::Tail => p = tail(0.0, size),
        Arrowhead::DoubleTail => {
            p = tail(0.0, size);
            append(&mut p, tail(-0.65, size));
        }
        Arrowhead::CircleOpen => {
            p = Circle::new((-size * 0.4, 0.0), size * 0.4).to_path(0.01);
            let inner = Circle::new((-size * 0.4, 0.0), size * 0.24).to_path(0.01);
            append(&mut p, inner.reverse_subpaths());
        }
        Arrowhead::SquareOpen => {
            p = polygon(&[(0.0, 0.4), (-0.8, 0.4), (-0.8, -0.4), (0.0, -0.4)], size);
            let inner = polygon(
                &[(-0.16, 0.24), (-0.64, 0.24), (-0.64, -0.24), (-0.16, -0.24)],
                size,
            );
            append(&mut p, inner.reverse_subpaths());
        }
        Arrowhead::DiamondOpen => {
            p = polygon(
                &[(0.0, 0.0), (-0.55, 0.45), (-1.1, 0.0), (-0.55, -0.45)],
                size,
            );
            let inner = polygon(
                &[(-0.21, 0.0), (-0.55, 0.27), (-0.89, 0.0), (-0.55, -0.27)],
                size,
            );
            append(&mut p, inner.reverse_subpaths());
        }
        Arrowhead::TriangleOpen => {
            p = polygon(&[(0.0, 0.0), (-1.0, 0.55), (-1.0, -0.55)], size);
            let inner = polygon(&[(-0.3, 0.0), (-0.83, 0.3), (-0.83, -0.3)], size);
            append(&mut p, inner.reverse_subpaths());
        }
        Arrowhead::BarDouble => {
            let t = (w / size).clamp(0.12, 0.25);
            for x in [0.0, -0.48] {
                append(
                    &mut p,
                    polygon(&[(x, 0.5), (x - t, 0.5), (x - t, -0.5), (x, -0.5)], size),
                );
            }
        }
        Arrowhead::Slash => {
            p = polygon(
                &[(0.0, 0.55), (-0.15, 0.55), (-0.6, -0.55), (-0.45, -0.55)],
                size,
            );
        }
        Arrowhead::Cross => {
            p = polygon(
                &[
                    (0.0, 0.3),
                    (-0.3, 0.0),
                    (0.0, -0.3),
                    (-0.1, -0.4),
                    (-0.4, -0.1),
                    (-0.7, -0.4),
                    (-0.8, -0.3),
                    (-0.5, 0.0),
                    (-0.8, 0.3),
                    (-0.7, 0.4),
                    (-0.4, 0.1),
                    (-0.1, 0.4),
                ],
                size,
            );
        }
        Arrowhead::Star => {
            for i in 0..10 {
                let angle = i as f64 * std::f64::consts::PI / 5.0;
                let radius = if i % 2 == 0 { 0.5 } else { 0.22 };
                let point = (
                    (-0.5 + radius * angle.cos()) * size,
                    radius * angle.sin() * size,
                );
                if i == 0 {
                    p.move_to(point);
                } else {
                    p.line_to(point);
                }
            }
            p.close_path();
        }
        Arrowhead::Hexagon => {
            p = polygon(
                &[
                    (0.0, 0.0),
                    (-0.25, 0.45),
                    (-0.75, 0.45),
                    (-1.0, 0.0),
                    (-0.75, -0.45),
                    (-0.25, -0.45),
                ],
                size,
            );
        }
        Arrowhead::Bracket => {
            p = polygon(
                &[
                    (-0.45, 0.5),
                    (0.0, 0.5),
                    (0.0, 0.34),
                    (-0.28, 0.34),
                    (-0.28, -0.34),
                    (0.0, -0.34),
                    (0.0, -0.5),
                    (-0.45, -0.5),
                ],
                size,
            );
        }
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{PathEl, Point};
    use crate::style::{arrowhead_paths, head_size_for_width, Stroke};
    use crate::Color;

    #[test]
    fn every_visible_preset_is_closed_finite_and_has_area() {
        for width in [Stroke::HAIRLINE, 0.1, 1.0, 20.0] {
            let (size, w) = head_size_for_width(width);
            for head in Arrowhead::ALL.iter().skip(1) {
                let path = outline(head, size, w).unwrap();
                let mut open = false;
                for element in path.elements() {
                    match element {
                        PathEl::MoveTo(point) => {
                            assert!(!open, "{head:?} has an unclosed contour");
                            assert!(point.is_finite(), "{head:?}: {point:?}");
                            open = true;
                        }
                        PathEl::LineTo(point) => assert!(point.is_finite(), "{head:?}: {point:?}"),
                        PathEl::QuadTo(a, b) => assert!(a.is_finite() && b.is_finite(), "{head:?}"),
                        PathEl::CurveTo(a, b, c) => {
                            assert!(a.is_finite() && b.is_finite() && c.is_finite(), "{head:?}");
                        }
                        PathEl::ClosePath => open = false,
                    }
                }
                assert!(!open, "{head:?} has an unclosed contour");
                let bounds = path.bounding_box();
                assert!(
                    bounds.width() > 0.0 && bounds.height() > 0.0,
                    "{head:?}: {bounds:?}"
                );
                assert!(
                    bounds.x1.abs() < size * 1e-6,
                    "{head:?} extends past its endpoint: {bounds:?}"
                );
                assert!(path.area().abs() > size * size * 1e-5, "{head:?}");
            }
        }
    }

    #[test]
    fn preset_silhouettes_are_distinct() {
        let samples: Vec<_> = Arrowhead::ALL
            .iter()
            .skip(1)
            .map(|head| {
                let path = outline(head, 1.0, 0.25).unwrap();
                let mut mask = Vec::new();
                for y in -48..=48 {
                    for x in -96..=0 {
                        mask.push(path.winding(Point::new(x as f64 / 48.0, y as f64 / 48.0)) != 0);
                    }
                }
                (head, mask)
            })
            .collect();
        for (i, (head, mask)) in samples.iter().enumerate() {
            for (other, other_mask) in samples.iter().skip(i + 1) {
                assert_ne!(mask, other_mask, "{head:?} duplicates {other:?}");
            }
        }
    }

    #[test]
    fn open_presets_keep_their_holes_under_nonzero_fill() {
        for (head, center, rim) in [
            (Arrowhead::CircleOpen, (-0.4, 0.0), (-0.4, 0.32)),
            (Arrowhead::SquareOpen, (-0.4, 0.0), (-0.4, 0.32)),
            (Arrowhead::DiamondOpen, (-0.55, 0.0), (-0.55, 0.36)),
            (Arrowhead::TriangleOpen, (-0.65, 0.0), (-0.93, 0.0)),
        ] {
            let path = outline(&head, 1.0, 0.25).unwrap();
            assert_eq!(path.winding(Point::from(center)), 0, "{head:?}");
            assert_ne!(path.winding(Point::from(rim)), 0, "{head:?}");
        }
    }

    #[test]
    fn half_arrows_face_the_named_side_and_reverse_at_the_start() {
        let left = outline(&Arrowhead::HalfArrowLeft, 1.0, 0.25).unwrap();
        let right = outline(&Arrowhead::HalfArrowRight, 1.0, 0.25).unwrap();
        assert!(left.bounding_box().y1 > -left.bounding_box().y0);
        assert!(right.bounding_box().y1 < -right.bounding_box().y0);
        assert_eq!(left, Affine::scale_non_uniform(1.0, -1.0) * right);
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((30.0, 0.0));
        for head in Arrowhead::ALL.iter().skip(1) {
            let end_stroke = Stroke {
                end_arrow: head.clone(),
                ..Stroke::new(Color::BLACK, 1.0)
            };
            let start_stroke = Stroke {
                start_arrow: head.clone(),
                ..Stroke::new(Color::BLACK, 1.0)
            };
            let end = arrowhead_paths(&line, &end_stroke);
            let start = arrowhead_paths(&line, &start_stroke);
            assert_eq!(end.len(), 1, "{head:?}");
            assert_eq!(start.len(), 1, "{head:?}");
            let expected = Affine::scale(-1.0) * Affine::translate((-30.0, 0.0)) * end[0].clone();
            assert_eq!(start[0].elements().len(), expected.elements().len());
            let points = |path: &BezPath| {
                let mut points = Vec::new();
                for el in path.elements() {
                    match *el {
                        PathEl::MoveTo(p) | PathEl::LineTo(p) => points.push(p),
                        PathEl::QuadTo(a, b) => points.extend([a, b]),
                        PathEl::CurveTo(a, b, c) => points.extend([a, b, c]),
                        PathEl::ClosePath => {}
                    }
                }
                points
            };
            for (actual, want) in points(&start[0]).iter().zip(points(&expected)) {
                assert!(
                    actual.distance(want) < 1e-9,
                    "{head:?}: {actual:?} != {want:?}"
                );
            }
        }
    }

    #[test]
    fn every_preset_round_trips_as_a_named_string_and_unknown_names_are_rejected() {
        let original = [
            "none",
            "arrow",
            "openarrow",
            "circle",
            "square",
            "bar",
            "diamond",
        ];
        for (head, name) in Arrowhead::ALL.iter().zip(original) {
            assert_eq!(serde_json::to_value(head).unwrap(), serde_json::json!(name));
        }
        for head in &Arrowhead::ALL {
            let json = serde_json::to_string(head).unwrap();
            assert!(
                serde_json::from_str::<String>(&json).is_ok(),
                "{head:?}: {json}"
            );
            assert_eq!(&serde_json::from_str::<Arrowhead>(&json).unwrap(), head);
        }
        for invalid in [
            r#""unknownarrow""#,
            r#"{"triangle":{}}"#,
            r#"{"custom":{"name":"broken"}}"#,
            "null",
            "42",
        ] {
            assert!(
                serde_json::from_str::<Arrowhead>(invalid).is_err(),
                "{invalid}"
            );
        }
    }
}
