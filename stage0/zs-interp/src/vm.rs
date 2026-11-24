use crate::interp::*;
use crate::value::*;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;
use zs_parser::ast::*;

/// liczba argumentów: zwykłe (pozycyjne) albo opis z nazwami/rozwinięciem `...`
#[derive(Clone, Copy)]
enum Argc { Plain(u16), Shaped(usize) }

#[derive(Clone)]
enum SegKind { Idx, Push, Prop(Name) }

enum Op {
    Const(usize), Null, True, False,
    Load(u16), LoadQuiet(u16), StoreKeep(u16),
    Pop, Dup, Truthy,
    Bin(BinOp), Un(UnOp),
    Jmp(usize), JmpF(usize), JmpT(usize), JmpTKeep(usize), JmpNotNull(usize),
    Array(usize),
    Index, Prop(Name),
    NameRes(Name), EvalExpr(usize),
    Call(Name, Argc), CallMethod(Name, Argc), CallDot(Name, Argc), CallValue(Argc), New(Name, Argc),
    QIndex, QProp(Name), QDot(usize), LooseEq, StrictEq, MatchFail,
    Closure(usize), TryEnter(usize), TryLeave, ExcMatch(usize), Rethrow, Throw(u32),
    InstanceOf(Name),
    ModAssign(u16, usize), ModOp(BinOp, u16, usize), ModInc(u16, usize, bool, bool),
    Echo(u16), Line(u32),
    ForeachInit(u16, u16), ForeachNext { arr: u16, idx: u16, key: Option<u16>, val: u16, end: usize },
    Ret, RetNull,
}

pub struct Prog {
    ops: Vec<Op>,
    consts: Vec<Value>,
    shapes: Vec<Vec<SegKind>>,
    arrs: Vec<Vec<bool>>,
    exprs: Vec<Expr>,
    slot_names: Vec<Name>,
    argshapes: Vec<Vec<(Option<Name>, bool)>>,
    decls: Vec<Rc<FuncDecl>>,
    types: Vec<Vec<Name>>,
    handlers: Vec<usize>,
}

struct LoopCtx { breaks: Vec<usize>, conts: Vec<usize>, is_switch: bool, tries: usize }

struct Comp {
    ops: Vec<Op>, consts: Vec<Value>, shapes: Vec<Vec<SegKind>>, arrs: Vec<Vec<bool>>, exprs: Vec<Expr>,
    slots: HashMap<Name, u16>, slot_names: Vec<Name>, loops: Vec<LoopCtx>, line: u32, hidden: usize,
    argshapes: Vec<Vec<(Option<Name>, bool)>>, decls: Vec<Rc<FuncDecl>>, types: Vec<Vec<Name>>, handlers: Vec<usize>, tries: usize,
}

fn var_free(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Null | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Str(_) | ExprKind::Name(_) => true,
        ExprKind::Dot(b, _) => var_free(b),
        ExprKind::Unary(_, x) => var_free(x),
        ExprKind::Binary(_, a, b) => var_free(a) && var_free(b),
        ExprKind::Array(items) => items.iter().all(|(k, v)| k.as_ref().map_or(true, var_free) && var_free(v) && !matches!(v.kind, ExprKind::Spread(_))),
        _ => false,
    }
}

impl Comp {
    fn slot(&mut self, n: &Name) -> u16 {
        if let Some(&s) = self.slots.get(n) { return s; }
        let s = self.slot_names.len() as u16;
        self.slots.insert(n.clone(), s);
        self.slot_names.push(n.clone());
        s
    }
    fn hidden(&mut self) -> u16 {
        self.hidden += 1;
        let n: Name = Rc::from(format!("\0h{}", self.hidden).as_str());
        self.slot(&n)
    }
    fn emit(&mut self, o: Op) -> usize { self.ops.push(o); self.ops.len() - 1 }
    fn here(&self) -> usize { self.ops.len() }
    fn patch(&mut self, at: usize, to: usize) {
        match &mut self.ops[at] {
            Op::Jmp(t) | Op::JmpF(t) | Op::JmpT(t) | Op::JmpTKeep(t) | Op::JmpNotNull(t) => *t = to,
            Op::ForeachNext { end, .. } => *end = to,
            _ => unreachable!(),
        }
    }
    fn line(&mut self, l: u32) { if l != self.line && l != 0 { self.line = l; self.emit(Op::Line(l)); } }
    fn forced_line(&mut self, l: u32) { self.line = l; self.emit(Op::Line(l)); }

