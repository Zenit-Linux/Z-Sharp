use crate::value::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use zs_parser::ast::*;

pub enum Flow { Break(u32), Continue(u32), Return(Value), Throw(Value), Err(String), Exit(i32) }
pub type R<T> = Result<T, Flow>;
pub fn rt<T>(m: impl Into<String>) -> R<T> { Err(Flow::Err(m.into())) }
pub fn rc(s: &str) -> Name { Rc::from(s) }

pub struct Frame {
    pub vars: HashMap<Name, Value>,
    pub this: Option<Value>,
    pub class: Option<Name>,
    pub module: Option<Name>,
}

pub struct MethodInfo { pub decl: Rc<FuncDecl>, pub is_static: bool, pub is_abstract: bool, pub class: Name }

pub struct Class {
    pub name: Name,
    pub decl: Rc<ClassDecl>,
    pub parent: Option<Name>,
    pub interfaces: Vec<Name>,
    pub methods: HashMap<Name, Rc<MethodInfo>>,
    pub statics: RefCell<HashMap<Name, Value>>,
    pub consts: HashMap<Name, Value>,
    pub module: Option<Name>,
}

#[derive(Default)]
pub struct Module { pub funcs: HashMap<Name, Rc<Closure>> }

pub type CallArgs = Vec<(Option<Name>, Value)>;

const PRELUDE: &str = r#"
interface Throwable {}
class Exception implements Throwable {
    public $message; public $code; public $previous; public $line;
    public func __construct($message = "", $code = 0, $previous = null) {
        $this->message = $message; $this->code = $code; $this->previous = $previous; $this->line = 0;
    }
    public func getMessage() { return $this->message; }
    public func getCode() { return $this->code; }
    public func getPrevious() { return $this->previous; }
    public func getLine() { return $this->line; }
}
class Error extends Exception {}
class TypeError extends Error {}
class ValueError extends Error {}
class IOError extends Error {}
"#;

pub const BUILTIN_MODULES: &[&str] = &["System", "Math", "File", "Directory", "Process", "Task"];

pub struct Interp {
    pub frames: Vec<Frame>,
    pub funcs: HashMap<Name, Rc<Closure>>,
    pub classes: HashMap<Name, Rc<Class>>,
    pub modules: HashMap<String, Module>,
    pub aliases: HashMap<String, String>,
    pub loaded: HashSet<PathBuf>,
    pub externs: HashSet<Name>,
    pub dir_stack: Vec<PathBuf>,
    pub cur_module: Option<Name>,
    pub line: u32,
    pub args: Vec<String>,
    pub out: std::io::BufWriter<std::io::Stdout>,
}

/// Uruchamia plik .zs; zwraca kod wyjścia procesu.
pub fn run_file(path: &str, args: Vec<String>) -> i32 {
    let path = path.to_string();
    // głęboka rekurencja drzewa AST wymaga dużego stosu
    let h = std::thread::Builder::new().stack_size(1 << 30).spawn(move || {
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => s, Err(e) => { eprintln!("{path}: {e}"); return 1; }
        };
        let prog = match zs_parser::parse(&src) {
            Ok(p) => p, Err(e) => { eprintln!("{path}:{e}"); return 1; }
        };
        let mut it = Interp::new(args);
        let dir = Path::new(&path).canonicalize().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| PathBuf::from("."));
        it.dir_stack.push(dir);
        let code = it.run_program(&prog);
        it.flush();
        code
    }).unwrap();
    h.join().unwrap_or(1)
}

impl Interp {
    pub fn new(args: Vec<String>) -> Self {
        let mut it = Interp {
            frames: vec![Frame { vars: HashMap::new(), this: None, class: None, module: None }],
            funcs: HashMap::new(), classes: HashMap::new(), modules: HashMap::new(),
            aliases: HashMap::new(), loaded: HashSet::new(), externs: HashSet::new(),
            dir_stack: vec![], cur_module: None, line: 0, args,
            out: std::io::BufWriter::new(std::io::stdout()),
        };
        let prelude = zs_parser::parse(PRELUDE).expect("prelude");
        it.hoist(&prelude).ok();
        it
    }

    pub fn flush(&mut self) { let _ = self.out.flush(); }

    fn run_program(&mut self, prog: &[Stmt]) -> i32 {
        let r = self.hoist(prog).and_then(|_| self.exec_block(prog));
        match r {
            Ok(()) => 0,
            Err(Flow::Exit(c)) => c,
            Err(f) => { self.report_uncaught(f); 1 }
        }
    }

    fn report_uncaught(&mut self, f: Flow) {
        self.flush();
        let exc = match self.flow_to_exc(&f) { Some(e) => e, None => { eprintln!("błąd: nieoczekiwane break/continue/return poza kontekstem"); return; } };
        if let Value::Object(o) = &exc {
            let o = o.borrow();
            let msg = o.props.get("message").map(to_text).unwrap_or_default();
            let line = o.props.get("line").map(to_text).unwrap_or_default();
            eprintln!("Nieobsłużony wyjątek {}: {} (linia {})", o.class, msg, line);
        }
    }

    // ---------- błędy -> wyjątki ----------
    fn make_error(&mut self, msg: &str) -> Value {
        let line = self.line;
        match self.new_object("Error", vec![(None, Value::str(msg))]) {
            Ok(v) => { if let Value::Object(o) = &v { o.borrow_mut().props.insert(rc("line"), Value::Int(line as i64)); } v }
            Err(_) => Value::str(msg),
        }
    }

