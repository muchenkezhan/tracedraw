use super::*;
use crate::app::App;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use tracedraw_core::{geometry::Size, Color, Document};

type PickResult = Result<Option<[u8; 3]>, String>;

fn drawing(app: &App) -> u64 {
    app.docs[app.active_doc].drawing
}

fn waiting(app: &mut App, drawing: u64) -> (Sender<PickResult>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    app.screen_pick.job = Some(Job {
        drawing,
        rx,
        cancel: cancel.clone(),
    });
    (tx, cancel)
}

#[test]
fn a_screen_colour_adds_one_undoable_palette_swatch() {
    let mut app = App::headless();
    let before = app.doc().palette.clone();
    let history = app.engine.history_labels().0.len();
    let id = drawing(&app);
    let (tx, _) = waiting(&mut app, id);
    tx.send(Ok(Some([21, 132, 243]))).expect("deliver colour");

    app.poll_palette_screen_pick(&egui::Context::default());

    let colour = Color::rgb8(21, 132, 243);
    let mut expected = before.clone();
    expected.push(colour);
    assert_eq!(app.doc().palette, expected);
    assert_eq!(app.doc_palette_current, Some(before.len()));
    assert_eq!(app.engine.history_labels().0.len(), history + 1);
    assert!(app.screen_pick.job.is_none());

    app.undo();
    assert_eq!(app.doc().palette, before);
    app.redo();
    assert_eq!(app.doc().palette, expected);
}

#[test]
fn picking_an_existing_swatch_selects_it_without_an_undo_step() {
    let mut app = App::headless();
    let colour = Color::rgb8(21, 132, 243);
    app.add_to_document_palette(colour);
    app.add_to_document_palette(Color::rgb8(9, 8, 7));
    let palette = app.doc().palette.clone();
    let history = app.engine.history_labels().0.len();
    let id = drawing(&app);
    let (tx, _) = waiting(&mut app, id);
    tx.send(Ok(Some([21, 132, 243]))).expect("deliver colour");

    app.poll_palette_screen_pick(&egui::Context::default());

    assert_eq!(app.doc().palette, palette);
    assert_eq!(
        app.doc_palette_current,
        palette.iter().position(|c| *c == colour)
    );
    assert_eq!(app.engine.history_labels().0.len(), history);
    assert!(app.screen_pick.job.is_none());
}

#[test]
fn waiting_for_the_screen_does_not_change_the_drawing() {
    let mut app = App::headless();
    let palette = app.doc().palette.clone();
    let history = app.engine.history_labels().0.len();
    let id = drawing(&app);
    let (_tx, cancel) = waiting(&mut app, id);

    app.poll_palette_screen_pick(&egui::Context::default());

    assert!(app.screen_pick.job.is_some());
    assert!(!cancel.load(Ordering::Relaxed));
    assert_eq!(app.doc().palette, palette);
    assert_eq!(app.engine.history_labels().0.len(), history);
}

#[test]
fn cancellation_and_errors_leave_the_palette_and_history_unchanged() {
    for result in [Ok(None), Err("Screen capture unavailable".to_owned())] {
        let mut app = App::headless();
        app.add_to_document_palette(Color::rgb8(3, 4, 5));
        let palette = app.doc().palette.clone();
        let current = app.doc_palette_current;
        let history = app.engine.history_labels().0.len();
        let id = drawing(&app);
        let (tx, _) = waiting(&mut app, id);
        tx.send(result).expect("deliver outcome");

        app.poll_palette_screen_pick(&egui::Context::default());

        assert!(app.screen_pick.job.is_none());
        assert_eq!(app.doc().palette, palette);
        assert_eq!(app.doc_palette_current, current);
        assert_eq!(app.engine.history_labels().0.len(), history);
    }
}

