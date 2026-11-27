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

/// How many steps a slice runs before the heap may collect.
///
/// Two thousand, as a host would give — except under GC torture, which
/// collects *and verifies the whole heap* after every slice. A rebuild is
/// some two hundred million steps, so at two thousand a slice torture did a
/// hundred thousand full collections of a 150,000-word heap in each rebuild
/// test, and those tests were ten of the job's fourteen minutes in CI.
/// Fifty thousand a slice is still four thousand collections a rebuild, at
/// points just as arbitrary; what torture is for, a collector that loses or
/// misplaces something, does not need more.
const SLICE: u64 = if quokka_vm::heap::TORTURE {
    50_000
} else {
    2_000
};

macro_rules! differential_cases {
    ($($name:ident => $stem:literal),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                let source = include_str!(concat!("../../../tests/lisp/", $stem, ".lisp"));
                let expected = include_str!(concat!("../../../tests/lisp/", $stem, ".expected"));
                let actual = quokka_bootstrap::run_forms(source, BUDGET);
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
    read_line => "read-line",
    packages => "packages",
    reader => "reader",
    printer => "printer",
    eval => "eval",
    setf_expanders => "setf-expanders",
    special_operators => "special-operators",
    list_functions => "list-functions",
    sequence_functions => "sequence-functions",
    string_functions => "string-functions",
    format => "format",
    bignums => "bignums",
    floats => "floats",
    first_gaps => "first-gaps",
    symbol_cells => "symbol-cells",
    standard_io_syntax => "standard-io-syntax",
    compile => "compile",
    package_system => "package-system",
    arrays => "arrays",
    small_functions => "small-functions",
    bit_fields => "bit-fields",
    reader_syntax => "reader-syntax",
    hash_tables_completed => "hash-tables-completed",
    labels_and_limits => "labels-and-limits",
    defstruct_options => "defstruct-options",
    sequence_gaps => "sequence-gaps",
    condition_types => "condition-types",
    macro_lambda_lists => "macro-lambda-lists",
    nil_arrays => "nil-arrays",
    setf_functions => "setf-functions",
    list_gaps => "list-gaps",
    compiler_gaps => "compiler-gaps",
    remaining_gaps => "remaining-gaps",
    ratios => "ratios",
    toplevel_forms => "toplevel-forms",
    review_fixes => "review-fixes",
    types => "types",
    subtypep => "subtypep",
    classes => "classes",
    generic_functions => "generic-functions",
    print_object => "print-object",
    redefinition => "redefinition",
    method_combination => "method-combination",
    mop => "mop",
    gray_streams => "gray-streams",
    class_details => "class-details",
    condition_objects => "condition-objects",
    signalling => "signalling",
    machine_errors => "machine-errors",
    restarts => "restarts",
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
    use quokka_bootstrap::boot::{COMPILER, LIBRARY, PRELUDE};
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
        let by_lisp = run_compiled_rust_read(&program, printer);
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

/// Numbers the Lisp reader reads, and tokens around them that are not
/// numbers and must stay symbols. Ratios are read since #59 (they were
/// refused until then, since reading `1/2` as a symbol would have meant
/// something else once they existed); floats since step M (#17), in every
/// syntax CLHS 2.3.1 gives them.
#[test]
fn the_lisp_reader_reads_ratios_and_floats() {
    let ratios = ["1/2", "-3/4", "+10/3"];
    let floats = [
        "1.5", ".5", "-1.5e3", "1e5", "2d0", "1.e2", "+.5", "1.5s0", "1L-3",
    ];
    let symbols = [
        "1+", "1-", "e5", "1/", "/2", "1.2.3", "+.", "a1.5", "1.5e", "1e+",
    ];
    let mut program = String::new();
    for text in ratios.iter().chain(&floats) {
        program.push_str(&format!("(symbolp (read-from-string \"{text}\"))\n"));
    }
    for text in &symbols {
        program.push_str(&format!("(symbolp (read-from-string \"{text}\"))\n"));
    }
    let out = run_compiled(&program);
    let lines: Vec<&str> = out.lines().collect();
    for (text, line) in ratios.iter().zip(&lines) {
        assert_eq!(*line, "=> NIL", "{text} should read as a ratio");
    }
    let rest = &lines[ratios.len()..];
    for (text, line) in floats.iter().zip(rest) {
        assert_eq!(*line, "=> NIL", "{text} should read as a float");
    }
    for (text, line) in symbols.iter().zip(&rest[floats.len()..]) {
        assert_eq!(*line, "=> T", "{text} should read as a symbol");
    }
}

/// Reading and printing floats, against the bits (#17, decisions 5 and 6).
///
/// For three hundred doubles spread over every exponent, written in the
/// syntaxes CLHS 2.3.1 allows, the Lisp reader must produce exactly the
/// double Rust's own `{:e}` stands for — checked through
/// `integer-decode-float` against the bits — and the Lisp printer's text
/// must read back as the same double. SBCL cannot be the oracle here: the
/// values are random, so there is no baseline to keep.
#[test]
fn floats_read_exactly_and_print_back() {
    // xorshift64: a deterministic stream of bit patterns, no dependency.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut program = String::new();
    let mut expected = Vec::new();
    while expected.len() < 300 {
        let x = f64::from_bits(next());
        if !x.is_finite() {
            continue;
        }
        let bits = x.to_bits();
        let field = ((bits >> 52) & 0x7ff) as i64;
        let fraction = (bits & ((1 << 52) - 1)) as i64;
        let (m, e) = match (field, fraction) {
            (0, 0) => (0, 0),
            (0, f) => (f, -1074),
            (field, f) => (f | (1 << 52), field - 1075),
        };
        let sign = if bits >> 63 == 0 { 1 } else { -1 };
        // `1.2345e-7` → one of `1.2345e-7`, `1.2345d-7`, `1.2345D-7`.
        let text = format!("{x:e}");
        let marker = ["e", "d", "D"][expected.len() % 3];
        let text = text.replacen('e', marker, 1);
        program.push_str(&format!(
            "(let ((x {text})) (list (multiple-value-list (integer-decode-float x)) \
             (eql x (read-from-string (prin1-to-string x)))))\n"
        ));
        expected.push(format!("=> (({m} {e} {sign}) T)"));
    }
    let out = run_compiled(&program);
    for (line, (want, form)) in out.lines().zip(expected.iter().zip(program.lines())) {
        assert_eq!(line, want, "{form}");
    }
    assert_eq!(out.lines().count(), expected.len());
}

/// Every form of SOURCE as the Rust reader reads it, printed as `run_compiled`
/// prints a result.
fn read_with_rust(source: &str) -> Vec<String> {
    let mut heap = quokka_vm::image::load(compiled_image(), 1 << 22).expect("the image loads");
    let chars = quokka_bootstrap::reader::Reader::chars_of(source);
    let mut reader = quokka_bootstrap::reader::Reader::new(&chars);
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
        quokka_bootstrap::boot::build_image(&mut heap).unwrap_or_else(|e| panic!("{e}"));
        quokka_vm::image::save(&mut heap)
    })
}

/// The system's source as a rebuilding image asks for it (#48): by the names
/// `rebuild.lisp` uses, answered from the copies the binary embeds for the
/// cold build, which is what a host without files does.
fn embedded_sources() -> Vec<(String, &'static str)> {
    use quokka_bootstrap::boot::{COMPILER, LIBRARY, PRELUDE};
    let mut files = vec![("lisp/prelude.lisp".to_string(), PRELUDE)];
    for (name, text) in COMPILER {
        files.push((format!("lisp/compiler/{name}"), *text));
    }
    for (name, text) in LIBRARY {
        files.push((format!("lisp/library/{name}"), *text));
    }
    files
}

/// Have the world in IMAGE rebuild itself, and save what it becomes.
fn rebuild(image: &[u8]) -> Vec<u8> {
    use quokka_vm::vm::{eval_source, Outcome};
    let mut heap = quokka_vm::image::load(image, 1 << 22).expect("the image loads");
    let output = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let files: Vec<(&'static str, &'static str)> = embedded_sources()
        .into_iter()
        .map(|(name, text)| (&*Box::leak(name.into_boxed_str()), text))
        .collect();
    let mut host = SharedHost(output, files);
    let mut said = Vec::new();
    eval_source(
        &mut heap,
        "(quokka::%rebuild-world)",
        // As `quokka rebuild` gives it (`crates/quokka/src/main.rs`, #75).
        BUDGET * 100,
        SLICE,
        &mut host,
        |outcome| {
            said.push(outcome);
            true
        },
    )
    .expect("the rebuild runs");
    assert_eq!(
        said,
        [Outcome::Value("(:FORMS 2426 :REPLACED 1756)".into())],
        "the rebuild did not finish as expected"
    );
    quokka_vm::image::save(&mut heap)
}

/// A warm rebuild keeps every class (#124): it reloads `clos.lisp` while
/// classes and instances are alive, so the bootstrap refills the classes it
/// finds, a class whose slots did not change keeps its wrapper, and an
/// instance made before the rebuild is still an instance of its class. It
/// also reloads `generic-functions.lisp`: a program's method combinations,
/// and generic functions using them or the built-in ones, still work.
#[test]
fn a_rebuild_keeps_classes_and_their_instances() {
    use quokka_vm::vm::{eval_source, Outcome};
    let mut heap = quokka_vm::image::load(compiled_image(), 1 << 22).expect("the image loads");
    let output = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let files: Vec<(&'static str, &'static str)> = embedded_sources()
        .into_iter()
        .map(|(name, text)| (&*Box::leak(name.into_boxed_str()), text))
        .collect();
    let mut host = SharedHost(output, files);
    let mut said = Vec::new();
    eval_source(
        &mut heap,
        "(defclass kept () ((a :initarg :a :reader kept-a))) \
         (defvar *i* (make-instance 'kept :a 1)) \
         (defgeneric area (s)) \
         (defmethod area ((s kept)) (* 10 (kept-a s))) \
         (defmethod print-object ((k kept) s) (write-string \"KEPT\" s)) \
         (defvar *mi* #'make-instance) \
         (defvar *w* (quokka::%class-wrapper (find-class 'standard-object))) \
         (defvar *sc* (find-class 'standard-class)) \
         (defvar *ic* (find-class 'integer)) \
         (defclass moved () ((a :initform 1))) \
         (defvar *m* (make-instance 'moved)) \
         (defclass moved () ((a :initform 1) (b :initform 2))) \
         (define-method-combination tally :operator + :identity-with-one-argument t) \
         (defgeneric tallied (s) (:method-combination tally)) \
         (defmethod tallied tally ((s kept)) 3) \
         (defmethod tallied tally ((s standard-object)) 4) \
         (define-method-combination collect () ((ms *)) \
           `(list ,@(mapcar (lambda (m) `(call-method ,m)) ms))) \
         (defgeneric collected (s) (:method-combination collect)) \
         (defmethod collected :a ((s kept)) :k) \
         (defmethod collected :b ((s standard-object)) :o) \
         (defgeneric summed (s) (:method-combination +)) \
         (defmethod summed + ((s kept)) 5) \
         (list (tallied *i*) (collected *i*) (summed *i*)) \
         (quokka::%rebuild-world) \
         (list (eq *w* (quokka::%class-wrapper (find-class 'standard-object))) \
               (eq *sc* (find-class 'standard-class)) \
               (eq *ic* (find-class 'integer)) \
               (eq (class-of (find-class 'standard-class)) (find-class 'standard-class)) \
               (kept-a *i*) (typep *i* 'kept) (slot-value (make-instance 'kept :a 2) 'a) \
               (eq *mi* #'make-instance) (area *i*) (prin1-to-string *i*) \
               (list (slot-value *m* 'a) (slot-value *m* 'b)) \
               (tallied *i*) (collected *i*) (summed *i*))",
        BUDGET * 100,
        SLICE,
        &mut host,
        |outcome| {
            said.push(outcome);
            true
        },
    )
    .expect("the forms run");
    assert_eq!(
        said.last(),
        Some(&Outcome::Value(
            "(T T T T 1 T 2 T 10 \"KEPT\" (1 2) 7 (:K :O) 5)".into()
        )),
        "{said:?}"
    );
}

