;;;; What Quokka lacks on purpose, for the ansi-test runner (#60, decision 3).
;;;;
;;;; A test that uses one of these operators is reported as needing it, with
;;;; the reason given here, and that counts as explained: the feature belongs
;;;; to a later tier (docs/design.md's tier table) or to an issue of its own.
;;;; An operator Quokka lacks that is NOT listed here is reported as a gap in
;;;; tier 1, which phase 2 has to close or argue onto this list.

(in-package :regression-test)

(%deferred "tier-2:loop" :loop :loop-finish)

(%deferred "tier-2:clos"
  :defclass :defgeneric :defmethod :make-instance :make-instances-obsolete :slot-value
  :slot-boundp :slot-makunbound :slot-exists-p :slot-missing :slot-unbound :class-of
  :find-class :class-name :call-next-method :next-method-p :change-class
  :reinitialize-instance :shared-initialize :initialize-instance :allocate-instance
  :update-instance-for-different-class :update-instance-for-redefined-class :with-slots
  :with-accessors :add-method :remove-method :find-method :compute-applicable-methods
  :ensure-generic-function :method-qualifiers :no-applicable-method :no-next-method
  :invalid-method-error :method-combination-error :define-method-combination
  :print-object :describe-object :make-load-form :make-load-form-saving-slots
  :function-keywords :documentation :describe)

(%deferred "tier-2:conditions"
  :handler-bind :signal :warn :define-condition :make-condition :restart-case
  :restart-bind :invoke-restart :invoke-restart-interactively :find-restart
  :compute-restarts :restart-name :with-simple-restart :with-condition-restarts
  :abort :continue :muffle-warning :store-value :use-value :break :invoke-debugger
  :cerror :assert :simple-condition-format-control :simple-condition-format-arguments
  :type-error-datum :type-error-expected-type :cell-error-name :package-error-package
  :stream-error-stream :file-error-pathname :print-not-readable-object
  :arithmetic-error-operation :arithmetic-error-operands :unbound-slot-instance)

(%deferred "phase-3:types"
  :typep :subtypep :type-of :deftype :typecase :etypecase :ctypecase :check-type
  :upgraded-array-element-type :upgraded-complex-part-type)

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
  :complex :complexp :realpart :imagpart :cis :phase :conjugate)

(%deferred "issue-59:ratios" :numerator :denominator :rational :rationalize)

(%deferred "deferred:compile-file" :compile-file :compile-file-pathname :with-compilation-unit)

(%deferred "tier-2:arrays"
  :array-rank :array-dimensions :array-dimension :array-total-size :array-in-bounds-p
  :array-row-major-index :row-major-aref :adjust-array :adjustable-array-p
  :array-displacement :array-has-fill-pointer-p :fill-pointer :vector-push
  :vector-push-extend :vector-pop :array-element-type :bit :sbit :bit-and :bit-andc1
  :bit-andc2 :bit-eqv :bit-ior :bit-nand :bit-nor :bit-not :bit-orc1 :bit-orc2
  :bit-xor :bit-vector-p :simple-bit-vector-p)

(%deferred "tier-2:environment"
  :get-universal-time :get-decoded-time :decode-universal-time :encode-universal-time
  :get-internal-real-time :get-internal-run-time :sleep :room :ed :inspect :dribble
  :apropos :apropos-list :time :trace :untrace :step :machine-type :machine-version
  :machine-instance :software-type :software-version :lisp-implementation-type
  :lisp-implementation-version :short-site-name :long-site-name)

(%deferred "tier-2:random" :random :make-random-state :random-state-p)
