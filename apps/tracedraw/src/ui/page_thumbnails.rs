//! Shared page pictures for navigation without leaving the drawing.

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, Rect, Response, Sense, Stroke, Ui, Vec2};
use tracedraw_core::PageId;

/// The compact controls shared by the Pages panel and Page Overview.
#[derive(Clone, Copy)]
pub enum Action {
    Insert,
    Duplicate,
    Delete,
    Overview,
    Earlier,
    Later,
}

impl Action {
    fn key(self) -> &'static str {
        match self {
            Self::Insert => "menu.layout.insert_page",
            Self::Duplicate => "menu.layout.duplicate_page",
            Self::Delete => "menu.layout.delete_page",
            Self::Overview => "menu.view.page_sorter",
            Self::Earlier => "docker.page_move_up",
            Self::Later => "docker.page_move_down",
        }
    }
}

/// Icon controls keep translated names, keyboard focus and disabled tips.
pub fn button(ui: &mut Ui, action: Action, enabled: bool, on: bool) -> Response {
    let side = Tokens::prop_button().clamp(24.0, 32.0);
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .frame(false)
            .min_size(Vec2::splat(side)),
    );
    let key = action.key();
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, tr(key)));
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new(("page_action", key)), response.rect);
    });
    if ui.is_rect_visible(response.rect) {
        crate::theme::paint_icon_button(
            ui.painter(),
            response.rect,
            enabled && (response.hovered() || response.is_pointer_button_down_on()),
            on,
        );
        let rect = Rect::from_center_size(
            response.rect.center(),
            Vec2::splat(Tokens::prop_icon().min(side - 8.0)),
        );
        let color = if enabled || on {
            Tokens::icon()
        } else {
            Tokens::icon_off()
        };
        match action {
            Action::Overview => {
                for x in [0.0, 0.55] {
                    for y in [0.0, 0.55] {
                        let cell = Rect::from_min_size(
                            rect.min + egui::vec2(x, y) * rect.size(),
                            rect.size() * 0.4,
                        );
                        ui.painter().rect_stroke(
                            cell,
                            1.0,
                            Stroke::new(1.2, color),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
            Action::Earlier | Action::Later => {
                let sign = if matches!(action, Action::Earlier) {
                    -1.0
                } else {
                    1.0
                };
                let center = rect.center();
                let tip = center + egui::vec2(0.0, sign * 5.0);
                let line = Stroke::new(1.5, color);
                ui.painter()
                    .line_segment([center - egui::vec2(0.0, sign * 5.0), tip], line);
                for dx in [-4.0, 4.0] {
                    ui.painter()
                        .line_segment([tip, center + egui::vec2(dx, sign)], line);
                }
            }
            _ => {
                if let Some(glyph) = crate::ui::menu_glyphs::for_key(key) {
                    crate::ui::menu_glyphs::draw(ui.painter(), rect, glyph, color);
                }
            }
        }
        if response.has_focus() {
            focus_ring(ui, response.rect);
        }
    }
    response
        .rich_tip(crate::ui::tips::keyed(key))
        .rich_disabled_tip(crate::ui::tips::keyed(key))
}

fn focus_ring(ui: &Ui, rect: Rect) {
    let r = rect.shrink(2.0);
    ui.painter().extend(egui::Shape::dashed_line(
        &[
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ],
        Stroke::new(1.0, Tokens::text()),
        3.0,
        3.0,
    ));
}

fn describe(ui: &Ui, response: &Response, page: PageId, selected: bool, name: &str) {
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, name)
    });
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new(("page_thumbnail", page.raw())), response.rect);
        d.insert_temp(
            egui::Id::new(("page_thumbnail_focus", page.raw())),
            response.id,
        );
    });
    #[cfg(not(test))]
    let _ = (ui, page);
}

/// One focusable page picture. Only visible pictures ask the cache to render.
pub fn picture(app: &mut App, ui: &mut Ui, page: PageId, name: &str, size: Vec2) -> Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(
        rect,
        egui::Id::new(("page_picture", app.engine.instance(), page)),
        Sense::click(),
    );
    describe(ui, &response, page, page == app.page, name);
    paint_picture(app, ui, page, rect);
    if response.has_focus() {
        focus_ring(ui, rect);
    }
    response.rich_tip_with(|| format!("{name}\n{}", tr("docker.page_list_desc")))
}

