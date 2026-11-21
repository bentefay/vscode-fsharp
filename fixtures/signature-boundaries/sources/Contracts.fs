module Contracts

type Person = { (*@implementation_name_def*)Name: string; Age: int }
let (*@implementation_create_def*)create name = { (*@name_construct*)Name = name; Age = 42 }
let private hidden = 1
let (*@implementation_read_def*)readName (person: Person) = (*@name_body_receiver*)person.(*@name_body_use*)Name
