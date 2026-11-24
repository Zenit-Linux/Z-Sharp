use crate::ast::*;
use std::rc::Rc;

#[derive(Debug)]
enum S { Node(String, u32, Vec<S>), List(Vec<S>), Str(String), Int(i64), Float(f64), Bool(bool), Null }

struct Rd<'a> { b: &'a [u8], i: usize }

impl<'a> Rd<'a> {
    fn ws(&mut self) { while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() { self.i += 1; } }
    fn atom(&mut self) -> &'a str {
        let st = self.i;
        while self.i < self.b.len() && !matches!(self.b[self.i], b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')' | b'[' | b']') { self.i += 1; }
        std::str::from_utf8(&self.b[st..self.i]).unwrap_or("")
    }
    fn parse(&mut self) -> Result<S, String> {
        self.ws();
        let Some(&c) = self.b.get(self.i) else { return Err("nieoczekiwany koniec danych".into()) };
        match c {
            b'(' => {
                self.i += 1;
                let head = self.atom();
                let (kind, line) = head.split_once('@').ok_or_else(|| format!("węzeł bez `@linia`: {head}"))?;
                let line: u32 = line.parse().map_err(|_| format!("zła linia w {head}"))?;
                let mut kids = vec![];
                loop {
                    self.ws();
                    match self.b.get(self.i) { Some(b')') => { self.i += 1; break; } None => return Err("brak `)`".into()), _ => kids.push(self.parse()?) }
                }
                Ok(S::Node(kind.to_string(), line, kids))
            }
            b'[' => {
                self.i += 1;
                let mut v = vec![];
                loop {
                    self.ws();
                    match self.b.get(self.i) { Some(b']') => { self.i += 1; break; } None => return Err("brak `]`".into()), _ => v.push(self.parse()?) }
                }
                Ok(S::List(v))
            }
            b'"' => {
                self.i += 1;
                let mut out: Vec<u8> = vec![];
                loop {
                    let Some(&ch) = self.b.get(self.i) else { return Err("niezamknięty napis".into()) };
                    self.i += 1;
                    match ch {
                        b'"' => break,
                        b'\\' => { let e = *self.b.get(self.i).ok_or("ucięta ucieczka")?; self.i += 1;
                            out.push(match e { b'n' => b'\n', b't' => b'\t', b'r' => b'\r', b'\\' => b'\\', b'"' => b'"', o => return Err(format!("zła ucieczka \\{}", o as char)) }) }
                        o => out.push(o),
                    }
                }
                String::from_utf8(out).map(S::Str).map_err(|_| "napis nie jest UTF-8".to_string())
            }
            _ => {
                let a = self.atom();
                Ok(match a {
                    "_" => S::Null, "true" => S::Bool(true), "false" => S::Bool(false),
                    _ => if let Ok(i) = a.parse::<i64>() { S::Int(i) } else if let Ok(f) = a.parse::<f64>() { S::Float(f) } else { return Err(format!("nierozpoznany atom `{a}`")) },
                })
            }
        }
    }
}

type R<T> = Result<T, String>;
fn rc(s: &str) -> Name { Rc::from(s) }
fn node<'a>(s: &'a S, want: &str) -> R<(u32, &'a [S])> {
    match s { S::Node(k, l, v) if k == want => Ok((*l, v)), S::Node(k, ..) => Err(format!("oczekiwano {want}, jest {k}")), _ => Err(format!("oczekiwano węzła {want}")) }
}
fn list(s: &S) -> R<&[S]> { if let S::List(v) = s { Ok(v) } else { Err("oczekiwano listy".into()) } }
fn name(s: &S) -> R<Name> { if let S::Str(x) = s { Ok(rc(x)) } else { Err("oczekiwano napisu".into()) } }
fn opt_name(s: &S) -> R<Option<Name>> { match s { S::Null => Ok(None), o => name(o).map(Some) } }
fn boolean(s: &S) -> R<bool> { if let S::Bool(b) = s { Ok(*b) } else { Err("oczekiwano bool".into()) } }
fn names(s: &S) -> R<Vec<Name>> { list(s)?.iter().map(name).collect() }
fn at<'a>(v: &'a [S], i: usize) -> R<&'a S> { v.get(i).ok_or_else(|| format!("brakuje pola {i}")) }

fn binop(n: &str) -> R<BinOp> {
    use BinOp::*;
    Ok(match n { "Add" => Add, "Sub" => Sub, "Mul" => Mul, "Div" => Div, "Mod" => Mod, "Pow" => Pow, "Concat" => Concat, "Eq" => Eq, "NotEq" => NotEq,
        "Identical" => Identical, "NotIdentical" => NotIdentical, "Lt" => Lt, "Gt" => Gt, "Le" => Le, "Ge" => Ge, "And" => And, "Or" => Or, "Xor" => Xor,
        "Coalesce" => Coalesce, "BitAnd" => BitAnd, "BitOr" => BitOr, "BitXor" => BitXor, "Shl" => Shl, "Shr" => Shr, _ => return Err(format!("zły operator {n}")) })
}

