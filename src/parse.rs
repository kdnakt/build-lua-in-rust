use std::io::Read;

use crate::{
    bytecode::ByteCode,
    lex::{Lex, Token},
    value::Value,
};

enum ExpDesc {
    Nil,
    Bool(bool),
    Integer(i16),
    Float(f64),
    String(String),
    Local(usize),
    Global(usize),
    Call,
    Index(usize, usize),
    IndexInt(usize, u8),
    IndexField(usize, usize),
}

#[derive(Debug)]
pub struct ParseProto<R: Read> {
    pub constants: Vec<Value>,
    pub byte_codes: Vec<ByteCode>,
    sp: usize,
    locals: Vec<String>,
    lex: Lex<R>,
}

impl<R: Read> ParseProto<R> {
    pub fn load(input: R) -> Self {
        let mut proto = ParseProto {
            constants: Vec::new(),
            byte_codes: Vec::new(),
            sp: 0,
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
            self.sp = self.locals.len();
            match self.lex.next() {
                Token::SemiColon => continue,
                Token::Name(name) => {
                    if self.lex.peek() == &Token::Assign {
                        self.assignment(name);
                    } else {
                        self.function_call(name);
                    }
                }
                Token::Eos => break,
                Token::Local => self.local(),
                t => panic!("unexpected token: {t:?}"),
            }
        }
    }

    fn assignment(&mut self, name: String) {
        if self.lex.next() != Token::Assign {
            panic!("expected `=`");
        }

        if let Some(i) = self.get_local(&name) {
            // local variable
            self.load_exp(i);
        } else {
            // global variable
            let dst = self.add_const(name) as u8;
            let code = match self.lex.next() {
                Token::Nil => ByteCode::SetGlobalConst(dst, self.add_const(Value::Nil) as u8),
                Token::True => {
                    ByteCode::SetGlobalConst(dst, self.add_const(Value::Boolean(true)) as u8)
                }
                Token::False => {
                    ByteCode::SetGlobalConst(dst, self.add_const(Value::Boolean(false)) as u8)
                }
                Token::Integer(i) => ByteCode::SetGlobalConst(dst, self.add_const(i) as u8),
                Token::Float(f) => ByteCode::SetGlobalConst(dst, self.add_const(f) as u8),
                Token::String(s) => ByteCode::SetGlobalConst(
                    dst,
                    self.add_const(String::from_utf8(s).unwrap()) as u8,
                ),
                Token::Name(var) => {
                    if let Some(i) = self.get_local(&var) {
                        ByteCode::SetGlobal(dst, i as u8)
                    } else {
                        ByteCode::SetGlobalGlobal(dst, self.add_const(var) as u8)
                    }
                }
                _ => panic!("invalid argument"),
            };
            self.byte_codes.push(code);
        }
    }

    fn function_call(&mut self, name: String) {
        let ifunc = self.locals.len();
        let iarg = ifunc + 1;
        let code = self.load_var(ifunc, name);
        self.byte_codes.push(code);

        match self.lex.next() {
            Token::ParL => {
                self.load_exp(iarg);
                if self.lex.next() != Token::ParR {
                    // ')'
                    panic!("expected `)`");
                }
            }
            Token::String(s) => {
                let code = self.load_const(iarg, String::from_utf8(s).unwrap().into());
                self.byte_codes.push(code);
            }
            _ => panic!("expected string"),
        }
        self.byte_codes.push(ByteCode::Call(ifunc as u8, 1));
    }

    fn add_const<T: Into<Value>>(&mut self, val: T) -> usize {
        let val = val.into();
        let constants = &mut self.constants;
        constants.iter().position(|v| v == &val).unwrap_or_else(|| {
            constants.push(val);
            constants.len() - 1
        })
    }

