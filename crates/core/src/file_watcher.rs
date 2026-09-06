//! Native file watching.
//!
//! Adapted from work by Pascal Kuthe and Blaž Hrastnik in
//! [Helix PR #14544](https://github.com/helix-editor/helix/pull/14544).

use std::borrow::Borrow;
use std::path::{Path, PathBuf};
use std::slice;
use std::sync::Arc;
use std::time::SystemTime;

// Re-export filesentry types (available on all platforms)
pub use filesentry::{CanonicalPathBuf, Event, EventType, Events, Filter, ShutdownOnDrop};

use event::{dispatch, events};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};

events! {
    FileSystemDidChange {
        fs_events: Events
    }
}

/// Resolve existing ancestors, including symlinks, while allowing a missing leaf.
/// Native events use real paths; editor paths may retain symlinks such as macOS `/var`.
pub fn canonicalize_path(path: &Path) -> PathBuf {
    let path = stdx::path::canonicalize(path);
    for ancestor in path.ancestors() {
        if let Ok(real) = ancestor.canonicalize() {
            return real.join(path.strip_prefix(ancestor).unwrap());
        }
    }
    path
}

/// Config for file watching
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct Config {
    /// Enable recursive native file watching.
    pub enable: bool,
    pub watch_vcs: bool,
    /// Only enable the file watcher inside Mitos workspaces (VCS repos and directories with .mitos
    /// directory) this prevents watching large directories like $HOME by default
    ///
    /// Defaults to `true`
    pub require_workspace: bool,
    /// Enables ignoring hidden files.
    pub hidden: bool,
    /// Enables reading `.ignore` files.
    pub ignore: bool,
    /// Enables reading `.gitignore` files.
    pub git_ignore: bool,
    /// Enables reading global .gitignore, whose path is specified in git's config: `core.excludefile` option.
    pub git_global: bool,
    /// Maximum depth below a watch root; deeper open files use polling.
    pub max_depth: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            enable: true,
            watch_vcs: true,
            require_workspace: true,
            hidden: true,
            ignore: true,
            git_ignore: true,
            git_global: true,
            max_depth: Some(10),
        }
    }
}

/// Recursive native watches, with coverage information for the polling fallback.
pub struct Watcher {
    watcher: Option<(filesentry::Watcher, ShutdownOnDrop)>,
    filter: Arc<WatchFilter>,
    workspace: PathBuf,
    roots: Vec<PathBuf>,
    active_roots: Vec<(PathBuf, Arc<std::sync::atomic::AtomicBool>)>,
    config: Config,
    extra_watched_paths: Vec<(PathBuf, Option<SystemTime>)>,
}

impl Watcher {
    pub fn new(config: &Config) -> Self {
        let (workspace, _) = loader::find_workspace();
        let mut watcher = Self {
            watcher: None,
            filter: Arc::new(WatchFilter::new(config, &workspace, [].into_iter())),
            workspace,
            roots: Vec::new(),
            active_roots: Vec::new(),
            config: config.clone(),
            extra_watched_paths: Vec::new(),
        };
        watcher.reload(config);
        watcher
    }

