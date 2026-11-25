# Quokka against ansi-test

Written by `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` (#60); do not edit.
*escaped* is an error no Lisp handler could catch, a fault in the machine;
*needs tier 1* a test that uses something Quokka lacks and tier 1 should
have (an operator, or reader syntax); *needs later* one that uses only
features deferred to a later tier or issue (`tests/ansi/deferred.lisp`).
A test can need several things.

| group | pass | fail | escaped | timeout | needs tier 1 | needs later | total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cons | 1477 | 0 | 0 | 0 | 0 | 161 | 1638 |
| sequences | 2549 | 0 | 0 | 0 | 0 | 609 | 3158 |
| strings | 363 | 0 | 0 | 0 | 0 | 138 | 501 |
| arrays | 629 | 0 | 0 | 0 | 0 | 605 | 1234 |
| characters | 133 | 0 | 0 | 0 | 0 | 126 | 259 |
| symbols | 126 | 0 | 0 | 0 | 0 | 1019 | 1145 |
| packages | 272 | 0 | 0 | 0 | 0 | 68 | 340 |
| numbers | 570 | 0 | 0 | 0 | 0 | 868 | 1438 |
| reader | 116 | 0 | 0 | 0 | 0 | 49 | 165 |
| printer | 229 | 0 | 0 | 0 | 0 | 593 | 822 |
| data-and-control-flow | 1165 | 0 | 0 | 0 | 0 | 255 | 1420 |
| eval-and-compile | 276 | 0 | 0 | 0 | 0 | 42 | 318 |
| iteration | 95 | 0 | 0 | 0 | 0 | 743 | 838 |
| hash-tables | 81 | 0 | 0 | 0 | 0 | 86 | 167 |
| structures | 60 | 0 | 0 | 0 | 0 | 55 | 115 |
| **all** | **8141** | **0** | **0** | **0** | **0** | **5417** | **13558** |

## Tier-1 gaps, by how many tests each blocks

| needs | tests |
| --- | ---: |

## Deferred features, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `LOOP[tier-2:loop]` | 4206 |
| `TYPEP[phase-3:types]` | 1148 |
| `ASSERT[tier-2:conditions]` | 640 |
| `RANDOM[tier-2:random]` | 523 |
| `FIND-CLASS[tier-2:clos]` | 511 |
| `reader[#C]` | 470 |
| `DEFGENERIC[tier-2:clos]` | 391 |
| `MAKE-RANDOM-STATE[tier-2:random]` | 373 |
| `MAKE-CONDITION[tier-2:conditions]` | 371 |
| `LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 361 |
| `MAKE-PATHNAME[tier-2:pathnames-and-files]` | 361 |
| `FIND-METHOD[tier-2:clos]` | 357 |
| `HANDLER-BIND[tier-2:conditions]` | 277 |
| `TYPE-ERROR-DATUM[tier-2:conditions]` | 267 |
| `TYPE-ERROR-EXPECTED-TYPE[tier-2:conditions]` | 267 |
| `MUFFLE-WARNING[tier-2:conditions]` | 262 |
| `SUBTYPEP[phase-3:types]` | 197 |
| `COMPLEX[deferred:complex-numbers]` | 180 |
| `ETYPECASE[phase-3:types]` | 76 |
| `REALPART[deferred:complex-numbers]` | 75 |
| `note[NIL-VECTORS-ARE-STRINGS]` | 57 |
| `PPRINT-LOGICAL-BLOCK[tier-2:pretty-printer]` | 54 |
| `DOCUMENTATION[tier-2:clos]` | 40 |
| `PPRINT-INDENT[tier-2:pretty-printer]` | 30 |
| `TYPECASE[phase-3:types]` | 30 |
| `WITH-OPEN-STREAM[tier-2:streams]` | 30 |
| `IMAGPART[deferred:complex-numbers]` | 27 |
| `COPY-PPRINT-DISPATCH[tier-2:pretty-printer]` | 26 |
| `PPRINT-NEWLINE[tier-2:pretty-printer]` | 26 |
| `COMPLEXP[deferred:complex-numbers]` | 21 |
| `CTYPECASE[phase-3:types]` | 21 |
| `PATHNAME[tier-2:pathnames-and-files]` | 21 |
| `TYPE-OF[phase-3:types]` | 21 |
| `LOOP-FINISH[tier-2:loop]` | 20 |
| `SET-PPRINT-DISPATCH[tier-2:pretty-printer]` | 18 |
| `MAKE-TWO-WAY-STREAM[tier-2:streams]` | 15 |
| `CLASS-OF[tier-2:clos]` | 14 |
| `BREAK[tier-2:conditions]` | 13 |
| `PHASE[deferred:complex-numbers]` | 13 |
| `PPRINT-POP[tier-2:pretty-printer]` | 13 |
| `PPRINT-FILL[tier-2:pretty-printer]` | 12 |
| `COMPUTE-RESTARTS[tier-2:conditions]` | 11 |
| `PPRINT-TABULAR[tier-2:pretty-printer]` | 11 |
| `RANDOM-STATE-P[tier-2:random]` | 11 |
| `ABORT[tier-2:conditions]` | 10 |
| `PPRINT-LINEAR[tier-2:pretty-printer]` | 10 |
| `RESTART-NAME[tier-2:conditions]` | 10 |
| `MAKE-BROADCAST-STREAM[tier-2:streams]` | 8 |
| `PPRINT-EXIT-IF-LIST-EXHAUSTED[tier-2:pretty-printer]` | 8 |
| `note[NO-FLOATING-POINT-UNDERFLOW-BY-DEFAULT]` | 8 |
| `PACKAGE-ERROR-PACKAGE[tier-2:conditions]` | 7 |
| `PPRINT-TAB[tier-2:pretty-printer]` | 7 |
| `reader[#P]` | 7 |
| `ARITHMETIC-ERROR-OPERANDS[tier-2:conditions]` | 6 |
| `ARITHMETIC-ERROR-OPERATION[tier-2:conditions]` | 6 |
| `CIS[deferred:complex-numbers]` | 6 |
| `DEFCLASS[tier-2:clos]` | 6 |
| `UPGRADED-COMPLEX-PART-TYPE[phase-3:types]` | 6 |
| `CHECK-TYPE[phase-3:types]` | 5 |
| `CONJUGATE[deferred:complex-numbers]` | 5 |
| `DEFMETHOD[tier-2:clos]` | 5 |
| `PPRINT[tier-2:pretty-printer]` | 5 |
| `STEP[tier-2:environment]` | 5 |
| `WITH-OPEN-FILE[tier-2:streams]` | 5 |
| `CELL-ERROR-NAME[tier-2:conditions]` | 4 |
| `DEFINE-CONDITION[tier-2:conditions]` | 4 |
| `DEFINE-METHOD-COMBINATION[tier-2:clos]` | 4 |
| `DEFTYPE[phase-3:types]` | 4 |
| `PPRINT-DISPATCH[tier-2:pretty-printer]` | 4 |
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
| `CLASS-NAME[tier-2:clos]` | 3 |
| `STORE-VALUE[tier-2:conditions]` | 3 |
| `COMPILE-FILE-PATHNAME[deferred:compile-file]` | 2 |
| `COMPILE-FILE[deferred:compile-file]` | 2 |
| `CONTINUE[tier-2:conditions]` | 2 |
| `DELETE-FILE[tier-2:pathnames-and-files]` | 2 |
| `NAMESTRING[tier-2:pathnames-and-files]` | 2 |
| `APROPOS-LIST[tier-2:environment]` | 1 |
| `APROPOS[tier-2:environment]` | 1 |
| `BROADCAST-STREAM-STREAMS[tier-2:streams]` | 1 |
| `CERROR[tier-2:conditions]` | 1 |
| `CLEAR-INPUT[tier-2:streams]` | 1 |
| `CLOSE[tier-2:streams]` | 1 |
| `CONCATENATED-STREAM-STREAMS[tier-2:streams]` | 1 |
| `DECODE-UNIVERSAL-TIME[tier-2:environment]` | 1 |
| `DESCRIBE[tier-2:clos]` | 1 |
| `DIRECTORY-NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `DIRECTORY[tier-2:pathnames-and-files]` | 1 |
| `DRIBBLE[tier-2:environment]` | 1 |
| `ECHO-STREAM-INPUT-STREAM[tier-2:streams]` | 1 |
| `ECHO-STREAM-OUTPUT-STREAM[tier-2:streams]` | 1 |
| `ED[tier-2:environment]` | 1 |
| `ENCODE-UNIVERSAL-TIME[tier-2:environment]` | 1 |
| `ENOUGH-NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `ENSURE-DIRECTORIES-EXIST[tier-2:pathnames-and-files]` | 1 |
| `ENSURE-GENERIC-FUNCTION[tier-2:clos]` | 1 |
| `FILE-AUTHOR[tier-2:pathnames-and-files]` | 1 |
| `FILE-ERROR-PATHNAME[tier-2:conditions]` | 1 |
| `FILE-LENGTH[tier-2:streams]` | 1 |
| `FILE-NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `FILE-POSITION[tier-2:streams]` | 1 |
| `FILE-STRING-LENGTH[tier-2:streams]` | 1 |
| `FILE-WRITE-DATE[tier-2:pathnames-and-files]` | 1 |
| `FIND-RESTART[tier-2:conditions]` | 1 |
| `GET-DECODED-TIME[tier-2:environment]` | 1 |
| `GET-INTERNAL-REAL-TIME[tier-2:environment]` | 1 |
| `GET-INTERNAL-RUN-TIME[tier-2:environment]` | 1 |
| `GET-UNIVERSAL-TIME[tier-2:environment]` | 1 |
| `HOST-NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `INSPECT[tier-2:environment]` | 1 |
| `INTERACTIVE-STREAM-P[tier-2:streams]` | 1 |
| `INVALID-METHOD-ERROR[tier-2:clos]` | 1 |
| `INVOKE-DEBUGGER[tier-2:conditions]` | 1 |
| `INVOKE-RESTART-INTERACTIVELY[tier-2:conditions]` | 1 |
| `INVOKE-RESTART[tier-2:conditions]` | 1 |
| `LISP-IMPLEMENTATION-TYPE[tier-2:environment]` | 1 |
| `LISP-IMPLEMENTATION-VERSION[tier-2:environment]` | 1 |
| `LISTEN[tier-2:streams]` | 1 |
| `LOAD-LOGICAL-PATHNAME-TRANSLATIONS[tier-2:pathnames-and-files]` | 1 |
| `LOGICAL-PATHNAME-TRANSLATIONS[tier-2:pathnames-and-files]` | 1 |
| `LONG-SITE-NAME[tier-2:environment]` | 1 |
| `MACHINE-INSTANCE[tier-2:environment]` | 1 |
| `MACHINE-TYPE[tier-2:environment]` | 1 |
| `MACHINE-VERSION[tier-2:environment]` | 1 |
| `MAKE-CONCATENATED-STREAM[tier-2:streams]` | 1 |
| `MAKE-ECHO-STREAM[tier-2:streams]` | 1 |
| `MAKE-INSTANCE[tier-2:clos]` | 1 |
| `MAKE-LOAD-FORM-SAVING-SLOTS[tier-2:clos]` | 1 |
| `MAKE-SYNONYM-STREAM[tier-2:streams]` | 1 |
| `MERGE-PATHNAMES[tier-2:pathnames-and-files]` | 1 |
| `METHOD-COMBINATION-ERROR[tier-2:clos]` | 1 |
| `OPEN-STREAM-P[tier-2:streams]` | 1 |
| `OPEN[tier-2:streams]` | 1 |
| `PARSE-NAMESTRING[tier-2:pathnames-and-files]` | 1 |
| `PATHNAME-DEVICE[tier-2:pathnames-and-files]` | 1 |
| `PATHNAME-DIRECTORY[tier-2:pathnames-and-files]` | 1 |
| `PATHNAME-HOST[tier-2:pathnames-and-files]` | 1 |
| `PATHNAME-MATCH-P[tier-2:pathnames-and-files]` | 1 |
| `PATHNAME-NAME[tier-2:pathnames-and-files]` | 1 |
| `PATHNAME-TYPE[tier-2:pathnames-and-files]` | 1 |
| `PATHNAME-VERSION[tier-2:pathnames-and-files]` | 1 |
| `PATHNAMEP[tier-2:pathnames-and-files]` | 1 |
| `PRINT-NOT-READABLE-OBJECT[tier-2:conditions]` | 1 |
| `PROBE-FILE[tier-2:pathnames-and-files]` | 1 |
| `READ-BYTE[tier-2:streams]` | 1 |
| `READ-CHAR-NO-HANG[tier-2:streams]` | 1 |
| `READ-LINE[tier-2:streams]` | 1 |
| `READ-SEQUENCE[tier-2:streams]` | 1 |
| `RENAME-FILE[tier-2:pathnames-and-files]` | 1 |
| `ROOM[tier-2:environment]` | 1 |
| `SHORT-SITE-NAME[tier-2:environment]` | 1 |
| `SIGNAL[tier-2:conditions]` | 1 |
| `SIMPLE-CONDITION-FORMAT-ARGUMENTS[tier-2:conditions]` | 1 |
| `SIMPLE-CONDITION-FORMAT-CONTROL[tier-2:conditions]` | 1 |
| `SLEEP[tier-2:environment]` | 1 |
| `SLOT-BOUNDP[tier-2:clos]` | 1 |
| `SLOT-EXISTS-P[tier-2:clos]` | 1 |
| `SLOT-MAKUNBOUND[tier-2:clos]` | 1 |
| `SLOT-VALUE[tier-2:clos]` | 1 |
| `SOFTWARE-TYPE[tier-2:environment]` | 1 |
| `SOFTWARE-VERSION[tier-2:environment]` | 1 |
| `STREAM-ELEMENT-TYPE[tier-2:streams]` | 1 |
| `STREAM-ERROR-STREAM[tier-2:conditions]` | 1 |
| `STREAM-EXTERNAL-FORMAT[tier-2:streams]` | 1 |
| `SYNONYM-STREAM-SYMBOL[tier-2:streams]` | 1 |
| `TRANSLATE-LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 1 |
| `TRANSLATE-PATHNAME[tier-2:pathnames-and-files]` | 1 |
| `TRUENAME[tier-2:pathnames-and-files]` | 1 |
| `TWO-WAY-STREAM-INPUT-STREAM[tier-2:streams]` | 1 |
| `TWO-WAY-STREAM-OUTPUT-STREAM[tier-2:streams]` | 1 |
| `UNBOUND-SLOT-INSTANCE[tier-2:conditions]` | 1 |
| `USE-VALUE[tier-2:conditions]` | 1 |
| `USER-HOMEDIR-PATHNAME[tier-2:pathnames-and-files]` | 1 |
| `WARN[tier-2:conditions]` | 1 |
| `WILD-PATHNAME-P[tier-2:pathnames-and-files]` | 1 |
| `WRITE-BYTE[tier-2:streams]` | 1 |
| `WRITE-SEQUENCE[tier-2:streams]` | 1 |
| `Y-OR-N-P[tier-2:streams]` | 1 |
| `YES-OR-NO-P[tier-2:streams]` | 1 |
| `sbcl[same-as-sbcl]` | 1 |
