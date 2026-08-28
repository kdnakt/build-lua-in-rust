use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use crate::parse::FuncProto;
use crate::utils::{ftoi, set_vec};
use crate::vm::ExeState;

const SHORT_STR_MAX_LEN: usize = 14; // sizeof(Value) - 1 (tag) - 1 (len)
const MID_STR_MAX_LEN: usize = 48 - 1;

#[derive(Debug, PartialEq)]
pub enum Upvalue {
    Open(usize),
    Closed(Value),
}

impl Upvalue {
    pub fn get<'a>(&'a self, stack: &'a [Value]) -> &'a Value {
        match self {
            Upvalue::Open(i) => &stack[*i],
            Upvalue::Closed(v) => v,
        }
    }
    pub fn set(&mut self, stack: &mut Vec<Value>, value: Value) {
        match self {
            Upvalue::Open(i) => stack[*i] = value,
            Upvalue::Closed(v) => *v = value,
        }
    }
}

pub struct LuaClosure {
    pub proto: Rc<FuncProto>,
    pub upvalues: Vec<Rc<RefCell<Upvalue>>>,
}

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
    LuaFunction(Rc<FuncProto>),
    RustFunction(fn(&mut ExeState) -> i32),
    RustClosure(Rc<RefCell<Box<dyn FnMut(&mut ExeState) -> i32>>>),
    LuaClosure(Rc<LuaClosure>),
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
    pub fn new_index(&mut self, k: Value, v: Value) {
        match k {
            Value::Integer(i) => self.new_index_array(i, v),
            _ => {
                self.map.insert(k, v);
            }
        }
    }
    pub fn new_index_array(&mut self, i: i64, v: Value) {
        if i > 0 && (i < 4 || i < self.array.capacity() as i64 * 2) {
            set_vec(&mut self.array, i as usize - 1, v);
        } else {
            self.map.insert(Value::Integer(i), v);
        }
    }

    pub fn index(&self, key: &Value) -> &Value {
        match key {
            &Value::Integer(i) => self.index_array(i),
            _ => self.map.get(key).unwrap_or(&Value::Nil),
        }
    }

    pub fn index_array(&self, i: i64) -> &Value {
        self.array.get(i as usize - 1).unwrap_or_else(|| {
            self.map
                .get(&Value::Integer(i as i64))
                .unwrap_or(&Value::Nil)
        })
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
            Self::RustFunction(_) => write!(f, "function"),
            Self::RustClosure(_) => write!(f, "function"),
            Self::LuaFunction(l) => write!(f, "function: {:?}", Rc::as_ptr(l)),
            Self::LuaClosure(c) => write!(f, "function: {:?}", Rc::as_ptr(c)),
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
            Self::LuaFunction(_) => write!(f, "lua function"),
            Self::LuaClosure(_) => write!(f, "lua closure"),
            Self::RustFunction(_) => write!(f, "function"),
            Self::RustClosure(_) => write!(f, "function"),
        }
    }
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Nil => (),
            Self::Boolean(b) => b.hash(state),
            Self::Integer(i) => i.hash(state),
            Self::Float(f) => {
                if let Some(i) = ftoi(*f) {
                    i.hash(state)
                } else {
                    unsafe { std::mem::transmute::<f64, i64>(*f).hash(state) }
                }
            }
            Self::ShortStr(len, arr) => arr[..*len as usize].hash(state),
            Self::MidStr(rc) => rc.1[..rc.0 as usize].hash(state),
            Self::LongStr(s) => s.hash(state),
            Self::Table(t) => Rc::as_ptr(t).hash(state),
            Self::RustFunction(f) => (*f as *const usize).hash(state),
            Self::RustClosure(c) => Rc::as_ptr(c).hash(state),
            Self::LuaFunction(f) => Rc::as_ptr(f).hash(state),
            Self::LuaClosure(c) => Rc::as_ptr(c).hash(state),
        }
    }
}

