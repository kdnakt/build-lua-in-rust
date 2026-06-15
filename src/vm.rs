use std::{
    collections::HashMap,
    io::{Read, Write},
};

use crate::{
    bytecode::ByteCode,
    parse::ParseProto,
    utils::ftoi,
    value::{Table, Value},
};

pub struct ExeState {
    globals: HashMap<String, Value>,
    stack: Vec<Value>,
    func_index: usize,
}

fn lib_print(state: &mut ExeState) -> i32 {
    println!("{}", state.stack[state.func_index + 1]);
    0
}

impl ExeState {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        globals.insert("print".to_string(), Value::Function(lib_print));
        Self {
            globals,
            stack: Vec::new(),
            func_index: 0,
        }
    }

    pub fn execute<R: Read>(&mut self, proto: &ParseProto<R>) {
        let mut pc = 0; // bytecode index
        while pc < proto.byte_codes.len() {
            match proto.byte_codes[pc] {
                ByteCode::GetGlobal(dst, name) => {
                    let name: &str = (&proto.constants[name as usize]).into();
                    let v = self.globals.get(name).unwrap_or(&Value::Nil).clone();
                    self.set_stack(dst, v);
                }
                ByteCode::SetGlobal(name, src) => {
                    let name = &proto.constants[name as usize];
                    let v = self.stack[src as usize].clone();
                    self.globals.insert(name.into(), v);
                }
                ByteCode::SetGlobalConst(name, src) => {
                    let name = &proto.constants[name as usize];
                    let v = proto.constants[src as usize].clone();
                    self.globals.insert(name.into(), v);
                }
                ByteCode::SetGlobalGlobal(name, src) => {
                    let name = &proto.constants[name as usize];
                    let src: &str = (&proto.constants[src as usize]).into();
                    let v = self.globals.get(src).unwrap_or(&Value::Nil).clone();
                    self.globals.insert(name.into(), v);
                }
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
                    let v = self.stack[i as usize].clone();
                    self.set_stack(dst, v);
                }
                ByteCode::Call(func, _) => {
                    self.func_index = func as usize;
                    let func = &self.stack[self.func_index];
                    if let Value::Function(f) = func {
                        f(self);
                    } else {
                        panic!("invalid function: {func:?}");
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
                    let key = self.stack[key as usize].clone();
                    let value = self.stack[value as usize].clone();
                    if let Value::Table(t) = &self.stack[table as usize] {
                        t.borrow_mut().map.insert(key, value);
                    } else {
                        panic!("not table");
                    }
                }
                ByteCode::SetTableConst(t, k, v) => {
                    let key = self.stack[k as usize].clone();
                    let value = proto.constants[v as usize].clone();
                    self.set_table(t, key, value);
                }
                ByteCode::SetInt(t, i, v) => {
                    let value = self.stack[v as usize].clone();
                    self.set_table_int(t, i as i64, value);
                }
                ByteCode::SetIntConst(t, i, v) => {
                    let value = proto.constants[v as usize].clone();
                    self.set_table_int(t, i as i64, value);
                }
                ByteCode::SetField(table, key, value) => {
                    let key = proto.constants[key as usize].clone();
                    let value = self.stack[value as usize].clone();
                    if let Value::Table(t) = &self.stack[table as usize] {
                        t.borrow_mut().map.insert(key, value);
                    } else {
                        panic!("not table");
                    }
                }
                ByteCode::SetFieldConst(t, k, v) => {
                    let key = proto.constants[k as usize].clone();
                    let value = proto.constants[v as usize].clone();
                    self.set_table(t, key, value);
                }
                ByteCode::SetList(table, n) => {
                    let ivalue = table as usize + 1;
                    if let Value::Table(t) = &self.stack[table as usize].clone() {
                        let values = self.stack.drain(ivalue..ivalue + n as usize);
                        t.borrow_mut().array.extend(values);
                    } else {
                        panic!("not table");
                    }
                }
                ByteCode::GetInt(dst, t, k) => {
                    let value = self.get_table_int(t, k as i64);
                    self.set_stack(dst, value);
                }
                ByteCode::GetField(dst, t, k) => {
                    let key = &proto.constants[k as usize];
                    let value = self.get_table(t, key);
                    self.set_stack(dst, value);
                }
                ByteCode::GetTable(dst, t, k) => {
                    let key = &self.stack[k as usize];
                    let value = self.get_table(t, key);
                    self.set_stack(dst, value);
                }
                ByteCode::Neg(dst, src) => {
                    let value = match &self.stack[src as usize] {
                        Value::Integer(i) => Value::Integer(-i),
                        Value::Float(f) => Value::Float(-f),
                        _ => panic!("invalid -"),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::Not(dst, src) => {
                    let value = match &self.stack[src as usize] {
                        Value::Nil => Value::Boolean(true),
                        Value::Boolean(b) => Value::Boolean(!b),
                        _ => Value::Boolean(false),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::Len(dst, src) => {
                    let value = match &self.stack[src as usize] {
                        Value::ShortStr(len, _) => Value::Integer(*len as i64),
                        Value::MidStr(rc) => Value::Integer(rc.0 as i64),
                        Value::LongStr(s) => Value::Integer(s.len() as i64),
                        Value::Table(t) => Value::Integer(t.borrow().array.len() as i64),
                        _ => panic!("invalid length operator"),
                    };
                    self.set_stack(dst, value);
                }
                ByteCode::BitNot(dst, src) => {
                    let value = match &self.stack[src as usize] {
                        Value::Integer(i) => Value::Integer(!i),
                        _ => panic!("invalid bitwise not operator"),
                    };
                    self.set_stack(dst, value);
                }
                // binops
                ByteCode::Add(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &self.stack[b as usize],
                        |x, y| x + y,
                        |x, y| x + y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::AddInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x + y, |x, y| x + y);
                    self.set_stack(dst, r);
                }
                ByteCode::AddConst(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x + y,
                        |x, y| x + y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Sub(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &self.stack[b as usize],
                        |x, y| x - y,
                        |x, y| x - y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::SubInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x - y, |x, y| x - y);
                    self.set_stack(dst, r);
                }
                ByteCode::SubConst(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x - y,
                        |x, y| x - y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Mul(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &self.stack[b as usize],
                        |x, y| x * y,
                        |x, y| x * y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::MulInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x * y, |x, y| x * y);
                    self.set_stack(dst, r);
                }
                ByteCode::MulConst(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x * y,
                        |x, y| x * y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Mod(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &self.stack[b as usize],
                        |x, y| x % y,
                        |x, y| x % y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::ModInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x % y, |x, y| x % y);
                    self.set_stack(dst, r);
                }
                ByteCode::ModConst(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x % y,
                        |x, y| x % y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Idiv(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &self.stack[b as usize],
                        |x, y| x / y,
                        |x, y| x / y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::IdivInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x / y, |x, y| x / y);
                    self.set_stack(dst, r);
                }
                ByteCode::IdivConst(dst, a, b) => {
                    let r = exe_binop(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x / y,
                        |x, y| x / y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Div(dst, a, b) => {
                    let r =
                        exe_binop_f(&self.stack[a as usize], &self.stack[b as usize], |x, y| {
                            x / y
                        });
                    self.set_stack(dst, r);
                }
                ByteCode::DivInt(dst, a, i) => {
                    let r = exe_binop_int_f(&self.stack[a as usize], i, |x, y| x / y);
                    self.set_stack(dst, r);
                }
                ByteCode::DivConst(dst, a, b) => {
                    let r = exe_binop_f(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x / y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Pow(dst, a, b) => {
                    let r =
                        exe_binop_f(&self.stack[a as usize], &self.stack[b as usize], |x, y| {
                            x.powf(y)
                        });
                    self.set_stack(dst, r);
                }
                ByteCode::PowInt(dst, a, i) => {
                    let r = exe_binop_int_f(&self.stack[a as usize], i, |x, y| x.powf(y));
                    self.set_stack(dst, r);
                }
                ByteCode::PowConst(dst, a, b) => {
                    let r = exe_binop_f(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x.powf(y),
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::BitAnd(dst, a, b) => {
                    let r =
                        exe_binop_i(&self.stack[a as usize], &self.stack[b as usize], |x, y| {
                            x & y
                        });
                    self.set_stack(dst, r);
                }
                ByteCode::BitAndInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.stack[a as usize], i, |x, y| x & y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitAndConst(dst, a, b) => {
                    let r = exe_binop_i(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x & y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::BitOr(dst, a, b) => {
                    let r =
                        exe_binop_i(&self.stack[a as usize], &self.stack[b as usize], |x, y| {
                            x | y
                        });
                    self.set_stack(dst, r);
                }
                ByteCode::BitOrInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.stack[a as usize], i, |x, y| x | y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitOrConst(dst, a, b) => {
                    let r = exe_binop_i(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x | y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::BitXor(dst, a, b) => {
                    let r =
                        exe_binop_i(&self.stack[a as usize], &self.stack[b as usize], |x, y| {
                            x ^ y
                        });
                    self.set_stack(dst, r);
                }
                ByteCode::BitXorInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.stack[a as usize], i, |x, y| x ^ y);
                    self.set_stack(dst, r);
                }
                ByteCode::BitXorConst(dst, a, b) => {
                    let r = exe_binop_i(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x ^ y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftL(dst, a, b) => {
                    let r =
                        exe_binop_i(&self.stack[a as usize], &self.stack[b as usize], |x, y| {
                            x << y
                        });
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftLInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.stack[a as usize], i, |x, y| x << y);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftLConst(dst, a, b) => {
                    let r = exe_binop_i(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x << y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftR(dst, a, b) => {
                    let r =
                        exe_binop_i(&self.stack[a as usize], &self.stack[b as usize], |x, y| {
                            x >> y
                        });
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftRInt(dst, a, i) => {
                    let r = exe_binop_int_i(&self.stack[a as usize], i, |x, y| x >> y);
                    self.set_stack(dst, r);
                }
                ByteCode::ShiftRConst(dst, a, b) => {
                    let r = exe_binop_i(
                        &self.stack[a as usize],
                        &proto.constants[b as usize],
                        |x, y| x >> y,
                    );
                    self.set_stack(dst, r);
                }
                ByteCode::Concat(dst, a, b) => {
                    let r = exe_concat(&self.stack[a as usize], &self.stack[b as usize]);
                    self.set_stack(dst, r);
                }
                ByteCode::ConcatInt(dst, a, i) => {
                    let r = exe_concat(&self.stack[a as usize], &Value::Integer(i as i64));
                    self.set_stack(dst, r);
                }
                ByteCode::ConcatConst(dst, a, b) => {
                    let r = exe_concat(&self.stack[a as usize], &proto.constants[b as usize]);
                    self.set_stack(dst, r);
                }
                ByteCode::Test(icond, jmp) => {
                    let cond = &self.stack[icond as usize];
                    if matches!(cond, Value::Nil | Value::Boolean(false)) {
                        pc = (pc as isize + jmp as isize) as usize;
                    }
                }
                ByteCode::Jump(jmp) => {
                    pc = (pc as isize + jmp as isize) as usize;
                }
                ByteCode::ForPrepare(dst, jmp) => {
                    if let (&Value::Integer(mut i), &Value::Integer(step)) =
                            (&self.stack[dst as usize], &self.stack[dst as usize + 2]) {
                        if step == 0 {
                            panic!("0 step in numeric for");
                        }
                        let limit = match self.stack[dst as usize + 1] {
                            Value::Integer(limit) => limit,
                            Value::Float(limit) => {
                                let limit = for_int_limit(limit, step>0, &mut i);
                                self.set_stack(dst+1, Value::Integer(limit));
                                limit
                            }
                            _ => panic!("invalid limit in numeric for"),
                        };
                        if !for_check(i, limit, step>0) {
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
                ByteCode::ForLoop(_, _) => todo!(),
            }

            // next bytecode
            pc += 1;
        }
    }

    fn set_stack(&mut self, idx: u8, val: Value) {
        let dst = idx as usize;
        match dst.cmp(&self.stack.len()) {
            std::cmp::Ordering::Equal => self.stack.push(val),
            std::cmp::Ordering::Less => self.stack[dst] = val,
            std::cmp::Ordering::Greater => panic!("fail in set_stack"),
        }
    }

    fn fill_stack(&mut self, begin: usize, num: usize) {
        let end = begin + num;
        let len = self.stack.len();
        if begin < len {
            self.stack[begin..len].fill(Value::Nil);
        }
        if end > len {
            self.stack.resize(end, Value::Nil);
        }
    }

    fn get_table(&self, table: u8, key: &Value) -> Value {
        match key {
            Value::Integer(i) => self.get_table_int(table, *i),
            _ => self.do_get_table(table, key),
        }
    }

    fn do_get_table(&self, table: u8, key: &Value) -> Value {
        if let Value::Table(t) = &self.stack[table as usize] {
            let t = t.borrow();
            t.map.get(key).unwrap_or(&Value::Nil).clone()
        } else {
            panic!("set invalid table");
        }
    }

    fn set_table(&mut self, table: u8, key: Value, value: Value) {
        match &key {
            Value::Integer(i) => self.set_table_int(table, *i, value),
            _ => self.do_set_table(table, key, value),
        }
    }

    fn do_set_table(&mut self, table: u8, key: Value, value: Value) {
        if let Value::Table(t) = &self.stack[table as usize] {
            t.borrow_mut().map.insert(key, value);
        } else {
            panic!("invalid table");
        }
    }

    fn get_table_int(&self, table: u8, key: i64) -> Value {
        if let Value::Table(t) = &self.stack[table as usize] {
            let t = t.borrow();
            t.array
                .get(key as usize - 1)
                .unwrap_or_else(|| t.map.get(&Value::Integer(key)).unwrap_or(&Value::Nil))
                .clone()
        } else {
            panic!("set invalid table");
        }
    }

    fn set_table_int(&mut self, table: u8, key: i64, value: Value) {
        if let Value::Table(t) = &self.stack[table as usize] {
            let mut t = t.borrow_mut();
            if key > 0 && (key < 4 || key < t.array.capacity() as i64 * 2) {
                set_vec(&mut t.array, key as usize - 1, value);
            } else {
                t.map.insert(Value::Integer(key), value);
            }
        } else {
            panic!("invalid table");
        }
    }

    fn make_float(&mut self, dst: u8) -> f64 {
        todo!()
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
