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
//! interrupt a primitive once control is inside it. Every one of these is O(1)
//! on its arguments. The first that is not — sorting, bignum multiplication,
//! hashing a long string — needs a plan for being resumable before it is added.
//!
//! # Indices are part of the image format
//!
//! A primitive is an immediate holding an index into this table. An image
//! carries those, so the VM that loads an image must have the same table in the
//! same order. Adding an entry at the end is a format addition; reordering is a
//! breaking change.

use crate::eval::EvalError;
use crate::heap::{symbol, Mutation, ObjectType};
use crate::value::Value;

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
    (Prim::Aref, "AREF", Arity::Exactly(2)),
    // Common Lisp spells this `(setf (aref v i) x)`. `setf` is tier 1, and it
    // is a macro that has to expand into something; this is the something.
    (Prim::SetAref, "%SET-AREF", Arity::Exactly(3)),
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
        Arity::Exactly(2),
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
];

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

/// Is `v` a heap object of this type?
fn boolean<'h>(yes: bool) -> Value<'h> {
    if yes {
        Value::T
    } else {
        Value::NIL
    }
}

fn is_type<'h>(mc: &Mutation<'h>, v: Value<'h>, want: ObjectType) -> Value<'h> {
    if v.is_object() && mc.header(v).object_type == want {
        Value::T
    } else {
        Value::NIL
    }
}

/// How long `v` is, and whether its slots are raw characters.
///
/// Common Lisp calls both a general vector and a string a vector, so `aref`
/// and `length` work on both. The difference is only whether a slot holds a
/// `Value` or a character code — the line `ObjectType::slots_are_values`
/// draws, which the collector also depends on.
fn sequence_shape<'h>(
    mc: &Mutation<'h>,
    v: Value<'h>,
    name: &'static str,
) -> Result<(usize, bool), EvalError> {
    if v.is_object() {
        let header = mc.header(v);
        match header.object_type {
            ObjectType::Vector => return Ok((header.slots, false)),
            t if t.is_string() => return Ok((header.slots, true)),
            _ => {}
        }
    }
    Err(EvalError::WrongType {
        name,
        wanted: "a vector or a string",
    })
}