/// Definitions keep their source (#55, phase 3 step C): a defining form
/// evaluated at the top level is recorded with its text, the comments
/// directly above it and at the end of its line, where it came from, and
/// the definition it replaced. Quokka's own, so not against SBCL: SBCL's
/// `function-lambda-expression` answers nil for a compiled function.
#[test]
fn definitions_keep_their_source() {
    let out = run_compiled(
        "(defun sq (x) (* x x)) \
         (quokka::definition-text (quokka::find-definition 'sq)) \
         (function-lambda-expression #'sq) \
         (quokka::%function-name #'sq) \
         (defun sq (x) (* x x x)) \
         (quokka::definition-text (first (quokka::definition-versions (quokka::find-definition 'sq)))) \
         (length (quokka::definition-versions (quokka::find-definition 'sq))) \
         (load (make-string-input-stream \";;; The file's own.

;; Doubles.
(defun twice (x) (* 2 x))
(defvar *dc* 0)   ; a counter
(defun after () 1)
\")) \
         (quokka::definition-text (quokka::find-definition 'twice)) \
         (quokka::definition-text (quokka::find-definition '*dc* :variable)) \
         (quokka::definition-text (quokka::find-definition 'after)) \
         (defmethod dm ((s string)) s) \
         (quokka::definition-kind (quokka::find-definition '(dm nil (string)) :method)) \
         (eval '(defmacro dmm (x) x)) \
         (list (quokka::definition-text (quokka::find-definition 'dmm :macro)) \
               (quokka::definition-kind (quokka::find-definition 'dmm :macro))) \
         (progn (defun in-progn-a () 1) (defun in-progn-b () 2)) \
         (mapcar (lambda (n) (not (null (quokka::find-definition n)))) '(in-progn-a in-progn-b)) \
         (function-lambda-expression #'car) \
         (quokka::find-definition 'mapcar) \
         (defun early (x) \"Doc.\" (declare (ignorable x)) (if x (return-from early :yes)) :no) \
         (funcall (compile nil (function-lambda-expression #'early)) t) \
         (load (make-string-input-stream (concatenate 'string \";;; head\" (string #\\Return) \"
\" (string #\\Return) \"
;; own\" (string #\\Return) \"
(defun crlf-f () 1)\" (string #\\Return) \"
\"))) \
         (quokka::definition-text (quokka::find-definition 'crlf-f)) \
         (defpackage #:dpk (:use)) \
         (defpackage \"DPK\" (:use)) \
         (length (quokka::definition-versions (quokka::find-definition \"DPK\" :package))) \
         (load (make-string-input-stream \"(defvar *bounded* 1) ; past the end\" 0 20)) \
         (quokka::definition-text (quokka::find-definition '*bounded* :variable)) \
         (third (multiple-value-list (function-lambda-expression (lambda (y) y))))",
    );
    assert_eq!(
        out,
        "=> SQ\n=> \"(defun sq (x) (* x x))\"\n=> (LAMBDA (X) (BLOCK SQ (* X X)))\n=> SQ\n=> SQ\n\
         => \"(defun sq (x) (* x x))\"\n=> 1\n=> T\n\
         => \";; Doubles.\n(defun twice (x) (* 2 x))\"\n\
         => \"(defvar *dc* 0)   ; a counter\"\n=> \"(defun after () 1)\"\n\
         => #<STANDARD-METHOD DM (STRING)>\n=> :METHOD\n=> DMM\n=> (NIL :MACRO)\n\
         => IN-PROGN-B\n=> (T T)\n=> NIL\n=> NIL\n=> EARLY\n=> :YES\n=> T\n\
         => \";; own\r\n(defun crlf-f () 1)\"\n=> #<PACKAGE \"DPK\">\n=> #<PACKAGE \"DPK\">\n=> 1\n\
         => T\n=> \"(defvar *bounded* 1)\"\n=> NIL\n"
    );
}

/// Cross-reference and recompilation (#55, sub-steps 2 and 3): a
/// definition knows the functions it calls and the macros it expanded;
/// redefining a macro marks what expanded it stale, and so does defining a
/// macro under a name a definition called as a function; recompiling a
/// stale definition evaluates its kept form again, keeping its text.
#[test]
fn definitions_know_their_references() {
    let out = run_compiled(
        "(defmacro xm (x) `(+ ,x 1)) \
         (defun xf (y) (xm y)) \
         (defun xg (y) (xf y)) \
         (mapcar #'quokka::definition-name (quokka::who-calls 'xf)) \
         (mapcar #'quokka::definition-name (quokka::who-macroexpands 'xm)) \
         (xf 1) \
         (defmacro xm (x) `(+ ,x 100)) \
         (list (xf 1) (mapcar #'quokka::definition-name (quokka::stale-definitions))) \
         (mapcar #'quokka::definition-name (quokka::recompile-stale-definitions)) \
         (list (xf 1) (quokka::stale-definitions)) \
         (quokka::definition-text (quokka::find-definition 'xf)) \
         (length (quokka::definition-versions (quokka::find-definition 'xf))) \
         (defun xh (y) (later-m y)) \
         (defmacro later-m (x) (list 'quote x)) \
         (mapcar #'quokka::definition-name (quokka::stale-definitions)) \
         (progn (quokka::recompile-stale-definitions) (xh 5))",
    );
    assert_eq!(
        out,
        "=> XM\n=> XF\n=> XG\n=> (XG)\n=> (XF)\n=> 2\n=> XM\n=> (2 (XF))\n=> (XF)\n\
         => (101 NIL)\n=> \"(defun xf (y) (xm y))\"\n=> 1\n=> XH\n=> LATER-M\n=> (XH)\n=> Y\n"
    );
}

/// `*debugger-hook*` is called when an error reaches the debugger, with the
/// condition and itself, and bound to nil while it runs (#125). SBCL cannot
/// be the oracle here: under `--script` its own debugger hook runs first and
/// ends the process. An unhandled error at the top level reaches the hook,
/// since the host loop offers an `abort` restart instead of catching errors.
#[test]
fn the_debugger_hook_sees_an_unhandled_error() {
    let out = run_compiled(
        "(let ((*debugger-hook* (lambda (c hook) hook (throw :dbg (list :hooked (type-of c)))))) \
           (catch :dbg (invoke-debugger (make-condition 'simple-error :format-control \"x\")))) \
         (let ((*debugger-hook* (lambda (c hook) c hook (throw :dbg (list :reached *debugger-hook*))))) \
           (catch :dbg (error \"unhandled\"))) \
         (let ((*debugger-hook* (lambda (c hook) c hook :declined))) (error \"still unhandled\")) \
         (+ 1 2)",
    );
    assert_eq!(
        out,
        "=> (:HOOKED SIMPLE-ERROR)\n=> (:REACHED NIL)\n!! error\n=> 3\n"
    );
}

/// Every frame keeps its call (#130, sub-step 1): `%frames` lists, innermost
/// first, each frame's place, its token, the function called, its
/// environment and the arguments as they were passed — before a `setq` of a parameter, and before `&key` parsing.
/// A tail call reuses its caller's frame, so the caller is gone from the
/// list; a local function is named `(flet f)` or `(labels f)`, as SBCL
/// names it, so that it is never taken for the global function `f`.
#[test]
fn frames_keep_their_calls() {
    let out = run_compiled(
        "(defun fb (z w) (setq z 0) (let ((fs (quokka::%frames))) \
           (list z (cddddr (first fs)) (cddddr (second fs)) (eq (caddr (first fs)) #'fb) \
                 (eq (caddr (second fs)) #'fa) (> (car (first fs)) (car (second fs)))))) \
         (defun fa (x y) (car (list (fb (+ x y) 'q)))) \
         (fa 1 2) \
         (defun tb (y) (car (list (mapcar #'cddddr (subseq (quokka::%frames) 0 2))))) \
         (defun ta (x) (tb (* x 10))) \
         (defun tc () (car (list (ta 1)))) \
         (tc) \
         (defun kf (a &key b) a b (cddddr (first (quokka::%frames)))) \
         (kf 1 :b 2) \
         (defun rf (&rest r) (setf (car r) :changed) (cddddr (first (quokka::%frames)))) \
         (rf 1 2 3) \
         (flet ((lf (x) x (quokka::%function-name (caddr (first (quokka::%frames)))))) (lf 1)) \
         (labels ((lg (x) x (quokka::%function-name (caddr (first (quokka::%frames)))))) (lg 1)) \
         (quokka::%function-name #'fa) \
         (flet ((two (a b) (list a b))) (handler-case (funcall #'two 1) (error () :refused)))",
    );
    assert_eq!(
        out,
        "=> FB\n=> FA\n=> (0 (3 Q) (1 2) T T T)\n=> TB\n=> TA\n=> TC\n=> ((10) NIL)\n\
         => KF\n=> (1 :B 2)\n=> RF\n=> (1 2 3)\n=> (FLET LF)\n=> (LABELS LG)\n=> FA\n=> :REFUSED\n"
    );
}

/// Lisp reads frames (#130, sub-step 2): `QUOKKA-DEBUG`'s `frames`,
/// `frame-name`, `frame-call`, and SBCL's `list-backtrace` and
/// `print-backtrace`, each starting from its caller's frame.
#[test]
fn lisp_reads_frames() {
    let out = run_compiled(
        "(defun inner2 (a) a (car (list (quokka-debug:list-backtrace :count 2)))) \
         (defun outer2 (b) (car (list (inner2 (* b 2))))) \
         (outer2 4) \
         (defun pb () (car (list (with-output-to-string (s) \
                                   (quokka-debug:print-backtrace :count 2 :stream s))))) \
         (defun pa (x) x (car (list (pb)))) \
         (pa '(1 2)) \
         (flet ((lf (x) x (car (list (quokka-debug:frame-name (first (quokka-debug:frames))))))) \
           (car (list (lf 1)))) \
         (defun (setf thing) (v x) v x \
           (car (list (quokka-debug:frame-call (first (quokka-debug:frames)))))) \
         (car (list (setf (thing 3) 4))) \
         (defun pf (x) x (car (list (prin1-to-string (first (quokka-debug:frames)))))) \
         (pf 9) \
         (length (quokka-debug:frames 1))",
    );
    assert_eq!(
        out,
        "=> INNER2\n=> OUTER2\n=> ((INNER2 8) (OUTER2 4))\n=> PB\n=> PA\n\
         => \"0: (PB)\n1: (PA (1 2))\n\"\n=> (FLET LF)\n=> (SETF THING)\n\
         => ((SETF THING) 4 3)\n=> PF\n=> \"#<QUOKKA-DEBUG:FRAME (PF 9)>\"\n=> 1\n"
    );
}

