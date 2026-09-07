use std::{fs, time::Duration};

use editor_core::{diagnostic::DiagnosticProvider, Range, Selection, Transaction};
use term::application::Application;
use view::{current, current_ref, quicklist::QuicklistTarget};

use super::helpers::*;

fn mistakes(app: &Application) -> Vec<String> {
    let (_, doc) = current_ref!(app.editor);
    doc.diagnostics()
        .iter()
        .filter(|d| d.provider == DiagnosticProvider::Spelling)
        .map(|d| doc.text().slice(d.range.start..d.range.end).to_string())
        .collect()
}

async fn wait_for_mistakes(app: &mut Application, expected: &[&str]) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            app.editor.reset_idle_timer();
            run_event_loop_until_idle(app).await;
            if mistakes(app) == expected {
                break;
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("expected {expected:?}, got {:?}", mistakes(app)))
}

fn replace(app: &mut Application, start: usize, end: usize, replacement: &str) {
    let (view, doc) = current!(app.editor);
    let tx = Transaction::change(
        doc.text(),
        [(start, end, Some(replacement.into()))].into_iter(),
    );
    doc.apply(&tx, view.id);
    doc.append_changes_to_history(view);
}

async fn keys(app: &mut Application, keys: &str) -> anyhow::Result<()> {
    for key in ui_core::input::parse_macro(keys)? {
        #[cfg(not(windows))]
        let event = termina::event::Event::Key(key.into());
        #[cfg(windows)]
        let event = crossterm::event::Event::Key(key.into());
        app.handle_terminal_events(Ok(event)).await;
    }
    app.editor.reset_idle_timer();
    tokio::time::timeout(Duration::from_secs(10), run_event_loop_until_idle(app)).await?;
    Ok(())
}

fn selection(app: &Application) -> &Selection {
    let (view, doc) = current_ref!(app.editor);
    doc.selection(view.id)
}

fn select(app: &mut Application, selection: Selection) {
    let (view, doc) = current!(app.editor);
    doc.set_selection(view.id, selection);
}

async fn navigation_app() -> anyhow::Result<Application> {
    let mut app = AppBuilder::new()
        .with_input_text("#[🚀|]# teh hello quik world wrld\n")
        .build()?;
    keys(&mut app, ":spelling en_US<ret>").await?;
    wait_for_mistakes(&mut app, &["teh", "quik", "wrld"]).await?;
    // A non-spelling diagnostic between the findings must be skipped, even with the same source
    // label. Navigation and textobjects should filter by provider, not severity or source text.
    let (_, doc) = current!(app.editor);
    let mut diagnostic = doc.diagnostics()[0].clone();
    diagnostic.range = editor_core::diagnostic::Range { start: 6, end: 11 };
    let provider = DiagnosticProvider::Lsp {
        server_id: Default::default(),
        identifier: None,
    };
    diagnostic.provider = provider.clone();
    doc.replace_diagnostics([diagnostic], &[], Some(&provider));
    Ok(app)
}

