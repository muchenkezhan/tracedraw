//! The document palette's desktop eyedropper. Only its final RGB sample
//! crosses back to the editor; screenshots never leave the native picker.

use crate::app::App;
use crate::i18n::tr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use tracedraw_core::Color;

#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

type PickResult = Result<Option<[u8; 3]>, String>;

struct Job {
    drawing: u64,
    rx: Receiver<PickResult>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct State {
    job: Option<Job>,
}

impl State {
    pub fn active(&self) -> bool {
        self.job.is_some()
    }
}

impl App {
    /// Desktop picking on Windows, drawing picking on other platforms.
    pub fn start_palette_sample(&mut self, ctx: &egui::Context) {
        if !self.drawing_shown() {
            return;
        }
        if self.screen_pick.active() {
            self.cancel_palette_screen_pick();
            return;
        }
        #[cfg(windows)]
        {
            let Some(drawing) = self.docs.get(self.active_doc).map(|d| d.drawing) else {
                return;
            };
            let (tx, rx) = std::sync::mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            let repaint = ctx.clone();
            let hint = tr("status.palette_screen_sample");
            let worker = std::thread::Builder::new()
                .name("palette-screen-pick".into())
                .spawn(move || {
                    let result = windows::pick(&flag, &hint);
                    let _ = tx.send(result);
                    repaint.request_repaint();
                });
            match worker {
                Ok(_) => {
                    self.pending_palette_sample = false;
                    self.screen_pick.job = Some(Job {
                        drawing,
                        rx,
                        cancel,
                    });
                    self.status = tr("status.palette_screen_sample");
                    ctx.request_repaint();
                }
                Err(error) => {
                    log::warn!("Could not start the screen eyedropper: {error}");
                    self.status = tr("status.palette_screen_failed");
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = ctx;
            self.pending_palette_sample = !self.pending_palette_sample;
            if self.pending_palette_sample {
                self.status = tr("status.palette_sample");
            }
        }
    }

    /// Discard a pending sample when its drawing is no longer active.
    pub fn cancel_palette_screen_pick(&mut self) {
        self.screen_pick.job = None;
    }

    /// Called once per UI frame. No screen capture or blocking work here.
    pub fn poll_palette_screen_pick(&mut self, ctx: &egui::Context) {
        let Some(job) = self.screen_pick.job.as_ref() else {
            return;
        };
        if !self.drawing_shown()
            || self.docs.get(self.active_doc).map(|d| d.drawing) != Some(job.drawing)
        {
            self.cancel_palette_screen_pick();
            return;
        }
        let result = match job.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                // Also poll if a worker finishes just after the last wakeup.
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("Screen picker stopped without a result".into()),
        };
        self.screen_pick.job = None;
        match result {
            Ok(Some([r, g, b])) => {
                self.add_to_document_palette(Color::rgb8(r, g, b));
                self.status = tr("status.palette_screen_added");
            }
            Ok(None) => self.status = tr("status.cancelled"),
            Err(error) => {
                log::warn!("Screen eyedropper: {error}");
                self.status = tr("status.palette_screen_failed");
            }
        }
    }
}