/// Retrying a frame (#130, sub-step 3), the gate's own scenario first: a
/// function errs, the handler redefines it and restarts its frame, and the
/// fixed function's value comes back to the caller. A frame is retried
/// with the arguments as first passed; a local function by its own
/// function, never the global one of the same name; returning from a frame
/// runs its cleanups and undoes its special bindings; a frame that has
/// returned cannot be unwound to, even when a later call of the same
/// function has taken its place on the stack.
#[test]
fn a_frame_can_be_retried() {
    let out = run_compiled(
        "(defun find-frame (name) \
           (find name (quokka-debug:frames) :key #'quokka-debug:frame-name :test #'equal)) \
         (defun broken (x) (error \"broken ~a\" x)) \
         (defun caller (y) (list :got (broken (* y 2)))) \
         (handler-bind ((error (lambda (c) c (eval '(defun broken (x) (+ x 1))) \
                                 (quokka-debug:restart-frame (find-frame 'broken))))) \
           (caller 5)) \
         (defvar *tries* 0) \
         (defun again (x) (setq x (+ x 100)) (incf *tries*) (if (< *tries* 2) (error \"first\") x)) \
         (handler-bind ((error (lambda (c) c (quokka-debug:restart-frame (find-frame 'again))))) \
           (list (again 1) *tries*)) \
         (defun lfn (x) x :global) \
         (let ((k 0)) \
           (flet ((lfn (x) (incf k) (if (= k 1) (error \"once\") (list :local x k)))) \
             (handler-bind ((error (lambda (c) c (quokka-debug:restart-frame (find-frame '(flet lfn)))))) \
               (car (list (lfn 7)))))) \
         (defun r1 (x) x (error \"no\")) \
         (defun r3 () (multiple-value-list (r1 1))) \
         (handler-bind ((error (lambda (c) c (quokka-debug:return-from-frame (find-frame 'r1) 1 2)))) \
           (r3)) \
         (defvar *depth* :outer) \
         (defvar *log* nil) \
         (defun deep (x) x (let ((*depth* :inner)) \
           (unwind-protect (error \"deep\") (push (list :cleanup *depth*) *log*)))) \
         (defun shallow () (list (deep 1) *depth*)) \
         (handler-bind ((error (lambda (c) c (quokka-debug:return-from-frame (find-frame 'deep) :ok)))) \
           (list (shallow) *log*)) \
         (defun keep-frame () (car (list (first (quokka-debug:frames))))) \
         (handler-case (quokka-debug:return-from-frame (keep-frame) 1) (control-error () :gone)) \
         (handler-case (quokka-debug:unwind-to-frame-and-call 5 (lambda () 1)) (type-error () :typed)) \
         (defvar *saved* nil) \
         (defun loopy (k) (if (= k 0) (setq *saved* (first (quokka-debug:frames))) \
                            (if (= k 1) (quokka-debug:return-from-frame *saved* :hijack))) k) \
         (defun twice () (list (progn (loopy 0) (loopy 1)))) \
         (handler-case (twice) (control-error () :not-the-same-call)) \
         (handler-case (quokka-debug:frames -1) (type-error () :bad-count))",
    );
    assert_eq!(
        out,
        "=> FIND-FRAME\n=> BROKEN\n=> CALLER\n=> (:GOT 11)\n=> *TRIES*\n=> AGAIN\n=> (101 2)\n\
         => LFN\n=> (:LOCAL 7 2)\n=> R1\n=> R3\n=> (1 2)\n=> *DEPTH*\n=> *LOG*\n=> DEEP\n\
         => SHALLOW\n=> ((:OK :OUTER) ((:CLEANUP :INNER)))\n=> KEEP-FRAME\n=> :GONE\n=> :TYPED\n\
         => *SAVED*\n=> LOOPY\n=> TWICE\n=> :NOT-THE-SAME-CALL\n=> :BAD-COUNT\n"
    );
}

/// The outermost frame is the host loop's, `%host-step`, whose value the
/// host reads as a step's outcome. Returning something else from it with
/// the debugger is reported, and the world carries on (#130, rule 10).
#[test]
fn returning_from_the_host_frame_does_not_panic() {
    let out = run_compiled(
        "(quokka-debug:return-from-frame (car (last (quokka-debug:frames))) :weird) \
         (quokka-debug:return-from-frame (car (last (quokka-debug:frames))) 1 2 3) \
         (+ 1 2)",
    );
    assert_eq!(out, "!! error\n!! error\n=> 3\n");
}

/// The console can be read (#131, sub-step 1): `*standard-input*` refills
/// itself from the host a line at a time, `read` crosses lines, the
/// console output streams read as the console, and a host with nobody at
/// its console gives end of file.
#[test]
fn the_console_can_be_read() {
    let out = run_compiled_typing(
        "(list (read-line) (read-line)) \
         (read) \
         (read *query-io*) \
         (multiple-value-list (read-line nil nil :eof)) \
         (read-char t nil :none) \
         (input-stream-p *debug-io*)",
        &["first line", "second", "(a", " b)", "42", "last"],
    );
    assert_eq!(
        out,
        "first line\nsecond\n=> (\"first line\" \"second\")\n(a\n b)\n=> (A B)\n\
         42\n=> 42\nlast\n=> (\"last\" NIL)\n=> :NONE\n=> T\n"
    );
}

/// The gate of phase 3, as a person at a terminal does it (#131, sub-step
/// 2): an error opens the debugger, which shows the restarts and the frame
/// that signalled; the person looks at the stack, redefines the function
/// and retries its frame, and the form that failed returns the fixed
/// function's answer.
#[test]
fn the_debugger_fixes_and_retries() {
    let out = run_compiled_typing(
        "(quokka-debug:enable-debugger) \
         (defun broken (x) (error \"broken ~a\" x)) \
         (defun caller (y) (list :got (broken (* y 2)))) \
         (caller 5)",
        &["backtrace", "(defun broken (x) (+ x 1))", "restart-frame"],
    );
    assert_eq!(
        out,
        "=> NIL\n=> BROKEN\n=> CALLER\n\
         \ndebugger invoked on a SIMPLE-ERROR: broken 10\n\n\
         Type HELP for debugger help.\n\n\
         restarts (invokable by number or by name):\n  \
         0: [ABORT] Return to the top level.\n\n\
         0: (BROKEN 10)\n\
         0] backtrace\n0: (BROKEN 10)\n1: (CALLER 5)\n\
         0] (defun broken (x) (+ x 1))\nBROKEN\n\
         0] restart-frame\n=> (:GOT 11)\n"
    );
}

/// The debugger's other ways out (#131, sub-step 2): an error inside it
/// opens a nested level whose `abort` returns to the one before; a restart
/// is invoked by its number or its name, interactively; `return` gives a
/// frame's value; the end of the console's input aborts, and the error is
/// reported as it was before there was a debugger. Without
/// `enable-debugger`, nothing changes at all.
#[test]
fn the_debugger_has_levels_and_restarts() {
    let out = run_compiled_typing(
        "(car (list (handler-case (car 'oops) (error () :not-enabled)))) \
         (quokka-debug:enable-debugger) \
         (defun half (n) (if (evenp n) (/ n 2) (error \"odd: ~a\" n))) \
         (list (half 7)) \
         (list (half 9)) \
         (let ((v 1)) (check-type v string) v) \
         (list (half 11))",
        &[
            "(car 5)",
            "abort",
            "up",
            "return 3",
            "frame 0",
            "0",
            "store-value",
            "\"ok\"",
        ],
    );
    let banner = |what: &str, restarts: &str| {
        format!(
            "\ndebugger invoked on {what}\n\nType HELP for debugger help.\n\n\
             restarts (invokable by number or by name):\n{restarts}"
        )
    };
    let top = "  0: [ABORT] Return to the top level.\n";
    let expected = [
        "=> :NOT-ENABLED\n=> NIL\n=> HALF\n".to_string(),
        banner("a SIMPLE-ERROR: odd: 7", top),
        "\n0: (HALF 7)\n0] (car 5)\n".to_string(),
        banner(
            "a TYPE-ERROR: The value 5 is not of type LIST",
            "  0: [ABORT] Return to debug level 0.\n  1: [ABORT] Return to the top level.\n",
        ),
        "1] abort\n0] up\nThis is the bottom frame.\n0] return 3\n=> (3)\n".to_string(),
        banner("a SIMPLE-ERROR: odd: 9", top),
        "\n0: (HALF 9)\n0] frame 0\n0: (HALF 9)\n0] 0\n!! error\n".to_string(),
        banner(
            "a SIMPLE-TYPE-ERROR: The value of V is 1, which is not of type STRING.",
            "  0: [STORE-VALUE] Supply a new value for V.\n  1: [ABORT      ] Return to the top level.\n",
        ),
        "0] store-value\nEnter a form for a new value of V: \"ok\"\n=> \"ok\"\n\n".to_string(),
        banner("a SIMPLE-ERROR: odd: 11", top),
        "\n0: (HALF 11)\n!! error\n".to_string(),
    ]
    .concat();
    assert_eq!(out, expected);
}

/// What a debugger shows of a frame besides its call (#131, sub-step 3): a
/// method's frame is named as SBCL names it; `list-locals` shows the
/// frame's own variables, not those of the code around a closure nor names
/// the compiler made; `source` shows a definition's text as it was written.
#[test]
fn the_debugger_shows_methods_locals_and_source() {
    let out = run_compiled_typing(
        "(quokka-debug:enable-debugger) \
         (defgeneric area (s)) \
         (defmethod area ((s integer)) (let ((twice (* 2 s))) (error \"no area for ~a\" twice))) \
         (defun measure (thing &optional (scale 3)) (list (area thing) scale)) \
         (let ((outer 1)) (defun wrap (x) (list outer (measure x)))) \
         (wrap 4)",
        &[
            "bt",
            "list-locals",
            "up",
            "list-locals",
            "source",
            "up",
            "list-locals",
            "abort",
        ],
    );
    let tail = out
        .split_once("0: ((:METHOD AREA (INTEGER)) 4)\n")
        .map_or(out.as_str(), |x| x.1);
    assert_eq!(
        tail,
        "0] bt\n0: ((:METHOD AREA (INTEGER)) 4)\n1: (MEASURE 4)\n2: (WRAP 4)\n\
         0] list-locals\n  TWICE = 8\n  S = 4\n\
         0] up\n1: (MEASURE 4)\n\
         0] list-locals\n  SCALE = 3\n  THING = 4\n\
         0] source\n(defun measure (thing &optional (scale 3)) (list (area thing) scale))\n\
         0] up\n2: (WRAP 4)\n0] list-locals\n  X = 4\n\
         0] abort\n!! error\n",
        "{out}"
    );
}

