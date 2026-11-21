namespace NavFixture

module RecordFlows =
    type Address = { (*@address_city_def*)City: string; Zip: int }
    type Customer = { (*@customer_name_def*)Name: string; (*@customer_home_def*)Home: Address }
    type Supplier = { (*@supplier_name_def*)Name: string; Code: int }
    type Box<'T> = { (*@box_value_def*)Value: 'T }
    type CustomerAlias = Customer
    type Counter = { mutable (*@counter_count_def*)Count: int }

    let explicitCustomer (customer: Customer) = (*@customer_q01*)customer.(*@customer_r01*)Name
    let explicitSupplier (supplier: Supplier) = (*@supplier_q01*)supplier.(*@supplier_r01*)Name

    let localAnnotation () =
        let customer: Customer = { (*@customer_r02*)Name = "local"; (*@home_r01*)Home = { (*@city_r01*)City = "London"; Zip = 1 } }
        (*@customer_q03*)customer.(*@customer_r03*)Name

    let qualifiedConstruction () =
        let customer = { (*@customer_q04*)Customer.(*@customer_r04*)Name = "qualified"; (*@home_r02*)Home = { (*@city_r02*)City = "Oslo"; Zip = 2 } }
        (*@customer_q05*)customer.(*@customer_r05*)Name

    let inferredConstruction () =
        let customer = { (*@customer_r06*)Name = "inferred"; (*@home_r03*)Home = { (*@city_r03*)City = "Tokyo"; Zip = 3 } }
        (*@customer_q07*)customer.(*@customer_r07*)Name

    let makeCustomer name = { (*@customer_r08*)Name = name; (*@home_r04*)Home = { (*@city_r04*)City = "Paris"; Zip = 4 } }
    let makeAnnotated name : Customer = { (*@customer_r09*)Name = name; (*@home_r05*)Home = { (*@city_r05*)City = "Bern"; Zip = 5 } }
    let returnedRecord () =
        let customer = makeCustomer "returned"
        (*@customer_q10*)customer.(*@customer_r10*)Name
    let annotatedReturn () =
        let customer = makeAnnotated "annotated"
        (*@customer_q11*)customer.(*@customer_r11*)Name
    let directReturnedRecord () = (*@customer_q12*)(makeCustomer "direct").(*@customer_r12*)Name

    let copiedRecord (customer: Customer) =
        let updated = { customer with (*@customer_r13*)Name = "updated" }
        (*@customer_q14*)updated.(*@customer_r14*)Name

    let aliasChain (customer: Customer) =
        let second = customer
        let third = second
        (*@customer_q15*)third.(*@customer_r15*)Name

    let typeAlias (customer: CustomerAlias) = (*@customer_q16*)customer.(*@customer_r16*)Name
    let nestedField (customer: Customer) = (*@city_q06*)(*@home_q06*)customer.(*@home_r06*)Home.(*@city_r06*)City
    let genericCustomer (box: Box<Customer>) = (*@customer_q17*)(*@box_q01*)box.(*@box_r01*)Value.(*@customer_r17*)Name
    let genericAddress (box: Box<Address>) = (*@city_q07*)(*@box_q02*)box.(*@box_r02*)Value.(*@city_r07*)City

    let tupleBinders ((customer: Customer), (supplier: Supplier)) =
        (*@customer_q18*)customer.(*@customer_r18*)Name, (*@supplier_q02*)supplier.(*@supplier_r02*)Name

    let optionBinder (input: Customer option) =
        match input with
        | Some customer -> (*@customer_q19*)customer.(*@customer_r19*)Name
        | None -> "none"

    let annotatedLambda (customers: Customer list) =
        customers |> List.map (fun (customer: Customer) -> (*@customer_q20*)customer.(*@customer_r20*)Name)

    let recordPattern ({ (*@customer_r21*)Name = name }: Customer) = name

    let dirtyReceiver (receiver: Customer) = (*@customer_q22*)receiver.(*@customer_r22*)Name
    let dirtyAlias (left: Customer) (right: Supplier) =
        let selected = left
        (*@customer_q23*)selected.(*@customer_r23*)Name

    let useCounter (counter: Counter) =
        (*@counter_q01*)counter.(*@counter_r01*)Count <- (*@counter_q02*)counter.(*@counter_r02*)Count + 1
        (*@counter_q03*)counter.(*@counter_r03*)Count

    module Shadowed =
        type Customer = Supplier
        let localAlias (customer: Customer) = (*@supplier_q03*)customer.(*@supplier_r03*)Name
        let outerAlias (customer: CustomerAlias) = (*@customer_q24*)customer.(*@customer_r24*)Name

    module OuterTypes =
        type Item = { (*@outer_item_name_def*)Name: string }
    module InnerTypes =
        type Item = { (*@inner_item_name_def*)Name: int }
        let local (item: Item) = (*@inner_item_q01*)item.(*@inner_item_r01*)Name
        let outer (item: OuterTypes.Item) = (*@outer_item_q01*)item.(*@outer_item_r01*)Name
