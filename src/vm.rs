use std::{
    cell::{Ref, RefCell},
    io::Write,
    rc::Rc,
};

use crate::{
    bytecode::ByteCode,
    parse::{FuncProto, UpIndex},
    utils::ftoi,
    value::{
        LuaClosure, Table,
        Upvalue::{self},
        Value::{self},
    },
};

struct OpenBroker {
    ilocal: usize,
    broker: Rc<RefCell<Upvalue>>,
}

impl From<usize> for OpenBroker {
    fn from(ilocal: usize) -> Self {
        Self {
            ilocal,
            broker: Rc::new(RefCell::new(Upvalue::Open(ilocal))),
        }
    }
}

pub struct ExeState {
    stack: Vec<Value>,
    base: usize,
}

fn lib_print(state: &mut ExeState) -> i32 {
    for i in 1..=state.get_top() {
        if i != 1 {
            print!("\t");
        }
        print!("{}", state.get::<&Value>(i).to_string());
    }
    println!("");
    0
}

fn lib_type(state: &mut ExeState) -> i32 {
    let ty = state.get::<&Value>(1).ty();
    state.push(ty);
    1
}

impl ExeState {
    pub fn new() -> Self {
        let mut env = Table::new(0, 0);
        env.map
            .insert("print".into(), Value::RustFunction(lib_print));
        env.map.insert("type".into(), Value::RustFunction(lib_type));
        Self {
            stack: vec![Value::Nil, Value::Table(Rc::new(RefCell::new(env)))],
            base: 1,
        }
    }

