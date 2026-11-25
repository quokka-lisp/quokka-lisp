//! Tests for [`crate::primitives`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.
//!
//! `src/unit/` and `tests/` are different things: these see everything, and
//! cargo's `tests/` is a separate crate that sees only the public API.

use super::*;

/// The enum's discriminant is the table index, which is what every `Prim::X
/// as u32` relies on — and what an image's stored primitive references mean.
///
/// This replaces a test that pinned five hand-counted constants. It existed
/// because `APPLY` was once written as 25, was really 21, and the
/// interception silently never fired. There is nothing left to miscount, but
/// the correspondence between a variant and its row is still an invariant and
/// still worth checking.
#[test]
fn the_table_is_in_enum_order() {
    for (index, (prim, name, _)) in TABLE.iter().enumerate() {
        assert_eq!(*prim as usize, index, "{name} is out of place");
        assert_eq!(Prim::from_u32(index as u32), Some(*prim));
    }
    assert_eq!(Prim::from_u32(TABLE.len() as u32), None);
}

/// Table order is part of the image format. This does not stop a reorder,
/// but it makes one a deliberate act with a failing test attached.
#[test]
fn the_table_order_is_pinned() {
    let names: Vec<&str> = TABLE.iter().map(|e| e.1).collect();
    assert_eq!(
        names,
        [
            "+",
            "-",
            "*",
            "=",
            "<",
            ">",
            "CONS",
            "CAR",
            "CDR",
            "EQ",
            "NULL",
            "CONSP",
            "SYMBOLP",
            "FUNCALL",
            "LIST",
            "APPEND",
            "GENSYM",
            "%SET-SYMBOL-FUNCTION",
            "SYMBOL-FUNCTION",
            "KEYWORDP",
            "SYMBOL-NAME",
            "APPLY",
            "%MACRO-FUNCTION",
            "%MAKE-VECTOR",
            "VECTOR",
            "AREF",
            "%SET-AREF",
            "%SEQUENCE-LENGTH",
            "VECTORP",
            "STRINGP",
            "CHARACTERP",
            "%MAKE-HASH-TABLE",
            "%GETHASH",
            "%SET-GETHASH",
            "REMHASH",
            "HASH-TABLE-COUNT",
            "HASH-TABLE-P",
            "%HASH-TABLE-ENTRIES",
            "RPLACA",
            "RPLACD",
            "ERROR",
            "%CONDITION-TYPE",
            "%CONDITION-MESSAGE",
            "%CONDITIONP",
            "%INTERN",
            "%STRING-APPEND",
            "%VECTOR-TO-LIST",
            "VALUES",
            "VALUES-LIST",
            "%MAKE-STRING",
            "%ASSEMBLE",
            "INTEGERP",
            "%DISASSEMBLE",
            "%REWRITE-LAMBDA-LIST",
            "%READ-SOURCE",
            "%SPECIAL-OPERATOR-P",
            "%REWRITE-MACRO-LAMBDA-LIST",
            "%MAKE-MACRO",
            "BOUNDP",
            "SYMBOL-VALUE",
            "SET",
            "%SYMBOL-HOME",
            "%SET-SYMBOL-HOME",
            "%SYSTEM-SYMBOLS",
            "%FIND-SYSTEM-SYMBOL",
            "MAKE-SYMBOL",
            "%INTERN-KEYWORD",
            "CHAR-CODE",
            "CODE-CHAR",
            "CHAR-UPCASE",
            "CHAR-DOWNCASE",
            "ALPHA-CHAR-P",
            "%CONSTANT-STRING",
            "%WRITE-CONSOLE",
            "%QUOTIENT",
            "REM",
            "%PRINTED",
            "MAKUNBOUND",
            "%MAKE-BIGNUM",
            "%BIGNUM-LENGTH",
            "%BIGNUM-NEGATIVE-P",
            "%BIGNUM-LIMB",
            "%SET-BIGNUM-LIMB",
            "%BIGNUM-NORMALIZE",
            "%BIGNUMP",
            "%FIXNUM-TO-BIGNUM",
            "%INTEGER-COMPARE",
            "%FIXNUM-LOGAND",
            "%FIXNUM-LOGIOR",
            "%FIXNUM-LOGXOR",
            "/",
            "FLOATP",
            "%FLOAT",
            "%DOUBLE-EQL",
            "%DOUBLE-ROUND",
            "%DOUBLE-INTEGER",
            "%DECODE-DOUBLE",
            "%SCALE-DOUBLE",
            "%PARSE-DOUBLE",
            "%DOUBLE-DIGITS",
            "%SQRT",
            "%EXP",
            "%LOG",
            "%SIN",
            "%COS",
            "%TAN",
            "%ASIN",
            "%ACOS",
            "%ATAN",
            "%SINH",
            "%COSH",
            "%TANH",
            "%ATAN2",
            "%POW",
            "SYMBOL-PLIST",
            "%SET-SYMBOL-PLIST",
            "%FBOUNDP",
            "%FMAKUNBOUND",
            "FUNCTIONP",
            "%MAKE-ARRAY-HEADER",
            "%ARRAY-HEADER-SLOT",
            "%SET-ARRAY-HEADER-SLOT",
            "%MAKE-BIT-VECTOR",
            "%ARRAY-KIND",
            "%ASINH",
            "%ACOSH",
            "%ATANH",
            "%HASH-BUCKET",
            "%HASH-INSERT",
            "%HASH-DELETE-ENTRY",
            "SXHASH",
            "HASH-TABLE-TEST",
            "%MAKE-STRUCTURE",
            "%STRUCTURE-LAYOUT",
            "%STRUCTURE-REF",
            "%SET-STRUCTURE-REF",
            "%MAKE-NIL-VECTOR",
            "%MAKE-RATIO",
            "%RATIOP",
            "%RATIO-NUMERATOR",
            "%RATIO-DENOMINATOR",
            "%STRING-MISMATCH"
        ]
    );
}
