//! External file changes, conflict prompts, and polling for unwatched buffers.
//!
//! Adapted from work by Pascal Kuthe and Blaž Hrastnik in
//! [Helix PR #14544](https://github.com/helix-editor/helix/pull/14544),
//! including reload contributions by Anthony Rubick.

use std::{borrow::Cow, path::PathBuf, time::Duration};

use editor_core::file_watcher::{canonicalize_path, EventType, FileSystemDidChange};
use event::{register_hook, send_blocking, AsyncHook};
use tokio::time::Instant;
use view::editor::Config;
use view::events::ConfigDidChange;
use view::handlers::{AutoReloadEvent, Handlers};
use view::{DocumentId, Editor};

use crate::compositor::Compositor;
use crate::ui::{Prompt, PromptEvent};
use crate::{job, ui};

/// Polling is also needed for Git refs in linked worktrees and ignored directories.
#[derive(Debug)]
pub(super) struct PollHandler;

impl PollHandler {
    pub fn new() -> Self {
        Self
    }
}

fn poll_event(config: &Config) -> AutoReloadEvent {
    if config.auto_reload.poll.enable
        && (config.auto_reload.enable || config.file_watcher.watch_vcs)
    {
        AutoReloadEvent::PollAfter {
            interval: config.auto_reload.poll.interval.max(100),
        }
    } else {
        AutoReloadEvent::Stop
    }
}

impl AsyncHook for PollHandler {
    type Event = AutoReloadEvent;

    fn handle_event(&mut self, event: Self::Event, _: Option<Instant>) -> Option<Instant> {
        match event {
            AutoReloadEvent::PollAfter { interval } => {
                Some(Instant::now() + Duration::from_millis(interval))
            }
            AutoReloadEvent::Stop => None,
        }
    }

    fn finish_debounce(&mut self) {
        job::dispatch_blocking(|editor, compositor| {
            if editor.config().auto_reload.poll.enable {
                check_unwatched(editor, compositor);
            }
            send_blocking(&editor.handlers.auto_reload, poll_event(&editor.config()));
        });
    }
}

/// Focus checks work even when periodic polling is disabled.
pub(crate) fn on_focus_gained() {
    // Use a compositor callback so conflict prompts share the native event path.
    job::dispatch_blocking(check_unwatched);
}

fn check_unwatched(editor: &mut Editor, compositor: &mut Compositor) {
    if editor.config().auto_reload.enable {
        let ids: Vec<_> = editor
            .documents()
            .filter(|doc| {
                doc.path()
                    .is_some_and(|path| !editor.file_watcher.is_watching(path))
            })
            .map(|doc| doc.id())
            .collect();
        for id in ids {
            if let Some(doc) = editor.documents.get(&id)
                && let Some(path) = doc.path()
                && let Ok(mtime) = path.metadata().and_then(|meta| meta.modified())
                && mtime != doc.last_saved_time()
                && doc.auto_reload_seen_mtime != Some(mtime)
            {
                editor
                    .language_servers
                    .file_event_handler
                    .file_changed(path.to_path_buf(), EventType::Modified);
            }
            handle_document_change(editor, compositor, id);
        }
    }
    if editor.config().file_watcher.watch_vcs && editor.file_watcher.poll_extra_paths() {
        reload_vcs(editor);
    }
}

fn handle_document_change(editor: &mut Editor, compositor: &mut Compositor, doc_id: DocumentId) {
    let Some(doc) = editor.documents.get_mut(&doc_id) else {
        return;
    };
    let Some(path) = doc.path().map(ToOwned::to_owned) else {
        return;
    };
    let Ok(mtime) = path.metadata().and_then(|meta| meta.modified()) else {
        // Keep the buffer when its file is deleted; a later recreation can reload it.
        return;
    };
    if mtime == doc.last_saved_time() || doc.auto_reload_seen_mtime == Some(mtime) {
        return;
    }
    if doc.is_modified() {
        doc.auto_reload_seen_mtime = Some(mtime);
        let display = doc.display_name().to_string();
        if editor.config().auto_reload.prompt_if_modified {
            prompt_reload_modified(compositor, doc_id, path, display);
        } else {
            editor.set_warning(|| {
                format!(
                    "{display} changed externally but has unsaved changes; use :reload to refresh"
                )
            });
        }
    } else if let Err(err) = reload_document(editor, doc_id) {
        editor.set_error(|| format!("{} auto-reload failed: {err}", path.display()));
    }
}