    fn plain_args(&mut self, args: &[Arg]) -> Option<Argc> {
        for a in args { self.expr(&a.value)?; }
        if args.iter().any(|a| a.spread || a.name.is_some()) {
            self.argshapes.push(args.iter().map(|a| (a.name.clone(), a.spread)).collect());
            Some(Argc::Shaped(self.argshapes.len() - 1))
        } else { Some(Argc::Plain(args.len() as u16)) }
    }
    fn loop_push(&mut self, is_switch: bool) { let tries = self.tries; self.loops.push(LoopCtx { breaks: vec![], conts: vec![], is_switch, tries }); }

    /// l-wartość: korzeń `$zmienna` + segmenty; wartości indeksów trafiają na stos (w kolejności), zwraca (slot, kształt)
    fn lvalue(&mut self, e: &Expr) -> Option<(u16, usize)> {
        let mut segs: Vec<SegKind> = vec![];
        let mut idx_exprs: Vec<&Expr> = vec![];
        let mut cur = e;
        let root = loop {
            match &cur.kind {
                ExprKind::Var(n) => break n.clone(),
                ExprKind::Index(b, i) => { match i { Some(i) => { segs.push(SegKind::Idx); idx_exprs.push(i); } None => segs.push(SegKind::Push) } cur = b; }
                ExprKind::Prop(b, n) => { segs.push(SegKind::Prop(n.clone())); cur = b; }
                _ => return None,
            }
        };
        segs.reverse();
        idx_exprs.reverse();
        let slot = self.slot(&root);
        for i in idx_exprs { self.expr(i)?; }
        self.shapes.push(segs);
        Some((slot, self.shapes.len() - 1))
    }

