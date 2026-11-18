use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zs_parser::ast::*;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Key { Int(i64), Str(Rc<str>) }

#[derive(Clone, Default)]
pub struct ZArray {
    pub entries: Vec<(Key, Value)>,
    index: HashMap<Key, usize>,
    next: i64,
}

impl ZArray {
    pub fn new() -> Self { Self::default() }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn get(&self, k: &Key) -> Option<&Value> { self.index.get(k).map(|&i| &self.entries[i].1) }
    pub fn set(&mut self, k: Key, v: Value) {
        if let Some(&i) = self.index.get(&k) { self.entries[i].1 = v; return; }
        if let Key::Int(n) = k { if n >= self.next { self.next = n + 1; } }
        self.index.insert(k.clone(), self.entries.len());
        self.entries.push((k, v));
    }
    pub fn push(&mut self, v: Value) { let k = Key::Int(self.next); self.set(k, v); }
    pub fn slot(&mut self, k: Key) -> &mut Value {
        if !self.index.contains_key(&k) { self.set(k.clone(), Value::Null); }
        let i = self.index[&k];
        &mut self.entries[i].1
    }
    pub fn push_slot(&mut self) -> &mut Value {
        self.push(Value::Null);
        &mut self.entries.last_mut().unwrap().1
    }
    pub fn from_values(v: Vec<Value>) -> Self { let mut a = Self::new(); for x in v { a.push(x); } a }
}

pub struct Object { pub class: Name, pub props: HashMap<Name, Value>, pub order: Vec<Name> }

pub struct Closure {
    pub decl: Rc<FuncDecl>,
    pub captured: HashMap<Name, Value>,
    pub this: Option<Value>,
    pub class: Option<Name>,
    pub module: Option<Name>,
}

#[derive(Clone)]
pub enum Value {
    Null, Bool(bool), Int(i64), Float(f64), Str(Rc<str>),
    Array(Rc<ZArray>),
    Object(Rc<RefCell<Object>>),
    Func(Rc<Closure>),
    Class(Name),
    Module(Name),
}

impl Value {
    pub fn str(s: &str) -> Value { Value::Str(Rc::from(s)) }
    pub fn string(s: String) -> Value { Value::Str(Rc::from(s)) }
    pub fn array(v: Vec<Value>) -> Value { Value::Array(Rc::new(ZArray::from_values(v))) }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null", Value::Bool(_) => "bool", Value::Int(_) => "int",
            Value::Float(_) => "float", Value::Str(_) => "string", Value::Array(_) => "array",
            Value::Object(_) => "object", Value::Func(_) => "function",
            Value::Class(_) => "class", Value::Module(_) => "module",
        }
    }
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false, Value::Bool(b) => *b, Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0, Value::Str(s) => !s.is_empty() && &**s != "0",
            Value::Array(a) => a.len() > 0,
            _ => true,
        }
    }
}

pub fn fmt_float(f: f64) -> String {
    if f.is_nan() { "NaN".into() }
    else if f.is_infinite() { if f > 0.0 { "Infinity".into() } else { "-Infinity".into() } }
    else if f.fract() == 0.0 && f.abs() < 1e15 { format!("{:.1}", f) }
    else { format!("{}", f) }
}

/// Tekst używany przez `echo` i `.`
pub fn to_text(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => fmt_float(*f),
        Value::Str(s) => s.to_string(),
        _ => dump(v),
    }
}

/// Reprezentacja diagnostyczna (ciągi w cudzysłowach)
pub fn dump(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("{:?}", &**s),
        Value::Array(a) => {
            let seq = a.entries.iter().enumerate().all(|(i, (k, _))| *k == Key::Int(i as i64));
            let parts: Vec<String> = a.entries.iter().map(|(k, v)| {
                if seq { dump(v) } else {
                    let ks = match k { Key::Int(i) => i.to_string(), Key::Str(s) => format!("{:?}", &**s) };
                    format!("{} => {}", ks, dump(v)) }
            }).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(o) => format!("<object {}>", o.borrow().class),
        Value::Func(_) => "<function>".into(),
        Value::Class(n) => format!("<class {}>", n),
        Value::Module(n) => format!("<module {}>", n),
        other => to_text(other),
    }
}

pub fn to_key(v: &Value) -> Result<Key, String> {
    match v {
        Value::Int(i) => Ok(Key::Int(*i)),
        Value::Str(s) => Ok(match s.parse::<i64>() {
            Ok(n) if n.to_string() == **s => Key::Int(n),
            _ => Key::Str(s.clone()),
        }),
        Value::Bool(b) => Ok(Key::Int(*b as i64)),
        Value::Float(f) if f.fract() == 0.0 => Ok(Key::Int(*f as i64)),
        _ => Err(format!("niepoprawny klucz tablicy typu {}", v.type_name())),
    }
}

