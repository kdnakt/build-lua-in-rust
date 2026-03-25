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
}

pub fn load(input: File) -> ParseProto {
    let mut constants = Vec::new();
    let mut byte_codes = Vec::new();
    let mut locals = Vec::new();
    let mut lex = Lex::new(input);

    loop {
        match lex.next() {
            Token::Name(name) => {
                let ic = add_const(&mut constants, Value::String(name));
                byte_codes.push(ByteCode::GetGlobal(0, ic as u8));

                match lex.next() {
                    Token::ParL => { // '(')
                        let code = match lex.next() {
                            Token::Nil => ByteCode::LoadNil(1),
                            Token::True => ByteCode::LoadBool(1, true),
                            Token::False => ByteCode::LoadBool(1, false),
                            Token::Integer(i) => {
                                if let Ok(val) = i16::try_from(i) {
                                    ByteCode::LoadInt(1, val)
                                } else {
                                    load_const(&mut constants, 1, Value::Integer(i))
                                }
                            }
                            Token::Float(f) => load_const(&mut constants, 1, Value::Float(f)),
                            Token::String(s) => load_const(&mut constants, 1, Value::String(s)),
                            Token::Name(name) => load_var(&mut constants, &locals, 1, name),
                            _ => panic!("invalid argument: {}", lex.ch),
                        };
                        byte_codes.push(code);
                        byte_codes.push(ByteCode::Call(0, 1));
                        if lex.next() != Token::ParR { // ')'
                            panic!("expected `)`");
                        }
                    }
                    Token::String(s) => {
                        let code = load_const(&mut constants, 1, Value::String(s));
                        byte_codes.push(code);
                        byte_codes.push(ByteCode::Call(0, 1));
                    }
                    _ => {
                        dbg!(&lex);
                        dbg!(&byte_codes);
                        dbg!(&constants);
                        dbg!(&locals);
                        dbg!("unexpected token: {:?}", lex.ch);
                        panic!("expected string");
                    }
                }
            }
            Token::Eos => break,
            Token::Local => {
                let var = if let Token::Name(var) = lex.next() {
                    var
                } else {
                    panic!("expected variable");
                };
                if lex.next() != Token::Assign {
                    panic!("expected `=`");
                }

                load_exp(&mut byte_codes, &mut constants, &locals, lex.next(), locals.len());
                locals.push(var);
            }
            t => panic!("unexpected token: {t:?}"),
        }
    }

    dbg!(&constants);
    dbg!(&byte_codes);
    ParseProto {
        constants,
        byte_codes,
    }
}

fn load_const(constants: &mut Vec<Value>, dst: usize, val: Value) -> ByteCode {
    ByteCode::LoadConst(dst as u8, add_const(constants, val) as u8)
}

fn add_const(constants: &mut Vec<Value>, val: Value) -> usize {
    constants.iter().position(|v| *v == val).unwrap_or_else(|| {
        constants.push(val);
        constants.len() - 1
    })
}

fn load_exp(byte_codes: &mut Vec<ByteCode>, constants: &mut Vec<Value>, locals: &Vec<String>, token: Token, dst: usize) {
    let code = match token {
        Token::String(s) => load_const(constants, dst, Value::String(s)),
        Token::Integer(i) => {
            if let Ok(val) = i16::try_from(i) {
                ByteCode::LoadInt(dst as u8, val)
            } else {
                load_const(constants, dst, Value::Integer(i))
            }
        }
        Token::Float(f) => load_const(constants, dst, Value::Float(f)),
        Token::Name(var) => load_var(constants, locals, dst, var),
        _ => panic!("invalid argument"),
    };
    byte_codes.push(code);
}

fn load_var(constants: &mut Vec<Value>, locals: &Vec<String>, dst: usize, name: String) -> ByteCode {
    if let Some(idx) = locals.iter().rposition(|v| v == &name) {
        ByteCode::Move(dst as u8, idx as u8)
    } else {
        let ic = add_const(constants, Value::String(name));
        ByteCode::GetGlobal(dst as u8, ic as u8)
    }
}

mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn test_hello() {
        let proto = load(File::open("test/hello.lua").unwrap());
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], Value::String("print".to_string()));
        assert_eq!(proto.constants[1], Value::String("hello, world!".to_string()));
        assert_eq!(proto.byte_codes.len(), 3);
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_multi_print() {
        let proto = load(File::open("test/multi-print.lua").unwrap());
        assert_eq!(proto.constants.len(), 3);
        assert_eq!(proto.constants[0], Value::String("print".to_string()));
        assert_eq!(proto.constants[1], Value::String("hello, world!".to_string()));
        assert_eq!(proto.constants[2], Value::String("hello, again...".to_string()));
        assert_eq!(proto.byte_codes.len(), 6);
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
    }
}
