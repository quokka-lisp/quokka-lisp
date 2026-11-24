# Quokka against ansi-test

Written by `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` (#60); do not edit.
*escaped* is an error no Lisp handler could catch, a fault in the machine;
*needs tier 1* a test that uses something Quokka lacks and tier 1 should
have (an operator, or reader syntax); *needs later* one that uses only
features deferred to a later tier or issue (`tests/ansi/deferred.lisp`).
A test can need several things.

| group | pass | fail | escaped | timeout | needs tier 1 | needs later | total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cons | 1474 | 0 | 0 | 0 | 0 | 164 | 1638 |
| sequences | 2549 | 0 | 0 | 0 | 0 | 609 | 3158 |
| strings | 363 | 0 | 0 | 0 | 0 | 138 | 501 |
| arrays | 629 | 0 | 0 | 1 | 0 | 604 | 1234 |
| characters | 133 | 0 | 0 | 0 | 0 | 126 | 259 |
| symbols | 126 | 0 | 0 | 0 | 0 | 1019 | 1145 |
| packages | 270 | 0 | 0 | 2 | 0 | 68 | 340 |
| numbers | 547 | 1 | 0 | 0 | 0 | 890 | 1438 |
| reader | 115 | 0 | 0 | 0 | 0 | 50 | 165 |
| printer | 229 | 0 | 0 | 0 | 0 | 593 | 822 |
| data-and-control-flow | 1165 | 0 | 0 | 0 | 0 | 255 | 1420 |
| eval-and-compile | 276 | 0 | 0 | 0 | 0 | 42 | 318 |
| iteration | 95 | 0 | 0 | 0 | 0 | 743 | 838 |
| hash-tables | 81 | 0 | 0 | 0 | 0 | 86 | 167 |
| structures | 60 | 0 | 0 | 1 | 12 | 43 | 116 |
| **all** | **8112** | **1** | **0** | **4** | **12** | **5430** | **13559** |

## Tier-1 gaps, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `load-error[STRUCT-TEST-45-B45]` | 4 |
| `load-error[STRUCT-TEST-53-B53]` | 2 |
| `load-error[STRUCT-TEST-55-C55]` | 2 |
| `load-error[STRUCT-TEST-37-C37]` | 1 |
| `load-error[STRUCT-TEST-38-C38]` | 1 |
| `load-error[STRUCT-TEST-39-C39]` | 1 |
| `load-error[STRUCT-TEST-60-B60]` | 1 |

## Deferred features, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `LOOP[tier-2:loop]` | 4138 |
| `TYPEP[phase-3:types]` | 1131 |
| `reader[ratio]` | 663 |
| `ASSERT[tier-2:conditions]` | 598 |
| `RANDOM[tier-2:random]` | 513 |
| `HANDLER-BIND[tier-2:conditions]` | 277 |
| `TYPE-ERROR-DATUM[tier-2:conditions]` | 266 |
| `TYPE-ERROR-EXPECTED-TYPE[tier-2:conditions]` | 266 |
| `FIND-CLASS[tier-2:clos]` | 263 |
| `MUFFLE-WARNING[tier-2:conditions]` | 261 |
| `reader[#C]` | 224 |
| `SUBTYPEP[phase-3:types]` | 184 |
| `COMPLEX[deferred:complex-numbers]` | 175 |
| `DEFGENERIC[tier-2:clos]` | 145 |
| `MAKE-RANDOM-STATE[tier-2:random]` | 127 |
| `MAKE-CONDITION[tier-2:conditions]` | 123 |
| `RATIONAL[issue-59:ratios]` | 123 |
| `LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 114 |
| `MAKE-PATHNAME[tier-2:pathnames-and-files]` | 114 |
| `FIND-METHOD[tier-2:clos]` | 111 |
| `ETYPECASE[phase-3:types]` | 75 |
| `REALPART[deferred:complex-numbers]` | 72 |
| `note[NIL-VECTORS-ARE-STRINGS]` | 57 |
| `PPRINT-LOGICAL-BLOCK[tier-2:pretty-printer]` | 53 |
| `TYPECASE[phase-3:types]` | 30 |
| `WITH-OPEN-STREAM[tier-2:streams]` | 29 |
| `DOCUMENTATION[tier-2:clos]` | 28 |
| `PPRINT-INDENT[tier-2:pretty-printer]` | 28 |
| `IMAGPART[deferred:complex-numbers]` | 26 |
| `COPY-PPRINT-DISPATCH[tier-2:pretty-printer]` | 25 |
| `PPRINT-NEWLINE[tier-2:pretty-printer]` | 25 |
| `CTYPECASE[phase-3:types]` | 21 |
| `COMPLEXP[deferred:complex-numbers]` | 20 |
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
| `COMPUTE-RESTARTS[tier-2:conditions]` | 10 |
| `PHASE[deferred:complex-numbers]` | 10 |
| `PPRINT-TABULAR[tier-2:pretty-printer]` | 10 |
| `RANDOM-STATE-P[tier-2:random]` | 10 |
| `ABORT[tier-2:conditions]` | 9 |
| `PPRINT-LINEAR[tier-2:pretty-printer]` | 9 |
| `RESTART-NAME[tier-2:conditions]` | 9 |
| `PATHNAME[tier-2:pathnames-and-files]` | 8 |
| `PPRINT-EXIT-IF-LIST-EXHAUSTED[tier-2:pretty-printer]` | 8 |
| `note[NO-FLOATING-POINT-UNDERFLOW-BY-DEFAULT]` | 8 |
| `MAKE-BROADCAST-STREAM[tier-2:streams]` | 7 |
| `reader[#P]` | 7 |
| `DEFCLASS[tier-2:clos]` | 6 |
| `PACKAGE-ERROR-PACKAGE[tier-2:conditions]` | 6 |
| `PPRINT-TAB[tier-2:pretty-printer]` | 6 |
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
| `format-directive[tier-2:format]` | 4 |
| `CELL-ERROR-NAME[tier-2:conditions]` | 3 |
| `CLASS-NAME[tier-2:clos]` | 3 |
| `PPRINT-DISPATCH[tier-2:pretty-printer]` | 3 |
| `STORE-VALUE[tier-2:conditions]` | 2 |
| `ratio[issue-59:ratios]` | 2 |
| `COMPILE-FILE-PATHNAME[deferred:compile-file]` | 1 |
| `COMPILE-FILE[deferred:compile-file]` | 1 |
| `CONTINUE[tier-2:conditions]` | 1 |
| `DELETE-FILE[tier-2:pathnames-and-files]` | 1 |
| `ED[tier-2:environment]` | 1 |
| `LOGICAL-PATHNAME-TRANSLATIONS[tier-2:pathnames-and-files]` | 1 |
| `MAKE-INSTANCE[tier-2:clos]` | 1 |
| `NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `sbcl[same-as-sbcl]` | 1 |
