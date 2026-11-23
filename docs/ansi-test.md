# Quokka against ansi-test

Written by `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` (#60); do not edit.
*escaped* is an error no Lisp handler could catch, a fault in the machine;
*needs tier 1* a test that uses something Quokka lacks and tier 1 should
have (an operator, or reader syntax); *needs later* one that uses only
features deferred to a later tier or issue (`tests/ansi/deferred.lisp`).
A test can need several things.

| group | pass | fail | escaped | timeout | needs tier 1 | needs later | total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cons | 847 | 200 | 246 | 1 | 246 | 98 | 1638 |
| sequences | 1361 | 140 | 650 | 2 | 60 | 946 | 3159 |
| strings | 200 | 0 | 189 | 0 | 17 | 95 | 501 |
| characters | 123 | 0 | 0 | 0 | 80 | 52 | 255 |
| symbols | 61 | 10 | 6 | 1 | 1061 | 6 | 1145 |
| packages | 39 | 2 | 164 | 0 | 262 | 13 | 480 |
| numbers | 362 | 22 | 7 | 4 | 328 | 715 | 1438 |
| reader | 45 | 8 | 49 | 2 | 82 | 7 | 193 |
| printer | 73 | 18 | 47 | 2 | 410 | 271 | 821 |
| data-and-control-flow | 718 | 87 | 61 | 0 | 378 | 176 | 1420 |
| eval-and-compile | 113 | 9 | 10 | 1 | 184 | 1 | 318 |
| iteration | 72 | 0 | 7 | 0 | 39 | 720 | 838 |
| hash-tables | 42 | 9 | 19 | 0 | 61 | 36 | 167 |
| structures | 2 | 68 | 0 | 0 | 13 | 32 | 115 |
| **all** | **4058** | **573** | **1455** | **13** | **3221** | **3168** | **12488** |

## Tier-1 gaps, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `reader[#+]` | 1094 |
| `reader[#-]` | 1080 |
| `DECLARE[tier-1]` | 742 |
| `WITH-STANDARD-IO-SYNTAX[tier-1]` | 467 |
| `COMPILE[tier-1]` | 378 |
| `MACRO-FUNCTION[tier-1]` | 205 |
| `DELETE-PACKAGE[tier-1]` | 199 |
| `PACKAGE-USED-BY-LIST[tier-1]` | 168 |
| `UNUSE-PACKAGE[tier-1]` | 167 |
| `load-error[*STREAMS*]` | 96 |
| `load-error[*STRUCTURES*]` | 96 |
| `READTABLE-CASE[tier-1]` | 73 |
| `FBOUNDP[tier-1]` | 72 |
| `NSET-EXCLUSIVE-OR[tier-1]` | 53 |
| `PARSE-INTEGER[tier-1]` | 49 |
| `BYTE[tier-1]` | 48 |
| `PACKAGE-SHADOWING-SYMBOLS[tier-1]` | 41 |
| `reader[#.]` | 40 |
| `DO-SYMBOLS[tier-1]` | 39 |
| `CCASE[tier-1]` | 37 |
| `FUNCTIONP[tier-1]` | 33 |
| `DEFCONSTANT[tier-1]` | 31 |
| `DO-EXTERNAL-SYMBOLS[tier-1]` | 30 |
| `CONSTANTP[tier-1]` | 27 |
| `READ-PRESERVING-WHITESPACE[tier-1]` | 27 |
| `MULTIPLE-VALUE-SETQ[tier-1]` | 26 |
| `NSUBST-IF[tier-1]` | 24 |
| `NSUBST-IF-NOT[tier-1]` | 23 |
| `WITH-PACKAGE-ITERATOR[tier-1]` | 21 |
| `IMPORT[tier-1]` | 20 |
| `PROG[tier-1]` | 20 |
| `reader[other]` | 20 |
| `GET[tier-1]` | 19 |
| `SYMBOL-PLIST[tier-1]` | 19 |
| `NAME-CHAR[tier-1]` | 18 |
| `DECLAIM[tier-1]` | 17 |
| `DO-ALL-SYMBOLS[tier-1]` | 17 |
| `GET-PROPERTIES[tier-1]` | 17 |
| `PROG*[tier-1]` | 17 |
| `SXHASH[tier-1]` | 17 |
| `WITH-HASH-TABLE-ITERATOR[tier-1]` | 17 |
| `DEFINE-COMPILER-MACRO[tier-1]` | 16 |
| `LOGORC1[tier-1]` | 16 |
| `LOGORC2[tier-1]` | 16 |
| `SHADOW[tier-1]` | 16 |
| `FDEFINITION[tier-1]` | 15 |
| `LOGANDC1[tier-1]` | 15 |
| `LOGEQV[tier-1]` | 15 |
| `LOGNAND[tier-1]` | 15 |
| `LOGNOR[tier-1]` | 15 |
| `CHAR-INT[tier-1]` | 14 |
| `LOGANDC2[tier-1]` | 14 |
| `FMAKUNBOUND[tier-1]` | 13 |
| `LDB[tier-1]` | 13 |
| `ATANH[tier-1]` | 12 |
| `BOOLE[tier-1]` | 12 |
| `MASK-FIELD[tier-1]` | 12 |
| `UNINTERN[tier-1]` | 12 |
| `ACOSH[tier-1]` | 10 |
| `ASINH[tier-1]` | 10 |
| `RENAME-PACKAGE[tier-1]` | 10 |
| `SET-SYNTAX-FROM-CHAR[tier-1]` | 10 |
| `SIMPLE-STRING-P[tier-1]` | 10 |
| `DEPOSIT-FIELD[tier-1]` | 9 |
| `DPB[tier-1]` | 9 |
| `GENTEMP[tier-1]` | 9 |
| `PROCLAIM[tier-1]` | 9 |
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
| `BYTE-SIZE[tier-1]` | 6 |
| `HASH-TABLE-REHASH-SIZE[tier-1]` | 6 |
| `HASH-TABLE-REHASH-THRESHOLD[tier-1]` | 6 |
| `SHADOWING-IMPORT[tier-1]` | 6 |
| `reader[#=]` | 6 |
| `COMPILER-MACRO-FUNCTION[tier-1]` | 5 |
| `FIND-ALL-SYMBOLS[tier-1]` | 4 |
| `load-error[*BOOLE-VALS*]` | 4 |
| `HASH-TABLE-SIZE[tier-1]` | 3 |
| `load-error[STRUCT-INCLUDE-02B]` | 3 |
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
| `LOOP[tier-2:loop]` | 2871 |
| `reader[ratio]` | 624 |
| `reader[#*]` | 578 |
| `TYPEP[phase-3:types]` | 557 |
| `RANDOM[tier-2:random]` | 497 |
| `ASSERT[tier-2:conditions]` | 452 |
| `HANDLER-BIND[tier-2:conditions]` | 260 |
| `TYPE-ERROR-DATUM[tier-2:conditions]` | 249 |
| `TYPE-ERROR-EXPECTED-TYPE[tier-2:conditions]` | 249 |
| `MUFFLE-WARNING[tier-2:conditions]` | 244 |
| `reader[#C]` | 208 |
| `COMPLEX[deferred:complex-numbers]` | 170 |
| `RATIONAL[issue-59:ratios]` | 118 |
| `MAKE-RANDOM-STATE[tier-2:random]` | 112 |
| `MAKE-CONDITION[tier-2:conditions]` | 108 |
| `LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 99 |
| `MAKE-PATHNAME[tier-2:pathnames-and-files]` | 99 |
| `SUBTYPEP[phase-3:types]` | 87 |
| `ETYPECASE[phase-3:types]` | 74 |
| `REALPART[deferred:complex-numbers]` | 72 |
| `PPRINT-LOGICAL-BLOCK[tier-2:pretty-printer]` | 54 |
| `FILL-POINTER[tier-2:arrays]` | 36 |
| `DEFGENERIC[tier-2:clos]` | 31 |
| `TYPECASE[phase-3:types]` | 29 |
| `WITH-OPEN-STREAM[tier-2:streams]` | 29 |
| `DOCUMENTATION[tier-2:clos]` | 28 |
| `PPRINT-INDENT[tier-2:pretty-printer]` | 28 |
| `BIT[tier-2:arrays]` | 26 |
| `IMAGPART[deferred:complex-numbers]` | 26 |
| `COPY-PPRINT-DISPATCH[tier-2:pretty-printer]` | 25 |
| `PPRINT-NEWLINE[tier-2:pretty-printer]` | 25 |
| `ARRAY-ELEMENT-TYPE[tier-2:arrays]` | 21 |
| `reader[#A]` | 21 |
| `COMPLEXP[deferred:complex-numbers]` | 20 |
| `CTYPECASE[phase-3:types]` | 20 |
| `LOOP-FINISH[tier-2:loop]` | 20 |
| `SET-PPRINT-DISPATCH[tier-2:pretty-printer]` | 17 |
| `TYPE-OF[phase-3:types]` | 17 |
| `DENOMINATOR[issue-59:ratios]` | 14 |
| `MAKE-TWO-WAY-STREAM[tier-2:streams]` | 14 |
| `NUMERATOR[issue-59:ratios]` | 14 |
| `PPRINT-POP[tier-2:pretty-printer]` | 14 |
| `CLASS-OF[tier-2:clos]` | 12 |
| `FIND-CLASS[tier-2:clos]` | 12 |
| `RATIONALIZE[issue-59:ratios]` | 12 |
| `PPRINT-FILL[tier-2:pretty-printer]` | 11 |
| `COMPUTE-RESTARTS[tier-2:conditions]` | 10 |
| `PHASE[deferred:complex-numbers]` | 10 |
| `PPRINT-TABULAR[tier-2:pretty-printer]` | 10 |
| `RANDOM-STATE-P[tier-2:random]` | 10 |
| `ABORT[tier-2:conditions]` | 9 |
| `PPRINT-EXIT-IF-LIST-EXHAUSTED[tier-2:pretty-printer]` | 9 |
| `PPRINT-LINEAR[tier-2:pretty-printer]` | 9 |
| `RESTART-NAME[tier-2:conditions]` | 9 |
| `PATHNAME[tier-2:pathnames-and-files]` | 8 |
| `MAKE-BROADCAST-STREAM[tier-2:streams]` | 7 |
| `DEFCLASS[tier-2:clos]` | 6 |
| `PACKAGE-ERROR-PACKAGE[tier-2:conditions]` | 6 |
| `PPRINT-TAB[tier-2:pretty-printer]` | 6 |
| `reader[#P]` | 6 |
| `CIS[deferred:complex-numbers]` | 5 |
| `DEFMETHOD[tier-2:clos]` | 5 |
| `STEP[tier-2:environment]` | 5 |
| `ARITHMETIC-ERROR-OPERANDS[tier-2:conditions]` | 4 |
| `ARITHMETIC-ERROR-OPERATION[tier-2:conditions]` | 4 |
| `CHECK-TYPE[phase-3:types]` | 4 |
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
| `UPGRADED-COMPLEX-PART-TYPE[phase-3:types]` | 4 |
| `WITH-ACCESSORS[tier-2:clos]` | 4 |
| `WITH-COMPILATION-UNIT[deferred:compile-file]` | 4 |
| `WITH-CONDITION-RESTARTS[tier-2:conditions]` | 4 |
| `WITH-OPEN-FILE[tier-2:streams]` | 4 |
| `WITH-SIMPLE-RESTART[tier-2:conditions]` | 4 |
| `WITH-SLOTS[tier-2:clos]` | 4 |
| `CELL-ERROR-NAME[tier-2:conditions]` | 3 |
| `CLASS-NAME[tier-2:clos]` | 3 |
| `PPRINT-DISPATCH[tier-2:pretty-printer]` | 3 |
| `SBIT[tier-2:arrays]` | 3 |
| `STORE-VALUE[tier-2:conditions]` | 2 |
| `CONTINUE[tier-2:conditions]` | 1 |
| `ED[tier-2:environment]` | 1 |
| `LOGICAL-PATHNAME-TRANSLATIONS[tier-2:pathnames-and-files]` | 1 |
| `MAKE-INSTANCE[tier-2:clos]` | 1 |
| `NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `ROW-MAJOR-AREF[tier-2:arrays]` | 1 |
