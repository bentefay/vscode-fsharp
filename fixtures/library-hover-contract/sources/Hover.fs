module LibraryHoverContract
open System
type Person = { Name: string }
let mappedInts: string list = List.(*@map_int_string*)map (fun (value:int) -> value.ToString()) [1;2]
let mappedBools: int list = List.(*@map_bool_int*)map (fun (value:bool) -> if value then 1 else 0) [true;false]
let chosenInts: string array = Array.(*@choose_int_string*)choose (fun (value:int) -> if value>0 then Some(value.ToString()) else None) [|1;2|]
let chosenStrings: int array = Array.(*@choose_string_int*)choose (fun (value:string) -> if value.Length>0 then Some value.Length else None) [|"a";""|]
let readPerson (wrapper:Tuple<Person>) = wrapper.(*@tuple_person*)Item1
let readInt (wrapper:Tuple<int>) = wrapper.(*@tuple_int*)Item1