    /// Rebuild watches when configuration or the working directory changes.
    pub fn reload(&mut self, config: &Config) {
        let (workspace, no_workspace) = loader::find_workspace();
        let workspace = canonicalize_path(&workspace);
        if self.config == *config && self.workspace == workspace && self.watcher.is_some() {
            return;
        }
        self.config = config.clone();
        self.watcher = None;
        self.active_roots.clear();
        self.workspace = workspace;
        self.filter = Arc::new(WatchFilter::new(
            config,
            &self.workspace,
            self.roots.iter().map(PathBuf::as_path),
        ));
        if !config.enable || (config.require_workspace && no_workspace && self.roots.is_empty()) {
            return;
        }
        let watcher = match filesentry::Watcher::new() {
            Ok(watcher) => watcher,
            Err(err) => {
                log::info!("file watcher unavailable; using polling: {err}");
                return;
            }
        };
        watcher.set_filter(self.filter.clone(), false);
        // Native callbacks run on a watcher thread. Enter the editor's runtime so
        // runtime-local event registries also work in integration tests.
        let runtime = tokio::runtime::Handle::current();
        watcher.add_handler(move |events| {
            let _guard = runtime.enter();
            dispatch(FileSystemDidChange { fs_events: events });
            true
        });
        if !config.require_workspace || !no_workspace {
            self.watch_root(&watcher, self.workspace.clone());
        }
        for root in self.roots.clone() {
            self.watch_root(&watcher, root);
        }
        let shutdown = watcher.shutdown_guard();
        watcher.start();
        self.watcher = Some((watcher, shutdown));
    }

    fn watch_root(&mut self, watcher: &filesentry::Watcher, root: PathBuf) {
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ready_ = ready.clone();
        match watcher.add_root(&root, true, move |ok| {
            ready_.store(ok, std::sync::atomic::Ordering::Release)
        }) {
            Ok(_) => self.active_roots.push((root, ready)),
            Err(err) => log::warn!("failed to watch {}: {err}", root.display()),
        }
    }

    /// True only after a native root is ready and the path passes its filter.
    /// Deleted and not-yet-created paths are checked lexically as well.
    pub fn is_watching(&self, path: &Path) -> bool {
        let path = canonicalize_path(path);
        self.active_roots.iter().any(|(root, ready)| {
            ready.load(std::sync::atomic::Ordering::Acquire) && path.starts_with(root)
        }) && !self.filter.ignore_path_rec(&path, Some(path.is_dir()))
    }

    /// Invalidate cached ignore files and recrawl affected watch roots.
    pub fn refresh_filter(&mut self) {
        self.filter = Arc::new(WatchFilter::new(
            &self.config,
            &self.workspace,
            self.roots.iter().map(PathBuf::as_path),
        ));
        if let Some((watcher, _)) = &self.watcher {
            watcher.set_filter(self.filter.clone(), true);
        }
    }

    /// Add an explicit LSP root even when the working directory is not a workspace.
    pub fn add_root(&mut self, root: &Path) {
        let Ok(root) = root.canonicalize() else {
            return;
        };
        if !root.is_dir() || self.roots.contains(&root) {
            return;
        }
        self.roots.push(root.clone());
        self.refresh_filter();
        if let Some((watcher, _)) = &self.watcher {
            let watcher = watcher.clone();
            self.watch_root(&watcher, root);
        } else {
            self.reload(&self.config.clone());
        }
    }

    /// Track VCS metadata even when native watching is unavailable or filters exclude it.
    /// Preserve timestamps when the set is refreshed so changes are not swallowed.
    pub fn set_extra_watched_paths(&mut self, mut paths: Vec<PathBuf>) {
        paths.sort();
        paths.dedup();
        let old = std::mem::take(&mut self.extra_watched_paths);
        self.extra_watched_paths = paths
            .into_iter()
            .map(|path| {
                let mtime = old
                    .iter()
                    .find(|(old, _)| *old == path)
                    .map(|(_, time)| *time)
                    .unwrap_or_else(|| path.metadata().ok().and_then(|m| m.modified().ok()));
                (path, mtime)
            })
            .collect();
    }

    pub fn poll_extra_paths(&mut self) -> bool {
        let mut changed = false;
        for (path, previous) in &mut self.extra_watched_paths {
            let current = path.metadata().ok().and_then(|m| m.modified().ok());
            changed |= current != *previous;
            *previous = current;
        }
        changed
    }

    pub fn is_vcs_path(&self, path: &Path) -> bool {
        self.extra_watched_paths
            .iter()
            .any(|(watched, _)| watched == path)
    }
}

