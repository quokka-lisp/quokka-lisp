;;;; Drawing (#143, phase 4 step I; docs/design.md, *Drawing, commands and
;;;; redisplay*).
;;;;
;;;; The renderer's other half. Text goes to the DOM, where the browser lays
;;;; it out, selects it and reads it aloud; a picture goes to SVG, where each
;;;; shape is an element of its own. So a shape is an output record like a
;;;; run of text: it has an identity, it remembers the function that drew it
;;;; (phase 5 will make it a presentation, a disk you can click), and
;;;; redisplay keeps it, and sends only what changed, when a viewer draws
;;;; itself again (chapter 96).
;;;;
;;;; The vocabulary is CLIM's: `with-drawing` gives a picture a box of its
;;;; own, in logical pixels, one to a CSS pixel; `draw-rectangle*`,
;;;; `draw-circle*` and the rest draw in it, with keyword options for the ink,
;;;; the line thickness and whether a shape is filled; `with-translation` and
;;;; `with-scaling` are CLIM's affine transformations, which SVG takes as
;;;; they are. The drawing functions take no stream, as in the tutorials:
;;;; they draw into the picture `with-drawing` is making, `*drawing*`.

;;; A picture: a box WIDTH by HEIGHT, which the renderer fits into its
;;; place keeping its proportions (`:scale :fit`), stretches to fill it
;;; (`:stretch`), or shows at its size (nil).
(defclass drawing-record (container-record) ())
(defmethod record-kind ((record drawing-record)) :drawing)

;;; A group of shapes under a transformation, `(a b c d e f)`, the matrix
;;; SVG's `transform` and the canvas's `setTransform` take.
(defclass group-record (container-record) ())
(defmethod record-kind ((record group-record)) :group)

;;; One shape; its properties say which and where.
(defclass shape-record (output-record) ())
(defmethod record-kind ((record shape-record)) :shape)

(defvar *drawing* nil)

;;; `(with-drawing (stream :width w :height h :scale :fit) body...)`: BODY
;;; draws into a new picture, written to STREAM (t or nil:
;;; `*standard-output*`).
(defmacro with-drawing ((stream &key (width 100) (height 100) (scale :fit)) &body body)
  `(%call-with-drawing ,stream ,width ,height ,scale (lambda () ,@body)))

(defun %call-with-drawing (stream width height scale thunk)
  (let* ((stream (if (member stream '(t nil)) *standard-output* stream))
         (drawing (make-instance 'drawing-record
                                 :properties (list :width (%coordinate width)
                                                   :height (%coordinate height)
                                                   :scale scale))))
    (let ((*drawing* drawing))
      (funcall thunk))
    (%add-drawing stream drawing)
    drawing))

;;; Put the picture where STREAM writes, after the text so far. A stream
;;; that is not a record stream says which record stream it writes through
;;; (`stream-record-stream`): the listener's, the entry that is running.
(defgeneric stream-record-stream (stream))
(defmethod stream-record-stream ((stream t)) nil)
(defmethod stream-record-stream ((stream record-stream)) stream)

(defun %add-drawing (stream drawing)
  (let ((target (stream-record-stream stream)))
    (unless target (error "~S cannot show a drawing" stream))
    (add-record (stream-container target) drawing)
    (setf (%stream-current target) nil)))

;;; A number as the renderer reads it: an integer, or a double; a ratio
;;; such as 1/3, which the page could not read, becomes the double it
;;; stands for.
(defun %coordinate (x)
  (cond ((integerp x) x)
        ((realp x) (float x))
        (t (error 'type-error :datum x :expected-type 'real))))

(defun %drawing-target ()
  (or *drawing* (error "drawing outside with-drawing")))

(defun %add-shape (type options &rest geometry)
  (let ((properties (list* :type type geometry)))
    (do ((tail options (cddr tail)))
        ((endp tail))
      (let ((value (cadr tail)))
        (when value
          (setq properties
                (append properties
                        (list (car tail)
                              (if (realp value) (%coordinate value) value)))))))
    (add-record (%drawing-target) (make-instance 'shape-record :properties properties))))

;;; The options every shape takes: its ink, the thickness of its line, and
;;; whether it is filled (the default for closed shapes, as in CLIM).
(defun draw-rectangle* (x1 y1 x2 y2 &key (ink +ink+) (filled t) line-thickness corner-radius)
  (%add-shape :rectangle (list :ink ink :filled filled :line-thickness line-thickness
                           :corner-radius corner-radius)
          :x (%coordinate (min x1 x2)) :y (%coordinate (min y1 y2))
          :width (%coordinate (abs (- x2 x1))) :height (%coordinate (abs (- y2 y1)))))

(defun draw-circle* (cx cy radius &key (ink +ink+) (filled t) line-thickness)
  (%add-shape :ellipse (list :ink ink :filled filled :line-thickness line-thickness)
          :cx (%coordinate cx) :cy (%coordinate cy)
          :rx (%coordinate radius) :ry (%coordinate radius)))

(defun draw-ellipse* (cx cy rx ry &key (ink +ink+) (filled t) line-thickness)
  (%add-shape :ellipse (list :ink ink :filled filled :line-thickness line-thickness)
          :cx (%coordinate cx) :cy (%coordinate cy)
          :rx (%coordinate rx) :ry (%coordinate ry)))

(defun draw-line* (x1 y1 x2 y2 &key (ink +ink+) line-thickness)
  (%add-shape :line (list :ink ink :line-thickness line-thickness)
          :points (mapcar #'%coordinate (list x1 y1 x2 y2))))

;;; POINTS is a list `(x y x y ...)`, as CLIM's `draw-polygon*` takes.
(defun draw-polyline* (points &key (ink +ink+) line-thickness)
  (%add-shape :polyline (list :ink ink :line-thickness line-thickness)
          :points (mapcar #'%coordinate points)))

(defun draw-polygon* (points &key (ink +ink+) (filled t) line-thickness)
  (%add-shape :polygon (list :ink ink :filled filled :line-thickness line-thickness)
          :points (mapcar #'%coordinate points)))

;;; Text at X, Y: its baseline starts there, or is centred or ends there
;;; with `:align :center` or `:end`. Text in a scaled picture scales with
;;; it (design, *Coordinates*).
(defun draw-text* (string x y &key (ink +ink+) (align :start) text-style)
  (%add-shape :text (list :ink ink :filled t :align align :style text-style)
          :x (%coordinate x) :y (%coordinate y) :text (princ-to-string string)))

;;; ------------------------------------------------------------------------
;;; Transformations
;;;
;;; CLIM's affine transformations, as the matrix `(a b c d e f)` that takes
;;; a point (x, y) to (ax + cy + e, bx + dy + f). Shapes drawn in BODY are
;;; in a group under it; groups nest, and the renderer composes them, as
;;; SVG does.

(defmacro with-translation ((dx dy) &body body)
  `(%call-with-transformation (list 1 0 0 1 ,dx ,dy) (lambda () ,@body)))

(defmacro with-scaling ((sx &optional sy) &body body)
  (let ((x (gensym "SX")))
    `(let ((,x ,sx))
       (%call-with-transformation (list ,x 0 0 (or ,sy ,x) 0 0) (lambda () ,@body)))))

(defun %call-with-transformation (matrix thunk)
  (let ((group (make-instance 'group-record
                              :properties (list :transform (mapcar #'%coordinate matrix)))))
    (add-record (%drawing-target) group)
    (let ((*drawing* group))
      (funcall thunk))
    group))
