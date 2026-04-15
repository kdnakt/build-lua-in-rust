use std::fmt::Debug;
use std::rc::Rc;

use crate::vm::ExeState;

const SHORT_STR_MAX_LEN: usize = 14; // sizeof(Value) - 1 (tag) - 1 (len)
const MID_STR_MAX_LEN: usize = 48 - 1;

#[derive(Clone)]
pub enum Value {
    Nil,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    ShortStr(u8, [u8; SHORT_STR_MAX_LEN]),
    MidStr(Rc<(u8, [u8; MID_STR_MAX_LEN])>),
    String(Rc<String>),
    Function(fn(&mut ExeState) -> i32),
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl:?}"),
            Self::String(s) => write!(f, "{s}"),
            Self::Function(_) => write!(f, "function"),
            // TODO
            _ => write!(f, "string"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Nil, Self::Nil) => true,
            (Self::Boolean(b1), Self::Boolean(b2)) => *b1 == *b2,
            (Self::Integer(i1), Self::Integer(i2)) => *i1 == *i2,
            (Self::Float(f1), Self::Float(f2)) => *f1 == *f2,
            (Self::String(s1), Self::String(s2)) => *s1 == *s2,
            (Self::Function(f1), Self::Function(f2)) => std::ptr::eq(f1, f2),
            _ => false,
        }
    }
}
