//! Differential tests: the same Lisp, four ways, all agreeing.
//!
//! Each case is an ordinary Common Lisp file in `tests/lisp/` and a baseline
//! beside it. Two different comparisons share those files:
//!
//! - **Quokka against SBCL** catches *we implemented Lisp wrong*. The SBCL leg
//!   runs in CI (`scripts/oracle.sh`) and checks the baselines independently,
//!   so a disagreement is a failing build rather than nothing at all.
//! - **Quokka native against Quokka wasm32** catches *the VM behaves
//!   differently depending on where it runs*. That is this file, run twice —
//!   once natively and once under wasmtime.
//! - **Quokka interpreted against Quokka compiled** catches *the compiler and
//!   the interpreter disagree*. Every file also runs in a compiled world — the
//!   image `boot::build_image` makes — against the same baseline (#38).
//!
//! # Why 99 unit tests do not cover this
//!
//! Every assertion in the unit tests came out of the author's head, so they
//! prove Quokka does what he *believed* Common Lisp does. The Lisp-2 bug is
//! what that is worth: the rules had been read, a chapter explaining them
//! written, and a Lisp-1 implemented anyway.
//!
//! An oracle does not share the misunderstanding. You need not know the right
//! answer — only notice the disagreement.
//!
//! # Why the files are embedded
//!
//! `include_str!` rather than reading at run time, so the test needs no file
//! system. That matters for the wasm leg, which would otherwise depend on
//! wasmtime being given the right directory, and it keeps the core VM crate's
//! tests as free of host assumptions as the crate itself.
//!
//! The cost is that a new file must be listed below. `scripts/check-differential.sh`
//! fails if one is left out, because a test file nobody runs is worse than no
//! test file.

/// Steps one file may take. Generous: the slowest case is `(down 10000)`.
const BUDGET: u64 = 5_000_000;

macro_rules! differential_cases {
    ($($name:ident => $stem:literal),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                let source = include_str!(concat!("../../../tests/lisp/", $stem, ".lisp"));
                let expected = include_str!(concat!("../../../tests/lisp/", $stem, ".expected"));
                let actual = quokka_vm::run_forms(source, BUDGET);
                compare($stem, source, expected, &actual);
            }
        )*

        /// **The fourth leg** (#38): the same files, the same baselines, run
        /// in a *compiled* world — the image `boot::build_image` makes, where
        /// the prelude, every macro and the compiler are bytecode and every
        /// form is compiled by that compiler and run on the VM. One baseline,
        /// three implementations: interpreted Quokka, compiled Quokka, SBCL.
        ///
        /// The leg that would have caught #36's `error` drift, where the two
        /// evaluators disagreed for weeks and each passed its own tests.
        mod compiled {
            $(
                #[test]
                fn $name() {
                    let source = include_str!(concat!("../../../tests/lisp/", $stem, ".lisp"));
                    let expected = include_str!(concat!("../../../tests/lisp/", $stem, ".expected"));
                    let actual = super::run_compiled(source);
                    super::compare($stem, source, expected, &actual);
                }
            )*
        }

        /// Every file listed above, for the coverage check in
        /// `scripts/check-differential.sh`.
        #[test]
        fn every_case_is_listed() {
            let listed = [$($stem),*];
            assert!(!listed.is_empty());
        }
    };
}

differential_cases! {
    arithmetic => "arithmetic",
    lists => "lists",
    control => "control",
    functions => "functions",
    scopes => "scopes",
    control_flow => "control-flow",
    macros => "macros",
    prelude => "prelude",
    keywords => "keywords",
    lambda_lists => "lambda-lists",
    destructuring => "destructuring",
    macroexpand => "macroexpand",
    vectors => "vectors",
    hash_tables => "hash-tables",
    conditions => "conditions",
    structures => "structures",
    values => "values",
}

