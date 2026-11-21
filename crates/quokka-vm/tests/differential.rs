//! Differential tests: the same Lisp, three ways, all agreeing.
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

/// The other half of each file: does the **compiler** handle all of it?
///
/// Not yet the fourth leg — that is step E of #23, where a file compiled and
/// run must print what it printed interpreted, and it needs #33's compiled
/// prelude to run anything. This is its precondition, checked now: every
/// form compiles, and the verifier accepts every function that comes out.
///
/// **Except a form that is meant to be an error.** A file like
/// `control-flow.lisp` contains `(return-from nowhere 1)` on purpose, and its
/// baseline says `!! error`; the compiler finds that at compile time where
/// the interpreter found it at run time, and either way it is an error. So a
/// form may fail to compile *only* where its baseline line is `!! error`.
/// The first version stopped at the first failure, which left every form
/// after a deliberate error unchecked.
///
/// A file still waiting on something is listed with a fragment of the error
/// it gives. It fails the moment it starts compiling — *"now compiles"* — so
/// the list cannot quietly go stale. #37 has the table this was built from.
macro_rules! compile_cases {
    ($($name:ident => $stem:literal : $why:expr),* $(,)?) => {
        mod compiles {
            use super::OK;
            $(
                #[test]
                fn $name() {
                    let source = include_str!(concat!("../../../tests/lisp/", $stem, ".lisp"));
                    let expected = include_str!(concat!("../../../tests/lisp/", $stem, ".expected"));
                    super::compile_file($stem, source, expected, $why);
                }
            )*
        }
    };
}

/// Nothing is waiting on this file.
const OK: Option<&str> = None;

fn compile_file(stem: &str, source: &str, expected: &str, why: Option<&str>) {
    let baseline: Vec<&str> = expected.lines().collect();
    let mut heap = quokka_vm::heap::Heap::new();
    quokka_vm::boot::install(&mut heap).expect("the prelude boots");
    quokka_vm::boot::install_compiler(&mut heap).expect("the compiler loads");
    let mut waiting = false;
    let mut unexpected = Vec::new();
    let read = quokka_vm::boot::compile_each(&mut heap, source, |form, outcome| {
        if let Err(e) = outcome {
            let meant = baseline.get(form - 1).is_some_and(|l| l.starts_with("!!"));
            let known = why.is_some_and(|w| e.to_string().contains(w));
            if known {
                waiting = true;
            } else if !meant {
                unexpected.push(format!("form {form}: {e}"));
            }
        }
        true
    });
    read.unwrap_or_else(|e| panic!("{stem} does not read: {e}"));
    assert!(
        unexpected.is_empty(),
        "{stem}: forms that should compile and do not:\n  {}",
        unexpected.join("\n  ")
    );
    if let Some(why) = why {
        assert!(
            waiting,
            "{stem} now compiles — mark it `OK` (it was waiting on: {why})"
        );
    }
}

compile_cases! {
    arithmetic => "arithmetic": OK,
    lists => "lists": OK,
    control => "control": OK,
    functions => "functions": OK,
    scopes => "scopes": OK,
    control_flow => "control-flow": OK,
    macros => "macros": OK,
    prelude => "prelude": OK,
    keywords => "keywords": OK,
    lambda_lists => "lambda-lists": OK,
    destructuring => "destructuring": OK,
    macroexpand => "macroexpand": OK,
    vectors => "vectors": OK,
    hash_tables => "hash-tables": OK,
    conditions => "conditions": OK,
    // `defstruct` defines its `setf` expanders when it *runs*, and this
    // compiles without running anything. Step E compiles and runs form by
    // form, which is the order a file actually means.
    structures => "structures": Some("SETF does not know"),
    // `macrolet`: this leg runs the *interpreted* bootstrap compiler, which
    // cannot call an expander it has just compiled. The compiled compiler in
    // a built image can (#33), and step E runs these files there.
    values => "values": Some("reached a compiled closure"),
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

    for (i, (want, got)) in expected_lines.iter().zip(&actual_lines).enumerate() {
        if want != got {
            let form = forms.get(i).map(String::as_str).unwrap_or("<unknown form>");
            panic!(
                "{stem}.lisp, form {}:\n    {form}\n  expected  {want}\n  got       {got}",
                i + 1
            );
        }
    }
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
