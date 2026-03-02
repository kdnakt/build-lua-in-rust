use std::fs::File;

use crate::{bytecode::ByteCode, value::Value};

#[derive(Debug)]
pub struct ParseProto {
    pub constants: Vec<Value>,
    pub byte_codes: Vec<ByteCode>,
}

pub fn load(input: File) -> ParseProto {
    todo!()
}
