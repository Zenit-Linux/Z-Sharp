use crate::ast::*;

fn esc(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c { '\\' => o.push_str("\\\\"), '"' => o.push_str("\\\""), '\n' => o.push_str("\\n"), '\t' => o.push_str("\\t"), '\r' => o.push_str("\\r"), c => o.push(c) }
    }
    o.push('"');
    o
}
fn float(f: f64) -> String { let s = format!("{}", f); if s.contains('.') || s.contains('e') || !f.is_finite() { s } else { format!("{s}.0") } }
fn list<T>(v: &[T], f: impl Fn(&T) -> String) -> String { format!("[{}]", v.iter().map(f).collect::<Vec<_>>().join(" ")) }
fn opt<T>(o: &Option<T>, f: impl Fn(&T) -> String) -> String { match o { Some(x) => f(x), None => "_".into() } }
fn names(v: &[Name]) -> String { list(v, |n| esc(n)) }
fn b(x: bool) -> &'static str { if x { "true" } else { "false" } }
fn n(kind: &str, line: u32, parts: &[String]) -> String {
    let mut s = format!("({kind}@{line}");
    for p in parts { s.push(' '); s.push_str(p); }
    s.push(')');
    s
}

pub fn stmts(v: &[Stmt]) -> Vec<String> { v.iter().map(stmt).collect() }
fn block(v: &[Stmt]) -> String { list(v, stmt) }

pub fn stmt(s: &Stmt) -> String {
    let l = s.line;
    match &s.kind {
        StmtKind::Expr(e) => n("Expr", l, &[expr(e)]),
        StmtKind::Echo(v) => n("Echo", l, &[list(v, expr)]),
        StmtKind::Block(v) => n("Block", l, &[block(v)]),
        StmtKind::If(c, t, e) => n("If", l, &[expr(c), block(t), opt(e, |x| block(x))]),
        StmtKind::While(c, bd) => n("While", l, &[expr(c), block(bd)]),
        StmtKind::DoWhile(bd, c) => n("DoWhile", l, &[block(bd), expr(c)]),
        StmtKind::For(i, c, st, bd) => n("For", l, &[list(i, expr), opt(c, expr), list(st, expr), block(bd)]),
        StmtKind::Foreach { iter, key, value, body } => n("Foreach", l, &[expr(iter), opt(key, |k| esc(k)), esc(value), block(body)]),
        StmtKind::Switch(e, cs) => n("Switch", l, &[expr(e), list(cs, |c| n("Case", 0, &[opt(&c.cond, expr), block(&c.body)]))]),
        StmtKind::Return(e) => n("Return", l, &[opt(e, expr)]),
        StmtKind::Break(k) => n("Break", l, &[k.to_string()]),
        StmtKind::Continue(k) => n("Continue", l, &[k.to_string()]),
        StmtKind::Throw(e) => n("Throw", l, &[expr(e)]),
        StmtKind::Try { body, catches, finally } => n("Try", l, &[block(body),
            list(catches, |c| n("Catch", 0, &[names(&c.types), opt(&c.var, |v| esc(v)), block(&c.body)])), opt(finally, |f| block(f))]),
        StmtKind::Func(d) => n("FuncStmt", l, &[func(d)]),
        StmtKind::Class(c) => n("ClassStmt", l, &[class(c, l)]),
        StmtKind::Import(p) => n("Import", l, &[names(p)]),
        StmtKind::Namespace(p) => n("Namespace", l, &[names(p)]),
        StmtKind::Using { resource, var, body } => n("Using", l, &[expr(resource), esc(var), block(body)]),
        StmtKind::Extern { lib, items } => n("Extern", l, &[esc(lib), list(items, |i| n("ExternItem", 0, &[esc(&i.name), list(&i.params, param), names(&i.ret)]))]),
    }
}

