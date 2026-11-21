//! Tests for [`crate::host`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.

use super::*;

#[test]
fn null_host_collects_output_and_has_no_files() {
    let mut host = NullHost::default();
    host.write_stdout("hello");
    assert_eq!(host.output, "hello");
    assert_eq!(
        host.read_source("boot.lisp"),
        Err(HostError::NotFound("boot.lisp".to_string()))
    );
}
