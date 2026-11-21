type R = { OnlyLabel:int }
type S = { Other:int }
let inline constrain< ^T when ^T : (static member Create: unit -> S)> () = ()
type Factory =
    static member Create() = { OnlyLabel=1 }
    static member Force() = constrain<Factory>()
