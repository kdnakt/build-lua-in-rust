use std::fs::File;
use std::mem;
use std::io::{Read, Seek, SeekFrom};

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
    String(String),
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
        "=" => Token::Assign,
        _ => Token::Name(ident),
    }
}

#[derive(Debug)]
pub struct Lex {
    input: File,
    ahead: Token,
}

impl Lex {
    pub fn new(input: File) -> Self {
        Self {
            input,
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

    fn next_token(&mut self) -> Token {
        let ch = self.read_char();
        match ch {
            _ if ch.is_whitespace() => self.next_token(),
            '+' => Token::Add,
            '*' => Token::Mul,
            '%' => Token::Mod,
            '^' => Token::Pow,
            '#' => Token::Len,
            '&' => Token::BitAnd,
            '|' => Token::BitOr,
            '(' => Token::ParL,
            ')' => Token::ParR,
            '{' => Token::CurlyL,
            '}' => Token::CurlyR,
            '[' => Token::SqurL,
            ']' => Token::SqurR,
            ';' => Token::SemiColon,
            '/' => self.check_ahead('/', Token::Idiv, Token::Div),
            '=' => self.check_ahead('=', Token::Equal, Token::Assign),
            '~' => self.check_ahead('=', Token::NotEq, Token::BitXor),
            ':' => self.check_ahead(':', Token::DoubColon, Token::Colon),
            '<' => self.check_ahead2('=', Token::LesEq, '<', Token::ShiftL, Token::Less),
            '>' => self.check_ahead2('=', Token::GreEq, '>', Token::ShiftR, Token::Greater),
            '\'' | '"' => self.read_string(ch),
            ',' => Token::Comma,
            '\0' => Token::Eos,
            '.' => match self.read_char() {
                '.' => {
                    if self.read_char() == '.' {
                        Token::Dots
                    } else {
                        self.putback_char();
                        Token::Concat
                    }
                },
                '0'..='9' => {
                    self.putback_char();
                    self.read_float(0)
                },
                _ => {
                    self.putback_char();
                    Token::Dot
                }
            },
            '-' => {
                if self.read_char() == '-' {
                    // skip comments
                    loop {
                        let ch = self.read_char();
                        if ch == '\n' || ch == '\0' {
                            break;
                        }
                    }
                    self.next_token()
                } else {
                    self.putback_char();
                    Token::Sub
                }
            },
            '0'..='9' => self.read_number(ch),
            'A'..='Z' | 'a'..='z' | '_' => self.read_name(ch),
            _ => panic!("unexpected character: {}", ch),
        }
    }

    fn check_ahead(&mut self, ahead: char, long: Token, short: Token) -> Token {
        if ahead == self.read_char() {
            long
        } else {
            self.putback_char();
            short
        }
    }

    fn check_ahead2(
        &mut self,
        ahead1: char,
        long1: Token,
        ahead2: char,
        long2: Token,
        short: Token,
    ) -> Token {
        let ch = self.read_char();
        if ahead1 == ch {
            long1
        } else if ch == ahead2 {
            long2
        } else {
            self.putback_char();
            short
        }
    }

    fn putback_char(&mut self) {
        self.input.seek(SeekFrom::Current(-1)).unwrap();
    }

    fn read_string(&mut self, quote: char) -> Token {
        let mut s = String::new();
        loop {
            match self.read_char() {
                '\n' | '\0' => panic!("unexpected end of string"),
                '\\' => todo!("escape sequences"),
                c if c == quote => break,
                c => s.push(c),
            }
        }
        Token::String(s)
    }

    #[allow(clippy::unused_io_amount)]
    fn read_char(&mut self) -> char {
        let mut buf: [u8; 1] = [0];
        self.input.read(&mut buf).unwrap();
        buf[0] as char
    }

    fn read_number(&mut self, first: char) -> Token {
        if first == '0' {
            let ch = self.read_char();
            if ch == 'x' || ch == 'X' {
                return self.read_hex();
            }
            self.putback_char();
        }

        let mut n = char::to_digit(first, 10).unwrap() as i64;
        loop {
            let ch = self.read_char();
            if let Some(d) = char::to_digit(ch, 10) {
                n = n * 10 + d as i64;
            } else if ch == '.' {
                return self.read_float(n);
            } else if ch == 'e' || ch == 'E' {
                return self.read_num_exp(n as f64);
            } else {
                self.putback_char();
                break;
            }
        }
        let fch = self.read_char();
        if fch.is_alphabetic() || fch == '.' {
            panic!("invalid number format");
        } else {
            self.putback_char();
        }
        Token::Integer(n)
    }

    fn read_hex(&mut self) -> Token {
        todo!("hexadecimal numbers")
    }

    fn read_float(&mut self, i: i64) -> Token {
        let mut f = i as f64;
        let mut div = 10.0;
        loop {
            let ch = self.read_char();
            if let Some(d) = char::to_digit(ch, 10) {
                f += d as f64 / div;
                div *= 10.0;
            } else if ch == 'e' || ch == 'E' {
                return self.read_num_exp(f);
            } else {
                self.putback_char();
                break;
            }
        }
        let fch = self.read_char();
        if fch.is_alphabetic() {
            panic!("invalid number format");
        } else {
            self.putback_char();
        }
        Token::Float(f)
    }

    #[allow(unused_variables)]
    fn read_num_exp(&mut self, f: f64) -> Token {
        todo!("exponent part of numbers")
    }

    fn read_name(&mut self, first: char) -> Token {
        let mut s = String::new();
        s.push(first);
        loop {
            let ch = self.read_char();
            if ch.is_alphanumeric() || ch == '_' {
                s.push(ch);
            } else {
                self.putback_char();
                break;
            }
        }

        lookup_ident(s)
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};

    use super::*;

    fn new_lex(input: String) -> Lex {
        let tempdir = std::env::temp_dir();
        let mut hash = std::hash::DefaultHasher::new();
        input.hash(&mut hash);
        let filepath = tempdir.join(format!("test-{}.lua", hash.finish()));
        std::fs::write(&filepath, input).unwrap();
        let file = File::open(filepath).unwrap();
        Lex::new(file)
    }

    #[test]
    fn test_hello_world() {
        let input = r#"print "Hello, World!""#.to_string();
        let mut lex = new_lex(input);
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::String("Hello, World!".to_string()));
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
        assert_eq!(lex.next(), Token::String("hello".to_string()));
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::ParL);
        assert_eq!(lex.next(), Token::Name("a".to_string()));
        assert_eq!(lex.next(), Token::ParR);
        assert_eq!(lex.next(), Token::Eos);
    }
}