impl Value {
    pub fn same(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other) && self == other
    }
    pub fn ty(&self) -> &'static str {
        match self {
            &Value::Nil => "nil",
            &Value::Boolean(_) => "boolean",
            &Value::Float(_) => "number",
            &Value::Integer(_) => "number",
            &Value::ShortStr(_, _) => "string",
            &Value::MidStr(_) => "string",
            &Value::LongStr(_) => "string",
            &Value::Table(_) => "table",
            &Value::RustFunction(_) => "function",
            &Value::RustClosure(_) => "function",
            &Value::LuaFunction(_) => "function",
            &Value::LuaClosure(_) => "function",
        }
    }
    pub fn new_index(&self, k: Value, v: Value) {
        match self {
            Value::Table(t) => t.borrow_mut().new_index(k, v),
            _ => todo!("meta __index"),
        }
    }
    pub fn index(&self, key: &Value) -> Value {
        match self {
            Value::Table(t) => t.borrow().index(key).clone(),
            _ => todo!("meta __index"),
        }
    }
    pub fn index_array(&self, i: i64) -> Value {
        match self {
            Value::Table(t) => t.borrow().index_array(i).clone(),
            _ => todo!("meta __index"),
        }
    }
    pub fn new_index_array(&self, i: i64, v: Value) {
        match self {
            Value::Table(t) => t.borrow_mut().new_index_array(i, v),
            _ => todo!("meta __index"),
        }
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
            (Self::Integer(i), Self::Float(f)) | (Self::Float(f), Self::Integer(i)) => {
                *i as f64 == *f && *i == *f as i64
            }
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
            (Self::RustFunction(f1), Self::RustFunction(f2)) => std::ptr::eq(f1, f2),
            (Self::LuaFunction(f1), Self::LuaFunction(f2)) => Rc::as_ptr(f1) == Rc::as_ptr(f2),
            _ => false,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Integer(i1), Self::Integer(i2)) => Some(i1.cmp(i2)),
            (Self::Float(f1), Self::Float(f2)) => f1.partial_cmp(f2),
            (Self::Integer(i), Self::Float(f)) => (*i as f64).partial_cmp(f),
            (Self::Float(f), Self::Integer(i)) => f.partial_cmp(&(*i as f64)),
            (Self::ShortStr(len1, arr1), Self::ShortStr(len2, arr2)) => {
                let s1 = &arr1[..*len1 as usize];
                let s2 = &arr2[..*len2 as usize];
                Some(s1.cmp(s2))
            }
            (Self::MidStr(s1), Self::MidStr(s2)) => {
                let s1 = &s1.1[..s1.0 as usize];
                let s2 = &s2.1[..s2.0 as usize];
                Some(s1.cmp(s2))
            }
            (Self::LongStr(s1), Self::LongStr(s2)) => Some(s1.cmp(s2)),
            (Self::ShortStr(len1, arr1), Self::MidStr(s2)) => {
                let s1 = &arr1[..*len1 as usize];
                let s2 = &s2.1[..s2.0 as usize];
                Some(s1.cmp(s2))
            }
            (Self::ShortStr(len1, arr1), Self::LongStr(s2)) => {
                let s1 = &arr1[..*len1 as usize];
                Some(s1.cmp(s2))
            }
            (Self::MidStr(s1), Self::ShortStr(len2, arr2)) => {
                let s1 = &s1.1[..s1.0 as usize];
                let s2 = &arr2[..*len2 as usize];
                Some(s1.cmp(s2))
            }
            (Self::MidStr(s1), Self::LongStr(s2)) => {
                let s1 = &s1.1[..s1.0 as usize];
                Some(s1.cmp(s2))
            }
            (Self::LongStr(s1), Self::ShortStr(len2, arr2)) => {
                let s2 = &arr2[..*len2 as usize];
                Some(s1.as_ref().as_slice().cmp(s2))
            }
            (Self::LongStr(s1), Self::MidStr(s2)) => {
                let s2 = &s2.1[..s2.0 as usize];
                Some(s1.as_ref().as_slice().cmp(s2))
            }
            _ => None,
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

impl From<&Value> for bool {
    fn from(v: &Value) -> Self {
        !matches!(v, Value::Nil | Value::Boolean(false))
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        s.as_bytes().into()
    }
}

impl From<&[u8]> for Value {
    fn from(v: &[u8]) -> Self {
        vec_to_short_mid_str(v).unwrap_or_else(|| Value::LongStr(Rc::new(v.to_vec())))
    }
}

impl From<&Value> for i64 {
    fn from(v: &Value) -> Self {
        match v {
            Value::Integer(i) => *i,
            Value::Float(f) => *f as i64,
            Value::ShortStr(_, _) => todo!("to number"),
            Value::MidStr(_) => todo!("to number"),
            Value::LongStr(_) => todo!("to number"),
            _ => panic!("invalid string value"),
        }
    }
}
