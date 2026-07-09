use std::{cmp::Ordering, io::Read};
use std::rc::Rc;

use crate::{
    bytecode::ByteCode::{self},
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
    
    Function(Value),
    Call(usize, usize),
    VarArgs,
    Index(usize, usize),
    IndexInt(usize, u8),
    IndexField(usize, usize),
    UnaryOp(fn(u8, u8) -> ByteCode, usize),
    BinaryOp(fn(u8, u8, u8) -> ByteCode, usize, usize),
    Test(Box<ExpDesc>, Vec<usize>, Vec<usize>), // (condition, true list, false list)
    Compare(
        fn(u8, u8, bool) -> ByteCode,
        usize,
        usize,
        Vec<usize>,
        Vec<usize>,
    ),
}

enum ConstStack {
    Const(usize),
    Stack(usize),
}

#[derive(Debug)]
struct GotoLabel {
    name: String,
    icode: usize,
    nvar: usize,
}

#[derive(Debug)]
pub struct FuncProto {
    pub has_varargs: bool,
    pub constants: Vec<Value>,
    pub byte_codes: Vec<ByteCode>,
}

#[derive(Debug)]
pub struct ParseProto<'a, R: Read> {
    fp: FuncProto,
    sp: usize,
    locals: Vec<String>,
    lex: &'a mut Lex<R>,
    break_blocks: Vec<Vec<usize>>,
    continue_blocks: Vec<Vec<(usize, usize)>>,
    gotos: Vec<GotoLabel>,
    labels: Vec<GotoLabel>,
}

impl<'a, R: Read> ParseProto<'a, R> {
    pub fn new(lex: &'a mut Lex<R>, has_varargs: bool, params: Vec<String>) -> Self {
        ParseProto {
            fp: FuncProto {
                has_varargs: has_varargs,
                constants: Vec::new(),
                byte_codes: Vec::new(),
            },
            sp: 0,
            locals: params,
            lex: lex,
            break_blocks: Vec::new(),
            continue_blocks: Vec::new(),
            gotos: Vec::new(),
            labels: Vec::new(),
        }
    }

    fn block(&mut self) -> Token {
        let nvar = self.locals.len();
        let end_token = self.block_scope();
        self.locals.truncate(nvar);
        end_token
    }

