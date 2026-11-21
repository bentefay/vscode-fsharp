namespace NavFixture

module Scopes =
    let outer (*@outer_value_def*)value =
        let first = (*@outer_value_use*)value
        let inner (*@inner_value_def*)value = (*@inner_value_use*)value + 1
        first + inner 2

    let (*@unicode_def*)unicode = "😀"
    let unicodeUse = "😀" + (*@unicode_use*)unicode

    let (*@escaped_def*)``a name`` = 42
    let escapedUse = (*@escaped_use*)``a name``

