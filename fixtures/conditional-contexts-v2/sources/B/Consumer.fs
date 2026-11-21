module LinkedProof.ConsumerB
open LinkedProof
let common = Common.(*@b_common_use*)Value
let item = Choice.(*@b_make_use*)make ()
let name = (*@b_receiver*)item.(*@b_name_use*)Name
let selected = Branch.(*@b_selected_use*)Selected
let nested = Nested.(*@b_nested_use*)Selected
let unicodeName = let _ = "😀" in (*@b_unicode_receiver*)item.(*@b_unicode_name_use*)Name
