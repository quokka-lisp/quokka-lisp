//! What going wrong looks like, for both evaluators: the interpreter of the
//! cold build and the bytecode VM report the same errors in the same words.

/// What went wrong.
///
/// These become Lisp conditions in phase 3. Until then they stop the machine
/// and are reported, rather than panicking — a REPL that dies on a typo is not
/// a REPL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// A compiled function the verifier refused. Not a fault in the program
    /// being run — a fault in whatever produced the bytecode, which after
    /// phase 1 means the compiler.
    Malformed(String),
    /// A symbol with no value.
    UnboundVariable(String),
    /// A symbol with no function.
    UndefinedFunction(String),
    /// Something that is not a function in head position.
    NotAFunction(String),
    /// A `return-from`, `go` or `throw` with nowhere to go.
    NoSuchExit(String),
    /// The machine's own fault, or the bytecode's: an opcode that is not one,
    /// a missing constant, a compiled caller meeting an interpreted closure.
    /// Never the running program's mistake and never the program's to catch —
    /// a `handler-case` would hide a compiler bug behind whatever the program
    /// does about errors. Split from [`BadSyntax`](Self::BadSyntax) by #97:
    /// one variant meant both, so a malformed lambda list, which is the
    /// program's, stopped the machine as if it were the machine's.
    Fault {
        /// What was being done.
        operator: &'static str,
        /// What is wrong.
        detail: &'static str,
    },
    /// A special operator or a lambda list written wrongly: the program's
    /// syntax, a `program-error` in both evaluators.
    BadSyntax {
        /// Which operator.
        operator: &'static str,
        /// What is wrong.
        detail: &'static str,
    },
    /// A primitive given the wrong number of arguments.
    WrongArgumentCount {
        /// Which primitive.
        name: &'static str,
        /// How many it wanted.
        wanted: usize,
        /// How many it got.
        got: usize,
    },
    /// A compiled function given the wrong number of arguments, by name.
    /// [`WrongArgumentCount`](Self::WrongArgumentCount) names a primitive
    /// with a string the program holds; a function's name is in the heap
    /// and has to be copied out, so it is a variant of its own (#110).
    WrongArgumentsTo {
        /// The function's name.
        function: String,
        /// How many it wanted.
        wanted: usize,
        /// How many it got.
        got: usize,
    },
    /// A call whose arguments do not fit the function's lambda list: an odd
    /// number of keyword arguments, or a keyword the function does not take.
    /// A fault in the *call*, so the program may catch it — CLHS calls it a
    /// `program-error` (#62, part 1).
    BadCall(&'static str),
    /// A primitive given the wrong kind of thing.
    WrongType {
        /// Which primitive.
        name: &'static str,
        /// What it wanted.
        wanted: &'static str,
    },
    /// A condition reached the top without being handled.
    Signalled(String),
    /// An index outside a sequence.
    IndexOutOfBounds {
        /// What was asked for.
        index: i64,
        /// How long the sequence is.
        length: usize,
    },
    /// Arithmetic left fixnum range, and bignums do not exist.
    IntegerOverflow,
    /// Division by zero, or a float operation whose result would be an
    /// infinity or a NaN (#17, decision 4).
    Arithmetic {
        /// The Common Lisp condition type, such as `DIVISION-BY-ZERO`.
        condition: &'static str,
        /// What happened, for a person.
        detail: &'static str,
    },
    /// The heap reached its hard ceiling. Common Lisp calls this a
    /// `storage-condition`; it becomes one in phase 3.
    HeapExhausted,
    /// The step budget ran out. Not an error in the program so much as a
    /// refusal to wait any longer — phase 3 replaces it with Ctrl-C landing in
    /// the debugger, which is what a person actually wants.
    BudgetExhausted,
    /// The computation asked the host for something and the driver running it
    /// has no host to answer. Not the program's fault — a test or a tool ran
    /// it with [`crate::vm::run_now`], which cannot answer requests (#35).
    Blocked,
}

impl core::fmt::Display for EvalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EvalError::UnboundVariable(n) => write!(f, "the variable {n} is unbound"),
            EvalError::UndefinedFunction(n) => write!(f, "the function {n} is undefined"),
            EvalError::NotAFunction(n) => write!(f, "{n} is not a function"),
            EvalError::Malformed(what) => write!(f, "this compiled function is malformed: {what}"),
            EvalError::NoSuchExit(n) => write!(f, "there is no exit named {n} here"),
            EvalError::BadSyntax { operator, detail } | EvalError::Fault { operator, detail } => {
                write!(f, "bad {operator}: {detail}")
            }
            EvalError::BadCall(detail) => write!(f, "bad call: {detail}"),
            EvalError::WrongArgumentCount { name, wanted, got } => {
                write!(f, "{name} wants {wanted} argument(s), got {got}")
            }
            EvalError::WrongArgumentsTo {
                function,
                wanted,
                got,
            } => write!(f, "{function} wants {wanted} argument(s), got {got}"),
            EvalError::WrongType { name, wanted } => write!(f, "{name} wants {wanted}"),
            EvalError::Signalled(what) => write!(f, "{what}"),
            EvalError::IndexOutOfBounds { index, length } => {
                write!(f, "index {index} is outside a sequence of length {length}")
            }
            EvalError::IntegerOverflow => {
                write!(f, "integer overflow, and bignums do not exist yet")
            }
            EvalError::Arithmetic { detail, .. } => write!(f, "{detail}"),
            EvalError::HeapExhausted => write!(
                f,
                "out of heap: what the program holds no longer fits in it, or \
                 one object was asked for that is larger than the heap may grow"
            ),
            EvalError::BudgetExhausted => write!(
                f,
                "gave up after the step budget — a computation that does not finish, perhaps?"
            ),
            EvalError::Blocked => write!(
                f,
                "waiting for the host to answer a request, and nothing here can answer it"
            ),
        }
    }
}
