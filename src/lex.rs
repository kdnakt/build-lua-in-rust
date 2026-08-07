use std::io::{Bytes, Read};
use std::iter::Peekable;
use std::mem;

#[derive(Debug, PartialEq)]
pub enum Token {
    // Keywords
    And,
    Break,
    Do,
    Else,
    Elseif,
    End,
    False,
    For,
    Function,
    Goto,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    While,
    // Operators
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Len,
    BitAnd,
    BitXor,
    BitOr,
    BitNot,
    ShiftL,
    ShiftR,
    Idiv,
    Equal,
    NotEq,
    LesEq,
    GreEq,
    Less,
    Greater,
    Assign,
    ParL,
    ParR,
    CurlyL,
    CurlyR,
    SqurL,
    SqurR,
    DoubColon,
    SemiColon,
    Colon,
    Comma,
    Dot,
    Concat,
    Dots,
    // Constant Values
    Integer(i64),
    Float(f64),
    String(Vec<u8>),
    // Name of variables or table keys
    Name(String),
    // End
    Eos,
}

fn lookup_ident(ident: String) -> Token {
    match &ident as &str {
        "and" => Token::And,
        "break" => Token::Break,
        "do" => Token::Do,
        "else" => Token::Else,
        "elseif" => Token::Elseif,
        "end" => Token::End,
        "false" => Token::False,
        "for" => Token::For,
        "function" => Token::Function,
        "goto" => Token::Goto,
        "if" => Token::If,
        "in" => Token::In,
        "local" => Token::Local,
        "nil" => Token::Nil,
        "not" => Token::Not,
        "or" => Token::Or,
        "repeat" => Token::Repeat,
        "return" => Token::Return,
        "then" => Token::Then,
        "true" => Token::True,
        "until" => Token::Until,
        "while" => Token::While,
        _ => Token::Name(ident),
    }
}

#[derive(Debug)]
pub struct Lex<R: Read> {
    input: Peekable<Bytes<R>>,
    ahead: Token,
}

impl<R: Read> Lex<R> {
    pub fn new(input: R) -> Self {
        Self {
            input: input.bytes().peekable(),
            ahead: Token::Eos,
        }
    }

    pub fn next(&mut self) -> Token {
        if self.ahead == Token::Eos {
            self.next_token()
        } else {
            mem::replace(&mut self.ahead, Token::Eos)
        }
    }

    pub fn peek(&mut self) -> &Token {
        if self.ahead == Token::Eos {
            self.ahead = self.next_token();
        }
        &self.ahead
    }

    fn next_token(&mut self) -> Token {
        let byt = self.next_byte();
        if byt.is_none() {
            return Token::Eos;
        }
        let byt = byt.unwrap();
        match byt {
            b'\n' | b'\t' | b' ' | b'\r' => self.next_token(),
            b'+' => Token::Add,
            b'*' => Token::Mul,
            b'%' => Token::Mod,
            b'^' => Token::Pow,
            b'#' => Token::Len,
            b'&' => Token::BitAnd,
            b'|' => Token::BitOr,
            b'(' => Token::ParL,
            b')' => Token::ParR,
            b'{' => Token::CurlyL,
            b'}' => Token::CurlyR,
            b'[' => Token::SqurL,
            b']' => Token::SqurR,
            b';' => Token::SemiColon,
            b'/' => self.check_ahead(b'/', Token::Idiv, Token::Div),
            b'=' => self.check_ahead(b'=', Token::Equal, Token::Assign),
            b'~' => self.check_ahead(b'=', Token::NotEq, Token::BitNot),
            b':' => self.check_ahead(b':', Token::DoubColon, Token::Colon),
            b'<' => self.check_ahead2(b'=', Token::LesEq, b'<', Token::ShiftL, Token::Less),
            b'>' => self.check_ahead2(b'=', Token::GreEq, b'>', Token::ShiftR, Token::Greater),
            b'\'' | b'"' => self.read_string(byt),
            b',' => Token::Comma,
            b'\0' => Token::Eos,
            b'.' => match self.peek_byte() {
                b'.' => {
                    self.next_byte();
                    if self.peek_byte() == b'.' {
                        self.next_byte();
                        Token::Dots
                    } else {
                        Token::Concat
                    }
                }
                b'0'..=b'9' => self.read_float(0),
                _ => Token::Dot,
            },
            b'-' => {
                if self.peek_byte() == b'-' {
                    self.next_byte();
                    self.read_comment();
                    self.next_token()
                } else {
                    Token::Sub
                }
            }
            b'0'..=b'9' => self.read_number(byt),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.read_name(byt),
            _ => panic!("unexpected byte: {}", byt),
        }
    }

