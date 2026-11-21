namespace NavFixture

module Linked =
#if FIRST
    let (*@linked_first_def*)value = 1
#else
    let (*@linked_second_def*)value = "second"
#endif

