//! The Workspace, What's New, Learn and Templates pages of the Welcome Screen.

use super::{news_heading, news_row_id, release_page, HISTORY_LEN, RELEASES};
use crate::app::{App, Workspace};
use crate::i18n::{tr, trf};
use crate::theme::{self, Tokens};
use crate::tools::Tool;
use crate::ui::icons;
use egui::{RichText, Ui};
use std::sync::Arc;

fn section_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Tokens::panel())
        .stroke(egui::Stroke::new(1.0, Tokens::border()))
        .corner_radius(8.0)
        .inner_margin(16.0)
}

pub(super) fn workspace(app: &mut App, ui: &mut Ui) {
    ui.label(RichText::new(tr("welcome.choose_workspace")).font(theme::bold(18.0)));
    ui.add_space(12.0);
    let presets = [
        (
            Workspace::Default,
            "menu.window.workspace_default",
            "welcome.ws_default_desc",
        ),
        (
            Workspace::Lite,
            "menu.window.workspace_lite",
            "welcome.ws_lite_desc",
        ),
        (
            Workspace::Classic,
            "menu.window.workspace_classic",
            "welcome.ws_classic_desc",
        ),
        (
            Workspace::Illustration,
            "menu.window.workspace_illustration",
            "welcome.ws_illustration_desc",
        ),
        (
            Workspace::PageLayout,
            "menu.window.workspace_page_layout",
            "welcome.ws_page_layout_desc",
        ),
        (
            Workspace::Touch,
            "menu.window.workspace_touch",
            "welcome.ws_touch_desc",
        ),
    ];
    let columns = if ui.available_width() >= 600.0 { 2 } else { 1 };
    let gap = 12.0;
    let width = (ui.available_width() - gap * (columns - 1) as f32) / columns as f32;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        ui.spacing_mut().button_padding = egui::vec2(16.0, 14.0);
        let visuals = ui.visuals_mut();
        visuals.widgets.inactive.weak_bg_fill = Tokens::field();
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Tokens::border());
        visuals.widgets.hovered.weak_bg_fill = Tokens::hovered_row();
        visuals.widgets.active.weak_bg_fill = Tokens::selected_row();
        visuals.selection.bg_fill = Tokens::selected_row();
        for row in presets.chunks(columns) {
            ui.horizontal_top(|ui| {
                for &(preset, title, description) in row {
                    let selected = app.workspace == preset;
                    let high_contrast = theme::theme() == theme::Theme::HighContrast;
                    // The native button supplies the ink for its focus and hover states.
                    let text = if high_contrast {
                        egui::Color32::PLACEHOLDER
                    } else if selected {
                        theme::on_highlight()
                    } else {
                        Tokens::text()
                    };
                    let mut label = egui::text::LayoutJob::default();
                    label.wrap.max_width = (width - 32.0 - ui.spacing().icon_spacing).max(1.0);
                    label.append(
                        &tr(title),
                        0.0,
                        egui::TextFormat {
                            font_id: theme::bold(15.0),
                            color: text,
                            ..Default::default()
                        },
                    );
                    label.append(
                        &format!("\n{}", tr(description)),
                        0.0,
                        egui::TextFormat {
                            font_id: egui::FontId::proportional(13.0),
                            color: if selected || high_contrast {
                                text
                            } else {
                                Tokens::text_dim()
                            },
                            line_height: Some(20.0),
                            ..Default::default()
                        },
                    );
                    let label = ui.painter().layout_job(label);
                    let height = (label.size().y + 28.0).max(92.0);
                    let button = egui::Button::new((label, egui::Atom::grow()))
                        .selected(selected)
                        .corner_radius(8.0)
                        .wrap();
                    if ui.add_sized([width, height], button).clicked() {
                        app.set_workspace(preset);
                        app.save_settings();
                    }
                }
            });
        }
    });
    ui.add_space(24.0);
    section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(tr("options.language")).font(theme::bold(16.0)));
        ui.add_space(10.0);
        let current = crate::i18n::language();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for (code, name) in crate::i18n::LANGUAGES {
                if ui.selectable_label(current == code, name).clicked() {
                    crate::i18n::set_language(code);
                    app.settings.language = code.to_string();
                    app.settings.save();
                }
            }
        });
    });
}

