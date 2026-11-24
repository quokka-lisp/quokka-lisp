//! The ansi-test run (#60, step N of #39): Quokka measured against the
//! conformance suite of Paul Dietz, group by group.
//!
//! Ignored by `cargo test`: it needs the suite in `target/ansi-test/`, which
//! `scripts/ansi-test.sh` fetches at a pinned commit (decision 1), and it
//! takes minutes. The script runs it, and so does its own CI job (decision
//! 5):
//!
//! ```text
//! cargo test --release -p quokka-bootstrap --test ansi -- --ignored --nocapture
//! ```
//!
//! The world loads `tests/ansi/runner.lisp`, a small runner in tier-1 Lisp
//! (decision 2), and this driver feeds it one file at a time and one form at
//! a time — `(rt::%step)` — each with its own step budget, so a test that
//! never finishes fails alone. Every result goes to
//! `target/ansi-test-results.txt`.
//!
//! **The ratchet** (decision 3): `tests/ansi/baseline.txt` lists every test
//! that passed when it was last updated. The run fails if one of those no
//! longer passes. With `ANSI_TEST_UPDATE=1` it instead rewrites the baseline
//! and the summary in `docs/ansi-test.md`.

use quokka_vm::host::{Host, HostError};
use quokka_vm::vm::{eval_source, Outcome};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The groups tier 1 is measured by (#60, decision 3), in this order.
/// `arrays` joined when step T moved arrays into tier 1 (#67).
const GROUPS: &[&str] = &[
    "cons",
    "sequences",
    "strings",
    "arrays",
    "characters",
    "symbols",
    "packages",
    "numbers",
    "reader",
    "printer",
    "data-and-control-flow",
    "eval-and-compile",
    "iteration",
    "hash-tables",
    "structures",
];

/// What ansi-test's own loader (`gclload1.lsp`) loads before any group:
/// the helpers tests call, the universe of sample objects, the notes.
const SUPPORT: &[&str] = &[
    "auxiliary/ansi-aux-macros.lsp",
    "universe.lsp",
    "auxiliary/random-aux.lsp",
    "auxiliary/ansi-aux.lsp",
    "cl-symbol-names.lsp",
    "notes.lsp",
];

/// Steps one form may take. A test that needs more is reported as TIMEOUT.
const STEP_BUDGET: u64 = 10_000_000;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Files by their names relative to the repository root, from the disk.
struct DiskHost {
    root: PathBuf,
}

impl Host for DiskHost {
    fn write_stdout(&mut self, _s: &str) {}
    fn read_source(&mut self, name: &str) -> Result<String, HostError> {
        let bytes = std::fs::read(self.root.join(name))
            .map_err(|_| HostError::NotFound(name.to_string()))?;
        // A few test files are Latin-1, not UTF-8.
        Ok(match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
        })
    }
}

/// Run SOURCE, one form, and return what it printed, or the failure.
fn run_one(heap: &mut quokka_vm::heap::Heap, host: &mut DiskHost, source: &str) -> Outcome {
    run_with_budget(heap, host, source, STEP_BUDGET)
}

/// Run SOURCE with BUDGET steps.
fn run_with_budget(
    heap: &mut quokka_vm::heap::Heap,
    host: &mut DiskHost,
    source: &str,
    budget: u64,
) -> Outcome {
    let mut outcome = Outcome::Failed(quokka_vm::error::EvalError::Blocked);
    let ran = eval_source(heap, source, budget, 2_000, host, |o| {
        outcome = o;
        false
    });
    if let Err(e) = ran {
        return Outcome::Failed(e);
    }
    outcome
}