/// What the review of #131 found: a method with `&optional` and `&key` is
/// named too (its declaration was moved by the lambda-list rewrite); the
/// newline after a restart's number is not left for the next read of the
/// console; a `program-error` in the form `return` evaluates opens a
/// debugger and is not reported as the command's wrong arguments, which are
/// counted; a `*debug-io*` that cannot be read does not loop; only the
/// two-way standard streams read the console.
#[test]
fn the_debugger_review_findings() {
    let out = run_compiled_typing(
        "(quokka-debug:enable-debugger) \
         (defgeneric ar (s &optional k &key)) \
         (defmethod ar ((s integer) &optional (k 2) &key (z 3)) k z (error \"no\")) \
         (list (ar 4)) \
         (list (read-line)) \
         (car (list (handler-case (let ((*debug-io* (make-string-output-stream))) (car 5)) \
                      (error () :reported)))) \
         (mapcar #'input-stream-p (list *standard-output* *error-output* *debug-io* *terminal-io*)) \
         (list (ar 5))",
        &[
            "bt", "0", "typed",
            "return (car 1 2)", "frame 1 2", "abort", "return 7",
        ],
    );
    let after = |marker: &str| out.split_once(marker).map_or("", |x| x.1).to_string();
    assert!(
        after("0] bt\n").starts_with("0: ((:METHOD AR (INTEGER)) 4)\n0] 0\n"),
        "{out}"
    );
    assert!(out.contains("!! error\ntyped\n=> (\"typed\")\n"), "{out}");
    assert!(out.contains("=> :REPORTED\n=> (NIL NIL T T)\n"), "{out}");
    let nested = after("0] return (car 1 2)\n");
    assert!(
        nested.contains("debugger invoked on a PROGRAM-ERROR"),
        "{out}"
    );
    assert!(
        nested.ends_with(
            "1] frame 1 2\nframe takes 0 to 1 arguments.\n1] abort\n0] return 7\n=> (7)\n"
        ),
        "{out}"
    );
}

/// Processes take turns (#34, step E, sub-step 1), through the machine's
/// primitives: round robin in ring order, a new process after the running
/// one; each keeps its own special bindings across switches; one told to end
/// runs its cleanups; one that never yields is preempted at the end of its
/// slice; one the machine stops with an error ends as failed and the world
/// goes on; one waiting on the console is parked while the others run.
#[test]
fn processes_take_turns() {
    let out = run_compiled_typing(
        "(defvar *log* nil) \
         (defun worker (name n) (dotimes (i n) (push (list name i) *log*) (quokka::%process-yield)) name) \
         (defun ended-p (p) (member (quokka::%process-state p) '(:finished :terminated :failed))) \
         (defun wait-for (p) (do () ((ended-p p)) (quokka::%process-yield)) (quokka::%process-state p)) \
         (let* ((a (quokka::%make-process (lambda () (worker :a 3)))) \
                (b (quokka::%make-process (lambda () (worker :b 3)))) \
                (before (list (quokka::%process-state a) \
                              (quokka::%process-main-p (quokka::%current-process)) \
                              (length (quokka::%all-processes))))) \
           (list before (wait-for a) (wait-for b) (reverse *log*) (quokka::%process-result a) \
                 (length (quokka::%all-processes)))) \
         (defvar *x* :global) \
         (let* ((seen nil) \
                (c (quokka::%make-process \
                     (lambda () (let ((*x* :c)) (quokka::%process-yield) (push (list :c *x*) seen)))))) \
           (let ((*x* :main)) (quokka::%process-yield) (wait-for c) (push (list :main *x*) seen)) \
           (list *x* (reverse seen))) \
         (let* ((cleaned nil) \
                (d (quokka::%make-process \
                     (lambda () (unwind-protect (do () (nil) (quokka::%process-yield)) (setq cleaned t)))))) \
           (quokka::%process-yield) \
           (quokka::%process-terminate d) \
           (list (wait-for d) cleaned)) \
         (let* ((count 0) \
                (spin (quokka::%make-process (lambda () (do () (nil) (setq count (+ count 1))))))) \
           (dotimes (i 3000) nil) \
           (quokka::%process-terminate spin) \
           (list (> count 0) (wait-for spin))) \
         (let ((f (quokka::%make-process (lambda () (car 5))))) \
           (list (wait-for f) (stringp (quokka::%process-result f)))) \
         (let ((g (quokka::%make-process (lambda () (read-line))))) \
           (list (wait-for g) (quokka::%process-result g))) \
         (list (quokka::%process-terminate (quokka::%current-process)) (length (quokka::%all-processes)))",
        &["typed"],
    );
    assert_eq!(
        out,
        "=> *LOG*\n=> WORKER\n=> ENDED-P\n=> WAIT-FOR\n\
         => ((:RUNNABLE T 3) :FINISHED :FINISHED ((:B 0) (:A 0) (:B 1) (:A 1) (:B 2) (:A 2)) (:A) 1)\n\
         => *X*\n=> (:GLOBAL ((:C :C) (:MAIN :MAIN)))\n\
         => (:TERMINATED T)\n\
         => (T :TERMINATED)\n\
         => (:FAILED T)\n\
         typed\n=> (:FINISHED (\"typed\" NIL))\n\
         => (NIL 1)\n"
    );
}

/// Threads, as a program uses them (#34, sub-step 2): SBCL's `sb-thread`
/// names. A thread's values come back from `join-thread`; one an error ended
/// says so on the console and gives the default or a `join-thread-error`; a
/// mutex keeps a read, a yield and a write together across three threads; a
/// wait queue hands an item from one thread to another; a terminated thread
/// ends; and three threads compiling at once, interleaved by the scheduler in
/// the middle of their compilations, each get their own compiler state.
#[test]
fn threads_share_a_world() {
    let out = run_compiled(
        "(use-package :quokka-thread) \
         (let ((th (make-thread (lambda (a b) (+ a b)) :name \"adder\" :arguments '(1 2)))) \
           (list (join-thread th) (thread-name th) (thread-alive-p th))) \
         (list (thread-name (current-thread)) (eq (current-thread) (main-thread)) \
               (length (list-all-threads))) \
         (let ((th (make-thread (lambda () (error \"boom\")) :name \"doomed\"))) \
           (list (join-thread th :default :failed) \
                 (handler-case (join-thread th) (join-thread-error () :join-error)))) \
         (let* ((m (make-mutex :name \"counter\")) (n 0) \
                (ths (mapcar (lambda (k) k \
                               (make-thread (lambda () (dotimes (i 40) \
                                 (with-mutex (m) (let ((v n)) (thread-yield) (setq n (+ v 1)))))))) \
                             '(1 2 3)))) \
           (mapc #'join-thread ths) \
           (list n (mutex-owner m))) \
         (let* ((m (make-mutex)) (q (make-waitqueue)) (box nil) \
                (consumer (make-thread (lambda () (with-mutex (m) (do () (box) (condition-wait q m)) box))))) \
           (thread-yield) \
           (with-mutex (m) (setq box :item) (condition-notify q)) \
           (join-thread consumer)) \
         (let ((th (make-thread (lambda () (do () (nil) (thread-yield)))))) \
           (thread-yield) \
           (terminate-thread th) \
           (list (join-thread th :default :ended) (thread-alive-p th))) \
         (defun compile-many (k) \
           (let ((ok t)) \
             (dotimes (i 12 ok) \
               (unless (= (funcall (compile nil (list 'lambda () \
                                                      (list '+ i k (list 'length (list 'quote (make-list 30))))))) \
                          (+ i k 30)) \
                 (setq ok nil))))) \
         (mapcar #'join-thread (mapcar (lambda (k) (make-thread (lambda () (compile-many k)))) '(100 200 300))) \
         (length (list-all-threads)) \
         (let* ((m (make-mutex)) (q (make-waitqueue)) \
                (a (make-thread (lambda () (with-mutex (m) (condition-wait q m)) :a)))) \
           (do () ((quokka::waitqueue-waiters q)) (thread-yield)) \
           (terminate-thread a) \
           (join-thread a :default nil) \
           (let ((b (make-thread (lambda () (with-mutex (m) (condition-wait q m)) :b)))) \
             (do () ((quokka::waitqueue-waiters q)) (thread-yield)) \
             (with-mutex (m) (condition-notify q)) \
             (list (join-thread b) (mutex-owner m) (length (quokka::waitqueue-waiters q)))))",
    );
    assert_eq!(
        out,
        "=> T\n=> (3 \"adder\" NIL)\n=> (\"main thread\" T 1)\n\
         ; thread \"doomed\" ended: boom\n=> (:FAILED :JOIN-ERROR)\n\
         => (120 NIL)\n=> :ITEM\n=> (:ENDED NIL)\n=> COMPILE-MANY\n=> (T T T)\n=> 1\n\
         => (:B NIL 0)\n"
    );
}

/// The console is the main process's (#34): a background thread's
/// `read-line` waits while the main process is busy, and reads the next line
/// only once the main process waits for it, so it never takes a line meant
/// for the main process; each thread reads into a buffer of its own.
#[test]
fn a_thread_waits_for_the_console() {
    let out = run_compiled_typing(
        "(let ((th (quokka-thread:make-thread (lambda () (read-line)) :name \"reader\"))) \
           (dotimes (i 4000) nil) \
           (let ((alive (not (null (quokka-thread:thread-alive-p th))))) \
             (list alive (quokka-thread:join-thread th)))) \
         (read-line)",
        &["for the thread", "for the main process"],
    );
    assert_eq!(
        out,
        "for the thread\n=> (T \"for the thread\")\nfor the main process\n=> \"for the main process\"\n"
    );
}

