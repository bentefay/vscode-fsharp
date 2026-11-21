type R = { OnlyLabel:int }
type C() = member _.OnlyLabel = 2
let value:C = { OnlyLabel=1 }
