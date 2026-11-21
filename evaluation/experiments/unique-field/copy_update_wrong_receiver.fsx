type R = { OnlyLabel:int }
type S = { Other:int }
let other = { Other=2 }
let value = { other with OnlyLabel=1 }