/// Channels between green threads (#34, layer 1), Go's model with Lisp
/// names: an unbuffered `send` waits until a receiver takes the value; a
/// buffered one waits only when the buffer is full; a closed channel gives
/// what it still holds and then nil and a false second value, and refuses a
/// `send`; `select` takes the first clause that can go, in the order
/// written, or its `otherwise`; `spawn-task` runs a function in a thread
/// and answers with a channel that receives its value, or re-signals its
/// error where the value is received. A Mandelbrot computed as one task a
/// row is the same as one computed row by row.
#[test]
fn channels_connect_threads() {
    let out = run_compiled(
        "(use-package :quokka-thread) \
         (let* ((ch (make-channel)) \
                (th (make-thread (lambda () (send ch :hello) :sent)))) \
           (dotimes (i 5) (thread-yield)) \
           (let ((waiting (not (null (thread-alive-p th))))) \
             (list waiting (receive ch) (join-thread th)))) \
         (let* ((ch (make-channel :capacity 2)) \
                (th (make-thread (lambda () (send ch 1) (send ch 2) (send ch 3) :done)))) \
           (dotimes (i 5) (thread-yield)) \
           (let ((waiting (not (null (thread-alive-p th))))) \
             (list waiting (receive ch) (receive ch) (receive ch) (join-thread th)))) \
         (let ((ch (make-channel :capacity 3))) \
           (send ch :a) \
           (close-channel ch) \
           (list (multiple-value-list (receive ch)) (multiple-value-list (receive ch)) \
                 (channel-closed-p ch) \
                 (handler-case (send ch :b) (error () :refused)))) \
         (let ((a (make-channel :capacity 1)) (b (make-channel :capacity 1))) \
           (send b :from-b) \
           (list (select ((receive a v) (list :a v)) ((receive b v) (list :b v))) \
                 (select ((receive a v) (list :a v)) (otherwise :nothing-ready)) \
                 (select ((send a :into-a) :sent-to-a) ((receive b v) (list :b v))) \
                 (receive a))) \
         (let ((ch (make-channel))) \
           (make-thread (lambda () (send ch :late))) \
           (select ((receive ch v ok) (list v ok)))) \
         (list (receive (spawn-task #'+ 1 2)) \
               (receive (spawn-task 'list :a :b)) \
               (handler-case (receive (spawn-task (lambda () (error \"bad task\")))) \
                 (error (c) (princ-to-string c)))) \
         (defun mandel-point (cr ci limit) \
           (do ((zr 0.0d0) (zi 0.0d0) (n 0 (+ n 1))) \
               ((or (= n limit) (> (+ (* zr zr) (* zi zi)) 4.0d0)) n) \
             (let ((next (+ (- (* zr zr) (* zi zi)) cr))) \
               (setq zi (+ (* 2.0d0 zr zi) ci)) \
               (setq zr next)))) \
         (defun mandel-row (y width height limit) \
           (let ((row (make-array width))) \
             (dotimes (x width row) \
               (setf (aref row x) \
                     (mandel-point (- (* 3.0d0 (/ x (float width 1.0d0))) 2.2d0) \
                                   (- (* 2.4d0 (/ y (float height 1.0d0))) 1.2d0) limit))))) \
         (let ((rows nil) (tasks nil)) \
           (dotimes (y 12) (push (mandel-row y 32 12 30) rows)) \
           (dotimes (y 12) (push (spawn-task 'mandel-row y 32 12 30) tasks)) \
           (equalp (reverse rows) (mapcar #'receive (reverse tasks))))",
    );
    assert_eq!(
        out,
        "=> T\n=> (T :HELLO :SENT)\n=> (T 1 2 3 :DONE)\n\
         => ((:A T) (NIL NIL) T :REFUSED)\n\
         => ((:B :FROM-B) :NOTHING-READY :SENT-TO-A :INTO-A)\n\
         => (:LATE T)\n\
         => (3 (:A :B) \"bad task\")\n\
         => MANDEL-POINT\n=> MANDEL-ROW\n=> T\n"
    );
}

/// What the review of the channels found, each a test (#34): a sender
/// waiting on an unbuffered channel that is closed either delivers or is
/// refused, never both, whenever the close falls; a select that sends and a
/// select that receives meet; `try-send` never waits, not even after a
/// receiver was terminated; a terminated sender leaves no value behind; a
/// task that ends without a value is a `thread-error` at `receive`; a body's
/// value, even `:none`, is the select's, with all its values; a task's
/// channel answers nil the second time.
#[test]
fn channels_review_findings() {
    let out = run_compiled(
        "(use-package :quokka-thread) \
         (let ((both 0) (ok 0) (refused 0)) \
           (dotimes (k 60) \
             (let* ((c (make-channel)) \
                    (th (make-thread (lambda () (handler-case (progn (send c :v) :ok) \
                                                    (closed-channel-error () :err)))))) \
               (thread-yield) \
               (dotimes (i k) nil) \
               (close-channel c) \
               (let ((got (receive c)) (sender (join-thread th))) \
                 (cond ((and (eq sender :err) got) (setq both (+ both 1))) \
                       ((eq sender :ok) (setq ok (+ ok 1))) \
                       (t (setq refused (+ refused 1))))))) \
           (list both (+ ok refused))) \
         (let ((c (make-channel))) \
           (make-thread (lambda () (select ((send c 1) :sent)))) \
           (select ((receive c v) v))) \
         (let* ((c (make-channel)) \
                (r (make-thread (lambda () (receive c))))) \
           (dotimes (i 3) (thread-yield)) \
           (terminate-thread r) \
           (join-thread r :default nil) \
           (list (try-send c :x) (multiple-value-list (try-receive c)))) \
         (let* ((c (make-channel)) \
                (s (make-thread (lambda () (send c :ghost))))) \
           (dotimes (i 3) (thread-yield)) \
           (terminate-thread s) \
           (join-thread s :default nil) \
           (multiple-value-list (try-receive c))) \
         (list (handler-case (receive (spawn-task (lambda () (abort)))) (thread-error () :ended)) \
               (let ((task (spawn-task #'+ 1 2))) \
                 (list (receive task) (multiple-value-list (receive task))))) \
         (let ((c (make-channel :capacity 3))) \
           (send c 1) (send c 2) \
           (list (select ((receive c v) v :none) (otherwise :other)) \
                 (multiple-value-list (select ((receive c v) (values v :second)))) \
                 (multiple-value-list (try-receive c))))",
    );
    assert_eq!(
        out,
        "=> T\n=> (0 60)\n=> 1\n=> (NIL (NIL NIL))\n=> (NIL NIL)\n\
         => (:ENDED (3 (NIL NIL)))\n=> (:NONE (2 :SECOND) (NIL NIL))\n"
    );
}

/// What the second review of the channels found (#34): ending a thread must
/// wait until it is safe. A thread ended at any of eighty moments while it
/// takes and lets go of a mutex never leaves the mutex held; a receiver ended
/// at any of forty moments while it starts to wait never takes a value with
/// it; the channels still work afterwards. An error in an operation is
/// signalled before the channels' lock is taken, so a handler may use a
/// channel; closing a channel twice is an error, as in Go; a task whose
/// channel its caller closed ends quietly.
#[test]
fn ending_a_thread_waits_until_it_is_safe() {
    let out = run_compiled(
        "(use-package :quokka-thread) \
         (let ((m (make-mutex)) (held 0)) \
           (dotimes (k 80) \
             (let ((th (make-thread (lambda () (do () (nil) (with-mutex (m) (list 1))))))) \
               (thread-yield) \
               (dotimes (i k) nil) \
               (terminate-thread th) \
               (join-thread th :default nil) \
               (when (mutex-owner m) (setq held (+ held 1)) \
                 (setf (svref (quokka::mutex-cell m) 0) nil)))) \
           held) \
         (let ((lost 0)) \
           (dotimes (k 40) \
             (let* ((c (make-channel)) \
                    (r (make-thread (lambda () (receive c))))) \
               (thread-yield) \
               (dotimes (i k) nil) \
               (terminate-thread r) \
               (join-thread r :default nil) \
               (when (try-send c :v) (setq lost (+ lost 1))))) \
           lost) \
         (let ((c (make-channel :capacity 1))) (send c 1) (receive c)) \
         (let ((log (make-channel :capacity 1))) \
           (handler-case (handler-bind ((error (lambda (c) c (try-send log :seen)))) (receive 5)) \
             (type-error () (list :typed (receive log))))) \
         (let ((c (make-channel))) \
           (close-channel c) \
           (handler-case (close-channel c) (closed-channel-error () :closed-twice))) \
         (let ((ch (spawn-task (lambda () (dotimes (i 50) (thread-yield)) 1)))) \
           (close-channel ch) \
           (dotimes (i 200) (thread-yield)) \
           :quiet)",
    );
    assert_eq!(
        out,
        "=> T\n=> 0\n=> 0\n=> 1\n=> (:TYPED :SEEN)\n=> :CLOSED-TWICE\n=> :QUIET\n"
    );
}

/// A thread an error stopped opens the debugger too, which says which thread
/// it is; `abort` there ends that thread, and the main thread carries on.
#[test]
fn a_thread_can_be_debugged() {
    let out = run_compiled_typing(
        "(quokka-debug:enable-debugger) \
         (let ((th (quokka-thread:make-thread (lambda () (car 'oops)) :name \"worker\"))) \
           (quokka-thread:join-thread th :default :aborted))",
        &["backtrace", "abort"],
    );
    assert!(
        out.contains("debugger invoked on a TYPE-ERROR in thread \"worker\": "),
        "{out}"
    );
    assert!(
        out.contains("  0: [ABORT] Terminate this thread.\n"),
        "{out}"
    );
    assert!(out.ends_with("0] backtrace\n0: (LAMBDA)\n0] abort\n; thread \"worker\" ended: The value OOPS is not of type LIST\n=> :ABORTED\n"), "{out}");
}

/// What only Quokka can be held to (#124): the default `#<...>` of an
/// instance, a condition and the metaobjects, which SBCL writes with an
/// address Quokka does not have.
#[test]
fn print_object_prints_metaobjects() {
    let out = run_compiled(
        "(defclass point () ((x :initarg :x))) \
         (defclass circle (point) ()) \
         (defgeneric area (s)) \
         (defmethod area ((s circle)) 1) \
         (defmethod area :around ((s point)) (call-next-method)) \
         (prin1-to-string (make-instance 'point)) \
         (prin1-to-string (list (make-instance 'circle))) \
         (prin1-to-string (find-class 'point)) \
         (prin1-to-string #'area) \
         (prin1-to-string (find-method #'area nil (list (find-class 'circle)))) \
         (prin1-to-string (find-method #'area '(:around) (list (find-class 'point)))) \
         (prin1-to-string (first (quokka::%class-slots (find-class 'point)))) \
         (handler-case (error \"x\") (error (c) (prin1-to-string c))) \
         (let ((*print-readably* t)) \
           (handler-case (prin1-to-string (make-instance 'point)) (print-not-readable () :nr))) \
         (progn (defmethod print-object ((c simple-error) s) \
                  (write-string \"[\" s) (call-next-method) (write-string \"]\" s)) :ok) \
         (handler-case (error \"y\") (error (c) (prin1-to-string c)))",
    );
    assert_eq!(
        out,
        "=> #<STANDARD-CLASS POINT>\n=> #<STANDARD-CLASS CIRCLE>\n=> #<STANDARD-GENERIC-FUNCTION AREA (0)>\n\
         => #<STANDARD-METHOD AREA (CIRCLE)>\n=> #<STANDARD-METHOD AREA :AROUND (POINT)>\n\
         => \"#<POINT>\"\n=> \"(#<CIRCLE>)\"\n=> \"#<STANDARD-CLASS POINT>\"\n\
         => \"#<STANDARD-GENERIC-FUNCTION AREA (2)>\"\n\
         => \"#<STANDARD-METHOD AREA (CIRCLE)>\"\n\
         => \"#<STANDARD-METHOD AREA :AROUND (POINT)>\"\n\
         => \"#<QUOKKA-MOP:STANDARD-EFFECTIVE-SLOT-DEFINITION X>\"\n\
         => \"#<SIMPLE-ERROR \\\"x\\\">\"\n=> :NR\n=> :OK\n=> \"[#<SIMPLE-ERROR \\\"y\\\">]\"\n"
    );
}

