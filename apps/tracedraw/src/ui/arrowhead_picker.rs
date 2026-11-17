//! Graphical arrowhead choices for the property bar.

use crate::i18n::{tr, trf};
use crate::theme::Tokens;
use crate::ui::tips::RichTip;
use egui::{Color32, Id, Rect, Sense, Ui, Vec2};
use tracedraw_core::geometry::{BezPath, Shape as _};
use tracedraw_core::Arrowhead;

const CELL: Vec2 = egui::vec2(54.0, 40.0);
const GAP: f32 = 4.0;
const COLUMNS: usize = 4;
const WIDTH: f32 = CELL.x * COLUMNS as f32 + GAP * (COLUMNS - 1) as f32;

#[derive(Clone)]
struct Preview {
    head: Arrowhead,
    start: bool,
    color: Color32,
    size: [usize; 2],
    texture: egui::TextureHandle,
}

pub(super) fn name(head: &Arrowhead) -> String {
    tr(match head {
        Arrowhead::None => "arrowhead_picker.none",
        Arrowhead::Arrow => "arrowhead_picker.arrow",
        Arrowhead::OpenArrow => "arrowhead_picker.open_arrow",
        Arrowhead::Circle => "arrowhead_picker.circle",
        Arrowhead::Square => "arrowhead_picker.square",
        Arrowhead::Bar => "arrowhead_picker.bar",
        Arrowhead::Diamond => "arrowhead_picker.diamond",
        Arrowhead::Triangle => "arrowhead_picker.triangle",
        Arrowhead::NarrowTriangle => "arrowhead_picker.narrow_triangle",
        Arrowhead::WideTriangle => "arrowhead_picker.wide_triangle",
        Arrowhead::Stealth => "arrowhead_picker.stealth",
        Arrowhead::LongStealth => "arrowhead_picker.long_stealth",
        Arrowhead::DoubleArrow => "arrowhead_picker.double_arrow",
        Arrowhead::TripleArrow => "arrowhead_picker.triple_arrow",
        Arrowhead::Chevron => "arrowhead_picker.chevron",
        Arrowhead::DoubleChevron => "arrowhead_picker.double_chevron",
        Arrowhead::HalfArrowLeft => "arrowhead_picker.half_arrow_left",
        Arrowhead::HalfArrowRight => "arrowhead_picker.half_arrow_right",
        Arrowhead::Harpoon => "arrowhead_picker.harpoon",
        Arrowhead::ReverseArrow => "arrowhead_picker.reverse_arrow",
        Arrowhead::Tail => "arrowhead_picker.tail",
        Arrowhead::DoubleTail => "arrowhead_picker.double_tail",
        Arrowhead::CircleOpen => "arrowhead_picker.circle_open",
        Arrowhead::SquareOpen => "arrowhead_picker.square_open",
        Arrowhead::DiamondOpen => "arrowhead_picker.diamond_open",
        Arrowhead::TriangleOpen => "arrowhead_picker.triangle_open",
        Arrowhead::BarDouble => "arrowhead_picker.bar_double",
        Arrowhead::Slash => "arrowhead_picker.slash",
        Arrowhead::Cross => "arrowhead_picker.cross",
        Arrowhead::Star => "arrowhead_picker.star",
        Arrowhead::Hexagon => "arrowhead_picker.hexagon",
        Arrowhead::Bracket => "arrowhead_picker.bracket",
        Arrowhead::Custom { name, .. } => return name.clone(),
    })
}

