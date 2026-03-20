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
                    _ => panic!("expected string"),
                }
            }
            Token::Eos => break,
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
