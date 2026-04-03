use std::fs::File;

use crate::{
    bytecode::ByteCode,
    lex::{Lex, Token},
    value::Value,
};

#[derive(Debug)]
pub struct ParseProto {
    pub constants: Vec<Value>,
    pub byte_codes: Vec<ByteCode>,
    locals: Vec<String>,
    lex: Lex,
}

impl ParseProto {
    pub fn load(input: File) -> ParseProto {
        let mut proto = ParseProto {
            constants: Vec::new(),
            byte_codes: Vec::new(),
            locals: Vec::new(),
            lex: Lex::new(input),
        };
        proto.chunk();

        println!("constants: {:?}", &proto.constants);
        println!("byte_codes:");
        for c in proto.byte_codes.iter() {
            println!("  {c:?}");
        }

        proto
    }

    fn chunk(&mut self) {
        loop {
            match self.lex.next() {
                Token::Name(name) => {
                    let _ic = add_const(&mut self.constants, Value::String(name.clone()));
                    match self.lex.next() {
                        Token::ParL => {
                            // '(')
                            match self.lex.next() {
                                Token::Nil => self.byte_codes.push(ByteCode::LoadNil(1)),
                                Token::True => self.byte_codes.push(ByteCode::LoadBool(1, true)),
                                Token::False => self.byte_codes.push(ByteCode::LoadBool(1, false)),
                                Token::Integer(i) => {
                                    if let Ok(val) = i16::try_from(i) {
                                        self.byte_codes.push(ByteCode::LoadInt(1, val));
                                    } else {
                                        load_const(
                                            &mut self.constants,
                                            &mut self.byte_codes,
                                            1,
                                            Value::Integer(i),
                                        );
                                    }
                                }
                                Token::Float(f) => load_const(
                                    &mut self.constants,
                                    &mut self.byte_codes,
                                    1,
                                    Value::Float(f),
                                ),
                                Token::String(s) => load_const(
                                    &mut self.constants,
                                    &mut self.byte_codes,
                                    1,
                                    Value::String(s),
                                ),
                                Token::Name(name) => load_var(
                                    &mut self.constants,
                                    &mut self.byte_codes,
                                    &self.locals,
                                    1,
                                    name,
                                ),
                                _ => panic!("invalid argument: {:?}", self.lex),
                            };
                            load_var(
                                &mut self.constants,
                                &mut self.byte_codes,
                                &self.locals,
                                0,
                                name,
                            );
                            self.byte_codes.push(ByteCode::Call(0, 1));
                            if self.lex.next() != Token::ParR {
                                // ')'
                                panic!("expected `)`");
                            }
                        }
                        Token::String(s) => {
                            load_const(
                                &mut self.constants,
                                &mut self.byte_codes,
                                1,
                                Value::String(s),
                            );
                            load_var(
                                &mut self.constants,
                                &mut self.byte_codes,
                                &self.locals,
                                0,
                                name,
                            );
                            self.byte_codes.push(ByteCode::Call(0, 1));
                        }
                        _ => {
                            dbg!(&self.lex);
                            dbg!(&self.byte_codes);
                            dbg!(&self.constants);
                            dbg!(&self.locals);
                            panic!("expected string");
                        }
                    }
                }
                Token::Eos => break,
                Token::Local => self.local(),
                t => panic!("unexpected token: {t:?}"),
            }
        }
    }

    fn local(&mut self) {
        let var = if let Token::Name(var) = self.lex.next() {
            var
        } else {
            panic!("expected variable");
        };
        if self.lex.next() != Token::Assign {
            panic!("expected `=`");
        }

        self.load_exp(self.locals.len());
        self.locals.push(var);
    }

    fn load_exp(&mut self, dst: usize) {
        let code = match self.lex.next() {
            Token::Nil => ByteCode::LoadNil(dst as u8),
            Token::True => ByteCode::LoadBool(dst as u8, true),
            Token::False => ByteCode::LoadBool(dst as u8, false),
            Token::Integer(i) => {
                if let Ok(val) = i16::try_from(i) {
                    ByteCode::LoadInt(dst as u8, val)
                } else {
                    self.load_const(dst, Value::Integer(i))
                }
            }
            Token::Float(f) => self.load_const(dst, Value::Float(f)),
            Token::String(s) => self.load_const(dst, Value::String(s)),
            Token::Name(var) => self.load_var(dst, &var),
            _ => panic!("invalid argument"),
        };
        self.byte_codes.push(code);
    }

    fn load_const(&mut self, dst: usize, val: Value) -> ByteCode {
        ByteCode::LoadConst(dst as u8, add_const(&mut self.constants, val) as u8)
    }

