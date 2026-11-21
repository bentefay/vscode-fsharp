module Consumer
open Contracts
let read (person: Person) = (*@name_direct_receiver*)person.(*@name_direct_use*)Name
let created = (*@create_use*)create "Ada"
let name = (*@name_inferred_receiver*)created.(*@name_inferred_use*)Name
let nameAgain = (*@read_use*)readName created
