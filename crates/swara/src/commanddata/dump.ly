% Swara's command-data dump. Swara runs this file with SWARA_DUMP_OUT naming the output file, and
% it writes one JSON record per line describing what this LilyPond defines: music functions,
% markup commands, the variables in the parser's module, grobs, interfaces, properties, contexts,
% translators, music types, event classes, note-name languages and drum names.
%
% Part of Swara, under the MIT licence. Everything is wrapped in one `let`, so the dump adds no
% bindings of its own to the module it lists.

#(let ()
  (define out (open-output-file (getenv "SWARA_DUMP_OUT")))
  ;; Under LC_ALL=C the port would write a non-ASCII letter, such as the one in "español", as `?`.
  (set-port-encoding! out "UTF-8")

  (define (json-string s)
    (display "\"" out)
    (string-for-each
     (lambda (c)
       (case c
         ((#\") (display "\\\"" out))
         ((#\\) (display "\\\\" out))
         ((#\newline) (display "\\n" out))
         ((#\tab) (display "\\t" out))
         ((#\return) (display "\\r" out))
         (else
          (if (< (char->integer c) 32)
              (begin
                (display "\\u" out)
                (display (string-pad (number->string (char->integer c) 16) 4 #\0) out))
              (display c out)))))
     s)
    (display "\"" out))

  ;; Stands for JSON's null, which no Scheme value is.
  (define null (list 'null))

  ;; A JSON value from a Scheme one: a symbol is its name and a list is an array.
  (define (json v)
    (cond
     ((eq? v null) (display "null" out))
     ((eq? v #f) (display "false" out))
     ((eq? v #t) (display "true" out))
     ((string? v) (json-string v))
     ((symbol? v) (json-string (symbol->string v)))
     ((number? v) (display v out))
     ((list? v)
      (display "[" out)
      (let loop ((v v) (first #t))
        (if (pair? v)
            (begin
              (if (not first) (display "," out))
              (json (car v))
              (loop (cdr v) #f))))
      (display "]" out))
     (else (json-string (object->string v)))))

  ;; One record from (key . value) pairs, in order.
  (define (record . fields)
    (display "{" out)
    (let loop ((fields fields) (first #t))
      (if (pair? fields)
          (begin
            (if (not first) (display "," out))
            (json-string (car fields))
            (display ":" out)
            (json (cadr fields))
            (loop (cddr fields) #f))))
    (display "}\n" out))

  ;; A string, or null for anything else, such as a docstring that is missing.
  (define (text v) (if (string? v) v null))

  (define (doc-of proc) (text (and (procedure? proc) (procedure-documentation proc))))

  (define (pred-name p)
    (let ((name (and (procedure? p) (procedure-name p))))
      (if name (symbol->string name) "")))

  ;; An argument is `pred`, or `(pred . default)` when it may be left out.
  (define (arg a)
    (if (pair? a)
        (list 'arg (pred-name (car a)) #t)
        (list 'arg (pred-name a) #f)))

  (define (arg-json a)
    (display "{\"pred\":" out)
    (json (cadr a))
    (display ",\"optional\":" out)
    (json (caddr a))
    (display "}" out))

  (define (function-record kind name args doc)
    (display "{\"k\":" out) (json kind)
    (display ",\"name\":" out) (json name)
    (display ",\"args\":[" out)
    (let loop ((args args) (first #t))
      (if (pair? args)
          (begin
            (if (not first) (display "," out))
            (arg-json (car args))
            (loop (cdr args) #f))))
    (display "],\"doc\":" out) (json doc)
    (display "}\n" out))

  (define (sorted-symbols lst) (sort lst (lambda (a b) (string<? (symbol->string a) (symbol->string b)))))

  (define parser-module (current-module))
  (define lily-module (resolve-module '(lily)))

  ;; The module's own bindings, as (symbol . value), without the unbound ones.
  (define (bindings module)
    (let ((found '()))
      (module-for-each
       (lambda (sym var)
         (if (variable-bound? var)
             (set! found (cons (cons sym (variable-ref var)) found))))
       module)
      (sort found (lambda (a b) (string<? (symbol->string (car a)) (symbol->string (car b)))))))

  (define (strip-suffix s suffix)
    (and (string-suffix? suffix s) (substring s 0 (- (string-length s) (string-length suffix)))))

  (record "k" "meta"
          "format" 1
          "lilypond" (lilypond-version)
          "datadir" (text (ly:get-option 'datadir)))

  ;; Music functions, and every other variable in the parser's module.
  (for-each
   (lambda (b)
     (let ((name (car b)) (v (cdr b)))
       (cond
        ((ly:music-function? v)
         (function-record "music-function" name
                          (map arg (cdr (ly:music-function-signature v)))
                          (doc-of (ly:music-function-extract v))))
        ;; Markup commands are listed from their own modules below.
        ((and (procedure? v) (or (markup-function? v) (markup-list-function? v))) #f)
        (else
         (record "k" "identifier"
                 "name" name
                 "kind" (cond
                         ((ly:music? v) "music")
                         ((string? v) "string")
                         ((markup? v) "markup")
                         ((number? v) "number")
                         ((or (ly:context-def? v) (ly:context-mod? v)) "context-def")
                         (else "scheme")))))))
   (bindings parser-module))

  ;; Markup and markup-list commands, from LilyPond's module and the parser's, each once.
  (let ((seen (make-hash-table)))
    (for-each
     (lambda (b)
       (let* ((s (symbol->string (car b)))
              (v (cdr b))
              (list-name (strip-suffix s "-markup-list"))
              (name (or list-name (strip-suffix s "-markup"))))
         (if (and name (procedure? v) (not (hash-ref seen s))
                  (if list-name (markup-list-function? v) (markup-function? v)))
             (begin
               (hash-set! seen s #t)
               (function-record (if list-name "markup-list" "markup") name
                                (map arg (markup-command-signature v))
                                (doc-of v))))))
     (append (bindings lily-module) (bindings parser-module))))

  (for-each
   (lambda (g)
     (let ((meta (assoc-get 'meta (cdr g))))
       (record "k" "grob"
               "name" (car g)
               "interfaces" (assoc-get 'interfaces meta '())
               "doc" (text (assoc-get 'description meta #f)))))
   (sort all-grob-descriptions (lambda (a b) (string<? (symbol->string (car a)) (symbol->string (car b))))))

  (let ((interfaces '()))
    (hash-for-each (lambda (k v) (set! interfaces (cons v interfaces))) (ly:all-grob-interfaces))
    (for-each
     (lambda (i)
       (record "k" "interface"
               "name" (car i)
               "props" (caddr i)
               "doc" (text (cadr i))))
     (sort interfaces (lambda (a b) (string<? (symbol->string (car a)) (symbol->string (car b)))))))

  (for-each
   (lambda (p)
     (record "k" "property"
             "name" p
             "scope" "backend"
             "pred" (pred-name (object-property p 'backend-type?))
             "doc" (text (object-property p 'backend-doc))))
   (sorted-symbols all-backend-properties))

  (for-each
   (lambda (p)
     (record "k" "property"
             "name" p
             "scope" "context"
             "pred" (pred-name (object-property p 'translation-type?))
             "doc" (text (object-property p 'translation-doc))
             "internal" (and (memq p all-internal-translation-properties) #t)))
   (sorted-symbols (append all-user-translation-properties all-internal-translation-properties)))

  (for-each
   (lambda (c)
     (let ((name (car c)))
       (record "k" "context"
               "name" name
               "doc" (text (ly:context-def-lookup (ly:output-def-lookup $defaultlayout name) 'description)))))
   (filter (lambda (c) (symbol? (car c))) (ly:output-description $defaultlayout)))

  (for-each
   (lambda (t)
     (record "k" "translator"
             "name" (ly:translator-name t)
             "doc" (text (assoc-get 'description (ly:translator-description t) #f))))
   (ly:get-all-translators))

  (for-each (lambda (m) (record "k" "music-type" "name" (car m))) music-descriptions)

  ;; A private binding, so a LilyPond that renames it loses only these records.
  (for-each (lambda (e) (record "k" "event-class" "name" (car e)))
            (or (false-if-exception (@@ (lily) all-event-classes)) '()))

  (for-each
   (lambda (lang)
     (record "k" "language"
             "name" (car lang)
             "pitches" (map car (cdr lang))))
   language-pitch-names)

  (record "k" "drums" "names" (map car (ly:parser-lookup 'drumPitchNames)))

  ;; Says the dump finished, so a cache cut short is never taken for a whole one.
  (record "k" "end")

  (close-port out))
