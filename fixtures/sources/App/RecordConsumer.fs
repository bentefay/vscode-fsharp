namespace NavFixture.App

open NavFixture.RecordFlows
open NavFixture.PropertyFlows

module RecordConsumer =
    let customerName (customer: Customer) = (*@customer_q25*)customer.(*@customer_r25*)Name
    let supplierName (supplier: Supplier) = (*@supplier_q04*)supplier.(*@supplier_r04*)Name
    let returnedCustomer () =
        let customer = makeCustomer "cross-project"
        (*@customer_q26*)customer.(*@customer_r26*)Name
    let nested (customer: Customer) = (*@city_q08*)(*@home_q07*)customer.(*@home_r07*)Home.(*@city_r08*)City
    let boxed (box: Box<Customer>) = (*@customer_q27*)(*@box_q03*)box.(*@box_r03*)Value.(*@customer_r27*)Name
    let productCode (product: Product) = (*@product_q06*)product.(*@product_r06*)Code
    let automaticName (value: Automatic) = (*@automatic_q03*)value.(*@automatic_r03*)Name
    let inheritedTitle (value: Derived) = (*@base_q03*)value.(*@base_r03*)Title