fn param(p: &Param) -> String { n("Param", 0, &[esc(&p.name), names(&p.types), opt(&p.default, expr), b(p.variadic).into()]) }
fn func(d: &FuncDecl) -> String {
    n("Func", d.line, &[opt(&d.name, |x| esc(x)), list(&d.params, param), names(&d.ret), block(&d.body), b(d.is_arrow).into()])
}
fn class(c: &ClassDecl, line: u32) -> String {
    n("Class", line, &[esc(&c.name), opt(&c.parent, |p| esc(p)), names(&c.interfaces), b(c.is_interface).into(), b(c.is_abstract).into(),
        list(&c.props, |p| n("Prop", 0, &[esc(&p.name), opt(&p.default, expr), b(p.is_static).into()])),
        list(&c.consts, |(k, e)| n("Const", 0, &[esc(k), expr(e)])),
        list(&c.methods, |m| n("Method", 0, &[func(&m.decl), b(m.is_static).into(), b(m.is_abstract).into()]))])
}
fn arg(a: &Arg) -> String { n("Arg", 0, &[opt(&a.name, |x| esc(x)), expr(&a.value), b(a.spread).into()]) }

pub fn expr(e: &Expr) -> String {
    let l = e.line;
    let bx = |x: &Box<Expr>| expr(x);
    match &e.kind {
        ExprKind::Null => n("Null", l, &[]),
        ExprKind::Bool(v) => n("Bool", l, &[b(*v).into()]),
        ExprKind::Int(v) => n("Int", l, &[v.to_string()]),
        ExprKind::Float(v) => n("Float", l, &[float(*v)]),
        ExprKind::Str(s) => n("Str", l, &[esc(s)]),
        ExprKind::Var(s) => n("Var", l, &[esc(s)]),
        ExprKind::Name(s) => n("Name", l, &[esc(s)]),
        ExprKind::Array(items) => n("Array", l, &[list(items, |(k, v)| n("Item", 0, &[opt(k, expr), expr(v)]))]),
        ExprKind::Index(t, i) => n("Index", l, &[bx(t), opt(i, |x| expr(x))]),
        ExprKind::Prop(t, nm) => n("Prop", l, &[bx(t), esc(nm)]),
        ExprKind::Dot(t, nm) => n("Dot", l, &[bx(t), esc(nm)]),
        ExprKind::Call(f, a) => n("Call", l, &[bx(f), list(a, arg)]),
        ExprKind::New(c, a) => n("New", l, &[esc(c), list(a, arg)]),
        ExprKind::Unary(op, x) => n("Unary", l, &[format!("{:?}", op).replace("\"", "").pipe_quote(), bx(x)]),
        ExprKind::Binary(op, x, y) => n("Binary", l, &[esc(&format!("{:?}", op)), bx(x), bx(y)]),
        ExprKind::Assign(x, y) => n("Assign", l, &[bx(x), bx(y)]),
        ExprKind::AssignOp(op, x, y) => n("AssignOp", l, &[esc(&format!("{:?}", op)), bx(x), bx(y)]),
        ExprKind::IncDec { target, inc, prefix } => n("IncDec", l, &[bx(target), b(*inc).into(), b(*prefix).into()]),
        ExprKind::Ternary(c, t, f) => n("Ternary", l, &[bx(c), opt(t, |x| expr(x)), bx(f)]),
        ExprKind::Closure(d) => n("Closure", l, &[func(d)]),
        ExprKind::Match(s, arms) => n("Match", l, &[bx(s), list(arms, |a| n("Arm", 0, &[opt(&a.conds, |c| list(c, expr)), expr(&a.body)]))]),
        ExprKind::Await(x) => n("Await", l, &[bx(x)]),
        ExprKind::InstanceOf(x, c) => n("InstanceOf", l, &[bx(x), esc(c)]),
    }
}

trait PipeQuote { fn pipe_quote(self) -> String; }
impl PipeQuote for String { fn pipe_quote(self) -> String { esc(&self) } }