/// **Tier 1**: files that run compiled only (#39). Tier 1 — special
/// variables, packages, the reader in Lisp and the rest — lives in a library
/// the compiled world loads and the interpreter never learns, because the
/// interpreter stays tier 0 (rule 5). So these files have two legs, not
/// three: compiled Quokka and SBCL. Their baselines come from SBCL, through
/// `scripts/oracle.lisp`, since there is no interpreted run to take them from.
macro_rules! tier1_cases {
    ($($name:ident => $stem:literal),* $(,)?) => {
        mod tier1 {
            $(
                #[test]
                fn $name() {
                    let source = include_str!(concat!("../../../tests/lisp/", $stem, ".lisp"));
                    let expected = include_str!(concat!("../../../tests/lisp/", $stem, ".expected"));
                    let actual = super::run_compiled(source);
                    super::compare($stem, source, expected, &actual);
                }
            )*
        }
    };
}

tier1_cases! {
    specials => "specials",
    packages => "packages",
    reader => "reader",
    printer => "printer",
}

/// The Lisp reader (#45) reads every source the system is built from exactly
/// as the Rust reader does, printed form for printed form.
///
/// SBCL cannot check this part. It reads backquote into its own internal
/// forms, where Quokka must expand it as `reader.rs::quasiquote` does: step F
/// compiles the compiler with the Lisp reader and expects the bytes the Rust
/// reader's reading produced, and a different expansion would change them
/// without anything being wrong. So the oracle here is the Rust reader, and
/// the input is the largest Lisp there is: the prelude, the compiler and the
/// library.
#[test]
fn the_lisp_reader_reads_the_system_as_the_rust_reader_does() {
    read_the_system_both_ways(Printer::Rust, "the two readers disagree");
}

/// The Lisp printer (#46) writes every form of the system as the Rust printer
/// does: the forms the previous test checked, now printed by `prin1` rather
/// than by `printer.rs`. Until step D switches the differential files to the
/// Lisp printer, this is the broad check on it — every symbol, string,
/// character, number and nesting the system is written in.
#[test]
fn the_lisp_printer_prints_the_system_as_the_rust_printer_does() {
    read_the_system_both_ways(Printer::Lisp, "the two printers disagree");
}

/// Read every form of the prelude, the compiler and the library with the Rust
/// reader and print it with the Rust printer; read it again with the Lisp
/// reader and print it with PRINTER; and insist the two agree.
fn read_the_system_both_ways(printer: Printer, disagree: &str) {
    use quokka_vm::boot::{COMPILER, LIBRARY, PRELUDE};
    let sources = std::iter::once(("prelude.lisp", PRELUDE))
        .chain(COMPILER.iter().copied())
        .chain(LIBRARY.iter().copied());
    for (name, source) in sources {
        let by_rust = read_with_rust(source);
        // A stream over the whole file, then one `read` for each form the Rust
        // reader found, and one more that must meet the end.
        let mut program = format!(
            "(defvar *source* (make-string-input-stream {}))\n",
            lisp_string(source)
        );
        for _ in &by_rust {
            program.push_str("(read *source*)\n");
        }
        program.push_str("(read *source* nil :end)\n");
        let by_lisp = run_compiled_printing(&program, printer);
        let by_lisp: Vec<&str> = by_lisp.lines().skip(1).collect();
        for (i, (rust, lisp)) in by_rust.iter().zip(&by_lisp).enumerate() {
            assert_eq!(lisp, rust, "{name}, form {}: {disagree}", i + 1);
        }
        assert_eq!(
            by_lisp.len(),
            by_rust.len() + 1,
            "{name}: a different number of forms"
        );
        assert_eq!(
            by_lisp[by_rust.len()],
            "=> :END",
            "{name}: the Lisp reader did not finish"
        );
    }
}