/// A string as `prin1` wrote it, read back.
fn unquote(printed: &str) -> String {
    let inner = printed.strip_prefix('"').and_then(|s| s.strip_suffix('"'));
    let mut out = String::new();
    let mut chars = inner.unwrap_or(printed).chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The files of GROUP in its `load.lsp`'s order: the auxiliary files it
/// compiles first, then its test files, following a nested `load.lsp`. The
/// names in a nested one are relative to the group (`printer/format/load.lsp`
/// says `format/format-c.lsp`), so DIR is the group's directory throughout
/// and LOAD the file being read.
fn group_files(suite: &Path, dir: &str) -> Vec<String> {
    group_files_in(suite, dir, &format!("{dir}/load.lsp"))
}

fn group_files_in(suite: &Path, dir: &str, load: &str) -> Vec<String> {
    let text = std::fs::read_to_string(suite.join(load)).unwrap_or_default();
    let mut files = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with(';') {
            continue;
        }
        let quoted = line.split('"').nth(1);
        if let Some(name) = quoted {
            if line.contains("compile-and-load") {
                files.push(format!("auxiliary/{name}"));
            } else if line.starts_with("(load ") && name.ends_with("load.lsp") {
                files.extend(group_files_in(suite, dir, &format!("{dir}/{name}")));
            } else if line.starts_with("(load ") {
                files.push(format!("{dir}/{name}"));
            }
        }
    }
    files
}

/// One result: the group, the status word, the test's name, the rest.
struct TestResult {
    group: String,
    status: String,
    name: String,
    detail: String,
}

/// Run every form of FILE, collecting the test results and the load lines.
fn run_file(
    heap: &mut quokka_vm::heap::Heap,
    host: &mut DiskHost,
    group: &str,
    file: &str,
    results: &mut Vec<TestResult>,
    log: &mut String,
) {
    let path = format!("target/ansi-test/{file}");
    match run_one(heap, host, &format!("(rt::%open \"{path}\")")) {
        Outcome::Value(_) => {}
        other => {
            log.push_str(&format!("{file}: cannot open: {other:?}\n"));
            return;
        }
    }
    let mut last_error = None;
    loop {
        match run_one(heap, host, "(rt::%step)") {
            Outcome::Value(printed) => {
                last_error = None;
                let text = unquote(&printed);
                if text == "DONE" {
                    return;
                }
                for line in text.lines() {
                    record(group, file, line, results, log);
                }
            }
            Outcome::Error(message) => {
                log.push_str(&format!("{file}: the runner failed: {message}\n"));
                if last_error.as_ref() == Some(&message) {
                    return;
                }
                last_error = Some(message);
            }
            Outcome::Failed(e) => {
                let name = match run_one(heap, host, "rt::*current*") {
                    Outcome::Value(name) => unquote(&name),
                    _ => "?".to_string(),
                };
                // Out of steps, or an error that escaped every Lisp handler —
                // which no error should do, so that is a fault in the machine.
                let status = match e {
                    quokka_vm::error::EvalError::BudgetExhausted => "TIMEOUT",
                    _ => "ESCAPED",
                };
                // A test that ran out of steps and uses something Quokka
                // lacks is explained by it, as a failing one is (#81).
                let needs = match (status, run_one(heap, host, "(rt::%timeout-needs)")) {
                    ("TIMEOUT", Outcome::Value(text)) => unquote(&text),
                    _ => String::new(),
                };
                let (status, detail) = if needs.is_empty() {
                    (status.to_string(), e.to_string())
                } else {
                    ("NEEDS".to_string(), needs)
                };
                results.push(TestResult {
                    group: group.to_string(),
                    status,
                    name,
                    detail,
                });
            }
        }
    }
}

fn record(group: &str, file: &str, line: &str, results: &mut Vec<TestResult>, log: &mut String) {
    let mut words = line.splitn(3, ' ');
    let status = words.next().unwrap_or("");
    match status {
        "PASS" | "FAIL" | "NEEDS" | "UNREADABLE" | "SKIP" => results.push(TestResult {
            group: group.to_string(),
            status: status.to_string(),
            name: words.next().unwrap_or("?").to_string(),
            detail: words.next().unwrap_or("").to_string(),
        }),
        _ => log.push_str(&format!("{file}: {line}\n")),
    }
}

