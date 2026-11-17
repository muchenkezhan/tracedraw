//! Bounded page pictures shared by the Pages panel and Page Sorter.

use std::collections::HashMap;
use tracedraw_core::{Document, PageId};

const MAX_PX: u32 = 384;
const MAX_ENTRIES: usize = 96;
const MAX_BYTES: usize = 16 * 1024 * 1024;

struct Entry {
    revision: u64,
    texture: Option<egui::TextureHandle>,
    used: u64,
}

/// Only visible pages request pictures. At most one new picture is drawn
/// per frame, then the remaining rows ask for another frame.
#[derive(Default)]
pub struct PageThumbs {
    entries: HashMap<(u64, PageId, u32), Entry>,
    frame: u64,
    drawn: bool,
    /// Pointer drags keep existing pictures until the gesture ends.
    pub paused: bool,
    pub waiting: bool,
}

impl PageThumbs {
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.drawn = false;
        self.waiting = false;
    }

    pub fn get(
        &mut self,
        ctx: &egui::Context,
        doc: &Document,
        (instance, revision): (u64, u64),
        page: PageId,
        px: u32,
    ) -> Option<egui::TextureHandle> {
        let px = px.clamp(1, MAX_PX);
        let key = (instance, page, px);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.frame;
            if entry.revision == revision {
                return entry.texture.clone();
            }
        }
        if self.drawn || self.paused {
            self.waiting = true;
            return self.entries.get(&key).and_then(|e| e.texture.clone());
        }
        self.drawn = true;
        let texture = render(doc, page, px).map(|image| {
            ctx.load_texture(
                format!("page-thumbnail:{instance}:{}", page.raw()),
                image,
                egui::TextureOptions::LINEAR,
            )
        });
        self.entries.insert(
            key,
            Entry {
                revision,
                texture: texture.clone(),
                used: self.frame,
            },
        );
        texture
    }

    pub fn trim(&mut self) {
        let bytes = |entry: &Entry| entry.texture.as_ref().map_or(0, |t| t.byte_size());
        let mut total: usize = self.entries.values().map(bytes).sum();
        if self.entries.len() <= MAX_ENTRIES && total <= MAX_BYTES {
            return;
        }
        let mut order: Vec<_> = self
            .entries
            .iter()
            .map(|(key, entry)| (*key, entry.used))
            .collect();
        order.sort_by_key(|(_, used)| *used);
        for (key, _) in order {
            if self.entries.len() <= MAX_ENTRIES && total <= MAX_BYTES {
                break;
            }
            if let Some(entry) = self.entries.remove(&key) {
                total = total.saturating_sub(bytes(&entry));
            }
        }
    }
}

