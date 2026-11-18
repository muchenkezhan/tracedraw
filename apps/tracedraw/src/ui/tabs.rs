//! The document tabs above the rulers: the Welcome Screen, one tab per open
//! drawing (an asterisk marks unsaved changes; the close button shows on
//! the hovered tab) and the New tab after the last one. Tabs share rounded
//! top corners and flat bottoms. The Welcome Screen tab closes to its
//! house alone, which shows the Welcome Screen again (#677). A drawing's
//! tab dragged along the strip takes the drawing to another place among
//! the tabs (#2903).

use crate::app::App;
use crate::i18n::tr;
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Height of the strip, including the space above each tab.
pub const TAB_HEIGHT: f32 = 30.0;
const TOP_PAD: f32 = 4.0;
const GAP: f32 = 4.0;
const CORNERS: egui::CornerRadius = egui::CornerRadius {
    nw: 6,
    ne: 6,
    sw: 0,
    se: 0,
};
const PAD: f32 = 8.0;
const ICON_GAP: f32 = 6.0;
const CLOSE_GAP: f32 = 4.0;
const CLOSE: f32 = 14.0;
const FONT: f32 = 13.0;

/// What a click on the strip asked for.
enum Action {
    Welcome,
    CloseWelcome,
    Switch(usize),
    Close(usize),
    New,
    /// A drawing's tab dragged to another place among the tabs.
    Move(usize, usize),
}

