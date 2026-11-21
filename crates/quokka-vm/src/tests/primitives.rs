//! Tests for [`crate::primitives`].
//!
//! A child module of it, included with `#[path]`, so it still sees the
//! module's private items. That is why these are a separate *file* and not
//! a separate crate — see CLAUDE.md for what the alternative would cost.

use super::*;

/// Both intercepted indices, not just one.
///
/// `APPLY` was first written as 25 — counted by eye, and the eye counted
/// the `name:` fields of some error constructors too. The interception
/// silently did not fire and `apply` reported that it should never have
/// been called. A constant that names a table slot needs a test that it
/// still names it.
#[test]
fn the_intercepted_indices_are_still_right() {
    assert_eq!(TABLE[FUNCALL as usize].name, "FUNCALL");
    assert_eq!(TABLE[APPLY as usize].name, "APPLY");
    assert_eq!(TABLE[ERROR as usize].name, "ERROR");
    assert_eq!(TABLE[VALUES as usize].name, "VALUES");
    assert_eq!(TABLE[VALUES_LIST as usize].name, "VALUES-LIST");
}

/// Table order is part of the image format. This does not stop a reorder,
/// but it makes one a deliberate act with a failing test attached.
#[test]
fn the_table_order_is_pinned() {
    let names: Vec<&str> = TABLE.iter().map(|p| p.name).collect();
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
            "%MAKE-STRING"
        ]
    );
}
