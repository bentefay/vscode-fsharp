module Contracts

type Person = { (*@signature_name_def*)Name: string; Age: int }
val (*@signature_create_def*)create: name: string -> Person
val (*@signature_read_def*)readName: person: Person -> string
