# Quokka against ansi-test

Written by `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` (#60); do not edit.
*escaped* is an error no Lisp handler could catch, a fault in the machine;
*needs tier 1* a test that uses something Quokka lacks and tier 1 should
have (an operator, or reader syntax); *needs later* one that uses only
features deferred to a later tier or issue (`tests/ansi/deferred.lisp`).
A test can need several things.

| group | pass | fail | escaped | timeout | needs tier 1 | needs later | total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cons | 1452 | 23 | 0 | 1 | 0 | 162 | 1638 |
| sequences | 2480 | 70 | 0 | 2 | 0 | 606 | 3158 |
| strings | 370 | 23 | 0 | 0 | 0 | 108 | 501 |
| arrays | 629 | 0 | 0 | 1 | 0 | 604 | 1234 |
| characters | 133 | 0 | 0 | 0 | 0 | 126 | 259 |
| symbols | 104 | 23 | 0 | 1 | 0 | 1017 | 1145 |
| packages | 254 | 17 | 0 | 2 | 0 | 67 | 340 |
| numbers | 532 | 21 | 0 | 4 | 0 | 881 | 1438 |
| reader | 111 | 4 | 0 | 1 | 0 | 49 | 165 |
| printer | 223 | 19 | 0 | 4 | 0 | 576 | 822 |
| data-and-control-flow | 1084 | 68 | 13 | 0 | 5 | 250 | 1420 |
| eval-and-compile | 237 | 28 | 5 | 0 | 6 | 42 | 318 |
| iteration | 95 | 0 | 0 | 0 | 0 | 743 | 838 |
| hash-tables | 81 | 1 | 0 | 0 | 3 | 82 | 167 |
| structures | 22 | 50 | 0 | 0 | 8 | 35 | 115 |
| **all** | **7807** | **347** | **18** | **16** | **22** | **5348** | **13558** |

## Tier-1 gaps, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `reader[#=]` | 6 |
| `reader[#.]` | 4 |
| `load-error[STRUCT-INCLUDE-02B]` | 3 |
| `reader[#*]` | 3 |
| `reader[#A]` | 3 |
| `load-error[STRUCT-INCLUDE-01B-D]` | 2 |
| `load-error[STRUCT-INCLUDE-04B]` | 2 |
| `load-error[DEFSETF.5-ACCESSOR]` | 1 |
| `load-error[STRUCT-INCLUDE-03B]` | 1 |

## Deferred features, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `LOOP[tier-2:loop]` | 4127 |
| `TYPEP[phase-3:types]` | 1135 |
| `reader[ratio]` | 663 |
| `ASSERT[tier-2:conditions]` | 596 |
| `RANDOM[tier-2:random]` | 511 |
| `HANDLER-BIND[tier-2:conditions]` | 277 |
| `TYPE-ERROR-DATUM[tier-2:conditions]` | 266 |
| `TYPE-ERROR-EXPECTED-TYPE[tier-2:conditions]` | 266 |
| `MUFFLE-WARNING[tier-2:conditions]` | 261 |
| `FIND-CLASS[tier-2:clos]` | 259 |
| `reader[#C]` | 223 |
| `SUBTYPEP[phase-3:types]` | 193 |
| `COMPLEX[deferred:complex-numbers]` | 175 |
| `DEFGENERIC[tier-2:clos]` | 145 |
| `MAKE-RANDOM-STATE[tier-2:random]` | 127 |
| `MAKE-CONDITION[tier-2:conditions]` | 123 |
| `RATIONAL[issue-59:ratios]` | 123 |
| `LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 114 |
| `MAKE-PATHNAME[tier-2:pathnames-and-files]` | 114 |
| `FIND-METHOD[tier-2:clos]` | 111 |
| `ETYPECASE[phase-3:types]` | 74 |
| `REALPART[deferred:complex-numbers]` | 72 |
| `PPRINT-LOGICAL-BLOCK[tier-2:pretty-printer]` | 54 |
| `TYPECASE[phase-3:types]` | 29 |
| `WITH-OPEN-STREAM[tier-2:streams]` | 29 |
| `DOCUMENTATION[tier-2:clos]` | 28 |
| `PPRINT-INDENT[tier-2:pretty-printer]` | 28 |
| `IMAGPART[deferred:complex-numbers]` | 26 |
| `COPY-PPRINT-DISPATCH[tier-2:pretty-printer]` | 25 |
| `PPRINT-NEWLINE[tier-2:pretty-printer]` | 25 |
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
| `STORE-VALUE[tier-2:conditions]` | 2 |
| `COMPILE-FILE-PATHNAME[deferred:compile-file]` | 1 |
| `COMPILE-FILE[deferred:compile-file]` | 1 |
| `CONTINUE[tier-2:conditions]` | 1 |
| `DELETE-FILE[tier-2:pathnames-and-files]` | 1 |
| `ED[tier-2:environment]` | 1 |
| `MAKE-INSTANCE[tier-2:clos]` | 1 |
| `NAMESTRING[tier-2:pathnames-and-files]` | 1 |
