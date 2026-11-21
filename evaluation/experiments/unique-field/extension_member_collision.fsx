type R = { OnlyLabel:int }
type C() = class end
type C with
    member _.OnlyLabel = 2
let read (value:C) = value.OnlyLabel
