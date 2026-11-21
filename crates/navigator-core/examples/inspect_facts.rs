//! Read-only, transient parser diagnostics. No workspace index or compiler.
use navigator_core::treesitter;
use serde_json::json;
fn main() {
    for path in std::env::args().skip(1) {
        let source = std::fs::read_to_string(&path).unwrap();
        if std::env::var_os("DUMP_AST").is_some() {
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&tree_sitter_fsharp::LANGUAGE_FSHARP.into())
                .unwrap();
            let tree = parser.parse(&source, None).unwrap();
            eprintln!("{}", tree.root_node().to_sexp());
        }
        let parsed = treesitter::parse(&source);
        let loc = |token: usize| {
            let t = &parsed.tokens[token];
            json!({"text":t.text,"line":t.line,"column":t.column,"byte":t.start})
        };
        let expressions: Vec<_> = parsed.record_expressions.iter().map(|r|json!({"fields":r.fields.iter().map(|i|loc(*i)).collect::<Vec<_>>(),"copy":r.copy.map(loc),"binding":r.binding.map(loc),"type_name":r.type_name,"type_token":r.type_token.map(loc),"parent_field":r.parent_field.map(loc),"method_result":r.method_result.map(loc),"fresh_method_result":r.fresh_method_result,"method_result_eligible":r.method_result_eligible,"method_result_uses":r.method_result_uses.iter().map(|t|loc(*t)).collect::<Vec<_>>(),"argument":r.argument})).collect();
        println!(
            "{}",
            json!({"path":path,"source_bytes":source.len(),"supported":parsed.supported,"uncertain":parsed.uncertain,"potential_bindings":parsed.potential_bindings.iter().map(|d|json!({"declaration":d,"token_location":loc(d.token),"module":parsed.declaration_module(d)})).collect::<Vec<_>>(),"record_expressions":expressions,"record_types":parsed.record_types,"attributes":parsed.attributes,"aliases":parsed.type_aliases,"pipeline_applications":parsed.pipeline_applications,"type_uses":parsed.type_uses,"expression_type_candidates":parsed.expression_type_candidates.iter().map(|c|json!({"candidate":c,"token_location":loc(c.token)})).collect::<Vec<_>>(),"explicit_constructor_heads":parsed.explicit_constructor_heads.iter().map(|t|json!({"token":t,"location":loc(*t)})).collect::<Vec<_>>(),"qualifier_roots":parsed.qualifier_roots.iter().map(|t|json!({"token":t,"location":loc(*t)})).collect::<Vec<_>>(),"module_name_tokens":parsed.module_name_tokens.iter().map(|t|json!({"token":t,"location":loc(*t)})).collect::<Vec<_>>(),"union_payload_labels":parsed.union_payload_labels.iter().map(|t|json!({"token":t,"location":loc(*t)})).collect::<Vec<_>>(),"binding_types":parsed.binding_types,"method_declarations":parsed.method_declarations,"method_calls":parsed.method_calls,"pattern_declarations":parsed.pattern_declarations,"function_returns":parsed.function_returns,"function_results":parsed.function_results,"union_case_payloads":parsed.union_case_payloads,"match_expressions":parsed.match_expressions,"tokens":if std::env::var_os("DUMP_AST").is_some(){Some(&parsed.tokens)}else{None}})
        );
    }
}
