;;; Keywords.
;;;
;;; Quokka has no packages (tier 1, phase 2), but it has the part of one that
;;; keywords need: a symbol is identified by its namespace *and* its name, so
;;; `foo` and `:foo` share a name and are different symbols. Everything below
;;; is specified behaviour, and the point of the file is that SBCL — which has
;;; a real package system — gives the same answers.

;; A keyword evaluates to itself. It is a constant variable bound to itself,
;; so this needs nothing from the evaluator.
:foo
:a
:hello-world

;; Self-evaluating means no quote is needed, and quoting changes nothing.
(eq :foo ':foo)
(eq :foo :foo)

;; Interning: reading the same text twice gives the same object.
(eq :abc :abc)

;; But a keyword is NOT the ordinary symbol of the same name.
(eq :foo 'foo)

;; ...and they have the same name, because the colon is punctuation.
(symbol-name :foo)
(symbol-name 'foo)

;; (`equal` would say so in one line, but string comparison does not exist
;; yet — the two results above are the claim.)

;; Keywords are symbols.
(symbolp :foo)
(keywordp :foo)
(keywordp 'foo)
(keywordp nil)
(keywordp 1)
(keywordp '(:a))

;; Case is folded on the way in, like any other symbol.
(eq :foo :FOO)
(symbol-name :FoO)

;; In data, which is where they will mostly be used until &key exists.
'(:a 1 :b 2)
(list :a 1 :b 2)
(car '(:a . :b))
(cdr '(:a . :b))
(length '(:a 1 :b 2))

;; nil and t are ordinary symbols, not keywords, despite also being constants.
(keywordp t)
(symbol-name t)
(symbol-name nil)
