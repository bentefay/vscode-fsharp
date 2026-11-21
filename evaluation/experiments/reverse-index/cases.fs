module ReverseIndex
let read (values:int array) offset =
    let direct = values[^offset]
    let dotted = values.[^offset]
    let endOnly = values[..^offset]
    let both = values[^3..^offset]
    let startOnly = values[^offset..]
    let normalEnd = values[0..^offset]
    let dottedEnd = values.[0..^offset]
    let computed = values[0..^(offset + 1)]
    direct + dotted
let read2 (values:int[,]) offset = values.[*, ^offset..^0]
let (^) (a:string) (b:string) = a + b
let caret (a:string) b = a ^ b
let inline traitCall (x:^T) = (^T : (static member Name : ^T -> string) x)
let inline multiple (x:^T) (y:^U) = ((^T or ^U) : (static member Join : ^T * ^U -> string) (x,y))
let quoted = "values[0..^offset]"
// values[0..^offset]