fn render(doc: &Document, page: PageId, px: u32) -> Option<egui::ColorImage> {
    let size = doc.page(page).ok()?.size;
    if !size.width.is_finite()
        || !size.height.is_finite()
        || size.width <= 0.0
        || size.height <= 0.0
    {
        return None;
    }
    let zoom = px.clamp(1, MAX_PX) as f64 / size.width.max(size.height);
    if !zoom.is_finite() || zoom <= 0.0 {
        return None;
    }
    let width = (size.width * zoom).ceil().clamp(1.0, MAX_PX as f64) as u32;
    let height = (size.height * zoom).ceil().clamp(1.0, MAX_PX as f64) as u32;
    let objects = tracedraw_render::render_page_on_one_thread(
        doc,
        page,
        &tracedraw_render::RenderOptions {
            width,
            height,
            view: tracedraw_render::ViewTransform {
                zoom,
                origin_x: 0.0,
                origin_y: height as f64,
                stretch_y: 1.0,
            },
            ..Default::default()
        },
    )?;
    let mut image = tiny_skia::Pixmap::new(width, height)?;
    image.fill(tiny_skia::Color::WHITE);
    image.draw_pixmap(
        0,
        0,
        objects.as_ref(),
        &Default::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    Some(egui::ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        image.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::{Color, Command, Engine, Fill, Size};

    #[test]
    fn pictures_reuse_textures_and_refresh_after_edit_undo_and_redo() {
        let ctx = egui::Context::default();
        let mut engine = Engine::default();
        let page = engine.document().pages[0].id;
        let mut cache = PageThumbs::default();
        let get = |cache: &mut PageThumbs, engine: &Engine| {
            cache.begin_frame();
            cache
                .get(
                    &ctx,
                    engine.document(),
                    (engine.instance(), engine.revision()),
                    page,
                    100,
                )
                .unwrap()
        };
        let first = get(&mut cache, &engine);
        assert_eq!(get(&mut cache, &engine).id(), first.id());
        engine
            .run(&Command::SetPageBackground {
                page,
                background: Some(Fill::Solid(Color::rgb8(10, 80, 120))),
            })
            .unwrap();
        let edited = get(&mut cache, &engine);
        assert_ne!(first.id(), edited.id());
        engine.undo().unwrap();
        let undone = get(&mut cache, &engine);
        assert_ne!(undone.id(), edited.id());
        engine.redo().unwrap();
        assert_ne!(get(&mut cache, &engine).id(), undone.id());
        let other = Engine::default();
        cache.begin_frame();
        let distinct = cache
            .get(
                &ctx,
                other.document(),
                (other.instance(), other.revision()),
                page,
                100,
            )
            .unwrap();
        assert_ne!(distinct.id(), get(&mut cache, &engine).id());
    }

    #[test]
    fn pictures_have_page_background_and_visible_objects() {
        let mut doc = Document::new("preview", Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        doc.pages[0].background = Some(Fill::Solid(Color::rgb8(20, 70, 110)));
        let image = render(&doc, page, 100).unwrap();
        assert_eq!(image.pixels[5050], egui::Color32::from_rgb(20, 70, 110));
        let mut shape = tracedraw_core::Shape::new(
            tracedraw_core::ShapeId(42),
            tracedraw_core::ShapeKind::Rect {
                rect: tracedraw_core::Rect::new(20.0, 20.0, 80.0, 80.0),
                radius: 0.0,
                corners: None,
            },
        );
        shape.fill = Fill::Solid(Color::rgb8(190, 30, 40));
        shape.stroke = None;
        doc.pages[0].layers[0].shapes.push(shape);
        assert_eq!(
            render(&doc, page, 100).unwrap().pixels[5050],
            egui::Color32::from_rgb(190, 30, 40)
        );
        doc.pages[0].layers[0].visible = false;
        assert_eq!(
            render(&doc, page, 100).unwrap().pixels[5050],
            image.pixels[5050]
        );
        let mut master = tracedraw_core::Layer::master(
            tracedraw_core::LayerId(99),
            "Master",
            tracedraw_core::MasterScope::All,
        );
        master.shapes = doc.pages[0].layers[0].shapes.clone();
        doc.master.push(master);
        assert_eq!(
            render(&doc, page, 100).unwrap().pixels[5050],
            egui::Color32::from_rgb(190, 30, 40)
        );
    }

    #[test]
    fn pictures_bound_resolution_and_reject_invalid_sizes() {
        let mut doc = Document::new("preview", Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        for size in [
            Size::new(1e12, 1e-6),
            Size::new(1e-6, 1e12),
            Size::new(100.0, 100.0),
        ] {
            doc.pages[0].size = size;
            let image = render(&doc, page, u32::MAX).unwrap();
            assert!(image.size.iter().all(|v| *v >= 1 && *v <= MAX_PX as usize));
        }
        for size in [
            Size::new(0.0, 100.0),
            Size::new(f64::NAN, 1.0),
            Size::new(f64::INFINITY, 1.0),
            Size::new(-1.0, 1.0),
        ] {
            doc.pages[0].size = size;
            assert!(render(&doc, page, 100).is_none());
        }
    }

    #[test]
    fn pictures_defer_work_and_bound_the_cache() {
        let ctx = egui::Context::default();
        let doc = Document::new("preview", Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        let mut cache = PageThumbs::default();
        cache.begin_frame();
        assert!(cache.get(&ctx, &doc, (1, 0), page, 16).is_some());
        assert!(cache.get(&ctx, &doc, (2, 0), page, 16).is_none());
        assert!(cache.waiting);
        cache.begin_frame();
        cache.paused = true;
        assert!(cache.get(&ctx, &doc, (2, 0), page, 16).is_none());
        assert!(cache.waiting);
        assert_eq!(
            cache.entries.len(),
            1,
            "a pointer gesture starts no new rendering"
        );
        cache.paused = false;
        for instance in 2..200 {
            cache.begin_frame();
            cache.get(&ctx, &doc, (instance, 0), page, MAX_PX);
            cache.trim();
        }
        assert!(cache.entries.len() <= MAX_ENTRIES);
        assert!(
            cache
                .entries
                .values()
                .filter_map(|e| e.texture.as_ref())
                .map(|t| t.byte_size())
                .sum::<usize>()
                <= MAX_BYTES
        );
    }

    #[test]
    fn panel_and_sorter_sizes_do_not_invalidate_each_other() {
        let ctx = egui::Context::default();
        let doc = Document::new("preview", Size::new(100.0, 100.0));
        let page = doc.pages[0].id;
        let mut cache = PageThumbs::default();
        cache.begin_frame();
        let panel = cache.get(&ctx, &doc, (1, 0), page, 100).unwrap();
        cache.begin_frame();
        let sorter = cache.get(&ctx, &doc, (1, 0), page, 180).unwrap();
        assert_ne!(panel.id(), sorter.id());
        cache.begin_frame();
        assert_eq!(
            cache.get(&ctx, &doc, (1, 0), page, 100).unwrap().id(),
            panel.id()
        );
        assert_eq!(
            cache.get(&ctx, &doc, (1, 0), page, 180).unwrap().id(),
            sorter.id()
        );
        assert!(!cache.waiting);
    }
}
