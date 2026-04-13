use std::collections::HashMap;

use crate::{bytecode::ByteCode, parse::ParseProto, value::Value};

pub struct ExeState {
    globals: HashMap<String, Value>,
    stack: Vec<Value>,
    func_index: usize,
}

fn lib_print(state: &mut ExeState) -> i32 {
    println!("{:?}", state.stack[1]);
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

    pub fn execute(&mut self, proto: &ParseProto) {
        for code in proto.byte_codes.iter() {
            match *code {
                ByteCode::GetGlobal(dst, name) => {
                    let name = &proto.constants[name as usize];
                    if let Value::String(key) = name {
                        let v = self.globals.get(key).unwrap_or(&Value::Nil).clone();
                        self.set_stack(dst, v);
                    } else {
                        panic!("invalid global key: {name:?}");
                    }
                }
                ByteCode::SetGlobal(name, src) => {
                    let name = proto.constants[name as usize].clone();
                    if let Value::String(key) = name {
                        let v = self.stack[src as usize].clone();
                        self.globals.insert(key, v);
                    } else {
                        panic!("invalid global key: {name:?}");
                    }
                }
                ByteCode::SetGlobalConst(name, src) => {
                    let name = proto.constants[name as usize].clone();
                    if let Value::String(key) = name {
                        let v = proto.constants[src as usize].clone();
                        self.globals.insert(key, v);
                    } else {
                        panic!("invalid global key: {name:?}");
                    }
                }
                ByteCode::SetGlobalGlobal(name, src) => {
                    let name = proto.constants[name as usize].clone();
                    if let Value::String(key) = name {
                        let src = &proto.constants[src as usize];
                        if let Value::String(src) = src {
                            let v = self.globals.get(src).unwrap_or(&Value::Nil).clone();
                            self.globals.insert(key, v);
                        } else {
                            panic!("invalid global key: {src:?}");
                        }
                    } else {
                        panic!("invalid global key: {name:?}");
                    }
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
                    self.set_stack(dst, Value::Integer(i as i64));
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