fn build_ignore(paths: impl IntoIterator<Item = PathBuf> + Clone, dir: &Path) -> Option<Gitignore> {
    let mut builder = GitignoreBuilder::new(dir);
    for path in paths.clone() {
        if let Some(err) = builder.add(&path)
            && !err.is_io()
        {
            log::error!("failed to read ignorefile at {path:?}: {err}");
        }
    }
    match builder.build() {
        Ok(ignore) => (!ignore.is_empty()).then_some(ignore),
        Err(err) => {
            if !err.is_io() {
                log::error!(
                    "failed to read ignorefile at {:?}: {err}",
                    paths.into_iter().collect::<Vec<_>>()
                );
            }
            None
        }
    }
}

struct IgnoreFiles {
    root: PathBuf,
    ignores: Vec<Arc<Gitignore>>,
}

impl IgnoreFiles {
    fn new(
        workspace_ignore: Option<Arc<Gitignore>>,
        config: &Config,
        root: &Path,
        globals: &[Arc<Gitignore>],
    ) -> Self {
        let mut ignores = Vec::with_capacity(8);
        // .mitos/ignore
        if let Some(workspace_ignore) = workspace_ignore {
            ignores.push(workspace_ignore);
        }
        for ancestor in root.ancestors() {
            let ignore = if config.ignore {
                if config.git_ignore {
                    // the second path takes priority
                    build_ignore(
                        [ancestor.join(".gitignore"), ancestor.join(".ignore")],
                        ancestor,
                    )
                } else {
                    build_ignore([ancestor.join(".ignore")], ancestor)
                }
            } else if config.git_ignore {
                build_ignore([ancestor.join(".gitignore")], ancestor)
            } else {
                None
            };
            if let Some(ignore) = ignore {
                ignores.push(Arc::new(ignore));
            }
        }
        ignores.extend(globals.iter().cloned());
        Self {
            root: root.into(),
            ignores,
        }
    }

    fn shared_ignores(
        workspace: &Path,
        config: &Config,
    ) -> (Vec<Arc<Gitignore>>, Option<Arc<Gitignore>>) {
        let mut ignores = Vec::new();
        let workspace_ignore = build_ignore(
            [
                loader::config_dir().join("ignore"),
                workspace.join(".mitos/ignore"),
            ],
            workspace,
        )
        .map(Arc::new);
        if config.git_global {
            let (gitignore_global, err) = Gitignore::global();
            if let Some(err) = err
                && !err.is_io()
            {
                log::error!("failed to read global ignore file: {err}");
            }
            if !gitignore_global.is_empty() {
                ignores.push(Arc::new(gitignore_global));
            }
        }
        (ignores, workspace_ignore)
    }

    fn filesentry_ignores(workspace: &Path) -> Gitignore {
        // the second path takes priority
        build_ignore(
            [
                loader::config_dir().join("filesentryignore"),
                workspace.join(".mitos/filesentryignore"),
            ],
            workspace,
        )
        .unwrap_or(Gitignore::empty())
    }

    fn is_ignored(
        ignores: &[impl Borrow<Gitignore>],
        path: &Path,
        is_dir: Option<bool>,
    ) -> Option<bool> {
        match is_dir {
            Some(is_dir) => {
                for ignore in ignores {
                    match ignore.borrow().matched(path, is_dir) {
                        ignore::Match::None => continue,
                        ignore::Match::Ignore(_) => return Some(true),
                        ignore::Match::Whitelist(_) => return Some(false),
                    }
                }
            }
            None => {
                // if we don't know whether this is a directory (on windows)
                // then we are conservative and allow the dirs
                for ignore in ignores {
                    match ignore.borrow().matched(path, true) {
                        ignore::Match::None => continue,
                        ignore::Match::Ignore(glob) => {
                            if glob.is_only_dir() {
                                match ignore.borrow().matched(path, false) {
                                    ignore::Match::None => continue,
                                    ignore::Match::Ignore(_) => return Some(true),
                                    ignore::Match::Whitelist(_) => return Some(false),
                                }
                            } else {
                                return Some(true);
                            }
                        }
                        ignore::Match::Whitelist(_) => return Some(false),
                    }
                }
            }
        }
        None
    }
}

