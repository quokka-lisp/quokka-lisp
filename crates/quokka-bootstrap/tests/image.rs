//! Tests for [`quokka_vm::image`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items — `checksum`, in particular, which is not something
//! the rest of the system should be able to reach.

use quokka_bootstrap::boot;
use quokka_bootstrap::eval::eval_forms;
use quokka_vm::heap::Heap;
use quokka_vm::image::*;
use quokka_vm::printer::print;

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
    use quokka_bootstrap::eval::{eval_sliced, EvalError};
    use quokka_bootstrap::reader::Reader;
    use quokka_vm::heap::Register;
    use quokka_vm::slice::{run_slice, Status};

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
        quokka_bootstrap::eval::Machine::start(mc, form, quokka_vm::value::Value::NIL);
    });
    let finished = heap.mutate(|mc| {
        let mut machine = quokka_bootstrap::eval::Machine::resume(mc);
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
                let mut machine = quokka_bootstrap::eval::Machine::resume(mc);
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

/// **An image built on one target runs on another** (#84). The design has a
/// browser load a world that CI built — a 64-bit program's image, read by a
/// 32-bit one — and until this test nothing had ever tried it: the wasm32
/// legs built their own images, on wasm32, and so only ever proved that a
/// target can read what it wrote itself.
///
/// The image is not in the repository (images are build artefacts), so the
/// leg that runs this under wasmtime builds one natively first and passes
/// its path in `QUOKKA_FOREIGN_IMAGE`. Without the variable there is nothing
/// to test and the test says so — unless `QUOKKA_FOREIGN_IMAGE_REQUIRED` is
/// set, as CI and `check.sh` set it, so that the leg cannot pass on nothing.
#[test]
fn an_image_built_elsewhere_runs_here() {
    use quokka_vm::vm::{eval_source, Outcome};
    let Ok(path) = std::env::var("QUOKKA_FOREIGN_IMAGE") else {
        assert!(
            std::env::var("QUOKKA_FOREIGN_IMAGE_REQUIRED").is_err(),
            "QUOKKA_FOREIGN_IMAGE_REQUIRED is set, but no image was given"
        );
        eprintln!("skipped: set QUOKKA_FOREIGN_IMAGE to an image built on another target");
        return;
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut heap = load(&bytes, 1 << 20).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut said = Vec::new();
    let mut host = quokka_vm::host::NullHost::default();
    eval_source(
        &mut heap,
        "(defun fib (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (fib 20) \
         (expt 2 100) (format nil \"~A/~S\" 'a \"b\") (/ 1.0 3) \
         (let ((table (make-hash-table :test 'equal))) \
           (setf (gethash \"k\" table) 1) (gethash \"k\" table)) \
         (handler-case (car 5) (type-error () 'caught))",
        50_000_000,
        2_000,
        &mut host,
        |outcome| {
            said.push(match outcome {
                Outcome::Value(text) => text,
                other => format!("{other:?}"),
            });
            true
        },
    )
    .expect("the world reads its input");
    assert_eq!(
        said,
        [
            "FIB",
            "6765",
            "1267650600228229401496703205376",
            "\"A/\\\"b\\\"\"",
            "0.3333333333333333",
            "1",
            "CAUGHT"
        ]
    );
    // And the collector can copy a heap that another target laid out.
    heap.collect();
    heap.verify().expect("consistent after a collection");
}

/// Put a correct checksum back on an image this file has altered: FNV-1a
/// over everything after the checksum word. Written out here, so it is also
/// the check that an image's checksum is the *published* FNV-1a and can be
/// verified by a program that is not Quokka — which until format 7 it could
/// not, the prime having been mistyped (#98).
fn reseal(bytes: &mut [u8]) {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in &bytes[32..] {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    bytes[24..32].copy_from_slice(&hash.to_le_bytes());
}

/// Overwrite word `index` of an image, counted from its first.
fn poke(bytes: &mut [u8], index: usize, word: u64) {
    bytes[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
}

#[test]
fn the_checksum_is_the_published_fnv_1a() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    let bytes = save(&mut heap);
    let mut again = bytes.clone();
    reseal(&mut again);
    assert_eq!(bytes, again);
}

/// **No image may panic the machine** (#99). The checksum catches accident;
/// these are images with a *correct* checksum and a heap that is not one —
/// what a bug in a saving collector, or a person, could produce. Each used
/// to load and then panic at the first `mutate`, the first `intern` or the
/// first error. Each is now refused by `load`, with a reason.
///
/// The scalars are words 4 to 23 of an image, in the order `Scalars` lists
/// them; the two words after say what machine the image needs.
#[test]
fn an_image_that_is_not_a_heap_is_refused_however_well_it_is_sealed() {
    let mut heap = Heap::new();
    boot::install(&mut heap).expect("boots");
    let good = save(&mut heap);
    assert!(load(&good, 1024).is_ok());

    let altered = |index: usize, word: u64| {
        let mut bytes = good.clone();
        poke(&mut bytes, index, word);
        reseal(&mut bytes);
        load(&bytes, 1024).err()
    };
    let inconsistent = |index: usize, word: u64| {
        let error = altered(index, word);
        assert!(
            matches!(error, Some(ImageError::Inconsistent(_))),
            "word {index} = {word:#x} gave {error:?}"
        );
    };
    // The static area ends a billion words in.
    inconsistent(4 + 17, 1_000_000_000);
    // The obarray is a *cons* at word 0, where the symbol `nil` is.
    inconsistent(4 + 2, 0b001);
    // A stack pointer, a frame pointer and a program counter from nowhere.
    inconsistent(4 + 12, 1_000_000_000);
    inconsistent(4 + 13, u64::MAX - 1);
    inconsistent(4 + 11, 1 << 40);
    // "No unwind record" as a 32-bit build used to write it: an index of
    // four billion to a 64-bit loader. The one spelling now is all ones.
    inconsistent(4 + 14, 0xFFFF_FFFF);
    assert_eq!(altered(4 + 14, u64::MAX), None);
    // A register that points into the middle of an object.
    inconsistent(4 + 9, (3 << 3) | 0b011);

    // Built against a machine with one more primitive than this one has.
    let primitives = u64::from_le_bytes(good[(4 + 20) * 8..(4 + 21) * 8].try_into().unwrap());
    assert!(matches!(
        altered(4 + 20, primitives + 1),
        Some(ImageError::NeedsNewerMachine { .. })
    ));

    // And a file cut in the middle of a word says so, not "too short".
    assert!(matches!(
        load(&good[..good.len() - 3], 1024),
        Err(ImageError::NotWholeWords { .. })
    ));
}

/// The other half: an image saved in the middle of a computation still
/// loads, with all the new checks on its registers and its code.
#[test]
fn every_function_in_a_compiled_world_passes_the_verifier_on_load() {
    let mut heap = Heap::new();
    boot::build_image(&mut heap).expect("builds");
    let bytes = save(&mut heap);
    let loaded = load(&bytes, 1 << 22).expect("a built world loads");
    assert!(loaded.verify().is_ok());
}

/// A process's saved registers are resumed as they stand (#34), so an image
/// whose background process has a function that is not compiled code, or a
/// request that is not a request, is refused by the check every image
/// passes, as a live set like it would be: resuming it would read slots the
/// objects do not have (rule 10). The review of #34 found both accepted.
#[test]
fn a_process_that_cannot_be_resumed_is_refused() {
    use quokka_vm::heap::Register;
    use quokka_vm::value::Value;
    // GC torture verifies the heap after every mutation, so the damage done
    // below is refused by that same check, as a panic, before the test can
    // ask: the property holds there too, said louder.
    if quokka_vm::heap::TORTURE {
        return;
    }
    let mut heap = Heap::new();
    boot::build_image(&mut heap).expect("builds");
    let mut host = quokka_vm::host::NullHost::default();
    quokka_vm::vm::eval_source(
        &mut heap,
        "(quokka-thread:make-thread (lambda () (do () (nil) (quokka-thread:thread-yield))))",
        5_000_000,
        2_000,
        &mut host,
        |_| true,
    )
    .expect("runs");
    heap.verify().expect("a world with a live thread is sound");
    let bytes = save(&mut heap);
    for (slot, bad) in [
        (quokka_vm::process::FUNCTION, 3),
        (quokka_vm::process::REQUEST, 7),
    ] {
        let mut damaged = load(&bytes, 1 << 22).expect("the saved world loads");
        damaged.mutate(|mc| {
            assert!(mc.register(Register::Process) != Value::NIL);
            let background = quokka_vm::process::ring(mc)
                .into_iter()
                .find(|p| mc.slot(*p, quokka_vm::process::MAIN) == Value::NIL)
                .expect("the thread is in the ring");
            mc.set_slot(background, slot, Value::fixnum(bad).unwrap());
        });
        assert!(damaged.verify().is_err(), "slot {slot} accepted");
    }
}

/// The records and frames on a stack lead where the machine will follow
/// them, or the image is refused (#171). The machine walks both chains
/// trusting every link, and `Heap::verify` used to look at the top record
/// alone: a link past the end of the stack panicked the first host call, a
/// record that was its own predecessor hung it outside any step budget, and
/// a live thread with no stack ran with `nil` for one and wrote over it.
/// Each damage here is one a correctly sealed image could carry.
#[test]
fn a_stack_whose_chains_lead_nowhere_is_refused() {
    use quokka_vm::process::{FP, MAIN, MP, OPERANDS};
    use quokka_vm::value::Value;
    use quokka_vm::vm::{frame, mark};
    // As above: under GC torture the damage is refused at once, as a panic.
    if quokka_vm::heap::TORTURE {
        return;
    }
    let mut heap = Heap::new();
    boot::build_image(&mut heap).expect("builds");
    let mut host = quokka_vm::host::NullHost::default();
    // A thread with a catch, a binding and a cleanup open, so its stack
    // holds records of several kinds and frames under them.
    quokka_vm::vm::eval_source(
        &mut heap,
        "(quokka-thread:make-thread \
           (lambda () \
             (let ((*print-base* 8)) \
               (unwind-protect \
                    (catch 'k (do () (nil) (quokka-thread:thread-yield))) \
                 (print :bye)))))",
        5_000_000,
        2_000,
        &mut host,
        |_| true,
    )
    .expect("runs");
    let bytes = save(&mut heap);
    load(&bytes, 1 << 22)
        .expect("the saved world loads")
        .verify()
        .expect("and its chains are sound");

    fn fixnum<'h>(n: i64) -> Value<'h> {
        Value::fixnum(n).unwrap()
    }
    // Given the top record, the first record that resumes in a function
    // and the frame pointer: the stack slot to spoil, and with what.
    type Damage<'a> = &'a dyn Fn(usize, usize, usize) -> (usize, i64);
    let damages: [(&str, Damage<'_>); 5] = [
        ("a link past the stack", &|mp, _, _| {
            (mp + mark::PREV, 10_000_000)
        }),
        ("a record that is its own predecessor", &|mp, _, _| {
            (mp + mark::PREV, mp as i64)
        }),
        ("a kind of record there is none of", &|mp, _, _| {
            (mp + mark::KIND, 99)
        }),
        (
            "a number where a record resumes in a function",
            &|_, exit, _| (exit + mark::FUNCTION, 3),
        ),
        ("a frame whose caller is not below it", &|_, _, fp| {
            (fp - frame::SIZE + frame::FP, fp as i64)
        }),
    ];
    for (what, damage) in damages {
        let mut damaged = load(&bytes, 1 << 22).expect("the saved world loads");
        damaged.mutate(|mc| {
            let thread = quokka_vm::process::ring(mc)
                .into_iter()
                .find(|p| mc.slot(*p, MAIN) == Value::NIL)
                .expect("the thread is in the ring");
            let stack = mc.slot(thread, OPERANDS);
            let mp = mc.slot(thread, MP).as_fixnum().unwrap() as usize;
            let fp = mc.slot(thread, FP).as_fixnum().unwrap() as usize;
            // The first record down the chain that resumes in a function.
            let mut exit = mp;
            while !matches!(
                mc.slot(stack, exit + mark::KIND).as_fixnum(),
                Some(mark::BLOCK | mark::CATCH | mark::TAG | mark::PROTECT | mark::HANDLER)
            ) {
                exit = mc.slot(stack, exit + mark::PREV).as_fixnum().unwrap() as usize;
            }
            let (slot, word) = damage(mp, exit, fp);
            mc.set_slot(stack, slot, fixnum(word));
        });
        assert!(damaged.verify().is_err(), "accepted: {what}");
    }

    // And a live thread with no stack at all.
    let mut damaged = load(&bytes, 1 << 22).expect("the saved world loads");
    damaged.mutate(|mc| {
        let thread = quokka_vm::process::ring(mc)
            .into_iter()
            .find(|p| mc.slot(*p, MAIN) == Value::NIL)
            .expect("the thread is in the ring");
        for slot in [quokka_vm::process::SP, FP, quokka_vm::process::PC] {
            mc.set_slot(thread, slot, fixnum(0));
        }
        mc.set_slot(thread, MP, fixnum(-1));
        mc.set_slot(thread, OPERANDS, Value::NIL);
    });
    assert!(
        damaged.verify().is_err(),
        "accepted: a live thread with no stack"
    );
}
