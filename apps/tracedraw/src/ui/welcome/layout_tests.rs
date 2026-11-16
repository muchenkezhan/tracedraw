use super::*;
use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

const TABS: [(WelcomeTab, &str); 5] = [
    (WelcomeTab::GetStarted, "welcome.get_started"),
    (WelcomeTab::Workspace, "welcome.workspace"),
    (WelcomeTab::News, "welcome.whats_new"),
    (WelcomeTab::Learn, "welcome.learn"),
    (WelcomeTab::Templates, "welcome.templates"),
];

struct Snapshot {
    controls: Vec<(String, Rect)>,
    texts: Vec<(String, Rect)>,
    panel: Rect,
    shell: Option<Rect>,
    cursor: egui::CursorIcon,
}

impl Snapshot {
    fn target(&self, label: &str) -> Option<Rect> {
        self.controls
            .iter()
            .filter(|(name, _)| name == label || name.lines().next() == Some(label))
            .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
            .map(|(_, rect)| *rect)
            .or_else(|| {
                self.texts
                    .iter()
                    .filter(|(name, _)| name == label)
                    .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
                    .map(|(_, rect)| *rect)
            })
    }

    fn contains_text(&self, label: &str) -> bool {
        self.texts.iter().any(|(text, _)| text == label)
    }

    fn tab_geometry(&self) -> [(Rect, Rect); 5] {
        TABS.map(|(_, key)| {
            let label = tr(key);
            let topmost = |items: &[(String, Rect)]| {
                items
                    .iter()
                    .filter(|(text, _)| text == &label)
                    .min_by(|a, b| a.1.top().total_cmp(&b.1.top()))
                    .map(|(_, rect)| *rect)
                    .unwrap_or_else(|| panic!("visible tab {label:?}"))
            };
            (topmost(&self.controls), topmost(&self.texts))
        })
    }

    fn assert_horizontal_bounds(&self, width: f32, context: &str) {
        for (label, rect) in self.controls.iter().chain(&self.texts) {
            assert!(
                rect.left() >= -1.0 && rect.right() <= width + 1.0,
                "{context}: {label:?} extends beyond the window at {rect:?}"
            );
        }
    }
}

struct Driver {
    ctx: egui::Context,
    app: App,
    size: Vec2,
    time: f64,
}

impl Driver {
    fn new(size: Vec2) -> Self {
        crate::ui::phone::set_phone(false);
        crate::theme::set_theme(crate::theme::Theme::Light);
        let ctx = crate::theme::ui_context();
        App::setup_context(&ctx);
        ctx.enable_accesskit();
        ctx.set_zoom_factor(1.0);
        let mut app = App::headless();
        app.show_welcome = true;
        app.welcome_tab = WelcomeTab::GetStarted;
        app.settings.recent_files.clear();
        Self {
            ctx,
            app,
            size,
            time: 0.0,
        }
    }

    fn frame(&mut self, events: Vec<Event>) -> Snapshot {
        self.time += 0.3;
        let mut snapshot = Snapshot {
            controls: Vec::new(),
            texts: Vec::new(),
            panel: Rect::NOTHING,
            shell: None,
            cursor: egui::CursorIcon::Default,
        };
        let mut out = self.ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ui| {
                snapshot.panel = egui::CentralPanel::default()
                    .show(ui, |ui| welcome_screen(&mut self.app, ui))
                    .response
                    .rect;
            },
        );
        out.textures_delta.clear();
        snapshot.cursor = out.platform_output.cursor_icon;
        if let Some(update) = out.platform_output.accesskit_update {
            snapshot.controls = update
                .nodes
                .into_iter()
                .filter_map(|(_, node)| {
                    if !matches!(
                        node.role(),
                        Role::Button | Role::RadioButton | Role::ComboBox
                    ) {
                        return None;
                    }
                    let bounds = node.bounds()?;
                    Some((
                        node.label()?.to_string(),
                        Rect::from_min_max(
                            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                        ),
                    ))
                })
                .collect();
        }
        for clipped in &out.shapes {
            collect_texts(&clipped.shape, clipped.clip_rect, &mut snapshot.texts);
        }
        snapshot.shell = self
            .ctx
            .data(|data| data.get_temp(egui::Id::new("welcome_shell_rect")));
        snapshot
    }

    fn settle(&mut self) -> Snapshot {
        for _ in 0..5 {
            self.frame(Vec::new());
        }
        self.frame(Vec::new())
    }

    fn click(&mut self, label: &str) {
        let at = self
            .settle()
            .target(label)
            .expect("visible control")
            .center();
        let moved = self.frame(vec![Event::PointerMoved(at)]);
        let at = moved.target(label).expect("control after hover").center();
        let press = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(at), press(true)]);
        self.frame(vec![press(false)]);
        self.settle();
    }

    fn reach_text(&mut self, label: &str) -> Snapshot {
        let mut snapshot = self.settle();
        for _ in 0..8 {
            if snapshot.contains_text(label) {
                return snapshot;
            }
            self.frame(vec![
                Event::PointerMoved(egui::pos2(self.size.x * 0.75, self.size.y * 0.75)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -180.0),
                    modifiers: Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                },
            ]);
            snapshot = self.settle();
        }
        assert!(
            snapshot.contains_text(label),
            "{label:?} is reachable by scrolling"
        );
        snapshot
    }
}

