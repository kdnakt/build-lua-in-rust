use std::env;
use std::fs::File;

mod bytecode;
mod lex;
mod parse;
mod value;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} script", args[0]);
        return;
    }

    println!("Running script: {}", args[1]);
    let file = File::open(&args[1]).unwrap();
    // TODO: parse file and execute
}
