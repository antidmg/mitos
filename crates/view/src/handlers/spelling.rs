//! Spell checking as a non-LSP diagnostic source.
//!
//! Adapted from Michael Davis's [Helix PR #15910](https://github.com/helix-editor/helix/pull/15910),
//! revision `1849ccc29792484a1e00bcc48ee8018ca058cf33`.
//!
//! This is the editor-side state for the spell checker. The detection logic (the debounced hook,
//! dictionary loading and the word checking itself) lives in `term`'s spelling handler, which
//! drives this state through [`SpellingEvent`]s and the editor's dictionaries.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use editor_core::{
    diagnostic::DiagnosticProvider, ChangeSet, SpellingLanguage, Tendril, Transaction,
};
use event::{send_blocking, TaskController, TaskHandle};
use tokio::sync::mpsc::Sender;

use crate::{action::Action, Dictionary, DocumentId, Editor};

#[derive(Debug)]
pub struct SpellingHandler {
    pub event_tx: Sender<SpellingEvent>,
    /// In-flight full-document checks, keyed by document. Starting a new full check for a document
    /// cancels the previous one (incremental checks run synchronously and need no cancellation).
    pub requests: HashMap<DocumentId, TaskController>,
    /// Languages whose dictionary is currently being loaded, so the same one isn't loaded twice
    /// concurrently.
    pub loading_dictionaries: HashSet<SpellingLanguage>,
}

impl SpellingHandler {
    pub fn new(event_tx: Sender<SpellingEvent>) -> Self {
        Self {
            event_tx,
            requests: HashMap::new(),
            loading_dictionaries: HashSet::new(),
        }
    }

    /// Registers a new in-flight full check for `document`, cancelling any previous one, and
    /// returns a handle the background task uses to observe cancellation.
    pub fn open_request(&mut self, document: DocumentId) -> TaskHandle {
        let mut controller = TaskController::new();
        let handle = controller.restart();
        self.requests.insert(document, controller);
        handle
    }
}

#[derive(Debug)]
pub enum SpellingEvent {
    /// A dictionary finished loading; (re-)check the open documents that use it.
    DictionaryLoaded { language: SpellingLanguage },
    /// A document was opened or its spelling settings changed; check it in full.
    CheckRequested { doc: DocumentId },
    /// A document was closed; discard its pending edits.
    DocumentClosed { doc: DocumentId },
    /// A document changed; re-check the regions around the change (or rescan, see the term-side
    /// handler). The version identifies the text these ranges apply to.
    DocumentChanged {
        doc: DocumentId,
        changes: ChangeSet,
        version: i32,
    },
}

/// Spelling actions sort after LSP code actions (which use a higher priority).
const SPELLING_ACTION_PRIORITY: u8 = 0;

/// Save an accepted word and publish a new dictionary snapshot. The personal dictionary file is
/// created if needed and read back when the dictionary is loaded in a later session.
fn add_personal_word(
    dictionary: &mut Arc<Dictionary>,
    path: &Path,
    word: &str,
) -> anyhow::Result<()> {
    // Workers keep immutable snapshots, so accepting a word never waits for a running scan.
    // Publish only after both validation and persistence succeed.
    let mut updated = (**dictionary).clone();
    updated
        .add(word)
        .map_err(|err| anyhow::anyhow!("could not add '{word}': {err:?}"))?;
    append_personal_word(path, word)?;
    *dictionary = Arc::new(updated);
    Ok(())
}

fn append_personal_word(path: &Path, word: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{word}")
}

/// Load accepted words from a personal dictionary, allowing the file to be absent.
pub fn load_personal_dictionary(dictionary: &mut Dictionary, path: &Path) -> std::io::Result<()> {
    use std::io::{BufRead as _, BufReader, ErrorKind};
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    for line in BufReader::new(file).lines() {
        let word = line?;
        let word = word.trim();
        if !word.is_empty()
            && let Err(err) = dictionary.add(word)
        {
            log::warn!("ignoring personal dictionary entry {word:?}: {err:?}");
        }
    }
    Ok(())
}

impl Editor {
    /// Invalidate old checks and diagnostics after changing a document's spelling settings.
    pub fn refresh_spelling(&mut self, doc_id: DocumentId) {
        self.handlers.spelling.requests.remove(&doc_id);
        let Some(doc) = self.documents.get_mut(&doc_id) else {
            return;
        };
        doc.detect_spelling_languages();
        doc.replace_diagnostics([], &[], Some(&DiagnosticProvider::Spelling));
        if !doc.spelling_languages.is_empty() {
            send_blocking(
                &self.handlers.spelling.event_tx,
                SpellingEvent::CheckRequested { doc: doc_id },
            );
        }
        event::dispatch(crate::events::DiagnosticsDidChange {
            editor: self,
            doc: doc_id,
        });
    }

