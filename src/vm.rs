use std::{collections::HashMap, io::Read};

use crate::{
    bytecode::ByteCode,
    parse::ParseProto,
    value::{Table, Value},
};

pub struct ExeState {
    globals: HashMap<String, Value>,
    stack: Vec<Value>,
    func_index: usize,
}

fn lib_print(state: &mut ExeState) -> i32 {
    println!("{:?}", state.stack[state.func_index + 1]);
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
        for code in proto.byte_codes.iter() {
            match *code {
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
                    let r = exe_binop(&self.stack[a as usize], &self.stack[b as usize], |x, y| x + y, |x, y| x + y);
                    self.set_stack(dst, r);
                }
                ByteCode::AddInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x + y, |x, y| x + y);
                    self.set_stack(dst, r);
                }
                ByteCode::AddConst(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &proto.constants[b as usize], |x, y| x + y, |x, y| x + y);
                    self.set_stack(dst, r);
                }
                ByteCode::Sub(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &self.stack[b as usize], |x, y| x - y, |x, y| x - y);
                    self.set_stack(dst, r);
                }
                ByteCode::SubInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x - y, |x, y| x - y);
                    self.set_stack(dst, r);
                }
                ByteCode::SubConst(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &proto.constants[b as usize], |x, y| x - y, |x, y| x - y);
                    self.set_stack(dst, r);
                }
                ByteCode::Mul(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &self.stack[b as usize], |x, y| x * y, |x, y| x * y);
                    self.set_stack(dst, r);
                }
                ByteCode::MulInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x * y, |x, y| x * y);
                    self.set_stack(dst, r);
                }
                ByteCode::MulConst(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &proto.constants[b as usize], |x, y| x * y, |x, y| x * y);
                    self.set_stack(dst, r);
                }
                ByteCode::Mod(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &self.stack[b as usize], |x, y| x % y, |x, y| x % y);
                    self.set_stack(dst, r);
                }
                ByteCode::ModInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x % y, |x, y| x % y);
                    self.set_stack(dst, r);
                }
                ByteCode::ModConst(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &proto.constants[b as usize], |x, y| x % y, |x, y| x % y);
                    self.set_stack(dst, r);
                }
                ByteCode::Idiv(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &self.stack[b as usize], |x, y| x / y, |x, y| x / y);
                    self.set_stack(dst, r);
                }
                ByteCode::IdivInt(dst, a, i) => {
                    let r = exe_binop_int(&self.stack[a as usize], i, |x, y| x / y, |x, y| x / y);
                    self.set_stack(dst, r);
                }
                ByteCode::IdivConst(dst, a, b) => {
                    let r = exe_binop(&self.stack[a as usize], &proto.constants[b as usize], |x, y| x / y, |x, y| x / y);
                    self.set_stack(dst, r);
                }
                _ => panic!("unimplemented bytecode: {code:?}"),
            }
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

fn exe_binop(v1: &Value, v2: &Value, arith_i: fn(i64, i64) -> i64, arith_f: fn(f64, f64) -> f64) -> Value {
    match (v1, v2) {
        (Value::Integer(i1), Value::Integer(i2)) => Value::Integer(arith_i(*i1, *i2)),
        (Value::Integer(i1), Value::Float(f2)) => Value::Float(arith_f(*i1 as f64, *f2)),
        (Value::Float(f1), Value::Integer(i2)) => Value::Float(arith_f(*f1, *i2 as f64)),
        (Value::Float(f1), Value::Float(f2)) => Value::Float(arith_f(*f1, *f2)),
        (_, _) => panic!("meta"),
    }
}

fn exe_binop_int(v1: &Value, v2: u8, arith_i: fn(i64, i64) -> i64, arith_f: fn(f64, f64) -> f64) -> Value {
    match v1 {
        Value::Integer(iv) => Value::Integer(arith_i(*iv, v2 as i64)),
        Value::Float(fv) => Value::Float(arith_f(*fv, v2 as f64)),
        _ => panic!("meta"),
    }
}
