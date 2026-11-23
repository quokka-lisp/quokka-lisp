;;;; Basic format: the directives step K settled on (#57).
;;;;
;;;; Tier 1, so this file runs compiled and against SBCL, and not
;;;; interpreted. Its baseline came from SBCL through scripts/oracle.lisp.

;; Destinations: nil makes a string, a stream is written to.
(format nil "plain text")
(with-output-to-string (s) (format s "to ~A" "a stream"))
(let ((s (make-string-output-stream))) (list (format s "x") (get-output-stream-string s)))

;; ~A and ~S, with padding.
(format nil "~A and ~S" "text" "text")
(format nil "~A ~S ~A" 'sym #\c '(1 "two" #\3))
(format nil "[~10A]" "left")
(format nil "[~10@A]" "right")
(format nil "[~10,,,'*A]" "stars")
(format nil "[~5,3A]" "ab")
(format nil "[~,,4A]" "pad")
(format nil "~A ~:A" nil nil)
(format nil "[~8S]" "q")

;; Integers in a radix: ~D ~B ~O ~X ~nR.
(format nil "~D ~B ~O ~X" 255 255 255 255)
(format nil "~D ~@D ~D" 42 42 -42)
(format nil "[~6D] [~6,'0D] [~,,'.,4:D]" 42 42 123456789)
(format nil "~:D ~:D ~:D" 1234567 -1234 12)
(format nil "~2R ~8R ~16R ~36R" 10 10 255 35)
(format nil "~16,8,'0R" 255)
(format nil "~D ~5D" 'not-a-number "str")

;; Characters.
(format nil "~C~C~C" #\a #\Space #\b)
(format nil "~:C ~:C ~:C" #\a #\Space #\Newline)
(format nil "~@C ~@C" #\a #\Space)

;; Newlines and tildes.
(format nil "a~%b~2%c")
(format nil "~&a~&~&b")
(format nil "x~3~y")
(format nil "one ~
            two")
(format nil "one ~:
   two")

;; Plurals.
(format nil "~D item~:P, ~D item~:P, ~D item~:P" 0 1 2)
(format nil "~D fl~:@P and ~D fl~:@P" 1 3)
(format nil "~P~P" 1 5)

;; Moving among the arguments.
(format nil "~A ~* ~A" 1 2 3)
(format nil "~A ~:* ~A" 'again)
(format nil "~A ~A ~1@* ~A" 1 2)
(format nil "~2*~A" 1 2 3)

;; Conditionals.
(format nil "~[zero~;one~;two~]" 1)
(format nil "~[zero~;one~:;many~]" 7)
(format nil "~[zero~;one~]" 5)
(format nil "~:[false~;true~]" nil)
(format nil "~:[false~;true~]" 'yes)
(format nil "~@[(~A)~] and ~A" 5 'next)
(format nil "~@[(~A)~] and ~A" nil 'next)
(format nil "~1[a~;b~;c~]")
(format nil "~#[none~;one~;two~:;many~]" 1 2 3 4)

;; Iteration, with ~^.
(format nil "~{~A~^, ~}" '(1 2 3))
(format nil "~{~A~}" nil)
(format nil "~:{(~A ~A)~}" '((a 1) (b 2)))
(format nil "~@{~A~^-~}" 'x 'y 'z)
(format nil "~:@{~A=~A~^ ~}" '(a 1) '(b 2))
(format nil "~2{~A~}" '(1 2 3 4))
(format nil "~{x~:}" nil)
(format nil "~{~A ~A~^; ~}" '(a 1 b 2))
(format nil "~{[~{~A~^ ~}]~^ ~}" '((1 2) (3) ()))
(format nil "~:{~A~:^,~}" '((1) (2) (3)))
(format nil "~A~^ and ~A" 'alone)

;; Case conversion.
(format nil "~(Hello World~)")
(format nil "~:(hello wide world~)")
(format nil "~@(hello WIDE world~)")
(format nil "~:@(shout~)")
(format nil "~(~A and ~A~)" 'ABC "DeF")

;; Indirection.
(format nil "~? end" "<~A ~A>" '(1 2))
(format nil "~@? end" "<~A>" 'x)

;; V and # parameters.
(format nil "[~vA]" 6 "v")
(format nil "[~v,'xD]" 5 7)
(format nil "~#*~A" 'a 'b)

;; formatter.
(funcall (formatter "~A-~A") (make-string-output-stream) 1 2 3)
(with-output-to-string (s) (format s (formatter "~D+~D") 3 4))

;; Errors in the control string or the arguments.
(handler-case (format nil "~A") (error () 'too-few-arguments))
(handler-case (format nil "~{~A") (error () 'unclosed))