    fn block_scope(&mut self) -> Token {
        let igoto = self.gotos.len();
        let ilabel = self.labels.len();
        loop {
            self.sp = self.locals.len();
            match self.lex.next() {
                Token::SemiColon => (),
                t @ Token::Name(_) | t @ Token::ParL => {
                    if self.try_continue_stat(&t) {
                        continue;
                    }

                    let desc = self.prefixexp(t);
                    if let ExpDesc::Call(ifunc, narg_plus) = desc {
                        self.fp.byte_codes.push(ByteCode::Call(ifunc as u8, narg_plus as u8, 0));
                    } else {
                        self.assignment(desc);
                    }
                }
                Token::Local =>
                    if self.lex.peek() == &Token::Function {
                        self.local_function()
                    } else {
                        self.local_variables()
                    }
                Token::If => self.if_stat(),
                Token::While => self.while_stat(),
                Token::Do => self.do_stat(),
                Token::Break => self.break_stat(),
                Token::Repeat => self.repeat_stat(),
                Token::For => self.for_stat(),
                Token::DoubColon => self.label_stat(),
                Token::Goto => self.goto_stat(),
                t => {
                    self.close_goto_labels(igoto, ilabel);
                    break t;
                }
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
        self.fp.byte_codes.push(code);
    }

    fn assign_from_const(&mut self, var: ExpDesc, value: usize) {
        let code = match var {
            ExpDesc::Global(name) => ByteCode::SetGlobalConst(name as u8, value as u8),
            ExpDesc::Index(t, key) => ByteCode::SetTableConst(t as u8, key as u8, value as u8),
            ExpDesc::IndexField(t, key) => ByteCode::SetFieldConst(t as u8, key as u8, value as u8),
            ExpDesc::IndexInt(t, key) => ByteCode::SetIntConst(t as u8, key, value as u8),
            _ => panic!("invalid assignment target"),
        };
        self.fp.byte_codes.push(code);
    }

    fn add_const<T: Into<Value>>(&mut self, val: T) -> usize {
        let val = val.into();
        let constants = &mut self.fp.constants;
        constants
            .iter()
            .position(|v| v.same(&val))
            .unwrap_or_else(|| {
                constants.push(val);
                constants.len() - 1
            })
    }

    fn local_function(&mut self) {
        self.lex.next();
        let name = self.read_name();
        println!("== function: {name}");
        let f = self.funcbody(false);
        self.discharge(self.sp, f);
        self.locals.push(name);
    }

    fn local_variables(&mut self) {
        let mut vars = vec![self.read_name()];
        while self.lex.peek() == &Token::Comma {
            self.lex.next();
            vars.push(self.read_name());
        }
        if self.lex.peek() == &Token::Assign {
            self.lex.next();
            let want = vars.len();
            let (nexp, last_exp) = self.explist();
            match (nexp + 1).cmp(&want) {
                Ordering::Equal => {
                    self.discharge(self.sp, last_exp);
                }
                Ordering::Less => {
                    self.discharge_expand_want(last_exp, want - nexp);
                }
                Ordering::Greater => {
                    self.sp -= nexp - want;
                }
            }
        } else {
            self.fp.byte_codes.push(ByteCode::LoadNil(self.sp as u8, vars.len() as u8));
        }
        self.locals.append(&mut vars);
    }

    fn funcbody(&mut self, with_self: bool) -> ExpDesc {
        let mut has_varargs = false;
        let mut params = Vec::new();
        if with_self {
            params.push("self".to_string());
        }
        self.lex.expect(Token::ParL);
        loop {
            match self.lex.next() {
                Token::ParR => break,
                Token::Dots => {
                    has_varargs = true;
                    self.lex.expect(Token::ParR);
                    break;
                }
                Token::Name(name) => {
                    params.push(name);
                    match self.lex.next() {
                        Token::Comma => continue,
                        Token::ParR => break,
                        t => panic!("unexpected token: {t:?}"),
                    }
                }
                t => panic!("unexpected token: {t:?}"),
            }
        }
        let proto = chunk(self.lex, has_varargs, params, Token::End);
        ExpDesc::Function(Value::LuaFunction(Rc::new(proto)))
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
                    desc = self.args(0);
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

    fn discharge_expand_want(&mut self, desc: ExpDesc, want: usize) {
        let code = match desc {
            ExpDesc::Call(ifunc, narg_plus) => ByteCode::Call(ifunc as u8, narg_plus as u8, want as u8),
            ExpDesc::VarArgs => ByteCode::VarArgs(self.sp as u8, want as u8),
            _ => {
                self.discharge(self.sp, desc);
                ByteCode::LoadNil(self.sp as u8, want as u8 - 1)
            }
        };
        self.fp.byte_codes.push(code);
    }
    
    fn discharge_expand(&mut self, desc: ExpDesc) -> bool {
        let code = match desc {
            ExpDesc::Call(ifunc, narg_plus) => 
            ByteCode::CallSet(ifunc as u8, narg_plus as u8, 1),
            ExpDesc::VarArgs => ByteCode::VarArgs(self.sp as u8, 0), 
            _ => {
                self.discharge(self.sp, desc);
                return false
            }
        };
        self.fp.byte_codes.push(code);
        true
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
            ExpDesc::Call(ifunc, narg_plus) => ByteCode::CallSet(dst as u8, ifunc as u8, narg_plus as u8),
            ExpDesc::UnaryOp(op, i) => op(dst as u8, i as u8),
            ExpDesc::BinaryOp(op, left, right) => op(dst as u8, left as u8, right as u8),
            ExpDesc::Test(condition, true_list, false_list) => {
                self.discharge(dst, *condition);
                self.fix_test_set_list(true_list, dst);
                self.fix_test_set_list(false_list, dst);
                return;
            }
            ExpDesc::Compare(op, left, right, true_list, false_list) => {
                self.fp.byte_codes.push(op(left as u8, right as u8, false));
                self.fp.byte_codes.push(ByteCode::Jump(1));
                self.fix_test_list(false_list);
                self.fp.byte_codes.push(ByteCode::SetFalseSkip(dst as u8));
                self.fix_test_list(true_list);
                ByteCode::LoadBool(dst as u8, true)
            }
            _ => panic!("invalid expression for discharge"),
        };
        self.fp.byte_codes.push(code);
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
            let binop = self.lex.next();
            desc = self.process_binop_left(desc, &binop);
            let right_desc = self.exp_limit(right_pri);
            desc = self.process_binop(binop, desc, right_desc);
        }
    }

    fn process_binop_left(&mut self, left: ExpDesc, binop: &Token) -> ExpDesc {
        match binop {
            Token::And => {
                ExpDesc::Test(Box::new(ExpDesc::Nil), Vec::new(), self.test_or_jump(left))
            }
            Token::Or => ExpDesc::Test(Box::new(ExpDesc::Nil), self.test_or_jump(left), Vec::new()),
            _ => {
                if matches!(
                    left,
                    ExpDesc::Integer(_) | ExpDesc::Float(_) | ExpDesc::String(_)
                ) {
                    left
                } else {
                    ExpDesc::Local(self.discharge_any(left))
                }
            }
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
            Token::Equal => self.do_compare(
                left,
                right,
                ByteCode::Equal,
                ByteCode::EqualInt,
                ByteCode::EqualConst,
            ),
            Token::NotEq => self.do_compare(
                left,
                right,
                ByteCode::NotEq,
                ByteCode::NotEqInt,
                ByteCode::NotEqConst,
            ),
            Token::LesEq => self.do_compare(
                left,
                right,
                ByteCode::LesEq,
                ByteCode::LesEqInt,
                ByteCode::LesEqConst,
            ),
            Token::GreEq => self.do_compare(
                left,
                right,
                ByteCode::GreEq,
                ByteCode::GreEqInt,
                ByteCode::GreEqConst,
            ),
            Token::Less => self.do_compare(
                left,
                right,
                ByteCode::Less,
                ByteCode::LessInt,
                ByteCode::LessConst,
            ),
            Token::Greater => self.do_compare(
                left,
                right,
                ByteCode::Greater,
                ByteCode::GreaterInt,
                ByteCode::GreaterConst,
            ),
            Token::And | Token::Or => {
                if let ExpDesc::Test(_, mut left_true_list, mut left_false_list) = left {
                    match right {
                        ExpDesc::Compare(op, l, r, mut right_true_list, mut right_false_list) => {
                            left_true_list.append(&mut right_true_list);
                            left_false_list.append(&mut right_false_list);
                            ExpDesc::Compare(op, l, r, left_true_list, left_false_list)
                        }
                        ExpDesc::Test(condition, mut right_true_list, mut right_false_list) => {
                            left_true_list.append(&mut right_true_list);
                            left_false_list.append(&mut right_false_list);
                            ExpDesc::Test(condition, left_true_list, left_false_list)
                        }
                        _ => ExpDesc::Test(Box::new(right), left_true_list, left_false_list),
                    }
                } else {
                    panic!("impossible: {left:?}");
                }
            }
            _ => panic!("impossible"),
        }
    }

    fn do_compare(
        &mut self,
        mut left: ExpDesc,
        mut right: ExpDesc,
        opr: fn(u8, u8, bool) -> ByteCode,
        opi: fn(u8, u8, bool) -> ByteCode,
        opk: fn(u8, u8, bool) -> ByteCode,
    ) -> ExpDesc {
        if opr == ByteCode::Equal || opr == ByteCode::NotEq {
            if matches!(left, ExpDesc::Integer(_) | ExpDesc::Float(_)) {
                (left, right) = (right, left)
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
            ExpDesc::String(s) => (opk, self.add_const(s)),
            _ => (opr, self.discharge_any(right)),
        };
        ExpDesc::Compare(op, left, right, Vec::new(), Vec::new())
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

    fn explist(&mut self) -> (usize, ExpDesc) {
        let mut n = 0;
        let sp0 = self.sp;
        loop {
            let desc = self.exp();
            if self.lex.peek() != &Token::Comma {
                self.sp = sp0 + n;
                return (n, desc);
            }
            self.lex.next();
            self.discharge(sp0 + n, desc);
            n += 1;
        }
    }

    fn args(&mut self, implicit_argn: usize) -> ExpDesc {
        let ifunc = self.sp - implicit_argn;
        let narg = match self.lex.next() {
            Token::ParL => {
                if self.lex.peek() != &Token::ParR {
                    let (nexp, last_exp) = self.explist();
                    self.lex.expect(Token::ParR);
                    if self.discharge_expand(last_exp) {
                        None
                    } else {
                        Some(nexp + 1)
                    }
                } else {
                    self.lex.next();
                    Some(0)
                }
            }
            Token::CurlyL => {
                self.table_constructor();
                Some(1)
            }
            Token::String(s) => {
                self.discharge(ifunc + 1, ExpDesc::String(String::from_utf8(s).unwrap()));
                Some(1)
            }
            t => panic!("unexpected token: {t:?}"),
        };
        let narg_plus = if let Some(n) = narg { n + implicit_argn + 1 } else { 0 };
        ExpDesc::Call(ifunc, narg_plus)
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

        let inew = self.fp.byte_codes.len();
        self.fp.byte_codes.push(ByteCode::NewTable(table as u8, 0, 0)); // placeholder

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
                    self.fp.byte_codes.push(code);

                    nmap += 1;
                    self.sp = sp0;
                }
                TableEntry::Array(desc) => {
                    self.discharge(sp0, desc);
                    narray += 1;
                    if narray % 2 == 50 {
                        // reset the array members every 50
                        self.fp.byte_codes.push(ByteCode::SetList(table as u8, 50));
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
            self.fp.byte_codes
                .push(ByteCode::SetList(table as u8, (self.sp - table - 1) as u8));
        }
        self.fp.byte_codes[inew] = ByteCode::NewTable(table as u8, narray, nmap);
        self.sp = table + 1;
        ExpDesc::Local(table)
    }

    fn while_stat(&mut self) {
        let istart = self.fp.byte_codes.len();

        let icond = self.exp_discharge_any();
        self.lex.expect(Token::Do);

        self.fp.byte_codes.push(ByteCode::Test(0, 0));
        let itest = self.fp.byte_codes.len() - 1;

        self.push_loop_block();
        assert_eq!(self.block(), Token::End);

        let iend = self.fp.byte_codes.len();
        self.fp.byte_codes
            .push(ByteCode::Jump(-((iend - istart) as i16) - 1));

        self.pop_loop_block(istart);

        self.fp.byte_codes[itest] = ByteCode::Test(icond as u8, (iend - itest) as i16);
    }

    fn push_loop_block(&mut self) {
        self.break_blocks.push(Vec::new());
        self.continue_blocks.push(Vec::new());
    }

    fn pop_loop_block(&mut self, icontinue: usize) {
        // breaks
        let iend = self.fp.byte_codes.len() - 1;
        for i in self.break_blocks.pop().unwrap().into_iter() {
            self.fp.byte_codes[i] = ByteCode::Jump((iend - i) as i16);
        }
        // continues
        let end_nvar = self.locals.len();
        for (i, i_nvar) in self.continue_blocks.pop().unwrap().into_iter() {
            if i_nvar < end_nvar {
                panic!("continue jump into local scope");
            }
            self.fp.byte_codes[i] = ByteCode::Jump((icontinue as isize - i as isize) as i16 - 1);
        }
    }

    fn do_stat(&mut self) {
        assert_eq!(self.block(), Token::End);
    }

    fn try_continue_stat(&mut self, name: &Token) -> bool {
        if let Token::Name(name) = name {
            if name.as_str() != "continue" {
                return false;
            }
            if !matches!(self.lex.peek(), Token::End | Token::Elseif | Token::Else) {
                return false;
            }

            if let Some(continues) = self.continue_blocks.last_mut() {
                self.fp.byte_codes.push(ByteCode::Jump(0));
                continues.push((self.fp.byte_codes.len() - 1, self.locals.len()));
            } else {
                panic!("continue outside loop");
            }
            true
        } else {
            false
        }
    }

    fn break_stat(&mut self) {
        if let Some(breaks) = self.break_blocks.last_mut() {
            self.fp.byte_codes.push(ByteCode::Jump(0)); // placeholder
            breaks.push(self.fp.byte_codes.len() - 1);
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

        let iend = self.fp.byte_codes.len() - 1;
        for i in jmp_ends.into_iter() {
            self.fp.byte_codes[i] = ByteCode::Jump((iend - i) as i16);
        }
    }

    fn do_if_block(&mut self, jmp_ends: &mut Vec<usize>) -> Token {
        let condition = self.exp();
        let false_list = self.test_or_jump(condition);
        self.lex.expect(Token::Then);

        let end_token = self.block();

        if matches!(end_token, Token::Elseif | Token::Else) {
            self.fp.byte_codes.push(ByteCode::Jump(0));
            jmp_ends.push(self.fp.byte_codes.len() - 1);
        }

        self.fix_test_list(false_list);
        end_token
    }

    fn test_or_jump(&mut self, condition: ExpDesc) -> Vec<usize> {
        let (code, true_list, mut false_list) = match condition {
            ExpDesc::Bool(true) | ExpDesc::Integer(_) | ExpDesc::Float(_) | ExpDesc::String(_) => {
                return Vec::new();
            }
            ExpDesc::Compare(op, left, right, true_list, false_list) => {
                self.fp.byte_codes.push(op(left as u8, right as u8, true));
                (ByteCode::Jump(0), Some(true_list), false_list)
            }
            ExpDesc::Test(condition, true_list, false_list) => {
                let icondition = self.discharge_any(*condition);
                (
                    ByteCode::TestOrJump(icondition as u8, 0),
                    Some(true_list),
                    false_list,
                )
            }
            _ => {
                let icondition = self.discharge_any(condition);
                (ByteCode::TestOrJump(icondition as u8, 0), None, Vec::new())
            }
        };
        self.fp.byte_codes.push(code);
        false_list.push(self.fp.byte_codes.len() - 1);
        if let Some(true_list) = true_list {
            self.fix_test_list(true_list);
        }
        false_list
    }

    fn fix_test_list(&mut self, list: Vec<usize>) {
        let here = self.fp.byte_codes.len();
        self.fix_test_list_to(list, here);
    }

    fn fix_test_list_to(&mut self, list: Vec<usize>, to: usize) {
        for i in list.into_iter() {
            let jmp = (to as isize - i as isize - 1) as i16;
            let code = match self.fp.byte_codes[i] {
                ByteCode::Jump(0) => ByteCode::Jump(jmp),
                ByteCode::TestAndJump(icondition, 0) => ByteCode::TestAndJump(icondition, jmp),
                ByteCode::TestOrJump(icondition, 0) => ByteCode::TestOrJump(icondition, jmp),
                _ => panic!("invalid test"),
            };
            self.fp.byte_codes[i] = code;
        }
    }

    fn fix_test_set_list(&mut self, list: Vec<usize>, dst: usize) {
        let here = self.fp.byte_codes.len();
        let dst = dst as u8;
        for i in list.into_iter() {
            let jmp = here - i - 1;
            let code = match self.fp.byte_codes[i] {
                ByteCode::Jump(0) => ByteCode::Jump(jmp as i16),
                ByteCode::TestOrJump(icondition, 0) => {
                    if icondition == dst {
                        ByteCode::TestOrJump(icondition, jmp as i16)
                    } else {
                        ByteCode::TestOrSetJump(dst as u8, icondition, jmp as u8)
                    }
                }
                ByteCode::TestAndJump(icondition, 0) => {
                    if icondition == dst {
                        ByteCode::TestAndJump(icondition, jmp as i16)
                    } else {
                        ByteCode::TestAndSetJump(dst as u8, icondition, jmp as u8)
                    }
                }
                _ => panic!("invalid test"),
            };
            self.fp.byte_codes[i] = code;
        }
    }

    fn repeat_stat(&mut self) {
        let istart = self.fp.byte_codes.len();

        self.push_loop_block();

        let nvar = self.locals.len();
        assert_eq!(self.block_scope(), Token::Until);

        let iend1 = self.fp.byte_codes.len();
        let icond = self.exp_discharge_any();
        let iend2 = self.fp.byte_codes.len();

        self.fp.byte_codes
            .push(ByteCode::Test(icond as u8, -((iend2 - istart + 1) as i16)));
        self.pop_loop_block(iend1);
        self.locals.truncate(nvar);
    }

    fn for_stat(&mut self) {
        let name = self.read_name();
        if self.lex.peek() == &Token::Assign {
            self.for_numerical(name);
        } else {
            todo!("generic for");
        }
    }

    fn label_stat(&mut self) {
        let name = self.read_name();
        self.lex.expect(Token::DoubColon);

        if self.labels.iter().any(|l| l.name == name) {
            panic!("duplicate label {}", name);
        }
        self.labels.push(GotoLabel {
            name,
            icode: self.fp.byte_codes.len(),
            nvar: self.locals.len(),
        });
    }

    fn goto_stat(&mut self) {
        let name = self.read_name();
        self.fp.byte_codes.push(ByteCode::Jump(0));
        self.gotos.push(GotoLabel {
            name,
            icode: self.fp.byte_codes.len() - 1,
            nvar: self.locals.len(),
        });
    }

    fn close_goto_labels(&mut self, igoto: usize, ilabel: usize) {
        let mut no_dsts = Vec::new();
        for goto in self.gotos.drain(igoto..) {
            if let Some(label) = self.labels.iter().rev().find(|l| l.name == goto.name) {
                if label.icode != self.fp.byte_codes.len() && label.nvar > goto.nvar {
                    panic!("goto jump into scope {}", goto.name);
                }
                let d = (label.icode as isize - goto.icode as isize) as i16;
                self.fp.byte_codes[goto.icode] = ByteCode::Jump(d - 1);
            } else {
                no_dsts.push(goto);
            }
        }
        self.gotos.append(&mut no_dsts);
        self.labels.truncate(ilabel);
    }

    fn for_numerical(&mut self, name: String) {
        self.lex.next(); // '='
        let (nexp, last_exp) = self.explist();
        self.discharge(self.sp, last_exp);
        match nexp + 1 {
            2 => self.discharge(self.sp, ExpDesc::Integer(1)),
            3 => (),
            _ => panic!("invalid numerical for exp"),
        }

        self.locals.push(name);
        self.locals.push(String::from(""));
        self.locals.push(String::from(""));

        self.lex.expect(Token::Do);
        self.fp.byte_codes.push(ByteCode::ForPrepare(0, 0));
        let iprepare = self.fp.byte_codes.len() - 1;
        let iname = self.sp - 3;

        self.push_loop_block();
        assert_eq!(self.block(), Token::End);

        self.locals.pop();
        self.locals.pop();
        self.locals.pop();

        let d = self.fp.byte_codes.len() - iprepare;
        self.fp.byte_codes
            .push(ByteCode::ForLoop(iname as u8, d as u16));
        self.fp.byte_codes[iprepare] = ByteCode::ForPrepare(iname as u8, d as u16);

        self.pop_loop_block(self.fp.byte_codes.len() - 1);
    }

    fn exp_discharge_any(&mut self) -> usize {
        let e = self.exp();
        self.discharge_any(e)
    }

    fn discharge_any(&mut self, desc: ExpDesc) -> usize {
        self.discharge_if_needed(self.sp, desc)
    }
}

pub fn load(input: impl Read) -> FuncProto {
    let mut lex = Lex::new(input);
    chunk(&mut lex, false, Vec::new(), Token::Eos)
}

fn chunk(lex: &mut Lex<impl Read>, has_varargs: bool, params: Vec<String>, end_token: Token) -> FuncProto {
    let mut proto = ParseProto::new(lex, has_varargs, params);
    assert_eq!(proto.block_scope(), end_token);
    if let Some(goto) = proto.gotos.first() {
        panic!("goto {} no destination", &goto.name);
    }
    proto.fp
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

        _ => None,
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

    fn load_proto(file: &str) -> FuncProto {
        let file = File::open(file).unwrap();
        load(file)
    }

    #[test]
    fn test_hello() {
        let proto = load_proto("test/hello.lua");
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.constants[1], "hello, world!".to_string().into());
        assert_eq!(proto.byte_codes.len(), 3);
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1, 0));
    }

    #[test]
    fn test_multi_print() {
        let proto = load_proto("test/multi-print.lua");
        assert_eq!(proto.constants.len(), 3);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.constants[1], "hello, world!".to_string().into());
        assert_eq!(proto.constants[2], "hello, again...".to_string().into());
        assert_eq!(proto.byte_codes.len(), 6);
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1, 0));
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1, 0));
    }

    #[test]
    fn test_print_keyword() {
        let proto = load_proto("test/print-keyword.lua");
        assert_eq!(proto.constants.len(), 1);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.byte_codes.len(), 12);
        // print(true)
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadBool(1, true));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1, 0));
        // print(false)
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadBool(1, false));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1, 0));
        // print(nil)
        assert_eq!(proto.byte_codes[6], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[7], ByteCode::LoadNil(1, 1));
        assert_eq!(proto.byte_codes[8], ByteCode::Call(0, 1, 0));
        // print(print)
        assert_eq!(proto.byte_codes[9], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[10], ByteCode::GetGlobal(1, 0));
        assert_eq!(proto.byte_codes[11], ByteCode::Call(0, 1, 0));
    }

    #[test]
    fn test_print_numbers() {
        let proto = load_proto("test/print-numbers.lua");
        assert_eq!(proto.constants.len(), 2);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.constants[1], Value::Float(123.456));
        assert_eq!(proto.byte_codes.len(), 14);
        // print(123)
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadInt(1, 123));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1, 0));
        // print(123.456)
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadConst(1, 1));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1, 0));
        // local a = 123
        assert_eq!(proto.byte_codes[6], ByteCode::LoadInt(0, 123));
        // print(a)
        assert_eq!(proto.byte_codes[7], ByteCode::GetGlobal(1, 0));
        assert_eq!(proto.byte_codes[8], ByteCode::Move(2, 0));
        assert_eq!(proto.byte_codes[9], ByteCode::Call(1, 1, 0));
        // local b = 123.456
        assert_eq!(proto.byte_codes[10], ByteCode::LoadConst(1, 1));
        // print(b)
        assert_eq!(proto.byte_codes[11], ByteCode::GetGlobal(2, 0));
        assert_eq!(proto.byte_codes[12], ByteCode::Move(3, 1));
        assert_eq!(proto.byte_codes[13], ByteCode::Call(2, 1, 0));
    }

    #[test]
    fn test_print_local_func() {
        let proto = load_proto("test/print-local-func.lua");
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
        assert_eq!(proto.byte_codes[3], ByteCode::Call(1, 1, 0));
    }

    #[test]
    fn test_print_table() {
        let proto = load_proto("test/print-table.lua");
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
        assert_eq!(proto.byte_codes[4], ByteCode::Call(1, 1, 0));
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
        assert_eq!(proto.byte_codes[15], ByteCode::Call(2, 1, 0));
        assert_eq!(proto.byte_codes[16], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[17], ByteCode::GetInt(3, 1, 1));
        assert_eq!(proto.byte_codes[18], ByteCode::Call(2, 1, 0));
        assert_eq!(proto.byte_codes[19], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[20], ByteCode::GetInt(3, 1, 2));
        assert_eq!(proto.byte_codes[21], ByteCode::GetInt(3, 3, 1));
        assert_eq!(proto.byte_codes[22], ByteCode::Call(2, 1, 0));
        assert_eq!(proto.byte_codes[23], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[24], ByteCode::GetInt(3, 1, 2));
        assert_eq!(proto.byte_codes[25], ByteCode::GetInt(3, 3, 2));
        assert_eq!(proto.byte_codes[26], ByteCode::Call(2, 1, 0));
        assert_eq!(proto.byte_codes[27], ByteCode::GetGlobal(2, 2));
        assert_eq!(proto.byte_codes[28], ByteCode::GetInt(3, 1, 3));
        assert_eq!(proto.byte_codes[29], ByteCode::Call(2, 1, 0));
    }

    #[test]
    fn test_unop() {
        let proto = load_proto("test/unop.lua");
        assert_eq!(proto.constants.len(), 1);
        assert_eq!(proto.constants[0], "print".to_string().into());
        assert_eq!(proto.byte_codes.len(), 21);
        // print(-5)
        assert_eq!(proto.byte_codes[0], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[1], ByteCode::LoadInt(1, -5));
        assert_eq!(proto.byte_codes[2], ByteCode::Call(0, 1, 0));
        // print(-(-3)))
        assert_eq!(proto.byte_codes[3], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[4], ByteCode::LoadInt(1, 3));
        assert_eq!(proto.byte_codes[5], ByteCode::Call(0, 1, 0));
        // print(not true)
        assert_eq!(proto.byte_codes[6], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[7], ByteCode::LoadBool(1, false));
        assert_eq!(proto.byte_codes[8], ByteCode::Call(0, 1, 0));
        // print(not false)
        assert_eq!(proto.byte_codes[9], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[10], ByteCode::LoadBool(1, true));
        assert_eq!(proto.byte_codes[11], ByteCode::Call(0, 1, 0));
        // print(not nil)
        assert_eq!(proto.byte_codes[12], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[13], ByteCode::LoadBool(1, true));
        assert_eq!(proto.byte_codes[14], ByteCode::Call(0, 1, 0));
        // print(~7)
        assert_eq!(proto.byte_codes[15], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[16], ByteCode::LoadInt(1, -8));
        assert_eq!(proto.byte_codes[17], ByteCode::Call(0, 1, 0));
        // print(#"hello")
        assert_eq!(proto.byte_codes[18], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[19], ByteCode::LoadInt(1, 5));
        assert_eq!(proto.byte_codes[20], ByteCode::Call(0, 1, 0));
    }

    #[test]
    fn test_binop() {
        let proto = load_proto("test/binop.lua");
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
        assert_eq!(proto.byte_codes[7], ByteCode::Call(3, 1, 0));
        //print(a-1)
        assert_eq!(proto.byte_codes[8], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[9], ByteCode::SubInt(4, 0, 1));
        assert_eq!(proto.byte_codes[10], ByteCode::Call(3, 1, 0));
        //print(100/c)
        assert_eq!(proto.byte_codes[11], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[12], ByteCode::LoadInt(4, 100));
        assert_eq!(proto.byte_codes[13], ByteCode::Div(4, 4, 2));
        assert_eq!(proto.byte_codes[14], ByteCode::Call(3, 1, 0));
        //print(100>>b)
        assert_eq!(proto.byte_codes[15], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[16], ByteCode::LoadInt(4, 100));
        assert_eq!(proto.byte_codes[17], ByteCode::ShiftR(4, 4, 1));
        assert_eq!(proto.byte_codes[18], ByteCode::Call(3, 1, 0));
        //print(100>>a)
        assert_eq!(proto.byte_codes[19], ByteCode::GetGlobal(3, 4));
        assert_eq!(proto.byte_codes[20], ByteCode::LoadInt(4, 100));
        assert_eq!(proto.byte_codes[21], ByteCode::ShiftR(4, 4, 0));
        assert_eq!(proto.byte_codes[22], ByteCode::Call(3, 1, 0));
    }

    #[test]
    fn test_if() {
        let proto = load_proto("test/if.lua");
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
        assert_eq!(proto.byte_codes[1], ByteCode::TestOrJump(0, 3));
        // print "skip this"
        assert_eq!(proto.byte_codes[2], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[3], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[4], ByteCode::Call(0, 1, 0));
        // end
        // if print then
        assert_eq!(proto.byte_codes[5], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[6], ByteCode::TestOrJump(0, 4));
        // local a = "I am true"
        assert_eq!(proto.byte_codes[7], ByteCode::LoadConst(0, 3));
        assert_eq!(proto.byte_codes[8], ByteCode::GetGlobal(1, 1));
        assert_eq!(proto.byte_codes[9], ByteCode::Move(2, 0));
        assert_eq!(proto.byte_codes[10], ByteCode::Call(1, 1, 0));
        // end
        // print(a) -- should be nil
        assert_eq!(proto.byte_codes[11], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[12], ByteCode::GetGlobal(1, 0));
        assert_eq!(proto.byte_codes[13], ByteCode::Call(0, 1, 0));
        // if a then
        assert_eq!(proto.byte_codes[14], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[15], ByteCode::TestOrJump(0, 4));
        // print "skip this"
        assert_eq!(proto.byte_codes[16], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[17], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[18], ByteCode::Call(0, 1, 0));
        // else
        assert_eq!(proto.byte_codes[19], ByteCode::Jump(3));
        // print "else branch"
        assert_eq!(proto.byte_codes[20], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[21], ByteCode::LoadConst(1, 4));
        assert_eq!(proto.byte_codes[22], ByteCode::Call(0, 1, 0));
        // if a then
        assert_eq!(proto.byte_codes[23], ByteCode::GetGlobal(0, 0));
        assert_eq!(proto.byte_codes[24], ByteCode::TestOrJump(0, 4));
        // print "skip this"
        assert_eq!(proto.byte_codes[25], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[26], ByteCode::LoadConst(1, 2));
        assert_eq!(proto.byte_codes[27], ByteCode::Call(0, 1, 0));
        // elseif print then
        assert_eq!(proto.byte_codes[28], ByteCode::Jump(9));
        assert_eq!(proto.byte_codes[29], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[30], ByteCode::TestOrJump(0, 4));
        // print "elseif branch"
        assert_eq!(proto.byte_codes[31], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[32], ByteCode::LoadConst(1, 5));
        assert_eq!(proto.byte_codes[33], ByteCode::Call(0, 1, 0));
        // else
        assert_eq!(proto.byte_codes[34], ByteCode::Jump(3));
        // print "else branch"
        assert_eq!(proto.byte_codes[35], ByteCode::GetGlobal(0, 1));
        assert_eq!(proto.byte_codes[36], ByteCode::LoadConst(1, 4));
        assert_eq!(proto.byte_codes[37], ByteCode::Call(0, 1, 0));
        // end
    }
}
