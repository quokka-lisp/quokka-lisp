//! The primitive table: the only door from Lisp into Rust.
//!
//! Rule 1 says Rust provides the VM and everything a user might want to change
//! lives in Lisp. These are the operations that cannot: arithmetic, allocation,
//! the accessors that define what a cons cell *is*. Everything else — `list`,
//! `mapcar`, `append` — is Lisp built on these, and belongs there.
//!
//! The table's size is part of the comprehensibility budget. Each entry is a
//! reason the system is less changeable by the people using it, so each one
//! should have to justify itself.
//!
//! # Rule 9: primitives are bounded
//!
//! A step budget only bounds time if one step is short, and nothing can
//! interrupt a primitive once control is inside it. Most of these are O(1) on
//! their arguments. The ones that are not are bounded another way, each
//! saying how where it is defined: a size is checked against the heap before
//! anything is allocated (`size`); a walk along a list stops on a cycle
//! (`Mutation::list_elements`); a hash looks at a fixed number of elements.
//! A few are still linear in one step — `%assemble`, a stale table's rehash —
//! and say so. A new primitive whose cost grows with its argument needs one
//! of these answers before it is added.
//!
//! # Indices are part of the image format
//!
//! A primitive is an immediate holding an index into this table. An image
//! carries those, so the VM that loads an image must have the same table in the
//! same order. Adding an entry at the end is a format addition; reordering is a
//! breaking change.

use crate::error::EvalError;
use crate::float::{self, Op};
use crate::heap::{symbol, Mutation, ObjectType};
use crate::value::Value;
use core::cmp::Ordering;

/// Which primitive.
///
/// An enum rather than a string, so that `call_slice`'s `match` has no
/// catch-all and the compiler is what guarantees every row of the table has
/// an arm. Before this, adding a row without writing an arm compiled cleanly
/// and failed at run time with "FOO is not a function", and `"CDR"` and
/// `"CRD"` were two string literals that simply did not match.
///
/// The discriminants are explicit because **the order is part of the image
/// format** — an image stores a primitive as its index, so a VM loading one
/// must have the same table in the same order. Appending is a format
/// addition; renumbering is a breaking change. See `the_table_is_in_order`.
///
/// The variants are deliberately undocumented one by one. `Prim::Add` with a
/// doc comment reading `` `+` `` is noise, and the authoritative statement of
/// what each one is — its Lisp name, its arity, and the paragraph explaining
/// why it is a primitive at all — is the table immediately below, which is
/// the thing to read. The three whose spelling is not obvious are `NumEq`,
/// `Lt` and `Gt`, for `=`, `<` and `>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum Prim {
    Add = 0,
    Sub = 1,
    Mul = 2,
    NumEq = 3,
    Lt = 4,
    Gt = 5,
    Cons = 6,
    Car = 7,
    Cdr = 8,
    Eq = 9,
    Null = 10,
    Consp = 11,
    Symbolp = 12,
    Funcall = 13,
    List = 14,
    Append = 15,
    Gensym = 16,
    SetSymbolFunction = 17,
    SymbolFunction = 18,
    Keywordp = 19,
    SymbolName = 20,
    Apply = 21,
    MacroFunction = 22,
    MakeVector = 23,
    Vector = 24,
    Aref = 25,
    SetAref = 26,
    SequenceLength = 27,
    Vectorp = 28,
    Stringp = 29,
    Characterp = 30,
    MakeHashTable = 31,
    Gethash = 32,
    SetGethash = 33,
    Remhash = 34,
    HashTableCount = 35,
    HashTableP = 36,
    HashTableEntries = 37,
    Rplaca = 38,
    Rplacd = 39,
    Error = 40,
    ConditionType = 41,
    ConditionMessage = 42,
    Conditionp = 43,
    Intern = 44,
    StringAppend = 45,
    VectorToList = 46,
    Values = 47,
    ValuesList = 48,
    MakeString = 49,
    Assemble = 50,
    Integerp = 51,
    Disassemble = 52,
    RewriteLambdaList = 53,
    ReadSource = 54,
    SpecialOperatorP = 55,
    RewriteMacroLambdaList = 56,
    MakeMacro = 57,
    Boundp = 58,
    SymbolValue = 59,
    Set = 60,
    SymbolHome = 61,
    SetSymbolHome = 62,
    SystemSymbols = 63,
    FindSystemSymbol = 64,
    MakeSymbol = 65,
    InternKeyword = 66,
    CharCode = 67,
    CodeChar = 68,
    CharUpcase = 69,
    CharDowncase = 70,
    AlphaCharP = 71,
    ConstantString = 72,
    WriteConsole = 73,
    Quotient = 74,
    Rem = 75,
    Printed = 76,
    Makunbound = 77,
    MakeBignum = 78,
    BignumLength = 79,
    BignumNegativeP = 80,
    BignumLimb = 81,
    SetBignumLimb = 82,
    BignumNormalize = 83,
    Bignump = 84,
    FixnumToBignum = 85,
    IntegerCompare = 86,
    FixnumLogand = 87,
    FixnumLogior = 88,
    FixnumLogxor = 89,
    Divide = 90,
    Floatp = 91,
    Float = 92,
    DoubleEql = 93,
    DoubleRound = 94,
    DoubleInteger = 95,
    DecodeDouble = 96,
    ScaleDouble = 97,
    ParseDouble = 98,
    DoubleDigits = 99,
    Sqrt = 100,
    Exp = 101,
    Log = 102,
    Sin = 103,
    Cos = 104,
    Tan = 105,
    Asin = 106,
    Acos = 107,
    Atan = 108,
    Sinh = 109,
    Cosh = 110,
    Tanh = 111,
    Atan2 = 112,
    Pow = 113,
    SymbolPlist = 114,
    SetSymbolPlist = 115,
    Fboundp = 116,
    Fmakunbound = 117,
    Functionp = 118,
    MakeArrayHeader = 119,
    ArrayHeaderSlot = 120,
    SetArrayHeaderSlot = 121,
    MakeBitVector = 122,
    ArrayKind = 123,
    Asinh = 124,
    Acosh = 125,
    Atanh = 126,
    HashBucket = 127,
    HashInsert = 128,
    HashDeleteEntry = 129,
    Sxhash = 130,
    HashTableTest = 131,
    MakeStructure = 132,
    StructureLayout = 133,
    StructureRef = 134,
    SetStructureRef = 135,
    MakeNilVector = 136,
    MakeRatio = 137,
    Ratiop = 138,
    RatioNumerator = 139,
    RatioDenominator = 140,
    StringMismatch = 141,
    AllocateStandardInstance = 142,
    AllocateFuncallableInstance = 143,
    InstanceWrapper = 144,
    InstanceSlots = 145,
    SetInstanceWrapper = 146,
    SetInstanceSlots = 147,
    FuncallableInstancep = 148,
    SetFuncallableInstanceFunction = 149,
    HandlerMarks = 150,
    UnwindToHandler = 151,
    Signalled = 152,
    FunctionName = 153,
    Frames = 154,
    UnwindToFrame = 155,
    ReadConsole = 156,
    FrameLocals = 157,
    MakeProcess = 158,
    CurrentProcess = 159,
    ProcessYield = 160,
    ProcessTerminate = 161,
    ProcessState = 162,
    ProcessResult = 163,
    AllProcesses = 164,
    ProcessData = 165,
    SetProcessData = 166,
    ProcessMainP = 167,
    CompareAndSwapSvref = 168,
    Display = 169,
    AwaitEvent = 170,
    FrameCallee = 171,
}

impl Prim {
    /// The primitive with this index, or `None`.
    ///
    /// Read out of the table rather than written out again as fifty match
    /// arms, which is what this was first: a second copy of the same
    /// correspondence, of exactly the kind #30 exists to remove.
    /// `the_table_is_in_enum_order` is what makes the lookup sound.
    pub fn from_u32(index: u32) -> Option<Prim> {
        TABLE.get(index as usize).map(|e| e.0)
    }
}

/// How many arguments a primitive takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arity {
    Exactly(usize),
    AtLeast(usize),
}

/// Every primitive's name, in table order: for a host or a test that wants
/// to try them all (the fuzz behind *no input may panic the machine*, #88).
pub fn names() -> impl Iterator<Item = &'static str> {
    TABLE.iter().map(|entry| entry.1)
}