/// A filter for hidden and ignored files. The point of this
/// is to avoid overwhelming the watcher with watching a ton of
/// files/directories (like the cargo target directory, node_modules or
/// VCS files) so ignoring a file is a performance optimization.
struct WatchFilter {
    filesentry_ignores: Gitignore,
    ignore_files: Vec<IgnoreFiles>,
    global_ignores: Vec<Arc<Gitignore>>,
    hidden: bool,
    watch_vcs: bool,
    max_depth: Option<usize>,
    config: Config,
    workspace_ignore: Option<Arc<Gitignore>>,
    nested_ignores:
        parking_lot::RwLock<std::collections::HashMap<PathBuf, Arc<Vec<Arc<Gitignore>>>>>,
}

impl WatchFilter {
    fn new<'a>(
        config: &Config,
        workspace: &'a Path,
        roots: impl Iterator<Item = &'a Path> + Clone,
    ) -> WatchFilter {
        let filesentry_ignores = IgnoreFiles::filesentry_ignores(workspace);
        let (global_ignores, workspace_ignore) = IgnoreFiles::shared_ignores(workspace, config);
        let ignore_files = roots
            .chain([workspace])
            .map(|root| IgnoreFiles::new(workspace_ignore.clone(), config, root, &global_ignores))
            .collect();
        WatchFilter {
            filesentry_ignores,
            ignore_files,
            global_ignores,
            hidden: config.hidden,
            watch_vcs: config.watch_vcs,
            max_depth: config.max_depth,
            config: config.clone(),
            workspace_ignore,
            nested_ignores: Default::default(),
        }
    }

    fn directory_ignores(&self, path: &Path, root: &Path) -> Option<Arc<Vec<Arc<Gitignore>>>> {
        let parent = path.parent()?;
        if parent == root || !parent.starts_with(root) {
            return None;
        }
        if let Some(ignores) = self.nested_ignores.read().get(parent) {
            return Some(ignores.clone());
        }
        let ignores = Arc::new(
            IgnoreFiles::new(
                self.workspace_ignore.clone(),
                &self.config,
                parent,
                &self.global_ignores,
            )
            .ignores,
        );
        self.nested_ignores
            .write()
            .insert(parent.to_path_buf(), ignores.clone());
        Some(ignores)
    }

    fn ignore_path_impl(
        &self,
        path: &Path,
        is_dir: Option<bool>,
        ignore_files: &[Arc<Gitignore>],
    ) -> bool {
        if let Some(ignore) =
            IgnoreFiles::is_ignored(slice::from_ref(&self.filesentry_ignores), path, is_dir)
        {
            return ignore;
        }
        if is_hardcoded_whitelist(path) {
            return false;
        }
        if is_hardcoded_blacklist(path, is_dir.unwrap_or(false)) {
            return true;
        }
        if let Some(ignore) = IgnoreFiles::is_ignored(ignore_files, path, is_dir) {
            return ignore;
        }
        // ignore .git directory except .git/HEAD (and .git itself)
        if is_vcs_ignore(path, self.watch_vcs) {
            return true;
        }
        self.hidden && is_hidden(path)
    }
}

impl filesentry::Filter for WatchFilter {
    fn ignore_path(&self, path: &Path, is_dir: Option<bool>) -> bool {
        let (root, ignore_files) = self
            .ignore_files
            .iter()
            .filter(|files| path.starts_with(&files.root))
            .max_by_key(|files| files.root.components().count())
            .map_or((Path::new(""), &self.global_ignores), |files| {
                (&files.root, &files.ignores)
            });
        if path == root {
            return false;
        }
        if self.max_depth.is_some_and(|depth| {
            path.strip_prefix(root)
                .is_ok_and(|p| p.components().count() > depth)
        }) {
            return true;
        }
        let nested = self.directory_ignores(path, root);
        self.ignore_path_impl(
            path,
            is_dir,
            nested.as_deref().map(Vec::as_slice).unwrap_or(ignore_files),
        )
    }

