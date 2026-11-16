use super::{horizontal, vertical, App, Tokens, BAR};
use crate::theme::{self, Theme};
use egui::{epaint::RectShape, Event, Modifiers, PointerButton, Pos2, Rect, Vec2};

fn bar(upright: bool) -> Rect {
    if upright {
        Rect::from_min_size(Pos2::new(610.0, 30.0), Vec2::new(BAR, 400.0))
    } else {
        Rect::from_min_size(Pos2::new(100.0, 450.0), Vec2::new(500.0, BAR))
    }
}

fn frame(ctx: &egui::Context, app: &mut App, upright: bool, events: Vec<Event>) -> RectShape {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        let canvas = Rect::from_min_max(Pos2::new(100.0, 30.0), Pos2::new(600.0, 430.0));
        if upright {
            vertical(app, ui, bar(true), canvas);
        } else {
            horizontal(app, ui, bar(false), canvas);
        }
    });
    out.textures_delta.clear();
    out.shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.rect.width().min(rect.rect.height()) <= 6.0
                    && rect.corner_radius.nw >= 2 =>
            {
                Some(rect.clone())
            }
            _ => None,
        })
        .expect("the painted scrollbar thumb")
}

fn contrast(a: egui::Color32, b: egui::Color32) -> f32 {
    let luminance = |color| {
        let color = egui::Rgba::from(color);
        0.2126 * color.r() + 0.7152 * color.g() + 0.0722 * color.b()
    };
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn setup() -> (egui::Context, App) {
    let ctx = egui::Context::default();
    let mut app = App::headless();
    let page = app.page_rect();
    app.view.fit(
        page,
        Rect::from_min_max(Pos2::new(100.0, 30.0), Pos2::new(600.0, 430.0)),
    );
    (ctx, app)
}

#[test]
fn canvas_scrollbars_paint_slim_rounded_thumbs_in_each_theme() {
    for theme in Theme::ALL {
        theme::set_theme(theme);
        for upright in [false, true] {
            let (ctx, mut app) = setup();
            frame(&ctx, &mut app, upright, Vec::new());
            let thumb = frame(&ctx, &mut app, upright, Vec::new());
            let thickness = if upright {
                thumb.rect.width()
            } else {
                thumb.rect.height()
            };
            assert!((3.0..=4.5).contains(&thickness), "{theme:?}: {thumb:?}");
            let radius = thumb.corner_radius;
            assert!(radius.nw >= 2, "{theme:?}: {radius:?}");
            assert_eq!(radius, egui::CornerRadius::same(radius.nw));
            let idle_contrast = contrast(thumb.fill, Tokens::panel());
            assert!(idle_contrast > 1.0);
            if theme == Theme::HighContrast {
                assert_eq!(thumb.fill, Tokens::slider_rail());
            } else {
                assert!(idle_contrast < contrast(Tokens::text_dim(), Tokens::panel()));
            }
            let id = if upright { "vscroll" } else { "hscroll" };
            let target = ctx.read_response(egui::Id::new(id)).expect("drag target");
            let target_thickness = if upright {
                target.rect.width()
            } else {
                target.rect.height()
            };
            assert_eq!(target_thickness, BAR, "the full drag target stays");
            assert!(target_thickness > thickness * 2.0);
        }
    }
    theme::set_theme(Theme::Light);
}

#[test]
fn slim_scrollbars_keep_hover_and_drag_targets_beside_the_painted_thumb() {
    for theme in Theme::ALL {
        theme::set_theme(theme);
        for upright in [false, true] {
            let (ctx, mut app) = setup();
            frame(&ctx, &mut app, upright, Vec::new());
            let thumb = frame(&ctx, &mut app, upright, Vec::new());
            let beside = thumb.rect.center()
                + if upright {
                    Vec2::new(4.5, 0.0)
                } else {
                    Vec2::new(0.0, 4.5)
                };
            assert!(!thumb.rect.contains(beside), "outside the painted line");
            let before = app.view;
            let hot = frame(&ctx, &mut app, upright, vec![Event::PointerMoved(beside)]);
            let idle_width = thumb.rect.width().min(thumb.rect.height());
            let hot_width = hot.rect.width().min(hot.rect.height());
            assert!(hot_width > idle_width && hot_width <= 5.5);
            if theme == Theme::HighContrast {
                assert_eq!(thumb.fill, Tokens::slider_rail());
                assert_eq!(hot.fill, Tokens::control_border());
            } else {
                assert_ne!(hot.fill, thumb.fill, "{theme:?}: hover beside the line");
                let idle_contrast = contrast(thumb.fill, Tokens::panel());
                let hot_contrast = contrast(hot.fill, Tokens::panel());
                assert!(hot_contrast > idle_contrast, "{theme:?}: clearer on hover");
                assert!(hot_contrast < contrast(Tokens::text(), Tokens::panel()));
            }
            assert_eq!(app.view.origin, before.origin, "hover does not pan");
            assert_eq!(app.view.zoom, before.zoom);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            frame(&ctx, &mut app, upright, vec![button(beside, true)]);
            let to = beside
                + if upright {
                    Vec2::new(0.0, 30.0)
                } else {
                    Vec2::new(30.0, 0.0)
                };
            frame(&ctx, &mut app, upright, vec![Event::PointerMoved(to)]);
            frame(&ctx, &mut app, upright, vec![button(to, false)]);
            if upright {
                assert!(app.view.origin.y < before.origin.y, "vertical drag pans");
                assert_eq!(app.view.origin.x, before.origin.x);
            } else {
                assert!(app.view.origin.x < before.origin.x, "horizontal drag pans");
                assert_eq!(app.view.origin.y, before.origin.y);
            }
            assert_eq!(app.view.zoom, before.zoom, "dragging does not zoom");
        }
    }
    theme::set_theme(Theme::Light);
}
