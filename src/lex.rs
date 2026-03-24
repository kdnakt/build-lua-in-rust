use std::fs::File;

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

fn lookup_ident(ident: &str) -> Option<Token> {
    match ident {
        "and" => Some(Token::And),
        "break" => Some(Token::Break),
        "do" => Some(Token::Do),
        "else" => Some(Token::Else),
        "elseif" => Some(Token::Elseif),
        "end" => Some(Token::End),
        "false" => Some(Token::False),
        "for" => Some(Token::For),
        "function" => Some(Token::Function),
        "goto" => Some(Token::Goto),
        "if" => Some(Token::If),
        "in" => Some(Token::In),
        "local" => Some(Token::Local),
        "nil" => Some(Token::Nil),
        "not" => Some(Token::Not),
        "or" => Some(Token::Or),
        "repeat" => Some(Token::Repeat),
        "return" => Some(Token::Return),
        "then" => Some(Token::Then),
        "true" => Some(Token::True),
        "until" => Some(Token::Until),
        "while" => Some(Token::While),
        "=" => Some(Token::Assign),
        _ => None,
    }
}

#[derive(Debug)]
pub struct Lex {
    content: String,
    pos: usize,
    read_pos: usize,
    pub ch: char,
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
            '(' => {
                self.read_char();
                Token::ParL
            }
            ')' => {
                self.read_char();
                Token::ParR
            }
            '=' => {
                self.read_char();
                if self.ch == '=' {
                    self.read_char();
                    Token::Equal
                } else {
                    Token::Assign
                }
            }
            _ => {
                let start = self.pos;
                if self.is_letter(self.ch) {
                    while self.is_letter(self.ch) {
                        self.read_char();
                    }
                    let name = &self.content[start..self.pos];
                    match lookup_ident(name) {
                        Some(tok) => tok,
                        None => Token::Name(name.to_string()),
                    }
                } else if self.is_digit(self.ch) {
                    let number = self.read_number();
                    if number.contains('.') {
                        Token::Float(number.parse().unwrap())
                    } else {
                        Token::Integer(number.parse().unwrap())
                    }
                } else {
                    panic!("unexpected character: {}", self.ch);
                }
            }
        }
    }

    fn is_letter(&self, ch: char) -> bool {
        ch.is_ascii_alphabetic() || ch == '_'
    }

    fn is_digit(&self, ch: char) -> bool {
        ch.is_ascii_digit()
    }

    fn read_char(&mut self) {
        self.ch = self.peek_char();
        self.pos = self.read_pos;
        self.read_pos += 1;
    }

    fn read_number(&mut self) -> String {
        let start = self.pos;
        while self.is_digit(self.ch) || self.ch == '.' {
            self.read_char();
        }
        self.content[start..self.pos].to_string()
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

    #[test]
    fn test_print_true() {
        let input = r#"print(true)"#.to_string();
        let mut lex = Lex::_new(input);
        assert_eq!(lex.next(), Token::Name("print".to_string()));
        assert_eq!(lex.next(), Token::ParL);
        assert_eq!(lex.next(), Token::True);
        assert_eq!(lex.next(), Token::ParR);
        assert_eq!(lex.next(), Token::Eos);
    }

    #[test]
    fn test_print_int() {
        let input = r#"print(123)"#.to_string();
        let mut lex = Lex::_new(input);
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
        "#.to_string();
        let mut lex = Lex::_new(input);
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
}