/// The table. **Order is part of the image format** — append, never reorder.
const TABLE: &[(Prim, &str, Arity)] = &[
    (Prim::Add, "+", Arity::AtLeast(0)),
    (Prim::Sub, "-", Arity::AtLeast(1)),
    (Prim::Mul, "*", Arity::AtLeast(0)),
    (Prim::NumEq, "=", Arity::AtLeast(1)),
    (Prim::Lt, "<", Arity::AtLeast(1)),
    (Prim::Gt, ">", Arity::AtLeast(1)),
    (Prim::Cons, "CONS", Arity::Exactly(2)),
    (Prim::Car, "CAR", Arity::Exactly(1)),
    (Prim::Cdr, "CDR", Arity::Exactly(1)),
    (Prim::Eq, "EQ", Arity::Exactly(2)),
    (Prim::Null, "NULL", Arity::Exactly(1)),
    (Prim::Consp, "CONSP", Arity::Exactly(1)),
    (Prim::Symbolp, "SYMBOLP", Arity::Exactly(1)),
    // Handled by the evaluator, not by `call` below: applying a function is
    // the machine's job, not a primitive's. Present here so it has a name and
    // a function cell like everything else.
    (Prim::Funcall, "FUNCALL", Arity::AtLeast(1)),
    // Allocates one cons per argument, so its cost grows with the *call*
    // rather than with the data. Rule 9 is satisfied because a written call has
    // a bounded number of arguments — but `apply` would break that, and when
    // `apply` arrives this needs looking at again.
    (Prim::List, "LIST", Arity::AtLeast(0)),
    // Needed by backquote: `(,@xs . rest) expands into an APPEND.
    //
    // Copies all but its last argument, so its cost grows with the *data*
    // rather than the call — the first primitive here for which rule 9 is a
    // live question. It is bounded by what is already in the heap, which is
    // bounded by the heap, so a single call cannot run away; but a long append
    // is a long step, and when slices start mattering this is the one to chunk.
    (Prim::Append, "APPEND", Arity::AtLeast(0)),
    // Macros need names nothing else can have, or an expansion that binds a
    // temporary captures whatever the caller happened to call it.
    (Prim::Gensym, "GENSYM", Arity::AtLeast(0)),
    // The one primitive the prelude could not do without: it is how `defun`,
    // written in Lisp, puts a function where the evaluator will find it.
    //
    // Common Lisp spells this `(setf (symbol-function 'name) f)`. Quokka has
    // no `setf` yet, and `setf` is itself a macro that needs a writer to
    // expand into — so the writer comes first and the sugar comes later. The
    // `%` marks it as Quokka's own rather than Common Lisp's, which is what
    // rule 6 asks for in the absence of packages.
    (
        Prim::SetSymbolFunction,
        "%SET-SYMBOL-FUNCTION",
        Arity::Exactly(2),
    ),
    // Reading the cell back, which is standard and which `defun` does not
    // need — but a prelude you cannot inspect is a prelude you debug blind.
    (Prim::SymbolFunction, "SYMBOL-FUNCTION", Arity::Exactly(1)),
    (Prim::Keywordp, "KEYWORDP", Arity::Exactly(1)),
    // Here to make a claim checkable. A keyword's name has no colon in it —
    // `(symbol-name :foo)` is "FOO" — and that is exactly what interning
    // `:foo` under the name ":FOO" would have got wrong, silently, until
    // somebody exposed this function. So it is exposed.
    (Prim::SymbolName, "SYMBOL-NAME", Arity::Exactly(1)),
    // Intercepted by `crate::eval`, like FUNCALL: calling a function is the
    // machine's job, not a primitive's.
    //
    // Rule 9: the fixed arguments are copied, and a written call has a bounded
    // number of them. The spread list is *shared*, not copied, so apply itself
    // is cheap however long it is — but the call it makes then binds that many
    // parameters, which is the unbounded part and lives in the binder.
    (Prim::Apply, "APPLY", Arity::AtLeast(2)),
    // The macro expander stored in a symbol's function cell, or nil.
    //
    // Named with a `%` because it is **not** Common Lisp's `macro-function`:
    // that returns a function of two arguments, the form and an environment,
    // and this returns the macro's own closure, which takes the argument forms
    // directly. `macroexpand-1` is built on it in the prelude and does obey the
    // standard. Giving the CL name to something with a different contract
    // would be worse than having no CL name at all.
    (Prim::MacroFunction, "%MACRO-FUNCTION", Arity::Exactly(1)),
    // ---- vectors and strings ------------------------------------------
    //
    // A vector's slots are `Value`s and a string's are raw character codes,
    // which is the distinction `ObjectType::slots_are_values` draws and the
    // collector depends on. Both answer `aref` and `length`, because Common
    // Lisp calls both of them vectors and the differential tests will hold us
    // to that.
    //
    // Rule 9: allocating a vector of n elements writes n words, so the cost
    // grows with an *argument* rather than with the call — the first primitive
    // where a single call can be arbitrarily long. Bounded by the heap, which
    // stops it, but a `(make-array 10000000)` is one long step. The same note
    // as APPEND, and the same answer: chunk it when slices start to matter.
    (Prim::MakeVector, "%MAKE-VECTOR", Arity::Exactly(2)),
    (Prim::Vector, "VECTOR", Arity::AtLeast(0)),
    // Any array, one subscript per dimension (#67, `crate::array`).
    (Prim::Aref, "AREF", Arity::AtLeast(1)),
    // Common Lisp spells this `(setf (aref v i) x)`. `setf` is tier 1, and it
    // is a macro that has to expand into something; this is the something.
    // The subscripts, then the value.
    (Prim::SetAref, "%SET-AREF", Arity::AtLeast(2)),
    (Prim::SequenceLength, "%SEQUENCE-LENGTH", Arity::Exactly(1)),
    (Prim::Vectorp, "VECTORP", Arity::Exactly(1)),
    (Prim::Stringp, "STRINGP", Arity::Exactly(1)),
    (Prim::Characterp, "CHARACTERP", Arity::Exactly(1)),
    // ---- hash tables ---------------------------------------------------
    //
    // Keyed by content, never by address, because the collector moves things.
    // See `crate::hash` and issue #22 for what that costs.
    (Prim::MakeHashTable, "%MAKE-HASH-TABLE", Arity::Exactly(1)),
    (Prim::Gethash, "%GETHASH", Arity::Exactly(3)),
    (Prim::SetGethash, "%SET-GETHASH", Arity::Exactly(3)),
    (Prim::Remhash, "REMHASH", Arity::Exactly(2)),
    (Prim::HashTableCount, "HASH-TABLE-COUNT", Arity::Exactly(1)),
    (Prim::HashTableP, "HASH-TABLE-P", Arity::Exactly(1)),
    // `maphash` takes a function, and calling one is the evaluator's job. So
    // the primitive hands back the entries and the prelude does the calling.
    (
        Prim::HashTableEntries,
        "%HASH-TABLE-ENTRIES",
        Arity::Exactly(1),
    ),
    // The two destructive list operations, which `setf` of `car` and `cdr`
    // expand into. They return the cons, as Common Lisp says — `setf` returns
    // the value, which is why its expansion wraps them.
    (Prim::Rplaca, "RPLACA", Arity::Exactly(2)),
    (Prim::Rplacd, "RPLACD", Arity::Exactly(2)),
    // ---- conditions ----------------------------------------------------
    //
    // Intercepted by `crate::eval`, like FUNCALL and APPLY: signalling means
    // unwinding the frame stack, which is the machine's business.
    (Prim::Error, "ERROR", Arity::AtLeast(1)),
    // Common Lisp reads a condition through its class — `type-of`, and
    // readers like `simple-condition-format-control` — all of which need the
    // class system. These are the `%` stand-ins until phase 3 has one.
    (Prim::ConditionType, "%CONDITION-TYPE", Arity::Exactly(1)),
    (
        Prim::ConditionMessage,
        "%CONDITION-MESSAGE",
        Arity::Exactly(1),
    ),
    (Prim::Conditionp, "%CONDITIONP", Arity::Exactly(1)),
    // ---- building names --------------------------------------------------
    //
    // `defstruct` makes `POINT-X` out of `POINT` and `X`, which means making
    // a string and interning it. Common Lisp spells these `intern`,
    // `concatenate` and `coerce`; each of those takes arguments Quokka has no
    // way to express yet (a package, a sequence type), so these are the
    // narrow versions under `%` names until the real ones can exist.
    (Prim::Intern, "%INTERN", Arity::Exactly(1)),
    (Prim::StringAppend, "%STRING-APPEND", Arity::Exactly(2)),
    (Prim::VectorToList, "%VECTOR-TO-LIST", Arity::Exactly(1)),
    // ---- multiple values -------------------------------------------------
    //
    // Intercepted by `crate::eval`, like FUNCALL, APPLY and ERROR: how many
    // values a form returns is machine state, and a primitive returns one.
    (Prim::Values, "VALUES", Arity::AtLeast(0)),
    (Prim::ValuesList, "VALUES-LIST", Arity::Exactly(1)),
    // ---- strings you may write to ----------------------------------------
    //
    // A source literal is a `ConstantString` and refuses `%set-aref`, so
    // without this there would be no writable string in the system at all.
    // The prelude's `copy-seq` and `make-string` are built on it.
    //
    // Rule 9: n characters is n words written, the same unbounded-by-argument
    // shape as `%MAKE-VECTOR`, and the same answer — chunk it when slices
    // start to matter.
    (Prim::MakeString, "%MAKE-STRING", Arity::Exactly(2)),
    // ---- the compiler's one door ------------------------------------------
    //
    // The Lisp compiler emits instructions as lists and this turns them into
    // a function: labels resolved, operands encoded, the result verified and
    // its max-stack computed. See `crate::code::assemble` for why the
    // encoding is not written in Lisp.
    //
    // Rule 9: unbounded in its argument, since the cost grows with the length
    // of the program. The same note as %MAKE-VECTOR, and more defensible —
    // this runs once per function compiled, not once per call.
    (Prim::Assemble, "%ASSEMBLE", Arity::Exactly(4)),
    // Appended rather than filed next to the other predicates, because a row's
    // position *is* its discriminant and an image stores that number. Adding
    // at the end is a format addition; moving CHARACTERP down one to make room
    // would be a breaking change, and `the_table_is_in_enum_order` would say so.
    //
    // The compiler wants it, to ask whether a constant fits in a FIX
    // instruction. Common Lisp's name and meaning; there was no numeric
    // predicate at all until now, which nothing in the prelude had needed.
    (Prim::Integerp, "INTEGERP", Arity::Exactly(1)),
    // A compiled function, rendered for a person: instructions with their
    // offsets, the constants their operands name, where each jump lands, and
    // any nested function indented beneath the CLOSURE that makes it.
    //
    // A `%` name because it is not Common Lisp's `disassemble`, which prints
    // to a stream and returns nil. There is no output primitive yet, so this
    // returns the text and the REPL prints it. When there are streams it can
    // become the real one; chapter 22 is the convention.
    (Prim::Disassemble, "%DISASSEMBLE", Arity::Exactly(1)),
    // `(params body)` in, `(simple-params rewritten-body names)` out.
    //
    // The compiler needs what `lambda_list::rewrite` already does for the
    // interpreter: default forms turned into a `let*` around the body, so
    // that binding never has to evaluate anything. Writing that again in Lisp
    // would be two implementations of `&key` that must agree, which is #30's
    // lesson and #28's boundary — the parts with no judgement in them stay
    // here.
    //
    // The third answer is the names the simple list binds, in the order the
    // binder binds them, because the compiler's scope must match the
    // machine's slot for slot and working it out twice is a way to disagree.
    (
        Prim::RewriteLambdaList,
        "%REWRITE-LAMBDA-LIST",
        Arity::Exactly(2),
    ),
    // The first primitive that needs the outside world, and so the first that
    // does not return: the VM intercepts it, leaves a request in the heap and
    // blocks until the host answers (#35, `crate::host`). A `%` name because
    // it is not Common Lisp — `load` will be built on it, in Lisp, once there
    // is `read-from-string` to build it with.
    (Prim::ReadSource, "%READ-SOURCE", Arity::Exactly(1)),
    // Does the evaluator treat this symbol as a special operator? Read from
    // the symbol's `SPECIAL` slot, so it is the interpreter's own answer and
    // cannot drift from it.
    //
    // For the compiler, which used to compile any head it did not recognise
    // as a call — so a special operator it had no case for became a call to
    // a function that does not exist, and `handler-case` went unnoticed for a
    // week that way (#36). Now that is a compile error naming the operator.
    //
    // A `%` name, not Common Lisp's `special-operator-p`: Quokka's answer
    // differs from SBCL's — `lambda` and `defmacro` are operators here and
    // macros there, and `%handler-case` exists only here — and rule 6 says a
    // CL name must give SBCL's answer.
    (
        Prim::SpecialOperatorP,
        "%SPECIAL-OPERATOR-P",
        Arity::Exactly(1),
    ),
    // `%rewrite-lambda-list` for a macro's parameters, which may destructure
    // — `((a b) c)` — and may be dotted. The same rewrite the interpreter's
    // `defmacro` does, so a compiled macro and an interpreted one take their
    // arguments apart identically (#33).
    (
        Prim::RewriteMacroLambdaList,
        "%REWRITE-MACRO-LAMBDA-LIST",
        Arity::AtLeast(2),
    ),
    // A closure, or a bare compiled function, as a **macro**: the same three
    // slots, a different type — exactly what the interpreter's `defmacro`
    // makes, with a compiled body. What a compiled `defmacro` stores in the
    // function cell, and what `macrolet` holds for its body's extent (#33).
    (Prim::MakeMacro, "%MAKE-MACRO", Arity::Exactly(1)),
    // A symbol's value cell, named at run time — which no instruction can do,
    // because `GREF` and `GSET` name theirs in the code. Common Lisp's own
    // functions, tier 1, and what `defvar` needs to leave a bound variable
    // alone (#40).
    (Prim::Boundp, "BOUNDP", Arity::Exactly(1)),
    (Prim::SymbolValue, "SYMBOL-VALUE", Arity::Exactly(1)),
    (Prim::Set, "SET", Arity::Exactly(2)),
    // Packages are Lisp objects in the tier-1 library (#41, #43); these are
    // the few things about symbols the library cannot do itself. A symbol's
    // home, read and written — the library adopts the system's symbols into
    // packages by setting it. The Rust system table as a list, and a look-up
    // in it that never creates. `make-symbol`, Common Lisp's own. And a
    // keyword interned the Rust way, so that a keyword made by either side is
    // one object.
    (Prim::SymbolHome, "%SYMBOL-HOME", Arity::Exactly(1)),
    (Prim::SetSymbolHome, "%SET-SYMBOL-HOME", Arity::Exactly(2)),
    (Prim::SystemSymbols, "%SYSTEM-SYMBOLS", Arity::Exactly(0)),
    (
        Prim::FindSystemSymbol,
        "%FIND-SYSTEM-SYMBOL",
        Arity::Exactly(2),
    ),
    (Prim::MakeSymbol, "MAKE-SYMBOL", Arity::Exactly(1)),
    (Prim::InternKeyword, "%INTERN-KEYWORD", Arity::Exactly(1)),
    // Step C (#45), for the reader in Lisp. Characters by number, and the two
    // questions that need Unicode's tables — which Rust's standard library
    // already carries, where Lisp would carry kilobytes of them in every
    // image. And a constant copy of a string, so that a literal the Lisp
    // reader reads is immutable (#26) and packed (#44) like one the Rust
    // reader reads.
    (Prim::CharCode, "CHAR-CODE", Arity::Exactly(1)),
    (Prim::CodeChar, "CODE-CHAR", Arity::Exactly(1)),
    (Prim::CharUpcase, "CHAR-UPCASE", Arity::Exactly(1)),
    (Prim::CharDowncase, "CHAR-DOWNCASE", Arity::Exactly(1)),
    (Prim::AlphaCharP, "ALPHA-CHAR-P", Arity::Exactly(1)),
    (Prim::ConstantString, "%CONSTANT-STRING", Arity::Exactly(1)),
    // Step G (#46), for the printer in Lisp. Console output, which is a
    // request to the host like `%read-source` and intercepted by the VM the
    // same way. Integer division, which printing a number in a base cannot do
    // without; `truncate`, `floor` and `mod` are Lisp over these two. And the
    // Rust printer's text for an object Lisp has no way to describe — a
    // function, a condition — so it reads the same whichever printer runs.
    (Prim::WriteConsole, "%WRITE-CONSOLE", Arity::Exactly(1)),
    (Prim::Quotient, "%QUOTIENT", Arity::Exactly(2)),
    (Prim::Rem, "REM", Arity::Exactly(2)),
    (Prim::Printed, "%PRINTED", Arity::Exactly(1)),
    // Step I (#53), for `progv`: a name given fewer values than names is
    // bound and then made to have no value (CLHS). The unbound marker is a
    // Rust value Lisp must never hold, so only Rust can put it in a symbol.
    (Prim::Makunbound, "MAKUNBOUND", Arity::Exactly(1)),
    // Step L (#16): bignums. The representation and what is bounded by it —
    // making one, its limbs, normalising, comparing (`crate::bignum`); the
    // arithmetic is Lisp. And the bitwise operations on two fixnums, which
    // Lisp cannot do without them, and which the bignum code uses on limbs.
    (Prim::MakeBignum, "%MAKE-BIGNUM", Arity::Exactly(2)),
    (Prim::BignumLength, "%BIGNUM-LENGTH", Arity::Exactly(1)),
    (
        Prim::BignumNegativeP,
        "%BIGNUM-NEGATIVE-P",
        Arity::Exactly(1),
    ),
    (Prim::BignumLimb, "%BIGNUM-LIMB", Arity::Exactly(2)),
    (Prim::SetBignumLimb, "%SET-BIGNUM-LIMB", Arity::Exactly(3)),
    (
        Prim::BignumNormalize,
        "%BIGNUM-NORMALIZE",
        Arity::Exactly(1),
    ),
    (Prim::Bignump, "%BIGNUMP", Arity::Exactly(1)),
    (Prim::FixnumToBignum, "%FIXNUM-TO-BIGNUM", Arity::Exactly(1)),
    (Prim::IntegerCompare, "%INTEGER-COMPARE", Arity::Exactly(2)),
    (Prim::FixnumLogand, "%FIXNUM-LOGAND", Arity::Exactly(2)),
    (Prim::FixnumLogior, "%FIXNUM-LOGIOR", Arity::Exactly(2)),
    (Prim::FixnumLogxor, "%FIXNUM-LOGXOR", Arity::Exactly(2)),
    // Step M (#17): double floats, `crate::float`. `/`, which arrives with
    // them: a fixnum quotient when the division is exact, a double when one
    // is involved, an error for a ratio (#59). Then what Lisp cannot do
    // without touching the bits — making a double, rounding one, taking one
    // apart and scaling it — the two conversions to and from text (decision
    // 5), and the functions IEEE arithmetic and `libm` provide (decision 7).
    // Each takes a fixnum or a double; Lisp converts a bignum first.
    (Prim::Divide, "/", Arity::AtLeast(1)),
    (Prim::Floatp, "FLOATP", Arity::Exactly(1)),
    (Prim::Float, "%FLOAT", Arity::Exactly(1)),
    (Prim::DoubleEql, "%DOUBLE-EQL", Arity::Exactly(2)),
    (Prim::DoubleRound, "%DOUBLE-ROUND", Arity::Exactly(2)),
    (Prim::DoubleInteger, "%DOUBLE-INTEGER", Arity::Exactly(1)),
    (Prim::DecodeDouble, "%DECODE-DOUBLE", Arity::Exactly(1)),
    (Prim::ScaleDouble, "%SCALE-DOUBLE", Arity::Exactly(2)),
    (Prim::ParseDouble, "%PARSE-DOUBLE", Arity::Exactly(1)),
    (Prim::DoubleDigits, "%DOUBLE-DIGITS", Arity::Exactly(1)),
    (Prim::Sqrt, "%SQRT", Arity::Exactly(1)),
    (Prim::Exp, "%EXP", Arity::Exactly(1)),
    (Prim::Log, "%LOG", Arity::Exactly(1)),
    (Prim::Sin, "%SIN", Arity::Exactly(1)),
    (Prim::Cos, "%COS", Arity::Exactly(1)),
    (Prim::Tan, "%TAN", Arity::Exactly(1)),
    (Prim::Asin, "%ASIN", Arity::Exactly(1)),
    (Prim::Acos, "%ACOS", Arity::Exactly(1)),
    (Prim::Atan, "%ATAN", Arity::Exactly(1)),
    (Prim::Sinh, "%SINH", Arity::Exactly(1)),
    (Prim::Cosh, "%COSH", Arity::Exactly(1)),
    (Prim::Tanh, "%TANH", Arity::Exactly(1)),
    (Prim::Atan2, "%ATAN2", Arity::Exactly(2)),
    (Prim::Pow, "%POW", Arity::Exactly(2)),
    // Step P (#63): a symbol's cells. Its property list, in a slot every
    // symbol has always had and nothing could reach; `get` and `remprop` are
    // Lisp over these two. Whether its function cell is full, emptying it —
    // which means putting back the unbound marker, a value Lisp never holds,
    // as `makunbound` does for the value cell — and what counts as a
    // function, which only Rust can tell from the object's type.
    (Prim::SymbolPlist, "SYMBOL-PLIST", Arity::Exactly(1)),
    (Prim::SetSymbolPlist, "%SET-SYMBOL-PLIST", Arity::Exactly(2)),
    (Prim::Fboundp, "%FBOUNDP", Arity::Exactly(1)),
    (Prim::Fmakunbound, "%FMAKUNBOUND", Arity::Exactly(1)),
    (Prim::Functionp, "FUNCTIONP", Arity::Exactly(1)),
    // ---- arrays (#67) ---------------------------------------------------
    // A header's six slots are Lisp's to fill and read (`arrays.lisp`); Rust
    // only follows them, in `aref`, `%set-aref` and `length`.
    (
        Prim::MakeArrayHeader,
        "%MAKE-ARRAY-HEADER",
        Arity::Exactly(6),
    ),
    (
        Prim::ArrayHeaderSlot,
        "%ARRAY-HEADER-SLOT",
        Arity::Exactly(2),
    ),
    (
        Prim::SetArrayHeaderSlot,
        "%SET-ARRAY-HEADER-SLOT",
        Arity::Exactly(3),
    ),
    (Prim::MakeBitVector, "%MAKE-BIT-VECTOR", Arity::Exactly(1)),
    (Prim::ArrayKind, "%ARRAY-KIND", Arity::Exactly(1)),
    // The inverse hyperbolic functions, from `libm` as the others are, so
    // their digits are SBCL's (#68, decision 2).
    (Prim::Asinh, "%ASINH", Arity::Exactly(1)),
    (Prim::Acosh, "%ACOSH", Arity::Exactly(1)),
    (Prim::Atanh, "%ATANH", Arity::Exactly(1)),
    // What the library needs to compare keys itself (#71): a bucket, and
    // adding or removing an entry once it has searched one.
    (Prim::HashBucket, "%HASH-BUCKET", Arity::Exactly(2)),
    (Prim::HashInsert, "%HASH-INSERT", Arity::Exactly(3)),
    (
        Prim::HashDeleteEntry,
        "%HASH-DELETE-ENTRY",
        Arity::Exactly(2),
    ),
    (Prim::Sxhash, "SXHASH", Arity::Exactly(1)),
    (Prim::HashTableTest, "HASH-TABLE-TEST", Arity::Exactly(1)),
    // A structure (#73): its layout, then its slots, numbered from 0 here.
    // `%structure-layout` of anything else is nil, so it is the predicate
    // too. What a layout says is the library's business (`structures.lisp`).
    (Prim::MakeStructure, "%MAKE-STRUCTURE", Arity::Exactly(2)),
    (
        Prim::StructureLayout,
        "%STRUCTURE-LAYOUT",
        Arity::Exactly(1),
    ),
    (Prim::StructureRef, "%STRUCTURE-REF", Arity::Exactly(2)),
    (
        Prim::SetStructureRef,
        "%SET-STRUCTURE-REF",
        Arity::Exactly(3),
    ),
    (Prim::MakeNilVector, "%MAKE-NIL-VECTOR", Arity::Exactly(1)),
    (Prim::MakeRatio, "%MAKE-RATIO", Arity::Exactly(2)),
    (Prim::Ratiop, "%RATIOP", Arity::Exactly(1)),
    (Prim::RatioNumerator, "%RATIO-NUMERATOR", Arity::Exactly(1)),
    (
        Prim::RatioDenominator,
        "%RATIO-DENOMINATOR",
        Arity::Exactly(1),
    ),
    (Prim::StringMismatch, "%STRING-MISMATCH", Arity::Exactly(7)),
    (
        Prim::AllocateStandardInstance,
        "%ALLOCATE-STANDARD-INSTANCE",
        Arity::Exactly(2),
    ),
    (
        Prim::AllocateFuncallableInstance,
        "%ALLOCATE-FUNCALLABLE-INSTANCE",
        Arity::Exactly(2),
    ),
    (
        Prim::InstanceWrapper,
        "%INSTANCE-WRAPPER",
        Arity::Exactly(1),
    ),
    (Prim::InstanceSlots, "%INSTANCE-SLOTS", Arity::Exactly(1)),
    (
        Prim::SetInstanceWrapper,
        "%SET-INSTANCE-WRAPPER",
        Arity::Exactly(2),
    ),
    (
        Prim::SetInstanceSlots,
        "%SET-INSTANCE-SLOTS",
        Arity::Exactly(2),
    ),
    (
        Prim::FuncallableInstancep,
        "%FUNCALLABLE-INSTANCE-P",
        Arity::Exactly(1),
    ),
    (
        Prim::SetFuncallableInstanceFunction,
        "%SET-FUNCALLABLE-INSTANCE-FUNCTION",
        Arity::Exactly(2),
    ),
    // ---- signalling in Lisp (phase 3 step B, #125) ----------------------
    //
    // `signal` is Lisp (`conditions.lisp`), and needs from the machine what
    // only the machine has: the open `handler-case` records and the
    // bindings of `handler-bind`'s variable, in the order they are on the
    // stack (`vm::handler_marks`); a way to unwind to one chosen record,
    // which only the machine can do and so is intercepted; and a way to
    // stop with an error nothing handled.
    (Prim::HandlerMarks, "%HANDLER-MARKS", Arity::Exactly(2)),
    (
        Prim::UnwindToHandler,
        "%UNWIND-TO-HANDLER",
        Arity::Exactly(2),
    ),
    (Prim::Signalled, "%SIGNALLED", Arity::Exactly(2)),
    // The name a compiled function was assembled under (`%pending-name` in
    // the compiler), so that a function can find its definition (#55).
    (Prim::FunctionName, "%FUNCTION-NAME", Arity::Exactly(1)),
    // ---- frames (phase 3 step D, #130) ----------------------------------
    //
    // Every frame keeps the call that made it, for the debugger: what the
    // stack holds, read in one walk (`vm::frames`); and a way to unwind to
    // one frame and call something in its place, which only the machine can
    // do and so is intercepted.
    (Prim::Frames, "%FRAMES", Arity::AtLeast(0)),
    (Prim::UnwindToFrame, "%UNWIND-TO-FRAME", Arity::Exactly(3)),
    // ---- console input (phase 3 step F, #131) ---------------------------
    //
    // A line from whoever is at the console, for the console input stream
    // (`reader.lisp`): a host request, as `%read-source` is, since a world
    // has no console of its own. The argument is the prompt the host may
    // show; the answer is the line with its newline, or nil at the end.
    (Prim::ReadConsole, "%READ-CONSOLE", Arity::Exactly(1)),
    // A frame's own variables, from its environment, for the debugger's
    // `list-locals` (`vm::frame_locals`).
    (Prim::FrameLocals, "%FRAME-LOCALS", Arity::Exactly(2)),
    // ---- processes (phase 3 step E, #34) --------------------------------
    //
    // Several computations in one world (`crate::process`). Making one,
    // finding the running one and the others, asking how one is or ended,
    // and a slot for the library's own object for it. Yielding and ending
    // one change which process runs, which only the machine can do, and
    // so are intercepted. One atomic operation, compare-and-swap on a
    // simple vector's element, for the library's locks: a primitive is one
    // step, and no process switch falls inside a step.
    (Prim::MakeProcess, "%MAKE-PROCESS", Arity::Exactly(1)),
    (Prim::CurrentProcess, "%CURRENT-PROCESS", Arity::Exactly(0)),
    (Prim::ProcessYield, "%PROCESS-YIELD", Arity::Exactly(0)),
    (
        Prim::ProcessTerminate,
        "%PROCESS-TERMINATE",
        Arity::Exactly(1),
    ),
    (Prim::ProcessState, "%PROCESS-STATE", Arity::Exactly(1)),
    (Prim::ProcessResult, "%PROCESS-RESULT", Arity::Exactly(1)),
    (Prim::AllProcesses, "%ALL-PROCESSES", Arity::Exactly(0)),
    (Prim::ProcessData, "%PROCESS-DATA", Arity::Exactly(1)),
    (Prim::SetProcessData, "%SET-PROCESS-DATA", Arity::Exactly(2)),
    (Prim::ProcessMainP, "%PROCESS-MAIN-P", Arity::Exactly(1)),
    (
        Prim::CompareAndSwapSvref,
        "%COMPARE-AND-SWAP-SVREF",
        Arity::Exactly(4),
    ),
    // ---- the display protocol (phase 4 step C, #137) ---------------------
    //
    // A world shows things on a screen it does not own, so drawing is a host
    // request like a console write: `%display` hands the host a message, an
    // S-expression printed as a string (`docs/display.md`, decision 1), and
    // is answered at once. `%await-event` asks for the next thing the person
    // did, as a string the world reads; the host answers when there is one,
    // which may be much later, or with nil if it will never have one (a host
    // with no screen). While it waits, the machine may be idle (decision 2).
    (Prim::Display, "%DISPLAY", Arity::Exactly(1)),
    (Prim::AwaitEvent, "%AWAIT-EVENT", Arity::Exactly(0)),
    // What the frame N frames out from the caller called, or nil past the
    // outermost (#138): how an output record finds the code that drew it
    // without `%frames`, which conses a list of every frame. Linear in N,
    // and allocates nothing.
    (Prim::FrameCallee, "%FRAME-CALLEE", Arity::Exactly(1)),
];