fn paint_picture(app: &mut App, ui: &Ui, page: PageId, rect: Rect) {
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 3.0, Tokens::panel_dark());
        let mut cache = std::mem::take(&mut app.page_thumbs);
        cache.paused = ui.input(|i| i.pointer.any_down());
        let px = (rect.size().max_elem() * ui.ctx().pixels_per_point()).ceil() as u32;
        let texture = cache.get(
            ui.ctx(),
            app.doc(),
            (app.engine.instance(), app.engine.revision()),
            page,
            px,
        );
        app.page_thumbs = cache;
        let available = rect.shrink(5.0);
        let ratio = app.doc().page(page).ok().map(|p| p.size).and_then(|s| {
            (s.width.is_finite() && s.height.is_finite() && s.width > 0.0 && s.height > 0.0)
                .then_some(s.width / s.height)
        });
        if let Some(ratio) = ratio {
            let width = available
                .width()
                .min((available.height() as f64 * ratio) as f32);
            let height = available
                .height()
                .min((available.width() as f64 / ratio) as f32);
            let sheet =
                Rect::from_center_size(rect.center(), egui::vec2(width.max(1.0), height.max(1.0)));
            painter.rect_filled(sheet, 0.0, Color32::WHITE);
            if let Some(texture) = texture {
                painter.image(
                    texture.id(),
                    sheet,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            painter.rect_stroke(
                sheet,
                0.0,
                Stroke::new(1.0, Tokens::page_border()),
                egui::StrokeKind::Inside,
            );
        }
    }
}

pub const ROW_HEIGHT: f32 = 104.0;

pub struct PageRow {
    pub response: Response,
    pub move_page: Option<(PageId, usize)>,
}

/// A single selection target surrounds the sheet, title and dimensions.
pub fn row(app: &mut App, ui: &mut Ui, index: usize) -> Option<PageRow> {
    let page = app.doc().pages.get(index)?;
    let (id, name, size) = (page.id, page.name.clone(), page.size);
    let selected = id == app.page;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let response = ui.interact(
        rect,
        egui::Id::new(("page_row", app.engine.instance(), id)),
        Sense::click(),
    );
    describe(ui, &response, id, selected, &name);
    let fill = if selected {
        Tokens::selected_row()
    } else if response.hovered() {
        Tokens::hovered_row()
    } else {
        Tokens::field()
    };
    ui.painter().rect_filled(rect, 5.0, fill);
    ui.painter().rect_stroke(
        rect,
        5.0,
        Stroke::new(
            1.0,
            if selected {
                Tokens::selected_row_line()
            } else {
                Tokens::border()
            },
        ),
        egui::StrokeKind::Inside,
    );
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(
                rect.min + egui::vec2(0.0, 8.0),
                egui::vec2(3.0, ROW_HEIGHT - 16.0),
            ),
            1.0,
            Tokens::selected_row_line(),
        );
    }
    let content = rect.shrink(8.0);
    let gap = 8.0;
    let controls_width = 2.0 * Tokens::prop_button().clamp(24.0, 32.0) + 4.0;
    let thumb_width = (content.width() * 0.32)
        .clamp(24.0, 88.0)
        .min((content.width() - controls_width - gap).max(0.0));
    let thumbnail = Rect::from_min_size(content.min, egui::vec2(thumb_width, content.height()));
    paint_picture(app, ui, id, thumbnail);
    let text_rect = Rect::from_min_max(
        egui::pos2(thumbnail.right() + gap, content.top()),
        content.max,
    );
    let mut move_page = None;
    let mut details = ui.new_child(
        egui::UiBuilder::new()
            .id(egui::Id::new((
                "page_row_controls",
                app.engine.instance(),
                id,
            )))
            .max_rect(text_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    {
        let ui = &mut details;
        ui.set_width(text_rect.width());
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.add(
            egui::Label::new(egui::RichText::new(format!("{}. {name}", index + 1)).strong())
                .truncate(),
        );
        let dimensions = format!(
            "{} x {} {}",
            app.units.from_mm(size.width).round(),
            app.units.from_mm(size.height).round(),
            app.units.label(),
        );
        ui.add(
            egui::Label::new(
                egui::RichText::new(&dimensions)
                    .small()
                    .color(Tokens::text_dim()),
            )
            .truncate(),
        )
        .rich_tip(dimensions);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let up = button(ui, Action::Earlier, index > 0, false);
            let down = button(ui, Action::Later, index + 1 < app.doc().pages.len(), false);
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new(("page_move_up", id.raw())), up.rect);
                d.insert_temp(egui::Id::new(("page_move_down", id.raw())), down.rect);
            });
            if up.clicked() {
                move_page = Some((id, index.saturating_sub(1)));
            }
            if down.clicked() {
                move_page = Some((id, index + 1));
            }
        });
    }
    if response.has_focus() {
        focus_ring(ui, rect);
    }
    Some(PageRow {
        response: response.rich_tip_with(|| format!("{name}\n{}", tr("docker.page_list_desc"))),
        move_page,
    })
}

