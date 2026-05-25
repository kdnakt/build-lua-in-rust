use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::vm::ExeState;
use crate::utils::ftoi;

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
    LongStr(Rc<Vec<u8>>),
    Table(Rc<RefCell<Table>>),
    Function(fn(&mut ExeState) -> i32),
}

pub struct Table {
    pub array: Vec<Value>,
    pub map: HashMap<Value, Value>,
}

impl Table {
    pub fn new(narray: usize, nmap: usize) -> Self {
        Self {
            array: Vec::with_capacity(narray),
            map: HashMap::with_capacity(nmap),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl:?}"),
            Self::ShortStr(len, arr) => {
                let s = String::from_utf8_lossy(&arr[..*len as usize]);
                write!(f, "{s}")
            }
            Self::MidStr(rc) => write!(f, "{}", String::from_utf8_lossy(&rc.1[..rc.0 as usize])),
            Self::LongStr(s) => write!(f, "{}", String::from_utf8_lossy(&s)),
            Self::Table(t) => write!(f, "table: {:?}", Rc::as_ptr(t)),
            Self::Function(_) => write!(f, "function"),
        }
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl:?}"),
            Self::ShortStr(len, arr) => write!(
                f,
                "SS: '{}'",
                String::from_utf8_lossy(&arr[..*len as usize])
            ),
            Self::MidStr(rc) => write!(
                f,
                "MS: '{}'",
                String::from_utf8_lossy(&rc.1[..rc.0 as usize])
            ),
            Self::LongStr(s) => write!(f, "LS: '{}'", String::from_utf8_lossy(&s)),
            Self::Table(t) => {
                let t = t.borrow();
                write!(f, "table:{}:{}", t.array.len(), t.map.len())
            }
            Self::Function(_) => write!(f, "function"),
        }
    }
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Nil => (),
            Self::Boolean(b) => b.hash(state),
            Self::Integer(i) => i.hash(state),
            Self::Float(f) =>
                if let Some(i) = ftoi(*f) {
                    i.hash(state)
                } else {
                    unsafe {
                        std::mem::transmute::<f64, i64>(*f).hash(state)
                    }
                }
            Self::ShortStr(len, arr) => arr[..*len as usize].hash(state),
            Self::MidStr(rc) => rc.1[..rc.0 as usize].hash(state),
            Self::LongStr(s) => s.hash(state),
            Self::Table(t) => Rc::as_ptr(t).hash(state),
            Self::Function(f) => (*f as *const usize).hash(state),
        }
    }
}

impl Value {
    pub fn same(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other) && self == other
    }
}

impl Eq for Value {}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Nil, Self::Nil) => true,
            (Self::Boolean(b1), Self::Boolean(b2)) => *b1 == *b2,
            (Self::Integer(i1), Self::Integer(i2)) => *i1 == *i2,
            (Self::Float(f1), Self::Float(f2)) => *f1 == *f2,
            (Self::Integer(i), Self::Float(f)) | (Self::Float(f), Self::Integer(i)) => *i as f64 == *f && *i == *f as i64,
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

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        vec_to_short_mid_str(&v).unwrap_or(Value::LongStr(Rc::new(v)))
    }
}

fn vec_to_short_mid_str(v: &[u8]) -> Option<Value> {
    let len = v.len();
    if len <= SHORT_STR_MAX_LEN {
        let mut arr = [0; SHORT_STR_MAX_LEN];
        arr[..len].copy_from_slice(&v);
        Some(Value::ShortStr(len as u8, arr))
    } else if len <= MID_STR_MAX_LEN {
        let mut arr = [0; MID_STR_MAX_LEN];
        arr[..len].copy_from_slice(&v);
        Some(Value::MidStr(Rc::new((len as u8, arr))))
    } else {
        None
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Nil
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Boolean(b)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        s.into_bytes().into() // Vec<u8>
    }
}

impl<'a> From<&'a Value> for &'a [u8] {
    fn from(value: &'a Value) -> Self {
        match value {
            Value::ShortStr(len, arr) => &arr[..*len as usize],
            Value::MidStr(rc) => &rc.1[..rc.0 as usize],
            Value::LongStr(s) => s,
            _ => panic!("cannot convert to &[u8]: {value:?}"),
        }
    }
}

impl<'a> From<&'a Value> for &'a str {
    fn from(value: &'a Value) -> Self {
        std::str::from_utf8(value.into()).unwrap()
    }
}

impl From<&Value> for String {
    fn from(value: &Value) -> Self {
        String::from_utf8_lossy(value.into()).to_string()
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
