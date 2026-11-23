;;; Step V (#69): integer bit fields and the logical operations. Tier 1:
;;; compiled and SBCL only. A byte specifier is never printed: what one is
;;; is each implementation's own.

;; The logical operations, on small and negative integers and bignums.
(list (logeqv 12 10) (lognand 12 10) (lognor 12 10) (logandc1 12 10)
      (logandc2 12 10) (logorc1 12 10) (logorc2 12 10))
(list (logeqv -5 3) (lognand -1 7) (lognor 0 0) (logandc1 -8 255) (logorc2 1 -2))
(list (logeqv) (logeqv 5) (logeqv 1 2 3))
(list (logtest 12 3) (logtest 12 4) (logtest -1 (expt 2 100)))
(logandc2 (1- (expt 2 70)) (expt 2 65))

;; boole: every operation, on 12 and 10.
(list boole-clr boole-set boole-1 boole-2 boole-c1 boole-c2 boole-and boole-ior
      boole-xor boole-eqv boole-nand boole-nor boole-andc1 boole-andc2 boole-orc1 boole-orc2)
(mapcar (lambda (op) (boole op 12 10))
        (list boole-clr boole-set boole-1 boole-2 boole-c1 boole-c2 boole-and boole-ior
              boole-xor boole-eqv boole-nand boole-nor boole-andc1 boole-andc2
              boole-orc1 boole-orc2))

;; Byte specifiers and the field operations.
(let ((b (byte 4 8))) (list (byte-size b) (byte-position b)))
(list (ldb (byte 4 4) #xABCD) (ldb (byte 8 0) -1) (ldb (byte 0 3) 255)
      (ldb (byte 16 60) (expt 2 70)) (ldb (byte 3 2) -12))
(list (mask-field (byte 4 4) #xABCD) (mask-field (byte 8 0) -2))
(list (ldb-test (byte 2 1) 4) (ldb-test (byte 2 3) 4))
(list (dpb 0 (byte 4 4) #xABCD) (dpb #xF (byte 4 0) 0) (dpb -1 (byte 3 2) 0)
      (dpb 1 (byte 1 100) 0) (dpb 5 (byte 2 0) -16))
(list (deposit-field #xFF (byte 4 4) 0) (deposit-field 0 (byte 8 8) -1))

;; setf of ldb and mask-field, through any place.
(let ((x #x0F)) (setf (ldb (byte 4 4) x) #xA) x)
(let ((v (vector 0 255))) (setf (ldb (byte 2 0) (aref v 1)) 0) v)
(let ((x 0)) (list (setf (mask-field (byte 4 4) x) #xFF) x))
(let ((cell (list 0))) (incf (ldb (byte 4 0) (car cell)) 3) cell)
