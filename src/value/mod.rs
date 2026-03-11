use std::fmt::Debug;

use crate::vm::ExeState;

#[derive(Clone)]
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Function(fn(&mut ExeState) -> i32),
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nil => write!(f, "Nil"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl:?}"),
            Self::String(s) => write!(f, "{s}"),
            Self::Function(_) => write!(f, "function"),
        }
    }
}