/// The Lisp function an arithmetic primitive hands over to when its fixnum
/// fast path cannot answer — on overflow, or when an argument is a bignum
/// (#16, decision 1) — and its slot in the symbol cache. `Vm::call_value`
/// makes the call; the fast path never pays for it.
///
/// The numbers are each fallback's place among the symbol cache's fallback
/// slots (`heap::symbol_cache::FALLBACKS` onwards). This match is the whole
/// table: a new fallback takes the next number here and nowhere else.
pub fn fallback(index: u32) -> Option<(usize, &'static str)> {
    let (n, name) = match Prim::from_u32(index)? {
        Prim::Add => (0, "%GENERIC-+"),
        Prim::Sub => (1, "%GENERIC--"),
        Prim::Mul => (2, "%GENERIC-*"),
        Prim::NumEq => (3, "%GENERIC-="),
        Prim::Lt => (4, "%GENERIC-<"),
        Prim::Gt => (5, "%GENERIC->"),
        Prim::Quotient => (6, "%GENERIC-QUOTIENT"),
        Prim::Rem => (7, "%GENERIC-REM"),
        Prim::Divide => (8, "%GENERIC-/"),
        // A key the library compares (#71): `hash::needs_lisp`.
        Prim::Gethash => (9, "%GENERIC-GETHASH"),
        Prim::SetGethash => (10, "%GENERIC-SET-GETHASH"),
        Prim::Remhash => (11, "%GENERIC-REMHASH"),
        _ => return None,
    };
    Some((crate::heap::symbol_cache::FALLBACKS + n, name))
}

