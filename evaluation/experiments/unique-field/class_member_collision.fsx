type R = { OnlyLabel:int }
type C() = member _.OnlyLabel = 2
let read (value:C) = value.OnlyLabel
