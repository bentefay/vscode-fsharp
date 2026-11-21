fn main() {
 let source=std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
 std::thread::Builder::new().stack_size(2*1024*1024).spawn(move||{
  let parsed=navigator_core::treesitter::parse(&source);
  println!("parsed {} tokens, {} uncertainties",parsed.tokens.len(),parsed.uncertain.len());
 }).unwrap().join().unwrap();
}