/// The primitives the machine intercepts rather than calling.
///
/// Each one does something a primitive cannot: `funcall` and `apply` call a
/// function, `error` unwinds the stack, `values` sets a register. The
/// evaluator and the VM check for these before dispatching.
///
/// These were five hand-counted `pub const`s with a test pinning them, which
/// existed because `APPLY` was once written as 25, was really 21, and the
/// interception silently never fired. `Prim::Apply as u32` cannot be wrong.
pub const fn index(prim: Prim) -> u32 {
    prim as u32
}

/// A Rust truth value as Lisp's.
/// ARGUMENT, if it is a process.
fn a_process<'h>(
    mc: &Mutation<'h>,
    argument: Value<'h>,
    name: &'static str,
) -> Result<Value<'h>, EvalError> {
    if crate::process::is(mc, argument) {
        Ok(argument)
    } else {
        Err(EvalError::WrongType {
            name,
            wanted: "a process",
        })
    }
}

/// `(%make-process function)`: a process that will call FUNCTION with no
/// arguments, through a trampoline of three instructions, the way a host
/// calls a global (`vm::begin_global`), so that the call is a frame like
/// any other and its function returning ends the process.
fn make_process<'h>(mc: &mut Mutation<'h>, function: Value<'h>) -> Result<Value<'h>, EvalError> {
    let callable = crate::closure::is_callable(mc, function)
        || crate::instance::is_funcallable(mc, function)
        || (function.is_object() && mc.header(function).object_type == ObjectType::Function);
    if !callable {
        return Err(EvalError::WrongType {
            name: "%MAKE-PROCESS",
            wanted: "a function",
        });
    }
    let constants = mc.alloc(ObjectType::Vector, 1);
    mc.set_slot(constants, 0, function);
    let mut bytes = Vec::new();
    crate::code::encode(&mut bytes, crate::code::Op::Const, 0, 0);
    crate::code::encode(&mut bytes, crate::code::Op::Call, 0, 0);
    crate::code::encode(&mut bytes, crate::code::Op::Ret, 0, 0);
    let label = mc.intern("PROCESS-START");
    let trampoline = crate::code::make_function(mc, &bytes, constants, Value::NIL, 1, label);
    Ok(crate::process::make(mc, trampoline, 64))
}

fn boolean<'h>(yes: bool) -> Value<'h> {
    if yes {
        Value::T
    } else {
        Value::NIL
    }
}

/// What primitive `index` is called, for printing.
pub fn name_of(index: u32) -> Option<&'static str> {
    TABLE.get(index as usize).map(|e| e.1)
}

/// Put every primitive in its symbol's function cell, so `(+ 1 2)` can find it.
///
/// Called once when a heap is prepared. Interning the names is also what puts
/// them in the obarray, so they survive collection like any other symbol.
pub fn install(mc: &mut Mutation<'_>) {
    // Special operators are marked on their own symbols; see `crate::eval`.
    for (name, code) in crate::special::ALL {
        let symbol = mc.intern(name);
        let value = Value::fixnum(*code).expect("small");
        mc.set_slot(symbol, crate::heap::symbol::SPECIAL, value);
    }
    for (index, (_, name, _)) in TABLE.iter().enumerate() {
        let symbol = mc.intern(name);
        let value = Value::primitive(index as u32);
        mc.set_slot(symbol, symbol::FUNCTION, value);
    }
}