fn collect_texts(shape: &egui::Shape, clip: Rect, texts: &mut Vec<(String, Rect)>) {
    match shape {
        egui::Shape::Text(text) if !text.galley.elided => {
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            if rect.intersects(clip) && rect.is_positive() {
                texts.push((text.galley.text().to_string(), rect));
            }
        }
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_texts(shape, clip, texts);
            }
        }
        _ => {}
    }
}

/// The top controls switch all five pages through their real click handlers.
#[test]
fn every_welcome_tab_can_be_selected() {
    for language in ["en", "zh-CN", "de"] {
        crate::i18n::with_thread_language(language, || {
            let mut driver = Driver::new(egui::vec2(1200.0, 900.0));
            for (tab, key) in TABS {
                driver.click(&tr(key));
                assert_eq!(driver.app.welcome_tab, tab, "{language}: {key}");
            }
        });
    }
}

/// Hovering any tab keeps every label in place and uses a hand cursor.
#[test]
fn welcome_tabs_keep_their_geometry_when_hovered() {
    for language in ["en", "zh-CN", "de"] {
        crate::i18n::with_thread_language(language, || {
            let mut driver = Driver::new(egui::vec2(1200.0, 900.0));
            for selected in [WelcomeTab::GetStarted, WelcomeTab::Templates] {
                driver.app.welcome_tab = selected;
                driver.frame(vec![Event::PointerGone]);
                let original = driver.settle().tab_geometry();
                for ((_, key), (control, _)) in TABS.into_iter().zip(original) {
                    let hovered = driver.frame(vec![Event::PointerMoved(control.center())]);
                    for snapshot in [hovered, driver.settle()] {
                        assert_eq!(
                            snapshot.tab_geometry(),
                            original,
                            "{language}, {selected:?}, hovering {key}"
                        );
                        assert_eq!(snapshot.cursor, egui::CursorIcon::PointingHand);
                    }
                    driver.frame(vec![Event::PointerGone]);
                    assert_eq!(
                        driver.settle().tab_geometry(),
                        original,
                        "{language}, {selected:?}, leaving {key}"
                    );
                }
            }
        });
    }
}

/// Every page shares a centered, bounded shell on a wide desktop window.
#[test]
fn welcome_pages_keep_their_maximum_width_and_center() {
    crate::i18n::with_thread_language("en", || {
        for width in [1920.0, 2560.0] {
            let mut driver = Driver::new(egui::vec2(width, 1000.0));
            for (tab, _) in TABS {
                driver.app.welcome_tab = tab;
                let snapshot = driver.settle();
                let shell = snapshot.shell.expect("the welcome shell is recorded");
                assert!(
                    shell.width() <= MAX_CONTENT_WIDTH + 1.0,
                    "{tab:?}: {shell:?}"
                );
                assert!(
                    shell.width() >= MAX_CONTENT_WIDTH - 1.0,
                    "{tab:?}: {shell:?}"
                );
                assert!((shell.center().x - snapshot.panel.center().x).abs() <= 1.0);
                snapshot.assert_horizontal_bounds(width, &format!("{tab:?}, {width}"));
            }
        }
    });
}

/// Narrow desktop space and long translated text use the same responsive pages.
#[test]
fn welcome_pages_fit_narrow_windows_and_long_translations() {
    for language in ["en", "zh-CN", "de"] {
        crate::i18n::with_thread_language(language, || {
            for size in [
                egui::vec2(640.0, 760.0),
                egui::vec2(360.0, 760.0),
                egui::vec2(640.0, 360.0),
            ] {
                let mut driver = Driver::new(size);
                for (tab, _) in TABS {
                    driver.app.welcome_tab = tab;
                    let snapshot = driver.settle();
                    snapshot.assert_horizontal_bounds(
                        size.x,
                        &format!("{language}, {tab:?}, {size:?}"),
                    );
                }
            }
        });
    }
}

/// Workspace presets remain selectable and the language choices stay reachable.
#[test]
fn workspace_choices_and_language_controls_remain_reachable() {
    for language in ["en", "zh-CN", "de"] {
        crate::i18n::with_thread_language(language, || {
            let mut driver = Driver::new(egui::vec2(640.0, 760.0));
            driver.click(&tr("welcome.workspace"));
            driver.click(&tr("menu.window.workspace_lite"));
            assert_eq!(driver.app.workspace, crate::app::Workspace::Lite);
            driver.reach_text(&tr("options.language"));
            driver.reach_text("English");
        });
    }
}

/// New Document keeps the existing request path and its dialog preference.
#[test]
fn welcome_new_document_opens_the_existing_dialog() {
    crate::i18n::with_thread_language("en", || {
        let mut driver = Driver::new(egui::vec2(1200.0, 900.0));
        driver.app.settings.show_new_document_dialog = true;
        driver.click(&tr("welcome.new_document"));
        assert!(matches!(
            &driver.app.dialog,
            crate::ui::dialogs::Dialog::NewDocument(_)
        ));
    });
}
