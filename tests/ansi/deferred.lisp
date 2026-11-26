;;;; What Quokka lacks on purpose, for the ansi-test runner (#60, decision 3).
;;;;
;;;; A test that uses one of these operators is reported as needing it, with
;;;; the reason given here, and that counts as explained: the feature belongs
;;;; to a later tier (docs/design.md's tier table) or to an issue of its own.
;;;; An operator Quokka lacks that is NOT listed here is reported as a gap in
;;;; tier 1, which phase 2 has to close or argue onto this list.

(in-package :regression-test)

(%deferred "tier-2:loop" :loop :loop-finish)

(%deferred "tier-2:clos" :function-keywords :documentation)

(%deferred "tier-2:conditions"
  :handler-bind :signal :warn :define-condition :make-condition :restart-case
  :restart-bind :invoke-restart :invoke-restart-interactively :find-restart
  :compute-restarts :restart-name :with-simple-restart :with-condition-restarts
  :abort :continue :muffle-warning :store-value :use-value :break :invoke-debugger
  :cerror :assert :simple-condition-format-control :simple-condition-format-arguments
  :type-error-datum :type-error-expected-type :cell-error-name :package-error-package
  :stream-error-stream :file-error-pathname :print-not-readable-object
  :arithmetic-error-operation :arithmetic-error-operands :unbound-slot-instance)

(%deferred "tier-2:streams"
  :open :close :with-open-file :with-open-stream :read-line :read-byte :write-byte
  :make-two-way-stream :make-echo-stream :make-broadcast-stream
  :make-concatenated-stream :make-synonym-stream :two-way-stream-input-stream
  :two-way-stream-output-stream :echo-stream-input-stream :echo-stream-output-stream
  :broadcast-stream-streams :concatenated-stream-streams :synonym-stream-symbol
  :file-length :file-position :file-string-length :listen :clear-input :clear-output
  :finish-output :force-output :interactive-stream-p :open-stream-p :input-stream-p
  :output-stream-p :stream-element-type :stream-external-format :read-char-no-hang
  :read-sequence :write-sequence :y-or-n-p :yes-or-no-p)

(%deferred "tier-2:pretty-printer"
  :pprint :pprint-dispatch :set-pprint-dispatch :copy-pprint-dispatch :pprint-fill
  :pprint-linear :pprint-tabular :pprint-indent :pprint-newline :pprint-tab
  :pprint-logical-block :pprint-pop :pprint-exit-if-list-exhausted)

(%deferred "tier-2:pathnames-and-files"
  :pathname :make-pathname :pathnamep :pathname-host :pathname-device
  :pathname-directory :pathname-name :pathname-type :pathname-version :namestring
  :file-namestring :directory-namestring :host-namestring :enough-namestring
  :parse-namestring :merge-pathnames :wild-pathname-p :pathname-match-p
  :translate-pathname :logical-pathname :logical-pathname-translations
  :load-logical-pathname-translations :translate-logical-pathname :probe-file
  :truename :delete-file :rename-file :directory :ensure-directories-exist
  :file-write-date :file-author :user-homedir-pathname)

(%deferred "deferred:complex-numbers"
  :complex :complexp :realpart :imagpart :cis :phase :conjugate
  :upgraded-complex-part-type)

(%deferred "deferred:compile-file" :compile-file :compile-file-pathname :with-compilation-unit)

(%deferred "tier-2:environment"
  :get-universal-time :get-decoded-time :decode-universal-time :encode-universal-time
  :get-internal-real-time :get-internal-run-time :sleep :room :ed :inspect :dribble
  :apropos :apropos-list :time :trace :untrace :step :machine-type :machine-version
  :machine-instance :software-type :software-version :lisp-implementation-type
  :lisp-implementation-version :short-site-name :long-site-name)

(%deferred "tier-2:random" :random :make-random-state :random-state-p)

;;; Tests where SBCL, the oracle, gives the answer Quokka gives and the suite
;;; expects another (#81). Each was checked by running the test's form in
;;; SBCL 2.6.
;;;
;;; print.array.level.8: an array nested too deep for *print-level*. SBCL
;;; prints `#2A((#2A#))`, its rank and then `#` for the contents; the suite
;;; expects `#2A((#))`.
(%follows-sbcl "same-as-sbcl" :print.array.level.8)

;;; typep.error.7: `(typep nil '(function () t))`. CLHS says `typep`
;;; refuses a function type specifier; SBCL refuses it only when the object
;;; is a function, and answers nil for nil (#124).
(%follows-sbcl "same-as-sbcl" :typep.error.7)

;;; From the objects group (#124, CLOS sub-step 7):
;;;
;;; class-04.error.3: `(s1-w)`, a function nothing defines, is expected to
;;; signal a program-error; SBCL signals undefined-function, as Quokka does.
(%follows-sbcl "same-as-sbcl" :class-04.error.3)

;;; call-next-method.error.1 and .2: `call-next-method` with arguments for
;;; which other methods would apply. CLHS says an error "should be
;;; signaled"; SBCL does not check, and calls the next method of the
;;; original arguments, as Quokka does.
(%follows-sbcl "same-as-sbcl" :call-next-method.error.1 :call-next-method.error.2)

;;; shared-initialize.error.4: `(shared-initialize obj nil '(a b c) nil)`,
;;; a list where an initarg's name should be. SBCL's `shared-initialize`
;;; does not check its initargs (`make-instance` does), and returns the
;;; instance, as Quokka's does.
(%follows-sbcl "same-as-sbcl" :shared-initialize.error.4)
