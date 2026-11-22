//! Tests for [`crate::image`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items — `checksum`, in particular, which is not something
//! the rest of the system should be able to reach.

use super::*;
use crate::boot;
use crate::eval::eval_forms;
use crate::printer::print;

/// Boot a heap, run `source`, and give back what the last form printed as.
fn answer(heap: &mut Heap, source: &str) -> String {
    let mut out = String::new();
    let _ = eval_forms(heap, source, 10_000_000, 2_000, |mc, result| {
        out = match result {
            Ok(v) => print(mc, v),
            Err(e) => format!("; {e}"),
        };
        true
    });
    out
}

/// The round trip the design's test plan asks for: save, reload, and find the
/// same heap.
#[test]
fn an_image_round_trips() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    assert_eq!(
        answer(&mut heap, "(defun double (x) (* 2 x)) (double 21)"),
        "42"
    );

    let bytes = save(&mut heap);
    let mut reloaded = load(&bytes, heap.soft_limit()).expect("loads");

    // The same words, before anything is run in either — `save` collects, so
    // both sides are counted after a collection and the figures are the live
    // set rather than whatever each heap had lying about.
    assert_eq!(reloaded.used_words(), heap.used_words());

    // And not merely that it loaded: that the *world* came back. `double` was
    // defined in the heap that was saved and nowhere else.
    assert_eq!(answer(&mut reloaded, "(double 100)"), "200");
}

/// **The gate.** An image saved in the middle of a computation resumes where
/// it left off.
///
/// This is what rule 4 bought. The budget is small enough that the evaluator
/// is certain to be partway through, so the saved heap contains a half-walked
/// expression: the frame list, the registers and the mode. If any of that
/// lived on the Rust call stack, the number below would be wrong.
#[test]
fn a_computation_survives_being_saved_mid_flight() {
    use crate::eval::{eval_sliced, EvalError};
    use crate::heap::Register;
    use crate::reader::Reader;
    use crate::slice::{run_slice, Status};

    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    answer(
        &mut heap,
        "(defun total (n acc) (if (= n 0) acc (total (- n 1) (+ acc n))))",
    );

    // Start it, and stop after a handful of steps — far short of finishing.
    heap.mutate(|mc| {
        let chars = Reader::chars_of("(total 60 0)");
        let mut reader = Reader::new(&chars);
        let form = reader.read(mc).expect("reads").expect("a form");
        crate::eval::Machine::start(mc, form, crate::value::Value::NIL);
    });
    let finished = heap.mutate(|mc| {
        let mut machine = crate::eval::Machine::resume(mc);
        matches!(run_slice(&mut machine, 40), Status::Done { .. })
    });
    assert!(!finished, "40 steps should not finish a 60-deep recursion");

    // Save it exactly there, and throw the original away.
    let bytes = save(&mut heap);
    let mut reloaded = load(&bytes, heap.soft_limit()).expect("loads");
    drop(heap);

    // Carry on in the new process. Nothing re-starts the computation.
    let mut spent = 0u64;
    let outcome = loop {
        assert!(spent < 10_000_000, "did not finish");
        spent += 2_000;
        let done = reloaded.mutate(|mc| {
            let (status, error) = {
                let mut machine = crate::eval::Machine::resume(mc);
                let status = run_slice(&mut machine, 2_000);
                (status, machine.error.take())
            };
            if let Some(e) = error {
                return Some(Err::<(), EvalError>(e));
            }
            matches!(status, Status::Done { .. }).then_some(Ok(()))
        });
        if let Some(done) = done {
            break done;
        }
    };
    outcome.expect("the resumed computation finishes");

    let result = reloaded.mutate(|mc| print(mc, mc.register(Register::Value)));
    assert_eq!(result, "1830", "60 * 61 / 2, finished in a different heap");
    let _ = eval_sliced; // the loop above is eval_sliced, written out to show it
}

/// Saving collects, so an image carries no garbage — and so the same world
/// saved twice gives the same bytes, which phase 2's fixpoint test needs.
#[test]
fn saving_collects_and_is_canonical() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    answer(
        &mut heap,
        "(defun junk (n) (if (= n 0) nil (cons n (junk (- n 1))))) (junk 200) nil",
    );

    let first = save(&mut heap);
    let second = save(&mut heap);

    // The header is magic, version, length, checksum, then the scalars — of
    // which the sequence number is the first. So bytes 32..40 are the
    // sequence and everything from 40 on is the world.
    assert_eq!(first.len(), second.len());
    assert_eq!(
        first[40..],
        second[40..],
        "the same world saved twice is the same bytes"
    );
    assert_ne!(
        first[32..40],
        second[32..40],
        "except the sequence number, which is what tells two snapshots apart"
    );
    // And the checksum must have moved with it, or it is not covering the
    // scalars as it claims to.
    assert_ne!(first[24..32], second[24..32]);
}

/// The sequence number is the one part of #24's change log that could not
/// wait, because a field cannot be added to images that already exist.
#[test]
fn each_save_advances_the_sequence() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    assert_eq!(heap.sequence(), 0);

    let first = load(&save(&mut heap), heap.soft_limit()).expect("loads");
    assert_eq!(first.sequence(), 1);

    let mut carried = first;
    let second = load(&save(&mut carried), carried.soft_limit()).expect("loads");
    assert_eq!(second.sequence(), 2, "history continues across a reload");
}

#[test]
fn a_file_that_is_not_an_image_is_refused() {
    let not = vec![0u8; HEADER_WORDS * 8];
    assert!(matches!(
        load(&not, 1024),
        Err(ImageError::NotAnImage { found: 0 })
    ));
}

#[test]
fn something_far_too_small_is_refused() {
    assert!(matches!(
        load(b"QUOKKA\0\0", 1024),
        Err(ImageError::TooShort { bytes: 8 })
    ));
}

/// A truncated download is the failure this format expects most, so it is
/// caught by the length before the checksum has to.
#[test]
fn a_truncated_image_is_refused() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    let mut bytes = save(&mut heap);
    bytes.truncate(bytes.len() - 8);
    assert!(
        matches!(load(&bytes, 1024), Err(ImageError::WrongLength { .. })),
        "{:?}",
        load(&bytes, 1024)
    );
}

/// A flipped bit anywhere in the heap or the scalars is caught.
#[test]
fn a_damaged_image_is_refused() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    let mut bytes = save(&mut heap);
    let at = bytes.len() / 2;
    bytes[at] ^= 0b1000_0000;
    assert!(matches!(
        load(&bytes, 1024),
        Err(ImageError::Corrupt { .. })
    ));
}

#[test]
fn an_image_from_another_version_is_refused() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    let mut bytes = save(&mut heap);
    bytes[8] = 99;
    assert!(matches!(
        load(&bytes, 1024),
        Err(ImageError::WrongVersion {
            expected: FORMAT_VERSION,
            ..
        })
    ));
}

/// The checksum has to actually distinguish things, which is worth one test
/// rather than an assumption.
#[test]
fn the_checksum_notices_a_difference() {
    assert_ne!(checksum(&[1, 2, 3]), checksum(&[1, 2, 4]));
    assert_ne!(checksum(&[1, 2, 3]), checksum(&[3, 2, 1]));
    assert_eq!(checksum(&[1, 2, 3]), checksum(&[1, 2, 3]));
}