    fn get_local(&self, name: &str) -> Option<usize> {
        self.locals.iter().rposition(|v| v == name)
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
                    self.load_const(dst, i.into())
                }
            }
            Token::Float(f) => self.load_const(dst, f.into()),
            Token::String(s) => self.load_const(dst, String::from_utf8(s).unwrap().into()),
            Token::Name(var) => self.load_var(dst, var),
            _ => panic!("invalid argument"),
        };
        self.byte_codes.push(code);
    }

    fn load_const(&mut self, dst: usize, val: Value) -> ByteCode {
        ByteCode::LoadConst(dst as u8, self.add_const(val) as u16)
    }

    fn load_var(&mut self, dst: usize, name: String) -> ByteCode {
        if let Some(idx) = self.get_local(&name) {
            ByteCode::Move(dst as u8, idx as u8)
        } else {
            let ic = self.add_const(name);
            ByteCode::GetGlobal(dst as u8, ic as u8)
        }
    }

    fn prefixexp(&mut self, ahead: Token) -> ExpDesc {
        let sp0 = self.sp;
        let mut desc = match ahead {
            Token::Name(name) => self.simple_name(name),
            Token::ParL => {
                let desc = self.exp();
                self.lex.expect(Token::ParR);
                desc
            },
            t => panic!("unexpected token: {t:?}"),
        };

        loop {
            match self.lex.peek() {
                Token::SqurL => {
                    self.lex.next();
                    let itable = self.discharge_if_needed(sp0, desc);
                    desc = match self.exp() {
                        ExpDesc::Integer(i)if u8::try_from(i).is_ok() => ExpDesc::IndexInt(itable, u8::try_from(i).unwrap()),
                        ExpDesc::String(s) => ExpDesc::IndexField(itable, self.add_const(s)),
                        _ => panic!("invalid index"),
                    };
                    self.lex.expect(Token::SqurR);
                }
                Token::Dot => {
                    self.lex.next();
                    let name = self.read_name();
                    let itable = self.discharge_if_needed(sp0, desc);
                    desc = ExpDesc::IndexField(itable, self.add_const(name));
                }
                Token::ParL | Token::CurlyL | Token::String(_) => {
                    self.discharge(sp0, desc);
                    desc = self.args();
                }
                _ => {
                    return desc;
                }
            }
        }
    }

    fn simple_name(&mut self, name: String) -> ExpDesc {
        if let Some(i) = self.locals.iter().rposition(|v| v == &name) {
            ExpDesc::Local(i)
        } else {
            ExpDesc::Global(self.add_const(name))
        }
    }

    fn argdsc(&mut self) -> ExpDesc {
        let ifunc = self.sp - 1;
        let argn = match self.lex.next() {
            Token::ParL => {
                if self.lex.peek() == &Token::ParR {
                    let argn = self.explist();
                    self.lex.expect(Token::ParR);
                    argn
                } else {
                    self.lex.next();
                    0
                }
            }
            Token::CurlyL => {
                // table constructor
                todo!()
            }
            Token::String(s) => {
                self.discharge(ifunc + 1, ExpDesc::String(String::from_utf8(s).unwrap()));
                1
            }
            t => panic!("unexpected token: {t:?}"),
        };
        self.byte_codes.push(ByteCode::Call(ifunc as u8, argn as u8));
        ExpDesc::Call
    }

    fn discharge_if_needed(&mut self, sp0: usize, desc: ExpDesc) -> usize {
        todo!()
    }

    fn discharge(&mut self, sp0: usize, desc: ExpDesc) {
        todo!()
    }

    fn exp(&mut self) -> ExpDesc {
        todo!()
    }

    fn explist(&mut self) -> usize {
        todo!()
    }

    fn args(&mut self) -> ExpDesc {
        todo!()
    }

    fn read_name(&mut self) -> String {
        todo!()
    }
}

mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn test_hello() {
        let proto = ParseProto::load(File::open("test/hello.lua").unwrap());
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.constants[1], "hello, world!".to_string().into());
        assert_eq!(proto.byte_codes.len(), 3);
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_multi_print() {
        let proto = ParseProto::load(File::open("test/multi-print.lua").unwrap());
        assert_eq!(proto.constants.len(), 3);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.constants[1], "hello, world!".to_string().into());
        assert_eq!(proto.constants[2], "hello, again...".to_string().into());
        assert_eq!(proto.byte_codes.len(), 6);
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_print_keyword() {
        let proto = ParseProto::load(File::open("test/print-keyword.lua").unwrap());
        assert_eq!(proto.constants.len(), 1);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.byte_codes.len(), 12);
        // print(true)
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadBool(1, true));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        // print(false)
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadBool(1, false));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
        // print(nil)
        assert_eq!(proto.byte_codes[6], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[7], ByteCode::LoadNil(1));
        assert_eq!(proto.byte_codes[8], ByteCode::Call(0, 1));
        // print(print)
        assert_eq!(proto.byte_codes[9], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[10], ByteCode::GetGlobal(1, 0));
        assert_eq!(proto.byte_codes[11], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_print_numbers() {
        let proto = ParseProto::load(File::open("test/print-numbers.lua").unwrap());
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.constants[1], Value::Float(123.456));
        assert_eq!(proto.byte_codes.len(), 14);
        // print(123)
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadInt(1, 123));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        // print(123.456)
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
        // local a = 123
        assert_eq!(proto.byte_codes[6], ByteCode::LoadInt(0, 123));
        // print(a)
        assert_eq!(proto.byte_codes[7], ByteCode::GetGlobal(1, 0));
        assert_eq!(proto.byte_codes[8], ByteCode::Move(2, 0));
        assert_eq!(proto.byte_codes[9], ByteCode::Call(1, 1));
        // local b = 123.456
        assert_eq!(proto.byte_codes[10], ByteCode::LoadConst(1, 1));
        // print(b)
        assert_eq!(proto.byte_codes[11], ByteCode::GetGlobal(2, 0));
        assert_eq!(proto.byte_codes[12], ByteCode::Move(3, 1));
        assert_eq!(proto.byte_codes[13], ByteCode::Call(2, 1));
    }

    #[test]
    fn test_print_local_func() {
        let proto = ParseProto::load(File::open("test/print-local-func.lua").unwrap());
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(
            proto.constants[1],
            "I am a local function.".to_string().into()
        );
        assert_eq!(proto.byte_codes.len(), 4);
        // local print = print
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::Move(1, 0));
        // print "I am a local function."
        assert_eq!(proto.byte_codes[2], ByteCode::LoadConst(2, 1));
        assert_eq!(proto.byte_codes[3], ByteCode::Call(1, 1));
    }
}