/// The preview uses the same geometry as the document, including custom
/// heads. Rasterise only when its head, end, colour or pixel size changes.
fn preview_image(
    head: &Arrowhead,
    start: bool,
    size: [usize; 2],
    color: Color32,
) -> Option<egui::ColorImage> {
    let mut line = BezPath::new();
    line.move_to((0.0, 0.0));
    line.line_to((28.0, 0.0));
    let stroke = tracedraw_core::Stroke {
        width: 2.0,
        start_arrow: if start { head.clone() } else { Arrowhead::None },
        end_arrow: if start { Arrowhead::None } else { head.clone() },
        ..Default::default()
    };
    let heads = tracedraw_core::arrowhead_paths(&line, &stroke);
    let shaft = tracedraw_core::arrowhead_shaft_path(&line, &stroke, 1.0);
    let bounds = heads
        .iter()
        .fold(line.bounding_box().inflate(0.0, 5.0), |b, h| {
            b.union(h.bounding_box())
        });
    if !bounds.is_finite() {
        return None;
    }
    let scale = ((size[0] as f64 - 2.0) / bounds.width().max(1.0))
        .min((size[1] as f64 - 2.0) / bounds.height().max(1.0));
    let transform = tiny_skia::Transform::from_row(
        scale as f32,
        0.0,
        0.0,
        -scale as f32,
        (size[0] as f64 * 0.5 - bounds.center().x * scale) as f32,
        (size[1] as f64 * 0.5 + bounds.center().y * scale) as f32,
    );
    let mut pixels = tiny_skia::Pixmap::new(size[0] as u32, size[1] as u32)?;
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(color.r(), color.g(), color.b(), color.a());
    paint.anti_alias = true;
    if let Some(path) = tracedraw_render::to_sk_path(&shaft) {
        pixels.stroke_path(
            &path,
            &paint,
            &tiny_skia::Stroke {
                width: stroke.width as f32,
                ..Default::default()
            },
            transform,
            None,
        );
    }
    for head in heads {
        if let Some(path) = tracedraw_render::to_sk_path(&head) {
            pixels.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
        }
    }
    Some(egui::ColorImage::from_rgba_premultiplied(
        size,
        pixels.data(),
    ))
}

