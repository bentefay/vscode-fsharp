namespace NavFixture

module SignatureApi =
    let private (*@hidden_def*)hidden value = value + 1
    let (*@exposed_impl*)exposed value = (*@hidden_use*)hidden value
    let hiddenBySignature = 3