    fn expr(&mut self, e: &Expr) -> Option<()> {
        match &e.kind {
            ExprKind::Null => { self.emit(Op::Null); }
            ExprKind::Bool(b) => { self.emit(if *b { Op::True } else { Op::False }); }
            ExprKind::Int(i) => { self.consts.push(Value::Int(*i)); let c = self.consts.len() - 1; self.emit(Op::Const(c)); }
            ExprKind::Float(f) => { self.consts.push(Value::Float(*f)); let c = self.consts.len() - 1; self.emit(Op::Const(c)); }
            ExprKind::Str(s) => { self.consts.push(Value::Str(s.clone())); let c = self.consts.len() - 1; self.emit(Op::Const(c)); }
            ExprKind::Var(n) => { let s = self.slot(n); self.emit(Op::Load(s)); }
            ExprKind::Name(n) => { self.emit(Op::NameRes(n.clone())); }
            ExprKind::Array(items) => {
                let mut shape = vec![];
                for (k, v) in items {
                    if matches!(v.kind, ExprKind::Spread(_)) { return None; }
                    if let Some(k) = k { self.expr(k)?; shape.push(true); } else { shape.push(false); }
                    self.expr(v)?;
                }
                self.arrs.push(shape);
                let a = self.arrs.len() - 1;
                self.emit(Op::Array(a));
            }
            ExprKind::Index(b, Some(i)) => { self.expr(b)?; self.expr(i)?; self.emit(Op::Index); }
            ExprKind::Prop(b, n) => { self.expr(b)?; self.emit(Op::Prop(n.clone())); }
            ExprKind::Dot(_, _) => {
                if !var_free(e) { return None; }
                self.exprs.push(e.clone());
                let i = self.exprs.len() - 1;
                self.forced_line(e.line);
                self.emit(Op::EvalExpr(i));
            }
            ExprKind::Call(c, args) => {
                match &c.kind {
                    ExprKind::Name(n) => { let k = self.plain_args(args)?; self.forced_line(c.line); self.emit(Op::Call(n.clone(), k)); }
                    ExprKind::Prop(obj, m) => { self.expr(obj)?; let k = self.plain_args(args)?; self.forced_line(c.line); self.emit(Op::CallMethod(m.clone(), k)); }
                    ExprKind::Dot(base, m) => { self.expr(base)?; let k = self.plain_args(args)?; self.forced_line(c.line); self.emit(Op::CallDot(m.clone(), k)); }
                    _ => { self.expr(c)?; let k = self.plain_args(args)?; self.forced_line(e.line); self.emit(Op::CallValue(k)); }
                }
            }
            ExprKind::New(n, args) => { let k = self.plain_args(args)?; self.forced_line(e.line); self.emit(Op::New(n.clone(), k)); }
            ExprKind::Unary(op, x) => { self.expr(x)?; self.emit(Op::Un(*op)); }
            ExprKind::Binary(op, l, r) => match op {
                BinOp::And => {
                    self.expr(l)?; let j = self.emit(Op::JmpF(0)); self.expr(r)?; self.emit(Op::Truthy);
                    let end = self.emit(Op::Jmp(0)); let f = self.here(); self.patch(j, f); self.emit(Op::False); let h = self.here(); self.patch(end, h);
                }
                BinOp::Or => {
                    self.expr(l)?; let j = self.emit(Op::JmpT(0)); self.expr(r)?; self.emit(Op::Truthy);
                    let end = self.emit(Op::Jmp(0)); let t = self.here(); self.patch(j, t); self.emit(Op::True); let h = self.here(); self.patch(end, h);
                }
                BinOp::Coalesce => {
                    self.quiet(l)?; let j = self.emit(Op::JmpNotNull(0)); self.expr(r)?; let h = self.here(); self.patch(j, h);
                }
                _ => { self.expr(l)?; self.expr(r)?; self.forced_line(e.line); self.emit(Op::Bin(*op)); }
            },
            ExprKind::Assign(t, v) => {
                if let ExprKind::Var(n) = &t.kind { self.expr(v)?; let s = self.slot(n); self.emit(Op::StoreKeep(s)); }
                else { self.expr(v)?; let (s, sh) = self.lvalue(t)?; self.emit(Op::ModAssign(s, sh)); }
            }
            ExprKind::AssignOp(op, t, v) => {
                if *op == BinOp::Coalesce {
                    self.quiet(t)?; let j = self.emit(Op::JmpNotNull(0));
                    if let ExprKind::Var(n) = &t.kind { self.expr(v)?; let s = self.slot(n); self.emit(Op::StoreKeep(s)); }
                    else { self.expr(v)?; let (s, sh) = self.lvalue(t)?; self.emit(Op::ModAssign(s, sh)); }
                    let h = self.here(); self.patch(j, h);
                } else {
                    self.expr(v)?; let (s, sh) = self.lvalue(t)?; self.forced_line(e.line); self.emit(Op::ModOp(*op, s, sh));
                }
            }
            ExprKind::IncDec { target, inc, prefix } => { let (s, sh) = self.lvalue(target)?; self.emit(Op::ModInc(s, sh, *inc, *prefix)); }
            ExprKind::Ternary(c, t, f) => {
                self.expr(c)?;
                match t {
                    Some(t) => {
                        let j = self.emit(Op::JmpF(0)); self.expr(t)?; let end = self.emit(Op::Jmp(0));
                        let el = self.here(); self.patch(j, el); self.expr(f)?; let h = self.here(); self.patch(end, h);
                    }
                    None => { let j = self.emit(Op::JmpTKeep(0)); self.expr(f)?; let h = self.here(); self.patch(j, h); }
                }
            }
            ExprKind::Closure(d) => { self.decls.push(d.clone()); let i = self.decls.len() - 1; self.emit(Op::Closure(i)); }
            ExprKind::Match(subj, arms) => {
                self.expr(subj)?;
                let h = self.hidden(); self.emit(Op::StoreKeep(h)); self.emit(Op::Pop);
                let mut jumps: Vec<(usize, usize)> = vec![];
                for (ai, arm) in arms.iter().enumerate() {
                    if let Some(cs) = &arm.conds {
                        for c in cs { self.emit(Op::Load(h)); self.expr(c)?; self.emit(Op::StrictEq); jumps.push((ai, self.emit(Op::JmpT(0)))); }
                    }
                }
                let to_default = self.emit(Op::Jmp(0));
                let mut starts = vec![]; let mut ends = vec![]; let mut default_start = None;
                for (ai, arm) in arms.iter().enumerate() {
                    starts.push(self.here());
                    if arm.conds.is_none() { default_start = Some(ai); }
                    self.expr(&arm.body)?;
                    ends.push(self.emit(Op::Jmp(0)));
                }
                for (ai, j) in jumps { self.patch(j, starts[ai]); }
                match default_start {
                    Some(d) => self.patch(to_default, starts[d]),
                    None => { let f = self.here(); self.patch(to_default, f); self.emit(Op::Load(h)); self.emit(Op::MatchFail); }
                }
                let end = self.here();
                for x in ends { self.patch(x, end); }
            }
            ExprKind::InstanceOf(x, n) => { self.expr(x)?; self.emit(Op::InstanceOf(n.clone())); }
            ExprKind::Await(x) => { self.expr(x)?; }
            _ => return None,
        }
        Some(())
    }

