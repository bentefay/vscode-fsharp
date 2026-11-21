namespace NavFixture

module Records =
    type Person = { (*@person_name_def*)Name: string }
    type Widget = { (*@widget_name_def*)Name: int }
    let personName (person: Person) = (*@person_name_qualified*)person.(*@person_name_use*)Name
    let widgetName (widget: Widget) = widget.(*@widget_name_use*)Name