#[test]
fn a_disconnected_worker_leaves_no_pending_pick_or_document_change() {
    let mut app = App::headless();
    let palette = app.doc().palette.clone();
    let history = app.engine.history_labels().0.len();
    let id = drawing(&app);
    let (tx, _) = waiting(&mut app, id);
    drop(tx);

    app.poll_palette_screen_pick(&egui::Context::default());

    assert!(app.screen_pick.job.is_none());
    assert_eq!(app.doc().palette, palette);
    assert_eq!(app.engine.history_labels().0.len(), history);
}

#[test]
fn cancelling_discards_an_already_queued_colour() {
    let mut app = App::headless();
    let palette = app.doc().palette.clone();
    let history = app.engine.history_labels().0.len();
    let id = drawing(&app);
    let (tx, cancel) = waiting(&mut app, id);
    tx.send(Ok(Some([21, 132, 243]))).expect("deliver colour");

    app.cancel_palette_screen_pick();
    app.poll_palette_screen_pick(&egui::Context::default());

    assert!(cancel.load(Ordering::Relaxed));
    assert!(app.screen_pick.job.is_none());
    assert_eq!(app.doc().palette, palette);
    assert_eq!(app.engine.history_labels().0.len(), history);
}

#[test]
fn a_colour_from_another_drawing_cannot_edit_the_current_one() {
    let mut app = App::headless();
    let origin = drawing(&app);
    let original_palette = app.doc().palette.clone();
    app.open_document(Document::new("Second", Size::new(100.0, 50.0)), None);
    let palette = app.doc().palette.clone();
    let history = app.engine.history_labels().0.len();
    let (tx, _) = waiting(&mut app, origin);
    tx.send(Ok(Some([21, 132, 243])))
        .expect("deliver stale colour");

    app.poll_palette_screen_pick(&egui::Context::default());

    assert!(app.screen_pick.job.is_none());
    assert_eq!(app.doc().palette, palette);
    assert_eq!(app.engine.history_labels().0.len(), history);
    app.switch_document(0);
    assert_eq!(app.doc().palette, original_palette);
}

#[test]
fn a_closed_drawing_cannot_send_a_colour_to_a_new_drawing_at_the_same_index() {
    let mut app = App::headless();
    let origin = drawing(&app);
    app.close_active_document();
    app.open_document(Document::new("Replacement", Size::new(100.0, 50.0)), None);
    assert_eq!(app.active_doc, 0);
    assert_ne!(drawing(&app), origin);
    let palette = app.doc().palette.clone();
    let history = app.engine.history_labels().0.len();
    let (tx, _) = waiting(&mut app, origin);
    tx.send(Ok(Some([21, 132, 243])))
        .expect("deliver stale colour");

    app.poll_palette_screen_pick(&egui::Context::default());

    assert!(app.screen_pick.job.is_none());
    assert_eq!(app.doc().palette, palette);
    assert_eq!(app.engine.history_labels().0.len(), history);
}

#[test]
fn a_screen_pick_follows_its_drawing_when_tabs_move() {
    let mut app = App::headless();
    app.open_document(Document::new("Second", Size::new(100.0, 50.0)), None);
    let origin = drawing(&app);
    let (tx, _) = waiting(&mut app, origin);
    app.move_document(1, 0);
    assert_eq!(app.active_doc, 0);
    assert_eq!(drawing(&app), origin);
    tx.send(Ok(Some([21, 132, 243]))).expect("deliver colour");

    app.poll_palette_screen_pick(&egui::Context::default());

    assert!(app.doc().palette.contains(&Color::rgb8(21, 132, 243)));
    app.switch_document(1);
    assert!(!app.doc().palette.contains(&Color::rgb8(21, 132, 243)));
}

#[test]
fn dropping_the_screen_picker_cancels_its_worker() {
    let mut app = App::headless();
    let id = drawing(&app);
    let (_tx, cancel) = waiting(&mut app, id);
    let state = std::mem::take(&mut app.screen_pick);

    drop(state);

    assert!(cancel.load(Ordering::Relaxed));
}
