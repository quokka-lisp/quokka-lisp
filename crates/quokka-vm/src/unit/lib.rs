//! Tests for the `quokka-vm` crate root.

use super::*;

#[test]
fn version_is_reported() {
    assert!(!VERSION.is_empty());
    // The phase moves; its shape does not. Pinning the string here would
    // only mean editing two places every time instead of one.
    assert!(PHASE.starts_with(|c: char| c.is_ascii_digit()));
}
