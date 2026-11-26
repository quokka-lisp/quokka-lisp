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
        BUDGET * 60,
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
        [Outcome::Value("(:FORMS 1692 :REPLACED 1372)".into())],
        "the rebuild did not finish as expected"
    );
    quokka_vm::image::save(&mut heap)
}

/// A warm rebuild keeps every class (#124): it reloads `clos.lisp` while
/// classes and instances are alive, so the bootstrap refills the classes it
/// finds, a class whose slots did not change keeps its wrapper, and an
/// instance made before the rebuild is still an instance of its class.
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
         (defvar *mi* #'make-instance) \
         (defvar *w* (quokka::%class-wrapper (find-class 'standard-object))) \
         (defvar *sc* (find-class 'standard-class)) \
         (defvar *ic* (find-class 'integer)) \
         (quokka::%rebuild-world) \
         (list (eq *w* (quokka::%class-wrapper (find-class 'standard-object))) \
               (eq *sc* (find-class 'standard-class)) \
               (eq *ic* (find-class 'integer)) \
               (eq (class-of (find-class 'standard-class)) (find-class 'standard-class)) \
               (kept-a *i*) (typep *i* 'kept) (slot-value (make-instance 'kept :a 2) 'a) \
               (eq *mi* #'make-instance) (area *i*))",
        BUDGET * 60,
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
        Some(&Outcome::Value("(T T T T 1 T 2 T 10)".into())),
        "{said:?}"
    );
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
        let count = heap[at + 1] as usize;
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

impl quokka_vm::host::Host for SharedHost {
    fn write_stdout(&mut self, s: &str) {
        self.0.borrow_mut().push_str(s);
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