    /// wersja „cicha” (`??`): brakująca zmienna/klucz/właściwość daje null (jak `eval_quiet`)
    fn quiet(&mut self, e: &Expr) -> Option<()> {
        match &e.kind {
            ExprKind::Var(n) => { let s = self.slot(n); self.emit(Op::LoadQuiet(s)); }
            ExprKind::Index(b, Some(i)) => {
                self.quiet(b)?; let j = self.emit(Op::JmpNotNull(0)); // baza null -> wynik null
                // JmpNotNull pomija, gdy baza != null; poniżej: baza == null już zdjęta, więc wynik null
                let skip = self.emit(Op::Null); let _ = skip; let end = self.emit(Op::Jmp(0));
                let idx_at = self.here(); self.patch(j, idx_at);
                self.expr(i)?; self.emit(Op::QIndex);
                let h = self.here(); self.patch(end, h);
            }
            ExprKind::Prop(b, n) => {
                self.quiet(b)?; let j = self.emit(Op::JmpNotNull(0));
                self.emit(Op::Null); let end = self.emit(Op::Jmp(0));
                let at = self.here(); self.patch(j, at);
                self.emit(Op::QProp(n.clone()));
                let h = self.here(); self.patch(end, h);
            }
            ExprKind::Dot(_, _) => {
                if !var_free(e) { return None; }
                self.exprs.push(e.clone()); let i = self.exprs.len() - 1; self.forced_line(e.line); self.emit(Op::QDot(i));
            }
            _ => self.expr(e)?,
        }
        Some(())
    }

    fn block(&mut self, b: &[Stmt]) -> Option<()> { for s in b { self.stmt(s)?; } Some(()) }

