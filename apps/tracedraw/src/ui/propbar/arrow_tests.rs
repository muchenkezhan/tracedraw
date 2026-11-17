use super::*;
use egui::{Context, Event, Id, Key, LayerId, Modifiers, Order};
use tracedraw_core::{Arrowhead, ArrowheadPlacement, ShapeId};

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn input(ctx: &Context, events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 600.0))),
        events,
        time: Some(ctx.cumulative_frame_nr() as f64 / 60.0),
        ..Default::default()
    }
}

fn picker_frame(
    ctx: &Context,
    value: &mut Arrowhead,
    custom: &[Arrowhead],
    events: Vec<Event>,
) -> Id {
    let mut button = Id::NULL;
    let mut out = ctx.run_ui(input(ctx, events), |ui| {
        crate::ui::keynav::frame(ui.ctx(), false);
        ui.horizontal(|ui| {
            button = ui.make_persistent_id("arrow_picker_test");
            if let Some(next) = arrow_picker(ui, "arrow_picker_test", value, true, true, custom) {
                *value = next;
            }
        });
        crate::ui::keynav::end_frame(ui.ctx());
    });
    out.textures_delta.clear();
    button
}

fn picker_run(
    ctx: &Context,
    value: &mut Arrowhead,
    custom: &[Arrowhead],
    events: Vec<Event>,
) -> Id {
    let mut events = Some(events);
    let mut button = Id::NULL;
    for _ in 0..4 {
        button = picker_frame(ctx, value, custom, events.take().unwrap_or_default());
    }
    button
}

/// Read the actual interactive tiles, excluding the popup's frame.
fn tiles(ctx: &Context, popup: Id) -> Vec<(Id, Rect)> {
    let mut tiles: Vec<_> = ctx.viewport(|v| {
        v.prev_pass
            .widgets
            .get_layer(LayerId::new(Order::Foreground, popup))
            .filter(|w| {
                w.enabled
                    && w.sense.senses_click()
                    && !w.sense.senses_drag()
                    && (w.rect.width() - 54.0).abs() < 1.0
                    && (w.rect.height() - 40.0).abs() < 1.0
            })
            .map(|w| (w.id, w.rect))
            .collect()
    });
    tiles.sort_by(|a, b| {
        a.1.top()
            .total_cmp(&b.1.top())
            .then_with(|| a.1.left().total_cmp(&b.1.left()))
    });
    tiles.dedup_by_key(|tile| tile.0);
    tiles
}

fn visible_tile(ctx: &Context, id: Id) -> Option<Rect> {
    ctx.viewport(|v| {
        let widget = v.prev_pass.widgets.get(id)?;
        widget
            .interact_rect
            .contains_rect(widget.rect)
            .then_some(widget.rect)
    })
}