    pub fn execute(&mut self, proto: &FuncProto, upvalues: &Vec<Rc<RefCell<Upvalue>>>) -> usize {
        let mut open_brokers: Vec<OpenBroker> = Vec::new();

        if self.stack.len() < self.base + proto.nparam as usize {
            self.fill_stack_nil(0, proto.nparam);
        }

        let varargs = if proto.has_varargs {
            self.stack.drain(self.base + proto.nparam..).collect()
        } else {
            Vec::new()
        };

        let mut pc = 0; // bytecode index
        loop {
            println!("  [{pc}]\t{:?}", proto.byte_codes[pc]);
            match proto.byte_codes[pc] {
                ByteCode::LoadConst(dst, idx) => {
                    let v = proto.constants[idx as usize].clone();
                    self.set_stack(dst, v);
                }
                ByteCode::LoadNil(dst, n) => {
                    self.fill_stack(dst as usize, n as usize);
                }
                ByteCode::LoadBool(dst, b) => {
                    self.set_stack(dst, Value::Boolean(b));
                }
                ByteCode::LoadInt(dst, i) => {
                    self.set_stack(dst, (i as i64).into());
                }
                ByteCode::Move(dst, i) => {
                    let v = self.get_stack(i).clone();
                    self.set_stack(dst, v);
                }
                ByteCode::Call(func, narg_plus, want_nret) => {
                    let nret = self.call_function(func, narg_plus);
                    let iret = self.stack.len() - nret;
                    self.stack.drain(self.base + func as usize..iret);
                    let want_nret = want_nret as usize;
                    if nret < want_nret {
                        self.fill_stack(nret, want_nret - nret);
                    }
                }
                ByteCode::NewTable(dst, narray, nmap) => {
                    let t = Table::new(narray as usize, nmap as usize);
                    self.set_stack(
                        dst,
                        Value::Table(std::rc::Rc::new(std::cell::RefCell::new(t))),
                    );
                }
                ByteCode::SetTable(table, key, value) => {
                    let key = self.get_stack(key).clone();
                    let value = self.get_stack(value).clone();
                    if let Value::Table(t) = &self.stack[table as usize] {
                        t.borrow_mut().map.insert(key, value);
                    } else {
                        panic!("not table");
                    }
                }
                ByteCode::SetTableConst(t, k, v) => {
                    let key = self.get_stack(k).clone();
                    let value = proto.constants[v as usize].clone();
                    self.get_stack(t).new_index(key, value);
                }
                ByteCode::SetInt(t, i, v) => {
                    let value = self.get_stack(v).clone();
                    self.get_stack(t).new_index_array(i as i64, value);
                }
                ByteCode::SetIntConst(t, i, v) => {
                    let value = proto.constants[v as usize].clone();
                    self.get_stack(t).new_index_array(i as i64, value);
                }
                ByteCode::SetField(table, key, value) => {
                    let key = proto.constants[key as usize].clone();
                    let value = self.get_stack(value).clone();
                    self.get_stack(table).new_index(key, value);
                }
                ByteCode::SetFieldConst(t, k, v) => {
                    let key = proto.constants[k as usize].clone();
                    let value = proto.constants[v as usize].clone();
                    self.get_stack(t).new_index(key, value);
                }
                ByteCode::SetList(table, n) => {
                    let ivalue = self.base + table as usize + 1;
                    if let Value::Table(t) = &self.get_stack(table).clone() {
                        let end = if n == 0 {
                            self.stack.len()
                        } else {
                            ivalue + n as usize
                        };
                        let values = self.stack.drain(ivalue..end);
                        t.borrow_mut().array.extend(values);
                    } else {
                        panic!("not table");
                    }
                }
                ByteCode::GetInt(dst, t, k) => {
                    let value = self.get_stack(t).index_array(k as i64);
                    self.set_stack(dst, value);
                }
                ByteCode::GetField(dst, t, k) => {
                    let key = &proto.constants[k as usize];
                    let value = self.get_stack(t).index(key);
                    self.set_stack(dst, value);
                }
                ByteCode::GetTable(dst, t, k) => {
                    let key = self.get_stack(k);
                    let value = self.get_stack(t).index(key);
                    self.set_stack(dst, value);
                }
                ByteCode::Neg(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::Integer(i) => Value::Integer(-i),
                        Value::Float(f) => Value::Float(-f),
                        _ => panic!("invalid -"),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::Not(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::Nil => Value::Boolean(true),
                        Value::Boolean(b) => Value::Boolean(!b),
                        _ => Value::Boolean(false),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::Len(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::ShortStr(len, _) => Value::Integer(*len as i64),
                        Value::MidStr(rc) => Value::Integer(rc.0 as i64),
                        Value::LongStr(s) => Value::Integer(s.len() as i64),
                        Value::Table(t) => Value::Integer(t.borrow().array.len() as i64),
                        _ => panic!("invalid length operator"),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::BitNot(dst, src) => {
                    let value = match &self.get_stack(src) {
                        Value::Integer(i) => Value::Integer(!i),
                        _ => panic!("invalid bitwise not operator"),
                    };
                    self.set_stack(dst, value);
                }
                // binops
                ByteCode::Add(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        self.get_stack(b),
                        |x, y| x + y,
                        |x, y| x + y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::AddInt(dst, a, i) => {
                    let r = exe_binop_int(self.get_stack(a), i, |x, y| x + y, |x, y| x + y);
                    self.set_stack(dst, r);
                }
                ByteCode::AddConst(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        &proto.constants[b as usize],
                        |x, y| x + y,
                        |x, y| x + y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Sub(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        self.get_stack(b),
                        |x, y| x - y,
                        |x, y| x - y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::SubInt(dst, a, i) => {
                    let r = exe_binop_int(self.get_stack(a), i, |x, y| x - y, |x, y| x - y);
                    self.set_stack(dst, r);
                }
                ByteCode::SubConst(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        &proto.constants[b as usize],
                        |x, y| x - y,
                        |x, y| x - y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Mul(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        self.get_stack(b),
                        |x, y| x * y,
                        |x, y| x * y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::MulInt(dst, a, i) => {
                    let r = exe_binop_int(self.get_stack(a), i, |x, y| x * y, |x, y| x * y);
                    self.set_stack(dst, r);
                }
                ByteCode::MulConst(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        &proto.constants[b as usize],
                        |x, y| x * y,
                        |x, y| x * y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Mod(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        self.get_stack(b),
                        |x, y| x % y,
                        |x, y| x % y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::ModInt(dst, a, i) => {
                    let r = exe_binop_int(self.get_stack(a), i, |x, y| x % y, |x, y| x % y);
                    self.set_stack(dst, r);
                }
                ByteCode::ModConst(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        &proto.constants[b as usize],
                        |x, y| x % y,
                        |x, y| x % y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Idiv(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        self.get_stack(b),
                        |x, y| x / y,
                        |x, y| x / y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::IdivInt(dst, a, i) => {
                    let r = exe_binop_int(self.get_stack(a), i, |x, y| x / y, |x, y| x / y);
                    self.set_stack(dst, r);
                }
                ByteCode::IdivConst(dst, a, b) => {
                    let r = exe_binop(
                        self.get_stack(a),
                        &proto.constants[b as usize],
                        |x, y| x / y,
                        |x, y| x / y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Div(dst, a, b) => {
                    let r = exe_binop_f(self.get_stack(a), self.get_stack(b), |x, y| x / y);
                    self.set_stack(dst, r);
                }
                ByteCode::DivInt(dst, a, i) => {
                    let r = exe_binop_int_f(self.get_stack(a), i, |x, y| x / y);
                    self.set_stack(dst, r);
                }
                ByteCode::DivConst(dst, a, b) => {
                    let r = exe_binop_f(self.get_stack(a), &proto.constants[b as usize], |x, y| {
                        x / y
                    });
                    self.set_stack(dst, r);
                }
                ByteCode::Pow(dst, a, b) => {
                    let r = exe_binop_f(self.get_stack(a), self.get_stack(b), |x, y| x.powf(y));
                    self.set_stack(dst, r);
                }
                ByteCode::PowInt(dst, a, i) => {
                    let r = exe_binop_int_f(self.get_stack(a), i, |x, y| x.powf(y));
                    self.set_stack(dst, r);
                }
                ByteCode::PowConst(dst, a, b) => {
                    let r = exe_binop_f(self.get_stack(a), &proto.constants[b as usize], |x, y| {
                        x.powf(y)
                    });
                    self.set_stack(dst, r);
                }
                ByteCode::BitAnd(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), self.get_stack(b), |x, y| x & y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitAndInt(dst, a, i) => {
                    let r = exe_binop_int_i(self.get_stack(a), i, |x, y| x & y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitAndConst(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), &proto.constants[b as usize], |x, y| {
                        x & y
                    });
                    self.set_stack(dst, r);
                }
                ByteCode::BitOr(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), self.get_stack(b), |x, y| x | y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitOrInt(dst, a, i) => {
                    let r = exe_binop_int_i(self.get_stack(a), i, |x, y| x | y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitOrConst(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), &proto.constants[b as usize], |x, y| {
                        x | y
                    });
                    self.set_stack(dst, r);
                }
                ByteCode::BitXor(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), self.get_stack(b), |x, y| x ^ y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitXorInt(dst, a, i) => {
                    let r = exe_binop_int_i(self.get_stack(a), i, |x, y| x ^ y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitXorConst(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), &proto.constants[b as usize], |x, y| {
                        x ^ y
                    });
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftL(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), self.get_stack(b), |x, y| x << y);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftLInt(dst, a, i) => {
                    let r = exe_binop_int_i(self.get_stack(a), i, |x, y| x << y);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftLConst(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), &proto.constants[b as usize], |x, y| {
                        x << y
                    });
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftR(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), self.get_stack(b), |x, y| x >> y);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftRInt(dst, a, i) => {
                    let r = exe_binop_int_i(self.get_stack(a), i, |x, y| x >> y);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftRConst(dst, a, b) => {
                    let r = exe_binop_i(self.get_stack(a), &proto.constants[b as usize], |x, y| {
                        x >> y
                    });
                    self.set_stack(dst, r);
                }
                ByteCode::Concat(dst, a, b) => {
                    let r = exe_concat(self.get_stack(a), self.get_stack(b));
                    self.set_stack(dst, r);
                }
                ByteCode::ConcatInt(dst, a, i) => {
                    let r = exe_concat(self.get_stack(a), &Value::Integer(i as i64));
                    self.set_stack(dst, r);
                }
                ByteCode::ConcatConst(dst, a, b) => {
                    let r = exe_concat(self.get_stack(a), &proto.constants[b as usize]);
                    self.set_stack(dst, r);
                }
                ByteCode::Test(icond, jmp) => {
                    let cond = self.get_stack(icond);
                    if matches!(cond, Value::Nil | Value::Boolean(false)) {
                        pc = (pc as isize + jmp as isize) as usize;
                    }
                }
                ByteCode::Jump(jmp) => {
                    pc = (pc as isize + jmp as isize) as usize;
                }
                ByteCode::ForPrepare(dst, jmp) => {
                    if let (&Value::Integer(mut i), &Value::Integer(step)) =
                        (self.get_stack(dst), self.get_stack(dst + 2))
                    {
                        if step == 0 {
                            panic!("0 step in numeric for");
                        }
                        let limit = match self.get_stack(dst + 1) {
                            &Value::Integer(limit) => limit,
                            &Value::Float(limit) => {
                                let limit = for_int_limit(limit, step > 0, &mut i);
                                self.set_stack(dst + 1, Value::Integer(limit));
                                limit
                            }
                            _ => panic!("invalid limit in numeric for"),
                        };
                        if !for_check(i, limit, step > 0) {
                            pc += jmp as usize;
                        }
                    } else {
                        let i = self.make_float(dst);
                        let limit = self.make_float(dst + 1);
                        let step = self.make_float(dst + 2);
                        if step == 0.0 {
                            panic!("0 step in numeric for");
                        }
                        if !for_check(i, limit, step > 0.0) {
                            pc += jmp as usize;
                        }
                    }
                }
                ByteCode::ForLoop(dst, jmp) => match self.get_stack(dst) {
                    Value::Integer(i) => {
                        let limit = self.read_int(dst + 1);
                        let step = self.read_int(dst + 2);
                        let i = i + step;
                        if for_check(i, limit, step > 0) {
                            self.set_stack(dst, Value::Integer(i));
                            pc -= jmp as usize;
                        }
                    }
                    Value::Float(f) => {
                        let limit = self.read_float(dst + 1);
                        let step = self.read_float(dst + 2);
                        let i = f + step;
                        if for_check(i, limit, step > 0.0) {
                            self.set_stack(dst, Value::Float(i));
                            pc -= jmp as usize;
                        }
                    }
                    _ => panic!("invalid for loop"),
                },
                ByteCode::ForCallLoop(iter, nvar, jmp) => {
                    let nret = self.call_function(iter, 2 + 1);
                    let iret = self.stack.len() - nret;

                    if nret > 0 && self.stack[iret] != Value::Nil {
                        let first_ret = self.stack[iret].clone();
                        self.set_stack(iter + 2, first_ret);
                        self.stack.drain(self.base + iter as usize + 3..iret);
                        self.fill_stack_nil(iter + 3, nvar as usize);
                        pc -= jmp as usize;
                    } else if jmp == 0 {
                        pc += 1;
                    }
                }
                ByteCode::TestAndJump(icondition, jmp) => {
                    if (self.get_stack(icondition)).into() {
                        pc = (pc as isize + jmp as isize) as usize;
                    }
                }
                ByteCode::TestOrJump(icondition, jmp) => {
                    if (self.get_stack(icondition)).into() {
                        // do nothing
                    } else {
                        pc = (pc as isize + jmp as isize) as usize;
                    }
                }
                ByteCode::TestAndSetJump(dst, icondition, jmp) => {
                    let condition = self.get_stack(icondition);
                    if condition.into() {
                        self.set_stack(dst, condition.clone());
                        pc += jmp as usize;
                    }
                }
                ByteCode::TestOrSetJump(dst, icondition, jmp) => {
                    let condition = self.get_stack(icondition);
                    if condition.into() {
                        // do nothing
                    } else {
                        self.set_stack(dst, condition.clone());
                        pc += jmp as usize;
                    }
                }
                ByteCode::SetFalseSkip(dst) => {
                    self.set_stack(dst, Value::Boolean(false));
                    pc += 1;
                }
                ByteCode::Equal(a, b, r) => {
                    if (&self.get_stack(a) == &self.get_stack(b)) == r {
                        pc += 1;
                    }
                }
                ByteCode::EqualConst(a, b, r) => {
                    if (self.get_stack(a) == &proto.constants[b as usize]) == r {
                        pc += 1;
                    }
                }
                ByteCode::EqualInt(a, i, r) => {
                    if let &Value::Integer(ii) = self.get_stack(a) {
                        if (ii == i as i64) == r {
                            pc += 1;
                        }
                    }
                }
                ByteCode::NotEq(a, b, r) => {
                    if (self.get_stack(a) != self.get_stack(b)) == r {
                        pc += 1;
                    }
                }
                ByteCode::NotEqConst(a, b, r) => {
                    if (self.get_stack(a) != &proto.constants[b as usize]) == r {
                        pc += 1;
                    }
                }
                ByteCode::NotEqInt(a, i, r) => {
                    if let &Value::Integer(ii) = self.get_stack(a) {
                        if (ii != i as i64) == r {
                            pc += 1;
                        }
                    }
                }
                ByteCode::LesEq(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if (!matches!(cmp, std::cmp::Ordering::Greater)) == r {
                        pc += 1;
                    }
                }
                ByteCode::LesEqConst(a, b, r) => {
                    let cmp = self
                        .get_stack(a)
                        .partial_cmp(&proto.constants[b as usize])
                        .unwrap();
                    if (!matches!(cmp, std::cmp::Ordering::Greater)) == r {
                        pc += 1;
                    }
                }
                ByteCode::LesEqInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid comparison"),
                    };
                    if (a <= i as i64) == r {
                        pc += 1;
                    }
                }
                ByteCode::Greater(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if matches!(cmp, std::cmp::Ordering::Greater) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreaterConst(a, b, r) => {
                    let cmp = self
                        .get_stack(a)
                        .partial_cmp(&proto.constants[b as usize])
                        .unwrap();
                    if matches!(cmp, std::cmp::Ordering::Greater) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreaterInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid comparison"),
                    };
                    if (a > i as i64) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreEq(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if !matches!(cmp, std::cmp::Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreEqConst(a, b, r) => {
                    let cmp = self
                        .get_stack(a)
                        .partial_cmp(&proto.constants[b as usize])
                        .unwrap();
                    if !matches!(cmp, std::cmp::Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::GreEqInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid comparison"),
                    };
                    if (a >= i as i64) == r {
                        pc += 1;
                    }
                }
                ByteCode::Less(a, b, r) => {
                    let cmp = self.get_stack(a).partial_cmp(self.get_stack(b)).unwrap();
                    if matches!(cmp, std::cmp::Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::LessConst(a, b, r) => {
                    let cmp = self
                        .get_stack(a)
                        .partial_cmp(&proto.constants[b as usize])
                        .unwrap();
                    if matches!(cmp, std::cmp::Ordering::Less) == r {
                        pc += 1;
                    }
                }
                ByteCode::LessInt(a, i, r) => {
                    let a = match self.get_stack(a) {
                        &Value::Integer(i) => i,
                        &Value::Float(f) => f as i64,
                        _ => panic!("invalid comparison"),
                    };
                    if (a < i as i64) == r {
                        pc += 1;
                    }
                }
                ByteCode::CallSet(dst, func, narg_plus) => {
                    let nret = self.call_function(func, narg_plus);

                    if nret == 0 {
                        self.set_stack(dst, Value::Nil);
                    } else {
                        let iret = self.stack.len() - nret;
                        self.stack.swap(self.base + dst as usize, iret);
                    }
                    self.stack.truncate(self.base + func as usize + 1);
                }
                ByteCode::TailCall(func, narg_plus) => {
                    self.stack.drain(self.base - 1..self.base + func as usize);
                    return self.do_call_function(narg_plus);
                }
                ByteCode::Return(iret, nret) => {
                    let iret = self.base + iret as usize;
                    if nret == 0 {
                        return self.stack.len() - iret;
                    } else {
                        self.stack.truncate(iret + nret as usize);
                        return nret as usize;
                    }
                }
                ByteCode::Return0 => {
                    return 0;
                }
                ByteCode::VarArgs(dst, want) => {
                    self.stack.truncate(self.base + dst as usize);

                    let len = varargs.len();
                    let want = want as usize;
                    if want == 0 {
                        self.stack.extend_from_slice(&varargs);
                    } else if want > len {
                        self.stack.extend_from_slice(&varargs);
                        self.fill_stack(dst as usize + len, want - len);
                    } else {
                        self.stack.extend_from_slice(&varargs[..want]);
                    }
                }
                ByteCode::SetUpField(t, k, v) => {
                    let key = proto.constants[k as usize].clone();
                    let value = self.get_stack(v).clone();
                    upvalues[t as usize]
                        .borrow()
                        .get(&self.stack)
                        .new_index(key, value);
                }
                ByteCode::SetUpFieldConst(t, k, v) => {
                    let key = proto.constants[k as usize].clone();
                    let value = proto.constants[v as usize].clone();
                    upvalues[t as usize]
                        .borrow()
                        .get(&self.stack)
                        .new_index(key, value);
                }
                ByteCode::GetUpField(dst, t, k) => {
                    let key = &proto.constants[k as usize];
                    let value = upvalues[t as usize]
                        .borrow()
                        .get(&self.stack)
                        .index(key)
                        .clone();
                    self.set_stack(dst, value);
                }
                ByteCode::Close(ilocal) => {
                    let ilocal = self.base + ilocal as usize;
                    let from = open_brokers
                        .binary_search_by_key(&ilocal, |b| b.ilocal)
                        .unwrap_or_else(|i| i);
                    self.close_brokers(open_brokers.drain(from..));
                }
                ByteCode::GetUpvalue(dst, src) => {
                    let v = upvalues[src as usize].borrow().get(&self.stack).clone();
                    self.set_stack(dst, v);
                }
                ByteCode::SetUpvalue(dst, src) => {
                    let v = self.get_stack(src).clone();
                    upvalues[dst as usize].borrow_mut().set(&mut self.stack, v);
                }
                ByteCode::SetUpvalueConst(dst, src) => {
                    let v = proto.constants[src as usize].clone();
                    upvalues[dst as usize].borrow_mut().set(&mut self.stack, v);
                }
                ByteCode::Closure(dst, inner) => {
                    let Value::LuaFunction(inner_proto) = proto.constants[inner as usize].clone()
                    else {
                        panic!("not function");
                    };
                    let inner_upvalues = inner_proto
                        .upindexes
                        .iter()
                        .map(|up| match up {
                            &UpIndex::Upvalue(i) => upvalues[i].clone(),
                            &UpIndex::Local(ilocal) => {
                                let ilocal = self.base + ilocal;
                                let iob = open_brokers
                                    .binary_search_by_key(&ilocal, |b| b.ilocal)
                                    .unwrap_or_else(|i| {
                                        open_brokers.insert(i, OpenBroker::from(ilocal));
                                        i
                                    });
                                open_brokers[iob].broker.clone()
                            }
                        })
                        .collect();

                    let c = LuaClosure {
                        upvalues: inner_upvalues,
                        proto: inner_proto,
                    };
                    self.set_stack(dst, Value::LuaClosure(Rc::new(c)));
                }
            }

            // next bytecode
            pc += 1;
        }
    }

    fn get_stack(&self, idx: u8) -> &Value {
        &self.stack[self.base + idx as usize]
    }

    fn set_stack(&mut self, idx: u8, val: Value) {
        set_vec(&mut self.stack, self.base + idx as usize, val);
    }

    fn fill_stack(&mut self, begin: usize, num: usize) {
        let begin = self.base + begin;
        let end = begin + num;
        let len = self.stack.len();
        if begin < len {
            self.stack[begin..len].fill(Value::Nil);
        }
        if end > len {
            self.stack.resize(end, Value::Nil);
        }
    }

    fn fill_stack_nil(&mut self, begin: u8, to: usize) {
        self.stack
            .resize(self.base + begin as usize + to, Value::Nil);
    }

    fn make_float(&mut self, dst: u8) -> f64 {
        match self.stack[dst as usize] {
            Value::Float(f) => f,
            Value::Integer(i) => {
                let f = i as f64;
                self.set_stack(dst, Value::Float(f));
                f
            }
            // TODO convert string
            ref v => panic!("not number {v:?}"),
        }
    }

    fn read_int(&self, dst: u8) -> i64 {
        if let Value::Integer(i) = self.stack[dst as usize] {
            i
        } else {
            panic!("not integer");
        }
    }

    fn read_float(&self, dst: u8) -> f64 {
        if let Value::Float(f) = self.stack[dst as usize] {
            f
        } else {
            panic!("not float");
        }
    }

    fn call_function(&mut self, func: u8, narg_plus: u8) -> usize {
        self.base += func as usize + 1;
        let nret = self.do_call_function(narg_plus);
        self.base -= func as usize + 1;
        nret
    }

    fn do_call_function(&mut self, narg_plus: u8) -> usize {
        if narg_plus != 0 {
            self.stack.truncate(self.base + narg_plus as usize - 1);
        }

        match self.stack[self.base - 1].clone() {
            Value::RustFunction(f) => f(self) as usize,
            Value::RustClosure(c) => c.borrow_mut()(self) as usize,
            Value::LuaFunction(f) => self.execute(&f, &Vec::new()),
            v => panic!("invalid function: {v:?}"),
        }
    }

    fn close_brokers(&mut self, brokers: impl IntoIterator<Item = OpenBroker>) {
        for OpenBroker { ilocal, broker } in brokers {
            let openi = broker.replace(Upvalue::Closed(self.stack[ilocal].clone()));
            debug_assert_eq!(openi, Upvalue::Open(ilocal));
        }
    }
}

impl<'a> ExeState {
    pub fn get_top(&self) -> usize {
        self.stack.len() - self.base
    }
    pub fn get<T>(&'a self, i: usize) -> T
    where
        T: From<&'a Value>,
    {
        (&self.stack[self.base + i - 1]).into()
    }
    pub fn push(&mut self, v: impl Into<Value>) {
        self.stack.push(v.into())
    }
}

fn for_check<T: PartialOrd>(i: T, limit: T, is_step_positive: bool) -> bool {
    if is_step_positive {
        i <= limit
    } else {
        i >= limit
    }
}

fn for_int_limit(limit: f64, is_step_positive: bool, i: &mut i64) -> i64 {
    if is_step_positive {
        if limit < i64::MIN as f64 {
            *i = 0;
            -1
        } else {
            limit.floor() as i64
        }
    } else {
        if limit > i64::MAX as f64 {
            *i = 0;
            1
        } else {
            limit.ceil() as i64
        }
    }
}

fn set_vec(vec: &mut Vec<Value>, idx: usize, value: Value) {
    match idx.cmp(&vec.len()) {
        std::cmp::Ordering::Equal => vec.push(value),
        std::cmp::Ordering::Less => vec[idx] = value,
        std::cmp::Ordering::Greater => {
            vec.resize(idx, Value::Nil);
            vec.push(value);
        }
    }
}

fn exe_binop(
    v1: &Value,
    v2: &Value,
    arith_i: fn(i64, i64) -> i64,
    arith_f: fn(f64, f64) -> f64,
) -> Value {
    match (v1, v2) {
        (Value::Integer(i1), Value::Integer(i2)) => Value::Integer(arith_i(*i1, *i2)),
        (Value::Integer(i1), Value::Float(f2)) => Value::Float(arith_f(*i1 as f64, *f2)),
        (Value::Float(f1), Value::Integer(i2)) => Value::Float(arith_f(*f1, *i2 as f64)),
        (Value::Float(f1), Value::Float(f2)) => Value::Float(arith_f(*f1, *f2)),
        (_, _) => panic!("meta"),
    }
}

fn exe_binop_int(
    v1: &Value,
    v2: u8,
    arith_i: fn(i64, i64) -> i64,
    arith_f: fn(f64, f64) -> f64,
) -> Value {
    match v1 {
        Value::Integer(iv) => Value::Integer(arith_i(*iv, v2 as i64)),
        Value::Float(fv) => Value::Float(arith_f(*fv, v2 as f64)),
        _ => panic!("meta"),
    }
}

fn exe_binop_f(v1: &Value, v2: &Value, arith_f: fn(f64, f64) -> f64) -> Value {
    let (f1, f2) = match (v1, v2) {
        (Value::Integer(i1), Value::Integer(i2)) => (*i1 as f64, *i2 as f64),
        (Value::Integer(i1), Value::Float(f2)) => (*i1 as f64, *f2),
        (Value::Float(f1), Value::Integer(i2)) => (*f1, *i2 as f64),
        (Value::Float(f1), Value::Float(f2)) => (*f1, *f2),
        (_, _) => panic!("meta"),
    };
    Value::Float(arith_f(f1, f2))
}

fn exe_binop_int_f(v1: &Value, v2: u8, arith_f: fn(f64, f64) -> f64) -> Value {
    let f1 = match v1 {
        Value::Integer(iv) => *iv as f64,
        Value::Float(fv) => *fv,
        _ => panic!("meta"),
    };
    Value::Float(arith_f(f1, v2 as f64))
}

fn exe_binop_i(v1: &Value, v2: &Value, arith_i: fn(i64, i64) -> i64) -> Value {
    let (i1, i2) = match (v1, v2) {
        (Value::Integer(i1), Value::Integer(i2)) => (*i1, *i2),
        (Value::Integer(i1), Value::Float(f2)) => (*i1, ftoi(*f2).unwrap()),
        (Value::Float(f1), Value::Integer(i2)) => (ftoi(*f1).unwrap(), *i2),
        (Value::Float(f1), Value::Float(f2)) => (ftoi(*f1).unwrap(), ftoi(*f2).unwrap()),
        _ => panic!("meta"),
    };
    Value::Integer(arith_i(i1, i2))
}

fn exe_binop_int_i(v1: &Value, v2: u8, arith_i: fn(i64, i64) -> i64) -> Value {
    let i1 = match v1 {
        Value::Integer(i1) => *i1,
        Value::Float(fv) => ftoi(*fv).unwrap(),
        _ => panic!("meta"),
    };
    Value::Integer(arith_i(i1, v2 as i64))
}

fn exe_concat(v1: &Value, v2: &Value) -> Value {
    let mut numbuf1: Vec<u8> = Vec::new();
    let v1 = match v1 {
        Value::Integer(i) => {
            write!(&mut numbuf1, "{}", i).unwrap();
            numbuf1.as_slice()
        }
        Value::Float(f) => {
            write!(&mut numbuf1, "{}", f).unwrap();
            numbuf1.as_slice()
        }
        _ => v1.into(),
    };

    let mut numbuf2: Vec<u8> = Vec::new();
    let v2 = match v2 {
        Value::Integer(i) => {
            write!(&mut numbuf2, "{}", i).unwrap();
            numbuf2.as_slice()
        }
        Value::Float(f) => {
            write!(&mut numbuf2, "{}", f).unwrap();
            numbuf2.as_slice()
        }
        _ => v2.into(),
    };

    [v1, v2].concat().into()
}