    fn stmt(&mut self, s: &Stmt) -> Option<()> {
        match &s.kind {
            StmtKind::Expr(e) => { self.forced_line(s.line); self.expr(e)?; self.emit(Op::Pop); }
            StmtKind::Echo(es) => { self.forced_line(s.line); for e in es { self.expr(e)?; } self.emit(Op::Echo(es.len() as u16)); }
            StmtKind::Block(b) => self.block(b)?,
            StmtKind::If(c, t, e) => {
                self.forced_line(s.line); self.expr(c)?; let j = self.emit(Op::JmpF(0)); self.block(t)?;
                match e {
                    Some(e) => { let end = self.emit(Op::Jmp(0)); let el = self.here(); self.patch(j, el); self.block(e)?; let h = self.here(); self.patch(end, h); }
                    None => { let h = self.here(); self.patch(j, h); }
                }
            }
            StmtKind::While(c, b) => {
                let top = self.here(); self.forced_line(s.line); self.expr(c)?; let j = self.emit(Op::JmpF(0));
                self.loop_push(false);
                self.block(b)?;
                self.emit(Op::Jmp(top));
                let end = self.here(); self.patch(j, end);
                let lc = self.loops.pop().unwrap();
                for x in lc.breaks { self.patch(x, end); }
                for x in lc.conts { self.patch(x, top); }
            }
            StmtKind::DoWhile(b, c) => {
                let top = self.here();
                self.loop_push(false);
                self.block(b)?;
                let cont = self.here();
                self.forced_line(s.line); self.expr(c)?; self.emit(Op::JmpT(top));
                let end = self.here();
                let lc = self.loops.pop().unwrap();
                for x in lc.breaks { self.patch(x, end); }
                for x in lc.conts { self.patch(x, cont); }
            }
            StmtKind::For(init, cond, step, b) => {
                self.forced_line(s.line);
                for e in init { self.expr(e)?; self.emit(Op::Pop); }
                let top = self.here();
                let j = match cond { Some(c) => { self.forced_line(s.line); self.expr(c)?; Some(self.emit(Op::JmpF(0))) } None => None };
                self.loop_push(false);
                self.block(b)?;
                let cont = self.here();
                for e in step { self.expr(e)?; self.emit(Op::Pop); }
                self.emit(Op::Jmp(top));
                let end = self.here();
                if let Some(j) = j { self.patch(j, end); }
                let lc = self.loops.pop().unwrap();
                for x in lc.breaks { self.patch(x, end); }
                for x in lc.conts { self.patch(x, cont); }
            }
            StmtKind::Foreach { iter, key, value, body } => {
                self.forced_line(s.line);
                self.expr(iter)?;
                let arr = self.hidden(); let idx = self.hidden();
                let kslot = key.as_ref().map(|k| self.slot(k)); let vslot = self.slot(value);
                self.emit(Op::ForeachInit(arr, idx));
                let top = self.emit(Op::ForeachNext { arr, idx, key: kslot, val: vslot, end: 0 });
                self.loop_push(false);
                self.block(body)?;
                self.emit(Op::Jmp(top));
                let end = self.here(); self.patch(top, end);
                let lc = self.loops.pop().unwrap();
                for x in lc.breaks { self.patch(x, end); }
                for x in lc.conts { self.patch(x, top); }
            }
            StmtKind::Return(e) => {
                self.forced_line(s.line);
                match e { Some(e) => { self.expr(e)?; self.emit(Op::Ret); } None => { self.emit(Op::RetNull); } }
            }
            StmtKind::Break(n) => {
                let n = *n as usize; if n == 0 || n > self.loops.len() { return None; }
                let at = self.loops.len() - n;
                for _ in self.loops[at].tries..self.tries { self.emit(Op::TryLeave); }
                let j = self.emit(Op::Jmp(0)); self.loops[at].breaks.push(j);
            }
            StmtKind::Continue(n) => {
                // `switch` jest przezroczysty dla `continue` (jak w interpreterze)
                let mut left = *n as usize; if left == 0 { return None; }
                let mut at = None;
                for i in (0..self.loops.len()).rev() { if !self.loops[i].is_switch { left -= 1; if left == 0 { at = Some(i); break; } } }
                let at = at?;
                for _ in self.loops[at].tries..self.tries { self.emit(Op::TryLeave); }
                let j = self.emit(Op::Jmp(0)); self.loops[at].conts.push(j);
            }
            StmtKind::Throw(e) => { self.forced_line(s.line); self.expr(e)?; self.emit(Op::Throw(s.line)); }
            StmtKind::Switch(subj, cases) => {
                self.forced_line(s.line); self.expr(subj)?;
                let h = self.hidden(); self.emit(Op::StoreKeep(h)); self.emit(Op::Pop);
                let mut jumps: Vec<Option<usize>> = vec![];
                for c in cases {
                    match &c.cond {
                        Some(ce) => { self.emit(Op::Load(h)); self.expr(ce)?; self.emit(Op::LooseEq); jumps.push(Some(self.emit(Op::JmpT(0)))); }
                        None => jumps.push(None),
                    }
                }
                let to_default = self.emit(Op::Jmp(0));
                self.loop_push(true);
                let mut starts = vec![];
                for c in cases { starts.push(self.here()); self.block(&c.body)?; }
                let end = self.here();
                for (i, j) in jumps.iter().enumerate() { if let Some(j) = j { self.patch(*j, starts[i]); } }
                match cases.iter().position(|c| c.cond.is_none()) { Some(d) => self.patch(to_default, starts[d]), None => self.patch(to_default, end) }
                let lc = self.loops.pop().unwrap();
                for x in lc.breaks { self.patch(x, end); }
                // `continue` wewnątrz switch celuje w pętlę zewnętrzną — nie trafia na tę listę
            }
            StmtKind::Try { body, catches, finally } => {
                if finally.is_some() { return None; }
                self.forced_line(s.line);
                let te = self.emit(Op::TryEnter(0));
                self.tries += 1;
                self.block(body)?;
                self.tries -= 1;
                self.emit(Op::TryLeave);
                let skip = self.emit(Op::Jmp(0));
                let handler = self.here();
                self.handlers.push(0);
                if let Op::TryEnter(t) = &mut self.ops[te] { *t = handler; }
                let mut ends = vec![skip];
                for c in catches {
                    self.types.push(c.types.clone());
                    let ti = self.types.len() - 1;
                    self.emit(Op::ExcMatch(ti));
                    let nomatch = self.emit(Op::JmpF(0));
                    match &c.var { Some(v) => { let sl = self.slot(v); self.emit(Op::StoreKeep(sl)); self.emit(Op::Pop); } None => { self.emit(Op::Pop); } }
                    self.block(&c.body)?;
                    ends.push(self.emit(Op::Jmp(0)));
                    let nx = self.here(); self.patch(nomatch, nx);
                }
                self.emit(Op::Rethrow);
                let end = self.here();
                for x in ends { self.patch(x, end); }
            }
            _ => return None,
        }
        Some(())
    }
}