/// The original release headings and persistent disclosure state also serve
/// Help > What's New and its interaction checks.
pub(super) fn news(ui: &mut Ui) -> Option<String> {
    let mut open = None;
    section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.visuals_mut().collapsing_header_frame = true;
        ui.visuals_mut().hyperlink_color = theme::link_color();
        ui.visuals_mut().widgets.inactive.weak_bg_fill = Tokens::panel();
        ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
        ui.visuals_mut().widgets.hovered.weak_bg_fill = Tokens::hovered_row();
        ui.spacing_mut().button_padding = egui::vec2(10.0, 10.0);
        for (index, release) in RELEASES.iter().take(HISTORY_LEN).enumerate() {
            if index > 0 {
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
            }
            let mut heading = egui::text::LayoutJob::single_section(
                news_heading(release),
                egui::TextFormat {
                    font_id: theme::bold(14.0),
                    color: if theme::theme() == theme::Theme::HighContrast {
                        egui::Color32::PLACEHOLDER
                    } else {
                        Tokens::text()
                    },
                    ..Default::default()
                },
            );
            heading.wrap.max_width = (ui.available_width() - ui.spacing().indent - 20.0).max(1.0);
            let heading = ui.painter().layout_job(heading);
            egui::CollapsingHeader::new(heading)
                .id_salt(news_row_id(release.version))
                .default_open(index == 0)
                .show(ui, |ui| {
                    let width = ui.available_width().min(860.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(width);
                            ui.add_space(6.0);
                            ui.add(
                                egui::Label::new(RichText::new(tr(release.body)).size(13.0)).wrap(),
                            );
                            ui.add_space(8.0);
                            let url = release_page(release.version);
                            if ui
                                .link(tr("welcome.news.more"))
                                .on_hover_text(&url)
                                .clicked()
                            {
                                open = Some(url);
                            }
                            ui.add_space(6.0);
                        },
                    );
                });
        }
    });
    open
}

pub(super) fn learn(ui: &mut Ui) {
    section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        for (index, (tool, title, body)) in [
            (
                Tool::Freehand,
                "welcome.learn_1_title",
                "welcome.learn_1_body",
            ),
            (Tool::Pick, "welcome.learn_2_title", "welcome.learn_2_body"),
            (
                Tool::InteractiveFill,
                "welcome.learn_3_title",
                "welcome.learn_3_body",
            ),
            (Tool::Shape, "welcome.learn_4_title", "welcome.learn_4_body"),
            (Tool::Blend, "welcome.learn_5_title", "welcome.learn_5_body"),
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(14.0);
            }
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let (icon, _) =
                    ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                icons::draw_on(
                    ui.painter(),
                    icon.shrink(2.0),
                    tool,
                    theme::link_color(),
                    Tokens::panel(),
                );
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(tr(title)).font(theme::bold(16.0))).wrap(),
                    );
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(tr(body)).size(13.0).color(Tokens::text_dim()),
                        )
                        .wrap(),
                    );
                });
            });
        }
    });
}

/// Read the template folder on entry or when returning from another window.
/// The cached list avoids directory reads while this page is being drawn.
fn user_templates(
    ui: &Ui,
    dir: Option<std::path::PathBuf>,
) -> Arc<Vec<crate::templates::Template>> {
    let id = egui::Id::new(("welcome_user_templates", &dir));
    let frame = ui.ctx().cumulative_frame_nr();
    let focused = ui.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::WindowFocused(true)))
    });
    let cached = ui
        .ctx()
        .data(|data| data.get_temp::<(u64, Arc<Vec<crate::templates::Template>>)>(id));
    let list = match cached {
        Some((last, list)) if frame.saturating_sub(last) <= 1 && (!focused || frame == last) => {
            list
        }
        _ => Arc::new(crate::templates::user_templates(dir)),
    };
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, (frame, Arc::clone(&list))));
    list
}

