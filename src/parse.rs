use std::{cmp::Ordering, io::Read};

use crate::{
    bytecode::ByteCode,
    lex::{Lex, Token},
    value::Value,
};

#[derive(Debug, PartialEq)]
enum ExpDesc {
    Nil,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Local(usize),
    Global(usize),
    Call,
    Index(usize, usize),
    IndexInt(usize, u8),
    IndexField(usize, usize),
}

enum ConstStack {
    Const(usize),
    Stack(usize),
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
                t @ Token::Name(_) | t @ Token::ParL => {
                    let desc = self.prefixexp(t);
                    if desc == ExpDesc::Call {
                        // do nothing
                    } else {
                        self.assignment(desc);
                    }
                }
                Token::Eos => break,
                Token::Local => self.local(),
                t => panic!("unexpected token: {t:?}"),
            }
        }
    }

    fn assignment(&mut self, first_var: ExpDesc) {
        let mut vars = vec![first_var];
        loop {
            match self.lex.next() {
                Token::Comma => {
                    let token = self.lex.next();
                    vars.push(self.prefixexp(token));
                }
                Token::Assign => break,
                t => panic!("unexpected token: {t:?}"),
            }
        }

        let exp_sp0 = self.sp;
        let mut nfexp = 0;
        let last_exp = loop {
            let desc = self.exp();
            if self.lex.peek() == &Token::Comma {
                self.lex.next();
                self.discharge(exp_sp0 + nfexp, desc);
                nfexp += 1;
            } else {
                break desc;
            }
        };

        match (nfexp + 1).cmp(&vars.len()) {
            Ordering::Equal => {
                let last_var = vars.pop().unwrap();
                self.assign_var(last_var, last_exp);
            }
            Ordering::Less => {
                todo!("expand last exp");
            }
            Ordering::Greater => {
                nfexp = vars.len();
            }
        }

        while let Some(var) = vars.pop() {
            nfexp -= 1;
            self.assign_from_stack(var, exp_sp0 + nfexp);
        }
    }

    fn assign_var(&mut self, var: ExpDesc, exp: ExpDesc) {
        if let ExpDesc::Local(idx) = var {
            self.discharge(idx, exp);
        } else {
            match self.discharge_const(exp) {
                ConstStack::Const(i) => self.assign_from_const(var, i),
                ConstStack::Stack(i) => self.assign_from_stack(var, i),
            }
        }
    }

    fn assign_from_stack(&mut self, var: ExpDesc, value: usize) {
        let code = match var {
            ExpDesc::Local(i) => ByteCode::Move(i as u8, value as u8),
            ExpDesc::Global(name) => ByteCode::SetGlobal(name as u8, value as u8),
            ExpDesc::Index(t, key) => ByteCode::SetTable(t as u8, key as u8, value as u8),
            ExpDesc::IndexField(t, key) => ByteCode::SetField(t as u8, key as u8, value as u8),
            ExpDesc::IndexInt(t, key) => ByteCode::SetInt(t as u8, key, value as u8),
            _ => panic!("invalid assignment target"),
        };
        self.byte_codes.push(code);
    }

    fn assign_from_const(&mut self, var: ExpDesc, value: usize) {
        let code = match var {
            ExpDesc::Global(name) => ByteCode::SetGlobalConst(name as u8, value as u8),
            ExpDesc::Index(t, key) => ByteCode::SetTableConst(t as u8, key as u8, value as u8),
            ExpDesc::IndexField(t, key) => ByteCode::SetFieldConst(t as u8, key as u8, value as u8),
            ExpDesc::IndexInt(t, key) => ByteCode::SetIntConst(t as u8, key, value as u8),
            _ => panic!("invalid assignment target"),
        };
        self.byte_codes.push(code);
    }

    fn add_const<T: Into<Value>>(&mut self, val: T) -> usize {
        let val = val.into();
        let constants = &mut self.constants;
        constants.iter().position(|v| v == &val).unwrap_or_else(|| {
            constants.push(val);
            constants.len() - 1
        })
    }

    fn local(&mut self) {
        let mut vars = Vec::new();
        let nexp = loop {
            vars.push(self.read_name());

            match self.lex.peek() {
                Token::Comma => {
                    self.lex.next();
                }
                Token::Assign => {
                    self.lex.next();
                    break self.explist();
                }
                _ => break 0, // no explist
            }
        };

        if nexp < vars.len() {
            let ivar = self.locals.len() + nexp;
            let nnil = vars.len() - nexp;
            self.byte_codes
                .push(ByteCode::LoadNil(ivar as u8, nnil as u8));
        }

        self.locals.append(&mut vars);
    }

    fn prefixexp(&mut self, ahead: Token) -> ExpDesc {
        let sp0 = self.sp;
        let mut desc = match ahead {
            Token::Name(name) => self.simple_name(name),
            Token::ParL => {
                let desc = self.exp();
                self.lex.expect(Token::ParR);
                desc
            }
            t => panic!("unexpected token: {t:?}"),
        };

        loop {
            match self.lex.peek() {
                Token::SqurL => {
                    self.lex.next();
                    let itable = self.discharge_if_needed(sp0, desc);
                    desc = match self.exp() {
                        ExpDesc::Integer(i) if u8::try_from(i).is_ok() => {
                            ExpDesc::IndexInt(itable, u8::try_from(i).unwrap())
                        }
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

    fn discharge_const(&mut self, desc: ExpDesc) -> ConstStack {
        match desc {
            ExpDesc::Nil => ConstStack::Const(self.add_const(())),
            ExpDesc::Bool(b) => ConstStack::Const(self.add_const(b)),
            ExpDesc::Integer(i) => ConstStack::Const(self.add_const(i)),
            ExpDesc::Float(f) => ConstStack::Const(self.add_const(f)),
            ExpDesc::String(s) => ConstStack::Const(self.add_const(s)),
            _ => ConstStack::Stack(self.discharge_top(desc)),
        }
    }

    fn discharge_top(&mut self, desc: ExpDesc) -> usize {
        self.discharge_if_needed(self.sp, desc)
    }

    fn discharge_if_needed(&mut self, dst: usize, desc: ExpDesc) -> usize {
        if let ExpDesc::Local(idx) = desc {
            idx
        } else {
            self.discharge(dst, desc);
            dst
        }
    }

    fn discharge(&mut self, dst: usize, desc: ExpDesc) {
        let code = match desc {
            ExpDesc::Nil => ByteCode::LoadNil(dst as u8, 1),
            ExpDesc::Bool(b) => ByteCode::LoadBool(dst as u8, b),
            ExpDesc::Integer(i) => {
                if let Ok(val) = i16::try_from(i) {
                    ByteCode::LoadInt(dst as u8, val)
                } else {
                    ByteCode::LoadConst(dst as u8, self.add_const(i) as u16)
                }
            }
            ExpDesc::Float(f) => ByteCode::LoadConst(dst as u8, self.add_const(f) as u16),
            ExpDesc::String(s) => ByteCode::LoadConst(dst as u8, self.add_const(s) as u16),
            ExpDesc::Local(src) => {
                if dst != src {
                    ByteCode::Move(dst as u8, src as u8)
                } else {
                    return;
                }
            }
            ExpDesc::Global(ic) => ByteCode::GetGlobal(dst as u8, ic as u8),
            ExpDesc::Index(itable, ikey) => ByteCode::GetTable(dst as u8, itable as u8, ikey as u8),
            ExpDesc::IndexField(itable, ikey) => {
                ByteCode::GetField(dst as u8, itable as u8, ikey as u8)
            }
            ExpDesc::IndexInt(itable, ikey) => ByteCode::GetInt(dst as u8, itable as u8, ikey),
            ExpDesc::Call => todo!(),
        };
        self.byte_codes.push(code);
        self.sp = dst + 1;
    }

    fn exp(&mut self) -> ExpDesc {
        let ahead = self.lex.next();
        self.exp_with_ahead(ahead)
    }

    fn exp_with_ahead(&mut self, ahead: Token) -> ExpDesc {
        match ahead {
            Token::Nil => ExpDesc::Nil,
            Token::True => ExpDesc::Bool(true),
            Token::False => ExpDesc::Bool(false),
            Token::Integer(i) => ExpDesc::Integer(i),
            Token::Float(f) => ExpDesc::Float(f),
            Token::String(s) => ExpDesc::String(String::from_utf8(s).unwrap()),
            Token::Function => todo!("function definition"),
            Token::CurlyL => self.table_constructor(),
            Token::Sub | Token::Not | Token::BitXor | Token::Len => todo!("unary operator"),
            Token::Dots => todo!("dots"),
            t => self.prefixexp(t),
        }
    }

    fn explist(&mut self) -> usize {
        let mut n = 0;
        let sp0 = self.sp;
        loop {
            let desc = self.exp();
            self.discharge(sp0 + n, desc);
            n += 1;
            if self.lex.peek() != &Token::Comma {
                return n;
            }
            self.lex.next();
        }
    }

    fn args(&mut self) -> ExpDesc {
        let ifunc = self.sp - 1;
        let argn = match self.lex.next() {
            Token::ParL => {
                if self.lex.peek() != &Token::ParR {
                    let argn = self.explist();
                    self.lex.expect(Token::ParR);
                    argn
                } else {
                    self.lex.next();
                    0
                }
            }
            Token::CurlyL => {
                self.table_constructor();
                1
            }
            Token::String(s) => {
                self.discharge(ifunc + 1, ExpDesc::String(String::from_utf8(s).unwrap()));
                1
            }
            t => panic!("unexpected token: {t:?}"),
        };
        self.byte_codes
            .push(ByteCode::Call(ifunc as u8, argn as u8));
        ExpDesc::Call
    }

    fn read_name(&mut self) -> String {
        if let Token::Name(name) = self.lex.next() {
            name
        } else {
            panic!("expected name");
        }
    }

    fn table_constructor(&mut self) -> ExpDesc {
        let table = self.sp;
        self.sp += 1;

        let inew = self.byte_codes.len();
        self.byte_codes.push(ByteCode::NewTable(table as u8, 0, 0)); // placeholder

        enum TableEntry {
            Map(
                (
                    fn(u8, u8, u8) -> ByteCode,
                    fn(u8, u8, u8) -> ByteCode,
                    usize,
                ),
            ),
            Array(ExpDesc),
        }

        let mut narray = 0;
        let mut nmap = 0;
        loop {
            let sp0 = self.sp;
            let entry = match self.lex.peek() {
                Token::CurlyR => {
                    // '}'
                    self.lex.next();
                    break;
                }
                Token::SqurL => {
                    // '[' exp ']' = exp
                    self.lex.next();
                    let key = self.exp();
                    self.lex.expect(Token::SqurR); // ']'
                    self.lex.expect(Token::Assign); // '='

                    TableEntry::Map(match key {
                        ExpDesc::Local(i) => (ByteCode::SetTable, ByteCode::SetTableConst, i),
                        ExpDesc::String(s) => (
                            ByteCode::SetField,
                            ByteCode::SetFieldConst,
                            self.add_const(s),
                        ),
                        ExpDesc::Integer(i) if u8::try_from(i).is_ok() => {
                            (ByteCode::SetInt, ByteCode::SetIntConst, i as usize)
                        }
                        ExpDesc::Nil => panic!("nil can not be a table key"),
                        ExpDesc::Float(f) if f.is_nan() => panic!("NaN can not be a table key"),
                        _ => (
                            ByteCode::SetTable,
                            ByteCode::SetTableConst,
                            self.discharge_top(key),
                        ),
                    })
                }
                Token::Name(_) => {
                    let name = self.read_name();
                    if self.lex.peek() == &Token::Assign {
                        self.lex.next();
                        TableEntry::Map((
                            ByteCode::SetField,
                            ByteCode::SetFieldConst,
                            self.add_const(name),
                        ))
                    } else {
                        TableEntry::Array(self.exp_with_ahead(Token::Name(name)))
                    }
                }
                _ => {
                    // exp
                    TableEntry::Array(self.exp())
                }
            };

            match entry {
                TableEntry::Map((op, opk, key)) => {
                    let value = self.exp();
                    let code = match self.discharge_const(value) {
                        ConstStack::Const(i) => opk(table as u8, key as u8, i as u8),
                        ConstStack::Stack(i) => op(table as u8, key as u8, i as u8),
                    };
                    self.byte_codes.push(code);

                    nmap += 1;
                    self.sp = sp0;
                }
                TableEntry::Array(desc) => {
                    self.discharge(sp0, desc);
                    narray += 1;
                    if narray % 2 == 50 {
                        // reset the array members every 50
                        self.byte_codes.push(ByteCode::SetList(table as u8, 50));
                        self.sp = table + 1;
                    }
                }
            }

            match self.lex.next() {
                Token::SemiColon | Token::Comma => (), // yes
                Token::CurlyR => break,                // no
                t => panic!("unexpected token: {t:?}"),
            }
        }
        if self.sp > table + 1 {
            self.byte_codes
                .push(ByteCode::SetList(table as u8, (self.sp - table - 1) as u8));
        }
        self.byte_codes[inew] = ByteCode::NewTable(table as u8, narray, nmap);
        self.sp = table + 1;
        ExpDesc::Local(table)
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
        assert_eq!(proto.byte_codes[7], ByteCode::LoadNil(1, 1));
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

    #[test]
    fn test_print_table() {
        let proto = ParseProto::load(File::open("test/print-table.lua").unwrap());
        assert_eq!(proto.constants.len(), 5);
        assert_eq!(proto.constants[0], "number".to_string().into());
        assert_eq!(proto.constants[1], 6.into());
        assert_eq!(proto.constants[2], "print".to_string().into());
        assert_eq!(proto.constants[3], "text".to_string().into());
        assert_eq!(proto.constants[4], "nested text".to_string().into());
        assert_eq!(proto.byte_codes.len(), 30);
        assert_eq!(proto.byte_codes[0], ByteCode::NewTable(0, 0, 1));
        assert_eq!(proto.byte_codes[1], ByteCode::SetFieldConst(0, 0, 1));
        assert_eq!(proto.byte_codes[2], ByteCode::GetGlobal(1, 2));
        assert_eq!(proto.byte_codes[3], ByteCode::GetField(2, 0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::Call(1, 1));
        assert_eq!(proto.byte_codes[5], ByteCode::NewTable(1, 3, 0));
        assert_eq!(proto.byte_codes[6], ByteCode::LoadConst(2, 3));
        assert_eq!(proto.byte_codes[7], ByteCode::NewTable(3, 2, 0));
        assert_eq!(proto.byte_codes[8], ByteCode::LoadConst(4, 4));
        assert_eq!(proto.byte_codes[9], ByteCode::LoadInt(5, 1432));
        assert_eq!(proto.byte_codes[10], ByteCode::SetList(3, 2));
        assert_eq!(proto.byte_codes[11], ByteCode::LoadBool(4, true));
        assert_eq!(proto.byte_codes[12], ByteCode::SetList(1, 3));
        assert_eq!(proto.byte_codes[13], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[14], ByteCode::Move(3, 1));
        assert_eq!(proto.byte_codes[15], ByteCode::Call(2, 1));
        assert_eq!(proto.byte_codes[16], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[17], ByteCode::GetInt(3, 1, 1));
        assert_eq!(proto.byte_codes[18], ByteCode::Call(2, 1));
        assert_eq!(proto.byte_codes[19], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[20], ByteCode::GetInt(3, 1, 2));
        assert_eq!(proto.byte_codes[21], ByteCode::GetInt(3, 3, 1));
        assert_eq!(proto.byte_codes[22], ByteCode::Call(2, 1));
        assert_eq!(proto.byte_codes[23], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[24], ByteCode::GetInt(3, 1, 2));
        assert_eq!(proto.byte_codes[25], ByteCode::GetInt(3, 3, 2));
        assert_eq!(proto.byte_codes[26], ByteCode::Call(2, 1));
        assert_eq!(proto.byte_codes[27], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[28], ByteCode::GetInt(3, 1, 3));
        assert_eq!(proto.byte_codes[29], ByteCode::Call(2, 1));
    }
}