/// Call primitive `index` with `arguments`, a Lisp list.
///
/// What the interpreter uses, because it has collected the arguments into a
/// list already. The bytecode VM has them in a row on its operand stack and
/// calls [`call_slice`] instead, so that a call does not have to build a list
/// only for this function to take it apart again.
pub fn call<'h>(
    mc: &mut Mutation<'h>,
    index: u32,
    arguments: Value<'h>,
) -> Result<Value<'h>, EvalError> {
    let args = to_vec(mc, arguments);
    call_slice(mc, index, &args)
}

/// `(params body)` in, `(simple-params rewritten-body names)` out, for the
/// compiler — the rewrite the interpreter does on the way into a call, done
/// once at compile time instead. See the table entries for why it lives here.
fn rewrite_for_compiler<'h>(
    mc: &mut Mutation<'h>,
    args: &[Value<'h>],
    kind: crate::lambda_list::Kind,
) -> Result<Value<'h>, EvalError> {
    let (params, body) = crate::lambda_list::rewrite(mc, args[0], args[1], kind)?;
    let names = crate::lambda_list::names(mc, params)?;
    let mut name_list = Value::NIL;
    for n in names.iter().rev() {
        name_list = mc.cons(*n, name_list);
    }
    let tail = mc.cons(name_list, Value::NIL);
    let middle = mc.cons(body, tail);
    Ok(mc.cons(params, middle))
}