fn paint_preview(ui: &Ui, id: Id, rect: Rect, head: &Arrowhead, start: bool) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let color = if ui.is_enabled() {
        Tokens::icon()
    } else {
        Tokens::icon_off()
    };
    let ppp = ui.ctx().pixels_per_point();
    let size = [
        (rect.width() * ppp).ceil().max(1.0) as usize,
        (rect.height() * ppp).ceil().max(1.0) as usize,
    ];
    let id = id.with("arrowhead_preview");
    let cached = ui.data(|d| d.get_temp::<Preview>(id));
    let cached = cached
        .filter(|p| p.head == *head && p.start == start && p.color == color && p.size == size);
    let preview = cached.or_else(|| {
        let image = preview_image(head, start, size, color)?;
        let p = Preview {
            head: head.clone(),
            start,
            color,
            size,
            texture: ui
                .ctx()
                .load_texture("arrowhead_picker", image, egui::TextureOptions::LINEAR),
        };
        ui.data_mut(|d| d.insert_temp(id, p.clone()));
        Some(p)
    });
    if let Some(preview) = preview {
        ui.painter().image(
            preview.texture.id(),
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

pub(super) fn show(
    ui: &mut Ui,
    id: &str,
    current: &Arrowhead,
    shown: bool,
    start: bool,
    custom: &[Arrowhead],
) -> Option<Arrowhead> {
    let button_id = ui.make_persistent_id(id);
    let popup_id = button_id.with("popup");
    let (_, rect) = ui.allocate_space(egui::vec2(56.0, ui.spacing().interact_size.y));
    let response = ui.interact(rect, button_id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let visuals = if open {
        &ui.visuals().widgets.open
    } else {
        ui.style().interact(&response)
    };
    ui.painter().rect(
        rect,
        2.0,
        visuals.bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let icon_x = rect.right() - 9.0;
    let cy = rect.center().y;
    ui.painter().add(egui::Shape::line(
        vec![
            egui::pos2(icon_x - 3.0, cy - 1.5),
            egui::pos2(icon_x, cy + 1.5),
            egui::pos2(icon_x + 3.0, cy - 1.5),
        ],
        egui::Stroke::new(1.0, Tokens::icon()),
    ));
    if shown {
        let preview = Rect::from_min_max(
            rect.min + egui::vec2(5.0, 3.0),
            egui::pos2(rect.right() - 18.0, rect.bottom() - 3.0),
        );
        paint_preview(ui, button_id, preview, current, start);
    }
    let label = tr(if start {
        "toolbar.start_arrow"
    } else {
        "toolbar.end_arrow"
    });
    response.widget_info(|| {
        let mut info =
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), &label);
        info.current_text_value = shown.then(|| name(current));
        info
    });
    let mut picked = None;
    egui::Popup::menu(&response).width(WIDTH + 12.0).show(|ui| {
        ui.set_width(WIDTH);
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .show(ui, |ui| {
                egui::Grid::new("arrowhead_grid")
                    .num_columns(COLUMNS)
                    .spacing(egui::vec2(GAP, GAP))
                    .show(ui, |ui| {
                        for (i, head) in Arrowhead::ALL.iter().chain(custom).enumerate() {
                            let selected = shown && *head == *current;
                            let response =
                                ui.add_sized(CELL, egui::Button::new("").selected(selected));
                            let mut label = name(head);
                            if label.is_empty() {
                                label = trf(
                                    "docker.custom_arrowhead_n",
                                    &[("n", &(i + 1 - Arrowhead::ALL.len()).to_string())],
                                );
                            }
                            crate::ui::keynav::note_list_choice(
                                ui.ctx(),
                                response.id,
                                label.clone(),
                                selected,
                            );
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    selected,
                                    &label,
                                )
                            });
                            paint_preview(
                                ui,
                                response.id,
                                response.rect.shrink2(egui::vec2(8.0, 9.0)),
                                head,
                                start,
                            );
                            if response.on_hover_text(label).clicked() {
                                picked = Some(head.clone());
                                ui.close();
                            }
                            if (i + 1) % COLUMNS == 0 {
                                ui.end_row();
                            }
                        }
                    });
                crate::ui::keynav::scroll_into_view(ui);
            });
    });
    crate::ui::keynav::list_button_keys(ui.ctx(), &response, popup_id);
    response.rich_tip(crate::ui::tips::keyed(if start {
        "toolbar.start_arrow"
    } else {
        "toolbar.end_arrow"
    }));
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Affine;

    fn save_canvas(canvas: &crate::soft_canvas::Canvas, path: &std::path::Path) {
        let image = image::RgbaImage::from_fn(canvas.w as u32, canvas.h as u32, |x, y| {
            let pixel = canvas.px[y as usize * canvas.w + x as usize];
            let alpha = pixel[3];
            let channel = |v: f32| {
                if alpha > 0.0 {
                    (v / alpha * 255.0).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                }
            };
            image::Rgba([
                channel(pixel[0]),
                channel(pixel[1]),
                channel(pixel[2]),
                (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
            ])
        });
        image.save(path).expect("screenshot PNG");
    }

    #[test]
    fn the_preview_draws_every_head_from_the_document_geometry() {
        for head in Arrowhead::ALL {
            for start in [true, false] {
                let image = preview_image(&head, start, [56, 32], Color32::BLACK).expect("preview");
                assert!(image.pixels.iter().any(|p| p.a() != 0), "{head:?}");
                if head != Arrowhead::None {
                    let none = preview_image(&Arrowhead::None, start, [56, 32], Color32::BLACK)
                        .expect("line");
                    assert_ne!(image.pixels, none.pixels, "{head:?}");
                }
            }
        }
    }

    /// An asymmetric head rotates with the curve's outward direction at
    /// the start; a horizontal mirror alone puts its two sides backwards.
    #[test]
    fn an_asymmetric_custom_head_rotates_at_the_start_instead_of_mirroring() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((-1.0, 0.5));
        path.line_to((-1.0, -0.5));
        path.line_to((-0.5, -0.5));
        path.line_to((-0.5, -0.2));
        path.close_path();
        let head = Arrowhead::Custom {
            path,
            name: "Asymmetric".into(),
        };
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((28.0, 0.0));
        let end = tracedraw_core::arrowhead_paths(
            &line,
            &tracedraw_core::Stroke {
                end_arrow: head.clone(),
                ..Default::default()
            },
        );
        let start = tracedraw_core::arrowhead_paths(
            &line,
            &tracedraw_core::Stroke {
                start_arrow: head,
                ..Default::default()
            },
        );
        assert_eq!(start.len(), 1);
        assert_eq!(end.len(), 1);
        let rotated = Affine::scale(-1.0) * Affine::translate((-28.0, 0.0)) * end[0].clone();
        assert_eq!(start[0], rotated, "both axes follow the outward direction");
        let mirrored =
            Affine::scale_non_uniform(-1.0, 1.0) * Affine::translate((-28.0, 0.0)) * end[0].clone();
        assert_ne!(
            start[0], mirrored,
            "the asymmetric sides must not be reflected"
        );
    }

    #[test]
    fn unchanged_arrowhead_previews_reuse_their_textures() {
        let ctx = crate::theme::ui_context();
        for start in [true, false] {
            for frame in 0..3 {
                let mut out = ctx.run_ui(Default::default(), |ui| {
                    show(ui, "cached_head", &Arrowhead::Diamond, true, start, &[]);
                });
                if frame == 0 {
                    assert!(
                        !out.textures_delta.set.is_empty(),
                        "a new direction updates the preview"
                    );
                } else if frame == 2 {
                    assert!(
                        out.textures_delta.set.is_empty(),
                        "no repeated texture uploads"
                    );
                }
                out.textures_delta.clear();
            }
        }
    }

    /// Produce the actual popup in both themes and directions for visual
    /// review. Set ARROWHEAD_SHOTS to the output directory.
    #[test]
    #[ignore]
    fn arrowhead_picker_screenshots() {
        let Some(dir) = std::env::var_os("ARROWHEAD_SHOTS") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("screenshot directory");
        crate::i18n::set_language("en");
        for theme in [crate::theme::Theme::Light, crate::theme::Theme::Dark] {
            crate::theme::set_theme(theme);
            for start in [true, false] {
                let ctx = crate::theme::ui_context();
                crate::app::App::setup_context(&ctx);
                ctx.set_visuals(crate::theme::visuals());
                let mut textures = crate::soft_canvas::Textures::default();
                let mut button = Id::NULL;
                let mut screenshot = None;
                for frame in 0..5 {
                    if frame == 1 {
                        egui::Popup::open_id(&ctx, button.with("popup"));
                    }
                    let mut out = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(320.0, 360.0),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            ui.horizontal(|ui| {
                                button = ui.make_persistent_id("shot");
                                show(ui, "shot", &Arrowhead::Arrow, true, start, &[]);
                            });
                        },
                    );
                    textures.apply(&out.textures_delta);
                    let shapes = std::mem::take(&mut out.shapes);
                    let primitives = ctx.tessellate(shapes, out.pixels_per_point);
                    let mut canvas = crate::soft_canvas::Canvas::new(320, 360);
                    canvas.paint(&textures, &primitives);
                    out.textures_delta.clear();
                    screenshot = Some(canvas);
                }
                save_canvas(
                    &screenshot.expect("rendered popup"),
                    &dir.join(format!(
                        "arrowheads_{}_{}.png",
                        theme.id(),
                        if start { "start" } else { "end" }
                    )),
                );
            }
        }
        let ctx = crate::theme::ui_context();
        crate::app::App::setup_context(&ctx);
        ctx.set_visuals(crate::theme::visuals());
        let mut textures = crate::soft_canvas::Textures::default();
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 640.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.painter()
                        .rect_filled(ui.max_rect(), 0.0, ui.visuals().window_fill);
                    egui::Grid::new("catalog_review")
                        .num_columns(4)
                        .spacing(egui::vec2(8.0, 8.0))
                        .show(ui, |ui| {
                            for (i, head) in Arrowhead::ALL.iter().enumerate() {
                                ui.vertical(|ui| {
                                    ui.set_width(184.0);
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(160.0, 40.0),
                                        Sense::hover(),
                                    );
                                    paint_preview(ui, Id::new(("catalog", i)), rect, head, false);
                                    ui.label(name(head));
                                });
                                if (i + 1) % 4 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                },
            );
            textures.apply(&out.textures_delta);
            let shapes = std::mem::take(&mut out.shapes);
            let primitives = ctx.tessellate(shapes, out.pixels_per_point);
            let mut canvas = crate::soft_canvas::Canvas::new(800, 640);
            canvas.paint(&textures, &primitives);
            save_canvas(&canvas, &dir.join("arrowhead_catalog.png"));
            out.textures_delta.clear();
        }
        crate::theme::set_theme(crate::theme::Theme::Light);
    }
}