pub(super) fn templates(app: &mut App, ui: &mut Ui) {
    section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("welcome.templates")).font(theme::bold(18.0)));
            if ui
                .add(
                    egui::Button::new(tr("tpl.more"))
                        .wrap()
                        .min_size(egui::vec2(0.0, 32.0)),
                )
                .clicked()
            {
                app.open_template_dialog();
            }
        });
        ui.add_space(14.0);
        let list = crate::templates::built_in();
        if let Some(index) = crate::ui::template_dialog::strip(ui, &list) {
            if let Some(template) = list.get(index) {
                let name = app.next_untitled_name();
                if let Some(document) = crate::templates::document(template, &name) {
                    app.open_document(document, None);
                }
            }
        }
    });
    ui.add_space(20.0);
    section_frame().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("welcome.my_templates")).font(theme::bold(18.0)));
            if ui
                .add(
                    egui::Button::new(tr("welcome.open_template"))
                        .wrap()
                        .min_size(egui::vec2(0.0, 32.0)),
                )
                .clicked()
            {
                crate::files::Dialog::new()
                    .add_filter(tr("file.template"), &["tdt", "tdraw"])
                    .pick_file(app, |app, path| app.open_as_template(path));
            }
        });
        ui.add_space(12.0);
        let list = user_templates(ui, crate::ui::template_dialog::user_dir());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for template in list.iter() {
                if let crate::templates::Source::File(path) = &template.source {
                    if ui.add(egui::Button::new(&template.name).wrap()).clicked() {
                        app.open_as_template(path.clone());
                    }
                }
            }
        });
        if let Some(dir) = crate::ui::template_dialog::user_dir() {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(trf(
                        "welcome.templates_folder",
                        &[("d", &dir.display().to_string())],
                    ))
                    .color(Tokens::text_dim())
                    .size(12.0),
                )
                .wrap(),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_frame(
        ctx: &egui::Context,
        app: &mut App,
        page: fn(&mut App, &mut Ui),
        events: Vec<egui::Event>,
    ) -> Vec<egui::epaint::ClippedShape> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 2400.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| page(app, ui));
            },
        );
        output.textures_delta.clear();
        output.shapes
    }

    fn text_shape<'a>(
        shapes: &'a [egui::epaint::ClippedShape],
        label: &str,
    ) -> &'a egui::epaint::ClippedShape {
        shapes
            .iter()
            .find(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == label)
            })
            .expect("the complete control label")
    }

    fn assert_text_ink(
        ctx: &egui::Context,
        shapes: &[egui::epaint::ClippedShape],
        label: &str,
        ink: egui::Color32,
    ) {
        let painted = ctx.tessellate(vec![text_shape(shapes, label).clone()], 1.0);
        let colors: Vec<_> = painted
            .iter()
            .filter_map(|primitive| match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => Some(&mesh.vertices),
                _ => None,
            })
            .flatten()
            .map(|vertex| vertex.color)
            .collect();
        assert!(!colors.is_empty(), "the label has rendered glyphs");
        assert!(
            colors.iter().all(|color| *color == ink),
            "{label:?}: every glyph uses {ink:?}, got {colors:?}"
        );
    }

    /// High contrast text follows real keyboard focus and pointer states,
    /// including descriptions inside an unselected workspace card.
    #[test]
    fn high_contrast_controls_keep_readable_text_when_focused_or_pressed() {
        crate::i18n::with_thread_language("en", || {
            let pages: [(fn(&mut App, &mut Ui), String); 2] = [
                (
                    workspace,
                    format!(
                        "{}\n{}",
                        tr("menu.window.workspace_lite"),
                        tr("welcome.ws_lite_desc")
                    ),
                ),
                (
                    |_, ui| {
                        let _ = news(ui);
                    },
                    news_heading(RELEASES.first().expect("a release")),
                ),
            ];
            for (page, label) in pages {
                let mut app = App::headless();
                app.workspace = Workspace::Default;
                theme::set_theme(theme::Theme::HighContrast);
                let ctx = theme::ui_context();
                App::setup_context(&ctx);
                let mut shapes = Vec::new();
                for _ in 0..3 {
                    shapes = page_frame(&ctx, &mut app, page, Vec::new());
                }
                assert_text_ink(&ctx, &shapes, &label, Tokens::text());
                let mut focused = None;
                for _ in 0..16 {
                    let tab = |pressed| egui::Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    };
                    page_frame(&ctx, &mut app, page, vec![tab(true)]);
                    page_frame(&ctx, &mut app, page, vec![tab(false)]);
                    shapes = page_frame(&ctx, &mut app, page, Vec::new());
                    let egui::Shape::Text(text) = &text_shape(&shapes, &label).shape else {
                        unreachable!("a text shape")
                    };
                    focused = ctx
                        .memory(|memory| memory.focused())
                        .and_then(|id| ctx.read_response(id))
                        .filter(|response| {
                            response.has_focus()
                                && response.rect.contains(text.visual_bounding_rect().center())
                        });
                    if focused.is_some() {
                        break;
                    }
                }
                let focused = focused.expect("Tab reaches the control");
                assert_text_ink(&ctx, &shapes, &label, theme::on_highlight());
                let at = focused.rect.center();
                page_frame(
                    &ctx,
                    &mut app,
                    page,
                    vec![
                        egui::Event::PointerMoved(at),
                        egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
                shapes = page_frame(&ctx, &mut app, page, Vec::new());
                assert!(ctx
                    .read_response(focused.id)
                    .expect("the pressed control")
                    .is_pointer_button_down_on());
                assert_text_ink(&ctx, &shapes, &label, theme::on_highlight());
            }
        });
        theme::set_theme(theme::Theme::Light);
    }

    #[test]
    fn news_paragraphs_use_readable_width_and_remain_visible() {
        for language in ["en", "zh-CN"] {
            crate::i18n::with_thread_language(language, || {
                for width in [1200.0, 1600.0] {
                    let ctx = crate::theme::ui_context();
                    ctx.all_styles_mut(|style| style.animation_time = 0.0);
                    let mut texts = Vec::new();
                    for _ in 0..3 {
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 2400.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                egui::CentralPanel::default().show(ui, |ui| {
                                    let _ = news(ui);
                                });
                            },
                        );
                        texts = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) => {
                                    Some((Arc::clone(&text.galley), text.pos, shape.clip_rect))
                                }
                                _ => None,
                            })
                            .collect();
                        output.textures_delta.clear();
                    }
                    let release = RELEASES.first().expect("a release");
                    let body = tr(release.body);
                    let (galley, position, clip) = texts
                        .iter()
                        .find(|(galley, _, _)| galley.text() == body)
                        .expect("the complete newest release paragraph");
                    assert!(
                        galley.rect.width() <= 860.5,
                        "{language}, screen {width}: paragraph width {}",
                        galley.rect.width()
                    );
                    assert!(!galley.elided, "{language}: the paragraph is complete");
                    assert!(
                        clip.expand(0.5)
                            .contains_rect(galley.rect.translate(position.to_vec2())),
                        "{language}, screen {width}: the whole paragraph is visible"
                    );
                    for release in RELEASES.iter().take(HISTORY_LEN) {
                        let heading = news_heading(release);
                        let (galley, position, clip) = texts
                            .iter()
                            .find(|(galley, _, _)| galley.text() == heading)
                            .expect("each release keeps its complete heading");
                        assert!(
                            clip.expand(0.5)
                                .contains_rect(galley.rect.translate(position.to_vec2())),
                            "{language}, screen {width}: release {} is visible",
                            release.version
                        );
                    }
                }
            });
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn user_template_cache_reuses_frames_and_refreshes_on_entry_or_focus() {
        let temporary = std::env::temp_dir()
            .canonicalize()
            .expect("the temporary folder");
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a current timestamp")
            .as_nanos();
        let dir = temporary.join(format!(
            "tracedraw-welcome-template-cache-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&dir).expect("create the test template folder");
        std::fs::write(dir.join("first.tdt"), []).expect("the first template");
        let ctx = crate::theme::ui_context();
        ctx.options_mut(|options| {
            options.max_passes = 2.try_into().expect("two layout passes");
        });
        let draw = |events| {
            let mut passes = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    passes.push(user_templates(ui, Some(dir.clone())));
                    if passes.len() == 1 {
                        ui.ctx().request_discard("template cache multipass check");
                    }
                },
            );
            output.textures_delta.clear();
            assert!(passes.len() >= 2, "the frame ran multiple layout passes");
            let first = passes.first().expect("the first layout pass");
            for list in &passes {
                assert!(Arc::ptr_eq(first, list), "layout passes share the cache");
            }
            Arc::clone(first)
        };

        let first = draw(Vec::new());
        assert_eq!(first.len(), 1);
        std::fs::write(dir.join("second.tdt"), []).expect("the second template");
        let consecutive = draw(Vec::new());
        assert!(Arc::ptr_eq(&first, &consecutive));
        assert_eq!(consecutive.len(), 1, "drawing again keeps the cached list");

        let mut other_page = ctx.run_ui(Default::default(), |_| {});
        other_page.textures_delta.clear();
        let reentered = draw(Vec::new());
        assert!(!Arc::ptr_eq(&first, &reentered));
        assert_eq!(
            reentered
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );

        std::fs::write(dir.join("third.tdt"), []).expect("the third template");
        let focused = draw(vec![egui::Event::WindowFocused(true)]);
        assert!(!Arc::ptr_eq(&reentered, &focused));
        assert_eq!(
            focused.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["first", "second", "third"]
        );

        let resolved = dir.canonicalize().expect("the test template folder");
        assert_eq!(resolved.parent(), Some(temporary.as_path()));
        std::fs::remove_dir_all(resolved).expect("remove the test template folder");
    }

    #[test]
    fn welcome_pages_fit_narrow_and_wide_content() {
        let pages: [(&str, fn(&mut App, &mut Ui)); 4] = [
            ("workspace", workspace),
            ("news", |_, ui| {
                let _ = news(ui);
            }),
            ("learn", |_, ui| learn(ui)),
            ("templates", templates),
        ];
        for language in ["en", "de", "fr"] {
            crate::i18n::with_thread_language(language, || {
                for width in [260.0, 360.0, 640.0, 1200.0] {
                    for (name, page) in pages {
                        let ctx = crate::theme::ui_context();
                        let mut app = App::headless();
                        let mut right = 0.0;
                        for _ in 0..3 {
                            let mut output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(width, 2400.0),
                                    )),
                                    ..Default::default()
                                },
                                |ui| {
                                    egui::CentralPanel::default().show(ui, |ui| {
                                        page(&mut app, ui);
                                        right = ui.min_rect().right();
                                    });
                                },
                            );
                            output.textures_delta.clear();
                        }
                        assert!(
                            right <= width + 0.5,
                            "{language}, {name}, width {width}: content ends at {right}"
                        );
                    }
                }
            });
        }
    }
}
