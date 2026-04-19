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
    LongStr(Rc<String>),
    Function(fn(&mut ExeState) -> i32),
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl:?}"),
            Self::ShortStr(len, arr) => {
                let s = std::str::from_utf8(&arr[..*len as usize]).unwrap_or("<invalid utf-8>");
                write!(f, "{s}")
            }
            Self::MidStr(rc) => {
                let (len, arr) = &**rc;
                let s = std::str::from_utf8(&arr[..*len as usize]).unwrap_or("<invalid utf-8>");
                write!(f, "{s}")
            }
            Self::LongStr(s) => write!(f, "{s}"),
            Self::Function(_) => write!(f, "function"),
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
            (Self::ShortStr(len1, arr1), Self::ShortStr(len2, arr2)) => {
                if len1 != len2 {
                    return false;
                }
                arr1[..*len1 as usize] == arr2[..*len2 as usize]
            }
            (Self::MidStr(s1), Self::MidStr(s2)) => {
                if s1.0 != s2.0 {
                    return false;
                }
                s1.1[..s1.0 as usize] == s2.1[..s2.0 as usize]
            }
            (Self::LongStr(s1), Self::LongStr(s2)) => s1 == s2,
            (Self::Function(f1), Self::Function(f2)) => std::ptr::eq(f1, f2),
            _ => false,
        }
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        let len = s.len();
        if len <= SHORT_STR_MAX_LEN {
            // 0-14
            let mut arr = [0; SHORT_STR_MAX_LEN];
            arr[..len].copy_from_slice(s.as_bytes());
            Value::ShortStr(len as u8, arr)
        } else if len <= MID_STR_MAX_LEN {
            // 15-47
            let mut arr = [0; MID_STR_MAX_LEN];
            arr[..len].copy_from_slice(s.as_bytes());
            Value::MidStr(Rc::new((len as u8, arr)))
        } else {
            // 48-
            Value::LongStr(Rc::new(s))
        }
    }
}

impl<'a> From<&'a Value> for &'a str {
    fn from(value: &'a Value) -> Self {
        match value {
            Value::ShortStr(len, arr) => std::str::from_utf8(&arr[..*len as usize]).unwrap(),
            Value::MidStr(rc) => std::str::from_utf8(&rc.1[..rc.0 as usize]).unwrap(),
            Value::LongStr(s) => s,
            _ => panic!("cannot convert to str: {value:?}"),
        }
    }
}

impl From<&Value> for String {
    fn from(value: &Value) -> Self {
        match value {
            Value::ShortStr(len, arr) => String::from_utf8_lossy(&arr[..*len as usize]).to_string(),
            Value::MidStr(rc) => String::from_utf8_lossy(&rc.1[..rc.0 as usize]).to_string(),
            Value::LongStr(s) => s.as_ref().clone(),
            _ => panic!("cannot convert to String: {value:?}"),
        }
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}
