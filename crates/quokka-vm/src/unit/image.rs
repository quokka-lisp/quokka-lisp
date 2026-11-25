//! Unit tests of `image.rs` that need its private items and nothing else.
//! The rest — saving a booted world, resuming a stopped one — need the
//! interpreter to set them up, and live with it in `quokka-bootstrap`'s
//! `tests/image.rs` (#29).

use super::*;

/// The checksum has to actually distinguish things, which is worth one test
/// rather than an assumption.
#[test]
fn the_checksum_notices_a_difference() {
    assert_ne!(
        checksum([1, 2, 3].into_iter()),
        checksum([1, 2, 4].into_iter())
    );
    assert_ne!(
        checksum([1, 2, 3].into_iter()),
        checksum([3, 2, 1].into_iter())
    );
    assert_eq!(
        checksum([1, 2, 3].into_iter()),
        checksum([1, 2, 3].into_iter())
    );
}
