use std::fs::File;

#[derive(Debug, PartialEq, Eq)]
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
        Lex::_new(content)
    }

    fn _new(content: String) -> Self {
        let mut lexer = Self {
            content,
            pos: 0,
            read_pos: 0,
            ch: '\0',
        };
        lexer.read_char();
        lexer
    }

    pub fn next(&mut self) -> Token {
        self.skip_whitespace();
        match self.ch {
            '\0' => Token::Eos,
            '"' => {
                let start = self.pos + 1;
                while self.peek_char() != '"' && self.peek_char() != '\0' {
                    self.read_char();
                }
                self.read_char();
                let s = self.content[start..self.pos].to_string();
                self.read_char(); // skip closing quote
                Token::String(s)
            }
            _ => {
                let start = self.pos;
                while !self.peek_char().is_whitespace() && self.peek_char() != '\0' {
                    self.read_char();
                }
                self.read_char();
                let name = self.content[start..self.pos].to_string();
                Token::Name(name)
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_world() {
        let input = r#"print "Hello, World!""#.to_string();
        let mut lex = Lex::_new(input);
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::String("Hello, World!".to_string()));
        assert_eq!(lex.next(), Token::Eos);
    }
}
