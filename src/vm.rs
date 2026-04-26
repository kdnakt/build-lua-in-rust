use std::{collections::HashMap, io::Read};

use crate::{bytecode::ByteCode, parse::ParseProto, value::{Table, Value}};

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
                ByteCode::LoadNil(dst) => {
                    self.set_stack(dst, Value::Nil);
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
                    self.set_stack(dst, Value::Table(std::rc::Rc::new(std::cell::RefCell::new(t))));
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
                ByteCode::SetField(table, key, value) => {
                    let key = proto.constants[key as usize].clone();
                    let value = self.stack[value as usize].clone();
                    if let Value::Table(t) = &self.stack[table as usize] {
                        t.borrow_mut().map.insert(key, value);
                    } else {
                        panic!("not table");
                    }
                }
                ByteCode::SetList(table, n) => {
                    let ivalue = table as usize + 1;
                    if let Value::Table(t) = &self.stack[table as usize].clone() {
                        let values = self.stack.drain(ivalue .. ivalue + n as usize);
                        t.borrow_mut().array.extend(values);
                    } else {
                        panic!("not table");
                    }
                }
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
}
