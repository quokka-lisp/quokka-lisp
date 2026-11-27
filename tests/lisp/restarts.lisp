;;; Restarts (phase 3 step B, sub-step 4, #125): restart-case, restart-bind,
;;; finding and invoking restarts, the standard restart functions, warn,
;;; cerror, assert, check-type, ccase and ctypecase, and *debugger-hook*.

;; restart-case and invoke-restart.
(restart-case (invoke-restart 'rs-one 1 2) (rs-one (a b) (list :one a b)))
(restart-case (progn (invoke-restart 'rs-two) :not-here) (rs-one () :one) (rs-two () :two))
(restart-case 42 (rs-one () :one))
(multiple-value-list (restart-case (values 1 2) (rs-one () :one)))
(restart-case (restart-case (invoke-restart 'rs-n) (rs-n () :inner)) (rs-n () :outer))

;; Finding restarts.
(restart-case (mapcar #'restart-name (subseq (compute-restarts) 0 2)) (rs-a () 1) (rs-b () 2))
(restart-case (restart-name (find-restart 'rs-b)) (rs-a () 1) (rs-b () 2))
(find-restart 'rs-nowhere)
(restart-case (list (not (null (find-restart 'rs-t)))) (rs-t () :test (lambda (c) c nil) 1))
(restart-bind ((rs-bound (lambda (x) (* x 10)))) (invoke-restart 'rs-bound 4))
(restart-case (invoke-restart-interactively 'rs-i)
  (rs-i (&rest args) :interactive (lambda () (list 1 2)) args))

;; Reports.
(restart-case (princ-to-string (find-restart 'rs-a)) (rs-a () :report "Do the A thing." 1))
(restart-case (princ-to-string (find-restart 'rs-a))
  (rs-a () :report (lambda (s) (format s "rep ~A" 7)) 1))
(restart-case (let ((s (prin1-to-string (find-restart 'rs-p)))) (string= (subseq s 0 2) "#<"))
  (rs-p () 1))

;; A handler chooses a restart.
(handler-bind ((error (lambda (c) c (invoke-restart 'rs-use 99))))
  (restart-case (error "x") (rs-use (v) (list :used v))))
(handler-bind ((error (lambda (c) (use-value 5 c))))
  (restart-case (error "x") (use-value (v) (* v 2))))
(handler-bind ((error (lambda (c) (store-value 6 c))))
  (restart-case (error "x") (store-value (v) (* v 3))))
(handler-case (invoke-restart 'rs-none) (control-error () :control-error))

;; with-simple-restart.
(multiple-value-list (with-simple-restart (rs-skip "Skip it.") (invoke-restart 'rs-skip)))
(multiple-value-list (with-simple-restart (rs-skip "Skip it.") 5))

;; Restarts associated with a condition.
(let ((c1 (make-condition 'simple-error :format-control "one"))
      (c2 (make-condition 'simple-error :format-control "two")))
  (restart-case (with-condition-restarts c1 (list (find-restart 'rs-assoc))
                  (list (not (null (find-restart 'rs-assoc c1)))
                        (not (null (find-restart 'rs-assoc c2)))
                        (not (null (find-restart 'rs-assoc)))))
    (rs-assoc () nil)))
(handler-bind ((error (lambda (c)
                        (throw :rs (list (not (null (find-restart 'rs-x c)))
                                         (not (null (find-restart 'rs-x (make-condition 'error)))))))))
  (catch :rs (restart-case (error "y") (rs-x () nil))))

;; cerror, warn, muffle-warning.
(handler-bind ((error (lambda (c) (continue c)))) (list (cerror "Go on." "trouble ~A" 1) :after))
(handler-bind ((warning (lambda (c) (muffle-warning c)))) (list (warn "careful ~A" 2) :after))
(handler-case (warn 'simple-warning :format-control "w") (warning (c) (type-of c)))
(handler-case (warn "x ~A" 3) (warning (c) (princ-to-string c)))
(handler-case (warn 'simple-error :format-control "not a warning") (type-error () :type-error))

;; check-type, ccase, ctypecase and their store-value restart.
(let ((x (read-from-string "a")))
  (handler-bind ((type-error (lambda (c) (store-value 5 c)))) (check-type x integer))
  x)
(handler-case (let ((x (read-from-string "a"))) (check-type x integer))
  (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(let ((k (read-from-string "3"))) (handler-bind ((type-error (lambda (c) (store-value 1 c)))) (ccase k (1 :one) (2 :two))))
(let ((k (read-from-string "\"s\"")))
  (handler-bind ((type-error (lambda (c) (store-value 'sym c))))
    (ctypecase k (symbol (list :symbol k)) (integer :int))))
(handler-case (ecase 5 (1 :one) ((2 3) :two))
  (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(handler-case (etypecase 5 (symbol 1) (string 2))
  (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))

;; assert and its continue restart.
(let ((n 0)) (handler-bind ((error (lambda (c) (incf n) (continue c)))) (assert (> n 2))) n)
(handler-case (assert (= 1 2)) (simple-error () :failed))
(handler-case (assert nil () 'type-error :datum 1 :expected-type 'list) (type-error () :typed))

*break-on-signals*
;; *debugger-hook* is tested in differential.rs: SBCL under --script calls
;; its own hook first, which ends the process.

;; A restart object found with its condition is invoked without its test
;; being asked again.
(restart-case (handler-bind ((error (lambda (c) (invoke-restart (find-restart 'rs-tested c)))))
                (error "x"))
  (rs-tested () :test (lambda (c) c) 42))
;; abort and muffle-warning do not return.
(restart-bind ((abort (lambda () 7))) (handler-case (abort) (control-error () :control-error)))
;; ecase and etypecase have no catch-all clause.
(handler-case (ecase (read-from-string "5") (t :k))
  (type-error (c) (list (type-error-datum c) (type-error-expected-type c))))
(ecase (read-from-string "t") (t :t-key))
(handler-case (etypecase (read-from-string "5") (otherwise :o)) (error () :error))
;; assert's report names its places.
(let ((x 1))
  (handler-bind ((error (lambda (c) (throw :rs (princ-to-string (find-restart 'continue c))))))
    (catch :rs (assert (> x 2) (x)))))