fn stmts(s: &S) -> R<Vec<Stmt>> { list(s)?.iter().map(stmt).collect() }
fn opt_expr(s: &S) -> R<Option<Expr>> { match s { S::Null => Ok(None), o => expr(o).map(Some) } }
fn exprs(s: &S) -> R<Vec<Expr>> { list(s)?.iter().map(expr).collect() }

fn param(s: &S) -> R<Param> {
    let (_, f) = node(s, "Param")?;
    Ok(Param { name: name(at(f, 0)?)?, types: names(at(f, 1)?)?, default: opt_expr(at(f, 2)?)?, variadic: boolean(at(f, 3)?)? })
}
fn params(s: &S) -> R<Vec<Param>> { list(s)?.iter().map(param).collect() }
fn func(s: &S) -> R<Rc<FuncDecl>> {
    let (line, f) = node(s, "Func")?;
    Ok(Rc::new(FuncDecl { name: opt_name(at(f, 0)?)?, params: params(at(f, 1)?)?, ret: names(at(f, 2)?)?, body: stmts(at(f, 3)?)?, is_arrow: boolean(at(f, 4)?)?, line }))
}
fn args(s: &S) -> R<Vec<Arg>> {
    list(s)?.iter().map(|a| { let (_, f) = node(a, "Arg")?; Ok(Arg { name: opt_name(at(f, 0)?)?, value: expr(at(f, 1)?)?, spread: boolean(at(f, 2)?)? }) }).collect()
}
fn bx(s: &S) -> R<Box<Expr>> { expr(s).map(Box::new) }

fn expr(s: &S) -> R<Expr> {
    let S::Node(k, line, f) = s else { return Err("oczekiwano wyrażenia".into()) };
    let line = *line;
    let kind = match k.as_str() {
        "Null" => ExprKind::Null,
        "Bool" => ExprKind::Bool(boolean(at(f, 0)?)?),
        "Int" => match at(f, 0)? { S::Int(i) => ExprKind::Int(*i), _ => return Err("Int".into()) },
        "Float" => match at(f, 0)? { S::Float(x) => ExprKind::Float(*x), S::Int(i) => ExprKind::Float(*i as f64), _ => return Err("Float".into()) },
        "Str" => ExprKind::Str(name(at(f, 0)?)?),
        "Var" => ExprKind::Var(name(at(f, 0)?)?),
        "Name" => ExprKind::Name(name(at(f, 0)?)?),
        "Array" => ExprKind::Array(list(at(f, 0)?)?.iter().map(|i| { let (_, g) = node(i, "Item")?; Ok((opt_expr(at(g, 0)?)?, expr(at(g, 1)?)?)) }).collect::<R<Vec<_>>>()?),
        "Spread" => ExprKind::Spread(bx(at(f, 0)?)?),
        "Index" => ExprKind::Index(bx(at(f, 0)?)?, match at(f, 1)? { S::Null => None, o => Some(bx(o)?) }),
        "Prop" => ExprKind::Prop(bx(at(f, 0)?)?, name(at(f, 1)?)?),
        "Dot" => ExprKind::Dot(bx(at(f, 0)?)?, name(at(f, 1)?)?),
        "Call" => ExprKind::Call(bx(at(f, 0)?)?, args(at(f, 1)?)?),
        "New" => ExprKind::New(name(at(f, 0)?)?, args(at(f, 1)?)?),
        "Unary" => ExprKind::Unary(match name(at(f, 0)?)?.as_ref() { "Neg" => UnOp::Neg, "Plus" => UnOp::Plus, "Not" => UnOp::Not, "BitNot" => UnOp::BitNot, o => return Err(format!("zły Unary {o}")) }, bx(at(f, 1)?)?),
        "Binary" => ExprKind::Binary(binop(&name(at(f, 0)?)?)?, bx(at(f, 1)?)?, bx(at(f, 2)?)?),
        "Assign" => ExprKind::Assign(bx(at(f, 0)?)?, bx(at(f, 1)?)?),
        "AssignOp" => ExprKind::AssignOp(binop(&name(at(f, 0)?)?)?, bx(at(f, 1)?)?, bx(at(f, 2)?)?),
        "IncDec" => ExprKind::IncDec { target: bx(at(f, 0)?)?, inc: boolean(at(f, 1)?)?, prefix: boolean(at(f, 2)?)? },
        "Ternary" => ExprKind::Ternary(bx(at(f, 0)?)?, match at(f, 1)? { S::Null => None, o => Some(bx(o)?) }, bx(at(f, 2)?)?),
        "Closure" => ExprKind::Closure(func(at(f, 0)?)?),
        "Match" => ExprKind::Match(bx(at(f, 0)?)?, list(at(f, 1)?)?.iter().map(|a| { let (_, g) = node(a, "Arm")?;
            Ok(MatchArm { conds: match at(g, 0)? { S::Null => None, o => Some(exprs(o)?) }, body: expr(at(g, 1)?)? }) }).collect::<R<Vec<_>>>()?),
        "Await" => ExprKind::Await(bx(at(f, 0)?)?),
        "InstanceOf" => ExprKind::InstanceOf(bx(at(f, 0)?)?, name(at(f, 1)?)?),
        o => return Err(format!("nieznane wyrażenie {o}")),
    };
    Ok(Expr { kind, line })
}

