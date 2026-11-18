#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Shebang(String),
    DocComment(String),
    Ident(String),
    Variable(String), // bez '$'
    Keyword(Keyword),
    Int(i64),
    Float(f64),
    Str(String),
    Op(Op),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Func, Fn, Return, If, Elseif, Else, Switch, Case, Default, Match,
    For, Foreach, While, Do, Break, Continue, As, Import, Namespace, Extern,
    Class, Interface, Trait, Extends, Implements, New, Public, Private,
    Protected, Static, Abstract, Try, Catch, Finally, Throw, Using, Await,
    Echo, True, False, Null, And, Or, Xor,
}

impl Keyword {
    fn from_str(s: &str) -> Option<Keyword> {
        use Keyword::*;
        Some(match s {
            "func" => Func, "fn" => Fn, "return" => Return, "if" => If,
            "elseif" => Elseif, "else" => Else, "switch" => Switch,
            "case" => Case, "default" => Default, "match" => Match,
            "for" => For, "foreach" => Foreach, "while" => While, "do" => Do,
            "break" => Break, "continue" => Continue, "as" => As,
            "import" => Import, "namespace" => Namespace, "extern" => Extern,
            "class" => Class, "interface" => Interface, "trait" => Trait,
            "extends" => Extends, "implements" => Implements, "new" => New,
            "public" => Public, "private" => Private, "protected" => Protected,
            "static" => Static, "abstract" => Abstract, "try" => Try,
            "catch" => Catch, "finally" => Finally, "throw" => Throw,
            "using" => Using, "await" => Await, "echo" => Echo,
            "true" => True, "false" => False, "null" => Null,
            "and" => And, "or" => Or, "xor" => Xor,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Plus, Minus, Star, Slash, Percent, Pow,
    Dot, Assign, PlusEq, MinusEq, StarEq, SlashEq, PercentEq, DotEq, CoalesceEq,
    Eq, NotEq, Identical, NotIdentical, Lt, Gt, Le, Ge,
    AndAnd, OrOr, Bang, Coalesce, Arrow, FatArrow, Inc, Dec,
    Question, Colon, Comma, Semi,
    LParen, RParen, LBrace, RBrace, LBracket, RBracket,
    Ellipsis, BitAnd, BitOr, BitXor, Tilde, Shl, Shr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.span.line, self.span.col, self.message)
    }
}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer { src, bytes: src.as_bytes(), pos: 0, line: 1, col: 1 }
    }

    /// Tokenizuje całe wejście; ostatni token to zawsze `Eof`.
    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut out = Vec::new();
        if self.src.starts_with("#!") {
            let (s, l, c) = self.mark();
            let end = self.src.find('\n').unwrap_or(self.src.len());
            let text = self.src[2..end].to_string();
            while self.pos < end { self.bump(); }
            out.push(Token { kind: TokenKind::Shebang(text), span: self.span(s, l, c) });
        }
        loop {
            self.skip_trivia(&mut out)?;
            let (s, l, c) = self.mark();
            let Some(b) = self.peek() else {
                out.push(Token { kind: TokenKind::Eof, span: self.span(s, l, c) });
                return Ok(out);
            };
            let kind = match b {
                b'$' => self.variable()?,
                b'"' | b'\'' => self.string(b)?,
                b'0'..=b'9' => self.number()?,
                _ if is_ident_start(b) => self.ident(),
                _ => self.operator()?,
            };
            out.push(Token { kind, span: self.span(s, l, c) });
        }
    }

    fn mark(&self) -> (usize, u32, u32) { (self.pos, self.line, self.col) }
    fn span(&self, s: usize, l: u32, c: u32) -> Span { Span { start: s, end: self.pos, line: l, col: c } }
    fn peek(&self) -> Option<u8> { self.bytes.get(self.pos).copied() }
    fn peek_at(&self, n: usize) -> Option<u8> { self.bytes.get(self.pos + n).copied() }

    fn bump(&mut self) {
        if let Some(b) = self.peek() {
            self.pos += 1;
            if b == b'\n' { self.line += 1; self.col = 1; }
            // kolumny liczone w bajtach początkowych UTF-8
            else if (b & 0xC0) != 0x80 { self.col += 1; }
        }
    }

    fn err<T>(&self, msg: &str, s: usize, l: u32, c: u32) -> Result<T, LexError> {
        Err(LexError { message: msg.to_string(), span: self.span(s, l, c) })
    }

    fn skip_trivia(&mut self, out: &mut Vec<Token>) -> Result<(), LexError> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.bump(),
                Some(b'#') => {
                    let (s, l, c) = self.mark();
                    if self.peek_at(1) == Some(b'(') {
                        self.bump(); self.bump();
                        loop {
                            match self.peek() {
                                None => return self.err("niezamknięty komentarz #( ... )#", s, l, c),
                                Some(b')') if self.peek_at(1) == Some(b'#') => {
                                    self.bump(); self.bump(); break;
                                }
                                _ => self.bump(),
                            }
                        }
                    } else if self.peek_at(1) == Some(b'#') {
                        self.bump(); self.bump();
                        let rest_end = self.src[self.pos..].find('\n').map_or(self.src.len(), |i| self.pos + i);
                        let alone = self.src[self.pos..rest_end].trim().is_empty();
                        let text_start = self.pos;
                        let text;
                        if alone {
                            // blok: do następnego `##`
                            loop {
                                match self.peek() {
                                    None => return self.err("niezamknięty blok dokumentacyjny ##", s, l, c),
                                    Some(b'#') if self.peek_at(1) == Some(b'#') => {
                                        text = self.src[text_start..self.pos].to_string();
                                        self.bump(); self.bump(); break;
                                    }
                                    _ => self.bump(),
                                }
                            }
                        } else {
                            while self.pos < rest_end { self.bump(); }
                            text = self.src[text_start..self.pos].trim().to_string();
                        }
                        out.push(Token { kind: TokenKind::DocComment(text), span: self.span(s, l, c) });
                    } else {
                        while !matches!(self.peek(), None | Some(b'\n')) { self.bump(); }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn variable(&mut self) -> Result<TokenKind, LexError> {
        let (s, l, c) = self.mark();
        self.bump(); // '$'
        match self.peek() {
            Some(b) if is_ident_start(b) => {
                let st = self.pos;
                while matches!(self.peek(), Some(b) if is_ident_cont(b)) { self.bump(); }
                Ok(TokenKind::Variable(self.src[st..self.pos].to_string()))
            }
            _ => self.err("po `$` oczekiwano nazwy zmiennej", s, l, c),
        }
    }

    fn ident(&mut self) -> TokenKind {
        let st = self.pos;
        while matches!(self.peek(), Some(b) if is_ident_cont(b)) { self.bump(); }
        let word = &self.src[st..self.pos];
        match Keyword::from_str(word) {
            Some(k) => TokenKind::Keyword(k),
            None => TokenKind::Ident(word.to_string()),
        }
    }

    fn number(&mut self) -> Result<TokenKind, LexError> {
        let (s, l, c) = self.mark();
        if self.peek() == Some(b'0') {
            let radix = match self.peek_at(1) { Some(b'x' | b'X') => 16, Some(b'b' | b'B') => 2, Some(b'o' | b'O') => 8, _ => 0 };
            if radix != 0 {
                self.bump(); self.bump();
                let st = self.pos;
                while matches!(self.peek(), Some(b) if (b as char).is_digit(radix) || b == b'_') { self.bump(); }
                let digits: String = self.src[st..self.pos].chars().filter(|&ch| ch != '_').collect();
                return match i64::from_str_radix(&digits, radix) {
                    Ok(v) => Ok(TokenKind::Int(v)),
                    Err(_) => self.err("niepoprawny literał całkowity", s, l, c),
                };
            }
        }
        let digits = |lx: &mut Self| while matches!(lx.peek(), Some(b'0'..=b'9' | b'_')) { lx.bump(); };
        digits(self);
        let mut is_float = false;
        // `1.5` to float, ale `1.foo` / `1..` nie — kropka musi być przed cyfrą
        if self.peek() == Some(b'.') && matches!(self.peek_at(1), Some(b'0'..=b'9')) {
            is_float = true; self.bump(); digits(self);
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let sign = matches!(self.peek_at(1), Some(b'+' | b'-')) as usize;
            if matches!(self.peek_at(1 + sign), Some(b'0'..=b'9')) {
                is_float = true;
                for _ in 0..=sign { self.bump(); }
                digits(self);
            }
        }
        let text: String = self.src[s..self.pos].chars().filter(|&ch| ch != '_').collect();
        if is_float {
            text.parse().map(TokenKind::Float).or_else(|_| self.err("niepoprawny literał zmiennoprzecinkowy", s, l, c))
        } else {
            text.parse().map(TokenKind::Int).or_else(|_| self.err("literał całkowity poza zakresem i64", s, l, c))
        }
    }

    fn string(&mut self, quote: u8) -> Result<TokenKind, LexError> {
        let (s, l, c) = self.mark();
        self.bump();
        let mut buf = String::new();
        loop {
            let Some(b) = self.peek() else { return self.err("niezamknięty literał tekstowy", s, l, c) };
            if b == quote { self.bump(); return Ok(TokenKind::Str(buf)); }
            if b == b'\\' {
                self.bump();
                let Some(e) = self.peek() else { return self.err("niezamknięty literał tekstowy", s, l, c) };
                self.bump();
                match (quote, e) {
                    (_, b'\\') => buf.push('\\'),
                    (b'"', b'n') => buf.push('\n'),
                    (b'"', b't') => buf.push('\t'),
                    (b'"', b'r') => buf.push('\r'),
                    (b'"', b'0') => buf.push('\0'),
                    (b'"', b'$') => buf.push('$'),
                    (_, q) if q == quote => buf.push(quote as char),
                    // nieznana sekwencja: zachowaj dosłownie (jak PHP)
                    (_, other) => { buf.push('\\'); self.push_char_from(other, &mut buf); }
                }
            } else {
                let st = self.pos;
                self.bump();
                while matches!(self.peek(), Some(b) if (b & 0xC0) == 0x80) { self.bump(); }
                buf.push_str(&self.src[st..self.pos]);
            }
        }
    }

    // `e` został już skonsumowany; dla ASCII wystarczy dopisać znak
    fn push_char_from(&mut self, e: u8, buf: &mut String) {
        if e < 0x80 { buf.push(e as char); }
        else {
            let st = self.pos - 1;
            while matches!(self.peek(), Some(b) if (b & 0xC0) == 0x80) { self.bump(); }
            buf.push_str(&self.src[st..self.pos]);
        }
    }

    fn operator(&mut self) -> Result<TokenKind, LexError> {
        use Op::*;
        let (s, l, c) = self.mark();
        // najdłuższe dopasowanie pierwsze
        const TABLE: &[(&str, Op)] = &[
            ("===", Identical), ("!==", NotIdentical), ("??=", CoalesceEq), ("...", Ellipsis),
            ("**", Pow), ("==", Eq), ("!=", NotEq), ("<=", Le), (">=", Ge), ("&&", AndAnd),
            ("||", OrOr), ("??", Coalesce), ("->", Arrow), ("=>", FatArrow), ("++", Inc),
            ("--", Dec), ("+=", PlusEq), ("-=", MinusEq), ("*=", StarEq), ("/=", SlashEq),
            ("%=", PercentEq), (".=", DotEq), ("<<", Shl), (">>", Shr),
            ("+", Plus), ("-", Minus), ("*", Star), ("/", Slash), ("%", Percent), (".", Dot),
            ("=", Assign), ("<", Lt), (">", Gt), ("!", Bang), ("?", Question), (":", Colon),
            (",", Comma), (";", Semi), ("(", LParen), (")", RParen), ("{", LBrace),
            ("}", RBrace), ("[", LBracket), ("]", RBracket), ("&", BitAnd), ("|", BitOr),
            ("^", BitXor), ("~", Tilde),
        ];
        let rest = &self.src[self.pos..];
        for (text, op) in TABLE {
            if rest.starts_with(text) {
                for _ in 0..text.len() { self.bump(); }
                return Ok(TokenKind::Op(*op));
            }
        }
        let ch = rest.chars().next().unwrap();
        self.err(&format!("nieoczekiwany znak {:?}", ch), s, l, c)
    }
}

fn is_ident_start(b: u8) -> bool { b == b'_' || b.is_ascii_alphabetic() || b >= 0x80 }
fn is_ident_cont(b: u8) -> bool { is_ident_start(b) || b.is_ascii_digit() }

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        Lexer::new(src).tokenize().unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn shebang_only_at_start() {
        assert!(matches!(kinds("#!/usr/bin/env zsi\n")[0], TokenKind::Shebang(_)));
        // w środku pliku `#!` to zwykły komentarz
        assert_eq!(kinds("\n#! nie shebang\n"), vec![TokenKind::Eof]);
    }

    #[test]
    fn comments() {
        assert_eq!(kinds("# a\n#( x\n y )# 1"), vec![TokenKind::Int(1), TokenKind::Eof]);
        assert!(Lexer::new("#( brak końca").tokenize().is_err());
        let k = kinds("##\n * opis\n##\nfunc");
        assert!(matches!(&k[0], TokenKind::DocComment(t) if t.contains("opis")));
        assert_eq!(k[1], TokenKind::Keyword(Keyword::Func));
        assert!(matches!(&kinds("## krótki\n1")[0], TokenKind::DocComment(t) if t == "krótki"));
    }

    #[test]
    fn variables_and_keywords() {
        assert_eq!(
            kinds("func $a_1 Math"),
            vec![TokenKind::Keyword(Keyword::Func), TokenKind::Variable("a_1".into()),
                 TokenKind::Ident("Math".into()), TokenKind::Eof]
        );
        assert!(Lexer::new("$ ").tokenize().is_err());
    }

    #[test]
    fn numbers() {
        assert_eq!(kinds("42 0xFF 0b101 1_000 3.14 1e3")[..6], [
            TokenKind::Int(42), TokenKind::Int(255), TokenKind::Int(5),
            TokenKind::Int(1000), TokenKind::Float(3.14), TokenKind::Float(1000.0)]);
        // `1.foo` => Int, Dot, Ident
        assert_eq!(kinds("1.x")[..3], [TokenKind::Int(1), TokenKind::Op(Op::Dot), TokenKind::Ident("x".into())]);
    }

    #[test]
    fn strings() {
        assert_eq!(kinds(r#""a\n\"b" 'it\'s \n'"#)[..2],
            [TokenKind::Str("a\n\"b".into()), TokenKind::Str("it's \\n".into())]);
        assert!(Lexer::new("\"abc").tokenize().is_err());
        assert_eq!(kinds("\"Michał\"")[0], TokenKind::Str("Michał".into()));
    }

    #[test]
    fn operators_longest_match() {
        use Op::*;
        let ops: Vec<_> = kinds("=== == = ??= ?? -> => ++ + . .= ...").into_iter()
            .filter_map(|k| if let TokenKind::Op(o) = k { Some(o) } else { None }).collect();
        assert_eq!(ops, vec![Identical, Eq, Assign, CoalesceEq, Coalesce, Arrow, FatArrow, Inc, Plus, Dot, DotEq, Ellipsis]);
    }

    #[test]
    fn spans_track_lines() {
        let t = Lexer::new("a\n  b").tokenize().unwrap();
        assert_eq!((t[1].span.line, t[1].span.col), (2, 3));
    }
}