    fn flow_to_exc(&mut self, f: &Flow) -> Option<Value> {
        match f {
            Flow::Throw(v) => Some(v.clone()),
            Flow::Err(m) => Some(self.make_error(m)),
            _ => None,
        }
    }

    // ---------- hoisting / deklaracje ----------
    pub fn hoist(&mut self, stmts: &[Stmt]) -> R<()> {
        for s in stmts { if let StmtKind::Func(d) = &s.kind { self.declare_func(d); } }
        let mut pending: Vec<&Rc<ClassDecl>> = stmts.iter().filter_map(|s| if let StmtKind::Class(c) = &s.kind { Some(c) } else { None }).collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut rest = vec![];
            for c in pending {
                let ready = c.parent.as_ref().map_or(true, |p| self.classes.contains_key(p))
                    && c.interfaces.iter().all(|i| self.classes.contains_key(i));
                if ready { self.declare_class(c)?; } else { rest.push(c); }
            }
            if rest.len() == before {
                return rt(format!("nie można zadeklarować klasy {}: brak klasy bazowej lub interfejsu", rest[0].name));
            }
            pending = rest;
        }
        Ok(())
    }

    fn declare_func(&mut self, d: &Rc<FuncDecl>) {
        let name = d.name.clone().unwrap();
        let f = Rc::new(Closure { decl: d.clone(), captured: HashMap::new(), this: None, class: None, module: self.cur_module.clone() });
        match &self.cur_module {
            Some(m) => { self.modules.entry(m.to_string()).or_default().funcs.insert(name, f); }
            None => { self.funcs.insert(name, f); }
        }
    }

    pub fn declare_class(&mut self, d: &Rc<ClassDecl>) -> R<()> {
        if self.classes.contains_key(&d.name) { return Ok(()); }
        if let Some(p) = &d.parent { if !self.classes.contains_key(p) { return rt(format!("nieznana klasa bazowa {}", p)); } }
        let mut methods = HashMap::new();
        for m in &d.methods {
            let n = m.decl.name.clone().unwrap();
            methods.insert(n, Rc::new(MethodInfo { decl: m.decl.clone(), is_static: m.is_static, is_abstract: m.is_abstract, class: d.name.clone() }));
        }
        // stałe i statyczne w kontekście klasy
        self.frames.push(Frame { vars: HashMap::new(), this: None, class: Some(d.name.clone()), module: self.cur_module.clone() });
        let mut consts = HashMap::new();
        let mut statics = HashMap::new();
        let mut res = Ok(());
        for (n, e) in &d.consts { match self.eval(e) { Ok(v) => { consts.insert(n.clone(), v); } Err(f) => { res = Err(f); break; } } }
        if res.is_ok() {
            for p in d.props.iter().filter(|p| p.is_static) {
                let v = match &p.default { Some(e) => match self.eval(e) { Ok(v) => v, Err(f) => { res = Err(f); break; } }, None => Value::Null };
                statics.insert(p.name.clone(), v);
            }
        }
        self.frames.pop();
        res?;
        let class = Class { name: d.name.clone(), decl: d.clone(), parent: d.parent.clone(), interfaces: d.interfaces.clone(),
            methods, statics: RefCell::new(statics), consts, module: self.cur_module.clone() };
        self.classes.insert(d.name.clone(), Rc::new(class));
        Ok(())
    }

    pub fn find_method(&self, class: &str, m: &str) -> Option<Rc<MethodInfo>> {
        let mut cur = self.classes.get(class)?;
        loop {
            if let Some(mi) = cur.methods.get(m) { return Some(mi.clone()); }
            cur = self.classes.get(cur.parent.as_ref()?)?;
        }
    }

    pub fn instance_of(&self, class: &str, target: &str) -> bool {
        if class == target { return true; }
        let Some(c) = self.classes.get(class) else { return false };
        if let Some(p) = &c.parent { if self.instance_of(p, target) { return true; } }
        c.interfaces.iter().any(|i| self.instance_of(i, target))
    }

    pub fn value_instance_of(&self, v: &Value, target: &str) -> bool {
        match v { Value::Object(o) => self.instance_of(&o.borrow().class, target), _ => false }
    }

    fn chain(&self, class: &str) -> Vec<Rc<Class>> {
        let mut v = vec![];
        let mut cur = self.classes.get(class).cloned();
        while let Some(c) = cur { cur = c.parent.as_ref().and_then(|p| self.classes.get(p).cloned()); v.push(c); }
        v.reverse();
        v
    }

    pub fn new_object(&mut self, cname: &str, args: CallArgs) -> R<Value> {
        let Some(class) = self.classes.get(cname).cloned() else { return rt(format!("nieznana klasa {}", cname)) };
        if class.decl.is_abstract || class.decl.is_interface { return rt(format!("nie można utworzyć instancji {} (abstrakcyjna/interfejs)", cname)); }
        let mut obj = Object { class: class.name.clone(), props: HashMap::new(), order: vec![] };
        for c in self.chain(cname) {
            self.frames.push(Frame { vars: HashMap::new(), this: None, class: Some(c.name.clone()), module: c.module.clone() });
            for p in c.decl.props.iter().filter(|p| !p.is_static) {
                let v = match &p.default { Some(e) => match self.eval(e) { Ok(v) => v, Err(f) => { self.frames.pop(); return Err(f); } }, None => Value::Null };
                if !obj.props.contains_key(&p.name) { obj.order.push(p.name.clone()); }
                obj.props.insert(p.name.clone(), v);
            }
            self.frames.pop();
        }
        let o = Value::Object(Rc::new(RefCell::new(obj)));
        if let Some(ctor) = self.find_method(cname, "__construct") {
            let module = self.classes.get(&ctor.class).and_then(|c| c.module.clone());
            self.call_decl(&ctor.decl, None, Some(o.clone()), Some(ctor.class.clone()), module, args)?;
        }
        Ok(o)
    }

    // ---------- wykonywanie ----------
    pub fn exec_block(&mut self, stmts: &[Stmt]) -> R<()> {
        for s in stmts { self.exec(s)?; }
        Ok(())
    }

    fn loop_body(&mut self, b: &[Stmt]) -> R<bool> {
        match self.exec_block(b) {
            Ok(()) => Ok(false),
            Err(Flow::Break(n)) => if n > 1 { Err(Flow::Break(n - 1)) } else { Ok(true) },
            Err(Flow::Continue(n)) => if n > 1 { Err(Flow::Continue(n - 1)) } else { Ok(false) },
            Err(e) => Err(e),
        }
    }

    fn set_var(&mut self, n: &Name, v: Value) { self.frames.last_mut().unwrap().vars.insert(n.clone(), v); }

    fn exec(&mut self, s: &Stmt) -> R<()> {
        self.line = s.line;
        match &s.kind {
            StmtKind::Expr(e) => { self.eval(e)?; }
            StmtKind::Echo(es) => {
                for e in es { let v = self.eval(e)?; let _ = self.out.write_all(to_text(&v).as_bytes()); }
            }
            StmtKind::Block(b) => self.exec_block(b)?,
            StmtKind::If(c, t, e) => {
                if self.eval(c)?.truthy() { self.exec_block(t)?; } else if let Some(e) = e { self.exec_block(e)?; }
            }
            StmtKind::While(c, b) => { while self.eval(c)?.truthy() { if self.loop_body(b)? { break; } } }
            StmtKind::DoWhile(b, c) => loop { if self.loop_body(b)? { break; } if !self.eval(c)?.truthy() { break; } },
            StmtKind::For(init, cond, step, b) => {
                for e in init { self.eval(e)?; }
                loop {
                    if let Some(c) = cond { if !self.eval(c)?.truthy() { break; } }
                    if self.loop_body(b)? { break; }
                    for e in step { self.eval(e)?; }
                }
            }
            StmtKind::Foreach { iter, key, value, body } => {
                let it = self.eval(iter)?;
                let Value::Array(arr) = it else { return rt(format!("foreach wymaga tablicy, jest {}", it.type_name())) };
                for (k, v) in arr.entries.iter() {
                    if let Some(kn) = key { self.set_var(kn, key_to_value(k)); }
                    self.set_var(value, v.clone());
                    if self.loop_body(body)? { break; }
                }
            }
            StmtKind::Switch(subj, cases) => {
                let v = self.eval(subj)?;
                let mut start = None;
                for (i, c) in cases.iter().enumerate() {
                    if let Some(ce) = &c.cond { let cv = self.eval(ce)?; if loose_eq(&v, &cv) { start = Some(i); break; } }
                }
                if start.is_none() { start = cases.iter().position(|c| c.cond.is_none()); }
                if let Some(i) = start {
                    for c in &cases[i..] {
                        match self.exec_block(&c.body) {
                            Ok(()) => {}
                            Err(Flow::Break(n)) => { if n > 1 { return Err(Flow::Break(n - 1)); } break; }
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
            StmtKind::Return(e) => { let v = match e { Some(e) => self.eval(e)?, None => Value::Null }; return Err(Flow::Return(v)); }
            StmtKind::Break(n) => return Err(Flow::Break(*n)),
            StmtKind::Continue(n) => return Err(Flow::Continue(*n)),
            StmtKind::Throw(e) => {
                let v = self.eval(e)?;
                if !self.value_instance_of(&v, "Throwable") { return rt("throw wymaga obiektu typu Throwable"); }
                if let Value::Object(o) = &v {
                    let mut ob = o.borrow_mut();
                    if matches!(ob.props.get("line"), Some(Value::Int(0))) { ob.props.insert(rc("line"), Value::Int(s.line as i64)); }
                }
                return Err(Flow::Throw(v));
            }
            StmtKind::Try { body, catches, finally } => {
                let mut result = self.exec_block(body);
                if let Err(f) = &result {
                    if let Some(exc) = self.flow_to_exc(f) {
                        let mut handled = false;
                        for c in catches {
                            if c.types.iter().any(|t| &**t == "Throwable" || self.value_instance_of(&exc, t)) {
                                if let Some(v) = &c.var { self.set_var(v, exc.clone()); }
                                result = self.exec_block(&c.body);
                                handled = true;
                                break;
                            }
                        }
                        if !handled { result = Err(Flow::Throw(exc)); }
                    }
                }
                if let Some(fin) = finally { self.exec_block(fin)?; }
                return result;
            }
            StmtKind::Func(d) => { if self.frames.len() > 1 || d.name.is_some() { self.declare_func(d); } }
            StmtKind::Class(c) => self.declare_class(c)?,
            StmtKind::Import(p) => self.import(p)?,
            StmtKind::Namespace(_) => {}
            StmtKind::Using { resource, var, body } => {
                let r = self.eval(resource)?;
                self.set_var(var, r.clone());
                let res = self.exec_block(body);
                if let Value::Object(o) = &r {
                    let cls = o.borrow().class.clone();
                    if self.find_method(&cls, "close").is_some() {
                        let c = self.call_method(&r, "close", vec![]);
                        if res.is_ok() { c?; }
                    }
                }
                return res;
            }
            StmtKind::Extern { lib: _, items } => { for i in items { self.externs.insert(i.name.clone()); } }
        }
        Ok(())
    }

    // ---------- import ----------
    fn import(&mut self, path: &[Name]) -> R<()> {
        if BUILTIN_MODULES.contains(&&*path[0]) { return Ok(()); }
        let Some(file) = self.resolve_import(path) else {
            return rt(format!("nie znaleziono modułu {}", path.iter().map(|p| &**p).collect::<Vec<_>>().join(".")));
        };
        let canon = file.canonicalize().unwrap_or(file.clone());
        let full = path.iter().map(|p| &**p).collect::<Vec<_>>().join(".");
        self.aliases.insert(path.last().unwrap().to_string(), full.clone());
        if !self.loaded.insert(canon) { return Ok(()); }
        let src = std::fs::read_to_string(&file).map_err(|e| Flow::Err(format!("{}: {}", file.display(), e)))?;
        let prog = zs_parser::parse(&src).map_err(|e| Flow::Err(format!("{}:{}", file.display(), e)))?;
        self.modules.entry(full.clone()).or_default();
        let saved_mod = self.cur_module.replace(rc(&full));
        self.dir_stack.push(file.parent().map(|p| p.to_path_buf()).unwrap_or_default());
        self.frames.push(Frame { vars: HashMap::new(), this: None, class: None, module: Some(rc(&full)) });
        let saved_line = self.line;
        let r = self.hoist(&prog).and_then(|_| self.exec_block(&prog));
        self.frames.pop();
        self.dir_stack.pop();
        self.cur_module = saved_mod;
        self.line = saved_line;
        r
    }

    fn workspace_members(&self) -> Option<(PathBuf, HashMap<String, String>)> {
        let mut roots: Vec<PathBuf> = vec![];
        if let Ok(r) = std::env::var("ZS_WORKSPACE") { roots.push(PathBuf::from(r)); }
        if let Some(d) = self.dir_stack.last() {
            for a in d.ancestors() { roots.push(a.to_path_buf()); roots.push(a.join("Z-Sharp")); }
        }
        for r in roots {
            let m = r.join("zworkspace.json");
            if let Ok(text) = std::fs::read_to_string(&m) { return Some((r, parse_members(&text))); }
        }
        None
    }

    fn resolve_import(&self, path: &[Name]) -> Option<PathBuf> {
        let segs: Vec<&str> = path.iter().map(|p| &**p).collect();
        if segs[0] == "Workspace" && segs.len() >= 2 {
            let (root, members) = self.workspace_members()?;
            let rel = members.get(segs[1])?;
            let mut f = root.join(rel);
            for extra in &segs[2..] { f = f.parent()?.join(format!("{}.zs", extra)); }
            return if f.exists() { Some(f) } else { None };
        }
        let mut dirs: Vec<PathBuf> = vec![];
        if let Some(d) = self.dir_stack.last() { dirs.push(d.clone()); }
        if let Ok(c) = std::env::current_dir() { dirs.push(c); }
        if let Ok(p) = std::env::var("ZS_PATH") { dirs.extend(p.split(':').map(PathBuf::from)); }
        for d in dirs {
            let mut f = d.clone();
            for s in &segs[..segs.len() - 1] { f.push(s); }
            f.push(format!("{}.zs", segs[segs.len() - 1]));
            if f.exists() { return Some(f); }
        }
        None
    }

    // ---------- wyrażenia ----------
    fn lookup_var(&self, n: &str) -> Option<Value> { self.frames.last().unwrap().vars.get(n).cloned() }

    pub fn eval(&mut self, e: &Expr) -> R<Value> {
        Ok(match &e.kind {
            ExprKind::Null => Value::Null,
            ExprKind::Bool(b) => Value::Bool(*b),
            ExprKind::Int(i) => Value::Int(*i),
            ExprKind::Float(f) => Value::Float(*f),
            ExprKind::Str(s) => Value::Str(s.clone()),
            ExprKind::Var(n) => match self.lookup_var(n) { Some(v) => v, None => return rt(format!("niezdefiniowana zmienna ${}", n)) },
            ExprKind::Name(n) => self.resolve_name(n)?,
            ExprKind::Array(items) => {
                let mut a = ZArray::new();
                for (k, v) in items {
                    match k {
                        Some(k) => { let kv = self.eval(k)?; let vv = self.eval(v)?; a.set(to_key(&kv).map_err(Flow::Err)?, vv); }
                        None => { let vv = self.eval(v)?; a.push(vv); }
                    }
                }
                Value::Array(Rc::new(a))
            }
            ExprKind::Index(b, i) => {
                let base = self.eval(b)?;
                let Some(i) = i else { return rt("`[]` bez indeksu jest dozwolone tylko przy zapisie") };
                let idx = self.eval(i)?;
                self.index_value(&base, &idx, false)?
            }
            ExprKind::Prop(b, n) => { let base = self.eval(b)?; self.get_prop(&base, n, false)? }
            ExprKind::Dot(b, n) => self.eval_dot(b, n)?,
            ExprKind::Call(c, args) => { self.line = e.line; self.eval_call(c, args)? }
            ExprKind::New(n, args) => { self.line = e.line; let a = self.eval_args(args)?; self.new_object(n, a)? }
            ExprKind::Unary(op, x) => {
                let v = self.eval(x)?;
                match (op, &v) {
                    (UnOp::Not, _) => Value::Bool(!v.truthy()),
                    (UnOp::Neg, Value::Int(i)) => Value::Int(i.checked_neg().ok_or_else(|| Flow::Err("przepełnienie liczby całkowitej".into()))?),
                    (UnOp::Neg, Value::Float(f)) => Value::Float(-f),
                    (UnOp::Plus, Value::Int(_) | Value::Float(_)) => v,
                    (UnOp::BitNot, Value::Int(i)) => Value::Int(!i),
                    _ => return rt(format!("operator jednoargumentowy {:?} niedostępny dla {}", op, v.type_name())),
                }
            }
            ExprKind::Binary(op, l, r) => match op {
                BinOp::And => { if !self.eval(l)?.truthy() { Value::Bool(false) } else { Value::Bool(self.eval(r)?.truthy()) } }
                BinOp::Or => { if self.eval(l)?.truthy() { Value::Bool(true) } else { Value::Bool(self.eval(r)?.truthy()) } }
                BinOp::Coalesce => { let v = self.eval_quiet(l)?; if matches!(v, Value::Null) { self.eval(r)? } else { v } }
                _ => { let a = self.eval(l)?; let b = self.eval(r)?; self.line = e.line; binop(*op, &a, &b).map_err(Flow::Err)? }
            },
            ExprKind::Assign(t, v) => {
                let val = self.eval(v)?;
                let vv = val.clone();
                self.modify(t, &mut |slot| { *slot = vv.clone(); Ok(()) })?;
                val
            }
            ExprKind::AssignOp(op, t, v) => {
                if *op == BinOp::Coalesce {
                    let cur = self.eval_quiet(t)?;
                    if !matches!(cur, Value::Null) { return Ok(cur); }
                    let val = self.eval(v)?;
                    let vv = val.clone();
                    self.modify(t, &mut |slot| { *slot = vv.clone(); Ok(()) })?;
                    val
                } else {
                    let rhs = self.eval(v)?;
                    let op = *op;
                    let mut result = Value::Null;
                    self.line = e.line;
                    self.modify(t, &mut |slot| {
                        let cur = std::mem::replace(slot, Value::Null);
                        *slot = binop(op, &cur, &rhs).map_err(Flow::Err).map_err(|f| { *slot = cur; f })?;
                        result = slot.clone();
                        Ok(())
                    })?;
                    result
                }
            }
            ExprKind::IncDec { target, inc, prefix } => {
                let (inc, prefix) = (*inc, *prefix);
                let mut old = Value::Null; let mut new = Value::Null;
                self.modify(target, &mut |slot| {
                    old = slot.clone();
                    let d = if inc { 1 } else { -1 };
                    *slot = match &*slot {
                        Value::Int(i) => Value::Int(i.checked_add(d).ok_or_else(|| Flow::Err("przepełnienie liczby całkowitej".into()))?),
                        Value::Float(f) => Value::Float(f + d as f64),
                        Value::Null => Value::Int(d),
                        o => return rt(format!("++/-- niedostępne dla {}", o.type_name())),
                    };
                    new = slot.clone();
                    Ok(())
                })?;
                if prefix { new } else { old }
            }
            ExprKind::Ternary(c, t, f) => {
                let cv = self.eval(c)?;
                match t { Some(t) => if cv.truthy() { self.eval(t)? } else { self.eval(f)? }, None => if cv.truthy() { cv } else { self.eval(f)? } }
            }
            ExprKind::Closure(d) => {
                let fr = self.frames.last().unwrap();
                Value::Func(Rc::new(Closure { decl: d.clone(), captured: fr.vars.clone(), this: fr.this.clone(), class: fr.class.clone(), module: fr.module.clone() }))
            }
            ExprKind::Match(s, arms) => {
                let v = self.eval(s)?;
                for arm in arms {
                    match &arm.conds {
                        None => return self.eval(&arm.body),
                        Some(cs) => for c in cs { let cv = self.eval(c)?; if strict_eq(&v, &cv) { return self.eval(&arm.body); } },
                    }
                }
                return rt(format!("brak dopasowania w match dla {}", dump(&v)));
            }
            ExprKind::Await(x) => self.eval(x)?,
            ExprKind::InstanceOf(x, n) => { let v = self.eval(x)?; Value::Bool(self.value_instance_of(&v, n)) }
        })
    }

    /// Wersja `??`: brakujące zmienne/klucze/właściwości dają null.
    fn eval_quiet(&mut self, e: &Expr) -> R<Value> {
        match &e.kind {
            ExprKind::Var(n) => Ok(self.lookup_var(n).unwrap_or(Value::Null)),
            ExprKind::Index(b, Some(i)) => {
                let base = self.eval_quiet(b)?;
                if matches!(base, Value::Null) { return Ok(Value::Null); }
                let idx = self.eval(i)?;
                self.index_value(&base, &idx, true)
            }
            ExprKind::Prop(b, n) => {
                let base = self.eval_quiet(b)?;
                if matches!(base, Value::Null) { return Ok(Value::Null); }
                self.get_prop(&base, n, true)
            }
            ExprKind::Dot(..) => match self.eval(e) { Err(Flow::Err(_)) => Ok(Value::Null), r => r },
            _ => self.eval(e),
        }
    }

    fn resolve_name(&mut self, n: &Name) -> R<Value> {
        let fr = self.frames.last().unwrap();
        match &**n {
            "self" | "static" => return match &fr.class { Some(c) => Ok(Value::Class(c.clone())), None => rt("`self` poza klasą") },
            "parent" => {
                let c = fr.class.clone();
                return match c.and_then(|c| self.classes.get(&c).and_then(|c| c.parent.clone())) {
                    Some(p) => Ok(Value::Class(p)), None => rt("`parent` bez klasy bazowej") };
            }
            "PI" => return Ok(Value::Float(std::f64::consts::PI)),
            "EOL" => return Ok(Value::str("\n")),
            "INT_MAX" => return Ok(Value::Int(i64::MAX)),
            "INT_MIN" => return Ok(Value::Int(i64::MIN)),
            _ => {}
        }
        if self.classes.contains_key(n) { return Ok(Value::Class(n.clone())); }
        if let Some(full) = self.aliases.get(&**n) { return Ok(Value::Module(rc(full))); }
        if self.modules.contains_key(&**n) || BUILTIN_MODULES.contains(&&**n) { return Ok(Value::Module(n.clone())); }
        rt(format!("nieznana nazwa `{}` (klasa, moduł lub stała)", n))
    }

    fn eval_dot(&mut self, base: &Expr, name: &Name) -> R<Value> {
        let b = self.eval(base)?;
        match b {
            Value::Class(c) => {
                let mut cur = self.classes.get(&c).cloned();
                while let Some(cl) = cur {
                    if let Some(v) = cl.statics.borrow().get(name) { return Ok(v.clone()); }
                    if let Some(v) = cl.consts.get(name) { return Ok(v.clone()); }
                    cur = cl.parent.as_ref().and_then(|p| self.classes.get(p).cloned());
                }
                rt(format!("klasa {} nie ma składowej statycznej {}", c, name))
            }
            Value::Module(m) => {
                let full = format!("{}.{}", m, name);
                if self.modules.contains_key(&full) { return Ok(Value::Module(rc(&full))); }
                if let Some(f) = self.modules.get(&*m).and_then(|md| md.funcs.get(name)) { return Ok(Value::Func(f.clone())); }
                if let Some(v) = self.builtin_const(&m, name) { return Ok(v); }
                rt(format!("moduł {} nie ma składowej {}", m, name))
            }
            _ => rt("dostęp przez `.` wymaga modułu lub klasy (dla obiektów użyj `->`)"),
        }
    }

    pub fn index_value(&self, base: &Value, idx: &Value, quiet: bool) -> R<Value> {
        match base {
            Value::Array(a) => {
                let k = to_key(idx).map_err(Flow::Err)?;
                match a.get(&k) { Some(v) => Ok(v.clone()), None if quiet => Ok(Value::Null),
                    None => rt(format!("brak klucza {}", match &k { Key::Int(i) => i.to_string(), Key::Str(s) => format!("{:?}", s) })) }
            }
            Value::Str(s) => {
                let Value::Int(i) = idx else { return rt("indeks tekstu musi być liczbą całkowitą") };
                let len = s.len() as i64;
                let i = if *i < 0 { len + i } else { *i };
                if i < 0 || i >= len { return if quiet { Ok(Value::Null) } else { rt(format!("indeks {} poza zakresem tekstu (długość {})", i, len)) }; }
                // indeks bajtowy; zwraca cały znak UTF-8 zaczynający się w tym bajcie
                if !s.is_char_boundary(i as usize) {
                    return if quiet { Ok(Value::Null) } else { rt(format!("indeks {} wskazuje środek znaku UTF-8", i)) };
                }
                Ok(Value::string(s[i as usize..].chars().next().unwrap().to_string()))
            }
            Value::Null => rt("indeksowanie wartości null"),
            o => rt(format!("typ {} nie obsługuje indeksowania", o.type_name())),
        }
    }

    pub fn get_prop(&self, base: &Value, name: &Name, quiet: bool) -> R<Value> {
        match base {
            Value::Object(o) => match o.borrow().props.get(name) {
                Some(v) => Ok(v.clone()),
                None if quiet => Ok(Value::Null),
                None => rt(format!("obiekt klasy {} nie ma właściwości {}", o.borrow().class, name)),
            },
            Value::Null => rt(format!("dostęp do właściwości `{}` wartości null", name)),
            o => rt(format!("dostęp do właściwości `{}` typu {}", name, o.type_name())),
        }
    }

    // ---------- l-wartości ----------
    fn modify(&mut self, target: &Expr, f: &mut dyn FnMut(&mut Value) -> R<()>) -> R<()> {
        let mut segs: Vec<Seg> = vec![];
        let root = self.flatten(target, &mut segs)?;
        match root {
            Root::Var(n) => {
                let fr = self.frames.last_mut().unwrap();
                let slot = fr.vars.entry(n).or_insert(Value::Null);
                walk(slot, &segs, f)
            }
            Root::Static(class, name) => {
                let mut cur = self.classes.get(&class).cloned();
                while let Some(cl) = cur {
                    if cl.statics.borrow().contains_key(&name) {
                        let mut st = cl.statics.borrow_mut();
                        return walk(st.get_mut(&name).unwrap(), &segs, f);
                    }
                    cur = cl.parent.as_ref().and_then(|p| self.classes.get(p).cloned());
                }
                rt(format!("klasa {} nie ma składowej statycznej {}", class, name))
            }
        }
    }

    fn flatten(&mut self, e: &Expr, segs: &mut Vec<Seg>) -> R<Root> {
        match &e.kind {
            ExprKind::Var(n) => Ok(Root::Var(n.clone())),
            ExprKind::Index(b, i) => {
                let r = self.flatten(b, segs)?;
                segs.push(match i { Some(i) => Seg::Idx(self.eval(i)?), None => Seg::Push });
                Ok(r)
            }
            ExprKind::Prop(b, n) => { let r = self.flatten(b, segs)?; segs.push(Seg::Prop(n.clone())); Ok(r) }
            ExprKind::Dot(b, n) => match self.eval(b)? {
                Value::Class(c) => Ok(Root::Static(c, n.clone())),
                _ => rt("przypisanie przez `.` możliwe tylko do składowej statycznej klasy"),
            },
            _ => rt("niepoprawna l-wartość"),
        }
    }

    // ---------- wywołania ----------
    fn eval_args(&mut self, args: &[Arg]) -> R<CallArgs> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = self.eval(&a.value)?;
            if a.spread {
                let Value::Array(arr) = v else { return rt("`...` wymaga tablicy") };
                for (k, x) in arr.entries.iter() {
                    out.push((if let Key::Str(s) = k { Some(s.clone()) } else { None }, x.clone()));
                }
            } else { out.push((a.name.clone(), v)); }
        }
        Ok(out)
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Arg]) -> R<Value> {
        match &callee.kind {
            ExprKind::Name(n) => { let a = self.eval_args(args)?; self.line = callee.line; self.call_named(n, a) }
            ExprKind::Prop(obj, m) => { let o = self.eval(obj)?; let a = self.eval_args(args)?; self.line = callee.line; self.call_method(&o, m, a) }
            ExprKind::Dot(base, m) => {
                let b = self.eval(base)?;
                let a = self.eval_args(args)?;
                self.line = callee.line;
                match b {
                    Value::Class(c) => self.call_static(&c, m, a),
                    Value::Module(md) => self.call_module(&md, m, a),
                    _ => rt("wywołanie przez `.` wymaga modułu lub klasy (dla obiektów użyj `->`)"),
                }
            }
            _ => { let f = self.eval(callee)?; let a = self.eval_args(args)?; self.call_value(&f, a) }
        }
    }

    pub fn call_named(&mut self, n: &Name, args: CallArgs) -> R<Value> {
        let module = self.frames.last().unwrap().module.clone();
        let f = module.as_ref().and_then(|m| self.modules.get(&**m)).and_then(|m| m.funcs.get(n)).cloned()
            .or_else(|| self.funcs.get(n).cloned());
        if let Some(f) = f { return self.call_closure(&f, args); }
        if let Some(r) = self.call_builtin(n, &args) { return r; }
        if self.externs.contains(n) { return rt(format!("funkcja extern `{}`: linkowanie extern jest dostępne tylko w zsc (stage0 interpreter go nie obsługuje)", n)); }
        // zmienna o nazwie funkcji? nie — PHP-podobnie wymaga $f()
        rt(format!("niezdefiniowana funkcja `{}`", n))
    }

    pub fn call_value(&mut self, f: &Value, args: CallArgs) -> R<Value> {
        match f {
            Value::Func(c) => self.call_closure(c, args),
            Value::Str(s) => self.call_named(&rc(s), args),
            o => rt(format!("wartość typu {} nie jest wywoływalna", o.type_name())),
        }
    }

    pub fn call_closure(&mut self, c: &Rc<Closure>, args: CallArgs) -> R<Value> {
        self.call_decl(&c.decl, Some(&c.captured), c.this.clone(), c.class.clone(), c.module.clone(), args)
    }

    pub fn call_method(&mut self, obj: &Value, m: &str, args: CallArgs) -> R<Value> {
        let Value::Object(o) = obj else {
            return rt(format!("wywołanie metody `{}` na wartości typu {}", m, obj.type_name()));
        };
        let cls = o.borrow().class.clone();
        if let Some(mi) = self.find_method(&cls, m) {
            if mi.is_abstract && mi.decl.body.is_empty() { return rt(format!("metoda abstrakcyjna {}.{}", mi.class, m)); }
            let module = self.classes.get(&mi.class).and_then(|c| c.module.clone());
            return self.call_decl(&mi.decl, None, if mi.is_static { None } else { Some(obj.clone()) }, Some(mi.class.clone()), module, args);
        }
        let prop = o.borrow().props.get(m).cloned();
        if let Some(f @ Value::Func(_)) = prop { return self.call_value(&f, args); }
        rt(format!("klasa {} nie ma metody {}", cls, m))
    }

    fn call_static(&mut self, class: &Name, m: &str, args: CallArgs) -> R<Value> {
        let Some(mi) = self.find_method(class, m) else { return rt(format!("klasa {} nie ma metody {}", class, m)) };
        let module = self.classes.get(&mi.class).and_then(|c| c.module.clone());
        let this = if mi.is_static { None } else {
            match self.frames.last().unwrap().this.clone() {
                Some(t) if self.value_instance_of(&t, class) => Some(t),
                _ => return rt(format!("metoda {}.{} nie jest statyczna", class, m)),
            }
        };
        self.call_decl(&mi.decl, None, this, Some(mi.class.clone()), module, args)
    }

    fn call_module(&mut self, md: &Name, name: &Name, args: CallArgs) -> R<Value> {
        if let Some(f) = self.modules.get(&**md).and_then(|m| m.funcs.get(name)).cloned() { return self.call_closure(&f, args); }
        if let Some(r) = self.call_builtin_module(md, name, &args) { return r; }
        rt(format!("moduł {} nie ma funkcji {}", md, name))
    }

    pub fn call_decl(&mut self, decl: &Rc<FuncDecl>, captured: Option<&HashMap<Name, Value>>, this: Option<Value>,
                     class: Option<Name>, module: Option<Name>, args: CallArgs) -> R<Value> {
        if self.frames.len() > 2500 { return rt("przepełnienie stosu wywołań"); }
        let mut vars = captured.cloned().unwrap_or_default();
        if let Some(t) = &this { vars.insert(rc("this"), t.clone()); }
        self.frames.push(Frame { vars, this, class, module });
        let saved_line = self.line;
        let r = self.bind_and_run(decl, args);
        self.frames.pop();
        if r.is_ok() { self.line = saved_line; }
        r
    }

    fn bind_and_run(&mut self, decl: &Rc<FuncDecl>, args: CallArgs) -> R<Value> {
        let fname = decl.name.as_deref().unwrap_or("{closure}").to_string();
        let mut positional: Vec<Value> = vec![];
        let mut named: HashMap<Name, Value> = HashMap::new();
        for (n, v) in args { match n { Some(n) => { named.insert(n, v); } None => positional.push(v) } }
        let mut pos = positional.into_iter();
        for p in decl.params.iter() {
            if p.variadic {
                let rest: Vec<Value> = pos.by_ref().collect();
                self.set_var(&p.name, Value::array(rest));
                continue;
            }
            let v = if let Some(v) = pos.next() { v }
                else if let Some(v) = named.remove(&p.name) { v }
                else if let Some(d) = &p.default { self.eval(d)? }
                else { return rt(format!("{}: brak argumentu ${}", fname, p.name)); };
            let v = self.check_type(&p.types, v, &format!("argument ${} funkcji {}", p.name, fname))?;
            self.set_var(&p.name, v);
        }
        if let Some((n, _)) = named.iter().next() { return rt(format!("{}: nieznany argument nazwany `{}`", fname, n)); }
        let ret = match self.exec_block(&decl.body) {
            Ok(()) => Value::Null,
            Err(Flow::Return(v)) => v,
            Err(e) => return Err(e),
        };
        if decl.ret.is_empty() || decl.ret.iter().any(|t| &**t == "void") { return Ok(ret); }
        self.check_type(&decl.ret, ret, &format!("wartość zwracana funkcji {}", fname))
    }

    fn check_type(&self, types: &[Name], v: Value, what: &str) -> R<Value> {
        if types.is_empty() { return Ok(v); }
        let mut want_float = false;
        for t in types {
            let ok = match &*t.to_ascii_lowercase() {
                "mixed" => true,
                "int" => matches!(v, Value::Int(_)),
                "float" => { want_float = true; matches!(v, Value::Float(_)) }
                "string" => matches!(v, Value::Str(_)),
                "bool" => matches!(v, Value::Bool(_)),
                "array" => matches!(v, Value::Array(_)),
                "null" => matches!(v, Value::Null),
                "object" => matches!(v, Value::Object(_)),
                "function" | "callable" => matches!(v, Value::Func(_)),
                "static" | "self" => matches!(v, Value::Object(_)),
                _ => self.value_instance_of(&v, t),
            };
            if ok { return Ok(v); }
        }
        if want_float { if let Value::Int(i) = v { return Ok(Value::Float(i as f64)); } }
        rt(format!("{}: oczekiwano {}, otrzymano {}", what, types.iter().map(|t| &**t).collect::<Vec<_>>().join("|"), v.type_name()))
    }
}

