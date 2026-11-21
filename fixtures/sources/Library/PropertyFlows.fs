namespace NavFixture

module PropertyFlows =
    type Product(code: string) =
        member _.(*@product_code_def*)Code = code
        member this.Display = (*@product_q01*)this.(*@product_r01*)Code

    type Alternate(code: int) =
        member _.(*@alternate_code_def*)Code = code

    type Defaults =
        static member (*@static_code_def*)Code = "default"

    type Automatic() =
        member val (*@automatic_name_def*)Name = "initial" with get, set

    type Base(title: string) =
        member _.(*@base_title_def*)Title = title

    type Derived(title: string) =
        inherit Base(title)

    let annotatedProduct (product: Product) = (*@product_q02*)product.(*@product_r02*)Code
    let annotatedAlternate (alternate: Alternate) = (*@alternate_q01*)alternate.(*@alternate_r01*)Code
    let constructedProduct () =
        let product = Product("created")
        (*@product_q03*)product.(*@product_r03*)Code
    let directConstructor () = (*@product_q04*)(Product("direct")).(*@product_r04*)Code
    let aliasProduct (product: Product) =
        let alias = product
        (*@product_q05*)alias.(*@product_r05*)Code
    let staticProperty = (*@static_q01*)Defaults.(*@static_r01*)Code
    let readAutomatic (value: Automatic) = (*@automatic_q01*)value.(*@automatic_r01*)Name
    let writeAutomatic (value: Automatic) = (*@automatic_q02*)value.(*@automatic_r02*)Name <- "changed"
    let inheritedProperty (value: Derived) = (*@base_q01*)value.(*@base_r01*)Title
    let directBase (value: Base) = (*@base_q02*)value.(*@base_r02*)Title
