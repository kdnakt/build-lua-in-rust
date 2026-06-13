use std::{cmp::Ordering, io::Read};

use crate::{
    bytecode::ByteCode,
    lex::{Lex, Token},
    utils::ftoi,
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
    UnaryOp(fn(u8, u8) -> ByteCode, usize),
    BinaryOp(fn(u8, u8, u8) -> ByteCode, usize, usize),
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
    break_blocks: Vec::<Vec::<usize>>,
    continue_blocks: Vec::<Vec::<(usize, usize)>>,
}

impl<R: Read> ParseProto<R> {
    pub fn load(input: R) -> Self {
        let mut proto = ParseProto {
            constants: Vec::new(),
            byte_codes: Vec::new(),
            sp: 0,
            locals: Vec::new(),
            lex: Lex::new(input),
            break_blocks: Vec::new(),
            continue_blocks: Vec::new(),
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
        assert_eq!(self.block(), Token::Eos);
    }

    fn block(&mut self) -> Token {
        let nvar = self.locals.len();
        let end_token = self.block_scope();
        self.locals.truncate(nvar);
        end_token
    }

    fn block_scope(&mut self) -> Token {
        loop {
            self.sp = self.locals.len();
            match self.lex.next() {
                Token::SemiColon => (),
                t @ Token::Name(_) | t @ Token::ParL => {
                    let desc = self.prefixexp(t);
                    if desc == ExpDesc::Call {
                        // do nothing
                    } else {
                        self.assignment(desc);
                    }
                }
                Token::Local => self.local(),
                Token::If => self.if_stat(),
                Token::While => self.while_stat(),
                Token::Break => self.break_stat(),
                // TODO: handle other statements
                t => break t,
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
        constants
            .iter()
            .position(|v| v.same(&val))
            .unwrap_or_else(|| {
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
                        key => ExpDesc::Index(itable, self.discharge_any(key)),
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
            _ => ConstStack::Stack(self.discharge_any(desc)),
        }
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
            ExpDesc::UnaryOp(op, i) => op(dst as u8, i as u8),
            ExpDesc::BinaryOp(op, left, right) => op(dst as u8, left as u8, right as u8),
        };
        self.byte_codes.push(code);
        self.sp = dst + 1;
    }

    fn exp(&mut self) -> ExpDesc {
        let ahead = self.lex.next();
        self.exp_with_ahead(ahead)
    }

    fn exp_limit(&mut self, limit: i32) -> ExpDesc {
        let ahead = self.lex.next();
        self.do_exp(limit, ahead)
    }

    fn exp_with_ahead(&mut self, ahead: Token) -> ExpDesc {
        self.do_exp(0, ahead)
    }

    fn do_exp(&mut self, limit: i32, ahead: Token) -> ExpDesc {
        let mut desc = match ahead {
            Token::Nil => ExpDesc::Nil,
            Token::True => ExpDesc::Bool(true),
            Token::False => ExpDesc::Bool(false),
            Token::Integer(i) => ExpDesc::Integer(i),
            Token::Float(f) => ExpDesc::Float(f),
            Token::String(s) => ExpDesc::String(String::from_utf8(s).unwrap()),
            Token::Function => todo!("function definition"),
            Token::CurlyL => self.table_constructor(),
            Token::Sub => self.unop_neg(),
            Token::Not => self.unop_not(),
            Token::BitXor => self.unop_bitnot(),
            Token::Len => self.unop_len(),
            Token::Dots => todo!("dots"),
            t => self.prefixexp(t),
        };

        loop {
            let (left_pri, right_pri) = binop_pri(self.lex.peek());
            if left_pri <= limit {
                return desc;
            }

            if !matches!(
                desc,
                ExpDesc::Integer(_) | ExpDesc::Float(_) | ExpDesc::String(_)
            ) {
                desc = ExpDesc::Local(self.discharge_any(desc));
            }
            let binop = self.lex.next();
            let right_desc = self.exp_limit(right_pri);
            desc = self.process_binop(binop, desc, right_desc);
        }
    }

    fn process_binop(&mut self, binop: Token, left: ExpDesc, right: ExpDesc) -> ExpDesc {
        if let Some(r) = fold_const(&binop, &left, &right) {
            return r;
        }
        match binop {
            Token::Add => self.do_binop(
                left,
                right,
                ByteCode::Add,
                ByteCode::AddInt,
                ByteCode::AddConst,
            ),
            Token::Sub => self.do_binop(
                left,
                right,
                ByteCode::Sub,
                ByteCode::SubInt,
                ByteCode::SubConst,
            ),
            Token::Mul => self.do_binop(
                left,
                right,
                ByteCode::Mul,
                ByteCode::MulInt,
                ByteCode::MulConst,
            ),
            Token::Div => self.do_binop(
                left,
                right,
                ByteCode::Div,
                ByteCode::DivInt,
                ByteCode::DivConst,
            ),
            Token::Mod => self.do_binop(
                left,
                right,
                ByteCode::Mod,
                ByteCode::ModInt,
                ByteCode::ModConst,
            ),
            Token::Pow => self.do_binop(
                left,
                right,
                ByteCode::Pow,
                ByteCode::PowInt,
                ByteCode::PowConst,
            ),
            Token::Idiv => self.do_binop(
                left,
                right,
                ByteCode::Idiv,
                ByteCode::IdivInt,
                ByteCode::IdivConst,
            ),
            Token::BitAnd => self.do_binop(
                left,
                right,
                ByteCode::BitAnd,
                ByteCode::BitAndInt,
                ByteCode::BitAndConst,
            ),
            Token::BitOr => self.do_binop(
                left,
                right,
                ByteCode::BitOr,
                ByteCode::BitOrInt,
                ByteCode::BitOrConst,
            ),
            Token::BitXor => self.do_binop(
                left,
                right,
                ByteCode::BitXor,
                ByteCode::BitXorInt,
                ByteCode::BitXorConst,
            ),
            Token::ShiftL => self.do_binop(
                left,
                right,
                ByteCode::ShiftL,
                ByteCode::ShiftLInt,
                ByteCode::ShiftLConst,
            ),
            Token::ShiftR => self.do_binop(
                left,
                right,
                ByteCode::ShiftR,
                ByteCode::ShiftRInt,
                ByteCode::ShiftRConst,
            ),
            Token::Concat => self.do_binop(
                left,
                right,
                ByteCode::Concat,
                ByteCode::ConcatInt,
                ByteCode::ConcatConst,
            ),
            _ => panic!("impossible"),
        }
    }

    fn do_binop(
        &mut self,
        mut left: ExpDesc,
        mut right: ExpDesc,
        opr: fn(u8, u8, u8) -> ByteCode,
        opi: fn(u8, u8, u8) -> ByteCode,
        opk: fn(u8, u8, u8) -> ByteCode,
    ) -> ExpDesc {
        if opr == ByteCode::Add || opr == ByteCode::Mul {
            if matches!(left, ExpDesc::Integer(_) | ExpDesc::Float(_)) {
                (left, right) = (right, left);
            }
        }

        let left = self.discharge_any(left);
        let (op, right) = match right {
            ExpDesc::Integer(i) => {
                if let Ok(i) = u8::try_from(i) {
                    (opi, i as usize)
                } else {
                    (opk, self.add_const(i))
                }
            }
            ExpDesc::Float(f) => (opk, self.add_const(f)),
            _ => (opr, self.discharge_any(right)),
        };

        ExpDesc::BinaryOp(op, left, right)
    }

    fn unop_neg(&mut self) -> ExpDesc {
        match self.exp_unop() {
            ExpDesc::Integer(i) => ExpDesc::Integer(-i),
            ExpDesc::Float(f) => ExpDesc::Float(-f),
            ExpDesc::Nil | ExpDesc::Bool(_) | ExpDesc::String(_) => panic!("invalid - operator"),
            desc => ExpDesc::UnaryOp(ByteCode::Neg, self.discharge_any(desc)),
        }
    }

    fn unop_not(&mut self) -> ExpDesc {
        match self.exp_unop() {
            ExpDesc::Bool(b) => ExpDesc::Bool(!b),
            ExpDesc::Nil => ExpDesc::Bool(true),
            ExpDesc::Integer(_) | ExpDesc::Float(_) | ExpDesc::String(_) => ExpDesc::Bool(false),
            desc => ExpDesc::UnaryOp(ByteCode::Not, self.discharge_any(desc)),
        }
    }

    fn unop_bitnot(&mut self) -> ExpDesc {
        match self.exp_unop() {
            ExpDesc::Integer(i) => ExpDesc::Integer(!i),
            ExpDesc::Nil | ExpDesc::Bool(_) | ExpDesc::Float(_) | ExpDesc::String(_) => {
                panic!("invalid ~ operator")
            }
            desc => ExpDesc::UnaryOp(ByteCode::BitNot, self.discharge_any(desc)),
        }
    }

    fn unop_len(&mut self) -> ExpDesc {
        match self.exp_unop() {
            ExpDesc::String(s) => ExpDesc::Integer(s.len() as i64),
            ExpDesc::Nil | ExpDesc::Bool(_) | ExpDesc::Integer(_) | ExpDesc::Float(_) => {
                panic!("invalid # operator")
            }
            desc => ExpDesc::UnaryOp(ByteCode::Len, self.discharge_any(desc)),
        }
    }

    fn exp_unop(&mut self) -> ExpDesc {
        self.exp_limit(12)
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
                            self.discharge_any(key),
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

    fn while_stat(&mut self) {
        let istart = self.byte_codes.len();

        let icond = self.exp_discharge_any();
        self.lex.expect(Token::Do);

        self.byte_codes.push(ByteCode::Test(0, 0));
        let itest = self.byte_codes.len() - 1;

        self.push_loop_block();
        assert_eq!(self.block(), Token::End);

        let iend = self.byte_codes.len();
        self.byte_codes.push(ByteCode::Jump(-((iend - istart) as i16) - 1));

        self.pop_loop_block(istart);

        self.byte_codes[itest] = ByteCode::Test(icond as u8, (iend - itest) as i16);
    }

    fn push_loop_block(&mut self) {
        self.break_blocks.push(Vec::new());
        self.continue_blocks.push(Vec::new());
    }

    fn pop_loop_block(&mut self, icontinue: usize) {
        // breaks
        let iend = self.byte_codes.len() - 1;
        for i in self.break_blocks.pop().unwrap().into_iter() {
            self.byte_codes[i] = ByteCode::Jump((iend - i) as i16);
        }
        // continues
        let end_nvar = self.locals.len();
        for (i, i_nvar) in self.continue_blocks.pop().unwrap().into_iter() {
            if i_nvar < end_nvar {
                panic!("continue jump into local scope");
            }
            self.byte_codes[i] = ByteCode::Jump((icontinue as isize - i as isize) as i16 - 1);
        }
    }

    fn break_stat(&mut self) {
        if let Some(breaks) = self.break_blocks.last_mut() {
            self.byte_codes.push(ByteCode::Jump(0)); // placeholder
            breaks.push(self.byte_codes.len() - 1);
        } else {
            panic!("break not inside a loop");
        }
    }

    fn if_stat(&mut self) {
        let mut jmp_ends = Vec::new();
        let mut end_token = self.do_if_block(&mut jmp_ends);
        while end_token == Token::Elseif {
            end_token = self.do_if_block(&mut jmp_ends);
        }

        if end_token == Token::Else {
            end_token = self.block();
        }

        assert_eq!(end_token, Token::End);

        let iend = self.byte_codes.len() - 1;
        for i in jmp_ends.into_iter() {
            self.byte_codes[i] = ByteCode::Jump((iend - i) as i16);
        }
    }

    fn do_if_block(&mut self, jmp_ends: &mut Vec<usize>) -> Token {
        let icond = self.exp_discharge_any();
        self.lex.expect(Token::Then);

        self.byte_codes.push(ByteCode::Test(0, 0));
        let itest = self.byte_codes.len() - 1;
        let end_token = self.block();

        if matches!(end_token, Token::Elseif | Token::Else) {
            self.byte_codes.push(ByteCode::Jump(0));
            jmp_ends.push(self.byte_codes.len() - 1);
        }

        let iend = self.byte_codes.len() - 1;
        self.byte_codes[itest] = ByteCode::Test(icond as u8, (iend - itest) as i16);

        end_token
    }

    fn exp_discharge_any(&mut self) -> usize {
        let e = self.exp();
        self.discharge_any(e)
    }

    fn discharge_any(&mut self, desc: ExpDesc) -> usize {
        self.discharge_if_needed(self.sp, desc)
    }
}

fn binop_pri(token: &Token) -> (i32, i32) {
    match token {
        Token::Pow => (14, 13),
        Token::Mul | Token::Div | Token::Mod | Token::Idiv => (11, 11),
        Token::Add | Token::Sub => (10, 10),
        Token::Concat => (9, 8),
        Token::ShiftL | Token::ShiftR => (7, 7),
        Token::BitAnd => (6, 6),
        Token::BitNot => (5, 5),
        Token::BitOr => (4, 4),
        Token::Equal
        | Token::NotEq
        | Token::Less
        | Token::LesEq
        | Token::Greater
        | Token::GreEq => (3, 3),
        Token::And => (2, 2),
        Token::Or => (1, 1),
        _ => (-1, -1),
    }
}

fn fold_const(binop: &Token, left: &ExpDesc, right: &ExpDesc) -> Option<ExpDesc> {
    match binop {
        Token::Add => do_fold_const(left, right, |l, r| l + r, |l, r| l + r),
        Token::Sub => do_fold_const(left, right, |l, r| l - r, |l, r| l - r),
        Token::Mul => do_fold_const(left, right, |l, r| l * r, |l, r| l * r),
        Token::Mod => do_fold_const(left, right, |l, r| l % r, |l, r| l % r),
        Token::Idiv => do_fold_const(left, right, |l, r| l / r, |l, r| (l / r).floor()),

        Token::Div => do_fold_const_float(left, right, |l, r| l / r),
        Token::Pow => do_fold_const_float(left, right, |l, r| l.powf(r)),

        Token::BitAnd => do_fold_const_int(left, right, |l, r| l & r),
        Token::BitOr => do_fold_const_int(left, right, |l, r| l | r),
        Token::BitXor => do_fold_const_int(left, right, |l, r| l ^ r),
        Token::ShiftL => do_fold_const_int(left, right, |l, r| l << r),
        Token::ShiftR => do_fold_const_int(left, right, |l, r| l >> r),

        Token::Concat => {
            if let (ExpDesc::String(l), ExpDesc::String(r)) = (left, right) {
                Some(ExpDesc::String(l.clone() + r.clone().as_str()))
            } else {
                None
            }
        }

        _ => panic!("impossible: {binop:?}"),
    }
}

fn do_fold_const(
    left: &ExpDesc,
    right: &ExpDesc,
    int_op: fn(i64, i64) -> i64,
    float_op: fn(f64, f64) -> f64,
) -> Option<ExpDesc> {
    match (left, right) {
        (ExpDesc::Integer(l), ExpDesc::Integer(r)) => Some(ExpDesc::Integer(int_op(*l, *r))),
        (ExpDesc::Float(l), ExpDesc::Float(r)) => Some(ExpDesc::Float(float_op(*l, *r))),
        (ExpDesc::Float(l), ExpDesc::Integer(r)) => Some(ExpDesc::Float(float_op(*l, *r as f64))),
        (ExpDesc::Integer(l), ExpDesc::Float(r)) => Some(ExpDesc::Float(float_op(*l as f64, *r))),
        (_, _) => None,
    }
}

fn do_fold_const_float(
    left: &ExpDesc,
    right: &ExpDesc,
    float_op: fn(f64, f64) -> f64,
) -> Option<ExpDesc> {
    match (left, right) {
        (ExpDesc::Integer(l), ExpDesc::Integer(r)) => {
            Some(ExpDesc::Float(float_op(*l as f64, *r as f64)))
        }
        (ExpDesc::Float(l), ExpDesc::Float(r)) => Some(ExpDesc::Float(float_op(*l, *r))),
        (ExpDesc::Float(l), ExpDesc::Integer(r)) => Some(ExpDesc::Float(float_op(*l, *r as f64))),
        (ExpDesc::Integer(l), ExpDesc::Float(r)) => Some(ExpDesc::Float(float_op(*l as f64, *r))),
        (_, _) => None,
    }
}

fn do_fold_const_int(
    left: &ExpDesc,
    right: &ExpDesc,
    int_op: fn(i64, i64) -> i64,
) -> Option<ExpDesc> {
    let (i1, i2) = match (left, right) {
        (ExpDesc::Integer(l), ExpDesc::Integer(r)) => (*l, *r),
        (ExpDesc::Float(l), ExpDesc::Float(r)) => (ftoi(*l).unwrap(), ftoi(*r).unwrap()),
        (ExpDesc::Float(l), ExpDesc::Integer(r)) => (ftoi(*l).unwrap(), *r),
        (ExpDesc::Integer(l), ExpDesc::Float(r)) => (*l, ftoi(*r).unwrap()),
        (_, _) => return None,
    };
    Some(ExpDesc::Integer(int_op(i1, i2)))
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

    #[test]
    fn test_unop() {
        let proto = ParseProto::load(File::open("test/unop.lua").unwrap());
        assert_eq!(proto.constants.len(), 1);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.byte_codes.len(), 21);
        // print(-5)
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadInt(1, -5));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1));
        // print(-(-3)))
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadInt(1, 3));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1));
        // print(not true)
        assert_eq!(proto.byte_codes[6], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[7], ByteCode::LoadBool(1, false));
        assert_eq!(proto.byte_codes[8], ByteCode::Call(0, 1));
        // print(not false)
        assert_eq!(proto.byte_codes[9], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[10], ByteCode::LoadBool(1, true));
        assert_eq!(proto.byte_codes[11], ByteCode::Call(0, 1));
        // print(not nil)
        assert_eq!(proto.byte_codes[12], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[13], ByteCode::LoadBool(1, true));
        assert_eq!(proto.byte_codes[14], ByteCode::Call(0, 1));
        // print(~7)
        assert_eq!(proto.byte_codes[15], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[16], ByteCode::LoadInt(1, -8));
        assert_eq!(proto.byte_codes[17], ByteCode::Call(0, 1));
        // print(#"hello")
        assert_eq!(proto.byte_codes[18], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[19], ByteCode::LoadInt(1, 5));
        assert_eq!(proto.byte_codes[20], ByteCode::Call(0, 1));
    }

    #[test]
    fn test_binop() {
        let proto = ParseProto::load(File::open("test/binop.lua").unwrap());
        assert_eq!(proto.constants.len(), 5);
        assert_eq!(proto.constants[0], "g".to_string().into());
        assert_eq!(proto.constants[1], 10.into());
        assert_eq!(proto.constants[2], 1.1.into());
        assert_eq!(proto.constants[3], 2.0.into());
        assert_eq!(proto.constants[4], "print".to_string().into());
        assert_eq!(proto.byte_codes.len(), 23);
        //g=10
        assert_eq!(proto.byte_codes[0], ByteCode::SetGlobalConst(0, 1));
        //local a,b,c=1.1,2.0,100
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(0, 2));
        assert_eq!(proto.byte_codes[2], ByteCode::LoadConst(1, 3));
        assert_eq!(proto.byte_codes[3], ByteCode::LoadInt(2, 100));
        //print(100+g)
        assert_eq!(proto.byte_codes[4], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[5], ByteCode::GetGlobal(4, 0));
        assert_eq!(proto.byte_codes[6], ByteCode::AddInt(4, 4, 100));
        assert_eq!(proto.byte_codes[7], ByteCode::Call(3, 1));
        //print(a-1)
        assert_eq!(proto.byte_codes[8], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[9], ByteCode::SubInt(4, 0, 1));
        assert_eq!(proto.byte_codes[10], ByteCode::Call(3, 1));
        //print(100/c)
        assert_eq!(proto.byte_codes[11], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[12], ByteCode::LoadInt(4, 100));
        assert_eq!(proto.byte_codes[13], ByteCode::Div(4, 4, 2));
        assert_eq!(proto.byte_codes[14], ByteCode::Call(3, 1));
        //print(100>>b)
        assert_eq!(proto.byte_codes[15], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[16], ByteCode::LoadInt(4, 100));
        assert_eq!(proto.byte_codes[17], ByteCode::ShiftR(4, 4, 1));
        assert_eq!(proto.byte_codes[18], ByteCode::Call(3, 1));
        //print(100>>a)
        assert_eq!(proto.byte_codes[19], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[20], ByteCode::LoadInt(4, 100));
        assert_eq!(proto.byte_codes[21], ByteCode::ShiftR(4, 4, 0));
        assert_eq!(proto.byte_codes[22], ByteCode::Call(3, 1));
    }

    #[test]
    fn test_if() {
        let proto = ParseProto::load(File::open("test/if.lua").unwrap());
        assert_eq!(proto.constants.len(), 6);
        assert_eq!(proto.constants[0], "a".to_string().into());
        assert_eq!(proto.constants[1], "print".to_string().into());
        assert_eq!(proto.constants[2], "skip this".to_string().into());
        assert_eq!(proto.constants[3], "I am true".to_string().into());
        assert_eq!(proto.constants[4], "else branch".to_string().into());
        assert_eq!(proto.constants[5], "elseif branch".to_string().into());
        assert_eq!(proto.byte_codes.len(), 38);
        // if a then
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::Test(0, 3));
        // print "skip this"
        assert_eq!(proto.byte_codes[2], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[3], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[4], ByteCode::Call(0, 1));
        // end
        // if print then
        assert_eq!(proto.byte_codes[5], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[6], ByteCode::Test(0, 4));
        // local a = "I am true"
        assert_eq!(proto.byte_codes[7], ByteCode::LoadConst(0, 3));
        assert_eq!(proto.byte_codes[8], ByteCode::GetGlobal(1, 1));
        assert_eq!(proto.byte_codes[9], ByteCode::Move(2, 0));
        assert_eq!(proto.byte_codes[10], ByteCode::Call(1, 1));
        // end
        // print(a) -- should be nil
        assert_eq!(proto.byte_codes[11], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[12], ByteCode::GetGlobal(1, 0));
        assert_eq!(proto.byte_codes[13], ByteCode::Call(0, 1));
        // if a then
        assert_eq!(proto.byte_codes[14], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[15], ByteCode::Test(0, 4));
        // print "skip this"
        assert_eq!(proto.byte_codes[16], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[17], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[18], ByteCode::Call(0, 1));
        // else
        assert_eq!(proto.byte_codes[19], ByteCode::Jump(3));
        // print "else branch"
        assert_eq!(proto.byte_codes[20], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[21], ByteCode::LoadConst(1, 4));
        assert_eq!(proto.byte_codes[22], ByteCode::Call(0, 1));
        // if a then
        assert_eq!(proto.byte_codes[23], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[24], ByteCode::Test(0, 4));
        // print "skip this"
        assert_eq!(proto.byte_codes[25], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[26], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[27], ByteCode::Call(0, 1));
        // elseif print then
        assert_eq!(proto.byte_codes[28], ByteCode::Jump(9));
        assert_eq!(proto.byte_codes[29], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[30], ByteCode::Test(0, 4));
        // print "elseif branch"
        assert_eq!(proto.byte_codes[31], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[32], ByteCode::LoadConst(1, 5));
        assert_eq!(proto.byte_codes[33], ByteCode::Call(0, 1));
        // else
        assert_eq!(proto.byte_codes[34], ByteCode::Jump(3));
        // print "else branch"
        assert_eq!(proto.byte_codes[35], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[36], ByteCode::LoadConst(1, 4));
        assert_eq!(proto.byte_codes[37], ByteCode::Call(0, 1));
        // end
    }
}