    fn check_ahead(&mut self, ahead: u8, long: Token, short: Token) -> Token {
        if self.peek_byte() == ahead {
            self.next_byte();
            long
        } else {
            short
        }
    }

    fn check_ahead2(
        &mut self,
        ahead1: u8,
        long1: Token,
        ahead2: u8,
        long2: Token,
        short: Token,
    ) -> Token {
        let ch = self.peek_byte();
        if ahead1 == ch {
            self.next_byte();
            long1
        } else if ch == ahead2 {
            self.next_byte();
            long2
        } else {
            short
        }
    }

    fn next_byte(&mut self) -> Option<u8> {
        self.input.next().and_then(|r| Some(r.unwrap()))
    }

    fn peek_byte(&mut self) -> u8 {
        match self.input.peek() {
            Some(Ok(b)) => *b,
            Some(_) => panic!("error reading input"),
            None => b'\0',
        }
    }

    fn read_string(&mut self, quote: u8) -> Token {
        let mut s = Vec::new();
        loop {
            match self.next_byte().expect("unfinished string") {
                b'\n' | b'\0' => panic!("unexpected end of string"),
                b'\\' => s.push(self.read_escape()),
                c if c == quote => break,
                c => s.push(c),
            }
        }
        Token::String(s)
    }

    fn read_escape(&mut self) -> u8 {
        match self.next_byte().expect("unfinished escape sequence") {
            b'a' => 0x07,
            b'b' => 0x08,
            b'f' => 0x0C,
            b'v' => 0x0B,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'\\' => b'\\',
            b'"' => b'"',
            b'\'' => b'\'',
            b'x' => {
                let n1 = char::to_digit(self.next_byte().unwrap() as char, 16).unwrap();
                let n2 = char::to_digit(self.next_byte().unwrap() as char, 16).unwrap();
                (n1 * 16 + n2) as u8
            }
            ch @ b'0'..=b'9' => {
                let mut n = char::to_digit(ch as char, 10).unwrap();
                if let Some(d) = char::to_digit(self.peek_byte() as char, 10) {
                    self.next_byte();
                    n = n * 10 + d;
                    if let Some(d) = char::to_digit(self.peek_byte() as char, 10) {
                        self.next_byte();
                        n = n * 10 + d;
                    }
                }
                u8::try_from(n).expect("escape sequence out of range")
            }
            _ => panic!("invalid escape sequence"),
        }
    }

    fn read_comment(&mut self) {
        match self.next_byte() {
            Some(b'[') => todo!("long comments"),
            None => (),
            Some(_) => {
                while let Some(ch) = self.next_byte() {
                    if ch == b'\n' {
                        break;
                    }
                }
            }
        }
    }

    fn read_number(&mut self, first: u8) -> Token {
        if first == b'0' {
            let ch = self.peek_byte();
            if ch == b'x' || ch == b'X' {
                return self.read_hex();
            }
        }

        let mut n = (first - b'0') as i64;
        loop {
            let ch = self.peek_byte();
            if let Some(d) = char::to_digit(ch as char, 10) {
                self.next_byte();
                n = n * 10 + d as i64;
            } else if ch == b'.' {
                return self.read_float(n);
            } else if ch == b'e' || ch == b'E' {
                return self.read_num_exp(n as f64);
            } else {
                break;
            }
        }
        let fch = self.peek_byte();
        if (fch as char).is_alphabetic() || fch == b'.' {
            panic!("invalid number format");
        }
        Token::Integer(n)
    }

    fn read_hex(&mut self) -> Token {
        self.next_byte(); // skip 'x'
        todo!("hexadecimal numbers")
    }

