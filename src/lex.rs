use std::fs::File;

#[derive(Debug)]
pub enum Token {
    Name(String),
    String(String),
    Eos,
}

#[derive(Debug)]
pub struct Lex {
    input: File,
}

impl Lex {
    pub fn new(input: File) -> Self {
        Self { input }
    }

    pub fn next(&mut self) -> Token {
        todo!()
    }
}