/// A fixnum index inside `length`, or an error naming both.
fn checked_index(index: Value<'_>, length: usize) -> Result<usize, EvalError> {
    let Some(i) = index.as_fixnum() else {
        return Err(EvalError::WrongType {
            name: "AREF",
            wanted: "a fixnum index",
        });
    };
    if i < 0 || i as usize >= length {
        return Err(EvalError::IndexOutOfBounds { index: i, length });
    }
    Ok(i as usize)
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
    for (name, code) in crate::eval::special::ALL {
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
        Prim::Add => fold(name, args, 0, |a, b| a.checked_add(b)),
        Prim::Mul => fold(name, args, 1, |a, b| a.checked_mul(b)),
        Prim::Sub => {
            let first = fixnum(name, args[0])?;
            if got == 1 {
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
        Prim::NumEq => compare(name, args, |a, b| a == b),
        Prim::Lt => compare(name, args, |a, b| a < b),
        Prim::Gt => compare(name, args, |a, b| a > b),
        Prim::Cons => Ok(mc.cons(args[0], args[1])),
        Prim::Car => list_part(mc, name, args[0], true),
        Prim::Cdr => list_part(mc, name, args[0], false),
        Prim::Eq => Ok(truth(args[0] == args[1])),
        Prim::Null => Ok(truth(args[0].is_nil())),
        Prim::Consp => Ok(truth(args[0].is_cons())),
        Prim::Symbolp => Ok(truth(mc.is_symbol(args[0]))),
        Prim::Append => {
            if args.is_empty() {
                return Ok(Value::NIL);
            }
            // The last argument is shared, not copied — which is what makes
            // append cheap enough to use in a macro expansion.
            let mut out = args[args.len() - 1];
            for list in args[..args.len() - 1].iter().rev() {
                let mut items = Vec::new();
                let mut cursor = *list;
                while cursor.is_cons() {
                    items.push(mc.car(cursor));
                    cursor = mc.cdr(cursor);
                }
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
        Prim::Values | Prim::ValuesList => Err(EvalError::BadSyntax {
            operator: "VALUES",
            detail: "is intercepted by the evaluator and should never be called here",
        }),
        Prim::Error => Err(EvalError::BadSyntax {
            operator: "ERROR",
            detail: "is intercepted by the evaluator and should never be called here",
        }),
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
            if n < 0 {
                return Err(EvalError::WrongType {
                    name: "MAKE-STRING",
                    wanted: "a length of zero or more",
                });
            }
            let s = mc.alloc(ObjectType::String, n as usize);
            for i in 0..n as usize {
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
        Prim::MakeVector => {
            let Some(n) = args[0].as_fixnum() else {
                return Err(EvalError::WrongType {
                    name: "MAKE-ARRAY",
                    wanted: "a fixnum size",
                });
            };
            if n < 0 {
                return Err(EvalError::WrongType {
                    name: "MAKE-ARRAY",
                    wanted: "a size that is not negative",
                });
            }
            let v = mc.alloc(ObjectType::Vector, n as usize);
            for i in 0..n as usize {
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
        Prim::Aref => {
            let (length, is_string) = sequence_shape(mc, args[0], "AREF")?;
            let index = checked_index(args[1], length)?;
            if is_string {
                let code = mc.raw_slot(args[0], index) as u32;
                Ok(Value::character(
                    char::from_u32(code).expect("a string holds characters"),
                ))
            } else {
                Ok(mc.slot(args[0], index))
            }
        }
        Prim::SetAref => {
            // A literal is readable everywhere and writable nowhere. CLHS
            // 3.7.1 leaves modifying one undefined, which is permission to
            // share literals and no help when a program modifies one anyway —
            // before this check, `(setf (aref (greeting) 0) #\J)` edited the
            // source of `greeting` and said nothing. See issue #26.
            if args[0].is_object() && mc.header(args[0]).object_type == ObjectType::ConstantString {
                return Err(EvalError::WrongType {
                    name: "SET-AREF",
                    wanted:
                        "a mutable string — this one is a literal, so copy it with COPY-SEQ first",
                });
            }
            let (length, is_string) = sequence_shape(mc, args[0], "SET-AREF")?;
            let index = checked_index(args[1], length)?;
            if is_string {
                let Some(ch) = args[2].as_character() else {
                    return Err(EvalError::WrongType {
                        name: "SET-AREF",
                        wanted: "a character, to store in a string",
                    });
                };
                mc.set_raw_slot(args[0], index, ch as u64);
            } else {
                mc.set_slot(args[0], index, args[2]);
            }
            Ok(args[2])
        }
        Prim::SequenceLength => {
            let (length, _) = sequence_shape(mc, args[0], "LENGTH")?;
            Ok(Value::fixnum(length as i64).expect("a heap object cannot be that long"))
        }
        Prim::Vectorp => Ok(is_type(mc, args[0], ObjectType::Vector)),
        // Both string types answer yes: a literal is a string, it just
        // cannot be written. The distinction is `%set-aref`'s business and
        // nothing else's.
        Prim::Stringp => Ok(boolean(
            args[0].is_object() && mc.header(args[0]).object_type.is_string(),
        )),
        Prim::Integerp => Ok(truth(args[0].as_fixnum().is_some())),
        Prim::SpecialOperatorP => Ok(truth(
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
        Prim::ReadSource => Err(EvalError::BadSyntax {
            operator: "%READ-SOURCE",
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
        Prim::FindSystemSymbol | Prim::MakeSymbol | Prim::InternKeyword => {
            if !(args[0].is_object() && mc.header(args[0]).object_type.is_string()) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a string",
                });
            }
            let text = mc.string_to_rust(args[0]);
            Ok(match prim {
                Prim::FindSystemSymbol => mc
                    .find_system_symbol(&text, args[1] != Value::NIL)
                    .unwrap_or(Value::NIL),
                Prim::MakeSymbol => mc.new_symbol(&text),
                _ => mc.intern_keyword(&text),
            })
        }
        Prim::Boundp | Prim::SymbolValue | Prim::Set => {
            if !mc.is_symbol(args[0]) {
                return Err(EvalError::WrongType {
                    name,
                    wanted: "a symbol",
                });
            }
            let value = mc.slot(args[0], symbol::VALUE);
            match prim {
                Prim::Boundp => Ok(truth(value != Value::UNBOUND)),
                Prim::SymbolValue if value == Value::UNBOUND => {
                    let what = mc.string_to_rust(mc.slot(args[0], symbol::NAME));
                    Err(EvalError::UnboundVariable(what))
                }
                Prim::SymbolValue => Ok(value),
                _ => {
                    mc.set_slot(args[0], symbol::VALUE, args[1]);
                    Ok(args[1])
                }
            }
        }
        Prim::RewriteMacroLambdaList => {
            rewrite_for_compiler(mc, args, crate::lambda_list::Kind::Macro)
        }
        Prim::MakeMacro => {
            use crate::eval::closure;
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
                f = mc.slot(f, crate::eval::closure::BODY);
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
        Prim::Apply | Prim::Funcall => Err(EvalError::BadSyntax {
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
            if f != Value::UNBOUND && crate::eval::closure::is_macro(mc, f) {
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
fn truth<'h>(b: bool) -> Value<'h> {
    if b {
        Value::T
    } else {
        Value::NIL
    }
}

fn fixnum(name: &'static str, v: Value<'_>) -> Result<i64, EvalError> {
    v.as_fixnum().ok_or(EvalError::WrongType {
        name,
        wanted: "a number",
    })
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
    let mut out = Vec::new();
    let mut cursor = list;
    while cursor.is_cons() {
        out.push(mc.car(cursor));
        cursor = mc.cdr(cursor);
    }
    out
}

#[cfg(test)]
#[path = "unit/primitives.rs"]
mod tests;