pub fn key_to_value(k: &Key) -> Value {
    match k { Key::Int(i) => Value::Int(*i), Key::Str(s) => Value::Str(s.clone()) }
}

pub fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.entries.iter().zip(y.entries.iter()).all(|((k1, v1), (k2, v2))| k1 == k2 && strict_eq(v1, v2))
        }
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        (Value::Func(x), Value::Func(y)) => Rc::ptr_eq(x, y),
        (Value::Class(x), Value::Class(y)) | (Value::Module(x), Value::Module(y)) => x == y,
        _ => false,
    }
}

/// `==` : liczby porównywane numerycznie, reszta jak `===` (bez magii PHP "abc" == 0)
pub fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => (*x as f64) == *y,
        _ => strict_eq(a, b),
    }
}

pub fn compare(a: &Value, b: &Value) -> Result<std::cmp::Ordering, String> {
    use std::cmp::Ordering::*;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(x.cmp(y)),
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            let (x, y) = (as_f64(a), as_f64(b));
            Ok(x.partial_cmp(&y).unwrap_or(Equal))
        }
        (Value::Str(x), Value::Str(y)) => Ok(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Ok(x.cmp(y)),
        _ => Err(format!("nie można porównać {} z {}", a.type_name(), b.type_name())),
    }
}

pub fn as_f64(v: &Value) -> f64 { match v { Value::Int(i) => *i as f64, Value::Float(f) => *f, _ => 0.0 } }

pub fn binop(op: BinOp, a: &Value, b: &Value) -> Result<Value, String> {
    use BinOp::*;
    use Value::{Float, Int};
    let bad = || format!("operacja {:?} niedostępna dla {} i {}", op, a.type_name(), b.type_name());
    Ok(match op {
        Concat => Value::string(format!("{}{}", to_text(a), to_text(b))),
        Add | Sub | Mul => match (a, b) {
            (Int(x), Int(y)) => {
                let r = match op { Add => x.checked_add(*y), Sub => x.checked_sub(*y), _ => x.checked_mul(*y) };
                Int(r.ok_or("przepełnienie liczby całkowitej")?)
            }
            (Int(_) | Float(_), Int(_) | Float(_)) => {
                let (x, y) = (as_f64(a), as_f64(b));
                Float(match op { Add => x + y, Sub => x - y, _ => x * y })
            }
            (Value::Array(x), Value::Array(y)) if op == Add => {
                let mut r = (**x).clone();
                for (k, v) in &y.entries { if r.get(k).is_none() { r.set(k.clone(), v.clone()); } }
                Value::Array(Rc::new(r))
            }
            _ => return Err(bad()),
        },
        Div => match (a, b) {
            (Int(_) | Float(_), Int(_) | Float(_)) => {
                if as_f64(b) == 0.0 { return Err("dzielenie przez zero".into()); }
                match (a, b) {
                    (Int(x), Int(y)) if x % y == 0 => Int(x / y),
                    _ => Float(as_f64(a) / as_f64(b)),
                }
            }
            _ => return Err(bad()),
        },
        Mod => match (a, b) {
            (Int(x), Int(y)) => { if *y == 0 { return Err("modulo przez zero".into()); } Int(x.wrapping_rem(*y)) }
            _ => return Err(bad()),
        },
        Pow => match (a, b) {
            (Int(x), Int(y)) if *y >= 0 => match x.checked_pow((*y).min(u32::MAX as i64) as u32) {
                Some(r) => Int(r), None => Float((*x as f64).powf(*y as f64)) },
            (Int(_) | Float(_), Int(_) | Float(_)) => Float(as_f64(a).powf(as_f64(b))),
            _ => return Err(bad()),
        },
        Eq => Value::Bool(loose_eq(a, b)),
        NotEq => Value::Bool(!loose_eq(a, b)),
        Identical => Value::Bool(strict_eq(a, b)),
        NotIdentical => Value::Bool(!strict_eq(a, b)),
        Lt | Gt | Le | Ge => {
            let o = compare(a, b)?;
            Value::Bool(match op { Lt => o.is_lt(), Gt => o.is_gt(), Le => o.is_le(), _ => o.is_ge() })
        }
        BitAnd | BitOr | BitXor | Shl | Shr => match (a, b) {
            (Int(x), Int(y)) => Int(match op {
                BitAnd => x & y, BitOr => x | y, BitXor => x ^ y,
                Shl => x.wrapping_shl(*y as u32), _ => x.wrapping_shr(*y as u32) }),
            _ => return Err(bad()),
        },
        Xor => Value::Bool(a.truthy() ^ b.truthy()),
        And | Or | Coalesce => unreachable!("obsługiwane z krótkim spięciem w interpreterze"),
    })
}
