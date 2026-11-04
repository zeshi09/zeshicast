//! `.gitignore` for the file index: inside a git repository, what git
//! ignores (build output, dependencies, caches) stays out of search.
//!
//! Like `fd`, only the `.gitignore` files of a repository count, and only
//! below `$HOME`: no global excludes file, nothing outside a repository, and
//! a repository at `$HOME` itself (a dotfiles checkout often ignores `*`) is
//! not one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The `.gitignore` files that apply in a folder, repository root first;
/// `None` outside a repository.
type Chain = Option<Arc<Vec<Gitignore>>>;

pub(crate) struct GitIgnores {
    top: PathBuf,
    /// Folder → its chain, so each `.gitignore` is read once per scan.
    chains: RefCell<HashMap<PathBuf, Chain>>,
}

impl GitIgnores {
    /// Repositories strictly below `top` (`$HOME`).
    pub(crate) fn new(top: &Path) -> Self {
        Self {
            top: top.to_path_buf(),
            chains: RefCell::new(HashMap::new()),
        }
    }

    /// Forget what was read, so an edited `.gitignore` applies from the next
    /// lookup on.
    pub(crate) fn forget(&self) {
        self.chains.borrow_mut().clear();
    }

    /// Whether git ignores `path`.
    pub(crate) fn ignores(&self, path: &Path, is_dir: bool) -> bool {
        let Some(chain) = path.parent().and_then(|parent| self.chain(parent)) else {
            return false;
        };
        // The deepest `.gitignore` that has a say wins; `!pattern` re-includes.
        for gitignore in chain.iter().rev() {
            match gitignore.matched(path, is_dir) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
        false
    }

    fn chain(&self, dir: &Path) -> Chain {
        let cached = self.chains.borrow().get(dir).cloned();
        if let Some(chain) = cached {
            return chain;
        }
        let chain = if dir == self.top || !dir.starts_with(&self.top) {
            None
        } else if dir.join(".git").exists() {
            Some(Arc::new(read(dir).into_iter().collect()))
        } else {
            dir.parent()
                .and_then(|parent| self.chain(parent))
                .map(|outer| match read(dir) {
                    Some(own) => {
                        let mut chain = (*outer).clone();
                        chain.push(own);
                        Arc::new(chain)
                    }
                    None => outer,
                })
        };
        self.chains
            .borrow_mut()
            .insert(dir.to_path_buf(), chain.clone());
        chain
    }
}

/// `dir/.gitignore`, if there is a readable one.
fn read(dir: &Path) -> Option<Gitignore> {
    let file = dir.join(".gitignore");
    if !file.is_file() {
        return None;
    }
    let mut builder = GitignoreBuilder::new(dir);
    if let Some(error) = builder.add(&file) {
        log::debug!("{}: {error}", file.display());
    }
    builder.build().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_repository_ignores_what_git_would() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let repo = home.join("Code/app");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        write(&repo.join(".gitignore"), "dist/\n*.log\n!keep.log\n");
        write(&repo.join("web/.gitignore"), "generated.ts\n");
        let ignores = GitIgnores::new(home);

        assert!(ignores.ignores(&repo.join("dist"), true));
        assert!(
            !ignores.ignores(&repo.join("dist"), false),
            "dist/ is folders only"
        );
        assert!(ignores.ignores(&repo.join("web/debug.log"), false));
        assert!(
            !ignores.ignores(&repo.join("keep.log"), false),
            "re-included"
        );
        assert!(ignores.ignores(&repo.join("web/generated.ts"), false));
        assert!(
            !ignores.ignores(&repo.join("generated.ts"), false),
            "nested rule stays nested"
        );
        assert!(!ignores.ignores(&repo.join("src/main.rs"), false));
    }

    #[test]
    fn outside_a_repository_and_at_home_nothing_is_ignored() {
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        write(&home.join("Notes/.gitignore"), "*.md\n");
        std::fs::create_dir_all(home.join(".git")).unwrap();
        write(&home.join(".gitignore"), "*\n");
        let ignores = GitIgnores::new(home);

        assert!(
            !ignores.ignores(&home.join("Notes/todo.md"), false),
            "no .git"
        );
        assert!(
            !ignores.ignores(&home.join("Downloads"), true),
            "dotfiles repo at ~"
        );

        write(&home.join("Notes/.git/HEAD"), "");
        assert!(
            !ignores.ignores(&home.join("Notes/todo.md"), false),
            "cached"
        );
        ignores.forget();
        assert!(
            ignores.ignores(&home.join("Notes/todo.md"), false),
            "re-read"
        );
    }
}