/// A `print-object` method on a built-in class is honoured wherever such an
/// object is printed (#124; CLHS 11.1.2.1.2 leaves it undefined, and SBCL
/// honours one on `cons` but not always one on `integer`), with
/// `call-next-method` reaching the printer's own code.
#[test]
fn print_object_on_a_built_in_class() {
    let out = run_compiled(
        "(prin1-to-string '(1 (2))) \
         (progn (defmethod print-object ((x integer) s) (write-string \"I\" s)) :ok) \
         (progn (defmethod print-object ((x cons) s) \
                  (write-string \"<\" s) (call-next-method) (write-string \">\" s)) :ok) \
         (prin1-to-string '(1 (2))) \
         (prin1-to-string (vector 3 '(4))) \
         (format nil \"~A\" 5)",
    );
    assert_eq!(
        out,
        "=> \"(1 (2))\"\n=> :OK\n=> :OK\n=> \"<(I <(I)>)>\"\n=> \"#(I <(I)>)\"\n=> \"I\"\n"
    );
}

/// `*print-circle*` still finds shared and circular tails once a program has
/// a `print-object` method on a built-in class, which sends every list
/// through `print-object` in the first pass too (#124, found in review: a
/// circular list then never finished printing).
#[test]
fn print_circle_after_a_method_on_a_built_in_class() {
    let out = run_compiled(
        "(progn (defmethod print-object ((h hash-table) s) (write-string \"TABLE\" s)) :ok) \
         (prin1-to-string (list (make-hash-table))) \
         (let* ((*print-circle* t) (x (list 1 2)) (y (list x (cdr x)))) (prin1-to-string y)) \
         (let* ((*print-circle* t) (x (list 1 2))) (setf (cdr (last x)) x) (prin1-to-string x)) \
         (let* ((*print-circle* t) (x (list 1 2))) (prin1-to-string (list x x)))",
    );
    assert_eq!(
        out,
        "=> :OK\n=> \"(TABLE)\"\n=> \"((1 . #1=(2)) #1#)\"\n=> \"#1=(1 2 . #1#)\"\n\
         => \"(#1=(1 2) #1#)\"\n"
    );
}

/// Under `*print-circle*` a structure with only the default methods is
/// walked by its slots in the first pass, as before `print-object`, not
/// printed through the generic function into a discarded stream: three
/// hundred of them fit the step budget (#124, found in review: 230 did,
/// against 350 before).
#[test]
fn print_circle_walks_plain_structures() {
    let out = run_compiled(
        "(progn (defstruct bp a b) :ok) \
         (let ((*print-circle* t) (l nil)) \
           (dotimes (i 300) (push (make-bp :a i :b (list i)) l)) \
           (< 0 (length (prin1-to-string l))))",
    );
    assert_eq!(out, "=> :OK\n=> T\n");
}

/// `describe` (#124): the text is Quokka's own, in outline what SBCL's is —
/// the object, its kind in brackets, then what that kind of object has.
#[test]
fn describe_calls_describe_object() {
    let out = run_compiled(
        "(progn (defclass dpoint () ((x :initarg :x) (y))) :ok) \
         (progn (defstruct dpos a b) :ok) \
         (with-output-to-string (s) (describe (make-instance 'dpoint :x 1) s)) \
         (with-output-to-string (s) (describe (make-dpos :a 1 :b \"b\") s)) \
         (with-output-to-string (s) (describe 5 s)) \
         (with-output-to-string (s) (describe '(1 2) s)) \
         (with-output-to-string (s) (describe \"abc\" s)) \
         (let ((s (with-output-to-string (o) (describe 'dpoint o)))) \
           (mapcar (lambda (x) (not (null x))) (list (search \"DPOINT names the standard-class #<STANDARD-CLASS DPOINT>\" s) \
                 (search \"Class precedence-list: DPOINT, STANDARD-OBJECT, T\" s) \
                 (search \"Direct superclasses: STANDARD-OBJECT\" s) \
                 (search \"No subclasses.\" s)))) \
         (defvar *dv* 3) \
         (not (null (search \"*DV* names a special variable:\" \
                            (with-output-to-string (o) (describe '*dv* o)))))",
    );
    assert_eq!(
        out,
        "=> :OK\n=> :OK\n\
         => \"#<DPOINT>\n  [standard-object]\n\nSlots with :INSTANCE allocation:\n  \
         X                              = 1\n  \
         Y                              = #<unbound slot>\n\"\n\
         => \"#S(DPOS :A 1 :B \\\"b\\\")\n  [structure-object]\n\nSlots with :INSTANCE allocation:\n  \
         A                              = 1\n  \
         B                              = \\\"b\\\"\n\"\n\
         => \"5\n  [fixnum]\n\"\n\
         => \"(1 2)\n  [list]\n\"\n\
         => \"\\\"abc\\\"\n  [simple-string]\n\nElement-type: CHARACTER\nLength: 3\n\"\n\
         => (T T T T)\n=> *DV*\n=> T\n"
    );
}

/// The library's own structures are described by their class, as SBCL
/// describes them, not as `[structure-object]`; a package by its names and
/// the packages it uses and is used by (#124).
#[test]
fn describe_names_the_library_structures() {
    let out = run_compiled(
        "(progn (defpackage :described (:use :cl) (:nicknames :dsc)) :ok) \
         (with-output-to-string (s) (describe (find-package :described) s)) \
         (let ((text (with-output-to-string (s) (describe *readtable* s)))) \
           (not (null (search \"[readtable]\" text)))) \
         (let ((text (with-output-to-string (s) (describe (make-string-output-stream) s)))) \
           (not (null (search \"[string-stream]\" text))))",
    );
    assert_eq!(
        out,
        "=> :OK\n\
         => \"#<PACKAGE \\\"DESCRIBED\\\">\n  [package]\n\nNicknames: DSC\n\
         Use-list: COMMON-LISP\nUsed-by-list: \n\"\n\
         => T\n=> T\n"
    );
}

/// An obsolete instance of a class that is not finalized just now, because
/// a redefinition named a superclass not defined yet, is brought up to the
/// layout the class was last finalized with, and reads its own slots
/// (#124, found in review: it read them through the new layout, swapped).
/// SBCL refuses the last `defclass`, so this is Quokka's alone.
#[test]
fn an_obsolete_instance_of_an_unfinalized_class() {
    let out = run_compiled(
        "(progn (defclass ra () ((x :initform :x) (y :initform :y))) :ok) \
         (defvar *ra* (make-instance 'ra)) \
         (progn (defclass ra () ((y :initform :y) (x :initform :x))) :ok) \
         (progn (defclass ra (not-defined-yet) ((y :initform :y) (x :initform :x) (z))) :ok) \
         (list (slot-value *ra* 'x) (slot-value *ra* 'y))",
    );
    assert_eq!(out, "=> :OK\n=> *RA*\n=> :OK\n=> :OK\n=> (:X :Y)\n");
}

/// `change-class` accepts a keyword that an `update-instance-for-different-class`
/// method applicable to both the previous and the new class declares (CLHS
/// 7.1.2); and the copy of a funcallable instance as it was is itself
/// funcallable, calling what the instance calls (#124, found in review).
/// SBCL 2.6.10 fails inside its own error reporting on both, so they are
/// held to CLHS here.
#[test]
fn change_class_initargs_and_funcallable_copies() {
    let out = run_compiled(
        "(progn (defclass ic0 () ()) (defclass ic2 () ((v :initform 0))) \
           (defmethod update-instance-for-different-class :after ((p ic0) (c ic2) &key extra) \
             (setf (slot-value c 'v) extra)) :ok) \
         (slot-value (change-class (make-instance 'ic0) 'ic2 :extra 3) 'v) \
         (progn (defclass fa () ((n :initarg :n)) (:metaclass quokka::funcallable-standard-class)) \
                (defclass fb () ((n :initarg :n)) (:metaclass quokka::funcallable-standard-class)) \
                (defvar *seen* nil) \
                (defmethod update-instance-for-different-class :before ((p fa) (c fb) &rest r) \
                  r (setq *seen* (list (functionp p) (funcall p 2) (slot-value p 'n)))) \
                :ok) \
         (let ((f (make-instance 'fa :n 1))) \
           (quokka::%set-funcallable-instance-function f (lambda (x) (* x 10))) \
           (change-class f 'fb) \
           (list *seen* (funcall f 3) (slot-value f 'n)))",
    );
    assert_eq!(out, "=> :OK\n=> 3\n=> :OK\n=> ((T 20 1) 30 1)\n");
}

/// C2 in #39's terms: the cold-built world, rebuilt once by itself.
fn rebuilt_image() -> &'static [u8] {
    static IMAGE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    IMAGE.get_or_init(|| rebuild(compiled_image()))
}

/// A rebuilt world is a whole world (#48): every differential file, tier 0
/// and tier 1, agrees with its baseline in it, as in the cold-built one.
#[test]
fn a_rebuilt_world_passes_every_differential_file() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/lisp");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("tests/lisp is there") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("lisp") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("a name")
            .to_string();
        let source = std::fs::read_to_string(&path).expect("the file reads");
        let expected =
            std::fs::read_to_string(path.with_extension("expected")).expect("a baseline");
        let actual = run_compiled_in(rebuilt_image(), &source);
        compare(&stem, &source, &expected, &actual);
        checked += 1;
    }
    assert!(
        checked >= 22,
        "only {checked} differential files were found"
    );
}

/// C3: the world rebuilt twice.
fn twice_rebuilt_image() -> &'static [u8] {
    static IMAGE: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    IMAGE.get_or_init(|| rebuild(rebuilt_image()))
}

/// And a rebuilt world can rebuild itself again — C3 from C2 — replacing as
/// many names from as many forms.
#[test]
fn a_rebuilt_world_rebuilds_again() {
    let out = run_compiled_in(
        twice_rebuilt_image(),
        "(list (+ 1 2) (read-from-string \"cl:car\"))",
    );
    assert_eq!(out, "=> (3 CAR)\n");
}

/// Every compiled function a world holds under a name, described so that two
/// generations can be compared: `(qualified name, description)`, sorted.
fn describe_world(image: &[u8]) -> Vec<(String, String)> {
    use quokka_vm::heap::symbol;
    let mut heap = quokka_vm::image::load(image, 1 << 22).expect("the image loads");
    heap.mutate(|mc| {
        let mut out = Vec::new();
        let mut cursor = mc.system_symbols();
        while cursor.is_cons() {
            let sym = mc.car(cursor);
            cursor = mc.cdr(cursor);
            let definition = mc.slot(sym, symbol::FUNCTION);
            if definition.is_object() {
                let mut gensyms = Vec::new();
                let text = describe(mc, definition, &mut gensyms, 0);
                out.push((qualified(mc, sym, &mut Vec::new()), text));
            }
        }
        out.sort();
        out
    })
}

/// A symbol as `PACKAGE::NAME`, a keyword as `:NAME`, and an uninterned one
/// — a gensym — as `#:G` and the order it was first met in, so that two
/// generations whose gensym counters differ still describe alike.
fn qualified<'h>(
    mc: &quokka_vm::heap::Mutation<'h>,
    sym: quokka_vm::value::Value<'h>,
    gensyms: &mut Vec<quokka_vm::value::Value<'h>>,
) -> String {
    use quokka_vm::heap::symbol;
    let name = mc.symbol_name(sym);
    let home = mc.slot(sym, symbol::HOME);
    if home.is_nil() {
        let n = gensyms.iter().position(|g| *g == sym).unwrap_or_else(|| {
            gensyms.push(sym);
            gensyms.len() - 1
        });
        return format!("#:G{n}");
    }
    match home.as_fixnum() {
        Some(0) => format!("?::{name}"),
        Some(_) => format!(":{name}"),
        // A package object: a `%package` structure, whose name is slot 1.
        None => format!("{}::{name}", mc.string_to_rust(mc.slot(home, 1))),
    }
}