pub fn document_tabs(app: &mut App, ui: &mut Ui) {
    let mut action = None;
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("document_tabs_strip"), ui.max_rect()));
    // The tabs' flat bottoms meet the rulers without a separator.
    ui.spacing_mut().item_spacing.y = 0.0;
    // The active tab, when it changed since the last frame: the strip
    // slides to show it (a drawing opened or chosen from the Window
    // menu).
    let shown_id = egui::Id::new("document_tabs_active");
    let active_now = (!app.show_welcome).then_some(app.active_doc);
    let changed = ui.ctx().data(|d| d.get_temp::<Option<usize>>(shown_id)) != Some(active_now);
    ui.ctx().data_mut(|d| d.insert_temp(shown_id, active_now));
    // More tabs than the window is wide: the strip slides with its « and
    // » buttons and the mouse wheel (#1908).
    crate::ui::overflow::row_showing(ui, "document_tabs", |ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        ui.add_space(GAP);
        let mut keep = None;
        let welcome = app.show_welcome;
        // Shown again, the Welcome Screen has its whole tab back.
        if welcome && app.settings.welcome_tab_closed {
            app.settings.welcome_tab_closed = false;
        }
        match welcome_tab(ui, welcome, app.settings.welcome_tab_closed) {
            TabClick::Select => action = Some(Action::Welcome),
            TabClick::Close => action = Some(Action::CloseWelcome),
            TabClick::None => {}
        }
        // A drawing's tab dragged along the strip (#2903), as a page tab
        // is (#2712): the tab it is and where the pointer took hold of
        // it. While it is held the other tabs slide aside to show where
        // it will land and it follows the pointer; let go, the drawing
        // takes that place.
        let n = app.docs.len();
        let titles: Vec<String> = (0..n).map(|i| app.document_tab_title(i)).collect();
        let widths: Vec<f32> = titles.iter().map(|t| doc_tab_width(ui, t)).collect();
        let drag_key = egui::Id::new("doc_tab_drag");
        let (pointer_x, released, down) = ui.input(|inp| {
            (
                inp.pointer.latest_pos().map(|p| p.x),
                inp.pointer.any_released(),
                inp.pointer.any_down(),
            )
        });
        let mut held = ui
            .ctx()
            .data(|d| d.get_temp::<(usize, f32)>(drag_key))
            .filter(|(from, _)| *from < n);
        if held.is_some() && !down && !released {
            // A drag that ended without its tab seeing it (the tab went).
            ui.ctx()
                .data_mut(|d| d.remove_temp::<(usize, f32)>(drag_key));
            held = None;
        }
        let gap = ui.spacing().item_spacing.x;
        let start_x = ui.cursor().min.x;
        let held_to = held.zip(pointer_x).map(|((from, grab), px)| {
            let others: Vec<f32> = (0..n)
                .filter(|j| *j != from)
                .map(|j| widths[j] + gap)
                .collect();
            // The dragged tab's middle decides, not the pointer's spot.
            let mid = px - grab + widths[from] / 2.0;
            (
                from,
                crate::ui::window_bars::tab_drop_slot(&others, start_x, mid),
            )
        });
        let order = match held_to {
            Some((from, to)) => crate::ui::window_bars::tab_order(n, from, to),
            None => (0..n).collect(),
        };
        for i in order {
            let active = !app.show_welcome && i == app.active_doc;
            let float_left = held
                .filter(|(from, _)| *from == i)
                .zip(pointer_x)
                .map(|((_, grab), px)| px - grab);
            let tab = doc_tab(ui, i, &titles[i], active, widths[i], float_left);
            if active && changed {
                keep = Some(tab.slot);
            }
            if tab.drag_started {
                let grab = pointer_x.map_or(widths[i] / 2.0, |px| px - tab.slot.min.x);
                ui.ctx().data_mut(|d| d.insert_temp(drag_key, (i, grab)));
            }
            if tab.drag_stopped {
                ui.ctx()
                    .data_mut(|d| d.remove_temp::<(usize, f32)>(drag_key));
                if let Some((from, to)) = held_to.filter(|(from, _)| *from == i) {
                    if to != from {
                        action = Some(Action::Move(from, to));
                    }
                }
            }
            match tab.click {
                TabClick::Select => action = Some(Action::Switch(i)),
                TabClick::Close => action = Some(Action::Close(i)),
                TabClick::None => {}
            }
        }
        if new_tab(ui) {
            action = Some(Action::New);
        }
        keep
    });
    match action {
        Some(Action::Welcome) => app.show_welcome = true,
        Some(Action::CloseWelcome) => {
            app.settings.welcome_tab_closed = true;
            app.show_welcome = false;
            // With no drawing open the empty workspace stays, and the
            // Hints docker comes up at its Home topics.
            if !app.has_document() {
                app.open_docker(crate::app::DockerTab::Hints);
                app.hints.go(crate::ui::hints::HintPage::Home);
            }
        }
        Some(Action::Switch(i)) => app.switch_document(i),
        Some(Action::Close(i)) => {
            app.switch_document(i);
            app.close_document();
        }
        Some(Action::New) => app.request_new_document(),
        Some(Action::Move(from, to)) => app.move_document(from, to),
        None => {}
    }
}

fn tab_fill(active: bool, hovered: bool) -> Color32 {
    if active {
        crate::theme::Tokens::tab_active()
    } else if hovered {
        crate::theme::Tokens::tool_hover()
    } else {
        crate::theme::Tokens::tab()
    }
}

/// Every tab leaves the same space above its rounded top.
fn tab_rect(slot: Rect) -> Rect {
    Rect::from_min_max(slot.min + Vec2::new(0.0, TOP_PAD), slot.max)
}

/// Reserve only the label and close button, even for short titles.
fn tab_width(content_width: f32) -> f32 {
    PAD + content_width + CLOSE_GAP + CLOSE + PAD
}

fn tab_close_rect(rect: Rect) -> Rect {
    Rect::from_center_size(
        Pos2::new(rect.right() - PAD - CLOSE / 2.0, rect.center().y),
        Vec2::splat(CLOSE),
    )
}

