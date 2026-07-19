//! インデント対応の字句解析器。
//!
//! 行指向でインデントスタックを管理し、INDENT / DEDENT / NEWLINE を発行する。
//! 括弧 `()` `[]` `{}` の内側では INDENT / DEDENT を発行しない
//! (NEWLINE はレコードフィールドやリスト要素の区切りとして発行する)。

use crate::diagnostics::{codes, Diagnostics};
use crate::span::{SourceFile, Span};
use crate::token::{Token, TokenKind};
use rust_decimal::Decimal;
use std::str::FromStr;

pub fn lex(file: &SourceFile, diags: &mut Diagnostics) -> Vec<Token> {
    Lexer {
        src: &file.src,
        bytes: file.src.as_bytes(),
        pos: 0,
        tokens: Vec::new(),
        indent_stack: vec![0],
        bracket_depth: 0,
        diags,
    }
    .run()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    indent_stack: Vec<u32>,
    bracket_depth: u32,
    diags: &'a mut Diagnostics,
}

impl<'a> Lexer<'a> {
    fn run(mut self) -> Vec<Token> {
        while self.pos < self.bytes.len() {
            self.lex_line();
        }
        // 未クローズのインデントを閉じる
        let end = self.src.len() as u32;
        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.push(TokenKind::Dedent, end, end);
        }
        self.push(TokenKind::Eof, end, end);
        self.tokens
    }

    /// 1物理行を処理する。空行・コメント行はトークンを発行しない。
    fn lex_line(&mut self) {
        let line_start = self.pos;

        // 先頭の空白 (インデント) を測る
        let mut indent: u32 = 0;
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b' ' => {
                    indent += 1;
                    self.pos += 1;
                }
                b'\t' => {
                    let p = self.pos as u32;
                    self.diags.emit_with_hint(
                        codes::LEX_TAB_INDENT,
                        Span::new(p, p + 1),
                        "タブによるインデントは使えません",
                        "半角スペース2個を使ってください",
                    );
                    // 回復: 1文字ぶんとして数えて続行
                    indent += 1;
                    self.pos += 1;
                }
                _ => break,
            }
        }

        // 空行・コメント行はインデント判定から除外する
        match self.bytes.get(self.pos) {
            None => return,
            Some(b'\n') => {
                self.pos += 1;
                return;
            }
            Some(b'\r') => {
                self.pos += 1;
                if self.bytes.get(self.pos) == Some(&b'\n') {
                    self.pos += 1;
                }
                return;
            }
            Some(b'#') => {
                self.skip_to_line_end();
                return;
            }
            _ => {}
        }

        // 括弧の外側でのみ INDENT / DEDENT を発行する
        if self.bracket_depth == 0 {
            let current = *self.indent_stack.last().unwrap();
            let sp = Span::new(line_start as u32, self.pos as u32);
            if indent > current {
                self.indent_stack.push(indent);
                self.push(TokenKind::Indent, sp.start, sp.end);
            } else if indent < current {
                while *self.indent_stack.last().unwrap() > indent {
                    self.indent_stack.pop();
                    self.push(TokenKind::Dedent, sp.start, sp.end);
                }
                if *self.indent_stack.last().unwrap() != indent {
                    self.diags.emit(
                        codes::LEX_BAD_INDENT,
                        sp,
                        "インデントが外側のどのブロックとも一致しません",
                    );
                    // 回復: この深さを新たなレベルとして積む
                    self.indent_stack.push(indent);
                }
            }
        }

        // 行内のトークン
        let mut had_token = false;
        loop {
            match self.bytes.get(self.pos) {
                None => break,
                Some(b'\n') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\r') => {
                    self.pos += 1;
                    if self.bytes.get(self.pos) == Some(&b'\n') {
                        self.pos += 1;
                    }
                    break;
                }
                Some(b'#') => {
                    self.skip_to_line_end();
                    break;
                }
                Some(b' ') => {
                    self.pos += 1;
                }
                Some(b'\t') => {
                    // 行中のタブは空白扱い (インデント以外)
                    self.pos += 1;
                }
                Some(_) => {
                    self.lex_token();
                    had_token = true;
                }
            }
        }

        if had_token {
            let p = self.pos as u32;
            self.push(TokenKind::Newline, p, p);
        }
    }

    fn skip_to_line_end(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            self.pos += 1;
            if b == b'\n' {
                break;
            }
        }
    }

    fn lex_token(&mut self) {
        let start = self.pos;
        let b = self.bytes[self.pos];
        match b {
            b'0'..=b'9' => self.lex_number(),
            b'"' => self.lex_text(),
            b'\'' => self.lex_char(),
            _ if b.is_ascii_alphabetic() || b == b'_' => self.lex_ident(),
            _ => {
                self.pos += 1;
                let two = |l: &Self| l.bytes.get(l.pos).copied();
                let kind = match b {
                    b'>' => {
                        if two(self) == Some(b'=') {
                            self.pos += 1;
                            TokenKind::Ge
                        } else {
                            TokenKind::Gt
                        }
                    }
                    b'<' => {
                        if two(self) == Some(b'=') {
                            self.pos += 1;
                            TokenKind::Le
                        } else {
                            TokenKind::Lt
                        }
                    }
                    b'=' => match two(self) {
                        Some(b'=') => {
                            self.pos += 1;
                            TokenKind::EqEq
                        }
                        Some(b'>') => {
                            self.pos += 1;
                            TokenKind::Arrow
                        }
                        _ => TokenKind::Eq,
                    },
                    b'!' => {
                        if two(self) == Some(b'=') {
                            self.pos += 1;
                            TokenKind::NotEq
                        } else {
                            TokenKind::Bang
                        }
                    }
                    b'|' => TokenKind::Pipe,
                    b'[' => {
                        self.bracket_depth += 1;
                        TokenKind::LBracket
                    }
                    b']' => {
                        self.bracket_depth = self.bracket_depth.saturating_sub(1);
                        TokenKind::RBracket
                    }
                    b'{' => {
                        self.bracket_depth += 1;
                        TokenKind::LBrace
                    }
                    b'}' => {
                        self.bracket_depth = self.bracket_depth.saturating_sub(1);
                        TokenKind::RBrace
                    }
                    b'(' => {
                        self.bracket_depth += 1;
                        TokenKind::LParen
                    }
                    b')' => {
                        self.bracket_depth = self.bracket_depth.saturating_sub(1);
                        TokenKind::RParen
                    }
                    b'.' => TokenKind::Dot,
                    b',' => TokenKind::Comma,
                    b':' => TokenKind::Colon,
                    b'+' => TokenKind::Plus,
                    b'-' => TokenKind::Minus,
                    b'*' => TokenKind::Star,
                    b'/' => TokenKind::Slash,
                    b'%' => TokenKind::Percent,
                    _ => {
                        // 未知の文字 (マルチバイト文字を含む)
                        let ch_len = self.src[start..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                        self.pos = start + ch_len;
                        self.diags.emit(
                            codes::LEX_INVALID_CHAR,
                            Span::new(start as u32, self.pos as u32),
                            format!(
                                "使用できない文字です: `{}`",
                                &self.src[start..self.pos]
                            ),
                        );
                        return;
                    }
                };
                self.push(kind, start as u32, self.pos as u32);
            }
        }
    }

    fn lex_number(&mut self) {
        let start = self.pos;
        while self.bytes.get(self.pos).is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        let mut is_decimal = false;
        if self.bytes.get(self.pos) == Some(&b'.')
            && self.bytes.get(self.pos + 1).is_some_and(|b| b.is_ascii_digit())
        {
            is_decimal = true;
            self.pos += 1;
            while self.bytes.get(self.pos).is_some_and(|b| b.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text = &self.src[start..self.pos];
        let span = Span::new(start as u32, self.pos as u32);
        if is_decimal {
            match Decimal::from_str(text) {
                Ok(d) => self.push(TokenKind::DecimalLit(d), span.start, span.end),
                Err(_) => self.diags.emit(
                    codes::LEX_BAD_NUMBER,
                    span,
                    format!("Decimal リテラルとして解釈できません: `{}`", text),
                ),
            }
        } else {
            match text.parse::<i64>() {
                Ok(v) => self.push(TokenKind::IntLit(v), span.start, span.end),
                Err(_) => self.diags.emit(
                    codes::LEX_BAD_NUMBER,
                    span,
                    format!("Int リテラルの範囲を超えています: `{}`", text),
                ),
            }
        }
    }

    fn lex_text(&mut self) {
        let start = self.pos;
        self.pos += 1; // 開き "
        let mut value = String::new();
        loop {
            match self.bytes.get(self.pos) {
                None | Some(b'\n') | Some(b'\r') => {
                    self.diags.emit(
                        codes::LEX_UNTERMINATED_TEXT,
                        Span::new(start as u32, self.pos as u32),
                        "Text リテラルが閉じられていません",
                    );
                    return;
                }
                Some(b'"') => {
                    self.pos += 1;
                    break;
                }
                Some(b'\\') => {
                    let esc_start = self.pos;
                    self.pos += 1;
                    match self.bytes.get(self.pos) {
                        Some(b'n') => {
                            value.push('\n');
                            self.pos += 1;
                        }
                        Some(b't') => {
                            value.push('\t');
                            self.pos += 1;
                        }
                        Some(b'\\') => {
                            value.push('\\');
                            self.pos += 1;
                        }
                        Some(b'"') => {
                            value.push('"');
                            self.pos += 1;
                        }
                        _ => {
                            self.diags.emit(
                                codes::LEX_BAD_ESCAPE,
                                Span::new(esc_start as u32, (self.pos + 1) as u32),
                                "不正なエスケープシーケンスです",
                            );
                            self.pos += 1;
                        }
                    }
                }
                Some(_) => {
                    let ch = self.src[self.pos..].chars().next().unwrap();
                    value.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
        self.push(TokenKind::TextLit(value), start as u32, self.pos as u32);
    }

    fn lex_char(&mut self) {
        let start = self.pos;
        self.pos += 1; // 開き '
        let ch = match self.bytes.get(self.pos) {
            None | Some(b'\n') | Some(b'\r') | Some(b'\'') => {
                self.diags.emit(
                    codes::LEX_BAD_CHAR,
                    Span::new(start as u32, (self.pos + 1) as u32),
                    "Char リテラルが不正です",
                );
                self.pos += 1;
                return;
            }
            Some(b'\\') => {
                self.pos += 1;
                let c = match self.bytes.get(self.pos) {
                    Some(b'n') => '\n',
                    Some(b't') => '\t',
                    Some(b'\\') => '\\',
                    Some(b'\'') => '\'',
                    _ => {
                        self.diags.emit(
                            codes::LEX_BAD_ESCAPE,
                            Span::new(start as u32, (self.pos + 1) as u32),
                            "不正なエスケープシーケンスです",
                        );
                        '\0'
                    }
                };
                self.pos += 1;
                c
            }
            Some(_) => {
                let c = self.src[self.pos..].chars().next().unwrap();
                self.pos += c.len_utf8();
                c
            }
        };
        if self.bytes.get(self.pos) == Some(&b'\'') {
            self.pos += 1;
            self.push(TokenKind::CharLit(ch), start as u32, self.pos as u32);
        } else {
            self.diags.emit(
                codes::LEX_BAD_CHAR,
                Span::new(start as u32, self.pos as u32),
                "Char リテラルが `'` で閉じられていません",
            );
        }
    }

    fn lex_ident(&mut self) {
        let start = self.pos;
        while self
            .bytes
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.pos += 1;
        }
        let text = &self.src[start..self.pos];
        let kind = match text {
            "import" => TokenKind::KwImport,
            "as" => TokenKind::KwAs,
            "T" => TokenKind::KwT,
            "C" => TokenKind::KwC,
            "R" => TokenKind::KwR,
            "F" => TokenKind::KwF,
            "when" => TokenKind::KwWhen,
            "match" => TokenKind::KwMatch,
            "at" => TokenKind::KwAt,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "and" => TokenKind::KwAnd,
            "or" => TokenKind::KwOr,
            "not" => TokenKind::KwNot,
            "Void" => TokenKind::KwVoid,
            "Error" => TokenKind::KwError,
            "List" => TokenKind::KwList,
            "from" => TokenKind::KwFrom,
            _ => {
                let first = text.chars().next().unwrap();
                if first.is_ascii_uppercase() {
                    TokenKind::UpperIdent(text.to_string())
                } else {
                    TokenKind::LowerIdent(text.to_string())
                }
            }
        };
        self.push(kind, start as u32, self.pos as u32);
    }

    fn push(&mut self, kind: TokenKind, start: u32, end: u32) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, end),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind::*;

    fn kinds(src: &str) -> (Vec<TokenKind>, Diagnostics) {
        let file = SourceFile::new("t.tcrf", src);
        let mut diags = Diagnostics::new();
        let toks = lex(&file, &mut diags);
        (toks.into_iter().map(|t| t.kind).collect(), diags)
    }

    #[test]
    fn hello_world() {
        let (k, d) = kinds("import std\n\nF main\n  Text \"Hello, World!\"\n  std.printLine\n");
        assert!(d.is_empty(), "{:?}", d.items);
        assert_eq!(
            k,
            vec![
                KwImport,
                LowerIdent("std".into()),
                Newline,
                KwF,
                LowerIdent("main".into()),
                Newline,
                Indent,
                UpperIdent("Text".into()),
                TextLit("Hello, World!".into()),
                Newline,
                LowerIdent("std".into()),
                Dot,
                LowerIdent("printLine".into()),
                Newline,
                Dedent,
                Eof,
            ]
        );
    }

    #[test]
    fn trailing_comment_inside_braces() {
        // 括弧内の複数行構造でも行末コメントを書ける (§2.1)
        let (k, d) = kinds("T P {  # レコード\n  a Int  # フィールド\n}\n");
        assert!(d.is_empty(), "{:?}", d.items);
        assert_eq!(
            k,
            vec![
                KwT,
                UpperIdent("P".into()),
                LBrace,
                Newline,
                LowerIdent("a".into()),
                UpperIdent("Int".into()),
                Newline,
                RBrace,
                Newline,
                Eof,
            ]
        );
    }

    #[test]
    fn hash_inside_literals_is_not_comment() {
        let (k, d) = kinds("C a = \"x # y\"\nC b = '#'\n");
        assert!(d.is_empty(), "{:?}", d.items);
        assert_eq!(
            k,
            vec![
                KwC,
                LowerIdent("a".into()),
                Eq,
                TextLit("x # y".into()),
                Newline,
                KwC,
                LowerIdent("b".into()),
                Eq,
                CharLit('#'),
                Newline,
                Eof,
            ]
        );
    }

    #[test]
    fn comment_lines_do_not_affect_indentation() {
        // ブロック途中のコメント行はインデント量にかかわらず構造に関与しない (§2.1)
        let (k, d) = kinds("R f x\n  a\n# 浅いコメント\n        # 深いコメント\n  b\n");
        assert!(d.is_empty(), "{:?}", d.items);
        assert_eq!(
            k,
            vec![
                KwR,
                LowerIdent("f".into()),
                LowerIdent("x".into()),
                Newline,
                Indent,
                LowerIdent("a".into()),
                Newline,
                LowerIdent("b".into()),
                Newline,
                Dedent,
                Eof,
            ]
        );
    }

    #[test]
    fn tab_indented_comment_line_is_error() {
        // タブ禁止はコメント行にも適用する (§2.1)
        let (_, d) = kinds("F main\n\t# comment\n  Void\n");
        assert!(d.items.iter().any(|i| i.code == codes::LEX_TAB_INDENT));
    }

    #[test]
    fn indent_dedent_nested() {
        let (k, d) = kinds("R f x\n  a\n    b\n  c\n");
        assert!(d.is_empty());
        assert_eq!(
            k,
            vec![
                KwR,
                LowerIdent("f".into()),
                LowerIdent("x".into()),
                Newline,
                Indent,
                LowerIdent("a".into()),
                Newline,
                Indent,
                LowerIdent("b".into()),
                Newline,
                Dedent,
                LowerIdent("c".into()),
                Newline,
                Dedent,
                Eof,
            ]
        );
    }

    #[test]
    fn tab_indent_is_error() {
        let (_, d) = kinds("F main\n\tText \"x\"\n");
        assert!(d.items.iter().any(|i| i.code == codes::LEX_TAB_INDENT));
    }

    #[test]
    fn bad_dedent_is_error() {
        let (_, d) = kinds("F main\n    a\n  b\n");
        assert!(d.items.iter().any(|i| i.code == codes::LEX_BAD_INDENT));
    }

    #[test]
    fn no_indent_tokens_inside_braces() {
        let (k, d) = kinds("F main\n  Limit {\n    value = 100\n  }\n  findPrimes\n");
        assert!(d.is_empty());
        // `{` の内側で INDENT/DEDENT が出ないこと
        assert_eq!(
            k,
            vec![
                KwF,
                LowerIdent("main".into()),
                Newline,
                Indent,
                UpperIdent("Limit".into()),
                LBrace,
                Newline,
                LowerIdent("value".into()),
                Eq,
                IntLit(100),
                Newline,
                RBrace,
                Newline,
                LowerIdent("findPrimes".into()),
                Newline,
                Dedent,
                Eof,
            ]
        );
    }

    #[test]
    fn operators_and_literals() {
        let (k, d) = kinds("x == 1.50 != -2 >= <= => ! | and or not 'A' '\\n'\n");
        assert!(d.is_empty(), "{:?}", d.items);
        use rust_decimal::Decimal;
        use std::str::FromStr;
        assert_eq!(
            k,
            vec![
                LowerIdent("x".into()),
                EqEq,
                DecimalLit(Decimal::from_str("1.50").unwrap()),
                NotEq,
                Minus,
                IntLit(2),
                Ge,
                Le,
                Arrow,
                Bang,
                Pipe,
                KwAnd,
                KwOr,
                KwNot,
                CharLit('A'),
                CharLit('\n'),
                Newline,
                Eof,
            ]
        );
    }

    #[test]
    fn comments_and_blank_lines_skipped() {
        let (k, d) = kinds("# top comment\n\nT Price [Decimal] # trailing\n");
        assert!(d.is_empty());
        assert_eq!(
            k,
            vec![
                KwT,
                UpperIdent("Price".into()),
                LBracket,
                UpperIdent("Decimal".into()),
                RBracket,
                Newline,
                Eof,
            ]
        );
    }

    #[test]
    fn crlf_lines() {
        let (k, d) = kinds("F main\r\n  a\r\n");
        assert!(d.is_empty());
        assert_eq!(
            k,
            vec![
                KwF,
                LowerIdent("main".into()),
                Newline,
                Indent,
                LowerIdent("a".into()),
                Newline,
                Dedent,
                Eof,
            ]
        );
    }

    #[test]
    fn empty_generic() {
        let (k, d) = kinds("std.empty<Int>\n");
        assert!(d.is_empty());
        assert_eq!(
            k,
            vec![
                LowerIdent("std".into()),
                Dot,
                LowerIdent("empty".into()),
                Lt,
                UpperIdent("Int".into()),
                Gt,
                Newline,
                Eof,
            ]
        );
    }
}