#[test]
#[ignore]
fn ansi_test() {
    let root = repo_root();
    let suite = root.join("target/ansi-test");
    if !suite.join("rt.lsp").exists() {
        eprintln!("ansi-test: target/ansi-test is missing; run scripts/ansi-test.sh");
        return;
    }
    let mut heap = quokka_vm::heap::Heap::new();
    quokka_bootstrap::boot::build_image(&mut heap).unwrap_or_else(|e| panic!("{e}"));
    let image = quokka_vm::image::save(&mut heap);
    let mut heap = quokka_vm::image::load(&image, 1 << 22).expect("the image loads");
    let mut host = DiskHost { root: root.clone() };
    for setup in [
        "(load \"tests/ansi/runner.lisp\")",
        "(load \"tests/ansi/cl-operators.lisp\")",
        "(load \"tests/ansi/deferred.lisp\")",
        "(rt::%prepare)",
    ] {
        // Loading a whole file is one form here, so it gets a file's budget
        // rather than a test's, as `quokka build` gives its sources: the
        // runner alone came to just under one test's ten million steps, and
        // step T's arrays tipped it over (#67).
        match run_with_budget(&mut heap, &mut host, setup, STEP_BUDGET * 40) {
            Outcome::Value(_) => {}
            other => panic!("{setup}: {other:?}"),
        }
    }

    let mut results = Vec::new();
    let mut log = String::new();
    for file in SUPPORT {
        run_file(
            &mut heap,
            &mut host,
            "support",
            file,
            &mut results,
            &mut log,
        );
    }
    // ANSI_TEST_ONLY=cons/cons.lsp,… runs those files only, after the
    // support files, for working on one corner; it reports and checks nothing.
    if let Ok(only) = std::env::var("ANSI_TEST_ONLY") {
        for file in only.split(',') {
            run_file(&mut heap, &mut host, "only", file, &mut results, &mut log);
        }
        for r in &results {
            eprintln!("{} {} {} {}", r.group, r.status, r.name, r.detail);
        }
        eprint!("{log}");
        return;
    }
    for group in GROUPS {
        for file in group_files(&suite, group) {
            run_file(&mut heap, &mut host, group, &file, &mut results, &mut log);
        }
        eprintln!("ansi-test: {group} done");
    }

    let mut out = String::new();
    for r in &results {
        out.push_str(&format!(
            "{} {} {} {}\n",
            r.group, r.status, r.name, r.detail
        ));
    }
    std::fs::write(root.join("target/ansi-test-results.txt"), out).unwrap();
    std::fs::write(root.join("target/ansi-test-load.txt"), &log).unwrap();

    let summary = summarise(&results);
    eprintln!("{summary}");

    let passed: BTreeSet<String> = results
        .iter()
        .filter(|r| r.status == "PASS")
        .map(|r| format!("{} {}", r.group, r.name))
        .collect();
    let baseline_path = root.join("tests/ansi/baseline.txt");
    if std::env::var("ANSI_TEST_UPDATE").is_ok() {
        let mut text = String::from(
            "# The ansi-test tests that pass (#60): the ratchet. Written by\n\
             # ANSI_TEST_UPDATE=1 scripts/ansi-test.sh; a run fails if one of these does not pass.\n",
        );
        for line in &passed {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(&baseline_path, text).unwrap();
        std::fs::write(root.join("docs/ansi-test.md"), summary).unwrap();
        return;
    }
    let baseline = std::fs::read_to_string(&baseline_path).unwrap_or_default();
    let lost: Vec<&str> = baseline
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter(|l| !passed.contains(*l))
        .collect();
    assert!(
        lost.is_empty(),
        "{} test(s) that passed no longer do:\n{}",
        lost.len(),
        lost.join("\n")
    );
}

/// Reader syntax that belongs to a deferred feature, so a test Quokka cannot
/// read because of it is explained (#60, decision 3); any other reader
/// reason is a gap in tier 1's reader.
const DEFERRED_SYNTAX: &[(&str, &str)] = &[
    ("reader[#C]", "deferred:complex-numbers"),
    ("reader[#P]", "tier-2:pathnames-and-files"),
];

/// Is this reason explained by a deferred feature?
fn deferred_reason(reason: &str) -> bool {
    if reason.starts_with("reader[") {
        return DEFERRED_SYNTAX.iter().any(|(r, _)| *r == reason);
    }
    reason.contains('[') && !reason.contains("[tier-1]") && !reason.starts_with("load-error")
}

/// The summary `docs/ansi-test.md` holds: per group, how many tests pass,
/// fail, escape every handler, run out of steps, need something tier 1
/// should have, or need only deferred features; then every need, by how
/// many tests it blocks.
fn summarise(results: &[TestResult]) -> String {
    const COLUMNS: [&str; 7] = [
        "pass",
        "fail",
        "escaped",
        "timeout",
        "needs tier 1",
        "needs later",
        "total",
    ];
    let mut table: BTreeMap<&str, [usize; 7]> = BTreeMap::new();
    let mut gaps: BTreeMap<String, usize> = BTreeMap::new();
    let mut deferred: BTreeMap<String, usize> = BTreeMap::new();
    for r in results.iter().filter(|r| r.group != "support") {
        let row = table.entry(r.group.as_str()).or_insert([0; 7]);
        let column = match r.status.as_str() {
            "PASS" => 0,
            "FAIL" => 1,
            "ESCAPED" => 2,
            "TIMEOUT" => 3,
            "NEEDS" | "UNREADABLE" if r.detail.split(' ').all(deferred_reason) => 5,
            "NEEDS" | "UNREADABLE" => 4,
            _ => 5,
        };
        row[column] += 1;
        row[6] += 1;
        if r.status == "NEEDS" || r.status == "UNREADABLE" {
            for reason in r.detail.split(' ').filter(|w| !w.is_empty()) {
                let count = if deferred_reason(reason) {
                    deferred.entry(reason.to_string()).or_default()
                } else {
                    gaps.entry(reason.to_string()).or_default()
                };
                *count += 1;
            }
        }
    }
    let mut text = String::from(
        "# Quokka against ansi-test\n\n\
         Written by `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` (#60); do not edit.\n\
         *escaped* is an error no Lisp handler could catch, a fault in the machine;\n\
         *needs tier 1* a test that uses something Quokka lacks and tier 1 should\n\
         have (an operator, or reader syntax); *needs later* one that uses only\n\
         features deferred to a later tier or issue (`tests/ansi/deferred.lisp`).\n\
         A test can need several things.\n\n",
    );
    text.push_str(&format!("| group | {} |\n", COLUMNS.join(" | ")));
    text.push_str(&format!("| --- |{}\n", " ---: |".repeat(COLUMNS.len())));
    let mut totals = [0usize; 7];
    for group in GROUPS {
        if let Some(row) = table.get(group) {
            for (t, n) in totals.iter_mut().zip(row) {
                *t += n;
            }
            let cells: Vec<String> = row.iter().map(|n| n.to_string()).collect();
            text.push_str(&format!("| {group} | {} |\n", cells.join(" | ")));
        }
    }
    let cells: Vec<String> = totals.iter().map(|n| format!("**{n}**")).collect();
    text.push_str(&format!("| **all** | {} |\n", cells.join(" | ")));
    for (title, counts) in [
        ("Tier-1 gaps, by how many tests each blocks", &gaps),
        (
            "Deferred features, by how many tests each blocks",
            &deferred,
        ),
    ] {
        text.push_str(&format!(
            "\n## {title}\n\n| needs | tests |\n| --- | ---: |\n"
        ));
        let mut sorted: Vec<(&String, &usize)> = counts.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (reason, n) in sorted {
            text.push_str(&format!("| `{reason}` | {n} |\n"));
        }
    }
    text
}