fn stmt(s: &S) -> R<Stmt> {
    let S::Node(k, line, f) = s else { return Err("oczekiwano instrukcji".into()) };
    let line = *line;
    let kind = match k.as_str() {
        "Expr" => StmtKind::Expr(expr(at(f, 0)?)?),
        "Echo" => StmtKind::Echo(exprs(at(f, 0)?)?),
        "Block" => StmtKind::Block(stmts(at(f, 0)?)?),
        "If" => StmtKind::If(expr(at(f, 0)?)?, stmts(at(f, 1)?)?, match at(f, 2)? { S::Null => None, o => Some(stmts(o)?) }),
        "While" => StmtKind::While(expr(at(f, 0)?)?, stmts(at(f, 1)?)?),
        "DoWhile" => StmtKind::DoWhile(stmts(at(f, 0)?)?, expr(at(f, 1)?)?),
        "For" => StmtKind::For(exprs(at(f, 0)?)?, opt_expr(at(f, 1)?)?, exprs(at(f, 2)?)?, stmts(at(f, 3)?)?),
        "Foreach" => StmtKind::Foreach { iter: expr(at(f, 0)?)?, key: opt_name(at(f, 1)?)?, value: name(at(f, 2)?)?, body: stmts(at(f, 3)?)? },
        "Switch" => StmtKind::Switch(expr(at(f, 0)?)?, list(at(f, 1)?)?.iter().map(|c| { let (_, g) = node(c, "Case")?;
            Ok(SwitchCase { cond: opt_expr(at(g, 0)?)?, body: stmts(at(g, 1)?)? }) }).collect::<R<Vec<_>>>()?),
        "Return" => StmtKind::Return(opt_expr(at(f, 0)?)?),
        "Break" => StmtKind::Break(match at(f, 0)? { S::Int(i) => *i as u32, _ => return Err("Break".into()) }),
        "Continue" => StmtKind::Continue(match at(f, 0)? { S::Int(i) => *i as u32, _ => return Err("Continue".into()) }),
        "Throw" => StmtKind::Throw(expr(at(f, 0)?)?),
        "Try" => StmtKind::Try { body: stmts(at(f, 0)?)?, catches: list(at(f, 1)?)?.iter().map(|c| { let (_, g) = node(c, "Catch")?;
            Ok(CatchClause { types: names(at(g, 0)?)?, var: opt_name(at(g, 1)?)?, body: stmts(at(g, 2)?)? }) }).collect::<R<Vec<_>>>()?,
            finally: match at(f, 2)? { S::Null => None, o => Some(stmts(o)?) } },
        "FuncStmt" => StmtKind::Func(func(at(f, 0)?)?),
        "ClassStmt" => {
            let (_, c) = node(at(f, 0)?, "Class")?;
            StmtKind::Class(Rc::new(ClassDecl {
                name: name(at(c, 0)?)?, parent: opt_name(at(c, 1)?)?, interfaces: names(at(c, 2)?)?,
                is_interface: boolean(at(c, 3)?)?, is_abstract: boolean(at(c, 4)?)?,
                props: list(at(c, 5)?)?.iter().map(|p| { let (_, g) = node(p, "Prop")?; Ok(PropDecl { name: name(at(g, 0)?)?, default: opt_expr(at(g, 1)?)?, is_static: boolean(at(g, 2)?)? }) }).collect::<R<Vec<_>>>()?,
                consts: list(at(c, 6)?)?.iter().map(|p| { let (_, g) = node(p, "Const")?; Ok((name(at(g, 0)?)?, expr(at(g, 1)?)?)) }).collect::<R<Vec<_>>>()?,
                methods: list(at(c, 7)?)?.iter().map(|p| { let (_, g) = node(p, "Method")?; Ok(MethodDecl { decl: func(at(g, 0)?)?, is_static: boolean(at(g, 1)?)?, is_abstract: boolean(at(g, 2)?)? }) }).collect::<R<Vec<_>>>()?,
            }))
        }
        "Import" => StmtKind::Import(names(at(f, 0)?)?),
        "Namespace" => StmtKind::Namespace(names(at(f, 0)?)?),
        "Using" => StmtKind::Using { resource: expr(at(f, 0)?)?, var: name(at(f, 1)?)?, body: stmts(at(f, 2)?)? },
        "Extern" => StmtKind::Extern { lib: name(at(f, 0)?)?, items: list(at(f, 1)?)?.iter().map(|i| { let (_, g) = node(i, "ExternItem")?;
            Ok(ExternItem { name: name(at(g, 0)?)?, params: params(at(g, 1)?)?, ret: names(at(g, 2)?)? }) }).collect::<R<Vec<_>>>()? },
        o => return Err(format!("nieznana instrukcja {o}")),
    };
    Ok(Stmt { kind, line })
}