/// The Welcome Screen tab: a house and the title, with a close button on
/// hover; `closed`, the house alone, which shows the Welcome Screen
/// again.
fn welcome_tab(ui: &mut Ui, active: bool, closed: bool) -> TabClick {
    let icon = 16.0;
    if closed {
        let (slot, _) = ui.allocate_exact_size(Vec2::new(28.0, TAB_HEIGHT), Sense::hover());
        let rect = tab_rect(slot);
        let resp = ui.interact(rect, egui::Id::new(WELCOME_TAB), Sense::click());
        let painter = ui.painter();
        painter.rect_filled(rect, CORNERS, tab_fill(active, resp.hovered()));
        house(
            painter,
            rect.center() - Vec2::splat(icon / 2.0),
            crate::theme::Tokens::icon(),
        );
        return if resp
            .rich_tip(crate::ui::tips::keyed("welcome.title"))
            .clicked()
        {
            TabClick::Select
        } else {
            TabClick::None
        };
    }
    let label = tr("welcome.title");
    let galley = ui.painter().layout_no_wrap(
        label,
        egui::FontId::proportional(FONT),
        crate::theme::Tokens::text(),
    );
    let width = tab_width(icon + ICON_GAP + galley.size().x);
    let (slot, _) = ui.allocate_exact_size(Vec2::new(width, TAB_HEIGHT), Sense::hover());
    let rect = tab_rect(slot);
    let resp = ui.interact(rect, egui::Id::new(WELCOME_TAB), Sense::click());
    let close_rect = tab_close_rect(rect);
    let close_resp = ui
        .interact(close_rect, egui::Id::new(WELCOME_CLOSE), Sense::click())
        .rich_tip(crate::ui::tips::keyed("tabs.close_welcome"));
    let close_hovered = close_resp.hovered();
    let painter = ui.painter();
    painter.rect_filled(rect, CORNERS, tab_fill(active, resp.hovered()));
    house(
        painter,
        Pos2::new(rect.left() + PAD, rect.center().y - icon / 2.0),
        crate::theme::Tokens::icon(),
    );
    painter.galley(
        Pos2::new(
            rect.left() + PAD + icon + ICON_GAP,
            (rect.center().y - galley.size().y / 2.0).round(),
        ),
        galley,
        crate::theme::Tokens::text(),
    );
    if resp.hovered() || close_hovered {
        close_mark(painter, close_rect, close_hovered);
    }
    let resp = resp.rich_tip(crate::ui::tips::keyed("welcome.title"));
    if close_resp.clicked() || resp.middle_clicked() {
        TabClick::Close
    } else if resp.clicked() {
        TabClick::Select
    } else {
        TabClick::None
    }
}

/// The ids of the Welcome Screen tab and of its close button.
pub(crate) const WELCOME_TAB: &str = "welcome_tab";
pub(crate) const WELCOME_CLOSE: &str = "welcome_tab_close";

/// A tab's close button: an X, on a square while the pointer is over it.
fn close_mark(painter: &egui::Painter, close_rect: Rect, hovered: bool) {
    if hovered {
        painter.rect_filled(close_rect, 2.0, Tokens::border());
    }
    let s = Stroke::new(1.2, Tokens::text_dim());
    let r = close_rect.shrink(3.5);
    painter.line_segment([r.left_top(), r.right_bottom()], s);
    painter.line_segment([r.right_top(), r.left_bottom()], s);
}

/// An outlined house, 16 pixels square, top-left at `o`.
fn house(painter: &egui::Painter, o: Pos2, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    for [a, b] in [
        [(0.5, 7.5), (8.0, 0.5)],
        [(8.0, 0.5), (15.5, 7.5)],
        [(2.5, 6.0), (2.5, 15.5)],
        [(2.5, 15.5), (6.0, 15.5)],
        [(6.0, 15.5), (6.0, 10.0)],
        [(6.0, 10.0), (10.0, 10.0)],
        [(10.0, 10.0), (10.0, 15.5)],
        [(10.0, 15.5), (13.5, 15.5)],
        [(13.5, 15.5), (13.5, 6.0)],
    ] {
        painter.line_segment([o + Vec2::new(a.0, a.1), o + Vec2::new(b.0, b.1)], stroke);
    }
}

