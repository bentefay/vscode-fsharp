type R = { OnlyLabel:int }
let value = {| OnlyLabel=2 |}
let read = value.OnlyLabel
