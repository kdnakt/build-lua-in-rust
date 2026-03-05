use std::env;
use std::fs::File;

mod bytecode;
mod lex;
mod parse;
mod value;
mod vm;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} script", args[0]);
        return;
    }

    println!("Running script: {}", args[1]);
    let file = File::open(&args[1]).unwrap();
    let proto = parse::load(file);
    vm::ExeState::new().execute(&proto);
}
