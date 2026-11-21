module Callable
type C(seed:int) =
    member _.Value = seed
    member _.M(amount:int) = amount + amount
    static member S(delta:int) = delta + delta
