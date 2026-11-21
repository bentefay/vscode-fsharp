module M
let first (values:int array) offset = values[^offset + 1]
let second (values:int array) offset = values[^(offset + 1)]
let third (values:int array) offset = values.[^offset + 1]
let fourth (values:int array) offset = values[0..^ offset]
