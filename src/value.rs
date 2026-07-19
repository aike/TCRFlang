//! 実行時値。用途型のタグは静的検査で保証されるため実行時には保持しない。

use crate::types::TypeId;
use rust_decimal::Decimal;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Decimal(Decimal),
    Text(Rc<String>),
    Char(char),
    Bool(bool),
    Void,
    List(Rc<Vec<Value>>),
    Record(Rc<RecordValue>),
    Adt {
        type_id: TypeId,
        ctor: usize,
        payload: Option<Rc<Value>>,
    },
}

#[derive(Debug)]
pub struct RecordValue {
    pub type_id: TypeId,
    /// TypeDef のフィールド宣言順
    pub fields: Vec<Value>,
}

impl Value {
    pub fn text(s: impl Into<String>) -> Value {
        Value::Text(Rc::new(s.into()))
    }

    /// `==` の実行時実装 (型検査済みの同型スカラー/用途型内部値にのみ使う)。
    pub fn equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Decimal(a), Value::Decimal(b)) => a == b,
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Void, Value::Void) => true,
            _ => false,
        }
    }
}
