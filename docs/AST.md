# AST Z# (stage0 Rust ⇄ stage1 Z#)

Węzeł w Z#: `[":Rodzaj", linia, pole1, ...]`; listy to tablice; brak = `null`. Węzły bez własnej linii mają `0`.
Kanoniczny zapis: `(Rodzaj@linia pole ...)`, listy `[a b]`, brak `_`, napisy w cudzysłowie z `\\ \" \n \t \r`.
`zsc0 --sexp plik` (Rust) i `parser.dump` (Z#) dają identyczny tekst — to jest test parytetu (`scripts/tests/parser_parity.zsharp`).

**Instrukcje:** `Expr e` · `Echo [e]` · `Block [s]` · `If c [then] else|_` · `While c [b]` · `DoWhile [b] c` ·
`For [init] cond|_ [step] [b]` · `Foreach iter key|_ value [b]` · `Switch e [Case cond|_ [s]]` · `Return e|_` · `Break n` · `Continue n` ·
`Throw e` · `Try [b] [Catch [typy] var|_ [b]] finally|_` · `FuncStmt Func` · `ClassStmt Class` · `Import [nazwy]` · `Namespace [nazwy]` ·
`Using res var [b]` · `Extern lib [ExternItem nazwa [Param] [ret]]`

**Deklaracje:** `Func nazwa|_ [Param] [ret] [body] isArrow` · `Param nazwa [typy] default|_ variadic` ·
`Class nazwa parent|_ [ifaces] isInterface isAbstract [Prop nazwa default|_ static] [Const nazwa e] [Method Func static abstract]`

**Wyrażenia:** `Null` `Bool` `Int` `Float` `Str` `Var` `Name` · `Array [Item key|_ v]` · `Index e idx|_` · `Prop e n` · `Dot e n` ·
`Call e [Arg nazwa|_ e spread]` · `New klasa [Arg]` · `Unary Not|Neg|Plus|BitNot e` · `Binary Op l r` · `Assign l r` · `AssignOp Op l r` ·
`IncDec target inc prefix` · `Ternary c then|_ else` · `Closure Func` · `Match subj [Arm [cond]|_ body]` · `Await e` · `InstanceOf e klasa`

`Op`: Add Sub Mul Div Mod Pow Concat Eq NotEq Identical NotIdentical Lt Gt Le Ge And Or Xor Coalesce BitAnd BitOr BitXor Shl Shr.
