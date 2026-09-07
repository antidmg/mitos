use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn replace_selections() -> anyhow::Result<()> {
    for (input, keys, output) in [
        ("#[hello|]#", "<A-r>x<ret>", "#[x|]#"),
        (
            "#(foo|)# #[|longer]#",
            "<A-r>replacement<ret>",
            "#(replacement|)# #[|replacement]#",
        ),
        (
            "#[e\u{301}|]# #(界|)#",
            "<A-r>é界$1(<ret>",
            "#[é界$1(|]# #(é界$1(|)#",
        ),
        ("#[hello|]#\n", "<A-r><ret>", "#[\n|]#"),
    ] {
        test((input, keys, output)).await?;
    }
    test((
        "foo#[|]#",
        "<A-r>x<ret>",
        "foo#[x|]#",
        LineFeedHandling::AsIs,
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_commits_once_and_undoes_once() -> anyhow::Result<()> {
    let mut app = AppBuilder::new()
        .with_input_text("#[foo|]# #(bar|)#\n")
        .build()?;
    let (view, doc) = view::current_ref!(app.editor);
    let original_text = doc.text().clone();
    let original_selection = doc.selection(view.id).clone();
    let original_version = doc.version();

    test_key_sequences(
        &mut app,
        vec![
            (
                Some("<A-r>replacement"),
                Some(&|app| {
                    let (view, doc) = view::current_ref!(app.editor);
                    assert_eq!(doc.text(), &original_text);
                    assert_eq!(doc.selection(view.id), &original_selection);
                    assert_eq!(doc.version(), original_version);
                }),
            ),
            (
                Some("<ret>"),
                Some(&|app| {
                    assert_eq!(doc!(app.editor).version(), original_version + 1);
                    assert_eq!(
                        doc!(app.editor).text().to_string(),
                        "replacement replacement\n"
                    );
                }),
            ),
            (
                Some("u"),
                Some(&|app| {
                    let (view, doc) = view::current_ref!(app.editor);
                    assert_eq!(doc.text(), &original_text);
                    assert_eq!(doc.selection(view.id), &original_selection);
                }),
            ),
            (
                Some("U"),
                Some(&|app| {
                    assert_eq!(
                        doc!(app.editor).text().to_string(),
                        "replacement replacement\n"
                    );
                }),
            ),
        ],
        false,
    )
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_cancel_and_repeat() -> anyhow::Result<()> {
    test(("#[foo|]# bar", "<A-r>ignored<esc>", "#[foo|]# bar")).await?;
    test(("#[a|]#bc", "<A-r><ret>.", "#[c|]#")).await?;
    test((
        "#[foo|]# bar",
        "<A-r>x<ret>2lmiw<A-r>ignored<esc>.",
        "x #[x|]#",
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_cancel_preserves_insert_repeat() -> anyhow::Result<()> {
    test((
        "#[foo|]# bar",
        "cX<esc>2lmiw<A-r>ignored<esc>.",
        "X X#[\n|]#",
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn insert_after_replace_updates_repeat() -> anyhow::Result<()> {
    test((
        "#[foo|]# bar baz",
        "<A-r>x<ret>2lmiwcY<esc>2lmiw.",
        "x Y Y#[\n|]#",
    ))
    .await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_from_select_mode() -> anyhow::Result<()> {
    test(("#[foo|]# bar", "v<A-r>x<ret>l", "x#[ |]#bar")).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_char_keeps_character_semantics() -> anyhow::Result<()> {
    test(("#[he\u{301}llo|]#", "rx", "#[xxxxx|]#")).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn replace_multiline_register_text_in_crlf_document() -> anyhow::Result<()> {
    let mut app = AppBuilder::new()
        .with_input_text("#[foo|]# #(bar|)#\r\n")
        .build()?;
    {
        let (_, doc) = view::current!(app.editor);
        doc.line_ending = editor_core::LineEnding::Crlf;
    }
    app.editor.registers.write('a', vec!["one\ntwo".into()])?;
    test_key_sequence(
        &mut app,
        Some("<A-r><C-r>a<ret>"),
        Some(&|app| {
            let (view, doc) = view::current_ref!(app.editor);
            assert_eq!(
                editor_core::test::plain(doc.text().slice(..), doc.selection(view.id)),
                "#[one\r\ntwo|]# #(one\r\ntwo|)#\r\n"
            );
        }),
        false,
    )
    .await?;
    Ok(())
}
