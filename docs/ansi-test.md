# Quokka against ansi-test

Written by `ANSI_TEST_UPDATE=1 scripts/ansi-test.sh` (#60); do not edit.
*escaped* is an error no Lisp handler could catch, a fault in the machine;
*needs tier 1* a test that uses something Quokka lacks and tier 1 should
have (an operator, or reader syntax); *needs later* one that uses only
features deferred to a later tier or issue (`tests/ansi/deferred.lisp`).
A test can need several things.

| group | pass | fail | escaped | timeout | needs tier 1 | needs later | total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| cons | 1557 | 0 | 0 | 0 | 0 | 165 | 1722 |
| sequences | 2678 | 0 | 0 | 0 | 0 | 609 | 3287 |
| strings | 371 | 0 | 0 | 0 | 0 | 138 | 509 |
| arrays | 638 | 0 | 0 | 0 | 0 | 652 | 1290 |
| characters | 133 | 0 | 0 | 0 | 0 | 126 | 259 |
| symbols | 126 | 0 | 0 | 0 | 0 | 1019 | 1145 |
| packages | 391 | 0 | 0 | 0 | 0 | 109 | 500 |
| numbers | 570 | 0 | 0 | 0 | 0 | 874 | 1444 |
| reader | 410 | 0 | 0 | 0 | 0 | 165 | 575 |
| printer | 419 | 0 | 0 | 0 | 0 | 1450 | 1869 |
| data-and-control-flow | 1165 | 0 | 0 | 0 | 0 | 263 | 1428 |
| eval-and-compile | 276 | 0 | 0 | 0 | 0 | 42 | 318 |
| iteration | 95 | 0 | 0 | 0 | 0 | 748 | 843 |
| hash-tables | 81 | 0 | 0 | 0 | 0 | 86 | 167 |
| structures | 60 | 0 | 0 | 0 | 0 | 56 | 116 |
| **all** | **8970** | **0** | **0** | **0** | **0** | **6502** | **15472** |

## Tier-1 gaps, by how many tests each blocks

| needs | tests |
| --- | ---: |

## Deferred features, by how many tests each blocks

| needs | tests |
| --- | ---: |
| `LOOP[tier-2:loop]` | 4305 |
| `ASSERT[tier-2:conditions]` | 1390 |
| `TYPEP[phase-3:types]` | 1196 |
| `RANDOM[tier-2:random]` | 565 |
| `FIND-CLASS[tier-2:clos]` | 524 |
| `reader[#C]` | 475 |
| `DEFGENERIC[tier-2:clos]` | 403 |
| `MAKE-RANDOM-STATE[tier-2:random]` | 385 |
| `MAKE-CONDITION[tier-2:conditions]` | 383 |
| `LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 373 |
| `MAKE-PATHNAME[tier-2:pathnames-and-files]` | 373 |
| `FIND-METHOD[tier-2:clos]` | 369 |
| `HANDLER-BIND[tier-2:conditions]` | 285 |
| `TYPE-ERROR-DATUM[tier-2:conditions]` | 275 |
| `TYPE-ERROR-EXPECTED-TYPE[tier-2:conditions]` | 275 |
| `MUFFLE-WARNING[tier-2:conditions]` | 270 |
| `SUBTYPEP[phase-3:types]` | 211 |
| `COMPLEX[deferred:complex-numbers]` | 196 |
| `PPRINT-LOGICAL-BLOCK[tier-2:pretty-printer]` | 141 |
| `format-directive[tier-2:format]` | 126 |
| `ETYPECASE[phase-3:types]` | 84 |
| `REALPART[deferred:complex-numbers]` | 83 |
| `PPRINT-NEWLINE[tier-2:pretty-printer]` | 73 |
| `note[NIL-VECTORS-ARE-STRINGS]` | 57 |
| `DOCUMENTATION[tier-2:clos]` | 48 |
| `WITH-OPEN-STREAM[tier-2:streams]` | 40 |
| `PPRINT-TABULAR[tier-2:pretty-printer]` | 39 |
| `PPRINT-INDENT[tier-2:pretty-printer]` | 38 |
| `TYPECASE[phase-3:types]` | 38 |
| `IMAGPART[deferred:complex-numbers]` | 35 |
| `LOOP-FINISH[tier-2:loop]` | 35 |
| `COPY-PPRINT-DISPATCH[tier-2:pretty-printer]` | 34 |
| `PPRINT-FILL[tier-2:pretty-printer]` | 31 |
| `COMPLEXP[deferred:complex-numbers]` | 29 |
| `CTYPECASE[phase-3:types]` | 29 |
| `PATHNAME[tier-2:pathnames-and-files]` | 29 |
| `PPRINT-LINEAR[tier-2:pretty-printer]` | 29 |
| `PPRINT-TAB[tier-2:pretty-printer]` | 29 |
| `TYPE-OF[phase-3:types]` | 29 |
| `SET-PPRINT-DISPATCH[tier-2:pretty-printer]` | 26 |
| `MAKE-TWO-WAY-STREAM[tier-2:streams]` | 25 |
| `CLASS-OF[tier-2:clos]` | 24 |
| `PPRINT-POP[tier-2:pretty-printer]` | 23 |
| `BREAK[tier-2:conditions]` | 21 |
| `PHASE[deferred:complex-numbers]` | 21 |
| `UPGRADED-COMPLEX-PART-TYPE[phase-3:types]` | 20 |
| `COMPUTE-RESTARTS[tier-2:conditions]` | 19 |
| `RANDOM-STATE-P[tier-2:random]` | 19 |
| `ABORT[tier-2:conditions]` | 18 |
| `PPRINT-EXIT-IF-LIST-EXHAUSTED[tier-2:pretty-printer]` | 18 |
| `RESTART-NAME[tier-2:conditions]` | 18 |
| `MAKE-BROADCAST-STREAM[tier-2:streams]` | 16 |
| `PACKAGE-ERROR-PACKAGE[tier-2:conditions]` | 15 |
| `ARITHMETIC-ERROR-OPERANDS[tier-2:conditions]` | 14 |
| `ARITHMETIC-ERROR-OPERATION[tier-2:conditions]` | 14 |
| `CIS[deferred:complex-numbers]` | 14 |
| `DEFCLASS[tier-2:clos]` | 14 |
| `CHECK-TYPE[phase-3:types]` | 13 |
| `CONJUGATE[deferred:complex-numbers]` | 13 |
| `DEFMETHOD[tier-2:clos]` | 13 |
| `PPRINT[tier-2:pretty-printer]` | 13 |
| `STEP[tier-2:environment]` | 13 |
| `WITH-OPEN-FILE[tier-2:streams]` | 13 |
| `CELL-ERROR-NAME[tier-2:conditions]` | 12 |
| `DEFINE-CONDITION[tier-2:conditions]` | 12 |
| `DEFINE-METHOD-COMBINATION[tier-2:clos]` | 12 |
| `DEFTYPE[phase-3:types]` | 12 |
| `PPRINT-DISPATCH[tier-2:pretty-printer]` | 12 |
| `RESTART-BIND[tier-2:conditions]` | 12 |
| `RESTART-CASE[tier-2:conditions]` | 12 |
| `TIME[tier-2:environment]` | 12 |
| `TRACE[tier-2:environment]` | 12 |
| `UNTRACE[tier-2:environment]` | 12 |
| `WITH-ACCESSORS[tier-2:clos]` | 12 |
| `WITH-COMPILATION-UNIT[deferred:compile-file]` | 12 |
| `WITH-CONDITION-RESTARTS[tier-2:conditions]` | 12 |
| `WITH-SIMPLE-RESTART[tier-2:conditions]` | 12 |
| `WITH-SLOTS[tier-2:clos]` | 12 |
| `CLASS-NAME[tier-2:clos]` | 11 |
| `STORE-VALUE[tier-2:conditions]` | 11 |
| `COMPILE-FILE-PATHNAME[deferred:compile-file]` | 10 |
| `COMPILE-FILE[deferred:compile-file]` | 10 |
| `CONTINUE[tier-2:conditions]` | 10 |
| `DELETE-FILE[tier-2:pathnames-and-files]` | 10 |
| `NAMESTRING[tier-2:pathnames-and-files]` | 10 |
| `APROPOS-LIST[tier-2:environment]` | 9 |
| `APROPOS[tier-2:environment]` | 9 |
| `BROADCAST-STREAM-STREAMS[tier-2:streams]` | 9 |
| `CERROR[tier-2:conditions]` | 9 |
| `CLEAR-INPUT[tier-2:streams]` | 9 |
| `CLOSE[tier-2:streams]` | 9 |
| `CONCATENATED-STREAM-STREAMS[tier-2:streams]` | 9 |
| `DECODE-UNIVERSAL-TIME[tier-2:environment]` | 9 |
| `DESCRIBE[tier-2:clos]` | 9 |
| `DIRECTORY-NAMESTRING[tier-2:pathnames-and-files]` | 9 |
| `DIRECTORY[tier-2:pathnames-and-files]` | 9 |
| `DRIBBLE[tier-2:environment]` | 9 |
| `ECHO-STREAM-INPUT-STREAM[tier-2:streams]` | 9 |
| `ECHO-STREAM-OUTPUT-STREAM[tier-2:streams]` | 9 |
| `ENCODE-UNIVERSAL-TIME[tier-2:environment]` | 9 |
| `ENOUGH-NAMESTRING[tier-2:pathnames-and-files]` | 9 |
| `ENSURE-DIRECTORIES-EXIST[tier-2:pathnames-and-files]` | 9 |
| `ENSURE-GENERIC-FUNCTION[tier-2:clos]` | 9 |
| `FILE-AUTHOR[tier-2:pathnames-and-files]` | 9 |
| `FILE-ERROR-PATHNAME[tier-2:conditions]` | 9 |
| `FILE-LENGTH[tier-2:streams]` | 9 |
| `FILE-NAMESTRING[tier-2:pathnames-and-files]` | 9 |
| `FILE-POSITION[tier-2:streams]` | 9 |
| `FILE-STRING-LENGTH[tier-2:streams]` | 9 |
| `FILE-WRITE-DATE[tier-2:pathnames-and-files]` | 9 |
| `FIND-RESTART[tier-2:conditions]` | 9 |
| `GET-DECODED-TIME[tier-2:environment]` | 9 |
| `GET-INTERNAL-REAL-TIME[tier-2:environment]` | 9 |
| `GET-INTERNAL-RUN-TIME[tier-2:environment]` | 9 |
| `GET-UNIVERSAL-TIME[tier-2:environment]` | 9 |
| `HOST-NAMESTRING[tier-2:pathnames-and-files]` | 9 |
| `INSPECT[tier-2:environment]` | 9 |
| `INTERACTIVE-STREAM-P[tier-2:streams]` | 9 |
| `INVALID-METHOD-ERROR[tier-2:clos]` | 9 |
| `INVOKE-DEBUGGER[tier-2:conditions]` | 9 |
| `INVOKE-RESTART-INTERACTIVELY[tier-2:conditions]` | 9 |
| `INVOKE-RESTART[tier-2:conditions]` | 9 |
| `LISP-IMPLEMENTATION-TYPE[tier-2:environment]` | 9 |
| `LISP-IMPLEMENTATION-VERSION[tier-2:environment]` | 9 |
| `LISTEN[tier-2:streams]` | 9 |
| `LOAD-LOGICAL-PATHNAME-TRANSLATIONS[tier-2:pathnames-and-files]` | 9 |
| `LOGICAL-PATHNAME-TRANSLATIONS[tier-2:pathnames-and-files]` | 9 |
| `LONG-SITE-NAME[tier-2:environment]` | 9 |
| `MACHINE-INSTANCE[tier-2:environment]` | 9 |
| `MACHINE-TYPE[tier-2:environment]` | 9 |
| `MACHINE-VERSION[tier-2:environment]` | 9 |
| `MAKE-CONCATENATED-STREAM[tier-2:streams]` | 9 |
| `MAKE-ECHO-STREAM[tier-2:streams]` | 9 |
| `MAKE-INSTANCE[tier-2:clos]` | 9 |
| `MAKE-LOAD-FORM-SAVING-SLOTS[tier-2:clos]` | 9 |
| `MAKE-SYNONYM-STREAM[tier-2:streams]` | 9 |
| `MERGE-PATHNAMES[tier-2:pathnames-and-files]` | 9 |
| `METHOD-COMBINATION-ERROR[tier-2:clos]` | 9 |
| `OPEN-STREAM-P[tier-2:streams]` | 9 |
| `OPEN[tier-2:streams]` | 9 |
| `PARSE-NAMESTRING[tier-2:pathnames-and-files]` | 9 |
| `PATHNAME-DEVICE[tier-2:pathnames-and-files]` | 9 |
| `PATHNAME-DIRECTORY[tier-2:pathnames-and-files]` | 9 |
| `PATHNAME-HOST[tier-2:pathnames-and-files]` | 9 |
| `PATHNAME-MATCH-P[tier-2:pathnames-and-files]` | 9 |
| `PATHNAME-NAME[tier-2:pathnames-and-files]` | 9 |
| `PATHNAME-TYPE[tier-2:pathnames-and-files]` | 9 |
| `PATHNAME-VERSION[tier-2:pathnames-and-files]` | 9 |
| `PATHNAMEP[tier-2:pathnames-and-files]` | 9 |
| `PRINT-NOT-READABLE-OBJECT[tier-2:conditions]` | 9 |
| `PROBE-FILE[tier-2:pathnames-and-files]` | 9 |
| `READ-BYTE[tier-2:streams]` | 9 |
| `READ-CHAR-NO-HANG[tier-2:streams]` | 9 |
| `READ-LINE[tier-2:streams]` | 9 |
| `READ-SEQUENCE[tier-2:streams]` | 9 |
| `RENAME-FILE[tier-2:pathnames-and-files]` | 9 |
| `ROOM[tier-2:environment]` | 9 |
| `SHORT-SITE-NAME[tier-2:environment]` | 9 |
| `SIGNAL[tier-2:conditions]` | 9 |
| `SIMPLE-CONDITION-FORMAT-ARGUMENTS[tier-2:conditions]` | 9 |
| `SIMPLE-CONDITION-FORMAT-CONTROL[tier-2:conditions]` | 9 |
| `SLEEP[tier-2:environment]` | 9 |
| `SLOT-BOUNDP[tier-2:clos]` | 9 |
| `SLOT-EXISTS-P[tier-2:clos]` | 9 |
| `SLOT-MAKUNBOUND[tier-2:clos]` | 9 |
| `SLOT-VALUE[tier-2:clos]` | 9 |
| `SOFTWARE-TYPE[tier-2:environment]` | 9 |
| `SOFTWARE-VERSION[tier-2:environment]` | 9 |
| `STREAM-ELEMENT-TYPE[tier-2:streams]` | 9 |
| `STREAM-ERROR-STREAM[tier-2:conditions]` | 9 |
| `STREAM-EXTERNAL-FORMAT[tier-2:streams]` | 9 |
| `SYNONYM-STREAM-SYMBOL[tier-2:streams]` | 9 |
| `TRANSLATE-LOGICAL-PATHNAME[tier-2:pathnames-and-files]` | 9 |
| `TRANSLATE-PATHNAME[tier-2:pathnames-and-files]` | 9 |
| `TRUENAME[tier-2:pathnames-and-files]` | 9 |
| `TWO-WAY-STREAM-INPUT-STREAM[tier-2:streams]` | 9 |
| `TWO-WAY-STREAM-OUTPUT-STREAM[tier-2:streams]` | 9 |
| `UNBOUND-SLOT-INSTANCE[tier-2:conditions]` | 9 |
| `USE-VALUE[tier-2:conditions]` | 9 |
| `USER-HOMEDIR-PATHNAME[tier-2:pathnames-and-files]` | 9 |
| `WARN[tier-2:conditions]` | 9 |
| `WILD-PATHNAME-P[tier-2:pathnames-and-files]` | 9 |
| `WRITE-BYTE[tier-2:streams]` | 9 |
| `WRITE-SEQUENCE[tier-2:streams]` | 9 |
| `Y-OR-N-P[tier-2:streams]` | 9 |
| `YES-OR-NO-P[tier-2:streams]` | 9 |
| `ADD-METHOD[tier-2:clos]` | 8 |
| `ALLOCATE-INSTANCE[tier-2:clos]` | 8 |
| `CALL-METHOD[tier-2:clos]` | 8 |
| `CHANGE-CLASS[tier-2:clos]` | 8 |
| `COMPUTE-APPLICABLE-METHODS[tier-2:clos]` | 8 |
| `DESCRIBE-OBJECT[tier-2:clos]` | 8 |
| `ED[tier-2:environment]` | 8 |
| `FUNCTION-KEYWORDS[tier-2:clos]` | 8 |
| `INITIALIZE-INSTANCE[tier-2:clos]` | 8 |
| `MAKE-INSTANCES-OBSOLETE[tier-2:clos]` | 8 |
| `MAKE-LOAD-FORM[tier-2:clos]` | 8 |
| `METHOD-QUALIFIERS[tier-2:clos]` | 8 |
| `NO-APPLICABLE-METHOD[tier-2:clos]` | 8 |
| `NO-NEXT-METHOD[tier-2:clos]` | 8 |
| `PRINT-OBJECT[tier-2:clos]` | 8 |
| `REINITIALIZE-INSTANCE[tier-2:clos]` | 8 |
| `REMOVE-METHOD[tier-2:clos]` | 8 |
| `SHARED-INITIALIZE[tier-2:clos]` | 8 |
| `SLOT-MISSING[tier-2:clos]` | 8 |
| `SLOT-UNBOUND[tier-2:clos]` | 8 |
| `UPDATE-INSTANCE-FOR-DIFFERENT-CLASS[tier-2:clos]` | 8 |
| `UPDATE-INSTANCE-FOR-REDEFINED-CLASS[tier-2:clos]` | 8 |
| `note[NO-FLOATING-POINT-UNDERFLOW-BY-DEFAULT]` | 8 |
| `reader[#P]` | 7 |
| `sbcl[same-as-sbcl]` | 1 |