#[tokio::test(flavor = "multi_thread")]
async fn spelling_navigation_skips_other_diagnostics_and_respects_counts_and_boundaries(
) -> anyhow::Result<()> {
    let mut app = navigation_app().await?;
    keys(&mut app, "]s").await?;
    assert_eq!(selection(&app), &Selection::single(2, 5));
    keys(&mut app, "<A-.>").await?;
    assert_eq!(selection(&app), &Selection::single(12, 16));
    // Reversing direction skips the selected finding rather than selecting it again.
    keys(&mut app, "[s").await?;
    assert_eq!(selection(&app), &Selection::single(5, 2));
    keys(&mut app, "[s").await?;
    assert_eq!(selection(&app), &Selection::single(5, 2));
    keys(&mut app, "2]s").await?;
    assert_eq!(selection(&app), &Selection::single(23, 27));
    keys(&mut app, "]s").await?;
    assert_eq!(selection(&app), &Selection::single(23, 27));
    keys(&mut app, "99[s").await?;
    assert_eq!(selection(&app), &Selection::single(5, 2));
    keys(&mut app, "99]s").await?;
    assert_eq!(selection(&app), &Selection::single(23, 27));
    // A cursor immediately after a finding can navigate back to it.
    select(&mut app, Selection::point(16));
    keys(&mut app, "[s").await?;
    assert_eq!(selection(&app), &Selection::single(16, 12));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn spelling_navigation_moves_each_cursor_and_extends_selections() -> anyhow::Result<()> {
    let mut app = navigation_app().await?;
    select(
        &mut app,
        Selection::new(vec![Range::point(0), Range::point(11)].into(), 1),
    );
    keys(&mut app, "]s").await?;
    assert_eq!(
        selection(&app),
        &Selection::new(vec![Range::new(2, 5), Range::new(12, 16)].into(), 1)
    );
    select(
        &mut app,
        Selection::new(vec![Range::point(12), Range::point(27)].into(), 0),
    );
    keys(&mut app, "[s").await?;
    assert_eq!(
        selection(&app),
        &Selection::new(vec![Range::new(5, 2), Range::new(27, 23)].into(), 0)
    );
    select(&mut app, Selection::single(0, 1));
    keys(&mut app, "v2]s").await?;
    assert_eq!(selection(&app), &Selection::single(0, 16));
    keys(&mut app, "[s").await?;
    assert_eq!(selection(&app), &Selection::single(0, 5));
    select(&mut app, Selection::single(28, 27));
    keys(&mut app, "2[s").await?;
    assert_eq!(selection(&app), &Selection::single(28, 12));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn spelling_textobjects_select_findings_and_preserve_direction() -> anyhow::Result<()> {
    let mut app = navigation_app().await?;
    select(&mut app, Selection::single(3, 4));
    keys(&mut app, "mis").await?;
    assert_eq!(selection(&app), &Selection::single(2, 5));
    select(&mut app, Selection::single(4, 3));
    keys(&mut app, "mas").await?;
    assert_eq!(selection(&app), &Selection::single(5, 2));
    // Outside a finding, the textobject leaves the selection alone.
    select(&mut app, Selection::single(6, 7));
    keys(&mut app, "mis").await?;
    assert_eq!(selection(&app), &Selection::single(6, 7));
    select(&mut app, Selection::single(0, 17));
    keys(&mut app, "mIs").await?;
    assert_eq!(
        selection(&app),
        &Selection::new(vec![Range::new(2, 5), Range::new(12, 16)].into(), 0)
    );
    // The partially selected first finding and the non-spelling diagnostic are excluded.
    select(&mut app, Selection::single(28, 3));
    keys(&mut app, "mAs").await?;
    assert_eq!(
        selection(&app),
        &Selection::new(vec![Range::new(16, 12), Range::new(27, 23)].into(), 0)
    );
    keys(&mut app, ":spelling off<ret>").await?;
    let before = selection(&app).clone();
    keys(&mut app, "[s]smis").await?;
    assert_eq!(selection(&app), &before);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn opt_in_commands_corrections_and_undo() -> anyhow::Result<()> {
    let mut app = AppBuilder::new()
        .with_input_text("#[t|]#eh quik\n")
        .build()?;
    run_event_loop_until_idle(&mut app).await;
    assert!(mistakes(&app).is_empty());
    keys(&mut app, ":spelling en_US en_US<ret>").await?;
    assert_eq!(current_ref!(app.editor).1.spelling_languages.len(), 1);
    wait_for_mistakes(&mut app, &["teh", "quik"]).await?;
    // A later dictionary with no useful corrections must not erase earlier suggestions.
    let second = "second_dictionary".parse()?;
    app.editor.dictionaries.insert(
        second,
        std::sync::Arc::new(view::Dictionary::new("SET UTF-8\n", "1\nworld\n").unwrap()),
    );
    current!(app.editor)
        .1
        .spelling_languages
        .push("second_dictionary".parse()?);
    let actions = app.editor.spelling_actions();
    let correction = actions
        .iter()
        .find(|a| a.title() == "Replace 'teh' with 'the'")
        .unwrap();
    correction.execute(&mut app.editor);
    wait_for_mistakes(&mut app, &["quik"]).await?;
    keys(&mut app, "u").await?;
    wait_for_mistakes(&mut app, &["teh", "quik"]).await?;
    // A menu captured before an edit must not overwrite a newer buffer version.
    correction.execute(&mut app.editor);
    assert_eq!(mistakes(&app), ["teh", "quik"]);
    let id = current_ref!(app.editor).1.id();
    let pending = app.editor.handlers.spelling.open_request(id);
    keys(&mut app, ":spelling off<ret>").await?;
    assert!(pending.is_canceled());
    wait_for_mistakes(&mut app, &[]).await?;
    assert!(app.editor.spelling_actions().is_empty());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_dictionaries_report_an_error_and_bad_names_preserve_settings() -> anyhow::Result<()>
{
    let mut app = AppBuilder::new().with_input_text("#[t|]#eh\n").build()?;
    keys(&mut app, ":spelling mitos_nonexistent_dictionary<ret>").await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !app
            .editor
            .get_status()
            .is_some_and(|(message, _)| message.contains("Could not load spelling dictionary"))
        {
            app.editor.reset_idle_timer();
            run_event_loop_until_idle(&mut app).await;
        }
    })
    .await?;
    keys(&mut app, ":spelling ../../invalid<ret>").await?;
    assert_eq!(
        current_ref!(app.editor).1.spelling_languages[0].as_str(),
        "mitos_nonexistent_dictionary"
    );
    assert!(mistakes(&app).is_empty());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn scratch_buffers_follow_config_and_feed_diagnostics_quicklists() -> anyhow::Result<()> {
    let mut config = test_config();
    config.editor.spelling.languages = Some(vec!["en_US".parse()?]);
    let mut app = AppBuilder::new()
        .with_config(config)
        .with_input_text("#[t|]#eh quik\n")
        .build()?;
    wait_for_mistakes(&mut app, &["teh", "quik"]).await?;
    keys(&mut app, "<space>d").await?;
    keys(&mut app, "<C-q><esc>").await?;
    assert_eq!(app.editor.quicklist.entries().len(), 2);
    let id = current_ref!(app.editor).1.id();
    assert!(app
        .editor
        .quicklist
        .entries()
        .iter()
        .all(|e| e.target == QuicklistTarget::Document(id)));

    let mut config = (*app.editor.config()).clone();
    config.spelling.words = vec!["TEH".into()];
    app.handle_config_events(view::editor::ConfigEvent::Update(Box::new(config.clone())));
    wait_for_mistakes(&mut app, &["quik"]).await?;
    config.spelling.languages = Some(Vec::new());
    app.handle_config_events(view::editor::ConfigEvent::Update(Box::new(config)));
    wait_for_mistakes(&mut app, &[]).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn editorconfig_and_manual_override_take_precedence() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(
        dir.path().join(".editorconfig"),
        "root = true\n[*]\nspelling_language = en_US\n",
    )?;
    let path = dir.path().join("test.txt");
    fs::write(&path, "teh\n")?;
    let mut config = test_config();
    config.editor.spelling.languages = Some(vec!["missing_dictionary".parse()?]);
    let mut app = AppBuilder::new()
        .with_config(config)
        .with_file(path, None)
        .build()?;
    wait_for_mistakes(&mut app, &["teh"]).await?;
    assert_eq!(
        current_ref!(app.editor).1.spelling_languages[0].as_str(),
        "en_US"
    );
    let mut config = (*app.editor.config()).clone();
    config.spelling.languages = Some(Vec::new());
    config.editor_config = false;
    app.handle_config_events(view::editor::ConfigEvent::Update(Box::new(config.clone())));
    wait_for_mistakes(&mut app, &[]).await?;
    config.editor_config = true;
    app.handle_config_events(view::editor::ConfigEvent::Update(Box::new(config)));
    wait_for_mistakes(&mut app, &["teh"]).await?;
    keys(&mut app, ":spelling off<ret>").await?;
    let config = (*app.editor.config()).clone();
    app.handle_config_events(view::editor::ConfigEvent::Update(Box::new(config)));
    wait_for_mistakes(&mut app, &[]).await?;
    assert!(current_ref!(app.editor).1.spelling_languages.is_empty());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn incremental_edits_keep_distant_diagnostics_and_whole_tokens() -> anyhow::Result<()> {
    let mut config = test_config();
    config.editor.spelling.languages = Some(vec!["en_US".parse()?]);
    let text = format!(
        "#[t|]#he {} quik {} teh\n",
        "hello ".repeat(12),
        "world ".repeat(12)
    );
    let mut app = AppBuilder::new()
        .with_config(config)
        .with_input_text(text)
        .build()?;
    wait_for_mistakes(&mut app, &["quik", "teh"]).await?;
    replace(&mut app, 0, 3, "teh");
    wait_for_mistakes(&mut app, &["teh", "quik", "teh"]).await?;
    replace(&mut app, 0, 3, "the");
    wait_for_mistakes(&mut app, &["quik", "teh"]).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn coalesced_edits_restore_diagnostics_even_when_text_is_unchanged() -> anyhow::Result<()> {
    let mut app = AppBuilder::new()
        .with_input_text("#[t|]#eh quik\n")
        .build()?;
    keys(&mut app, ":spelling en_US<ret>").await?;
    wait_for_mistakes(&mut app, &["teh", "quik"]).await?;
    // Deleting a diagnostic's range removes it immediately. Restoring the text before the
    // debounce expires must restore the diagnostic too, despite the empty net text diff.
    replace(&mut app, 0, 3, "");
    replace(&mut app, 0, 0, "teh");
    assert_eq!(mistakes(&app), ["quik"]);
    wait_for_mistakes(&mut app, &["teh", "quik"]).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn syntax_changes_recheck_prose_beyond_the_edit_window() -> anyhow::Result<()> {
    let mut config = test_config();
    config.editor.spelling.languages = Some(vec!["en_US".parse()?]);
    let text = format!("#[/|]#* {} teh */\n", "hello ".repeat(30));
    let mut app = AppBuilder::new()
        .with_config(config)
        .with_input_text(text)
        .build()?;
    keys(&mut app, ":lang rust<ret>").await?;
    wait_for_mistakes(&mut app, &["teh"]).await?;
    replace(&mut app, 0, 2, "  ");
    wait_for_mistakes(&mut app, &[]).await?;
    replace(&mut app, 0, 2, "/*");
    wait_for_mistakes(&mut app, &["teh"]).await?;
    let (view, doc) = current!(app.editor);
    doc.set_selection(view.id, Selection::point(0));
    keys(&mut app, ":lang text<ret>").await?;
    wait_for_mistakes(&mut app, &["teh"]).await?;
    Ok(())
}