    fn load_var(&mut self, dst: usize, name: &str) -> ByteCode {
        if let Some(idx) = self.locals.iter().rposition(|v| v == &name) {
            ByteCode::Move(dst as u8, idx as u8)
        } else {
            let ic = add_const(&mut self.constants, Value::String(name.to_string()));
            ByteCode::GetGlobal(dst as u8, ic as u8)
        }
    }
}

fn load_const(constants: &mut Vec<Value>, byte_codes: &mut Vec<ByteCode>, dst: usize, val: Value) {
    let code = ByteCode::LoadConst(dst as u8, add_const(constants, val) as u8);
    byte_codes.push(code);
}

fn add_const(constants: &mut Vec<Value>, val: Value) -> usize {
    constants.iter().position(|v| *v == val).unwrap_or_else(|| {
        constants.push(val);
        constants.len() - 1
    })
}

fn load_var(
    constants: &mut Vec<Value>,
    byte_codes: &mut Vec<ByteCode>,
    locals: &Vec<String>,
    dst: usize,
    name: String,
) {
    let code = if let Some(idx) = locals.iter().rposition(|v| v == &name) {
        ByteCode::Move(dst as u8, idx as u8)
    } else {
        let ic = add_const(constants, Value::String(name));
        ByteCode::GetGlobal(dst as u8, ic as u8)
    };
    byte_codes.push(code);
}

mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn test_hello() {
        let proto = ParseProto::load(File::open("test/hello.lua").unwrap());
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], Value::String("print".to_string()));
        assert_eq!(
            proto.constants[1],
            Value::String("hello, world!".to_string())
        );
        assert_eq!(proto.byte_codes.len(), 3);
        assert_eq!(proto.byte_codes[0], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[1], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_multi_print() {
        let proto = ParseProto::load(File::open("test/multi-print.lua").unwrap());
        assert_eq!(proto.constants.len(), 3);
        assert_eq!(proto.constants[0], Value::String("print".to_string()));
        assert_eq!(
            proto.constants[1],
            Value::String("hello, world!".to_string())
        );
        assert_eq!(
            proto.constants[2],
            Value::String("hello, again...".to_string())
        );
        assert_eq!(proto.byte_codes.len(), 6);
        assert_eq!(proto.byte_codes[0], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[1], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        assert_eq!(proto.byte_codes[3], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[4], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_print_keyword() {
        let proto = ParseProto::load(File::open("test/print-keyword.lua").unwrap());
        assert_eq!(proto.constants.len(), 1);
        assert_eq!(proto.constants[0], Value::String("print".to_string()));
        assert_eq!(proto.byte_codes.len(), 12);
        // print(true)
        assert_eq!(proto.byte_codes[0], ByteCode::LoadBool(1, true));
        assert_eq!(proto.byte_codes[1], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        // print(false)
        assert_eq!(proto.byte_codes[3], ByteCode::LoadBool(1, false));
        assert_eq!(proto.byte_codes[4], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
        // print(nil)
        assert_eq!(proto.byte_codes[6], ByteCode::LoadNil(1));
        assert_eq!(proto.byte_codes[7], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[8], ByteCode::Call(0, 1));
        // print(print)
        assert_eq!(proto.byte_codes[9], ByteCode::GetGlobal(1, 0));
        assert_eq!(proto.byte_codes[10], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[11], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_print_numbers() {
        let proto = ParseProto::load(File::open("test/print-numbers.lua").unwrap());
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], Value::String("print".to_string()));
        assert_eq!(proto.constants[1], Value::Float(123.456));
        assert_eq!(proto.byte_codes.len(), 14);
        // print(123)
        assert_eq!(proto.byte_codes[0], ByteCode::LoadInt(1, 123));
        assert_eq!(proto.byte_codes[1], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        // print(123.456)
        assert_eq!(proto.byte_codes[3], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[4], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
        // local a = 123
        assert_eq!(proto.byte_codes[6], ByteCode::LoadInt(0, 123));
        // print(a)
        assert_eq!(proto.byte_codes[7], ByteCode::Move(1, 0));
        assert_eq!(proto.byte_codes[8], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[9], ByteCode::Call(0, 1));
        // local b = 123.456
        assert_eq!(proto.byte_codes[10], ByteCode::LoadConst(1, 1));
        // print(b)
        assert_eq!(proto.byte_codes[11], ByteCode::Move(1, 1));
        assert_eq!(proto.byte_codes[12], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[13], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_print_local_func() {
        let proto = ParseProto::load(File::open("test/print-local-func.lua").unwrap());
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], Value::String("print".to_string()));
        assert_eq!(
            proto.constants[1],
            Value::String("I am a local function.".to_string())
        );
        assert_eq!(proto.byte_codes.len(), 4);
        // local print = print
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(1, 1));
        // print "I am a local function."
        assert_eq!(proto.byte_codes[2], ByteCode::Move(0, 0));
        assert_eq!(proto.byte_codes[3], ByteCode::Call(0, 1));
    }
}