/// Kompiluje ciało funkcji; `None` = poza podzbiorem (zostaje w interpreterze drzewiastym).
pub fn compile(decl: &FuncDecl) -> Option<Prog> {
    let mut c = Comp { ops: vec![], consts: vec![], shapes: vec![], arrs: vec![], exprs: vec![], slots: HashMap::new(), slot_names: vec![], loops: vec![], line: 0, hidden: 0, argshapes: vec![], decls: vec![], types: vec![], handlers: vec![], tries: 0 };
    for p in &decl.params { c.slot(&p.name); }
    c.block(&decl.body)?;
    c.emit(Op::RetNull);
    Some(Prog { ops: c.ops, consts: c.consts, shapes: c.shapes, arrs: c.arrs, exprs: c.exprs, slot_names: c.slot_names, argshapes: c.argshapes, decls: c.decls, types: c.types, handlers: c.handlers })
}

fn pop(st: &mut Vec<Value>) -> Value { st.pop().expect("VM: stos pusty") }

fn take_segs(p: &Prog, shape: usize, st: &mut Vec<Value>) -> Vec<Seg> {
    let kinds = &p.shapes[shape];
    let nidx = kinds.iter().filter(|k| matches!(k, SegKind::Idx)).count();
    let at = st.len() - nidx;
    let mut idxs = st.drain(at..);
    kinds.iter().map(|k| match k { SegKind::Idx => Seg::Idx(idxs.next().unwrap()), SegKind::Push => Seg::Push, SegKind::Prop(n) => Seg::Prop(n.clone()) }).collect()
}

pub fn run(it: &mut Interp, p: &Prog) -> R<Value> {
    let mut slots: Vec<Option<Value>> = Vec::with_capacity(p.slot_names.len());
    {
        let fr = it.frames.last().unwrap();
        for n in p.slot_names.iter() {
            slots.push(if !n.starts_with('\0') { fr.vars.get(n).cloned() } else { None });
        }
    }
    let mut st: Vec<Value> = Vec::with_capacity(16);
    let mut handlers: Vec<(usize, usize)> = vec![];
    let mut pc = 0usize;
    loop {
        match step(it, p, &mut slots, &mut st, &mut pc, &mut handlers) {
            Ok(None) => {}
            Ok(Some(v)) => return Ok(v),
            Err(f) => {
                // wyjątek (Throw / błąd runtime) łapany przez najbliższe `try` w tej funkcji; pozostałe przepływy lecą wyżej
                match (handlers.last().copied(), it.flow_to_exc_pub(&f)) {
                    (Some((hpc, slen)), Some(exc)) => { handlers.pop(); st.truncate(slen); st.push(exc); pc = hpc; }
                    _ => return Err(f),
                }
            }
        }
    }
}

fn take_args(it: &mut Interp, p: &Prog, st: &mut Vec<Value>, k: Argc) -> R<CallArgs> {
    match k {
        Argc::Plain(n) => { let at = st.len() - n as usize; Ok(st.drain(at..).map(|v| (None, v)).collect()) }
        Argc::Shaped(i) => {
            let shape = &p.argshapes[i];
            let at = st.len() - shape.len();
            let vals: Vec<Value> = st.drain(at..).collect();
            let mut out: CallArgs = Vec::with_capacity(vals.len());
            for ((name, spread), v) in shape.iter().zip(vals.into_iter()) {
                if *spread {
                    let Value::Array(arr) = v else { return rt("`...` wymaga tablicy") };
                    for (k, x) in arr.entries.iter() { out.push((if let Key::Str(s) = k { Some(s.clone()) } else { None }, x.clone())); }
                } else { out.push((name.clone(), v)); }
            }
            let _ = it;
            Ok(out)
        }
    }
}