/// The New tab: a grey plus on a tab-coloured square.
fn new_tab(ui: &mut Ui) -> bool {
    let (slot, _) = ui.allocate_exact_size(Vec2::new(28.0, TAB_HEIGHT), Sense::hover());
    let rect = tab_rect(slot);
    let resp = ui.interact(rect, ui.id().with("new_document_tab"), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, CORNERS, tab_fill(false, resp.hovered()));
    let c = rect.center();
    let s = Stroke::new(2.0, crate::theme::Tokens::text_dim());
    painter.line_segment([c - Vec2::new(6.0, 0.0), c + Vec2::new(6.0, 0.0)], s);
    painter.line_segment([c - Vec2::new(0.0, 6.0), c + Vec2::new(0.0, 6.0)], s);
    resp.rich_tip(tr("menu.file.new")).clicked()
}

enum TabClick {
    None,
    Select,
    Close,
}

/// The id of the tab of the drawing `i`.
pub(crate) fn doc_tab_id(i: usize) -> egui::Id {
    egui::Id::new(("doc_tab", i))
}

/// The width of a drawing's tab titled `title`.
fn doc_tab_width(ui: &Ui, title: &str) -> f32 {
    let galley = ui.painter().layout_no_wrap(
        title.to_string(),
        egui::FontId::proportional(FONT),
        crate::theme::Tokens::text(),
    );
    tab_width(galley.size().x)
}

/// What a drawing's tab did this frame.
struct DocTab {
    click: TabClick,
    /// Its place in the strip.
    slot: Rect,
    drag_started: bool,
    drag_stopped: bool,
}

