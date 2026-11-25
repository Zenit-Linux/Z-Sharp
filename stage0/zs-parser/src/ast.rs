use std::rc::Rc;

pub type Name = Rc<str>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod, Pow, Concat,
    Eq, NotEq, Identical, NotIdentical, Lt, Gt, Le, Ge,
    And, Or, Xor, Coalesce, BitAnd, BitOr, BitXor, Shl, Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp { Neg, Plus, Not, BitNot }

#[derive(Debug, Clone)]
pub struct Expr { pub kind: ExprKind, pub line: u32 }

#[derive(Debug, Clone)]
pub struct Arg { pub name: Option<Name>, pub value: Expr, pub spread: bool }

#[derive(Debug, Clone)]
pub struct MatchArm { pub conds: Option<Vec<Expr>>, pub body: Expr } // None = default

#[derive(Debug, Clone)]
pub enum ExprKind {
    Null, Bool(bool), Int(i64), Float(f64), Str(Name),
    Var(Name),
    /// goły identyfikator: klasa, moduł lub stała
    Name(Name),
    Array(Vec<(Option<Expr>, Expr)>),
    Index(Box<Expr>, Option<Box<Expr>>),
    /// `$obj->prop`
    Prop(Box<Expr>, Name),
    /// `Module.member` / `Class.member` — kropka po nazwie
    Dot(Box<Expr>, Name),
    Call(Box<Expr>, Vec<Arg>),
    New(Name, Vec<Arg>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    AssignOp(BinOp, Box<Expr>, Box<Expr>),
    IncDec { target: Box<Expr>, inc: bool, prefix: bool },
    Ternary(Box<Expr>, Option<Box<Expr>>, Box<Expr>),
    Closure(Rc<FuncDecl>),
    Match(Box<Expr>, Vec<MatchArm>),
    Await(Box<Expr>),
    InstanceOf(Box<Expr>, Name),
    /// `...$x` w literale tablicy albo w wywołaniu
    Spread(Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: Name,
    pub types: Vec<Name>,
    pub default: Option<Expr>,
    pub variadic: bool,
}

#[derive(Debug)]
pub struct FuncDecl {
    pub name: Option<Name>,
    pub params: Vec<Param>,
    pub ret: Vec<Name>,
    pub body: Vec<Stmt>,
    pub is_arrow: bool,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct PropDecl { pub name: Name, pub default: Option<Expr>, pub is_static: bool }

#[derive(Debug, Clone)]
pub struct MethodDecl { pub decl: Rc<FuncDecl>, pub is_static: bool, pub is_abstract: bool }

#[derive(Debug)]
pub struct ClassDecl {
    pub name: Name,
    pub parent: Option<Name>,
    pub interfaces: Vec<Name>,
    pub is_interface: bool,
    pub is_abstract: bool,
    pub props: Vec<PropDecl>,
    pub consts: Vec<(Name, Expr)>,
    pub methods: Vec<MethodDecl>,
}

#[derive(Debug, Clone)]
pub struct CatchClause { pub types: Vec<Name>, pub var: Option<Name>, pub body: Vec<Stmt> }

#[derive(Debug, Clone)]
pub struct SwitchCase { pub cond: Option<Expr>, pub body: Vec<Stmt> }

#[derive(Debug, Clone)]
pub struct ExternItem { pub name: Name, pub params: Vec<Param>, pub ret: Vec<Name> }

#[derive(Debug, Clone)]
pub struct Stmt { pub kind: StmtKind, pub line: u32 }

#[derive(Debug, Clone)]
pub enum StmtKind {
    Expr(Expr),
    Echo(Vec<Expr>),
    Block(Vec<Stmt>),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
    DoWhile(Vec<Stmt>, Expr),
    For(Vec<Expr>, Option<Expr>, Vec<Expr>, Vec<Stmt>),
    Foreach { iter: Expr, key: Option<Name>, value: Name, body: Vec<Stmt> },
    Switch(Expr, Vec<SwitchCase>),
    Return(Option<Expr>),
    Break(u32), Continue(u32),
    Throw(Expr),
    Try { body: Vec<Stmt>, catches: Vec<CatchClause>, finally: Option<Vec<Stmt>> },
    Func(Rc<FuncDecl>),
    Class(Rc<ClassDecl>),
    Import(Vec<Name>),
    Namespace(Vec<Name>),
    Using { resource: Expr, var: Name, body: Vec<Stmt> },
    /// extern "lib" { func name(params): ret; ... } — parsowane, linkowane dopiero w zsc
    Extern { lib: Name, items: Vec<ExternItem> },
}