/// Keep deferred pictures moving and evict pictures no longer in view.
pub fn finish(app: &mut App, ui: &Ui) {
    app.page_thumbs.trim();
    if app.page_thumbs.waiting && !app.page_thumbs.paused {
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::DockerTab;

    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> usize {
        frame_at(ctx, app, events, 320.0)
    }

    fn frame_at(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>, width: f32) -> usize {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 480.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                app.page_thumbs.begin_frame();
                crate::ui::dockers2::show(app, ui, DockerTab::Pages);
            },
        );
        let uploads = output.textures_delta.set.len();
        output.textures_delta.clear();
        uploads
    }

    fn click(ctx: &egui::Context, app: &mut App, at: egui::Pos2) {
        frame(ctx, app, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(
                ctx,
                app,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    }

    #[test]
    fn page_thumbnails_rows_keep_controls_inside_a_narrow_panel() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let first = app.page;
        app.run(tracedraw_core::Command::RenamePage {
            page: first,
            name: "A very long page title that must stay on one line".repeat(4),
        });
        app.add_page();
        let second = app.page;
        app.goto_page(0);
        for width in [120.0, 200.0, 240.0, 320.0] {
            for _ in 0..3 {
                frame_at(&ctx, &mut app, Vec::new(), width);
            }
            let rect = |key, page: PageId| {
                ctx.data(|d| d.get_temp::<Rect>(egui::Id::new((key, page.raw()))))
                    .expect("visible row or control")
            };
            let first_row = rect("page_thumbnail", first);
            let second_row = rect("page_thumbnail", second);
            assert!(first_row.right() <= width, "{width}: {first_row:?}");
            assert!(
                second_row.top() >= first_row.bottom(),
                "{width}: {first_row:?} {second_row:?}"
            );
            assert_eq!(first_row.height(), ROW_HEIGHT);
            for page in [first, second] {
                let row = rect("page_thumbnail", page);
                assert!(row.contains_rect(rect("page_move_up", page)));
                assert!(row.contains_rect(rect("page_move_down", page)));
            }
        }
    }

    #[test]
    fn page_thumbnails_move_buttons_do_not_select_their_row() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        let first = app.page;
        app.add_page();
        let second = app.page;
        app.goto_page(0);
        for _ in 0..4 {
            frame(&ctx, &mut app, Vec::new());
        }
        let at = ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new(("page_move_up", second.raw()))))
            .expect("move earlier control")
            .center();
        click(&ctx, &mut app, at);
        assert_eq!(app.doc().pages[0].id, second);
        assert_eq!(
            app.page, first,
            "moving a different row keeps the current page"
        );
        assert!(app.engine.undo().is_ok());
        assert_eq!(app.doc().pages[0].id, first);
    }

    #[test]
    fn page_thumbnails_overview_opens_from_multipage_view() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.set_multipage(true);
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let at = ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new(("page_action", Action::Overview.key()))))
            .expect("overview button")
            .center();
        let before = app.doc().clone();
        click(&ctx, &mut app, at);
        assert!(app.page_sorter);
        assert!(!app.multipage);
        assert_eq!(app.doc(), &before);
    }

    #[test]
    fn page_thumbnails_navigate_without_changing_the_drawing() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.add_page();
        let second = app.page;
        app.goto_page(0);
        let before = app.doc().clone();
        let revision = app.engine.revision();
        for _ in 0..4 {
            frame(&ctx, &mut app, Vec::new());
        }
        let rect = ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new(("page_thumbnail", second.raw()))))
            .expect("second picture");
        assert!(
            rect.bottom() <= 480.0,
            "two page pictures fit in a narrow panel"
        );
        let at = rect.center();
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(app.page, second);
        assert_eq!(app.doc(), &before);
        assert_eq!(app.engine.revision(), revision);
        app.goto_page(0);
        frame(&ctx, &mut app, Vec::new());
        let focus = ctx
            .data(|d| d.get_temp::<egui::Id>(egui::Id::new(("page_thumbnail_focus", second.raw()))))
            .expect("focusable picture");
        ctx.memory_mut(|m| m.request_focus(focus));
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(app.page, second, "Enter activates the focused picture");
        // The same action activates a page in place in Multipage View.
        app.set_multipage(true);
        app.goto_page(0);
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let at = ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new(("page_thumbnail", second.raw()))))
            .unwrap()
            .center();
        frame(&ctx, &mut app, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert_eq!(app.page, second);
        assert!(app.multipage);
    }

    #[test]
    fn page_thumbnails_render_only_the_visible_rows_of_a_long_drawing() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        for _ in 0..100 {
            app.add_page();
        }
        app.goto_page(0);
        for _ in 0..12 {
            frame(&ctx, &mut app, Vec::new());
        }
        let hidden = app.doc().pages[70].id;
        assert!(ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new(("page_thumbnail", hidden.raw()))))
            .is_none());
        let steady = frame(&ctx, &mut app, Vec::new());
        assert!(steady == 0, "cached rows upload no new textures");
        app.goto_page(70);
        for _ in 0..6 {
            frame(&ctx, &mut app, Vec::new());
        }
        assert!(ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new(("page_thumbnail", hidden.raw()))))
            .is_some());
    }

    #[test]
    fn page_thumbnails_are_reachable_from_the_page_counter() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_dockers = false;
        let run = |app: &mut App, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    crate::ui::window_bars::page_navigator(app, ui);
                },
            );
            output.textures_delta.clear();
        };
        run(&mut app, Vec::new());
        let at = ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new("page_thumbnails_button")))
            .unwrap()
            .center();
        run(&mut app, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            run(
                &mut app,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(app.show_dockers);
        assert_eq!(app.docker_tab, DockerTab::Pages);
    }
}