/// The tab of drawing `i`, `width` wide. While it is dragged
/// (`float_left`, the left edge where the pointer holds it) its place in
/// the strip shows as an outline and the tab is drawn at the pointer,
/// over the other tabs (#2903).
fn doc_tab(
    ui: &mut Ui,
    i: usize,
    title: &str,
    active: bool,
    width: f32,
    float_left: Option<f32>,
) -> DocTab {
    let galley = ui.painter().layout_no_wrap(
        title.to_string(),
        egui::FontId::proportional(FONT),
        crate::theme::Tokens::text(),
    );
    let (slot, _) = ui.allocate_exact_size(Vec2::new(width, TAB_HEIGHT), Sense::hover());
    let rect = tab_rect(match float_left {
        Some(left) => Rect::from_min_size(Pos2::new(left, slot.min.y), slot.size()),
        None => slot,
    });
    let resp = ui.interact(rect, doc_tab_id(i), Sense::click_and_drag());
    let close_rect = tab_close_rect(rect);
    let close_id = ui.id().with(("doc_tab_close", i));
    let close_resp = ui
        .interact(close_rect, close_id, Sense::click())
        .rich_tip(crate::ui::tips::keyed("tabs.close_drawing"));
    let painter = if float_left.is_some() {
        // The place it will land, then the tab itself over its
        // neighbours.
        ui.painter().rect_stroke(
            slot.shrink(2.0),
            2.0,
            Stroke::new(1.0, Tokens::text_dim()),
            egui::StrokeKind::Inside,
        );
        ui.ctx().request_repaint();
        ui.painter().clone().with_layer_id(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("doc_tab_dragged"),
        ))
    } else {
        ui.painter().clone()
    };
    let fill = if float_left.is_some() && !active {
        crate::theme::Tokens::tool_hover()
    } else {
        tab_fill(active, resp.hovered())
    };
    painter.rect_filled(rect, CORNERS, fill);
    painter.galley(
        Pos2::new(
            rect.left() + PAD,
            (rect.center().y - galley.size().y / 2.0).round(),
        ),
        galley,
        crate::theme::Tokens::text(),
    );
    if resp.hovered() || close_resp.hovered() {
        close_mark(&painter, close_rect, close_resp.hovered());
    }
    let drag_started = resp.drag_started_by(egui::PointerButton::Primary)
        && !crate::ui::overflow::finger_sliding(ui.ctx());
    let drag_stopped = resp.drag_stopped();
    let resp = resp.rich_tip(format!("{title}\n{}", tr("tabs.drawing_desc")));
    let click = if close_resp.clicked() || resp.middle_clicked() {
        TabClick::Close
    } else if resp.clicked() {
        TabClick::Select
    } else {
        TabClick::None
    };
    DocTab {
        click,
        slot,
        drag_started,
        drag_stopped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton};

    /// Short, long and multilingual titles keep the same compact padding,
    /// including while each close cross appears and disappears.
    #[test]
    fn tabs_fit_their_titles_without_resizing_on_hover() {
        fn lines_inside(shape: &egui::Shape, rect: Rect) -> usize {
            match shape {
                egui::Shape::LineSegment { points, .. }
                    if points.iter().all(|p| rect.contains(*p)) =>
                {
                    1
                }
                egui::Shape::Vec(shapes) => shapes.iter().map(|s| lines_inside(s, rect)).sum(),
                _ => 0,
            }
        }
        let ctx = crate::theme::ui_context();
        let titles = [
            "A",
            "Plan.tdraw",
            "未命名-1",
            "未命名-2未命名-2",
            "未命名-1*",
        ];
        let mut baseline = Vec::new();
        let targets = std::iter::once(None)
            .chain((0..=titles.len()).map(Some))
            .chain(std::iter::once(None));
        for target in targets {
            let pointer = target.map(|i| tab_close_rect(baseline[i]).center());
            for frame in 0..3 {
                let mut drawn = Vec::new();
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            Vec2::new(1200.0, TAB_HEIGHT),
                        )),
                        events: vec![pointer.map_or(Event::PointerGone, Event::PointerMoved)],
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new())
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.x = GAP;
                                ui.horizontal(|ui| {
                                    let text_width = |ui: &Ui, title: &str| {
                                        ui.painter()
                                            .layout_no_wrap(
                                                title.to_owned(),
                                                egui::FontId::proportional(FONT),
                                                Tokens::text(),
                                            )
                                            .size()
                                            .x
                                    };
                                    welcome_tab(ui, false, false);
                                    drawn.push((
                                        ctx.read_response(egui::Id::new(WELCOME_TAB))
                                            .expect("welcome tab")
                                            .rect,
                                        16.0 + 6.0 + text_width(ui, &tr("welcome.title")),
                                    ));
                                    for (i, title) in titles.iter().enumerate() {
                                        let width = doc_tab_width(ui, title);
                                        let tab = doc_tab(ui, i, title, false, width, None);
                                        drawn.push((tab_rect(tab.slot), text_width(ui, title)));
                                    }
                                });
                            });
                    },
                );
                out.textures_delta.clear();
                for (rect, content_width) in &drawn {
                    assert!((rect.width() - content_width - 34.0).abs() < 0.01);
                    assert!(
                        (tab_close_rect(*rect).left() - rect.left() - 8.0 - content_width - 4.0)
                            .abs()
                            < 0.01
                    );
                }
                let rects: Vec<_> = drawn.iter().map(|(rect, _)| *rect).collect();
                if baseline.is_empty() {
                    assert!(
                        rects[1].width() < 60.0,
                        "short titles have no minimum width"
                    );
                    baseline = rects;
                } else {
                    assert_eq!(rects, baseline, "hover does not move or resize tabs");
                }
                // Hover responses settle after the pointer changes tabs.
                if frame == 2 {
                    for (i, rect) in baseline.iter().enumerate() {
                        let lines: usize = out
                            .shapes
                            .iter()
                            .map(|s| lines_inside(&s.shape, tab_close_rect(*rect)))
                            .sum();
                        assert_eq!(
                            lines,
                            if target == Some(i) { 2 } else { 0 },
                            "close cross on tab {i}, hovered tab {target:?}"
                        );
                    }
                }
            }
        }
    }

    /// Welcome, drawing and New tabs share their silhouette and spacing;
    /// their flat bottoms leave the strip visible in every gap.
    #[test]
    fn document_tabs_share_rounded_tops_and_flat_bottoms() {
        fn rectangles<'a>(shape: &'a egui::Shape, out: &mut Vec<&'a egui::epaint::RectShape>) {
            match shape {
                egui::Shape::Rect(rect) => out.push(rect),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        rectangles(shape, out);
                    }
                }
                _ => {}
            }
        }
        for theme in crate::theme::Theme::ALL {
            for closed in [false, true] {
                let mut app = App::headless();
                while app.docs.len() < 3 {
                    app.new_document();
                }
                app.switch_document(1);
                app.show_welcome = false;
                app.settings.welcome_tab_closed = closed;
                crate::theme::set_theme(theme);
                let ctx = crate::theme::ui_context();
                ctx.set_visuals(crate::theme::visuals());
                let mut textures = crate::soft_canvas::Textures::default();
                for _ in 0..3 {
                    let mut out = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(640.0, 30.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            egui::CentralPanel::default()
                                .frame(egui::Frame::new().fill(Tokens::tab_strip()))
                                .show(ui, |ui| document_tabs(&mut app, ui));
                        },
                    );
                    textures.apply(&out.textures_delta);
                    out.textures_delta.clear();
                    let mut rects = Vec::new();
                    for shape in &out.shapes {
                        rectangles(&shape.shape, &mut rects);
                    }
                    let mut tabs: Vec<_> = rects
                        .into_iter()
                        .filter(|r| r.corner_radius == CORNERS)
                        .collect();
                    tabs.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()));
                    assert_eq!(tabs.len(), 5, "{theme:?}, closed={closed}");
                    for (i, tab) in tabs.iter().enumerate() {
                        assert_eq!(tab.rect.top(), 4.0);
                        assert_eq!(tab.rect.bottom(), 30.0);
                        assert_eq!(
                            tab.fill,
                            if i == 2 {
                                Tokens::tab_active()
                            } else {
                                Tokens::tab()
                            }
                        );
                    }
                    for pair in tabs.windows(2) {
                        assert_eq!(pair[1].rect.left() - pair[0].rect.right(), 4.0);
                    }
                    let gaps: Vec<_> = tabs
                        .windows(2)
                        .map(|p| p[0].rect.right() as usize + 2)
                        .collect();
                    let shapes = std::mem::take(&mut out.shapes);
                    let prims = ctx.tessellate(shapes, out.pixels_per_point);
                    let mut canvas = crate::soft_canvas::Canvas::new(640, 30);
                    canvas.paint(&textures, &prims);
                    let rgb = |x: usize, y: usize| {
                        let p = canvas.px[y * canvas.w + x];
                        [p[0], p[1], p[2]].map(|c| (c * 255.0).round() as u8)
                    };
                    for x in gaps {
                        assert_eq!(rgb(x, 29), Tokens::tab_strip().to_array()[..3]);
                    }
                    // Close crosses stay hidden while the pointer is
                    // absent, leaving the compact tabs' fills visible.
                    for i in 0..4 {
                        if i == 0 && closed {
                            continue;
                        }
                        let rect = if i == 0 {
                            ctx.read_response(egui::Id::new(WELCOME_TAB))
                                .expect("welcome tab")
                                .rect
                        } else {
                            tab(&ctx, i - 1)
                        };
                        let x = tab_close_rect(rect).center().x as usize;
                        let y = rect.center().y as usize;
                        let fill = if i == 2 {
                            Tokens::tab_active()
                        } else {
                            Tokens::tab()
                        };
                        assert_eq!(
                            rgb(x, y),
                            fill.to_array()[..3],
                            "close cross stays hidden on tab {i}"
                        );
                    }
                    if theme == crate::theme::Theme::Light && !closed {
                        if let Some(path) = std::env::var_os("TRACEDRAW_TABS_SHOT") {
                            let img = image::RgbaImage::from_fn(640, 30, |x, y| {
                                image::Rgba(
                                    canvas.px[y as usize * canvas.w + x as usize]
                                        .map(|c| (c * 255.0).round() as u8),
                                )
                            });
                            img.save(path).expect("save the rendered document tabs");
                        }
                    }
                }
            }
        }
    }

    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(700.0, 800.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| crate::ui::root(app, ui));
        out.textures_delta.clear();
    }

    fn click(ctx: &egui::Context, app: &mut App, at: Pos2) {
        frame(ctx, app, vec![Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(
                ctx,
                app,
                vec![Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        frame(ctx, app, Vec::new());
    }

    fn tab(ctx: &egui::Context, i: usize) -> Rect {
        ctx.read_response(doc_tab_id(i))
            .map(|r| r.rect)
            .expect("the tab is drawn")
    }

    /// Hovering the cross on an inactive tab closes that drawing only.
    #[test]
    fn an_inactive_drawing_closes_from_its_cross() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        while app.docs.len() < 3 {
            app.new_document();
        }
        app.switch_document(0);
        let drawings: Vec<_> = app.docs.iter().map(|d| d.drawing).collect();
        // Enough room for every tab, without the full window's dockers
        // reducing the strip and sliding this cross out of view.
        let mut draw = |events| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 30.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(Tokens::tab_strip()))
                        .show(ui, |ui| document_tabs(&mut app, ui));
                },
            );
            out.textures_delta.clear();
        };
        for _ in 0..3 {
            draw(Vec::new());
        }
        let rect = tab(&ctx, 1);
        let close = tab_close_rect(rect).center();
        draw(vec![Event::PointerMoved(close)]);
        for pressed in [true, false] {
            draw(vec![Event::PointerButton {
                pos: close,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        assert_eq!(
            app.docs.iter().map(|d| d.drawing).collect::<Vec<_>>(),
            vec![drawings[0], drawings[2]]
        );
    }

    /// Twelve drawings in a 700 point window: the tabs past the window's
    /// edge are reached with the strip's » button and the mouse wheel and
    /// can be chosen; choosing a drawing elsewhere (the Window menu)
    /// slides its tab into view (#1908).
    #[test]
    fn tabs_past_the_window_edge_can_be_reached() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        while app.docs.len() < 12 {
            app.new_document();
        }
        app.switch_document(0);
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        // The room between the strip's « and » buttons.
        let strip = ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new("document_tabs_strip")))
            .expect("the strip is drawn");
        let b = crate::ui::overflow::BUTTON;
        let window = Rect::from_min_max(
            Pos2::new(strip.left() + b - 0.5, strip.top()),
            Pos2::new(strip.right() - b + 0.5, strip.top() + TAB_HEIGHT),
        );
        let last = app.docs.len() - 1;
        assert!(tab(&ctx, last).left() > window.right(), "past the edge");
        // » until the last tab is wholly in the window.
        let first = tab(&ctx, 0);
        let forward = Pos2::new(strip.right() - b / 2.0, first.center().y);
        for _ in 0..20 {
            if window.contains_rect(tab(&ctx, last)) {
                break;
            }
            click(&ctx, &mut app, forward);
        }
        let r = tab(&ctx, last);
        assert!(window.contains_rect(r), "{r:?} reached with »");
        click(&ctx, &mut app, r.center());
        assert_eq!(app.active_doc, last, "and chosen");
        // The wheel over the strip slides it back to the first tab.
        for _ in 0..40 {
            frame(
                &ctx,
                &mut app,
                vec![
                    Event::PointerMoved(Pos2::new(window.center().x, first.center().y)),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, 200.0),
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                    },
                ],
            );
        }
        let r = ctx
            .read_response(egui::Id::new(WELCOME_TAB))
            .map(|r| r.rect)
            .expect("the Welcome Screen tab");
        assert!(window.contains_rect(r), "{r:?} back with the wheel");
        assert!(tab(&ctx, 0).left() < window.right());
        assert!(!window.contains_rect(tab(&ctx, last)));
        // Another drawing chosen without the strip: its tab shows.
        app.switch_document(last - 1);
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let r = tab(&ctx, last - 1);
        assert!(window.contains_rect(r), "{r:?} the active tab shows");
    }

    fn press(ctx: &egui::Context, app: &mut App, at: Pos2, pressed: bool) {
        frame(
            ctx,
            app,
            vec![Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }

    /// A drawing's tab dragged along the strip, as a page tab is: while
    /// it is held the others slide aside and it follows the pointer;
    /// let go past the last tab, the drawing is the last one, the same
    /// drawing still active (#2903).
    #[test]
    fn a_drawing_tab_dragged_along_the_strip_moves_the_drawing() {
        let ctx = crate::theme::ui_context();
        let mut app = App::headless();
        app.show_welcome = false;
        while app.docs.len() < 3 {
            app.new_document();
        }
        app.switch_document(0);
        let drawings: Vec<u64> = app.docs.iter().map(|d| d.drawing).collect();
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let first = tab(&ctx, 0);
        let last = tab(&ctx, 2);
        let mut at = first.center();
        frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        press(&ctx, &mut app, at, true);
        let target = Pos2::new(last.right() + 20.0, at.y);
        for _ in 0..12 {
            at.x += (target.x - first.center().x) / 12.0;
            frame(&ctx, &mut app, vec![Event::PointerMoved(at)]);
        }
        // Held: the second tab has slid to the first place.
        let r = tab(&ctx, 1);
        assert!((r.left() - first.left()).abs() < 1.5, "{r:?} vs {first:?}");
        assert_eq!(app.docs[0].drawing, drawings[0], "not moved yet");
        press(&ctx, &mut app, at, false);
        frame(&ctx, &mut app, Vec::new());
        let now: Vec<u64> = app.docs.iter().map(|d| d.drawing).collect();
        assert_eq!(now, vec![drawings[1], drawings[2], drawings[0]]);
        assert_eq!(app.active_doc, 2, "the same drawing stays active");
        // A plain click still chooses a drawing (the strip slid to show
        // the active tab; any tab in reach will do).
        for _ in 0..3 {
            frame(&ctx, &mut app, Vec::new());
        }
        let strip = ctx
            .data(|d| d.get_temp::<Rect>(egui::Id::new("document_tabs_strip")))
            .expect("the strip is drawn");
        let b = crate::ui::overflow::BUTTON;
        let window =
            Rect::from_min_max(strip.min + Vec2::new(b, 0.0), strip.max - Vec2::new(b, 0.0));
        let j = (0..3)
            .find(|j| *j != app.active_doc && window.contains_rect(tab(&ctx, *j)))
            .expect("another tab in reach");
        click(&ctx, &mut app, tab(&ctx, j).center());
        assert_eq!(app.active_doc, j);
    }

    /// Moving a tab keeps the active window and the others' order.
    #[test]
    fn move_document_keeps_the_active_window() {
        let mut app = App::headless();
        while app.docs.len() < 4 {
            app.new_document();
        }
        app.switch_document(1);
        let d: Vec<u64> = app.docs.iter().map(|s| s.drawing).collect();
        app.move_document(3, 0);
        let now: Vec<u64> = app.docs.iter().map(|s| s.drawing).collect();
        assert_eq!(now, vec![d[3], d[0], d[1], d[2]]);
        assert_eq!(app.active_doc, 2);
        app.move_document(2, 3);
        assert_eq!(app.active_doc, 3);
        app.move_document(0, 1);
        assert_eq!(app.active_doc, 3);
        app.move_document(9, 0);
        assert_eq!(app.docs.len(), 4);
    }
}
