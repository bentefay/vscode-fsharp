namespace LinkedProof

module Common =
    let (*@common_def*)Value = 42

module Choice =
#if A
    type Entry = { (*@a_name_def*)Name: string; Number: int }
    let (*@a_make_def*)make () = { (*@a_name_create*)Name = "A"; Number = 1 }
#else
    type Entry = { (*@b_name_def*)Name: int; Number: int }
    let (*@b_make_def*)make () = { (*@b_name_create*)Name = 2; Number = 2 }
#endif

module Branch =
#if A && !B
    let (*@a_selected_def*)Selected = 1
#else
    let (*@b_selected_def*)Selected = 2
#endif

module Nested =
#if A
#if EXTRA
    let Never = 0
#else
    let (*@a_nested_def*)Selected = 3
#endif
#else
    let (*@b_nested_def*)Selected = 4
#endif

module Lexical =
    (* outer comment
#if B
       (* nested comment *)
#endif
    *)
    let triple = """first
#if A
#else
#endif
last"""
#if NEVER
    let invalid = "unterminated
    (* unterminated inactive comment
#else
    let Healthy = 5
#endif