    fn ignore_path_rec(&self, mut path: &Path, mut is_dir: Option<bool>) -> bool {
        let (root, ignore_files) = self
            .ignore_files
            .iter()
            .filter(|files| path.starts_with(&files.root))
            .max_by_key(|files| files.root.components().count())
            .map_or((Path::new(""), &self.global_ignores), |files| {
                (&files.root, &files.ignores)
            });
        if self.max_depth.is_some_and(|depth| {
            path.strip_prefix(root)
                .is_ok_and(|p| p.components().count() > depth)
        }) {
            return true;
        }
        loop {
            if path == root {
                return false;
            }
            let nested = self.directory_ignores(path, root);
            if self.ignore_path_impl(
                path,
                is_dir,
                nested.as_deref().map(Vec::as_slice).unwrap_or(ignore_files),
            ) {
                return true;
            }
            let Some(parent) = path.parent() else {
                break;
            };
            path = parent;
            // Ancestors of the leaf are always directories. Passing the leaf's
            // `is_dir` up the chain makes a dir-only pattern (gitignore `target/`)
            // miss the ancestor directory it names.
            is_dir = Some(true);
        }
        false
    }
}

fn is_hidden(path: &Path) -> bool {
    path.file_name().is_some_and(|it| {
        it.as_encoded_bytes().first() == Some(&b'.')
        // handled by vcs ignore rules
        && it != ".git"
    })
}

// hidden directories we want to watch by default
fn is_hardcoded_whitelist(path: &Path) -> bool {
    path.ends_with(".gitignore")
        | path.ends_with(".ignore")
        | path.ends_with(".mitos")
        | path.ends_with(".github")
        | path.ends_with(".cargo")
        | path.ends_with(".envrc")
}

fn is_hardcoded_blacklist(path: &Path, is_dir: bool) -> bool {
    // don't descend into the cargo registry and similar
    path.parent()
        .is_some_and(|parent| parent.ends_with(".cargo"))
        && is_dir
}

fn file_name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|it| it.to_str())
}

