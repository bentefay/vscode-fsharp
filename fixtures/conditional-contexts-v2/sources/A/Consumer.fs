module LinkedProof.ConsumerA
open LinkedProof
let common = Common.(*@a_common_use*)Value
let item = Choice.(*@a_make_use*)make ()
let name = (*@a_receiver*)item.(*@a_name_use*)Name
let selected = Branch.(*@a_selected_use*)Selected
let nested = Nested.(*@a_nested_use*)Selected
let unicodeName = let _ = "😀" in (*@a_unicode_receiver*)item.(*@a_unicode_name_use*)Name