/// What the Lisp reader refuses that SBCL reads, so no baseline can hold it:
/// ratios and floats, until steps L and M. Reading `1.5` as a symbol would
/// succeed quietly and mean something else once floats exist. The tokens
/// around them that are *not* numbers by CLHS 2.3.1 must still be symbols.
#[test]
fn the_lisp_reader_refuses_ratios_and_floats() {
    let refused = ["1.5", ".5", "-1.5e3", "1e5", "2d0", "1/2", "-3/4"];
    let symbols = ["1+", "1-", "e5", "1/", "/2", "1.2.3", "+.", "a1.5"];
    let mut program = String::new();
    for text in refused.iter().chain(&symbols) {
        program.push_str(&format!("(symbolp (read-from-string \"{text}\"))\n"));
    }
    let out = run_compiled(&program);
    let lines: Vec<&str> = out.lines().collect();
    for (text, line) in refused.iter().zip(&lines) {
        assert_eq!(*line, "!! error", "{text} should be refused");
    }
    for (text, line) in symbols.iter().zip(&lines[refused.len()..]) {
        assert_eq!(*line, "=> T", "{text} should read as a symbol");
    }
}

/// Every form of SOURCE as the Rust reader reads it, printed as `run_compiled`
/// prints a result.
fn read_with_rust(source: &str) -> Vec<String> {
    let mut heap = quokka_vm::image::load(compiled_image(), 1 << 22).expect("the image loads");
    let chars = quokka_vm::reader::Reader::chars_of(source);
    let mut reader = quokka_vm::reader::Reader::new(&chars);
    heap.mutate(|mc| {
        let mut forms = Vec::new();
        while let Some(form) = reader.read(mc).expect("the Rust reader reads it") {
            forms.push(format!("=> {}", quokka_vm::printer::print(mc, form)));
        }
        forms
    })
}

/// TEXT as a Lisp string literal: `"` and `\` are the only escapes.
fn lisp_string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// The compiled world, built once and kept as image bytes; each file loads
/// its own copy, so no file can see what another defined.
fn compiled_image() -> &'static [u8] {
    static IMAGE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    IMAGE.get_or_init(|| {
        let mut heap = quokka_vm::heap::Heap::new();
        quokka_vm::boot::build_image(&mut heap).unwrap_or_else(|e| panic!("{e}"));
        quokka_vm::image::save(&mut heap)
    })
}

/// `run_forms`, in a compiled world: each form compiled by the image's own
/// compiler and run on the VM, one at a time — so a `defstruct` has run
/// before the form that uses its accessors is compiled, which is the order
/// a file means. A compile error is a condition signalled inside
/// `compile-form`, and so arrives as the form's `!! error`, exactly as a
/// deliberately malformed form errs at run time when interpreted.
///
/// Anything a form writes to `*standard-output*` comes first, as SBCL's does
/// in the oracle's output.
fn run_compiled(source: &str) -> String {
    run_compiled_printing(source, Printer::Rust)
}

/// Which printer writes a compiled form's result.
///
/// The Lisp printer is the one the differential files should use, so that
/// all of them check it against SBCL — and they will, from step D (#46).
/// Until then the files are read by the Rust reader, whose flat table gives
/// a test's `a` the `QUOKKA` symbol the build already used, and a correct
/// printer in `CL-USER` writes that as `QUOKKA::A`: 66 forms disagreed by
/// that prefix alone. At D the files are read by the Lisp reader, a test's
/// `a` is `CL-USER::A`, and `run_compiled` switches to `Printer::Lisp`.
#[derive(Clone, Copy, PartialEq)]
enum Printer {
    Rust,
    Lisp,
}

