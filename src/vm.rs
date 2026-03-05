use std::collections::HashMap;

use crate::{parse::ParseProto, value::Value};


pub struct ExeState {
    globals: HashMap<String, Value>,
    stack: Vec::<Value>,
}

impl ExeState {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        globals.insert("print".to_string(), Value::Nil);
        Self {
            globals,
            stack: Vec::new(),
        }
    }

    pub fn execute(&mut self, proto: &ParseProto) {
        todo!()
    }
}
