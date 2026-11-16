use super::*;
use crate::soft_canvas::{Canvas, Textures};
use crate::theme::Theme;
use egui::{Pos2, RawInput, Rect, Vec2};
use std::path::{Path, PathBuf};
use tracedraw_core::{geometry::Rect as MmRect, Color, Fill, ShapeKind};

fn recent_drawings(out: &Path) -> Vec<PathBuf> {
    let folder = out.join("recent-documents");
    std::fs::create_dir_all(&folder).expect("recent drawings folder");
    let mut paths = Vec::new();
    for (name, color) in [
        ("Poster-study.tdraw", Color::rgb8(47, 111, 208)),
        ("Card-layout.tdraw", Color::rgb8(33, 145, 160)),
        ("Shape-study.tdraw", Color::rgb8(72, 104, 180)),
    ] {
        let mut app = App::headless();
        for kind in [
            ShapeKind::Rect {
                rect: MmRect::new(24.0, 32.0, 180.0, 100.0),
                radius: 8.0,
                corners: None,
            },
            ShapeKind::Ellipse {
                rect: MmRect::new(70.0, 130.0, 140.0, 200.0),
                arc: None,
            },
        ] {
            let id = app.new_shape(kind).expect("sample shape");
            app.select(vec![id]);
            app.apply_fill(Fill::Solid(color));
        }
        let path = folder.join(name);
        std::fs::write(&path, app.doc().to_json().expect("drawing JSON")).expect("sample drawing");
        app.store_thumbnail(&path);
        crate::file_presence::exists(&path, None);
        paths.push(path);
    }
    // The files are local and metadata arrives through the normal worker.
    for _ in 0..100 {
        if paths
            .iter()
            .all(|path| crate::file_presence::info(path).is_some())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    paths
}

fn save_page(path: &Path, tab: WelcomeTab, theme: Theme, size: Vec2, recent: &[PathBuf]) {
    let mut app = App::headless();
    app.show_welcome = true;
    app.welcome_tab = tab;
    app.settings.language = "en".into();
    app.settings.theme = theme.id().into();
    app.settings.recent_files = recent.to_vec();
    crate::i18n::set_language("en");
    crate::ui::phone::set_phone(size.x.min(size.y) < crate::ui::phone::PHONE_DP);

    let ctx = crate::theme::ui_context();
    App::setup_context(&ctx);
    ctx.set_zoom_factor(1.0);
    let mut textures = Textures::default();
    for frame in 0..6 {
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(frame as f64 * 0.3),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(1.0);
        let mut out = ctx.run_ui(input, |ui| {
            crate::ui::apply_theme(ui.ctx(), &app.settings.theme);
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(Tokens::panel()))
                .show(ui, |ui| welcome_screen(&mut app, ui));
        });
        textures.apply(&out.textures_delta);
        if frame == 5 {
            let primitives = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
            let mut canvas = Canvas::new(size.x as usize, size.y as usize);
            canvas.paint(&textures, &primitives);
            canvas.save(path, (0, 0, canvas.w, canvas.h));
        }
        for id in &out.textures_delta.free {
            textures.0.remove(id);
        }
        out.textures_delta.clear();
    }
}

/// Export all welcome pages into WELCOME_SHOTS for visual review.
#[test]
#[ignore]
fn welcome_page_shots() {
    let Some(out) = std::env::var_os("WELCOME_SHOTS") else {
        return;
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("welcome screenshots folder");
    crate::i18n::with_thread_language("en", || {
        let recent = recent_drawings(&out);
        for (tag, theme, size) in [
            ("light", Theme::Light, egui::vec2(1600.0, 900.0)),
            ("dark", Theme::Dark, egui::vec2(1600.0, 900.0)),
            ("narrow", Theme::Light, egui::vec2(360.0, 800.0)),
        ] {
            for (tab, _) in TABS {
                let name = match tab {
                    WelcomeTab::GetStarted => "get-started",
                    WelcomeTab::Workspace => "workspace",
                    WelcomeTab::News => "news",
                    WelcomeTab::Learn => "learn",
                    WelcomeTab::Templates => "templates",
                };
                let path = out.join(format!("welcome-{name}-{tag}.png"));
                save_page(&path, tab, theme, size, &recent);
                eprintln!("{}", path.display());
            }
        }
    });
    crate::ui::phone::set_phone(false);
    crate::theme::set_theme(Theme::Light);
}