#[test]
fn arrow_picker_uses_four_columns_and_keeps_custom_heads_in_the_grid() {
    let ctx = crate::theme::ui_context();
    let mut path = tracedraw_core::geometry::BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((-1.0, 0.5));
    path.line_to((-1.0, -0.5));
    path.close_path();
    let custom: Vec<_> = ["First custom head", "Second custom head"]
        .into_iter()
        .map(|name| Arrowhead::Custom {
            path: path.clone(),
            name: name.into(),
        })
        .collect();
    let mut value = Arrowhead::None;
    let button = picker_run(&ctx, &mut value, &custom, Vec::new());
    let popup = button.with("popup");
    egui::Popup::open_id(&ctx, popup);
    picker_run(&ctx, &mut value, &custom, Vec::new());
    let rows = tiles(&ctx, popup);
    assert_eq!(
        rows.len(),
        Arrowhead::ALL.len() + custom.len(),
        "all presets and custom heads are clickable"
    );
    for row in rows.chunks(4) {
        let top = row[0].1.top();
        assert!(row.iter().all(|(_, r)| (r.top() - top).abs() < 1.0));
        assert!(row
            .windows(2)
            .all(|pair| pair[0].1.right() < pair[1].1.left()));
    }
    assert!(rows[4].1.top() > rows[0].1.bottom());
    assert!(rows[8].1.top() > rows[4].1.bottom());
    let area = ctx.memory(|m| m.area_rect(popup)).expect("an open popup");
    let button_rect = ctx.read_response(button).expect("the picker button").rect;
    assert!(area.width() >= 220.0 && area.width() > button_rect.width() * 3.0);
    let target = rows.last().expect("a custom head").0;
    assert!(visible_tile(&ctx, rows[0].0).is_some());
    assert!(
        visible_tile(&ctx, target).is_none(),
        "the custom heads start below the scrolling viewport"
    );
    let over_grid = rows[0].1.center();
    for _ in 0..12 {
        if visible_tile(&ctx, target).is_some() {
            break;
        }
        picker_run(
            &ctx,
            &mut value,
            &custom,
            vec![
                Event::PointerMoved(over_grid),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -160.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    let at = visible_tile(&ctx, target)
        .expect("scrolling reaches the last custom head")
        .center();
    assert!(
        tiles(&ctx, popup)[0].1.top() < rows[0].1.top() - 40.0,
        "the grid actually scrolled"
    );
    picker_run(&ctx, &mut value, &custom, vec![Event::PointerMoved(at)]);
    for pressed in [true, false] {
        picker_frame(
            &ctx,
            &mut value,
            &custom,
            vec![Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    assert_eq!(value, custom[1], "the custom tile chooses its real head");
    assert!(!egui::Popup::is_id_open(&ctx, popup));
}

fn bar_frame(ctx: &Context, app: &mut App, events: Vec<Event>) -> [Id; 2] {
    let mut buttons = [Id::NULL; 2];
    let mut out = ctx.run_ui(input(ctx, events), |ui| {
        crate::ui::keynav::frame(ui.ctx(), false);
        ui.horizontal(|ui| {
            buttons = [
                ui.make_persistent_id("arrow_start_bar"),
                ui.make_persistent_id("arrow_end_bar"),
            ];
            outline_part(app, ui);
        });
        crate::ui::keynav::end_frame(ui.ctx());
    });
    out.textures_delta.clear();
    buttons
}

fn stroke_of(app: &App, id: ShapeId) -> tracedraw_core::Stroke {
    app.doc()
        .find_shape(id)
        .and_then(|shape| shape.stroke.clone())
        .expect("an outlined shape")
}

#[test]
fn arrow_picker_changes_only_its_end_on_each_selected_outline_and_undoes_once() {
    for start in [true, false] {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let mut originals = Vec::new();
        for index in 0..2 {
            let id = app
                .new_shape(ShapeKind::Rect {
                    rect: tracedraw_core::geometry::Rect::new(
                        10.0 + index as f64 * 60.0,
                        10.0,
                        50.0 + index as f64 * 60.0,
                        40.0,
                    ),
                    radius: 0.0,
                    corners: None,
                })
                .expect("a shape");
            let stroke = tracedraw_core::Stroke {
                width: 0.7 + index as f64,
                dash: vec![2.0 + index as f64, 1.0],
                start_arrow: if index == 0 {
                    Arrowhead::Circle
                } else {
                    Arrowhead::Bar
                },
                end_arrow: if index == 0 {
                    Arrowhead::Square
                } else {
                    Arrowhead::OpenArrow
                },
                start_head: ArrowheadPlacement {
                    size: 140.0,
                    offset: 0.5,
                    ..Default::default()
                },
                end_head: ArrowheadPlacement {
                    size: 75.0,
                    rotation: 15.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            app.run(Command::SetStroke {
                shapes: vec![id],
                stroke: Some(stroke.clone()),
            });
            originals.push((id, stroke));
        }
        app.select(originals.iter().map(|(id, _)| *id).collect());
        let mut buttons = [Id::NULL; 2];
        for _ in 0..4 {
            buttons = bar_frame(&ctx, &mut app, Vec::new());
        }
        let popup = buttons[usize::from(!start)].with("popup");
        egui::Popup::open_id(&ctx, popup);
        for _ in 0..4 {
            bar_frame(&ctx, &mut app, Vec::new());
        }
        let rows = tiles(&ctx, popup);
        assert_eq!(rows.len(), Arrowhead::ALL.len());
        let diamond = Arrowhead::ALL
            .iter()
            .position(|head| *head == Arrowhead::Diamond)
            .expect("the diamond preset");
        let at = rows[diamond].1.center();
        let depth = app.engine.history_labels().0.len();
        bar_frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        for pressed in [true, false] {
            bar_frame(
                &ctx,
                &mut app,
                vec![Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        assert!(!egui::Popup::is_id_open(&ctx, popup));
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        for (id, original) in &originals {
            let mut expected = original.clone();
            if start {
                expected.start_arrow = Arrowhead::Diamond;
            } else {
                expected.end_arrow = Arrowhead::Diamond;
            }
            assert_eq!(stroke_of(&app, *id), expected);
        }
        app.engine.undo().expect("one arrowhead undo");
        for (id, original) in &originals {
            assert_eq!(&stroke_of(&app, *id), original);
        }
    }
}

#[test]
fn arrow_picker_navigates_the_grid_and_picks_or_cancels_from_the_keyboard() {
    let ctx = crate::theme::ui_context();
    let mut value = Arrowhead::None;
    let button = picker_run(&ctx, &mut value, &[], Vec::new());
    let popup = button.with("popup");
    ctx.memory_mut(|m| m.request_focus(button));
    picker_run(&ctx, &mut value, &[], vec![key(Key::ArrowDown)]);
    assert!(egui::Popup::is_id_open(&ctx, popup));
    let rows = tiles(&ctx, popup);
    assert_eq!(rows.len(), Arrowhead::ALL.len());
    picker_run(&ctx, &mut value, &[], vec![key(Key::Home)]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(rows[0].0));
    picker_run(&ctx, &mut value, &[], vec![key(Key::ArrowRight)]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(rows[1].0));
    picker_run(&ctx, &mut value, &[], vec![key(Key::ArrowDown)]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(rows[5].0));
    picker_run(&ctx, &mut value, &[], vec![key(Key::ArrowUp)]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(rows[1].0));
    assert_eq!(value, Arrowhead::None, "navigation alone applies nothing");
    picker_run(&ctx, &mut value, &[], vec![key(Key::Enter)]);
    assert_eq!(value, Arrowhead::Arrow);
    assert!(!egui::Popup::is_id_open(&ctx, popup));

    ctx.memory_mut(|m| m.request_focus(button));
    picker_run(&ctx, &mut value, &[], vec![key(Key::ArrowDown)]);
    assert!(egui::Popup::is_id_open(&ctx, popup));
    picker_run(&ctx, &mut value, &[], vec![key(Key::Home)]);
    picker_run(&ctx, &mut value, &[], vec![key(Key::ArrowDown)]);
    picker_run(&ctx, &mut value, &[], vec![key(Key::Escape)]);
    assert_eq!(value, Arrowhead::Arrow, "Escape leaves the head unchanged");
    assert!(!egui::Popup::is_id_open(&ctx, popup));
    assert_eq!(ctx.memory(|m| m.focused()), Some(button));
}

#[test]
fn arrow_picker_contains_only_style_choices_for_both_ends() {
    for start in [true, false] {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let id = app
            .new_shape(ShapeKind::Rect {
                rect: tracedraw_core::geometry::Rect::new(10.0, 10.0, 50.0, 40.0),
                radius: 0.0,
                corners: None,
            })
            .expect("an outlined shape");
        let original = tracedraw_core::Stroke {
            width: 1.2,
            start_arrow: Arrowhead::Arrow,
            end_arrow: Arrowhead::Diamond,
            ..Default::default()
        };
        app.run(Command::SetStroke {
            shapes: vec![id],
            stroke: Some(original.clone()),
        });
        app.select(vec![id]);
        let mut buttons = [Id::NULL; 2];
        for _ in 0..4 {
            buttons = bar_frame(&ctx, &mut app, Vec::new());
        }
        let popup = buttons[usize::from(!start)].with("popup");
        egui::Popup::open_id(&ctx, popup);
        for _ in 0..4 {
            bar_frame(&ctx, &mut app, Vec::new());
        }
        let choices = tiles(&ctx, popup);
        assert_eq!(choices.len(), Arrowhead::ALL.len());
        let interactive: Vec<_> = ctx.viewport(|v| {
            v.prev_pass
                .widgets
                .get_layer(LayerId::new(Order::Foreground, popup))
                .filter(|w| {
                    w.id != popup
                        && w.id != popup.with("move")
                        && w.enabled
                        && w.sense.senses_click()
                        && !w.sense.senses_drag()
                        && w.rect.is_positive()
                })
                .map(|w| w.id)
                .collect()
        });
        assert_eq!(interactive.len(), choices.len());
        assert!(interactive
            .iter()
            .all(|id| choices.iter().any(|(choice, _)| choice == id)));
        assert!(crate::ui::arrowhead_editor::take_request(&ctx).is_none());
        assert_eq!(stroke_of(&app, id), original);
    }
}
