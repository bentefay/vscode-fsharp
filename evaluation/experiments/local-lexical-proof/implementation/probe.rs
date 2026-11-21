//! Development-only stdin source probe. Not a shipping daemon or a project loader.
use local_lexical_proof_harness::{cancellation::CancellationToken, local_lexical::*};
use std::io::Read;
fn main() -> anyhow::Result<()> {
    let mut input=String::new();std::io::stdin().take(2*1024*1024).read_to_string(&mut input)?;
    let request:serde_json::Value=serde_json::from_str(&input)?;
    let source=request["source"].as_str().unwrap();
    let hash=blake3::hash(source.as_bytes()).to_hex().to_string();
    let mut metrics=Metrics::default();
    let operation=if request["operation"]=="definition" {Operation::Definition} else {Operation::References{include_declaration:request["includeDeclaration"].as_bool().unwrap_or(true)}};
    let answer=query_with_metrics(Snapshot{uri:request["uri"].as_str().unwrap_or("file:///Main.fs"),source,version:request["version"].as_i64().unwrap_or(1),source_hash:&hash},Position{line:request["line"].as_u64().unwrap() as u32,character:request["character"].as_u64().unwrap() as u32},operation,&CancellationToken::new(),&mut metrics)?;
    println!("{}",serde_json::json!({"answer":answer,"metrics":metrics}));Ok(())
}