/// Call primitive `index` with arguments already in a row.
pub fn call_slice<'h>(
    mc: &mut Mutation<'h>,
    index: u32,
    args: &[Value<'h>],
) -> Result<Value<'h>, EvalError> {
    let Some(&(prim, name, arity)) = TABLE.get(index as usize) else {
        return Err(EvalError::NotAFunction(String::from("#<bad primitive>")));
    };

    let got = args.len();
    let ok = match arity {
        Arity::Exactly(n) => got == n,
        Arity::AtLeast(n) => got >= n,
    };
    if !ok {
        let wanted = match arity {
            Arity::Exactly(n) | Arity::AtLeast(n) => n,
        };
        return Err(EvalError::WrongArgumentCount { name, wanted, got });
    }

    // No catch-all, on purpose: a row added to the table without an arm here
    // is a compile error rather than a surprise at run time.
    match prim {
        // The fixnum fast path first, exactly as it was; only if it fails is
        // a double looked for (#17, decision 3).
        Prim::Add => {
            fold(name, args, 0, |a, b| a.checked_add(b)).or_else(|e| doubles(mc, Op::Add, args, e))
        }
        Prim::Mul => {
            fold(name, args, 1, |a, b| a.checked_mul(b)).or_else(|e| doubles(mc, Op::Mul, args, e))
        }
        Prim::Sub => subtract(name, args).or_else(|e| doubles(mc, Op::Sub, args, e)),
        Prim::Divide => float::arithmetic(mc, Op::Div, args).unwrap_or(Err(EvalError::WrongType {
            name,
            wanted: "a number",
        })),
        Prim::NumEq => compare(name, args, |a, b| a == b)
            .or_else(|e| float::compare(mc, args, Ordering::is_eq).unwrap_or(Err(e))),
        Prim::Lt => compare(name, args, |a, b| a < b)
            .or_else(|e| float::compare(mc, args, Ordering::is_lt).unwrap_or(Err(e))),
        Prim::Gt => compare(name, args, |a, b| a > b)
            .or_else(|e| float::compare(mc, args, Ordering::is_gt).unwrap_or(Err(e))),
        Prim::Cons => Ok(mc.cons(args[0], args[1])),
        Prim::Car => list_part(mc, name, args[0], true),
        Prim::Cdr => list_part(mc, name, args[0], false),
        Prim::Eq => Ok(boolean(args[0] == args[1])),
        Prim::Null => Ok(boolean(args[0].is_nil())),
        Prim::Consp => Ok(boolean(args[0].is_cons())),
        Prim::Symbolp => Ok(boolean(mc.is_symbol(args[0]))),
        Prim::Append => {
            if args.is_empty() {
                return Ok(Value::NIL);
            }
            // The last argument is shared, not copied — which is what makes
            // append cheap enough to use in a macro expansion.
            let mut out = args[args.len() - 1];
            for list in args[..args.len() - 1].iter().rev() {
                let (items, cursor) = mc.list_elements(*list).ok_or(circular("APPEND"))?;
                if !cursor.is_nil() {
                    return Err(EvalError::WrongType {
                        name: "APPEND",
                        wanted: "proper lists",
                    });
                }
                for item in items.into_iter().rev() {
                    out = mc.cons(item, out);
                }
            }
            Ok(out)
        }
        Prim::Gensym => Ok(mc.gensym()),
        Prim::SetSymbolFunction => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name: "%SET-SYMBOL-FUNCTION",
                    wanted: "a symbol",
                });
            }
            mc.set_slot(args[0], symbol::FUNCTION, args[1]);
            Ok(args[1])
        }
        Prim::Values | Prim::ValuesList => Err(EvalError::Fault {
            operator: "VALUES",
            detail: "is intercepted by the evaluator and should never be called here",
        }),
        Prim::Error => Err(EvalError::Fault {
            operator: "ERROR",
            detail: "is intercepted by the evaluator and should never be called here",
        }),
        Prim::UnwindToHandler => Err(EvalError::Fault {
            operator: "%UNWIND-TO-HANDLER",
            detail: "is intercepted by the machine and should never be called here",
        }),
        Prim::ProcessYield | Prim::ProcessTerminate => Err(EvalError::Fault {
            operator: name,
            detail: "is intercepted by the machine and should never be called here",
        }),
        Prim::MakeProcess => make_process(mc, args[0]),
        Prim::CurrentProcess => Ok(crate::process::current(mc)),
        Prim::ProcessState => {
            let p = a_process(mc, args[0], name)?;
            let state = match mc.slot(p, crate::process::STATE).as_fixnum() {
                Some(crate::process::FINISHED) => "FINISHED",
                Some(crate::process::TERMINATED) => "TERMINATED",
                Some(crate::process::FAILED) => "FAILED",
                _ if p == mc.register(crate::heap::Register::Process) => "RUNNING",
                _ if crate::process::runnable(mc, p) => "RUNNABLE",
                _ => "WAITING",
            };
            Ok(mc.intern_keyword(state))
        }
        Prim::ProcessResult => {
            let p = a_process(mc, args[0], name)?;
            Ok(mc.slot(p, crate::process::RESULT))
        }
        Prim::AllProcesses => {
            crate::process::current(mc);
            let ring = crate::process::ring(mc);
            let mut list = Value::NIL;
            for p in ring.into_iter().rev() {
                list = mc.cons(p, list);
            }
            Ok(list)
        }
        Prim::ProcessData => {
            let p = a_process(mc, args[0], name)?;
            Ok(mc.slot(p, crate::process::DATA))
        }
        Prim::SetProcessData => {
            let p = a_process(mc, args[0], name)?;
            mc.set_slot(p, crate::process::DATA, args[1]);
            Ok(args[1])
        }
        Prim::ProcessMainP => {
            let p = a_process(mc, args[0], name)?;
            Ok(boolean(mc.slot(p, crate::process::MAIN) != Value::NIL))
        }
        Prim::CompareAndSwapSvref => {
            let (vector, at, old, new) = (args[0], args[1], args[2], args[3]);
            if !(vector.is_object() && mc.header(vector).object_type == ObjectType::Vector) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a simple vector",
                });
            }
            let slots = mc.header(vector).slots;
            let Some(i) = at
                .as_fixnum()
                .and_then(|i| usize::try_from(i).ok())
                .filter(|i| *i < slots)
            else {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "an index of the vector",
                });
            };
            let now = mc.slot(vector, i);
            if now == old {
                mc.set_slot(vector, i, new);
            }
            Ok(now)
        }
        Prim::UnwindToFrame => Err(EvalError::Fault {
            operator: "%UNWIND-TO-FRAME",
            detail: "is intercepted by the machine and should never be called here",
        }),
        Prim::HandlerMarks => crate::vm::handler_marks(mc, args[0], args[1]),
        Prim::Frames => {
            // `(%frames)` for every frame, `(%frames n)` for the innermost N.
            let limit = match args {
                [] => None,
                [n] if n.is_nil() => None,
                [n] => match n.as_fixnum().and_then(|n| usize::try_from(n).ok()) {
                    Some(n) => Some(n),
                    None => {
                        return Err(EvalError::WrongType {
                            name,
                            wanted: "a count of frames, or nil for all of them",
                        })
                    }
                },
                _ => {
                    return Err(EvalError::WrongArgumentCount {
                        name: "%FRAMES",
                        wanted: 1,
                        got: args.len(),
                    })
                }
            };
            crate::vm::frames(mc, limit)
        }
        Prim::FrameCallee => {
            let Some(n) = args[0].as_fixnum().filter(|n| *n >= 0) else {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a non-negative fixnum",
                });
            };
            Ok(crate::vm::frame_callee(mc, n as u64))
        }
        Prim::FrameLocals => crate::vm::frame_locals(mc, args[0], args[1]),
        // A compiled function's name, through a closure to its body, or nil
        // for an interpreted closure or anything that is not a function. The
        // compiler assembles an anonymous function under the name LAMBDA.
        Prim::FunctionName => {
            let mut f = args[0];
            if f.is_object() && mc.header(f).object_type == ObjectType::Closure {
                f = mc.slot(f, crate::closure::BODY);
            }
            if f.is_object() && mc.header(f).object_type == ObjectType::Function {
                let name = mc.slot(f, crate::code::function::NAME);
                Ok(if name == Value::UNBOUND {
                    Value::NIL
                } else {
                    name
                })
            } else {
                Ok(Value::NIL)
            }
        }
        // An error that no handler took: the machine stops, carrying the
        // condition's type and report as `error` once carried them, so a host
        // reports it as it always has (`condition::report_error`).
        Prim::Signalled => {
            let type_name = if mc.is_symbol(args[0]) {
                mc.symbol_name(args[0])
            } else {
                return Err(EvalError::WrongType {
                    name: "%SIGNALLED",
                    wanted: "a symbol",
                });
            };
            if !(args[1].is_object() && mc.header(args[1]).object_type.is_string()) {
                return Err(EvalError::WrongType {
                    name: "%SIGNALLED",
                    wanted: "a string",
                });
            }
            let text = mc.string_to_rust(args[1]);
            Err(EvalError::Signalled(format!("{type_name}: {text}")))
        }
        Prim::Intern => {
            if !(args[0].is_object() && mc.header(args[0]).object_type.is_string()) {
                return Err(EvalError::WrongType {
                    name: "%INTERN",
                    wanted: "a string",
                });
            }
            let text = mc.string_to_rust(args[0]);
            Ok(mc.intern(&text))
        }
        Prim::StringAppend => {
            let mut text = String::new();
            for arg in args.iter().take(2) {
                if !(arg.is_object() && mc.header(*arg).object_type.is_string()) {
                    return Err(EvalError::WrongType {
                        name: "%STRING-APPEND",
                        wanted: "two strings",
                    });
                }
                text.push_str(&mc.string_to_rust(*arg));
            }
            Ok(mc.alloc_string(&text))
        }
        Prim::Assemble => crate::code::assemble::assemble(mc, args[0], args[1], args[2], args[3]),
        Prim::MakeString => {
            let Some(n) = args[0].as_fixnum() else {
                return Err(EvalError::WrongType {
                    name: "MAKE-STRING",
                    wanted: "a fixnum length",
                });
            };
            let Some(fill) = args[1].as_character() else {
                return Err(EvalError::WrongType {
                    name: "MAKE-STRING",
                    wanted: "a character to fill with",
                });
            };
            let n = size(mc, "MAKE-STRING", n, 0)?;
            let s = mc.alloc(ObjectType::String, n);
            for i in 0..n {
                mc.set_raw_slot(s, i, fill as u64);
            }
            Ok(s)
        }
        Prim::VectorToList => {
            if !(args[0].is_object() && mc.header(args[0]).object_type == ObjectType::Vector) {
                return Err(EvalError::WrongType {
                    name: "%VECTOR-TO-LIST",
                    wanted: "a vector",
                });
            }
            let mut out = Value::NIL;
            for i in (0..mc.header(args[0]).slots).rev() {
                let item = mc.slot(args[0], i);
                out = mc.cons(item, out);
            }
            Ok(out)
        }
        Prim::Conditionp => Ok(if crate::condition::is_condition(mc, args[0]) {
            Value::T
        } else {
            Value::NIL
        }),
        Prim::ConditionType => {
            if !crate::condition::is_condition(mc, args[0]) {
                return Err(EvalError::WrongType {
                    name: "%CONDITION-TYPE",
                    wanted: "a condition",
                });
            }
            Ok(mc.slot(args[0], crate::condition::TYPE))
        }
        Prim::ConditionMessage => {
            if !crate::condition::is_condition(mc, args[0]) {
                return Err(EvalError::WrongType {
                    name: "%CONDITION-MESSAGE",
                    wanted: "a condition",
                });
            }
            Ok(mc.slot(args[0], crate::condition::MESSAGE))
        }
        Prim::Rplaca => {
            if !args[0].is_cons() {
                return Err(EvalError::WrongType {
                    name: "RPLACA",
                    wanted: "a cons",
                });
            }
            mc.set_car(args[0], args[1]);
            Ok(args[0])
        }
        Prim::Rplacd => {
            if !args[0].is_cons() {
                return Err(EvalError::WrongType {
                    name: "RPLACD",
                    wanted: "a cons",
                });
            }
            mc.set_cdr(args[0], args[1]);
            Ok(args[0])
        }
        Prim::MakeHashTable => crate::hash::make(mc, args[0]),
        Prim::Gethash => crate::hash::get(mc, args[1], args[0], args[2]),
        Prim::SetGethash => crate::hash::put(mc, args[1], args[0], args[2]),
        Prim::Remhash => Ok(if crate::hash::remove(mc, args[1], args[0])? {
            Value::T
        } else {
            Value::NIL
        }),
        Prim::HashTableCount => crate::hash::count(mc, args[0]),
        Prim::HashTableP => Ok(if crate::hash::is_table(mc, args[0]) {
            Value::T
        } else {
            Value::NIL
        }),
        Prim::HashTableEntries => crate::hash::entries(mc, args[0]),
        Prim::HashBucket => crate::hash::bucket(mc, args[1], args[0]),
        Prim::HashInsert => crate::hash::insert(mc, args[1], args[0], args[2]),
        Prim::HashDeleteEntry => {
            crate::hash::delete_entry(mc, args[1], args[0]).map(|_| Value::NIL)
        }
        Prim::Sxhash => Ok(crate::hash::sxhash(mc, args[0])),
        Prim::HashTableTest => crate::hash::test(mc, args[0]),
        // Rule 9: n slots is n words, as `%MAKE-VECTOR`.
        Prim::MakeStructure => match args[1].as_fixnum() {
            Some(n) if n >= 0 => {
                let s = mc.alloc(ObjectType::Instance, size(mc, name, n, 1)?);
                mc.set_slot(s, 0, args[0]);
                Ok(s)
            }
            _ => Err(EvalError::WrongType {
                name,
                wanted: "a slot count",
            }),
        },
        Prim::StructureLayout => Ok(if is_structure(mc, args[0]) {
            mc.slot(args[0], 0)
        } else {
            Value::NIL
        }),
        Prim::StructureRef | Prim::SetStructureRef => {
            if !is_structure(mc, args[0]) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a structure",
                });
            }
            let length = mc.header(args[0]).slots - 1;
            let index = args[1].as_fixnum().unwrap_or(-1);
            if !(0..length as i64).contains(&index) {
                return Err(EvalError::IndexOutOfBounds { index, length });
            }
            if prim == Prim::StructureRef {
                return Ok(mc.slot(args[0], index as usize + 1));
            }
            mc.set_slot(args[0], index as usize + 1, args[2]);
            Ok(args[2])
        }
        Prim::MakeVector => {
            let Some(n) = args[0].as_fixnum() else {
                return Err(EvalError::WrongType {
                    name: "MAKE-ARRAY",
                    wanted: "a fixnum size",
                });
            };
            let n = size(mc, "MAKE-ARRAY", n, 0)?;
            let v = mc.alloc(ObjectType::Vector, n);
            for i in 0..n {
                mc.set_slot(v, i, args[1]);
            }
            Ok(v)
        }
        Prim::Vector => {
            let v = mc.alloc(ObjectType::Vector, args.len());
            for (i, item) in args.iter().enumerate() {
                mc.set_slot(v, i, *item);
            }
            Ok(v)
        }
        Prim::Aref => crate::array::aref(mc, args),
        Prim::SetAref => crate::array::set_aref(mc, args),
        Prim::SequenceLength => {
            let length = crate::array::length(mc, args[0])?;
            Ok(Value::fixnum(length as i64).expect("a heap object cannot be that long"))
        }
        // Every one-dimensional array is a vector, strings and bit vectors
        // included (CLHS 15.1.2); every one of characters is a string. A
        // literal is a string too: it just cannot be written, which is
        // `%set-aref`'s business and nothing else's.
        Prim::Vectorp => Ok(boolean(crate::array::is_vector(mc, args[0]))),
        Prim::Stringp => Ok(boolean(crate::array::is_string(mc, args[0]))),
        Prim::MakeArrayHeader => Ok(crate::array::make_header(mc, args)),
        Prim::ArrayHeaderSlot => {
            let slot = crate::array::header_slot(mc, args[0], args[1])?;
            Ok(mc.slot(args[0], slot))
        }
        Prim::SetArrayHeaderSlot => {
            let slot = crate::array::header_slot(mc, args[0], args[1])?;
            mc.set_slot(args[0], slot, args[2]);
            Ok(args[2])
        }
        Prim::MakeBitVector => crate::array::make_bits(mc, args[0]),
        Prim::MakeNilVector => crate::array::make_nil_vector(mc, args[0]),
        Prim::MakeRatio => {
            // Only `%ratio` should call this, with a ratio already in lowest
            // terms — but what the slots hold is trusted by everything that
            // reads a ratio, so their types are checked here (#100).
            let integer = |v: Value<'h>| v.is_fixnum() || crate::bignum::is(mc, v);
            if !integer(args[0]) || !integer(args[1]) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "two integers",
                });
            }
            Ok(crate::ratio::make(mc, args[0], args[1]))
        }
        Prim::StringMismatch => Ok(string_mismatch(mc, args)),
        // The two kinds of instance CLOS is built on (#124): see
        // `crate::instance` for why they are Rust. Every one is O(1).
        Prim::AllocateStandardInstance => Ok(crate::instance::make_standard(mc, args[0], args[1])),
        Prim::AllocateFuncallableInstance => {
            Ok(crate::instance::make_funcallable(mc, args[0], args[1]))
        }
        Prim::InstanceWrapper => Ok(crate::instance::wrapper(mc, args[0]).unwrap_or(Value::NIL)),
        Prim::InstanceSlots => crate::instance::slots(mc, args[0]).ok_or(EvalError::WrongType {
            name,
            wanted: "an instance",
        }),
        Prim::SetInstanceWrapper | Prim::SetInstanceSlots => {
            let done = if prim == Prim::SetInstanceWrapper {
                crate::instance::set_wrapper(mc, args[0], args[1])
            } else {
                crate::instance::set_slots(mc, args[0], args[1])
            };
            if done {
                Ok(args[1])
            } else {
                Err(EvalError::WrongType {
                    name,
                    wanted: "an instance",
                })
            }
        }
        Prim::FuncallableInstancep => Ok(boolean(crate::instance::is_funcallable(mc, args[0]))),
        Prim::SetFuncallableInstanceFunction => crate::instance::set_function(mc, args[0], args[1]),
        Prim::Ratiop => Ok(boolean(crate::ratio::is(mc, args[0]))),
        Prim::RatioNumerator | Prim::RatioDenominator => {
            if !crate::ratio::is(mc, args[0]) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a ratio",
                });
            }
            let slot = if prim == Prim::RatioNumerator {
                crate::ratio::NUMERATOR
            } else {
                crate::ratio::DENOMINATOR
            };
            Ok(mc.slot(args[0], slot))
        }
        Prim::ArrayKind => Ok(Value::fixnum(crate::array::kind_code(mc, args[0])).expect("small")),
        Prim::Integerp => Ok(boolean(
            args[0].as_fixnum().is_some() || crate::bignum::is(mc, args[0]),
        )),
        Prim::SpecialOperatorP => Ok(boolean(
            mc.is_symbol(args[0])
                && mc
                    .slot(args[0], crate::heap::symbol::SPECIAL)
                    .as_fixnum()
                    .is_some(),
        )),
        // Reached only from the interpreter: the VM intercepts it first. And
        // refused there on purpose. The interpreter is the bootstrap
        // evaluator — it reads its sources from `include_str!`, builds an
        // image and goes cold (#25) — so giving it I/O would mean writing
        // blocking twice for an evaluator nothing runs after #33. A loud
        // refusal rather than a quiet one, because a rule enforced on one side
        // only is enforced where it happened to be tested.
        Prim::Rem if !(args[0].is_fixnum() && args[1].is_fixnum()) => doubles(
            mc,
            Op::Rem,
            args,
            EvalError::WrongType {
                name,
                wanted: "a number",
            },
        ),
        Prim::Quotient | Prim::Rem => {
            let (n, d) = (fixnum(name, args[0])?, fixnum(name, args[1])?);
            // A `division-by-zero`, as `/` signals: it was a type error, so
            // `(mod 5 0)` got past a handler for the condition CL names
            // (#113).
            if d == 0 {
                return Err(EvalError::Arithmetic {
                    condition: "DIVISION-BY-ZERO",
                    detail: "division by zero",
                });
            }
            // Rust's `/` and `%` truncate towards zero, which is exactly CL's
            // `truncate` and `rem`. The one overflow is the most negative
            // fixnum divided by -1.
            make(if prim == Prim::Quotient {
                n.checked_div(d)
            } else {
                n.checked_rem(d)
            })
        }
        Prim::Printed => Ok(mc.alloc_string(&crate::printer::print(mc, args[0]))),
        Prim::MakeBignum => {
            let limbs = size(mc, name, fixnum(name, args[1])?, 1)? - 1;
            Ok(crate::bignum::make(mc, !args[0].is_nil(), limbs))
        }
        Prim::BignumLength
        | Prim::BignumNegativeP
        | Prim::BignumLimb
        | Prim::SetBignumLimb
        | Prim::BignumNormalize => {
            use crate::bignum;
            let b = args[0];
            if !bignum::is(mc, b) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a bignum",
                });
            }
            let index = |v: Value<'h>| -> Result<usize, EvalError> {
                match v.as_fixnum() {
                    Some(i) if i >= 0 && (i as usize) < bignum::length(mc, b) => Ok(i as usize),
                    _ => Err(EvalError::WrongType {
                        name,
                        wanted: "a limb index inside the bignum",
                    }),
                }
            };
            match prim {
                Prim::BignumLength => make(Some(bignum::length(mc, b) as i64)),
                Prim::BignumNegativeP => Ok(boolean(bignum::negative(mc, b))),
                Prim::BignumLimb => make(Some(bignum::limb(mc, b, index(args[1])?) as i64)),
                Prim::SetBignumLimb => {
                    let at = index(args[1])?;
                    match args[2].as_fixnum() {
                        Some(l) if (0..=bignum::LIMB_MASK as i64).contains(&l) => {
                            bignum::set_limb(mc, b, at, l as u64);
                            Ok(args[2])
                        }
                        _ => Err(EvalError::WrongType {
                            name,
                            wanted: "a limb, from 0 below 2^30",
                        }),
                    }
                }
                _ => Ok(bignum::normalize(mc, b)),
            }
        }
        Prim::Bignump => Ok(boolean(crate::bignum::is(mc, args[0]))),
        Prim::FixnumToBignum => Ok(crate::bignum::from_i64(mc, fixnum(name, args[0])?)),
        Prim::IntegerCompare => match crate::bignum::compare(mc, args[0], args[1]) {
            Some(order) => make(Some(order as i64)),
            None => Err(EvalError::WrongType {
                name,
                wanted: "two integers",
            }),
        },
        Prim::FixnumLogand | Prim::FixnumLogior | Prim::FixnumLogxor => {
            let (a, b) = (fixnum(name, args[0])?, fixnum(name, args[1])?);
            make(Some(match prim {
                Prim::FixnumLogand => a & b,
                Prim::FixnumLogior => a | b,
                _ => a ^ b,
            }))
        }
        Prim::Floatp => Ok(boolean(float::is(mc, args[0]))),
        Prim::Float => float::float(mc, args[0]),
        Prim::DoubleEql => Ok(boolean(
            float::is(mc, args[0])
                && float::is(mc, args[1])
                && mc.raw_slot(args[0], 0) == mc.raw_slot(args[1], 0),
        )),
        Prim::DoubleRound => {
            let x = float::value(mc, name, args[0])?;
            float::make(mc, float::round(x, fixnum(name, args[1])?))
        }
        Prim::DoubleInteger => {
            Ok(float::integer(float::value(mc, name, args[0])?).unwrap_or(Value::NIL))
        }
        Prim::DecodeDouble => {
            let (m, e, s) = float::decode(float::value(mc, name, args[0])?);
            let list = [m, e, s].map(|n| Value::fixnum(n).expect("53 bits fit a fixnum"));
            Ok(list
                .iter()
                .rev()
                .fold(Value::NIL, |tail, v| mc.cons(*v, tail)))
        }
        Prim::ScaleDouble => {
            let x = float::value(mc, name, args[0])?;
            float::make(mc, float::scale(x, fixnum(name, args[1])?))
        }
        Prim::ParseDouble => {
            if !(args[0].is_object() && mc.header(args[0]).object_type.is_string()) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a string",
                });
            }
            let text = mc.string_to_rust(args[0]);
            float::parse(mc, &text)
        }
        Prim::DoubleDigits => {
            let x = float::value(mc, name, args[0])?;
            Ok(mc.alloc_string(&float::digits(x)))
        }
        Prim::Sqrt => float::unary(mc, name, args[0], f64::sqrt),
        Prim::Exp => float::unary(mc, name, args[0], libm::exp),
        Prim::Log => float::unary(mc, name, args[0], libm::log),
        Prim::Sin => float::unary(mc, name, args[0], libm::sin),
        Prim::Cos => float::unary(mc, name, args[0], libm::cos),
        Prim::Tan => float::unary(mc, name, args[0], libm::tan),
        Prim::Asin => float::unary(mc, name, args[0], libm::asin),
        Prim::Acos => float::unary(mc, name, args[0], libm::acos),
        Prim::Atan => float::unary(mc, name, args[0], libm::atan),
        Prim::Sinh => float::unary(mc, name, args[0], libm::sinh),
        Prim::Cosh => float::unary(mc, name, args[0], libm::cosh),
        Prim::Tanh => float::unary(mc, name, args[0], libm::tanh),
        Prim::Asinh => float::unary(mc, name, args[0], libm::asinh),
        Prim::Acosh => float::unary(mc, name, args[0], libm::acosh),
        Prim::Atanh => float::unary(mc, name, args[0], libm::atanh),
        Prim::Atan2 | Prim::Pow => {
            let (y, x) = (
                float::value(mc, name, args[0])?,
                float::value(mc, name, args[1])?,
            );
            let f = if prim == Prim::Pow {
                libm::pow
            } else {
                libm::atan2
            };
            float::make(mc, f(y, x))
        }
        Prim::ReadSource
        | Prim::WriteConsole
        | Prim::ReadConsole
        | Prim::Display
        | Prim::AwaitEvent => Err(EvalError::Fault {
            operator: name,
            detail: "asks the host for something, which only compiled code can do: \
                     the interpreter is the bootstrap evaluator and has no I/O",
        }),
        Prim::RewriteLambdaList => {
            rewrite_for_compiler(mc, args, crate::lambda_list::Kind::Ordinary)
        }
        Prim::SymbolHome | Prim::SetSymbolHome => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a symbol",
                });
            }
            if prim == Prim::SymbolHome {
                return Ok(mc.slot(args[0], symbol::HOME));
            }
            mc.set_slot(args[0], symbol::HOME, args[1]);
            Ok(args[1])
        }
        Prim::SystemSymbols => Ok(mc.system_symbols()),
        Prim::CharCode | Prim::CharUpcase | Prim::CharDowncase | Prim::AlphaCharP => {
            let Some(ch) = args[0].as_character() else {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a character",
                });
            };
            Ok(match prim {
                Prim::CharCode => Value::fixnum(ch as i64).expect("a code fits a fixnum"),
                Prim::AlphaCharP => boolean(ch.is_alphabetic()),
                Prim::CharUpcase => Value::character(case_partner(ch, true)),
                _ => Value::character(case_partner(ch, false)),
            })
        }
        Prim::CodeChar => {
            let Some(code) = args[0].as_fixnum() else {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "an integer",
                });
            };
            // Not every number is a character: a surrogate half is not, and
            // CLHS lets `code-char` answer nil for those.
            Ok(u32::try_from(code)
                .ok()
                .and_then(char::from_u32)
                .map_or(Value::NIL, Value::character))
        }
        Prim::FindSystemSymbol | Prim::MakeSymbol | Prim::InternKeyword | Prim::ConstantString => {
            let text = if args[0].is_object() && mc.header(args[0]).object_type.is_string() {
                mc.string_to_rust(args[0])
            } else if let Some((false, text)) = crate::array::equal_text(mc, args[0]) {
                // A string that is not simple — a fill pointer, or displaced:
                // its active characters, as `equal` reads them (#79).
                text
            } else {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a string",
                });
            };
            Ok(match prim {
                Prim::FindSystemSymbol => mc
                    .find_system_symbol(&text, args[1] != Value::NIL)
                    .unwrap_or(Value::NIL),
                Prim::MakeSymbol => mc.new_symbol(&text),
                Prim::ConstantString => mc.alloc_constant_string(&text),
                _ => mc.intern_keyword(&text),
            })
        }
        Prim::Boundp | Prim::SymbolValue | Prim::Set | Prim::Makunbound => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a symbol",
                });
            }
            // `nil`, `t` and a keyword are constants: `(set nil 5)` used to
            // succeed, and after it nothing in the world meant what it said
            // (#113). The compiler refuses to bind or assign one by name;
            // this is the same rule for a name computed at run time.
            if matches!(prim, Prim::Set | Prim::Makunbound)
                && (args[0].is_nil() || args[0] == Value::T || mc.is_keyword(args[0]))
            {
                return Err(EvalError::WrongType {
                    name,
                    wanted:
                        "a symbol that is not a constant: nil, t and keywords keep their values",
                });
            }
            let value = mc.slot(args[0], symbol::VALUE);
            match prim {
                Prim::Boundp => Ok(boolean(value != Value::UNBOUND)),
                Prim::SymbolValue if value == Value::UNBOUND => {
                    let what = mc.string_to_rust(mc.slot(args[0], symbol::NAME));
                    Err(EvalError::UnboundVariable(what))
                }
                Prim::SymbolValue => Ok(value),
                Prim::Makunbound => {
                    mc.set_slot(args[0], symbol::VALUE, Value::UNBOUND);
                    Ok(args[0])
                }
                _ => {
                    mc.set_slot(args[0], symbol::VALUE, args[1]);
                    Ok(args[1])
                }
            }
        }
        // A third argument, true, asks for `destructuring-bind`'s rewrite.
        Prim::RewriteMacroLambdaList => {
            let kind = match args.get(2) {
                Some(d) if !d.is_nil() => crate::lambda_list::Kind::Destructuring,
                _ => crate::lambda_list::Kind::Macro,
            };
            rewrite_for_compiler(mc, args, kind)
        }
        Prim::MakeMacro => {
            use crate::closure;
            let f = args[0];
            if closure::is(mc, f) {
                let params = mc.slot(f, closure::PARAMS);
                let body = mc.slot(f, closure::BODY);
                let env = mc.slot(f, closure::ENV);
                return Ok(closure::make_macro(mc, params, body, env));
            }
            if f.is_object() && mc.header(f).object_type == ObjectType::Function {
                // A function compiled on its own, with nothing to capture:
                // what `macrolet` makes at compile time.
                let params = mc.slot(f, crate::code::function::PARAMS);
                return Ok(closure::make_macro(mc, params, f, Value::NIL));
            }
            Err(EvalError::WrongType {
                name: "%MAKE-MACRO",
                wanted: "a closure or a compiled function",
            })
        }
        Prim::Disassemble => {
            // A closure or the function inside one, so that both
            // `(%disassemble (compile-form ...))` and a closure work.
            let mut f = args[0];
            if f.is_object() && mc.header(f).object_type == ObjectType::Closure {
                f = mc.slot(f, crate::closure::BODY);
            }
            if !(f.is_object() && mc.header(f).object_type == ObjectType::Function) {
                return Err(EvalError::WrongType {
                    name: "%DISASSEMBLE",
                    wanted: "a compiled function",
                });
            }
            let text = crate::code::disassemble_function(mc, f);
            Ok(mc.alloc_string(&text))
        }
        Prim::Characterp => Ok(if args[0].as_character().is_some() {
            Value::T
        } else {
            Value::NIL
        }),
        // `apply` and `funcall` both call a function, which is the machine's
        // job and not a primitive's, so both are intercepted before they get
        // here. `funcall` had no arm at all until the catch-all went: it
        // compiled, and a call that somehow reached it would have reported
        // that FUNCALL is not a function. The compiler found this the moment
        // there was nothing to fall through to, which is the whole argument
        // of #30 in one line of build output.
        Prim::Apply | Prim::Funcall => Err(EvalError::Fault {
            operator: "APPLY or FUNCALL",
            detail: "is intercepted by the evaluator and should never be called here",
        }),
        Prim::MacroFunction => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name: "%MACRO-FUNCTION",
                    wanted: "a symbol",
                });
            }
            let f = mc.slot(args[0], symbol::FUNCTION);
            if f != Value::UNBOUND && crate::closure::is_macro(mc, f) {
                Ok(f)
            } else {
                Ok(Value::NIL)
            }
        }
        Prim::Keywordp => Ok(if mc.is_keyword(args[0]) {
            Value::T
        } else {
            Value::NIL
        }),
        Prim::SymbolName => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name: "SYMBOL-NAME",
                    wanted: "a symbol",
                });
            }
            // The symbol's own name, not a copy (#44). It is a constant string,
            // so a program that tries to modify it is refused rather than
            // renaming the symbol behind the obarray's back — CLHS leaves that
            // undefined, and refusing is the safe reading of undefined.
            Ok(mc.slot(args[0], symbol::NAME))
        }
        Prim::SymbolPlist | Prim::SetSymbolPlist | Prim::Fboundp | Prim::Fmakunbound => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a symbol",
                });
            }
            match prim {
                Prim::SymbolPlist => Ok(mc.slot(args[0], symbol::PLIST)),
                Prim::SetSymbolPlist => {
                    mc.set_slot(args[0], symbol::PLIST, args[1]);
                    Ok(args[1])
                }
                // A function, a macro or a special operator (CLHS `fboundp`).
                Prim::Fboundp => Ok(boolean(
                    mc.slot(args[0], symbol::FUNCTION) != Value::UNBOUND
                        || mc.slot(args[0], symbol::SPECIAL).as_fixnum().is_some(),
                )),
                _ => {
                    mc.set_slot(args[0], symbol::FUNCTION, Value::UNBOUND);
                    Ok(args[0])
                }
            }
        }
        // A closure, a compiled function or a primitive; not a macro, and not
        // a symbol, though a symbol can designate a function (#62).
        // A funcallable instance is a function too (#124).
        Prim::Functionp => Ok(boolean(
            crate::instance::is_plain_function(mc, args[0])
                || crate::instance::is_funcallable(mc, args[0]),
        )),
        Prim::SymbolFunction => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name: "SYMBOL-FUNCTION",
                    wanted: "a symbol",
                });
            }
            let f = mc.slot(args[0], symbol::FUNCTION);
            if f == Value::UNBOUND {
                return Err(EvalError::UndefinedFunction(mc.symbol_name(args[0])));
            }
            Ok(f)
        }
        Prim::List => {
            let mut out = Value::NIL;
            for v in args.iter().rev() {
                out = mc.cons(*v, out);
            }
            Ok(out)
        }
    }
}

