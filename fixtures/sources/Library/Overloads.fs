namespace NavFixture

type Picker =
    static member (*@pick_int_def*)Pick(value: int) = value + 1
    static member (*@pick_string_def*)Pick(value: string) = value.Length

module Overloads =
    let intValue = Picker.(*@pick_int_use*)Pick(1)
    let stringValue = Picker.(*@pick_string_use*)Pick("one")

