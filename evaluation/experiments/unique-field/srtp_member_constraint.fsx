type R = { OnlyLabel:int }
let inline read (value:^T) = ((^T) : (member OnlyLabel:int) value)