/// `nil` is false and the only false thing; `t` is the canonical true.
/// The other-case partner of `ch`, or `ch` itself if it has none.
///
/// CLHS 13.1.4.3: a character has case only if it and its partner map to each
/// other one to one. Unicode upcases `ß` to "SS", two characters, so in Lisp
/// `ß` has no case; `ſ` upcases to `S`, which downcases to `s`, so neither has
/// `ſ` as a partner.
pub(crate) fn case_partner(ch: char, up: bool) -> char {
    fn one(mut it: impl Iterator<Item = char>) -> Option<char> {
        let c = it.next()?;
        it.next().is_none().then_some(c)
    }
    let other = if up {
        one(ch.to_uppercase())
    } else {
        one(ch.to_lowercase())
    };
    match other {
        Some(o) if up && one(o.to_lowercase()) == Some(ch) => o,
        Some(o) if !up && one(o.to_uppercase()) == Some(ch) => o,
        _ => ch,
    }
}

fn is_structure<'h>(mc: &Mutation<'h>, v: Value<'h>) -> bool {
    v.is_object() && mc.header(v).object_type == ObjectType::Instance
}

/// A list that loops where a primitive wanted one that ends (#92).
pub(crate) fn circular(name: &'static str) -> EvalError {
    EvalError::WrongType {
        name,
        wanted: "a list that ends, and this one is circular",
    }
}