fn run_compiled_printing(source: &str, printer: Printer) -> String {
    use quokka_vm::heap::Register;
    use quokka_vm::vm::{call_global, run_sliced};
    let mut heap = quokka_vm::image::load(compiled_image(), 1 << 22).expect("the image loads");
    let mut host = quokka_vm::host::NullHost::default();
    let mut out = String::new();
    let chars = quokka_vm::reader::Reader::chars_of(source);
    let mut reader = quokka_vm::reader::Reader::new(&chars);
    loop {
        let read = heap.mutate(|mc| match reader.read(mc) {
            Err(_) => Some(false),
            Ok(None) => None,
            Ok(Some(form)) => {
                mc.set_register(Register::Value, form);
                Some(true)
            }
        });
        match read {
            None => break,
            Some(false) => {
                out.push_str("!! read error\n");
                break;
            }
            Some(true) => {}
        }
        let ran = call_global(&mut heap, "COMPILE-FORM", BUDGET, 2_000, &mut host)
            .and_then(|()| run_sliced(&mut heap, BUDGET, 2_000, &mut host));
        // The form's value is in the Value register; the Lisp printer leaves
        // its text there instead. A printer that fails is a bug, not a result.
        if ran.is_ok() && printer == Printer::Lisp {
            call_global(&mut heap, "PRIN1-TO-STRING", BUDGET, 2_000, &mut host)
                .unwrap_or_else(|e| panic!("the Lisp printer failed: {e}"));
        }
        out.push_str(&std::mem::take(&mut host.output));
        match ran {
            Ok(()) => {
                let text = heap.mutate(|mc| {
                    let value = mc.register(Register::Value);
                    match printer {
                        Printer::Lisp => mc.string_to_rust(value),
                        Printer::Rust => quokka_vm::printer::print(mc, value),
                    }
                });
                out.push_str("=> ");
                out.push_str(&text);
                out.push('\n');
            }
            Err(_) => out.push_str("!! error\n"),
        }
    }
    out
}

/// Compare line by line, and on a mismatch say which form disagreed.
///
/// A diff of two long outputs is not useful on its own; what a person needs is
/// the form, what came out, and what should have. Pairing the output lines back
/// up with the source forms costs a little and saves a lot.
fn compare(stem: &str, source: &str, expected: &str, actual: &str) {
    let forms = top_level_forms(source);
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();

    // Every disagreement, not just the first: one wrong form often explains
    // the next three, and seeing them together is how that is noticed.
    let mut wrong = Vec::new();
    for (i, (want, got)) in expected_lines.iter().zip(&actual_lines).enumerate() {
        if want != got {
            let form = forms.get(i).map(String::as_str).unwrap_or("<unknown form>");
            wrong.push(format!(
                "form {}:\n    {form}\n  expected  {want}\n  got       {got}",
                i + 1
            ));
        }
    }
    assert!(wrong.is_empty(), "{stem}.lisp:\n{}", wrong.join("\n"));
    assert_eq!(
        actual_lines.len(),
        expected_lines.len(),
        "{stem}.lisp produced {} lines, the baseline has {}",
        actual_lines.len(),
        expected_lines.len()
    );
}

/// Split a source file into its top-level forms, for error messages only.
///
/// Counts parentheses and skips what is inside strings, character literals and
/// comments. Not a reader — it does not have to be right about every input,
/// only about the test files, and a wrong answer costs a vaguer message rather
/// than a wrong result.
fn top_level_forms(source: &str) -> Vec<String> {
    let mut forms = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut after_backslash = false;

    for line in source.lines() {
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if after_backslash {
                after_backslash = false;
                current.push(ch);
                continue;
            }
            match ch {
                '\\' => {
                    after_backslash = true;
                    current.push(ch);
                }
                '"' => {
                    in_string = !in_string;
                    current.push(ch);
                }
                '#' if !in_string && chars.peek() == Some(&'\\') => {
                    current.push(ch);
                    current.push(chars.next().expect("peeked"));
                    if let Some(c) = chars.next() {
                        current.push(c);
                    }
                }
                ';' if !in_string => break, // comment to end of line
                '(' if !in_string => {
                    depth += 1;
                    current.push(ch);
                }
                ')' if !in_string => {
                    depth -= 1;
                    current.push(ch);
                    if depth == 0 {
                        forms.push(current.trim().to_string());
                        current.clear();
                    }
                }
                _ => current.push(ch),
            }
        }
        if depth == 0 && !current.trim().is_empty() {
            // An atom on its own line: `a`, `x`, `'foo`.
            forms.push(current.trim().to_string());
            current.clear();
        } else if depth > 0 {
            current.push(' ');
        }
    }
    forms
}
