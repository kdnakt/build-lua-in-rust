use std::fmt::Debug;

#[derive(Clone)]
pub enum Value {
    Nil,
    String(String),
    // TODO: add Function
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nil => write!(f, "Nil"),
            Self::String(s) => write!(f, "{s}"),
        }
    }
}