/// A count a program supplied, plus `extra` slots, as a size the heap can
/// hold — checked *before* anything is allocated (#91, rule 9).
///
/// `try_from` and not `as`: on wasm32 a cast would quietly keep the low 32
/// bits of a fixnum, and `(make-array 4294967301)` would make five slots.
pub(crate) fn size(
    mc: &Mutation<'_>,
    name: &'static str,
    n: i64,
    extra: usize,
) -> Result<usize, EvalError> {
    if n < 0 {
        return Err(EvalError::WrongType {
            name,
            wanted: "a size that is not negative",
        });
    }
    usize::try_from(n)
        .ok()
        .and_then(|n| n.checked_add(extra))
        .filter(|&slots| mc.fits(slots))
        .ok_or(EvalError::HeapExhausted)
}

fn fixnum(name: &'static str, v: Value<'_>) -> Result<i64, EvalError> {
    v.as_fixnum().ok_or(EvalError::WrongType {
        name,
        wanted: "a number",
    })
}

/// `-` on fixnums: negation of one, or the first less the rest.
fn subtract<'h>(name: &'static str, args: &[Value<'h>]) -> Result<Value<'h>, EvalError> {
    let first = fixnum(name, args[0])?;
    if args.len() == 1 {
        return make(first.checked_neg());
    }
    let mut acc = first;
    for v in &args[1..] {
        acc = acc
            .checked_sub(fixnum(name, *v)?)
            .ok_or(EvalError::IntegerOverflow)?;
    }
    make(Some(acc))
}

/// An arithmetic primitive's second try, once its fixnum path has failed
/// with `error`: the same operation with doubles among the arguments, or the
/// error as it was if there are none.
fn doubles<'h>(
    mc: &mut Mutation<'h>,
    op: Op,
    args: &[Value<'h>],
    error: EvalError,
) -> Result<Value<'h>, EvalError> {
    float::arithmetic(mc, op, args).unwrap_or(Err(error))
}

fn make<'h>(n: Option<i64>) -> Result<Value<'h>, EvalError> {
    n.and_then(Value::fixnum).ok_or(EvalError::IntegerOverflow)
}

fn fold<'h>(
    name: &'static str,
    args: &[Value<'h>],
    start: i64,
    op: impl Fn(i64, i64) -> Option<i64>,
) -> Result<Value<'h>, EvalError> {
    let mut acc = start;
    for v in args {
        acc = op(acc, fixnum(name, *v)?).ok_or(EvalError::IntegerOverflow)?;
    }
    make(Some(acc))
}

/// Comparisons chain, as Common Lisp's do: `(< 1 2 3)` is true.
fn compare<'h>(
    name: &'static str,
    args: &[Value<'h>],
    op: impl Fn(i64, i64) -> bool,
) -> Result<Value<'h>, EvalError> {
    for pair in args.windows(2) {
        if !op(fixnum(name, pair[0])?, fixnum(name, pair[1])?) {
            return Ok(Value::NIL);
        }
    }
    Ok(Value::T)
}

/// `(car nil)` and `(cdr nil)` are `nil`, which Common Lisp allows and a great
/// deal of list code relies on.
fn list_part<'h>(
    mc: &Mutation<'h>,
    name: &'static str,
    v: Value<'h>,
    want_car: bool,
) -> Result<Value<'h>, EvalError> {
    if v.is_nil() {
        return Ok(Value::NIL);
    }
    if !v.is_cons() {
        return Err(EvalError::WrongType {
            name,
            wanted: "a list",
        });
    }
    Ok(if want_car { mc.car(v) } else { mc.cdr(v) })
}

fn to_vec<'h>(mc: &Mutation<'h>, list: Value<'h>) -> Vec<Value<'h>> {
    // The evaluator built this list itself, so it ends.
    mc.list_elements(list)
        .map(|(items, _)| items)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "unit/primitives.rs"]
mod tests;

/// `(%string-mismatch a b start1 end1 start2 end2 fold)` (#82): where the two
/// ranges of simple strings A and B first differ, and how — `4i` if they are
/// equal, `4i + 1` if A's is less at index I of A, `4i + 2` if greater —
/// comparing characters by code, or with FOLD by their lower-case codes, as
/// `char-downcase` makes them. Nil for anything else, which the library's
/// own walk handles. The library has checked the bounds.
///
/// All twelve string comparisons are this one walk, and sorting symbols by
/// name makes it hot: in Lisp it cost some 500 steps for two eleven-letter
/// names. It is linear in the strings' length, as `equal` on two strings
/// already is in `hash.rs`.
fn string_mismatch<'h>(mc: &Mutation<'h>, args: &[Value<'h>]) -> Value<'h> {
    let simple = |v: Value<'h>| v.is_object() && mc.header(v).object_type.is_string();
    let index = |v: Value<'h>| v.as_fixnum().and_then(|n| usize::try_from(n).ok());
    let (a, b) = (args[0], args[1]);
    let (Some(s1), Some(e1), Some(s2), Some(e2)) = (
        index(args[2]),
        index(args[3]),
        index(args[4]),
        index(args[5]),
    ) else {
        return Value::NIL;
    };
    if !simple(a) || !simple(b) || e1 > mc.string_length(a) || e2 > mc.string_length(b) {
        return Value::NIL;
    }
    let fold = args[6] != Value::NIL;
    let code = |c: char| if fold { case_partner(c, false) } else { c } as u32;
    let (mut i, mut j) = (s1, s2);
    let order = loop {
        if i >= e1 {
            break if j < e2 { 1 } else { 0 };
        }
        if j >= e2 {
            break 2;
        }
        let (x, y) = (mc.string_char(a, i), mc.string_char(b, j));
        if x != y {
            match code(x).cmp(&code(y)) {
                core::cmp::Ordering::Less => break 1,
                core::cmp::Ordering::Greater => break 2,
                core::cmp::Ordering::Equal => {}
            }
        }
        i += 1;
        j += 1;
    };
    Value::fixnum((i * 4 + order) as i64).unwrap_or(Value::NIL)
}
