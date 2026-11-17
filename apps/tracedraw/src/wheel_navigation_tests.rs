//! Canvas wheel modifiers keep the same axes with either wheel action.

use super::{frame, setup, wheel_notch};
use crate::app::App;
use egui::{Context, Event, Modifiers, MouseWheelUnit, Pos2, TouchPhase, Vec2};

#[test]
fn pinch_zoom_is_not_swallowed_by_a_brush_wheel_tail() {
    for wheel_zooms in [false, true] {
        let modifiers = if wheel_zooms {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::ALT
        };
        let (ctx, mut app, at) = wheel_setup(wheel_zooms, false);
        app.set_tool(crate::tools::Tool::Twirl);
        app.brush_radius = 10.0;
        frame(
            &ctx,
            &mut app,
            vec![wheel(MouseWheelUnit::Line, egui::vec2(0.0, 1.0), modifiers)],
            modifiers,
        );
        let radius = app.brush_radius;
        assert!(radius > 10.0, "the wheel sizes the brush first");
        let before = app.view;
        let spot = app.view.to_page(at);
        let revision = app.engine.revision();
        frame(&ctx, &mut app, vec![Event::Zoom(1.25)], Modifiers::NONE);
        assert!(0.0001 > (app.view.zoom / before.zoom - 1.25).abs());
        assert_eq!(
            app.brush_radius, radius,
            "pinch leaves the brush size alone"
        );
        let after = app.view.to_page(at);
        assert!(0.001 > (after.x - spot.x).abs());
        assert!(0.001 > (after.y - spot.y).abs());
        assert_eq!(app.engine.revision(), revision);
    }
}

fn wheel_setup(wheel_zooms: bool, centers: bool) -> (Context, App, Pos2) {
    let (ctx, mut app, _) = setup();
    app.settings.wheel_zooms = wheel_zooms;
    app.settings.wheel_zoom_centers = centers;
    let at = app.canvas_rect.center() - egui::vec2(113.0, 71.0);
    for _ in 0..2 {
        frame(
            &ctx,
            &mut app,
            vec![Event::PointerMoved(at)],
            Modifiers::NONE,
        );
    }
    (ctx, app, at)
}

fn wheel(unit: MouseWheelUnit, delta: Vec2, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit,
        delta,
        phase: TouchPhase::Move,
        modifiers,
    }
}

fn assert_pan_axis(delta: Vec2, horizontal: bool) {
    let (along, across) = if horizontal {
        (delta.x, delta.y)
    } else {
        (delta.y, delta.x)
    };
    assert!(along.abs() > 0.001, "the requested axis moves: {delta:?}");
    assert!(
        0.001 > across.abs(),
        "only the requested axis moves: {delta:?}"
    );
}

#[test]
fn shift_and_alt_wheel_pan_their_axes_with_either_mouse_wheel_action() {
    for wheel_zooms in [false, true] {
        for centers in [false, true] {
            for (modifiers, horizontal) in [(Modifiers::SHIFT, true), (Modifiers::ALT, false)] {
                for unit in [MouseWheelUnit::Line, MouseWheelUnit::Point] {
                    // Some platforms send Shift+wheel on the X axis already.
                    for delta in [egui::vec2(0.0, 1.0), egui::vec2(1.0, 0.0)] {
                        let (ctx, mut app, _) = wheel_setup(wheel_zooms, centers);
                        let before = app.view;
                        let revision = app.engine.revision();
                        frame(
                            &ctx,
                            &mut app,
                            vec![wheel(unit, delta, modifiers)],
                            modifiers,
                        );
                        for _ in 0..3 {
                            frame(&ctx, &mut app, Vec::new(), modifiers);
                        }
                        assert_pan_axis(app.view.origin - before.origin, horizontal);
                        assert_eq!(app.view.zoom, before.zoom, "panning never zooms");
                        assert_eq!(
                            app.engine.revision(),
                            revision,
                            "the document stays unchanged"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn wheel_pan_finishes_on_the_same_axis_after_the_modifier_is_released() {
    for wheel_zooms in [false, true] {
        for (modifiers, horizontal) in [(Modifiers::SHIFT, true), (Modifiers::ALT, false)] {
            let (ctx, mut app, _) = wheel_setup(wheel_zooms, true);
            let before = app.view;
            frame(
                &ctx,
                &mut app,
                vec![wheel(MouseWheelUnit::Line, egui::vec2(0.0, 1.0), modifiers)],
                modifiers,
            );
            let first = app.view.origin;
            assert_pan_axis(first - before.origin, horizontal);
            for _ in 0..20 {
                frame(&ctx, &mut app, Vec::new(), Modifiers::NONE);
            }
            assert_pan_axis(app.view.origin - first, horizontal);
            assert_eq!(
                app.view.zoom, before.zoom,
                "the remaining scroll never zooms"
            );
        }
    }
}

#[test]
fn plain_wheel_keeps_the_configured_zoom_or_scroll_action() {
    for wheel_zooms in [false, true] {
        let (ctx, mut app, at) = wheel_setup(wheel_zooms, false);
        let before = app.view;
        let spot = app.view.to_page(at);
        wheel_notch(&ctx, &mut app, at, Modifiers::NONE);
        if wheel_zooms {
            assert!(0.01 > (app.view.zoom / before.zoom - 1.15).abs());
            let after = app.view.to_page(at);
            assert!(0.001 > (after.x - spot.x).abs());
            assert!(0.001 > (after.y - spot.y).abs());
        } else {
            assert_pan_axis(app.view.origin - before.origin, false);
            assert_eq!(app.view.zoom, before.zoom);
        }
    }
}

#[test]
fn control_wheel_and_pinch_keep_zooming_with_either_mouse_wheel_action() {
    for wheel_zooms in [false, true] {
        for modifiers in [
            Modifiers::CTRL,
            Modifiers::CTRL | Modifiers::COMMAND,
            Modifiers::COMMAND,
        ] {
            let (ctx, mut app, at) = wheel_setup(wheel_zooms, false);
            let before = app.view;
            let spot = app.view.to_page(at);
            wheel_notch(&ctx, &mut app, at, modifiers);
            assert!(app.view.zoom > before.zoom, "Ctrl/Cmd keeps zooming");
            let after = app.view.to_page(at);
            assert!(
                0.001 > (after.x - spot.x).abs(),
                "zoom stays about the pointer"
            );
            assert!(
                0.001 > (after.y - spot.y).abs(),
                "zoom stays about the pointer"
            );
        }
        let (ctx, mut app, at) = wheel_setup(wheel_zooms, false);
        let before = app.view;
        let spot = app.view.to_page(at);
        frame(&ctx, &mut app, vec![Event::Zoom(1.25)], Modifiers::NONE);
        assert!(0.0001 > (app.view.zoom / before.zoom - 1.25).abs());
        let after = app.view.to_page(at);
        assert!(0.001 > (after.x - spot.x).abs());
        assert!(0.001 > (after.y - spot.y).abs());
    }
}