    fn read_float(&mut self, i: i64) -> Token {
        self.next_byte(); // skip '.'

        let mut n: i64 = 0;
        let mut x: f64 = 1.0;
        loop {
            let ch = self.peek_byte();
            if let Some(d) = char::to_digit(ch as char, 10) {
                self.next_byte();
                n = n * 10 + d as i64;
                x *= 10.0;
            } else {
                break;
            }
        }
        Token::Float(i as f64 + n as f64 / x)
    }

    #[allow(unused_variables)]
    fn read_num_exp(&mut self, f: f64) -> Token {
        self.next_byte(); // skip 'e'
        todo!("exponent part of numbers")
    }

    fn read_name(&mut self, first: u8) -> Token {
        let mut s = String::new();
        s.push(first as char);
        loop {
            let ch = self.peek_byte() as char;
            if ch.is_alphanumeric() || ch == '_' {
                self.next_byte();
                s.push(ch);
            } else {
                break;
            }
        }

        lookup_ident(s)
    }

    pub fn expect(&mut self, expected: Token) {
        assert_eq!(self.next(), expected);
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};

    use super::*;

    fn new_lex(input: String) -> Lex<std::fs::File> {
        let tempdir = std::env::temp_dir();
        let mut hash = std::hash::DefaultHasher::new();
        input.hash(&mut hash);
        let filepath = tempdir.join(format!("test-{}.lua", hash.finish()));
        std::fs::write(&filepath, input).unwrap();
        let file = std::fs::File::open(filepath).unwrap();
        Lex::new(file)
    }

    #[test]
    fn test_hello_world() {
        let input = r#"print "Hello, World!""#.to_string();
        let mut lex = new_lex(input);
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(
            lex.next(),
            Token::String("Hello, World!".as_bytes().to_vec())
        );
        assert_eq!(lex.next(), Token::Eos);
    }

    #[test]
    fn test_print_true() {
        let input = r#"print(true)"#.to_string();
        let mut lex = new_lex(input);
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::ParL);
        assert_eq!(lex.next(), Token::True);
        assert_eq!(lex.next(), Token::ParR);
        assert_eq!(lex.next(), Token::Eos);
    }

    #[test]
    fn test_print_int() {
        let input = r#"print(123)"#.to_string();
        let mut lex = new_lex(input);
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::ParL);
        assert_eq!(lex.next(), Token::Integer(123));
        assert_eq!(lex.next(), Token::ParR);
        assert_eq!(lex.next(), Token::Eos);
    }

    #[test]
    fn test_print_int_var() {
        let input = r#"
            local a = 123
            print(a)
        "#
        .to_string();
        let mut lex = new_lex(input);
        assert_eq!(lex.next(), Token::Local);
        assert_eq!(lex.next(), Token::Name("a".to_string()));
        assert_eq!(lex.next(), Token::Assign);
        assert_eq!(lex.next(), Token::Integer(123));
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::ParL);
        assert_eq!(lex.next(), Token::Name("a".to_string()));
        assert_eq!(lex.next(), Token::ParR);
        assert_eq!(lex.next(), Token::Eos);
    }

    #[test]
    fn test_print_str_var() {
        let input = r#"
            local a = "hello"
            print(a)
        "#
        .to_string();
        let mut lex = new_lex(input);
        assert_eq!(lex.next(), Token::Local);
        assert_eq!(lex.next(), Token::Name("a".to_string()));
        assert_eq!(lex.next(), Token::Assign);
        assert_eq!(lex.next(), Token::String("hello".as_bytes().to_vec()));
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::ParL);
        assert_eq!(lex.next(), Token::Name("a".to_string()));
        assert_eq!(lex.next(), Token::ParR);
        assert_eq!(lex.next(), Token::Eos);
    }

    #[test]
    fn test_escape_sequences() {
        let input = r#"print("Hello, \nWorld!")"#.to_string();
        let mut lex = new_lex(input);
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::ParL);
        assert_eq!(
            lex.next(),
            Token::String("Hello, \nWorld!".as_bytes().to_vec())
        );
        assert_eq!(lex.next(), Token::ParR);
        assert_eq!(lex.next(), Token::Eos);
    }
}
