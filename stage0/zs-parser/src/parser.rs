use crate::ast::*;
use std::rc::Rc;
use zs_lexer::{Keyword as K, Lexer, Op as O, Token, TokenKind as T};

#[derive(Debug, Clone)]
pub struct ParseError { pub message: String, pub line: u32, pub col: u32 }

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

type PResult<T> = Result<T, ParseError>;

pub fn parse(src: &str) -> PResult<Vec<Stmt>> {
    let toks = Lexer::new(src).tokenize().map_err(|e| ParseError {
        message: e.message, line: e.span.line, col: e.span.col })?;
    let toks: Vec<Token> = toks.into_iter()
        .filter(|t| !matches!(t.kind, T::Shebang(_) | T::DocComment(_))).collect();
    let mut p = Parser { toks, pos: 0 };
    let mut out = Vec::new();
    while !p.at_eof() { out.push(p.stmt()?); }
    Ok(out)
}

struct Parser { toks: Vec<Token>, pos: usize }

fn rc(s: &str) -> Name { Rc::from(s) }

impl Parser {
    // ---------- narzędzia ----------
    fn peek(&self) -> &T { &self.toks[self.pos.min(self.toks.len() - 1)].kind }
    fn peek_n(&self, n: usize) -> &T { &self.toks[(self.pos + n).min(self.toks.len() - 1)].kind }
    fn line(&self) -> u32 { self.toks[self.pos.min(self.toks.len() - 1)].span.line }
    fn at_eof(&self) -> bool { matches!(self.peek(), T::Eof) }
    fn bump(&mut self) -> T { let t = self.peek().clone(); if self.pos < self.toks.len() - 1 { self.pos += 1; } t }
    fn err<X>(&self, msg: impl Into<String>) -> PResult<X> {
        let s = self.toks[self.pos.min(self.toks.len() - 1)].span;
        Err(ParseError { message: msg.into(), line: s.line, col: s.col })
    }
    fn is_op(&self, o: O) -> bool { matches!(self.peek(), T::Op(x) if *x == o) }
    fn is_kw(&self, k: K) -> bool { matches!(self.peek(), T::Keyword(x) if *x == k) }
    fn is_word(&self, w: &str) -> bool { matches!(self.peek(), T::Ident(x) if &**x == w) }
    fn eat_op(&mut self, o: O) -> bool { if self.is_op(o) { self.bump(); true } else { false } }
    fn eat_kw(&mut self, k: K) -> bool { if self.is_kw(k) { self.bump(); true } else { false } }
    fn eat_word(&mut self, w: &str) -> bool { if self.is_word(w) { self.bump(); true } else { false } }
    fn expect_op(&mut self, o: O) -> PResult<()> {
        if self.eat_op(o) { Ok(()) } else { self.err(format!("oczekiwano {:?}, jest {:?}", o, self.peek())) }
    }
    fn ident(&mut self) -> PResult<Name> {
        match self.bump() { T::Ident(s) => Ok(rc(&s)), t => { self.pos -= 1; let _ = t; self.err(format!("oczekiwano identyfikatora, jest {:?}", self.peek())) } }
    }
    fn is_func_kw(&self) -> bool { self.is_kw(K::Func) || self.is_word("function") }

