namespace NavFixture

module ScopeEdges =
    let (*@branch_global_def*)branchValue = 10
    let fromOption value =
        match value with
        | Some branchValue -> branchValue
        | None -> (*@branch_global_use*)branchValue

module OpenProvider =
    let (*@open_provider_def*)shared = 42

module OpenConsumer =
    let shared = 17
    open OpenProvider
    let result = (*@open_shadow_use*)shared

module LocalSystem =
    module System =
        type String =
            static member IsNullOrEmpty (_value: string) = false
    let value = System.String.(*@workspace_system_hover*)IsNullOrEmpty ""

module AutoOpenConsumer =
    let chosen = 1
    [<AutoOpen>]
    module Imported =
        let (*@autoopen_def*)chosen = 2
    let selected = (*@autoopen_use*)chosen

