# Quokka against ansi-test

Written by `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` (#60); do not edit.
*escaped* is an error no Lisp handler could catch, a fault in the machine;
*needs tier 1* a test that uses something Quokka lacks and tier 1 should
have (an operator, or reader syntax); *needs later* one that uses only
features deferred to a later tier or issue (`tests/ansi/deferred.lisp`).
A test can need several things.

| group | pass | fail | escaped | timeout | needs tier 1 | needs later | total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cons | 1269 | 31 | 0 | 1 | 238 | 99 | 1638 |
| sequences | 1741 | 277 | 0 | 2 | 76 | 1062 | 3158 |
| strings | 293 | 90 | 0 | 0 | 13 | 105 | 501 |
| characters | 123 | 0 | 0 | 0 | 46 | 90 | 259 |
| symbols | 66 | 19 | 0 | 1 | 1051 | 8 | 1145 |
| packages | 55 | 21 | 0 | 0 | 251 | 13 | 340 |
| numbers | 366 | 22 | 0 | 4 | 328 | 718 | 1438 |
| reader | 50 | 8 | 0 | 2 | 82 | 23 | 165 |
| printer | 80 | 37 | 0 | 2 | 422 | 281 | 822 |
| data-and-control-flow | 845 | 83 | 13 | 0 | 277 | 202 | 1420 |
| eval-and-compile | 188 | 20 | 5 | 1 | 94 | 10 | 318 |
| iteration | 81 | 14 | 0 | 0 | 16 | 727 | 838 |
| hash-tables | 42 | 9 | 0 | 0 | 57 | 59 | 167 |
| structures | 20 | 50 | 0 | 1 | 12 | 33 | 116 |
| **all** | **5219** | **681** | **18** | **14** | **2963** | **3430** | **12325** |

## Tier-1 gaps, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `SYMBOL-PLIST[tier-1]` | 997 |
| `WITH-STANDARD-IO-SYNTAX[tier-1]` | 494 |
| `COMPILE[tier-1]` | 380 |
| `MACRO-FUNCTION[tier-1]` | 207 |
| `DELETE-PACKAGE[tier-1]` | 202 |
| `PACKAGE-USED-BY-LIST[tier-1]` | 171 |
| `UNUSE-PACKAGE[tier-1]` | 170 |
| `load-error[*HASH-TABLES*]` | 98 |
| `load-error[*STREAMS*]` | 98 |
| `READTABLE-CASE[tier-1]` | 78 |
| `FBOUNDP[tier-1]` | 74 |
| `PARSE-INTEGER[tier-1]` | 56 |
| `NSET-EXCLUSIVE-OR[tier-1]` | 53 |
| `BYTE[tier-1]` | 48 |
| `DO-SYMBOLS[tier-1]` | 43 |
| `PACKAGE-SHADOWING-SYMBOLS[tier-1]` | 41 |
| `CCASE[tier-1]` | 38 |
| `DO-EXTERNAL-SYMBOLS[tier-1]` | 34 |
| `FUNCTIONP[tier-1]` | 33 |
| `DEFCONSTANT[tier-1]` | 30 |
| `CONSTANTP[tier-1]` | 28 |
| `READ-PRESERVING-WHITESPACE[tier-1]` | 27 |
| `MULTIPLE-VALUE-SETQ[tier-1]` | 26 |
| `NSUBST-IF[tier-1]` | 24 |
| `NSUBST-IF-NOT[tier-1]` | 23 |
| `SXHASH[tier-1]` | 21 |
| `WITH-PACKAGE-ITERATOR[tier-1]` | 21 |
| `IMPORT[tier-1]` | 20 |
| `PROG[tier-1]` | 20 |
| `DO-ALL-SYMBOLS[tier-1]` | 19 |
| `GET[tier-1]` | 18 |
| `NAME-CHAR[tier-1]` | 18 |
| `GET-PROPERTIES[tier-1]` | 17 |
| `PROG*[tier-1]` | 17 |
| `WITH-HASH-TABLE-ITERATOR[tier-1]` | 17 |
| `BYTE-SIZE[tier-1]` | 16 |
| `DEFINE-COMPILER-MACRO[tier-1]` | 16 |
| `LOGORC1[tier-1]` | 16 |
| `LOGORC2[tier-1]` | 16 |
| `SHADOW[tier-1]` | 16 |
| `FDEFINITION[tier-1]` | 15 |
| `LOGANDC1[tier-1]` | 15 |
| `LOGANDC2[tier-1]` | 15 |
| `LOGEQV[tier-1]` | 15 |
| `LOGNAND[tier-1]` | 15 |
| `LOGNOR[tier-1]` | 15 |
| `SIMPLE-STRING-P[tier-1]` | 15 |
| `CHAR-INT[tier-1]` | 14 |
| `FMAKUNBOUND[tier-1]` | 13 |
| `LDB[tier-1]` | 13 |
| `RENAME-PACKAGE[tier-1]` | 13 |
| `ATANH[tier-1]` | 12 |
| `BOOLE[tier-1]` | 12 |
| `MASK-FIELD[tier-1]` | 12 |
| `UNINTERN[tier-1]` | 12 |
| `load-error[*SEARCHED-VECTOR*]` | 12 |
| `FIND-ALL-SYMBOLS[tier-1]` | 11 |
| `ACOSH[tier-1]` | 10 |
| `ASINH[tier-1]` | 10 |
| `SET-SYNTAX-FROM-CHAR[tier-1]` | 10 |
| `DEPOSIT-FIELD[tier-1]` | 9 |
| `DPB[tier-1]` | 9 |
| `GENTEMP[tier-1]` | 9 |
| `SPECIAL-OPERATOR-P[tier-1]` | 9 |
| `CAAAAR[tier-1]` | 8 |
| `CAAADR[tier-1]` | 8 |
| `CAADAR[tier-1]` | 8 |
| `CAADDR[tier-1]` | 8 |
| `CADAAR[tier-1]` | 8 |
| `CADADR[tier-1]` | 8 |
| `CADDAR[tier-1]` | 8 |
| `CDAAAR[tier-1]` | 8 |
| `CDAADR[tier-1]` | 8 |
| `CDADAR[tier-1]` | 8 |
| `CDADDR[tier-1]` | 8 |
| `CDDAAR[tier-1]` | 8 |
| `CDDADR[tier-1]` | 8 |
| `CDDDAR[tier-1]` | 8 |
| `COMPILED-FUNCTION-P[tier-1]` | 8 |
| `DEFINE-SYMBOL-MACRO[tier-1]` | 8 |
| `LOGTEST[tier-1]` | 8 |
| `REMPROP[tier-1]` | 8 |
| `STANDARD-CHAR-P[tier-1]` | 8 |
| `UNEXPORT[tier-1]` | 8 |
| `COPY-SYMBOL[tier-1]` | 7 |
| `FUNCTION-LAMBDA-EXPRESSION[tier-1]` | 7 |
| `HASH-TABLE-TEST[tier-1]` | 7 |
| `PRINT-UNREADABLE-OBJECT[tier-1]` | 7 |
| `BYTE-POSITION[tier-1]` | 6 |
| `HASH-TABLE-REHASH-SIZE[tier-1]` | 6 |
| `HASH-TABLE-REHASH-THRESHOLD[tier-1]` | 6 |
| `SHADOWING-IMPORT[tier-1]` | 6 |
| `reader[#=]` | 6 |
| `COMPILER-MACRO-FUNCTION[tier-1]` | 5 |
| `SIMPLE-VECTOR-P[tier-1]` | 4 |
| `load-error[*BOOLE-VALS*]` | 4 |
| `reader[#.]` | 4 |
| `HASH-TABLE-SIZE[tier-1]` | 3 |
| `load-error[STRUCT-INCLUDE-02B]` | 3 |
| `reader[other]` | 3 |
| `ARRAYP[tier-1]` | 2 |
| `COPY-STRUCTURE[tier-1]` | 2 |
| `load-error[STRUCT-INCLUDE-01B-D]` | 2 |
| `load-error[STRUCT-INCLUDE-04B]` | 2 |
| `LDB-TEST[tier-1]` | 1 |
| `load-error[DEFSETF.5-ACCESSOR]` | 1 |
| `load-error[STRUCT-INCLUDE-03B]` | 1 |

## Deferred features, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `LOOP[tier-2:loop]` | 3898 |
| `reader[ratio]` | 628 |
| `TYPEP[phase-3:types]` | 596 |
| `reader[#*]` | 578 |
| `ASSERT[tier-2:conditions]` | 561 |
| `RANDOM[tier-2:random]` | 503 |
| `HANDLER-BIND[tier-2:conditions]` | 260 |
| `TYPE-ERROR-DATUM[tier-2:conditions]` | 249 |
| `TYPE-ERROR-EXPECTED-TYPE[tier-2:conditions]` | 249 |
| `MUFFLE-WARNING[tier-2:conditions]` | 244 |
| `reader[#C]` | 210 |
| `COMPLEX[deferred:complex-numbers]` | 170 |
| `DEFGENERIC[tier-2:clos]` | 132 |
| `RATIONAL[issue-59:ratios]` | 119 |
| `MAKE-RANDOM-STATE[tier-2:random]` | 114 |
| `FIND-CLASS[tier-2:clos]` | 110 |
| `MAKE-CONDITION[tier-2:conditions]` | 110 |
| `LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 101 |
| `MAKE-PATHNAME[tier-2:pathnames-and-files]` | 101 |
| `FIND-METHOD[tier-2:clos]` | 98 |
| `SUBTYPEP[phase-3:types]` | 88 |
| `ETYPECASE[phase-3:types]` | 74 |
| `REALPART[deferred:complex-numbers]` | 72 |
| `BIT[tier-2:arrays]` | 55 |
| `PPRINT-LOGICAL-BLOCK[tier-2:pretty-printer]` | 54 |
| `FILL-POINTER[tier-2:arrays]` | 46 |
| `ARRAY-ELEMENT-TYPE[tier-2:arrays]` | 45 |
| `TYPECASE[phase-3:types]` | 29 |
| `WITH-OPEN-STREAM[tier-2:streams]` | 29 |
| `DOCUMENTATION[tier-2:clos]` | 28 |
| `PPRINT-INDENT[tier-2:pretty-printer]` | 28 |
| `IMAGPART[deferred:complex-numbers]` | 26 |
| `COPY-PPRINT-DISPATCH[tier-2:pretty-printer]` | 25 |
| `PPRINT-NEWLINE[tier-2:pretty-printer]` | 25 |
| `reader[#A]` | 21 |
| `COMPLEXP[deferred:complex-numbers]` | 20 |
| `CTYPECASE[phase-3:types]` | 20 |
| `LOOP-FINISH[tier-2:loop]` | 20 |
| `TYPE-OF[phase-3:types]` | 19 |
| `SET-PPRINT-DISPATCH[tier-2:pretty-printer]` | 17 |
| `DENOMINATOR[issue-59:ratios]` | 15 |
| `NUMERATOR[issue-59:ratios]` | 15 |
| `MAKE-TWO-WAY-STREAM[tier-2:streams]` | 14 |
| `CLASS-OF[tier-2:clos]` | 13 |
| `PPRINT-POP[tier-2:pretty-printer]` | 13 |
| `RATIONALIZE[issue-59:ratios]` | 12 |
| `PPRINT-FILL[tier-2:pretty-printer]` | 11 |
| `ADJUST-ARRAY[tier-2:arrays]` | 10 |
| `COMPUTE-RESTARTS[tier-2:conditions]` | 10 |
| `PHASE[deferred:complex-numbers]` | 10 |
| `PPRINT-TABULAR[tier-2:pretty-printer]` | 10 |
| `RANDOM-STATE-P[tier-2:random]` | 10 |
| `ABORT[tier-2:conditions]` | 9 |
| `PPRINT-LINEAR[tier-2:pretty-printer]` | 9 |
| `RESTART-NAME[tier-2:conditions]` | 9 |
| `PATHNAME[tier-2:pathnames-and-files]` | 8 |
| `PPRINT-EXIT-IF-LIST-EXHAUSTED[tier-2:pretty-printer]` | 8 |
| `MAKE-BROADCAST-STREAM[tier-2:streams]` | 7 |
| `DEFCLASS[tier-2:clos]` | 6 |
| `PACKAGE-ERROR-PACKAGE[tier-2:conditions]` | 6 |
| `PPRINT-TAB[tier-2:pretty-printer]` | 6 |
| `reader[#P]` | 6 |
| `ARRAY-DIMENSIONS[tier-2:arrays]` | 5 |
| `ARRAY-RANK[tier-2:arrays]` | 5 |
| `ARRAY-TOTAL-SIZE[tier-2:arrays]` | 5 |
| `CHECK-TYPE[phase-3:types]` | 5 |
| `CIS[deferred:complex-numbers]` | 5 |
| `DEFMETHOD[tier-2:clos]` | 5 |
| `STEP[tier-2:environment]` | 5 |
| `UPGRADED-COMPLEX-PART-TYPE[phase-3:types]` | 5 |
| `WITH-OPEN-FILE[tier-2:streams]` | 5 |
| `ARITHMETIC-ERROR-OPERANDS[tier-2:conditions]` | 4 |
| `ARITHMETIC-ERROR-OPERATION[tier-2:conditions]` | 4 |
| `CONJUGATE[deferred:complex-numbers]` | 4 |
| `DEFINE-CONDITION[tier-2:conditions]` | 4 |
| `DEFINE-METHOD-COMBINATION[tier-2:clos]` | 4 |
| `DEFTYPE[phase-3:types]` | 4 |
| `PPRINT[tier-2:pretty-printer]` | 4 |
| `RESTART-BIND[tier-2:conditions]` | 4 |
| `RESTART-CASE[tier-2:conditions]` | 4 |
| `TIME[tier-2:environment]` | 4 |
| `TRACE[tier-2:environment]` | 4 |
| `UNTRACE[tier-2:environment]` | 4 |
| `WITH-ACCESSORS[tier-2:clos]` | 4 |
| `WITH-COMPILATION-UNIT[deferred:compile-file]` | 4 |
| `WITH-CONDITION-RESTARTS[tier-2:conditions]` | 4 |
| `WITH-SIMPLE-RESTART[tier-2:conditions]` | 4 |
| `WITH-SLOTS[tier-2:clos]` | 4 |
| `CELL-ERROR-NAME[tier-2:conditions]` | 3 |
| `CLASS-NAME[tier-2:clos]` | 3 |
| `PPRINT-DISPATCH[tier-2:pretty-printer]` | 3 |
| `SBIT[tier-2:arrays]` | 3 |
| `ARRAY-DIMENSION[tier-2:arrays]` | 2 |
| `BREAK[tier-2:conditions]` | 2 |
| `STORE-VALUE[tier-2:conditions]` | 2 |
| `VECTOR-PUSH-EXTEND[tier-2:arrays]` | 2 |
| `ARRAY-HAS-FILL-POINTER-P[tier-2:arrays]` | 1 |
| `BIT-VECTOR-P[tier-2:arrays]` | 1 |
| `COMPILE-FILE-PATHNAME[deferred:compile-file]` | 1 |
| `COMPILE-FILE[deferred:compile-file]` | 1 |
| `CONTINUE[tier-2:conditions]` | 1 |
| `DELETE-FILE[tier-2:pathnames-and-files]` | 1 |
| `ED[tier-2:environment]` | 1 |
| `LOGICAL-PATHNAME-TRANSLATIONS[tier-2:pathnames-and-files]` | 1 |
| `MAKE-INSTANCE[tier-2:clos]` | 1 |
| `NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `ROW-MAJOR-AREF[tier-2:arrays]` | 1 |