    // ---------- instrukcje ----------
    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect_op(O::LBrace)?;
        let mut v = Vec::new();
        while !self.is_op(O::RBrace) {
            if self.at_eof() { return self.err("niezamknięty blok `{`"); }
            v.push(self.stmt()?);
        }
        self.bump();
        Ok(v)
    }

    fn body(&mut self) -> PResult<Vec<Stmt>> {
        if self.is_op(O::LBrace) { self.block() } else { Ok(vec![self.stmt()?]) }
    }

    fn paren_expr(&mut self) -> PResult<Expr> {
        self.expect_op(O::LParen)?;
        let e = self.expr()?;
        self.expect_op(O::RParen)?;
        Ok(e)
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let line = self.line();
        let kind = match self.peek().clone() {
            T::Op(O::LBrace) => StmtKind::Block(self.block()?),
            T::Op(O::Semi) => { self.bump(); StmtKind::Block(vec![]) }
            T::Keyword(K::If) => { self.bump(); return self.if_rest(line); }
            T::Keyword(K::While) => {
                self.bump(); let c = self.paren_expr()?; let b = self.body()?; StmtKind::While(c, b)
            }
            T::Keyword(K::Do) => {
                self.bump(); let b = self.body()?;
                if !self.eat_kw(K::While) { return self.err("oczekiwano `while` po `do`"); }
                let c = self.paren_expr()?; self.eat_op(O::Semi); StmtKind::DoWhile(b, c)
            }
            T::Keyword(K::For) => self.for_stmt()?,
            T::Keyword(K::Foreach) => self.foreach_stmt()?,
            T::Keyword(K::Switch) => self.switch_stmt()?,
            T::Keyword(K::Return) => {
                self.bump();
                let e = if self.is_op(O::Semi) { None } else { Some(self.expr()?) };
                self.expect_op(O::Semi)?; StmtKind::Return(e)
            }
            T::Keyword(K::Break) | T::Keyword(K::Continue) => {
                let brk = self.is_kw(K::Break); self.bump();
                let n = if let T::Int(n) = self.peek().clone() { self.bump(); n as u32 } else { 1 };
                self.expect_op(O::Semi)?;
                if brk { StmtKind::Break(n) } else { StmtKind::Continue(n) }
            }
            T::Keyword(K::Throw) => { self.bump(); let e = self.expr()?; self.expect_op(O::Semi)?; StmtKind::Throw(e) }
            T::Keyword(K::Try) => self.try_stmt()?,
            T::Keyword(K::Echo) => {
                self.bump();
                let mut v = vec![self.expr()?];
                while self.eat_op(O::Comma) { v.push(self.expr()?); }
                self.expect_op(O::Semi)?; StmtKind::Echo(v)
            }
            T::Keyword(K::Import) => {
                self.bump(); let p = self.path()?; self.expect_op(O::Semi)?; StmtKind::Import(p)
            }
            T::Keyword(K::Namespace) => {
                self.bump(); let p = self.path()?; self.expect_op(O::Semi)?; StmtKind::Namespace(p)
            }
            T::Keyword(K::Using) => {
                self.bump();
                let r = self.expr()?;
                if !self.eat_kw(K::As) { return self.err("oczekiwano `as` w `using`"); }
                let v = match self.bump() { T::Variable(v) => rc(&v), _ => { self.pos -= 1; return self.err("oczekiwano zmiennej po `as`"); } };
                let b = self.block()?; StmtKind::Using { resource: r, var: v, body: b }
            }
            T::Keyword(K::Extern) => self.extern_stmt()?,
            T::Keyword(K::Func) | T::Ident(_) if self.is_func_kw() && matches!(self.peek_n(1), T::Ident(_)) => {
                self.bump(); let d = self.func_rest(true)?; StmtKind::Func(Rc::new(d))
            }
            T::Keyword(K::Class) | T::Keyword(K::Interface) | T::Keyword(K::Abstract) => self.class_decl()?,
            _ => { let e = self.expr()?; self.expect_op(O::Semi)?; StmtKind::Expr(e) }
        };
        Ok(Stmt { kind, line })
    }

    fn path(&mut self) -> PResult<Vec<Name>> {
        let mut v = vec![self.ident()?];
        while self.eat_op(O::Dot) { v.push(self.ident()?); }
        Ok(v)
    }

    fn if_rest(&mut self, line: u32) -> PResult<Stmt> {
        let c = self.paren_expr()?;
        let then = self.body()?;
        let els = if self.is_kw(K::Elseif) {
            let l = self.line(); self.bump(); Some(vec![self.if_rest(l)?])
        } else if self.is_kw(K::Else) {
            self.bump();
            if self.is_kw(K::If) { let l = self.line(); self.bump(); Some(vec![self.if_rest(l)?]) }
            else { Some(self.body()?) }
        } else { None };
        Ok(Stmt { kind: StmtKind::If(c, then, els), line })
    }

    fn for_stmt(&mut self) -> PResult<StmtKind> {
        self.bump(); self.expect_op(O::LParen)?;
        let mut init = vec![];
        if !self.is_op(O::Semi) { init.push(self.expr()?); while self.eat_op(O::Comma) { init.push(self.expr()?); } }
        self.expect_op(O::Semi)?;
        let cond = if self.is_op(O::Semi) { None } else { Some(self.expr()?) };
        self.expect_op(O::Semi)?;
        let mut step = vec![];
        if !self.is_op(O::RParen) { step.push(self.expr()?); while self.eat_op(O::Comma) { step.push(self.expr()?); } }
        self.expect_op(O::RParen)?;
        let b = self.body()?;
        Ok(StmtKind::For(init, cond, step, b))
    }

    fn var_name(&mut self) -> PResult<Name> {
        match self.bump() { T::Variable(v) => Ok(rc(&v)), _ => { self.pos -= 1; self.err("oczekiwano zmiennej") } }
    }

    fn foreach_stmt(&mut self) -> PResult<StmtKind> {
        self.bump(); self.expect_op(O::LParen)?;
        let iter = self.expr()?;
        if !self.eat_kw(K::As) { return self.err("oczekiwano `as` w foreach"); }
        let first = self.var_name()?;
        let (key, value) = if self.eat_op(O::FatArrow) { (Some(first), self.var_name()?) } else { (None, first) };
        self.expect_op(O::RParen)?;
        let body = self.body()?;
        Ok(StmtKind::Foreach { iter, key, value, body })
    }

    fn switch_stmt(&mut self) -> PResult<StmtKind> {
        self.bump();
        let subject = self.paren_expr()?;
        self.expect_op(O::LBrace)?;
        let mut cases = vec![];
        while !self.is_op(O::RBrace) {
            let cond = if self.eat_kw(K::Case) { Some(self.expr()?) }
                else if self.eat_kw(K::Default) { None }
                else { return self.err("oczekiwano `case` lub `default`"); };
            if !self.eat_op(O::Colon) { self.expect_op(O::Semi)?; }
            let mut body = vec![];
            while !self.is_kw(K::Case) && !self.is_kw(K::Default) && !self.is_op(O::RBrace) {
                if self.at_eof() { return self.err("niezamknięty switch"); }
                body.push(self.stmt()?);
            }
            cases.push(SwitchCase { cond, body });
        }
        self.bump();
        Ok(StmtKind::Switch(subject, cases))
    }

    fn try_stmt(&mut self) -> PResult<StmtKind> {
        self.bump();
        let body = self.block()?;
        let mut catches = vec![];
        while self.eat_kw(K::Catch) {
            self.expect_op(O::LParen)?;
            let mut types = vec![self.ident()?];
            while self.eat_op(O::BitOr) { types.push(self.ident()?); }
            let var = if let T::Variable(_) = self.peek() { Some(self.var_name()?) } else { None };
            self.expect_op(O::RParen)?;
            let b = self.block()?;
            catches.push(CatchClause { types, var, body: b });
        }
        let finally = if self.eat_kw(K::Finally) { Some(self.block()?) } else { None };
        if catches.is_empty() && finally.is_none() { return self.err("try wymaga catch lub finally"); }
        Ok(StmtKind::Try { body, catches, finally })
    }

    fn extern_stmt(&mut self) -> PResult<StmtKind> {
        self.bump();
        let lib = match self.bump() { T::Str(s) => rc(&s), _ => { self.pos -= 1; return self.err("extern wymaga nazwy biblioteki w cudzysłowie"); } };
        let mut items = vec![];
        if self.eat_op(O::Semi) { return Ok(StmtKind::Extern { lib, items }); }
        self.expect_op(O::LBrace)?;
        while !self.is_op(O::RBrace) {
            if !self.is_func_kw() { return self.err("w extern oczekiwano `func`"); }
            self.bump();
            let name = self.ident()?;
            let params = self.params()?;
            let ret = if self.eat_op(O::Colon) { self.types()? } else { vec![] };
            self.expect_op(O::Semi)?;
            items.push(ExternItem { name, params, ret });
        }
        self.bump();
        Ok(StmtKind::Extern { lib, items })
    }

    // ---------- funkcje, typy, klasy ----------
    fn types(&mut self) -> PResult<Vec<Name>> {
        let mut v = vec![];
        if self.eat_op(O::Question) { v.push(rc("null")); }
        loop {
            match self.bump() {
                T::Ident(s) => v.push(rc(&s)),
                T::Keyword(K::Null) => v.push(rc("null")),
                T::Keyword(K::Static) => v.push(rc("static")),
                T::Keyword(K::Fn) => v.push(rc("function")),
                T::Keyword(K::Func) => v.push(rc("function")),
                _ => { self.pos -= 1; return self.err("oczekiwano nazwy typu"); }
            }
            if !self.eat_op(O::BitOr) { break; }
        }
        Ok(v)
    }

    fn params(&mut self) -> PResult<Vec<Param>> {
        self.expect_op(O::LParen)?;
        let mut ps = vec![];
        while !self.is_op(O::RParen) {
            let types = if matches!(self.peek(), T::Variable(_)) || self.is_op(O::Ellipsis) { vec![] } else { self.types()? };
            let variadic = self.eat_op(O::Ellipsis);
            let name = self.var_name()?;
            let default = if self.eat_op(O::Assign) { Some(self.expr()?) } else { None };
            ps.push(Param { name, types, default, variadic });
            if !self.eat_op(O::Comma) { break; }
        }
        self.expect_op(O::RParen)?;
        Ok(ps)
    }

    /// po słowie `func` (i nazwie, jeśli `named`)
    fn func_rest(&mut self, named: bool) -> PResult<FuncDecl> {
        let line = self.line();
        let name = if named { Some(self.ident()?) } else { None };
        let params = self.params()?;
        let ret = if self.eat_op(O::Colon) { self.types()? } else { vec![] };
        let body = self.block()?;
        Ok(FuncDecl { name, params, ret, body, is_arrow: false, line })
    }

    fn class_decl(&mut self) -> PResult<StmtKind> {
        let is_abstract = self.eat_kw(K::Abstract);
        let is_interface = if self.eat_kw(K::Interface) { true } else {
            if !self.eat_kw(K::Class) { return self.err("oczekiwano `class`"); } false };
        let name = self.ident()?;
        let mut parent = None;
        let mut interfaces = vec![];
        if self.eat_kw(K::Extends) {
            if is_interface { interfaces.push(self.ident()?); while self.eat_op(O::Comma) { interfaces.push(self.ident()?); } }
            else { parent = Some(self.ident()?); }
        }
        if self.eat_kw(K::Implements) {
            interfaces.push(self.ident()?);
            while self.eat_op(O::Comma) { interfaces.push(self.ident()?); }
        }
        self.expect_op(O::LBrace)?;
        let (mut props, mut consts, mut methods) = (vec![], vec![], vec![]);
        while !self.is_op(O::RBrace) {
            if self.at_eof() { return self.err("niezamknięta klasa"); }
            let (mut is_static, mut abs) = (false, false);
            loop {
                if self.eat_kw(K::Public) || self.eat_kw(K::Private) || self.eat_kw(K::Protected) || self.eat_word("final") {}
                else if self.eat_kw(K::Static) { is_static = true; }
                else if self.eat_kw(K::Abstract) { abs = true; }
                else { break; }
            }
            if self.eat_word("const") {
                let n = self.ident()?; self.expect_op(O::Assign)?; let e = self.expr()?; self.expect_op(O::Semi)?;
                consts.push((n, e));
            } else if self.is_func_kw() {
                self.bump();
                let line = self.line();
                let n = self.ident()?;
                let params = self.params()?;
                let ret = if self.eat_op(O::Colon) { self.types()? } else { vec![] };
                let (body, is_abs) = if self.eat_op(O::Semi) { (vec![], true) } else { (self.block()?, false) };
                let decl = Rc::new(FuncDecl { name: Some(n), params, ret, body, is_arrow: false, line });
                methods.push(MethodDecl { decl, is_static, is_abstract: abs || is_abs });
            } else {
                if !matches!(self.peek(), T::Variable(_)) { self.types()?; }
                let n = self.var_name()?;
                let default = if self.eat_op(O::Assign) { Some(self.expr()?) } else { None };
                self.expect_op(O::Semi)?;
                props.push(PropDecl { name: n, default, is_static });
            }
        }
        self.bump();
        Ok(StmtKind::Class(Rc::new(ClassDecl { name, parent, interfaces, is_interface, is_abstract, props, consts, methods })))
    }

    // ---------- wyrażenia ----------
    pub fn expr(&mut self) -> PResult<Expr> { self.expr_bp(0) }

    fn mk(&self, kind: ExprKind, line: u32) -> Expr { Expr { kind, line } }

    fn expr_bp(&mut self, min: u8) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let line = self.line();
            // (lbp, rbp, operator)
            let t = self.peek().clone();
            enum Kind { Bin(BinOp), Assign, AssignOp(BinOp), CoalAssign, Ternary, InstanceOf }
            let (lbp, rbp, kind) = match &t {
                T::Keyword(K::Or) => (1, 2, Kind::Bin(BinOp::Or)),
                T::Keyword(K::Xor) => (2, 3, Kind::Bin(BinOp::Xor)),
                T::Keyword(K::And) => (3, 4, Kind::Bin(BinOp::And)),
                T::Op(o) => match o {
                    O::Assign => (5, 5, Kind::Assign),
                    O::PlusEq => (5, 5, Kind::AssignOp(BinOp::Add)),
                    O::MinusEq => (5, 5, Kind::AssignOp(BinOp::Sub)),
                    O::StarEq => (5, 5, Kind::AssignOp(BinOp::Mul)),
                    O::SlashEq => (5, 5, Kind::AssignOp(BinOp::Div)),
                    O::PercentEq => (5, 5, Kind::AssignOp(BinOp::Mod)),
                    O::DotEq => (5, 5, Kind::AssignOp(BinOp::Concat)),
                    O::CoalesceEq => (5, 5, Kind::CoalAssign),
                    O::Question => (6, 7, Kind::Ternary),
                    O::Coalesce => (8, 8, Kind::Bin(BinOp::Coalesce)),
                    O::OrOr => (9, 10, Kind::Bin(BinOp::Or)),
                    O::AndAnd => (10, 11, Kind::Bin(BinOp::And)),
                    O::BitOr => (11, 12, Kind::Bin(BinOp::BitOr)),
                    O::BitXor => (12, 13, Kind::Bin(BinOp::BitXor)),
                    O::BitAnd => (13, 14, Kind::Bin(BinOp::BitAnd)),
                    O::Eq => (14, 15, Kind::Bin(BinOp::Eq)),
                    O::NotEq => (14, 15, Kind::Bin(BinOp::NotEq)),
                    O::Identical => (14, 15, Kind::Bin(BinOp::Identical)),
                    O::NotIdentical => (14, 15, Kind::Bin(BinOp::NotIdentical)),
                    O::Lt => (15, 16, Kind::Bin(BinOp::Lt)),
                    O::Gt => (15, 16, Kind::Bin(BinOp::Gt)),
                    O::Le => (15, 16, Kind::Bin(BinOp::Le)),
                    O::Ge => (15, 16, Kind::Bin(BinOp::Ge)),
                    O::Dot => (16, 17, Kind::Bin(BinOp::Concat)),
                    O::Shl => (17, 18, Kind::Bin(BinOp::Shl)),
                    O::Shr => (17, 18, Kind::Bin(BinOp::Shr)),
                    O::Plus => (18, 19, Kind::Bin(BinOp::Add)),
                    O::Minus => (18, 19, Kind::Bin(BinOp::Sub)),
                    O::Star => (19, 20, Kind::Bin(BinOp::Mul)),
                    O::Slash => (19, 20, Kind::Bin(BinOp::Div)),
                    O::Percent => (19, 20, Kind::Bin(BinOp::Mod)),
                    O::Pow => (23, 22, Kind::Bin(BinOp::Pow)),
                    _ => break,
                },
                T::Ident(w) if &**w == "instanceof" => (16, 17, Kind::InstanceOf),
                _ => break,
            };
            if lbp < min { break; }
            self.bump();
            lhs = match kind {
                Kind::Bin(op) => { let r = self.expr_bp(rbp)?; self.mk(ExprKind::Binary(op, Box::new(lhs), Box::new(r)), line) }
                Kind::Assign => {
                    self.check_lvalue(&lhs)?;
                    let r = self.expr_bp(rbp)?; self.mk(ExprKind::Assign(Box::new(lhs), Box::new(r)), line)
                }
                Kind::AssignOp(op) => {
                    self.check_lvalue(&lhs)?;
                    let r = self.expr_bp(rbp)?; self.mk(ExprKind::AssignOp(op, Box::new(lhs), Box::new(r)), line)
                }
                Kind::CoalAssign => {
                    self.check_lvalue(&lhs)?;
                    let r = self.expr_bp(rbp)?; self.mk(ExprKind::AssignOp(BinOp::Coalesce, Box::new(lhs), Box::new(r)), line)
                }
                Kind::Ternary => {
                    let then = if self.is_op(O::Colon) { None } else { Some(Box::new(self.expr_bp(0)?)) };
                    self.expect_op(O::Colon)?;
                    let els = self.expr_bp(rbp)?;
                    self.mk(ExprKind::Ternary(Box::new(lhs), then, Box::new(els)), line)
                }
                Kind::InstanceOf => { let n = self.ident()?; self.mk(ExprKind::InstanceOf(Box::new(lhs), n), line) }
            };
        }
        Ok(lhs)
    }

    fn check_lvalue(&self, e: &Expr) -> PResult<()> {
        match e.kind {
            ExprKind::Var(_) | ExprKind::Index(..) | ExprKind::Prop(..) | ExprKind::Dot(..) => Ok(()),
            _ => self.err("lewa strona przypisania nie jest l-wartością"),
        }
    }

    fn unary(&mut self) -> PResult<Expr> {
        let line = self.line();
        match self.peek() {
            T::Op(O::Bang) => { self.bump(); let e = self.expr_bp(21)?; Ok(self.mk(ExprKind::Unary(UnOp::Not, Box::new(e)), line)) }
            T::Op(O::Minus) => { self.bump(); let e = self.expr_bp(21)?; Ok(self.mk(ExprKind::Unary(UnOp::Neg, Box::new(e)), line)) }
            T::Op(O::Plus) => { self.bump(); let e = self.expr_bp(21)?; Ok(self.mk(ExprKind::Unary(UnOp::Plus, Box::new(e)), line)) }
            T::Op(O::Tilde) => { self.bump(); let e = self.expr_bp(21)?; Ok(self.mk(ExprKind::Unary(UnOp::BitNot, Box::new(e)), line)) }
            T::Op(O::Inc) | T::Op(O::Dec) => {
                let inc = self.is_op(O::Inc); self.bump();
                let t = self.unary()?; self.check_lvalue(&t)?;
                Ok(self.mk(ExprKind::IncDec { target: Box::new(t), inc, prefix: true }, line))
            }
            T::Keyword(K::Await) => { self.bump(); let e = self.unary()?; Ok(self.mk(ExprKind::Await(Box::new(e)), line)) }
            _ => self.postfix(),
        }
    }

    fn is_namepath(e: &Expr) -> bool {
        match &e.kind { ExprKind::Name(_) => true, ExprKind::Dot(b, _) => Self::is_namepath(b), _ => false }
    }

    fn args(&mut self) -> PResult<Vec<Arg>> {
        self.expect_op(O::LParen)?;
        let mut v = vec![];
        while !self.is_op(O::RParen) {
            let spread = self.eat_op(O::Ellipsis);
            let name = if matches!(self.peek(), T::Ident(_)) && matches!(self.peek_n(1), T::Op(O::Colon)) {
                let n = self.ident()?; self.bump(); Some(n) } else { None };
            v.push(Arg { name, value: self.expr()?, spread });
            if !self.eat_op(O::Comma) { break; }
        }
        self.expect_op(O::RParen)?;
        Ok(v)
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            let line = self.line();
            match self.peek() {
                T::Op(O::LParen) => { let a = self.args()?; e = self.mk(ExprKind::Call(Box::new(e), a), line); }
                T::Op(O::LBracket) => {
                    self.bump();
                    let idx = if self.is_op(O::RBracket) { None } else { Some(Box::new(self.expr()?)) };
                    self.expect_op(O::RBracket)?;
                    e = self.mk(ExprKind::Index(Box::new(e), idx), line);
                }
                T::Op(O::Arrow) => { self.bump(); let n = self.ident()?; e = self.mk(ExprKind::Prop(Box::new(e), n), line); }
                T::Op(O::Dot) if Self::is_namepath(&e) && matches!(self.peek_n(1), T::Ident(_)) => {
                    self.bump(); let n = self.ident()?; e = self.mk(ExprKind::Dot(Box::new(e), n), line);
                }
                T::Op(O::Inc) | T::Op(O::Dec) => {
                    let inc = self.is_op(O::Inc); self.bump();
                    self.check_lvalue(&e)?;
                    e = self.mk(ExprKind::IncDec { target: Box::new(e), inc, prefix: false }, line);
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let line = self.line();
        let kind = match self.bump() {
            T::Int(n) => ExprKind::Int(n),
            T::Float(f) => ExprKind::Float(f),
            T::Str(s) => ExprKind::Str(rc(&s)),
            T::Keyword(K::True) => ExprKind::Bool(true),
            T::Keyword(K::False) => ExprKind::Bool(false),
            T::Keyword(K::Null) => ExprKind::Null,
            T::Variable(v) => ExprKind::Var(rc(&v)),
            T::Keyword(K::Static) => ExprKind::Name(rc("static")),
            T::Keyword(K::New) => {
                let n = self.ident()?;
                let a = if self.is_op(O::LParen) { self.args()? } else { vec![] };
                ExprKind::New(n, a)
            }
            T::Keyword(K::Match) => {
                let subj = self.paren_expr()?;
                self.expect_op(O::LBrace)?;
                let mut arms = vec![];
                while !self.is_op(O::RBrace) {
                    let conds = if self.eat_kw(K::Default) { None } else {
                        let mut c = vec![self.expr()?];
                        while self.eat_op(O::Comma) { if self.is_op(O::FatArrow) { break; } c.push(self.expr()?); }
                        Some(c) };
                    self.expect_op(O::FatArrow)?;
                    let body = self.expr()?;
                    arms.push(MatchArm { conds, body });
                    if !self.eat_op(O::Comma) { break; }
                }
                self.expect_op(O::RBrace)?;
                ExprKind::Match(Box::new(subj), arms)
            }
            T::Keyword(K::Fn) => {
                let params = self.params()?;
                let ret = if self.eat_op(O::Colon) { self.types()? } else { vec![] };
                self.expect_op(O::FatArrow)?;
                let body = self.expr_bp(5)?;
                let bl = body.line;
                let d = FuncDecl { name: None, params, ret, is_arrow: true, line,
                    body: vec![Stmt { kind: StmtKind::Return(Some(body)), line: bl }] };
                ExprKind::Closure(Rc::new(d))
            }
            T::Keyword(K::Func) => ExprKind::Closure(Rc::new(self.func_rest(false)?)),
            T::Ident(s) if &*s == "function" && self.is_op(O::LParen) => ExprKind::Closure(Rc::new(self.func_rest(false)?)),
            T::Ident(s) => ExprKind::Name(rc(&s)),
            T::Op(O::LParen) => { let e = self.expr()?; self.expect_op(O::RParen)?; return Ok(e); }
            T::Op(O::LBracket) => {
                let mut items = vec![];
                while !self.is_op(O::RBracket) {
                    let first = self.expr()?;
                    if self.eat_op(O::FatArrow) { let v = self.expr()?; items.push((Some(first), v)); }
                    else { items.push((None, first)); }
                    if !self.eat_op(O::Comma) { break; }
                }
                self.expect_op(O::RBracket)?;
                ExprKind::Array(items)
            }
            t => { self.pos -= 1; return self.err(format!("nieoczekiwany token w wyrażeniu: {:?}", t)); }
        };
        Ok(Expr { kind, line })
    }
}
