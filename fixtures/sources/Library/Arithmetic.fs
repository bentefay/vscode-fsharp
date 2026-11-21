namespace NavFixture

/// Operations used by the navigation fixture.
module Arithmetic =
    /// Adds one to the supplied number.
    let (*@increment_def*)increment (*@increment_value_def*)value = (*@increment_value_use*)value + 1
    let twice value = (*@increment_use1*)increment ((*@increment_use2*)increment value)

