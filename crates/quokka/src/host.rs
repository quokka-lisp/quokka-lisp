//! The native host: what a world is given of the machine it runs on.
//!
//! A world has no file system, console or clock of its own. It asks, by
//! leaving a request in its heap, and a host answers between slices
//! (`quokka_vm::host`). This is the host for a terminal: the console is
//! stdout, and source files are files — those under a directory the world
//! was granted, and no others (#87).

use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use quokka_vm::host::{Host, HostError};

use crate::usage_error;

/// The native host: real files, real console.
pub(crate) struct NativeHost;

impl Host for NativeHost {
    fn write_stdout(&mut self, s: &str) {
        print!("{s}");
        let _ = io::stdout().flush();
    }

    /// A source file, for a world that asks by name.
    ///
    /// Only one under a directory this world was granted ([`permitted`]),
    /// only a regular file, and only one of a size a source file has (#87).
    /// A world's request is answered between slices, where no step budget
    /// applies: a FIFO would hold the host for ever, and a 100 MB file
    /// became a string of 1.7 GB.
    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        let grants = GRANTS.get().map(Vec::as_slice).unwrap_or(&[]);
        // A relative name is relative to the root, which is the first
        // grant: the world's idea of "here" is the directory it was given.
        let Some(base) = grants.first() else {
            return Err(HostError::Denied(format!(
                "{name}: this world may read nothing"
            )));
        };
        // What is opened is the path that was checked, with every link
        // already followed — not the name, which could be made to mean
        // something else between the check and the read.
        let file = permitted(grants, base, name)?;
        let name = file.to_str().unwrap_or(name);
        match std::fs::metadata(name) {
            Ok(file) if !file.is_file() => {
                return Err(HostError::Denied(format!("{name}, which is not a file")))
            }
            Ok(file) if file.len() > SOURCE_LIMIT => {
                return Err(HostError::Denied(format!(
                    "{name}, which at {} bytes is too large to be source",
                    file.len()
                )))
            }
            // Missing or unreadable: the read below says which.
            _ => {}
        }
        std::fs::read_to_string(name).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => HostError::NotFound(name.to_string()),
            io::ErrorKind::PermissionDenied => HostError::Denied(name.to_string()),
            _ => HostError::Other(e.to_string()),
        })
    }
}

/// The directories this process's worlds may read from: the current one
/// unless `--root` named another, and whatever `--allow-read` added. Set
/// once, in `main`, before any world runs; a host answers for the process.
pub(crate) static GRANTS: OnceLock<Vec<PathBuf>> = OnceLock::new();

/// The file NAME names, if a world granted GRANTS may read it.
///
/// **A world reaches the outside only through its host, and a host grants a
/// directory, never the user's whole authority** (#87). Images are meant to
/// be shared, and opening one runs its code; `%read-source` used to read
/// any file the user could, so a downloaded world could read `~/.ssh`. This
/// is the *capability* shape — authority is what was handed over, not what
/// the user could do — that WASI's preopened directories and Deno's
/// `--allow-read` have, and that a `local:/` file host will stand on.
///
/// The check is on where the name **leads**, not on how it is spelt. `..`
/// and an absolute path are the obvious ways out and a symbolic link is the
/// quiet one: `root/notes` is inside the root as text and may be a link to
/// `/etc`. So the name is resolved first — `canonicalize` follows every
/// link and every `..`, as the kernel would — and it is the resolved path
/// that must lie under a grant, and the resolved path that is then opened.
/// The grants were resolved the same way when they were given.
///
/// A name that leads nowhere still must not say more than "not found"
/// about a place the world may not look. So for a missing file the nearest
/// directory that does exist is resolved and checked: outside the grants
/// the answer is the refusal, the same as for a file that is there.
pub(crate) fn permitted(grants: &[PathBuf], base: &Path, name: &str) -> Result<PathBuf, HostError> {
    let inside = |real: &Path| grants.iter().any(|grant| real.starts_with(grant));
    let refused = || {
        HostError::Denied(format!(
            "{name}, which is outside what this world may read \
             (--root DIR or --allow-read PATH grants more)"
        ))
    };
    let asked = base.join(name);
    if let Ok(real) = asked.canonicalize() {
        return if inside(&real) {
            Ok(real)
        } else {
            Err(refused())
        };
    }
    // It does not exist. Take `.` and `..` out as text, then find the
    // deepest ancestor that does exist and see where *that* really is.
    let mut plain = PathBuf::new();
    for part in asked.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                plain.pop();
            }
            other => plain.push(other),
        }
    }
    let exists = plain.ancestors().find_map(|a| a.canonicalize().ok());
    match exists {
        Some(real) if inside(&real) => Err(HostError::NotFound(name.to_string())),
        _ => Err(refused()),
    }
}

/// Take `--root DIR` and `--allow-read PATH` out of the arguments and make
/// them the process's grants; what is left is the command.
pub(crate) fn grant(arguments: Vec<String>) -> Vec<String> {
    let resolve = |flag: &str, path: Option<String>| -> PathBuf {
        let Some(path) = path else {
            usage_error(&format!("{flag} wants a path after it"));
        };
        // Resolved now, links and all, so that a grant is a real place and
        // the comparison in `permitted` is between two real places.
        std::fs::canonicalize(&path).unwrap_or_else(|e| usage_error(&format!("{flag} {path}: {e}")))
    };
    let (mut root, mut more, mut rest) = (None, Vec::new(), Vec::new());
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => root = Some(resolve("--root", arguments.next())),
            "--allow-read" => more.push(resolve("--allow-read", arguments.next())),
            _ => rest.push(argument),
        }
    }
    let root = root.unwrap_or_else(|| resolve("the current directory", Some(".".to_string())));
    more.insert(0, root);
    GRANTS.set(more).expect("the grants are set once");
    rest
}

/// The largest file a world may read as source: sixteen megabytes, about
/// forty times all of Quokka's own Lisp.
pub(crate) const SOURCE_LIMIT: u64 = 16 << 20;

#[cfg(test)]
#[path = "unit/host.rs"]
mod tests;
