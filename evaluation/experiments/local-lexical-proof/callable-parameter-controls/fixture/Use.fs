module Use
open Callable
let instance = C(seed=1)
let methodResult = instance.M(amount=2)
let staticResult = C.S(delta=3)