/// Use the existing reload transaction so selections, undo history, and LSPs stay in sync.
fn reload_document(editor: &mut Editor, doc_id: DocumentId) -> anyhow::Result<()> {
    let view_id = editor.get_synced_view_id(doc_id);
    let scrolloff = editor.config().scrolloff;
    let doc = doc_mut!(editor, &doc_id);
    let trust_full = editor
        .workspace_trust
        .query(
            doc.workspace_root(),
            loader::workspace_trust::TrustQuery::Git,
        )
        .is_trusted();
    let view = view_mut!(editor, view_id);
    doc.reload(view, &editor.diff_providers, trust_full)?;
    // Reload commits history through one view. Sync every split displaying the
    // document before its jumplist is used against the new text.
    for (view, _) in editor.tree.views_mut() {
        if view.doc == doc_id {
            view.sync_changes(doc);
            view.ensure_cursor_in_view(doc, scrolloff);
        }
    }
    let display = doc.display_name().to_string();
    editor.set_status(format!("{display} reloaded (external changes)"));
    Ok(())
}

fn reload_vcs(editor: &mut Editor) {
    for doc in editor.documents.values_mut() {
        let Some(path) = doc.path().map(ToOwned::to_owned) else {
            continue;
        };
        let trust_full = editor
            .workspace_trust
            .query(
                doc.workspace_root(),
                loader::workspace_trust::TrustQuery::Git,
            )
            .is_trusted();
        doc.refresh_vcs(&editor.diff_providers, trust_full);
        log::debug!("refreshed VCS state for {}", path.display());
    }
    // HEAD may now point to another loose ref.
    editor.refresh_vcs_watches();
}

/// Co-Authored-By: Anthony Rubick <68485672+AnthonyMichaelTDM@users.noreply.github.com>
fn prompt_reload_modified(
    compositor: &mut Compositor,
    doc_id: DocumentId,
    path: PathBuf,
    display: String,
) {
    let prompt = Prompt::new(
        Cow::Owned(format!("{display} changed externally (unsaved changes exist). Press Enter to reload, Esc to ignore: ")),
        None,
        ui::completers::none,
        move |cx, _input, event| {
            match event {
                PromptEvent::Validate => {
                    // The buffer may have been closed or renamed while the prompt was open.
                    if !cx.editor.documents.get(&doc_id).is_some_and(|doc| doc.path() == Some(path.as_path())) {
                        return;
                    }
                    if let Err(err) = reload_document(cx.editor, doc_id) {
                        cx.editor.set_error(|| format!("{display} reload failed: {err}"));
                    }
                }
                PromptEvent::Abort => cx.editor.set_status(format!("{display} external changes ignored")),
                PromptEvent::Update => {}
            }
        },
    );
    compositor.push(Box::new(prompt));
}

pub(super) fn register_hooks(handlers: &Handlers, config: &Config) {
    register_hook!(move |event: &mut FileSystemDidChange| {
        let events = event.fs_events.clone();
        job::dispatch_blocking(move |editor, compositor| {
            let auto_reload = editor.config().auto_reload.enable;
            let watch_vcs = editor.config().file_watcher.watch_vcs;
            let mut vcs_changed = false;
            let mut ignores_changed = false;
            for event in &*events {
                if event.ty == EventType::Tempfile {
                    continue;
                }
                let path = event.path.as_std_path();
                ignores_changed |= path.file_name().is_some_and(|name| {
                    name == ".gitignore" || name == ".ignore" || name == "filesentryignore"
                }) || path.ends_with(".mitos/ignore");
                vcs_changed |= watch_vcs && editor.file_watcher.is_vcs_path(path);
            }
            if auto_reload {
                let changed: std::collections::HashSet<_> = events
                    .iter()
                    .filter(|event| matches!(event.ty, EventType::Modified | EventType::Create))
                    .map(|event| event.path.as_std_path())
                    .collect();
                let ids: Vec<_> = editor
                    .documents()
                    .filter(|doc| {
                        doc.path()
                            .is_some_and(|path| changed.contains(canonicalize_path(path).as_path()))
                    })
                    .map(|doc| doc.id())
                    .collect();
                for id in ids {
                    handle_document_change(editor, compositor, id);
                }
            }
            if vcs_changed {
                reload_vcs(editor);
            }
            if ignores_changed {
                editor.file_watcher.refresh_filter();
            }
        });
        Ok(())
    });
    register_hook!(move |event: &mut ConfigDidChange<'_>| {
        send_blocking(&event.editor.handlers.auto_reload, poll_event(event.new));
        Ok(())
    });
    send_blocking(&handlers.auto_reload, poll_event(config));
}
