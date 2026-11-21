namespace NavFixture.App

open NavFixture
module Alias = Arithmetic

module Consumer =
    let direct = (*@increment_app_qualified*)Arithmetic.(*@increment_app*)increment 3
    let alias = (*@increment_alias_qualified*)Alias.(*@increment_alias*)increment 4
    let api = SignatureApi.(*@exposed_use*)exposed 5
    let externalLibraryHover = System.String.(*@external_hover*)IsNullOrEmpty ""
    // increment is only text in a comment, never a semantic reference.
    let text = "increment"

