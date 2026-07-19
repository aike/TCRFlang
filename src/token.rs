//! トークン定義。

use crate::span::Span;
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // 構造
    Newline,
    Indent,
    Dedent,
    Eof,

    // キーワード
    KwImport,
    KwAs,
    KwT,
    KwC,
    KwR,
    KwF,
    KwWhen,
    KwMatch,
    KwAt,
    KwTrue,
    KwFalse,
    KwAnd,
    KwOr,
    KwNot,
    KwVoid,
    KwError,
    KwList,
    KwFrom,

    // 識別子
    /// 大文字開始 (型名・コンストラクタ名)
    UpperIdent(String),
    /// 小文字開始 (値名・R名・F名・フィールド名)
    LowerIdent(String),

    // リテラル
    IntLit(i64),
    DecimalLit(Decimal),
    TextLit(String),
    CharLit(char),

    // 記号
    Gt,        // >
    Lt,        // <
    Ge,        // >=
    Le,        // <=
    EqEq,      // ==
    NotEq,     // !=
    Arrow,     // =>
    Bang,      // !
    Pipe,      // |
    LBracket,  // [
    RBracket,  // ]
    LBrace,    // {
    RBrace,    // }
    LParen,    // (
    RParen,    // )
    Dot,       // .
    Comma,     // , (std.tcrf の複数入力型シグネチャ用)
    Colon,     // :
    Eq,        // =
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
}

impl TokenKind {
    /// 診断メッセージ用の表示名。
    pub fn describe(&self) -> String {
        use TokenKind::*;
        match self {
            Newline => "改行".to_string(),
            Indent => "インデント".to_string(),
            Dedent => "デデント".to_string(),
            Eof => "ファイル末尾".to_string(),
            KwImport => "`import`".to_string(),
            KwAs => "`as`".to_string(),
            KwT => "`T`".to_string(),
            KwC => "`C`".to_string(),
            KwR => "`R`".to_string(),
            KwF => "`F`".to_string(),
            KwWhen => "`when`".to_string(),
            KwMatch => "`match`".to_string(),
            KwAt => "`at`".to_string(),
            KwTrue => "`true`".to_string(),
            KwFalse => "`false`".to_string(),
            KwAnd => "`and`".to_string(),
            KwOr => "`or`".to_string(),
            KwNot => "`not`".to_string(),
            KwVoid => "`Void`".to_string(),
            KwError => "`Error`".to_string(),
            KwList => "`List`".to_string(),
            KwFrom => "`from`".to_string(),
            UpperIdent(s) | LowerIdent(s) => format!("識別子 `{}`", s),
            IntLit(v) => format!("整数 `{}`", v),
            DecimalLit(v) => format!("小数 `{}`", v),
            TextLit(_) => "Text リテラル".to_string(),
            CharLit(_) => "Char リテラル".to_string(),
            Gt => "`>`".to_string(),
            Lt => "`<`".to_string(),
            Ge => "`>=`".to_string(),
            Le => "`<=`".to_string(),
            EqEq => "`==`".to_string(),
            NotEq => "`!=`".to_string(),
            Arrow => "`=>`".to_string(),
            Bang => "`!`".to_string(),
            Pipe => "`|`".to_string(),
            LBracket => "`[`".to_string(),
            RBracket => "`]`".to_string(),
            LBrace => "`{`".to_string(),
            RBrace => "`}`".to_string(),
            LParen => "`(`".to_string(),
            RParen => "`)`".to_string(),
            Dot => "`.`".to_string(),
            Comma => "`,`".to_string(),
            Colon => "`:`".to_string(),
            Eq => "`=`".to_string(),
            Plus => "`+`".to_string(),
            Minus => "`-`".to_string(),
            Star => "`*`".to_string(),
            Slash => "`/`".to_string(),
            Percent => "`%`".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