/// Everything about V that a compiler decided, recursively: a function's
/// code bytes, parameters, stack depth, name and constants; a closure's
/// function and environment; lists, vectors and strings by content.
fn describe<'h>(
    mc: &quokka_vm::heap::Mutation<'h>,
    v: quokka_vm::value::Value<'h>,
    gensyms: &mut Vec<quokka_vm::value::Value<'h>>,
    depth: usize,
) -> String {
    use quokka_vm::heap::ObjectType;
    if depth > 64 {
        return "#<deep>".into();
    }
    if v.is_cons() {
        let mut parts = Vec::new();
        let mut cursor = v;
        while cursor.is_cons() {
            parts.push(describe(mc, mc.car(cursor), gensyms, depth + 1));
            cursor = mc.cdr(cursor);
        }
        if !cursor.is_nil() {
            parts.push(format!(". {}", describe(mc, cursor, gensyms, depth + 1)));
        }
        return format!("({})", parts.join(" "));
    }
    if !v.is_object() {
        return quokka_vm::printer::print(mc, v);
    }
    let header = mc.header(v);
    match header.object_type {
        ObjectType::Symbol => qualified(mc, v, gensyms),
        t if t.is_string() => format!("{t:?}{}", quokka_vm::printer::print(mc, v)),
        ObjectType::Function => {
            use quokka_vm::code::function;
            let code: String = quokka_vm::code::code_bytes(mc, v)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            format!(
                "fn[{} params={} stack={} code={} constants={}]",
                describe(mc, mc.slot(v, function::NAME), gensyms, depth + 1),
                describe(mc, mc.slot(v, function::PARAMS), gensyms, depth + 1),
                describe(mc, mc.slot(v, function::MAX_STACK), gensyms, depth + 1),
                code,
                describe(mc, mc.slot(v, function::CONSTANTS), gensyms, depth + 1),
            )
        }
        // An instance of a class (#124) is described by its class's name:
        // it is not something a compiler decided, and its slots lead into
        // the whole graph of classes and methods, which a walk bounded only
        // by depth would follow along every branch. `a_rebuilt_world_is_the_
        // same_world` still compares every word of it.
        ObjectType::StandardInstance | ObjectType::FuncallableInstance => {
            let wrapper =
                quokka_vm::instance::wrapper(mc, v).unwrap_or(quokka_vm::value::Value::NIL);
            let name = if wrapper.is_object() {
                let class = mc.slot(wrapper, 0);
                match quokka_vm::instance::slots(mc, class) {
                    Some(slots) if slots.is_object() => {
                        describe(mc, mc.slot(slots, 0), gensyms, depth + 1)
                    }
                    _ => "?".into(),
                }
            } else {
                "?".into()
            };
            format!("{:?}<{name}>", header.object_type)
        }
        t if t.slots_are_values() => {
            let slots: Vec<String> = (0..header.slots)
                .map(|i| describe(mc, mc.slot(v, i), gensyms, depth + 1))
                .collect();
            format!("{t:?}[{}]", slots.join(" "))
        }
        t => format!("{t:?}<{} raw words>", header.slots),
    }
}

/// The words of an image file: its header, then its heap.
fn image_words(image: &[u8]) -> (Vec<u64>, Vec<u64>) {
    let words: Vec<u64> = image
        .chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().expect("eight bytes")))
        .collect();
    let (header, heap) = words.split_at(quokka_vm::image::HEADER_WORDS);
    (header.to_vec(), heap.to_vec())
}

/// Where in an image's header the two counters live that differ between
/// generations by design. The header is magic, version, length, checksum,
/// then the scalars in `Scalars::to_words` order — sequence first, the
/// gensym counter seventeenth. Both are confirmed against the loaded world,
/// so a reordering fails here rather than allowing the wrong word.
const CHECKSUM_WORD: usize = 3;
const SEQUENCE_WORD: usize = 4;
const GENSYM_COUNTER_WORD: usize = 4 + 16;

fn confirm_header_layout(image: &[u8]) {
    let (header, _) = image_words(image);
    let mut heap = quokka_vm::image::load(image, 1 << 22).expect("the image loads");
    assert_eq!(
        header[SEQUENCE_WORD],
        heap.sequence(),
        "the sequence word moved"
    );
    // The counter holds the last number issued, so the next gensym is one
    // more. It is the machine's own counter, which the primitive `gensym`
    // advanced until the library's `gensym` (step Y, #72) took over with
    // `*gensym-counter*`; the cold build still uses it, so it is still in
    // the header, and asked for here directly.
    let name = heap.mutate(|mc| {
        let made = mc.gensym();
        mc.symbol_name(made)
    });
    assert_eq!(
        name,
        format!("G{}", header[GENSYM_COUNTER_WORD] + 1),
        "the gensym counter word moved"
    );
}

/// The heap of a saved world, object by object: each object's first word and
/// its type, read from headers the way the collector reads them (tag `101`,
/// subtag 31 in the low byte, the type in the next, the slot count above).
/// A word that is not a header begins a two-word cons.
fn objects(heap: &[u64]) -> Vec<(usize, u8, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < heap.len() {
        let word = heap[at];
        if word & 0xFF == 0xFD {
            let slots = (word >> 16) as usize;
            out.push((at, ((word >> 8) & 0xFF) as u8, slots));
            at += 1 + slots;
        } else {
            at += 2;
        }
    }
    assert_eq!(at, heap.len(), "the walk lost the object boundaries");
    out
}

/// The text of a string object at `at`: packed (eight Latin-1 bytes a word,
/// after a count) or one character a word.
fn string_at(heap: &[u64], at: usize, object_type: u8, slots: usize) -> String {
    if object_type == quokka_vm::heap::ObjectType::PackedString as u8 {
        // Bounded by the object's size: read with the other world's layout,
        // the count word may belong to something else.
        let count = (heap[at + 1] as usize).min(slots.saturating_sub(1) * 8);
        (0..count)
            .map(|i| char::from((heap[at + 2 + i / 8] >> ((i % 8) * 8)) as u8))
            .collect()
    } else {
        (0..slots)
            .filter_map(|i| char::from_u32(heap[at + 1 + i] as u32))
            .collect()
    }
}

/// Compare two saved worlds word by word, allowing only what differs between
/// generations by design: the names of uninterned symbols (gensyms), if they
/// correspond consistently, and the header's checksum, sequence number and
/// gensym counter. Returns what else differs.
fn whole_world_differences(a: &[u8], b: &[u8]) -> Vec<String> {
    use quokka_vm::heap::{symbol, ObjectType};
    let (header_a, heap_a) = image_words(a);
    let (header_b, heap_b) = image_words(b);
    let mut wrong = Vec::new();
    for (i, (x, y)) in header_a.iter().zip(&header_b).enumerate() {
        if x != y && ![CHECKSUM_WORD, SEQUENCE_WORD, GENSYM_COUNTER_WORD].contains(&i) {
            wrong.push(format!("header word {i}: {x:#x} against {y:#x}"));
        }
    }
    if heap_a.len() != heap_b.len() {
        wrong.push(format!(
            "heaps of {} and {} words",
            heap_a.len(),
            heap_b.len()
        ));
        return wrong;
    }
    // Every word of every gensym's name may differ — provided the two names
    // correspond one to one.
    let found = objects(&heap_a);
    let by_start: std::collections::HashMap<usize, (u8, usize)> =
        found.iter().map(|&(at, t, n)| (at, (t, n))).collect();
    let nil = quokka_vm::value::Value::NIL.bits();
    let mut allowed = std::collections::HashSet::new();
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut back: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for &(at, t, _) in &found {
        if t != ObjectType::Symbol as u8 || heap_a[at + 1 + symbol::HOME] != nil {
            continue;
        }
        let name_ref = heap_a[at + 1 + symbol::NAME];
        let name_at = (name_ref >> 3) as usize;
        let Some(&(nt, ns)) = by_start.get(&name_at) else {
            continue;
        };
        let (text_a, text_b) = (
            string_at(&heap_a, name_at, nt, ns),
            string_at(&heap_b, name_at, nt, ns),
        );
        if names
            .insert(text_a.clone(), text_b.clone())
            .is_some_and(|t| t != text_b)
            || back
                .insert(text_b.clone(), text_a.clone())
                .is_some_and(|t| t != text_a)
        {
            wrong.push(format!("gensym {text_a} does not correspond to one name"));
        }
        allowed.extend(name_at..=name_at + ns);
    }
    for (i, (x, y)) in heap_a.iter().zip(&heap_b).enumerate() {
        if x != y && !allowed.contains(&i) {
            wrong.push(format!("heap word {i}: {x:#x} against {y:#x}"));
        }
    }
    wrong
}