pub enum Seg { Idx(Value), Push, Prop(Name) }
pub enum Root { Var(Name), Static(Name, Name) }

fn walk(cur: &mut Value, segs: &[Seg], f: &mut dyn FnMut(&mut Value) -> R<()>) -> R<()> {
    let Some((first, rest)) = segs.split_first() else { return f(cur) };
    match first {
        Seg::Idx(_) | Seg::Push if matches!(cur, Value::Null | Value::Array(_)) => {
            if matches!(cur, Value::Null) { *cur = Value::Array(Rc::new(ZArray::new())); }
            let Value::Array(a) = cur else { unreachable!() };
            let arr = Rc::make_mut(a);
            let slot = match first { Seg::Idx(k) => arr.slot(to_key(k).map_err(Flow::Err)?), _ => arr.push_slot() };
            walk(slot, rest, f)
        }
        Seg::Idx(_) | Seg::Push => rt(format!("nie można indeksować wartości typu {} przy zapisie", cur.type_name())),
        Seg::Prop(n) => match cur {
            Value::Object(o) => {
                let o = o.clone();
                let mut b = o.borrow_mut();
                let slot = b.props.entry(n.clone()).or_insert(Value::Null);
                walk(slot, rest, f)
            }
            o => rt(format!("zapis właściwości `{}` na wartości typu {}", n, o.type_name())),
        },
    }
}

/// Minimalny odczyt sekcji "members" z zworkspace.json: pary "klucz": "wartość".
fn parse_members(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(i) = text.find("\"members\"") else { return out };
    let rest = &text[i + 9..];
    let (mut strs, mut cur, mut in_s) = (vec![], String::new(), false);
    let mut depth = 0;
    for ch in rest.chars() {
        if in_s { if ch == '"' { in_s = false; strs.push(std::mem::take(&mut cur)); } else { cur.push(ch); } continue; }
        match ch { '"' => in_s = true, '{' => depth += 1, '}' => { depth -= 1; if depth <= 0 { break; } } _ => {} }
    }
    for pair in strs.chunks(2) { if let [k, v] = pair { out.insert(k.clone(), v.clone()); } }
    out
}
