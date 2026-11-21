module Hidden =
    type R = private { OnlyLabel:int }
open Hidden
let value = { OnlyLabel=1 }
