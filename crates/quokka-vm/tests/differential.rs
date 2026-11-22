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
fn run_compiled(source: &str) -> String {
    use quokka_vm::printer::print;
    let mut heap = quokka_vm::image::load(compiled_image(), 1 << 22).expect("the image loads");
    let mut host = quokka_vm::host::NullHost::default();
    let mut out = String::new();
    let read = quokka_vm::vm::compile_and_run_forms(
        &mut heap,
        source,
        BUDGET,
        2_000,
        &mut host,
        |mc, result| {
            match result {
                Ok(value) => {
                    out.push_str("=> ");
                    out.push_str(&print(mc, value));
                    out.push('\n');
                }
                Err(_) => out.push_str("!! error\n"),
            }
            true
        },
    );
    if read.is_err() {
        out.push_str("!! read error\n");
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
