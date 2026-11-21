type R = { OnlyLabel:int }
type Alias = R
let value:Alias = { OnlyLabel=1 }
let read = value.OnlyLabel