#[inline(always)]
fn step(it: &mut Interp, p: &Prog, slots: &mut Vec<Option<Value>>, st: &mut Vec<Value>, pc: &mut usize, handlers: &mut Vec<(usize, usize)>) -> R<Option<Value>> {
    let op = &p.ops[*pc];
    *pc += 1;
    match op {
            Op::Const(i) => st.push(p.consts[*i].clone()),
            Op::Null => st.push(Value::Null),
            Op::True => st.push(Value::Bool(true)),
            Op::False => st.push(Value::Bool(false)),
            Op::Load(s) => match &slots[*s as usize] {
                Some(v) => st.push(v.clone()),
                None => return rt(format!("niezdefiniowana zmienna ${}", p.slot_names[*s as usize])),
            },
            Op::LoadQuiet(s) => st.push(slots[*s as usize].clone().unwrap_or(Value::Null)),
            Op::StoreKeep(s) => { let v = st.last().unwrap().clone(); slots[*s as usize] = Some(v); }
            Op::Pop => { st.pop(); }
            Op::Dup => { let v = st.last().unwrap().clone(); st.push(v); }
            Op::Truthy => { let v = pop(st); st.push(Value::Bool(v.truthy())); }
            Op::Bin(op) => {
                let b = pop(st); let a = pop(st);
                st.push(binop(*op, &a, &b).map_err(Flow::Err)?);
            }
            Op::Un(op) => {
                let v = pop(st);
                st.push(match (op, &v) {
                    (UnOp::Not, _) => Value::Bool(!v.truthy()),
                    (UnOp::Neg, Value::Int(i)) => Value::Int(i.checked_neg().ok_or_else(|| Flow::Err("przepełnienie liczby całkowitej".into()))?),
                    (UnOp::Neg, Value::Float(f)) => Value::Float(-f),
                    (UnOp::Plus, Value::Int(_) | Value::Float(_)) => v,
                    (UnOp::BitNot, Value::Int(i)) => Value::Int(!i),
                    _ => return rt(format!("operator jednoargumentowy {:?} niedostępny dla {}", op, v.type_name())),
                });
            }
            Op::Jmp(t) => *pc = *t,
            Op::JmpF(t) => { if !pop(st).truthy() { *pc = *t; } }
            Op::JmpT(t) => { if pop(st).truthy() { *pc = *t; } }
            Op::JmpTKeep(t) => { if st.last().unwrap().truthy() { *pc = *t; } else { st.pop(); } }
            Op::JmpNotNull(t) => { if !matches!(st.last().unwrap(), Value::Null) { *pc = *t; } else { st.pop(); } }
            Op::Array(a) => {
                let shape = &p.arrs[*a];
                let mut items: Vec<(Option<Value>, Value)> = Vec::with_capacity(shape.len());
                for has_key in shape.iter().rev() {
                    let v = pop(st);
                    let k = if *has_key { Some(pop(st)) } else { None };
                    items.push((k, v));
                }
                let mut arr = ZArray::new();
                for (k, v) in items.into_iter().rev() {
                    match k { Some(k) => arr.set(to_key(&k).map_err(Flow::Err)?, v), None => arr.push(v) }
                }
                st.push(Value::Array(Rc::new(arr)));
            }
            Op::Index => {
                let idx = pop(st); let base = pop(st);
                st.push(it.index_value(&base, &idx, false)?);
            }
            Op::Prop(n) => { let base = pop(st); st.push(it.get_prop(&base, n, false)?); }
            Op::NameRes(n) => st.push(it.resolve_name_pub(n)?),
            Op::EvalExpr(i) => st.push(it.eval(&p.exprs[*i])?),
            Op::Call(n, k) => { let a = take_args(it, p, st, *k)?; st.push(it.call_named(n, a)?); }
            Op::CallMethod(m, k) => { let a = take_args(it, p, st, *k)?; let o = pop(st); st.push(it.call_method(&o, m, a)?); }
            Op::CallDot(m, k) => {
                let a = take_args(it, p, st, *k)?; let b = pop(st);
                st.push(it.call_dot(b, m, a)?);
            }
            Op::CallValue(k) => { let a = take_args(it, p, st, *k)?; let f = pop(st); st.push(it.call_value(&f, a)?); }
            Op::New(n, k) => { let a = take_args(it, p, st, *k)?; st.push(it.new_object(n, a)?); }
            Op::InstanceOf(n) => { let v = pop(st); st.push(Value::Bool(it.value_instance_of(&v, n))); }
            Op::ModAssign(s, sh) => {
                let segs = take_segs(p, *sh, st);
                let val = pop(st);
                let slot = slots[*s as usize].get_or_insert(Value::Null);
                let vv = val.clone();
                walk(slot, &segs, &mut |x| { *x = vv.clone(); Ok(()) })?;
                st.push(val);
            }
            Op::ModOp(bop, s, sh) => {
                let segs = take_segs(p, *sh, st);
                let rhs = pop(st);
                let slot = slots[*s as usize].get_or_insert(Value::Null);
                let mut result = Value::Null;
                let bop = *bop;
                walk(slot, &segs, &mut |x| {
                    let cur = std::mem::replace(x, Value::Null);
                    *x = binop(bop, &cur, &rhs).map_err(Flow::Err).map_err(|f| { *x = cur; f })?;
                    result = x.clone();
                    Ok(())
                })?;
                st.push(result);
            }
            Op::ModInc(s, sh, inc, prefix) => {
                let segs = take_segs(p, *sh, st);
                let slot = slots[*s as usize].get_or_insert(Value::Null);
                let (inc, prefix) = (*inc, *prefix);
                let mut old = Value::Null; let mut new = Value::Null;
                walk(slot, &segs, &mut |x| {
                    old = x.clone();
                    let d = if inc { 1 } else { -1 };
                    *x = match &*x {
                        Value::Int(i) => Value::Int(i.checked_add(d).ok_or_else(|| Flow::Err("przepełnienie liczby całkowitej".into()))?),
                        Value::Float(f) => Value::Float(f + d as f64),
                        Value::Null => Value::Int(d),
                        o => return rt(format!("++/-- niedostępne dla {}", o.type_name())),
                    };
                    new = x.clone();
                    Ok(())
                })?;
                st.push(if prefix { new } else { old });
            }
            Op::Echo(n) => {
                let at = st.len() - *n as usize;
                let parts: Vec<String> = st.drain(at..).map(|v| to_text(&v)).collect();
                let _ = it.out.write_all(parts.join(" ").as_bytes());
                let _ = it.out.write_all(b"\n");
            }
            Op::Line(l) => it.line = *l,
            Op::ForeachInit(arr, idx) => {
                let v = pop(st);
                if !matches!(v, Value::Array(_)) { return rt(format!("foreach wymaga tablicy, jest {}", v.type_name())); }
                slots[*arr as usize] = Some(v);
                slots[*idx as usize] = Some(Value::Int(0));
            }
            Op::ForeachNext { arr, idx, key, val, end } => {
                let Some(Value::Int(i)) = &slots[*idx as usize] else { unreachable!() };
                let i = *i as usize;
                let Some(Value::Array(a)) = &slots[*arr as usize] else { unreachable!() };
                if i >= a.entries.len() { *pc = *end; return Ok(None); }
                let (k, v) = a.entries[i].clone();
                slots[*idx as usize] = Some(Value::Int(i as i64 + 1));
                if let Some(ks) = key { slots[*ks as usize] = Some(key_to_value(&k)); }
                slots[*val as usize] = Some(v);
            }
            Op::QIndex => { let idx = pop(st); let base = pop(st); st.push(it.index_value(&base, &idx, true)?); }
            Op::QProp(n) => { let base = pop(st); st.push(it.get_prop(&base, n, true)?); }
            Op::QDot(i) => { let v = match it.eval(&p.exprs[*i]) { Err(Flow::Err(_)) => Value::Null, r => r? }; st.push(v); }
            Op::LooseEq => { let b = pop(st); let a = pop(st); st.push(Value::Bool(loose_eq(&a, &b))); }
            Op::StrictEq => { let b = pop(st); let a = pop(st); st.push(Value::Bool(strict_eq(&a, &b))); }
            Op::MatchFail => { let v = pop(st); return rt(format!("brak dopasowania w match dla {}", dump(&v))); }
            Op::Closure(i) => {
                let fr = it.frames.last().unwrap();
                let mut captured = fr.vars.clone();
                for (n, sl) in p.slot_names.iter().zip(slots.iter()) { if !n.starts_with('\0') { if let Some(v) = sl { captured.insert(n.clone(), v.clone()); } } }
                st.push(Value::Func(Rc::new(Closure { decl: p.decls[*i].clone(), captured, this: fr.this.clone(), class: fr.class.clone(), module: fr.module.clone() })));
            }
            Op::TryEnter(h) => handlers.push((*h, st.len())),
            Op::TryLeave => { handlers.pop(); }
            Op::ExcMatch(ti) => { let v = st.last().unwrap().clone(); let ok = p.types[*ti].iter().any(|t| &**t == "Throwable" || it.value_instance_of(&v, t)); st.push(Value::Bool(ok)); }
            Op::Rethrow => { let v = pop(st); return Err(Flow::Throw(v)); }
            Op::Throw(line) => {
                let v = pop(st);
                if !it.value_instance_of(&v, "Throwable") { return rt("throw wymaga obiektu typu Throwable"); }
                if let Value::Object(o) = &v {
                    let mut ob = o.borrow_mut();
                    if matches!(ob.props.get("line"), Some(Value::Int(0))) { ob.props.insert(rc("line"), Value::Int(*line as i64)); }
                }
                return Err(Flow::Throw(v));
            }
            Op::Ret => return Ok(Some(pop(st))),
            Op::RetNull => return Ok(Some(Value::Null)),
    }
    Ok(None)
}