/// Jedna instrukcja z tekstu S-expression.
pub fn stmt_from_sexp(text: &str) -> R<Stmt> {
    let mut r = Rd { b: text.as_bytes(), i: 0 };
    stmt(&r.parse()?)
}

pub struct Zbc { pub entry: Vec<Stmt>, pub modules: Vec<(String, Vec<Stmt>)> }

/// Parsuje kontener `.zbc` (nagłówek `ZBC 1`, sekcje `@@module <klucz>` i `@@entry`).
pub fn parse_zbc(text: &str) -> R<Zbc> {
    let mut lines = text.lines();
    match lines.next() { Some("ZBC 1") => {}, Some(o) => return Err(format!("nieobsługiwany nagłówek `{o}` (oczekiwano `ZBC 1`)")), None => return Err("pusty plik .zbc".into()) }
    let mut modules: Vec<(String, Vec<Stmt>)> = vec![];
    let mut entry: Option<Vec<Stmt>> = None;
    let mut cur: Option<(Option<String>, Vec<Stmt>)> = None;
    let mut flush = |cur: &mut Option<(Option<String>, Vec<Stmt>)>, modules: &mut Vec<(String, Vec<Stmt>)>, entry: &mut Option<Vec<Stmt>>| {
        if let Some((k, v)) = cur.take() { match k { Some(k) => modules.push((k, v)), None => *entry = Some(v) } }
    };
    for (n, l) in lines.enumerate() {
        if l.is_empty() { continue; }
        if let Some(m) = l.strip_prefix("@@module ") { flush(&mut cur, &mut modules, &mut entry); cur = Some((Some(m.to_string()), vec![])); }
        else if l == "@@entry" { flush(&mut cur, &mut modules, &mut entry); cur = Some((None, vec![])); }
        else if let Some((_, v)) = cur.as_mut() { v.push(stmt_from_sexp(l).map_err(|e| format!("linia {}: {e}", n + 2))?); }
        else { return Err(format!("linia {}: instrukcja poza sekcją", n + 2)); }
    }
    flush(&mut cur, &mut modules, &mut entry);
    Ok(Zbc { entry: entry.ok_or("brak sekcji @@entry")?, modules })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dump, parse};

    #[test]
    fn roundtrip_all_repo_sources() {
        let srcs = [include_str!("../../../tests/basics.zsharp"), include_str!("../../../examples/hello.zsharp"), include_str!("../../../lib/cli.zsharp"),
                    include_str!("../../../lib/lexer.zsharp"), include_str!("../../../lib/parser.zsharp")];
        for s in srcs {
            let ast = parse(s).unwrap();
            for st in &ast {
                let text = dump::stmt(st);
                let back = stmt_from_sexp(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
                assert_eq!(dump::stmt(&back), text);
            }
        }
    }
    #[test]
    fn container() {
        let z = parse_zbc("ZBC 1\n@@module A.b\n(Import@1 [\"System\"])\n@@entry\n(Expr@2 (Int@2 1))\n").unwrap();
        assert_eq!(z.modules.len(), 1); assert_eq!(z.modules[0].0, "A.b"); assert_eq!(z.entry.len(), 1);
        assert!(parse_zbc("ZBC 2\n").is_err()); assert!(parse_zbc("ZBC 1\n@@module x\n").is_err());
    }
    #[test]
    fn rejects_garbage() { assert!(stmt_from_sexp("(Nope@1)").is_err()); assert!(stmt_from_sexp("(Expr@1 (Int@1 5)").is_err()); }
}