/// **The fixpoint** (#39's gate, step F). Every compiled function the
/// cold-built world holds under a name (C1, compiled by the interpreted
/// compiler) must be what the world rebuilt by itself holds (C2, compiled by
/// C1's compiled compiler), and what that rebuilds holds (C3).
#[test]
fn the_compiler_reproduces_itself() {
    let c1 = describe_world(compiled_image());
    let c2 = describe_world(rebuilt_image());
    let c3 = describe_world(twice_rebuilt_image());
    let report = |a: &[(String, String)], b: &[(String, String)], what: &str| {
        let names_a: Vec<&String> = a.iter().map(|(n, _)| n).collect();
        let names_b: Vec<&String> = b.iter().map(|(n, _)| n).collect();
        let differ: Vec<&String> = a
            .iter()
            .filter(|(n, d)| b.iter().any(|(m, e)| m == n && e != d))
            .map(|(n, _)| n)
            .collect();
        eprintln!(
            "{what}: {} and {} named functions; {} differ: {:?}",
            names_a.len(),
            names_b.len(),
            differ.len(),
            &differ[..differ.len().min(40)]
        );
        differ.len()
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fixpoint");
    let _ = std::fs::create_dir_all(&dir);
    for (file, world) in [("c1.txt", &c1), ("c2.txt", &c2), ("c3.txt", &c3)] {
        let text: Vec<String> = world.iter().map(|(n, d)| format!("{n}\n  {d}\n")).collect();
        let _ = std::fs::write(dir.join(file), text.concat());
    }
    let first = report(&c1, &c2, "C1 against C2");
    let second = report(&c2, &c3, "C2 against C3");
    assert_eq!((first, second), (0, 0), "see target/fixpoint/c*.txt");
}

/// **The fixpoint, for the whole world** (#50). C2 and C3 are the same
/// world: the same objects in the same places, word for word, except the
/// gensyms' names — a counter that only goes up — and the header's
/// checksum, sequence number and gensym counter. This sees what comparing
/// named functions cannot: code held in data, such as the readtable's reader
/// macros, every symbol, and the heap's layout.
#[test]
fn a_rebuilt_world_is_the_same_world() {
    confirm_header_layout(twice_rebuilt_image());
    let wrong = whole_world_differences(rebuilt_image(), twice_rebuilt_image());
    assert!(
        wrong.is_empty(),
        "C2 and C3 differ:\n{}",
        wrong[..wrong.len().min(20)].join("\n")
    );
}

/// The comparisons can fail: they are not passing by comparing nothing. One
/// redefined function is exactly one named function that differs, and a
/// world that has run one more form is not the same world.
#[test]
fn the_fixpoint_notices_a_change() {
    use quokka_vm::vm::eval_source;
    let mut heap = quokka_vm::image::load(rebuilt_image(), 1 << 22).expect("the image loads");
    let mut host = SharedHost(
        std::rc::Rc::new(std::cell::RefCell::new(String::new())),
        Vec::new(),
    );
    eval_source(
        &mut heap,
        "(defun abs (n) (if (minusp n) (- 0 n) n))",
        BUDGET,
        SLICE,
        &mut host,
        |_| true,
    )
    .expect("the redefinition runs");
    let changed = quokka_vm::image::save(&mut heap);
    let before = describe_world(rebuilt_image());
    let after = describe_world(&changed);
    let differ: Vec<&String> = before
        .iter()
        .filter(|(n, d)| after.iter().any(|(m, e)| m == n && e != d))
        .map(|(n, _)| n)
        .collect();
    assert_eq!(differ, ["COMMON-LISP::ABS"]);
    assert!(!whole_world_differences(rebuilt_image(), &changed).is_empty());
}

/// `rebuild.lisp` names the library's files itself, since it asks the host
/// for them; it must name the same files, in the same order, as the cold
/// build loads.
#[test]
fn the_rebuild_reads_the_library_the_cold_build_loads() {
    let names: Vec<String> = quokka_bootstrap::boot::LIBRARY
        .iter()
        .map(|(name, _)| format!("\"lisp/library/{name}\""))
        .collect();
    let out = run_compiled("quokka::%library-files");
    assert_eq!(out, format!("=> ({})\n", names.join(" ")));
}

/// `run_forms`, in a compiled world, entirely in the image's own Lisp
/// (#47): each form read by the Lisp reader in `CL-USER`, evaluated by
/// `eval` — compiled by the image's compiler and run on the VM — and printed
/// by the Lisp printer, one at a time, so a `defstruct` has run before the
/// form that uses its accessors is read, which is the order a file means. A
/// compile error is a condition, and so arrives as the form's `!! error`,
/// exactly as a deliberately malformed form errs at run time when
/// interpreted. The interpreted leg prints with the Rust printer against the
/// same baseline, so the two printers are held to one answer.
///
/// Anything a form writes to `*standard-output*` comes first, as SBCL's does
/// in the oracle's output.
fn run_compiled(source: &str) -> String {
    run_compiled_in(compiled_image(), source)
}

/// `run_compiled`, with LINES typed at the console, each ending in a
/// newline, for whatever reads it (#131).
fn run_compiled_typing(source: &str, lines: &[&str]) -> String {
    TYPED.with(|typed| {
        *typed.borrow_mut() = lines.iter().map(|l| format!("{l}\n")).collect();
    });
    let out = run_compiled(source);
    let left = TYPED.with(|typed| std::mem::take(&mut *typed.borrow_mut()));
    assert!(left.is_empty(), "lines nobody read: {left:?}\n{out}");
    out
}

/// `run_compiled`, in the world IMAGE holds.
fn run_compiled_in(image: &[u8], source: &str) -> String {
    use quokka_vm::vm::{eval_source, Outcome};
    let mut heap = quokka_vm::image::load(image, 1 << 22).expect("the image loads");
    let mut out = String::new();
    // The host's output is collected as it happens; it is moved to `out`
    // before each result, which is the order SBCL writes them in.
    let output = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let mut host = SharedHost(output.clone(), Vec::new());
    let ran = eval_source(&mut heap, source, BUDGET, SLICE, &mut host, |outcome| {
        out.push_str(&std::mem::take(&mut *output.borrow_mut()));
        match outcome {
            Outcome::Value(text) => {
                out.push_str("=> ");
                out.push_str(&text);
                out.push('\n');
            }
            Outcome::Error { condition, .. } => {
                out.push_str(&quokka_vm::condition::report(&condition))
            }
            Outcome::Failed(e) => out.push_str(&quokka_vm::condition::report_error(&e)),
        }
        true
    });
    if ran.is_err() {
        out.push_str("!! error\n");
    }
    out
}

/// A host whose console output the result callback can also reach — the
/// callback runs while `eval_source` holds the host itself — and which has
/// the files it is given, by name, and no others.
struct SharedHost(
    std::rc::Rc<std::cell::RefCell<String>>,
    Vec<(&'static str, &'static str)>,
);

thread_local! {
    /// The lines a test types at the console, for `run_compiled_typing`;
    /// empty, so the end of the input, for every other test.
    static TYPED: std::cell::RefCell<std::collections::VecDeque<String>> =
        const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

impl quokka_vm::host::Host for SharedHost {
    fn write_stdout(&mut self, s: &str) {
        self.0.borrow_mut().push_str(s);
    }
    /// The next typed line, written to the output after its prompt as a
    /// terminal would show it, so a transcript reads as a session did.
    fn read_line(&mut self, prompt: &str) -> Option<String> {
        let line = TYPED.with(|typed| typed.borrow_mut().pop_front())?;
        let mut out = self.0.borrow_mut();
        out.push_str(prompt);
        out.push_str(&line);
        Some(line)
    }
    fn read_source(&mut self, name: &str) -> Result<String, quokka_vm::host::HostError> {
        self.1
            .iter()
            .find(|(file, _)| *file == name)
            .map(|(_, text)| text.to_string())
            .ok_or_else(|| quokka_vm::host::HostError::NotFound(name.to_string()))
    }
}

/// `load` with a file name asks the host for the file (#47) — the only way
/// to test it, since SBCL's oracle would read a real file and Quokka's
/// harness has none. The file defines a function, changes package, and a
/// form after the `load` sees that the package change stayed inside it.
/// A funcallable instance is called through the function it holds (#124);
/// with none, or before one is set, calling it is an error. SBCL has no
/// such primitives, so this cannot be a differential file.
#[test]
fn a_funcallable_instance_is_called_through_its_function() {
    let out = run_compiled(
        "(defvar *f* (quokka::%allocate-funcallable-instance :w (vector))) \
         (handler-case (funcall *f* 1) (error () :no-function)) \
         (progn (quokka::%set-funcallable-instance-function *f* (lambda (x) (* x 2))) :set) \
         (funcall *f* 21) \
         (apply *f* '(4)) \
         (mapcar *f* '(1 2)) \
         (list (functionp *f*) (compiled-function-p *f*) (quokka::%funcallable-instance-p *f*)) \
         (quokka::%instance-wrapper *f*) \
         (handler-case (quokka::%set-funcallable-instance-function *f* *f*) (error () :refused))",
    );
    assert_eq!(
        out,
        "=> *F*\n=> :NO-FUNCTION\n=> :SET\n=> 42\n=> 8\n=> (2 4)\n=> (T NIL T)\n=> :W\n=> :REFUSED\n"
    );
}

/// An instance prints as `#<NAME>` and a class as `#<STANDARD-CLASS NAME>`
/// (#124), and neither can be printed readably. SBCL adds an address, so
/// this cannot be a differential file; `print-object` comes in sub-step 4.
#[test]
fn instances_and_classes_print_unreadably() {
    let out = run_compiled(
        "(defclass pp () ()) \
         (prin1-to-string (make-instance 'pp)) \
         (prin1-to-string (find-class 'integer)) \
         (handler-case (let ((*print-readably* t)) (prin1-to-string (make-instance 'pp))) \
           (print-not-readable () :refused))",
    );
    assert_eq!(
        out,
        "=> #<STANDARD-CLASS PP>\n=> \"#<PP>\"\n=> \"#<BUILT-IN-CLASS INTEGER>\"\n=> :REFUSED\n"
    );
}

/// A chain of `deftype`s that never ends is an error, not a hang (#124).
/// SBCL never returns from one, so this cannot be a differential file.
#[test]
fn a_circular_deftype_is_an_error() {
    let out = run_compiled(
        "(deftype loop-a () 'loop-b) (deftype loop-b () 'loop-a) \
         (handler-case (typep 1 'loop-a) (error () :error))",
    );
    assert_eq!(out, "=> LOOP-A\n=> LOOP-B\n=> :ERROR\n");
}

#[test]
fn load_reads_a_file_through_the_host() {
    use quokka_vm::vm::{eval_source, Outcome};
    let mut heap = quokka_vm::image::load(compiled_image(), 1 << 22).expect("the image loads");
    let output = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
    let file = "(defpackage :loaded (:use :cl)) (defun from-file () 42) (in-package :loaded)";
    let mut host = SharedHost(output.clone(), vec![("virtual.lisp", file)]);
    let mut results = Vec::new();
    let source =
        r#"(load "virtual.lisp") (from-file) (package-name *package*) (load "missing.lisp")"#;
    eval_source(&mut heap, source, BUDGET, SLICE, &mut host, |outcome| {
        results.push(outcome);
        true
    })
    .expect("the loop runs");
    assert_eq!(results[0], Outcome::Value("T".into()));
    assert_eq!(results[1], Outcome::Value("42".into()));
    assert_eq!(results[2], Outcome::Value("\"COMMON-LISP-USER\"".into()));
    assert!(
        matches!(results[3], Outcome::Error { .. }),
        "{:?}",
        results[3]
    );
}

/// Which printer writes a result in [`run_compiled_rust_read`].
#[derive(Clone, Copy, PartialEq)]
enum Printer {
    Rust,
    Lisp,
}

/// The compiled world as it was before #47: forms read by the *Rust* reader,
/// results printed by PRINTER. What the two system-wide tests above need,
/// since their point is to hold the Lisp reader and printer to the Rust ones.
fn run_compiled_rust_read(source: &str, printer: Printer) -> String {
    use quokka_vm::heap::Register;
    use quokka_vm::vm::{call_global, run_sliced};
    let mut heap = quokka_vm::image::load(compiled_image(), 1 << 22).expect("the image loads");
    let mut host = quokka_vm::host::NullHost::default();
    let mut out = String::new();
    let chars = quokka_bootstrap::reader::Reader::chars_of(source);
    let mut reader = quokka_bootstrap::reader::Reader::new(&chars);
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
        let ran = call_global(&mut heap, "COMPILE-FORM", BUDGET, SLICE, &mut host)
            .and_then(|()| run_sliced(&mut heap, BUDGET, SLICE, &mut host));
        // The form's value is in the Value register; the Lisp printer leaves
        // its text there instead. A printer that fails is a bug, not a result.
        if ran.is_ok() && printer == Printer::Lisp {
            call_global(&mut heap, "PRIN1-TO-STRING", BUDGET, SLICE, &mut host)
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
            Err(e) => out.push_str(&quokka_vm::condition::report_error(&e)),
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
