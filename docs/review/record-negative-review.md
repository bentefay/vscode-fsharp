# Record-field negative-constraint boundary

The proposed optimization is logically useful for well-typed flat nominal record expressions: a direct label absent from a complete queried record's field inventory rules out that record as the expression's owner. It does **not**, by itself, establish exact reference absence during editing.

Actual FCS evidence in `record-negative-oracle.json` contains two valid controls and five semantic-error controls. All are syntactically complete. Both valid expressions select `Other.Name`. In each invalid expression, FCS still resolves the known `Name` occurrence to `R.Name`, despite an extra label absent from `R`:

```fsharp
type R = { Name: string }
type Other = { Name: string; Extra: int }
let value: R = { Name = "a"; Extra = 1 }
```

The same result holds for an unknown `Bogus` label, a copy-update of an annotated R, explicit `R.Name` qualification, and a later `r.Name` after an invalid `{ r with Extra = 1 }` update. The compiler reports an error but retains the R field binding. Thus a shortcut could wrongly remove real best-effort compiler occurrences and label the remaining set complete. A successful CST parse is not a semantic-validity certificate.

Do not implement absent-label-only elimination as an unconditional exact-reference rule. A sound restricted path needs the relevant compatible expected context and complete record representation, or a positively proven other owner; conflicting annotations, qualified labels, opaque `.fsi` representations and unresolved constraints must remain unavailable. Anonymous records, nested update paths, puns and generic/binding propagation require their own representation and proofs. The error controls are not valid-code feature coverage, but they are material dirty-code safety boundaries for an editor.

No engine implementation was changed by this review.
