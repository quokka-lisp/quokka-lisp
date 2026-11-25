//! Unit tests of the native host: what a world may read (#87).

use super::*;

/// A directory of this test's own, with a root, a file in it, and a secret
/// beside the root — all resolved, since a temporary directory is itself
/// often reached through a link (`/tmp` on macOS).
struct Place {
    top: PathBuf,
    root: PathBuf,
}

impl Place {
    fn new(name: &str) -> Place {
        let top = std::env::temp_dir().join(format!("quokka-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        std::fs::create_dir_all(top.join("root/lisp")).unwrap();
        std::fs::write(top.join("root/lisp/a.lisp"), "(+ 1 2)").unwrap();
        std::fs::write(top.join("secret"), "a key").unwrap();
        let top = top.canonicalize().unwrap();
        let root = top.join("root");
        Place { top, root }
    }

    fn ask(&self, name: &str) -> Result<PathBuf, HostError> {
        permitted(std::slice::from_ref(&self.root), &self.root, name)
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.top);
    }
}

fn denied(answer: Result<PathBuf, HostError>) -> bool {
    matches!(answer, Err(HostError::Denied(_)))
}

#[test]
fn a_file_under_the_root_is_read() {
    let place = Place::new("inside");
    assert_eq!(place.ask("lisp/a.lisp"), Ok(place.root.join("lisp/a.lisp")));
    assert_eq!(
        place.ask("./lisp/../lisp/a.lisp"),
        Ok(place.root.join("lisp/a.lisp"))
    );
    assert!(matches!(
        place.ask("lisp/missing.lisp"),
        Err(HostError::NotFound(_))
    ));
}

/// The two obvious ways out.
#[test]
fn dots_and_absolute_paths_do_not_leave_the_root() {
    let place = Place::new("dots");
    assert!(denied(place.ask("../secret")));
    assert!(denied(place.ask("lisp/../../secret")));
    assert!(denied(
        place.ask(place.top.join("secret").to_str().unwrap())
    ));
    assert!(denied(place.ask("/etc/hosts")));
}

/// **The quiet way out** (#87): a link inside the root is inside it as text
/// and may lead anywhere. To a file, to a directory, through a chain of
/// links, or by a link whose own target climbs out with `..` — each is
/// judged by where it leads.
#[cfg(unix)]
#[test]
fn a_symbolic_link_does_not_leave_the_root() {
    use std::os::unix::fs::symlink;
    let place = Place::new("links");
    symlink(place.top.join("secret"), place.root.join("to-file")).unwrap();
    symlink(&place.top, place.root.join("to-dir")).unwrap();
    symlink("to-dir", place.root.join("chain")).unwrap();
    symlink("../secret", place.root.join("relative")).unwrap();
    assert!(denied(place.ask("to-file")));
    assert!(denied(place.ask("to-dir/secret")));
    assert!(denied(place.ask("chain/secret")));
    assert!(denied(place.ask("relative")));
    // A link that stays inside is only another name for a file inside.
    symlink("lisp/a.lisp", place.root.join("alias")).unwrap();
    assert_eq!(place.ask("alias"), Ok(place.root.join("lisp/a.lisp")));
}

/// A missing file outside the root is refused, not "not found": the
/// difference would tell a world which files exist where it may not look.
#[cfg(unix)]
#[test]
fn nothing_is_said_about_what_is_outside() {
    use std::os::unix::fs::symlink;
    let place = Place::new("probe");
    symlink(&place.top, place.root.join("out")).unwrap();
    assert!(denied(place.ask("../no-such-file")));
    assert!(denied(place.ask("out/no-such-file")));
    assert!(denied(place.ask("/no-such-directory/file")));
}

/// `--allow-read` adds a place; it does not open everything.
#[test]
fn a_further_grant_adds_one_place() {
    let place = Place::new("grant");
    let grants = [place.root.clone(), place.top.join("secret")];
    let ask = |name: &str| permitted(&grants, &place.root, name);
    assert_eq!(ask("../secret"), Ok(place.top.join("secret")));
    assert!(denied(ask("../other")));
}
