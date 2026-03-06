use std::fs::File;

#[derive(Debug)]
pub enum Token {
    Name(String),
    String(String),
    Eos,
}

#[derive(Debug)]
pub struct Lex {
    content: String,
    pos: usize,
    read_pos: usize,
    ch: char,
}

impl Lex {
    pub fn new(input: File) -> Self {
        let content = std::io::read_to_string(input).unwrap();
        Self {
            content,
            pos: 0,
            read_pos: 0,
            ch: '\0',
        }
    }

    pub fn next(&mut self) -> Token {
        todo!()
    }

    fn read_char(&mut self) {
        self.ch = self.peek_char();
        self.pos = self.read_pos;
        self.read_pos += 1;
    }

    fn peek_char(&self) -> char {
        if self.read_pos >= self.content.len() {
            '\0'
        } else {
            self.content.as_bytes()[self.read_pos] as char
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.content.len()
            && self.content[self.pos..].starts_with(char::is_whitespace)
        {
            self.read_char();
        }
    }
}