    /// Code actions for the spelling diagnostics overlapping the primary selection: a replacement
    /// for each of the dictionary's suggestions, plus an "add to dictionary" action.
    pub fn spelling_actions(&self) -> Vec<Action> {
        let (view, doc) = current_ref!(self);
        // The dictionaries this document is checked against, in configuration order.
        let dictionaries: Vec<(SpellingLanguage, _)> = doc
            .spelling_languages
            .iter()
            .filter_map(|language| Some((language.clone(), self.dictionaries.get(language)?)))
            .collect();
        if dictionaries.is_empty() {
            return Vec::new();
        }
        let doc_id = doc.id();
        let view_id = view.id;
        let selection = doc.selection(view_id).primary();
        let text = doc.text();

        let mut suggestions = Vec::new();
        let mut actions = Vec::new();
        for diagnostic in doc.diagnostics() {
            if diagnostic.provider != DiagnosticProvider::Spelling {
                continue;
            }
            let range = diagnostic.range;
            if !selection.overlaps(&editor_core::Range::new(range.start, range.end)) {
                continue;
            }
            let word = Cow::<str>::from(text.slice(range.start..range.end)).into_owned();

            // Offer the suggestions from every dictionary, in order, without duplicates.
            suggestions.clear();
            for (_, dictionary) in &dictionaries {
                let mut candidates = Vec::new();
                dictionary.suggest(&word, &mut candidates);
                suggestions.extend(candidates);
            }
            let mut seen = HashSet::new();
            suggestions.retain(|suggestion| seen.insert(suggestion.clone()));
            for suggestion in &suggestions {
                let suggestion = suggestion.clone();
                let title = format!("Replace '{word}' with '{suggestion}'");
                let version = doc.version();
                actions.push(Action::new(
                    title,
                    SPELLING_ACTION_PRIORITY,
                    move |editor| {
                        let Some(doc) = editor.documents.get_mut(&doc_id) else {
                            return;
                        };
                        let Some(view) = editor.tree.try_get(view_id) else {
                            return;
                        };
                        // A file reload or edit may have invalidated the menu's captured range.
                        if doc.version() != version || view.doc != doc_id {
                            return;
                        }
                        let view = editor.tree.get_mut(view_id);
                        let transaction = Transaction::change(
                            doc.text(),
                            std::iter::once((
                                range.start,
                                range.end,
                                Some(Tendril::from(&*suggestion)),
                            )),
                        );
                        doc.apply(&transaction, view_id);
                        doc.append_changes_to_history(view);
                    },
                ));
            }

            // "Add to dictionary" targets one dictionary, so offer one action per language.
            for (language, _) in &dictionaries {
                let language = language.clone();
                let word = word.clone();
                let title = format!("Add '{word}' to dictionary '{language}'");
                actions.push(Action::new(
                    title,
                    SPELLING_ACTION_PRIORITY,
                    move |editor| {
                        let Some(dictionary) = editor.dictionaries.get_mut(&language) else {
                            return;
                        };
                        let path = loader::personal_dictionary_file(language.as_str());
                        if let Err(err) = add_personal_word(dictionary, &path, &word) {
                            log::error!(
                                "could not persist '{word}' to the personal dictionary: {err}"
                            );
                            editor.set_error(|| {
                                format!("Could not save personal dictionary '{language}': {err}")
                            });
                            return;
                        }
                        // The dictionary's contents changed; re-check the open documents using it.
                        send_blocking(
                            &editor.handlers.spelling.event_tx,
                            SpellingEvent::DictionaryLoaded {
                                language: language.clone(),
                            },
                        );
                    },
                ));
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn personal_words_persist_and_remain_separate_by_language() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dictionaries/en_US.txt");
        append_personal_word(&path, "Mitos").unwrap();
        append_personal_word(&path, "spellbook").unwrap();
        let mut dictionary = Dictionary::new("SET UTF-8\n", "1\nhello\n").unwrap();
        assert!(!dictionary.check("Mitos"));
        load_personal_dictionary(&mut dictionary, &dir.path().join("de_DE.txt")).unwrap();
        assert!(!dictionary.check("Mitos"));
        load_personal_dictionary(&mut dictionary, &path).unwrap();
        assert!(dictionary.check("Mitos"));
        assert!(dictionary.check("spellbook"));
        assert!(dictionary.check("hello"));
    }

    #[test]
    fn accepting_a_word_preserves_snapshots_and_requires_a_successful_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("en_US.txt");
        let mut dictionary = Arc::new(Dictionary::new("SET UTF-8\n", "1\nhello\n").unwrap());
        let snapshot = dictionary.clone();
        // A directory cannot be opened as an append-only file, even when running as root.
        assert!(add_personal_word(&mut dictionary, dir.path(), "Mitos").is_err());
        assert!(Arc::ptr_eq(&snapshot, &dictionary));
        assert!(!dictionary.check("Mitos"));

        add_personal_word(&mut dictionary, &path, "Mitos").unwrap();
        assert!(dictionary.check("Mitos"));
        assert!(!snapshot.check("Mitos"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "Mitos\n");
    }
}