fn is_vcs_ignore(path: &Path, watch_vcs: bool) -> bool {
    // ignore .git directory contents except .git/HEAD (and .git itself)
    // Note: only checks immediate parent; recursive checking is done by ignore_path_rec
    if watch_vcs
        && path.parent().is_some_and(|it| it.ends_with(".git"))
        && !path.ends_with(".git/HEAD")
    {
        return true;
    }
    match file_name(path) {
        Some(".jj" | ".svn" | ".hg") => true,
        Some(".git") => !watch_vcs,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::file_watcher::{is_hardcoded_whitelist, is_hidden, is_vcs_ignore};

    #[test]
    fn test_vcs_ignore() {
        assert!(!is_vcs_ignore(Path::new(".git"), true));
        assert!(!is_vcs_ignore(Path::new(".git/HEAD"), true));
        assert!(is_vcs_ignore(Path::new(".git/foo"), true));
        // Note: .git/foo/bar is NOT caught by is_vcs_ignore (only checks immediate parent)
        // but it IS caught by ignore_path_rec which checks ancestors recursively
        assert!(!is_vcs_ignore(Path::new(".git/foo/bar"), true));
        assert!(!is_vcs_ignore(Path::new(".foo"), true));
        assert!(is_vcs_ignore(Path::new(".jj"), true));
        assert!(is_vcs_ignore(Path::new(".svn"), true));
        assert!(is_vcs_ignore(Path::new(".hg"), true));
    }

    #[test]
    fn test_hidden() {
        assert!(is_hidden(Path::new(".foo")));
        // handled by vcs ignore rules
        assert!(!is_hidden(Path::new(".git")));
    }

    #[test]
    fn test_whitelist() {
        // Note: .git is NOT in whitelist - it has special handling in is_vcs_ignore and is_hidden
        assert!(is_hardcoded_whitelist(Path::new(".mitos")));
        assert!(is_hardcoded_whitelist(Path::new(".github")));
        assert!(!is_hardcoded_whitelist(Path::new(".githup")));
    }

    #[test]
    fn ignore_path_rec_treats_ancestors_as_dirs() {
        use std::sync::Arc;

        use filesentry::Filter;
        use ignore::gitignore::{Gitignore, GitignoreBuilder};

        use crate::file_watcher::{Config, IgnoreFiles, WatchFilter};

        let mut builder = GitignoreBuilder::new("/repo");
        // dir-only pattern: matches the `target` directory, not a file named target
        builder.add_line(None, "target/").unwrap();
        let ignores = builder.build().unwrap();
        let filter = WatchFilter {
            filesentry_ignores: Gitignore::empty(),
            ignore_files: vec![IgnoreFiles {
                root: "/repo".into(),
                ignores: vec![Arc::new(ignores)],
            }],
            global_ignores: Vec::new(),
            hidden: true,
            watch_vcs: true,
            max_depth: None,
            config: Config::default(),
            workspace_ignore: None,
            nested_ignores: Default::default(),
        };

        // A file under `target/` is ignored even though the leaf is passed as a file:
        // the walk must re-check the `target` ancestor as a directory.
        assert!(filter.ignore_path_rec(Path::new("/repo/target/foo.rs"), Some(false)));
        assert!(filter.ignore_path_rec(Path::new("/repo/target/deep/foo.rs"), Some(false)));
        assert!(!filter.ignore_path_rec(Path::new("/repo/src/main.rs"), Some(false)));
    }
    #[test]
    fn filters_nested_ignores_hidden_paths_depth_and_independent_roots() {
        use super::{Config, Filter, WatchFilter};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("src")).unwrap();
        std::fs::write(root.join("src/.gitignore"), "generated/\n").unwrap();
        let other = tempfile::tempdir().unwrap();
        let other = other.path().canonicalize().unwrap();
        let config = Config {
            git_global: false,
            max_depth: Some(3),
            ..Config::default()
        };
        let filter = WatchFilter::new(&config, &root, [other.as_path()].into_iter());
        assert!(filter.ignore_path_rec(&root.join("src/generated/file.rs"), Some(false)));
        assert!(filter.ignore_path_rec(&root.join(".hidden/file.rs"), Some(false)));
        assert!(filter.ignore_path_rec(&root.join("a/b/c/deep.rs"), Some(false)));
        assert!(!filter.ignore_path_rec(&root.join("src/main.rs"), Some(false)));
        assert!(!filter.ignore_path_rec(&other.join("src/generated/file.rs"), Some(false)));
        assert!(!filter.ignore_path_rec(&root.join(".mitos/config.toml"), Some(false)));
    }

    #[test]
    fn polling_tracks_missing_metadata_and_preserves_timestamps_when_refreshed() {
        use super::{Config, Watcher};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("HEAD");
        let mut watcher = Watcher::new(&Config {
            enable: false,
            ..Config::default()
        });
        watcher.set_extra_watched_paths(vec![path.clone()]);
        assert!(!watcher.poll_extra_paths());
        std::fs::write(&path, "ref: refs/heads/main\n").unwrap();
        watcher.set_extra_watched_paths(vec![path.clone(), path.clone()]);
        assert!(watcher.poll_extra_paths());
        assert!(!watcher.poll_extra_paths());
        std::fs::remove_file(path).unwrap();
        assert!(watcher.poll_extra_paths());
    }
}
