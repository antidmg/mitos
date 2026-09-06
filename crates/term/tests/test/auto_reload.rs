use anyhow::Context as _;
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use editor_core::file_watcher::{CanonicalPathBuf, Event, EventType, Events, FileSystemDidChange};
use tempfile::TempDir;
use term::application::Application;
use view::{current, current_ref};

use super::helpers::*;

fn changed(path: &Path, text: &str, tick: u64) -> anyhow::Result<()> {
    fs::write(path, text)?;
    fs::File::options()
        .write(true)
        .open(path)?
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(tick))?;
    Ok(())
}

fn app(path: &Path) -> anyhow::Result<Application> {
    let mut config = test_config();
    config.editor.auto_reload.enable = true;
    AppBuilder::new()
        .with_config(config)
        .with_file(path, None)
        .build()
}

async fn notify(app: &mut Application, path: &Path, ty: EventType) {
    let path = editor_core::file_watcher::canonicalize_path(path);
    event::dispatch(FileSystemDidChange {
        fs_events: Events::from(vec![Event {
            path: CanonicalPathBuf::assert_canonicalized(&path),
            ty,
        }]),
    });
    run_event_loop_until_idle(app).await;
}

fn text(app: &Application) -> String {
    current_ref!(app.editor).1.text().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn clean_buffers_reload_and_deletion_preserves_the_buffer() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("file.txt");
    changed(&path, "before\n", 1)?;
    let mut app = app(&path)?;
    changed(&path, "after\n", 2)?;
    notify(&mut app, &path, EventType::Modified).await;
    assert_eq!(text(&app), "after\n");
    assert!(!current_ref!(app.editor).1.is_modified());
    fs::remove_file(&path)?;
    notify(&mut app, &path, EventType::Delete).await;
    assert_eq!(text(&app), "after\n");
    changed(&path, "recreated\n", 3)?;
    notify(&mut app, &path, EventType::Create).await;
    assert_eq!(text(&app), "recreated\n");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn modified_buffers_prompt_once_and_reload_only_when_confirmed() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("file.txt");
    changed(&path, "before\n", 1)?;
    let mut app = app(&path)?;
    {
        let (view, doc) = current!(app.editor);
        let transaction = editor_core::Transaction::change(
            doc.text(),
            [(0, 0, Some("local ".into()))].into_iter(),
        );
        doc.apply(&transaction, view.id);
        doc.append_changes_to_history(view);
    }
    changed(&path, "external\n", 2)?;
    notify(&mut app, &path, EventType::Modified).await;
    let seen = current_ref!(app.editor).1.auto_reload_seen_mtime;
    assert!(seen.is_some());
    assert_eq!(text(&app), "local before\n");
    notify(&mut app, &path, EventType::Modified).await;
    assert_eq!(current_ref!(app.editor).1.auto_reload_seen_mtime, seen);
    // One Escape must dismiss the only prompt. A second external edit prompts again.
    #[cfg(not(windows))]
    let escape = termina::event::Event::Key(ui_core::input::parse_macro("<esc>")?[0].into());
    #[cfg(windows)]
    let escape = crossterm::event::Event::Key(ui_core::input::parse_macro("<esc>")?[0].into());
    app.handle_terminal_events(Ok(escape)).await;
    assert_eq!(text(&app), "local before\n");
    changed(&path, "new external\n", 3)?;
    notify(&mut app, &path, EventType::Modified).await;
    test_key_sequence(
        &mut app,
        Some("<ret>"),
        Some(&|app| {
            assert_eq!(text(app), "new external\n");
            assert!(!current_ref!(app.editor).1.is_modified());
        }),
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn focus_reloads_unwatched_files_and_own_saves_do_not_conflict() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("file.txt");
    changed(&path, "before\n", 1)?;
    let mut app = app(&path)?;
    changed(&path, "focus\n", 2)?;
    #[cfg(not(windows))]
    app.handle_terminal_events(Ok(termina::event::Event::FocusIn))
        .await;
    #[cfg(windows)]
    app.handle_terminal_events(Ok(crossterm::event::Event::FocusGained))
        .await;
    run_event_loop_until_idle(&mut app).await;
    assert_eq!(text(&app), "focus\n");
    let id = current_ref!(app.editor).1.id();
    app.editor.save(id, None::<&Path>, false)?;
    app.editor.flush_writes().await?;
    notify(&mut app, &path, EventType::Modified).await;
    assert!(current_ref!(app.editor).1.auto_reload_seen_mtime.is_none());
    assert!(!current_ref!(app.editor).1.is_modified());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn native_watcher_detects_atomic_replacement() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("file.txt");
    changed(&path, "before\n", 1)?;
    let mut app = app(&path)?;
    app.editor
        .file_watcher
        .reload(&editor_core::file_watcher::Config::default());
    app.editor.file_watcher.add_root(dir.path());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !app.editor.file_watcher.is_watching(&path) {
            run_event_loop_until_idle(&mut app).await;
        }
    })
    .await
    .context("native watch root did not become ready")?;
    let replacement = dir.path().join("replacement");
    changed(&replacement, "replacement\n", 2)?;
    fs::rename(replacement, &path)?;
    tokio::time::timeout(Duration::from_secs(10), async {
        while text(&app) != "replacement\n" {
            run_event_loop_until_idle(&mut app).await;
        }
    })
    .await
    .context("atomic replacement did not reload the buffer")?;
    assert!(!current_ref!(app.editor).1.is_modified());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn polling_starts_after_config_reload_and_can_be_disabled() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("file.txt");
    changed(&path, "before\n", 1)?;
    let mut app = app(&path)?;
    let mut config = (*app.editor.config()).clone();
    config.auto_reload.poll.enable = true;
    config.auto_reload.poll.interval = 100;
    app.handle_config_events(view::editor::ConfigEvent::Update(Box::new(config.clone())));
    changed(&path, "polled\n", 2)?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while text(&app) != "polled\n" {
            // The periodic poll itself keeps resetting the application's idle timer.
            let _ = tokio::time::timeout(
                Duration::from_millis(150),
                run_event_loop_until_idle(&mut app),
            )
            .await;
        }
    })
    .await?;
    config.auto_reload.enable = false;
    app.handle_config_events(view::editor::ConfigEvent::Update(Box::new(config)));
    changed(&path, "disabled\n", 3)?;
    notify(&mut app, &path, EventType::Modified).await;
    assert_eq!(text(&app), "polled\n");
    Ok(())
}
