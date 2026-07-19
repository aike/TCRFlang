//! 再帰下降パーサ (仕様 §25 簡易 EBNF 準拠)。

use crate::ast::*;
use crate::diagnostics::{codes, Diagnostics};
use crate::span::Span;
use crate::token::{Token, TokenKind};

pub fn parse(tokens: Vec<Token>, diags: &mut Diagnostics) -> Program {
    Parser {
        tokens,
        pos: 0,
        diags,
    }
    .parse_program()
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    diags: &'a mut Diagnostics,
}

impl<'a> Parser<'a> {
    // ---- 基本操作 ----

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos.min(self.tokens.len() - 1)].kind
    }

    fn peek_at(&self, n: usize) -> &TokenKind {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos.min(self.tokens.len() - 1)].span
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos.min(self.tokens.len() - 1)].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.peek() == kind
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, context: &str) -> bool {
        if self.eat(kind) {
            true
        } else {
            let found = self.peek().describe();
            self.diags.emit(
                codes::PARSE_UNEXPECTED,
                self.peek_span(),
                format!("{}には{}が必要ですが、{}が見つかりました", context, kind.describe(), found),
            );
            false
        }
    }

    fn expect_lower(&mut self, context: &str) -> Option<(String, Span)> {
        if let TokenKind::LowerIdent(name) = self.peek().clone() {
            let sp = self.peek_span();
            self.advance();
            Some((name, sp))
        } else {
            self.diags.emit(
                codes::PARSE_UNEXPECTED,
                self.peek_span(),
                format!(
                    "{}には小文字で始まる名前が必要ですが、{}が見つかりました",
                    context,
                    self.peek().describe()
                ),
            );
            None
        }
    }

    fn expect_upper(&mut self, context: &str) -> Option<(String, Span)> {
        if let TokenKind::UpperIdent(name) = self.peek().clone() {
            let sp = self.peek_span();
            self.advance();
            Some((name, sp))
        } else {
            self.diags.emit(
                codes::PARSE_UNEXPECTED,
                self.peek_span(),
                format!(
                    "{}には大文字で始まる名前が必要ですが、{}が見つかりました",
                    context,
                    self.peek().describe()
                ),
            );
            None
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
    }

    /// エラー回復: インデントの釣り合いを取りながら次のトップレベル宣言まで読み飛ばす。
    fn sync_to_decl(&mut self) {
        let mut depth: i32 = 0;
        loop {
            match self.peek() {
                TokenKind::Eof => return,
                TokenKind::Indent => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Dedent => {
                    depth -= 1;
                    self.advance();
                }
                TokenKind::KwT | TokenKind::KwC | TokenKind::KwR | TokenKind::KwF
                | TokenKind::KwImport
                    if depth <= 0 =>
                {
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // ---- プログラム ----

    fn parse_program(mut self) -> Program {
        let mut imports = Vec::new();
        let mut decls: Vec<Decl> = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek().clone() {
                TokenKind::Eof => break,
                TokenKind::KwImport => {
                    let sp = self.peek_span();
                    if !decls.is_empty() {
                        self.diags.emit(
                            codes::PARSE_IMPORT_POSITION,
                            sp,
                            "import はすべての T/C/R/F 宣言より前に置く必要があります",
                        );
                    }
                    if let Some(im) = self.parse_import() {
                        imports.push(im);
                    } else {
                        self.sync_to_decl();
                    }
                }
                TokenKind::KwT => match self.parse_type_decl() {
                    Some(d) => decls.push(Decl::Type(d)),
                    None => self.sync_to_decl(),
                },
                TokenKind::KwC => match self.parse_const_decl() {
                    Some(d) => decls.push(Decl::Const(d)),
                    None => self.sync_to_decl(),
                },
                TokenKind::KwR => match self.parse_rule_decl() {
                    Some(d) => decls.push(Decl::Rule(d)),
                    None => self.sync_to_decl(),
                },
                TokenKind::KwF => match self.parse_flow_decl() {
                    Some(d) => decls.push(Decl::Flow(d)),
                    None => self.sync_to_decl(),
                },
                _ => {
                    self.diags.emit(
                        codes::PARSE_UNEXPECTED,
                        self.peek_span(),
                        format!(
                            "トップレベルには import / T / C / R / F だけを書けますが、{}が見つかりました",
                            self.peek().describe()
                        ),
                    );
                    self.sync_to_decl();
                }
            }
        }
        Program { imports, decls }
    }

    // ---- import ----

    fn parse_import(&mut self) -> Option<ImportDecl> {
        let start = self.peek_span();
        self.advance(); // import
        let mut path = Vec::new();
        let (seg, _) = self.expect_lower("モジュール名")?;
        path.push(seg);
        while self.eat(&TokenKind::Dot) {
            let (seg, _) = self.expect_lower("モジュール名")?;
            path.push(seg);
        }
        let alias = if self.eat(&TokenKind::KwAs) {
            let (a, _) = self.expect_lower("import 別名")?;
            Some(a)
        } else {
            None
        };
        let end = self.peek_span();
        self.expect(&TokenKind::Newline, "import 宣言の後");
        Some(ImportDecl {
            path,
            alias,
            span: start.merge(end),
        })
    }

    // ---- 型宣言 ----

    fn parse_type_decl(&mut self) -> Option<TypeDecl> {
        self.advance(); // T
        let (name, name_span) = self.expect_upper("型名")?;
        match self.peek() {
            TokenKind::LBracket => {
                self.advance();
                let inner = self.parse_type_expr()?;
                self.expect(&TokenKind::RBracket, "用途型の内部型の後");
                self.expect(&TokenKind::Newline, "用途型宣言の後");
                Some(TypeDecl {
                    name,
                    name_span,
                    kind: TypeDeclKind::Usage(inner),
                })
            }
            TokenKind::LBrace => {
                self.advance();
                let mut fields = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.eat(&TokenKind::RBrace) {
                        break;
                    }
                    if self.check(&TokenKind::Eof) {
                        self.expect(&TokenKind::RBrace, "レコード型宣言");
                        return None;
                    }
                    let (fname, fspan) = self.expect_lower("フィールド名")?;
                    let ty = self.parse_type_expr()?;
                    fields.push(FieldDecl {
                        name: fname,
                        name_span: fspan,
                        ty,
                    });
                    if !self.check(&TokenKind::RBrace) {
                        self.expect(&TokenKind::Newline, "フィールド宣言の後");
                    }
                }
                self.expect(&TokenKind::Newline, "レコード型宣言の後");
                Some(TypeDecl {
                    name,
                    name_span,
                    kind: TypeDeclKind::Record(fields),
                })
            }
            TokenKind::Newline => {
                self.advance();
                self.expect(&TokenKind::Indent, "代数データ型のコンストラクタ行の前");
                let mut ctors = Vec::new();
                while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
                    if !self.expect(&TokenKind::Pipe, "コンストラクタ行の先頭") {
                        self.sync_to_decl();
                        return None;
                    }
                    let (cname, cspan) = self.expect_upper("コンストラクタ名")?;
                    let payload = if self.check(&TokenKind::Newline) {
                        None
                    } else {
                        let first = self.parse_type_expr()?;
                        // 複数ペイロードの検出 (§18)
                        if !self.check(&TokenKind::Newline) {
                            let sp = self.peek_span();
                            self.diags.emit_with_hint(
                                codes::PARSE_ADT_MULTI_PAYLOAD,
                                sp,
                                format!(
                                    "コンストラクタ `{}` に複数のペイロードは持てません",
                                    cname
                                ),
                                "複数の値が必要な場合はレコード型にまとめてください",
                            );
                            while !self.check(&TokenKind::Newline) && !self.check(&TokenKind::Eof) {
                                self.advance();
                            }
                        }
                        Some(first)
                    };
                    self.expect(&TokenKind::Newline, "コンストラクタ宣言の後");
                    ctors.push(CtorDecl {
                        name: cname,
                        name_span: cspan,
                        payload,
                    });
                }
                self.eat(&TokenKind::Dedent);
                Some(TypeDecl {
                    name,
                    name_span,
                    kind: TypeDeclKind::Adt(ctors),
                })
            }
            _ => {
                self.diags.emit(
                    codes::PARSE_UNEXPECTED,
                    self.peek_span(),
                    format!(
                        "型宣言には `[内部型]`・`{{ フィールド }}`・コンストラクタ行のいずれかが必要ですが、{}が見つかりました",
                        self.peek().describe()
                    ),
                );
                None
            }
        }
    }

    // ---- 型式 ----

    fn parse_type_expr(&mut self) -> Option<TypeExpr> {
        let start = self.peek_span();
        match self.peek().clone() {
            TokenKind::KwVoid => {
                self.advance();
                Some(TypeExpr::Void { span: start })
            }
            TokenKind::KwList => {
                self.advance();
                self.expect(&TokenKind::Lt, "`List` の後");
                let elem = self.parse_type_expr()?;
                let end = self.peek_span();
                self.expect(&TokenKind::Gt, "List 要素型の後");
                Some(TypeExpr::List {
                    elem: Box::new(elem),
                    span: start.merge(end),
                })
            }
            TokenKind::UpperIdent(name) => {
                self.advance();
                Some(TypeExpr::Named {
                    qualifier: None,
                    name,
                    span: start,
                })
            }
            TokenKind::LowerIdent(first) => {
                // 修飾型名: std.RangeInput / std.range.RangeInput
                self.advance();
                let mut qual = first;
                loop {
                    if !self.expect(&TokenKind::Dot, "修飾型名") {
                        return None;
                    }
                    match self.peek().clone() {
                        TokenKind::LowerIdent(seg) => {
                            self.advance();
                            qual.push('.');
                            qual.push_str(&seg);
                        }
                        TokenKind::UpperIdent(name) => {
                            let end = self.peek_span();
                            self.advance();
                            return Some(TypeExpr::Named {
                                qualifier: Some(qual),
                                name,
                                span: start.merge(end),
                            });
                        }
                        _ => {
                            self.diags.emit(
                                codes::PARSE_UNEXPECTED,
                                self.peek_span(),
                                format!(
                                    "修飾型名には識別子が必要ですが、{}が見つかりました",
                                    self.peek().describe()
                                ),
                            );
                            return None;
                        }
                    }
                }
            }
            _ => {
                self.diags.emit(
                    codes::PARSE_UNEXPECTED,
                    self.peek_span(),
                    format!("型が必要ですが、{}が見つかりました", self.peek().describe()),
                );
                None
            }
        }
    }

    /// 診断を出さずに型式のパースを試みる (std.empty<T> の判別用)。
    fn try_parse_type_expr_quiet(&mut self) -> Option<TypeExpr> {
        let saved_pos = self.pos;
        let saved_len = self.diags.items.len();
        let result = self.parse_type_expr();
        if result.is_none() {
            self.pos = saved_pos;
        }
        self.diags.items.truncate(saved_len);
        result
    }

    // ---- 定数 ----

    fn parse_const_decl(&mut self) -> Option<ConstDecl> {
        self.advance(); // C
        let (name, name_span) = self.expect_lower("定数名")?;
        self.expect(&TokenKind::Eq, "定数名の後");
        let value = self.parse_expr()?;
        self.expect(&TokenKind::Newline, "定数宣言の後");
        Some(ConstDecl {
            name,
            name_span,
            value,
        })
    }

    // ---- Rule ----

    fn parse_rule_decl(&mut self) -> Option<RuleDecl> {
        self.advance(); // R
        let (name, name_span) = self.expect_lower("R 名")?;
        // パラメータ名の列 (通常 R は最大1個。複数は宣言のみ R 専用で、後段で検査)
        let mut params = Vec::new();
        while let TokenKind::LowerIdent(p) = self.peek().clone() {
            let sp = self.peek_span();
            self.advance();
            params.push((p, sp));
        }
        self.expect(&TokenKind::Newline, "R ヘッダの後");
        self.expect(&TokenKind::Indent, "R 本体の前");

        let input = self.parse_type_expr()?;
        match self.peek().clone() {
            TokenKind::Arrow => {
                self.advance();
                let output = self.parse_type_expr()?;
                self.expect(&TokenKind::Newline, "表現保持型遷移の後");
                self.expect(&TokenKind::Dedent, "表現保持型遷移 R の末尾");
                if params.len() > 1 {
                    self.diags.emit(
                        codes::PARSE_UNEXPECTED,
                        params[1].1,
                        "R のパラメータは1個までです",
                    );
                }
                Some(RuleDecl {
                    name,
                    name_span,
                    params,
                    kind: RuleKind::Transition { input, output },
                })
            }
            TokenKind::Comma | TokenKind::Gt => {
                // カンマ区切りの入力型列 (複数は宣言のみ R 専用)
                let mut inputs = vec![input];
                while self.eat(&TokenKind::Comma) {
                    inputs.push(self.parse_type_expr()?);
                }
                self.expect(&TokenKind::Gt, "R シグネチャの入力型の後");
                let output = self.parse_type_expr()?;
                let can_fail = if self.eat(&TokenKind::Bang) {
                    self.expect(&TokenKind::KwError, "`!` の後");
                    true
                } else {
                    false
                };
                self.expect(&TokenKind::Newline, "R シグネチャの後");

                // シグネチャ行の直後で R が終わる → 宣言のみ (本体なし)
                if self.check(&TokenKind::Dedent) {
                    self.advance();
                    return Some(RuleDecl {
                        name,
                        name_span,
                        params,
                        kind: RuleKind::External {
                            inputs,
                            output,
                            can_fail,
                        },
                    });
                }

                // 本体付きの通常 R: 入力型1個・パラメータ1個まで
                if inputs.len() > 1 {
                    self.diags.emit(
                        codes::PARSE_UNEXPECTED,
                        inputs[1].span(),
                        "本体を持つ R の入力型は1個です (カンマ区切りは宣言のみ R 専用)",
                    );
                }
                if params.len() > 1 {
                    self.diags.emit(
                        codes::PARSE_UNEXPECTED,
                        params[1].1,
                        "本体を持つ R のパラメータは1個までです",
                    );
                }
                let body = self.parse_block()?;
                self.expect(&TokenKind::Dedent, "R 本体の末尾");
                Some(RuleDecl {
                    name,
                    name_span,
                    params,
                    kind: RuleKind::Normal {
                        input: inputs.into_iter().next().unwrap(),
                        output,
                        can_fail,
                        body,
                    },
                })
            }
            _ => {
                self.diags.emit(
                    codes::PARSE_UNEXPECTED,
                    self.peek_span(),
                    format!(
                        "R シグネチャには `>` または `=>` が必要ですが、{}が見つかりました",
                        self.peek().describe()
                    ),
                );
                None
            }
        }
    }

    /// 束縛の列 + 最終式。呼び出し側が末尾の DEDENT を処理する。
    fn parse_block(&mut self) -> Option<Block> {
        let mut bindings = Vec::new();
        loop {
            // 束縛の先読み: lowerIdent の直後が `:` または `=`
            let is_binding = matches!(self.peek(), TokenKind::LowerIdent(_))
                && matches!(self.peek_at(1), TokenKind::Colon | TokenKind::Eq);
            if !is_binding {
                break;
            }
            bindings.push(self.parse_binding()?);
        }
        let result = self.parse_block_result()?;
        Some(Block { bindings, result })
    }

    fn parse_binding(&mut self) -> Option<Binding> {
        let (name, name_span) = self.expect_lower("束縛名")?;
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq, "束縛名の後");
        let value = if self.check(&TokenKind::Newline) {
            // 継続行: 値が次行にインデントされている
            self.advance();
            self.expect(&TokenKind::Indent, "束縛値の前");
            let v = self.parse_block_result()?;
            self.expect(&TokenKind::Dedent, "束縛値の後");
            v
        } else {
            let v = self.parse_expr()?;
            self.expect(&TokenKind::Newline, "束縛値の後");
            v
        };
        Some(Binding {
            name,
            name_span,
            ty,
            value,
        })
    }

    /// ブロックの結果位置の式: when / match / 1行の式。
    fn parse_block_result(&mut self) -> Option<Expr> {
        match self.peek() {
            TokenKind::KwWhen => self.parse_when(),
            TokenKind::KwMatch => self.parse_match(),
            _ => {
                let e = self.parse_expr()?;
                self.expect(&TokenKind::Newline, "式の後");
                Some(e)
            }
        }
    }

    fn parse_when(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        self.advance(); // when
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::Newline, "when 条件の後");
        self.expect(&TokenKind::Indent, "when 分岐の前");

        self.expect(&TokenKind::KwTrue, "when の最初の分岐 (`true`)");
        let then_block = self.parse_when_branch()?;

        self.expect(&TokenKind::KwFalse, "when の2番目の分岐 (`false`)");
        let else_block = self.parse_when_branch()?;

        let end = self.peek_span();
        self.expect(&TokenKind::Dedent, "when の末尾");
        Some(Expr::When {
            cond: Box::new(cond),
            then_block: Box::new(then_block),
            else_block: Box::new(else_block),
            span: start.merge(end),
        })
    }

    fn parse_when_branch(&mut self) -> Option<Block> {
        if self.check(&TokenKind::Newline) {
            // 通常形: true NEWLINE INDENT block DEDENT
            self.advance();
            self.expect(&TokenKind::Indent, "when 分岐本体の前");
            let block = self.parse_block()?;
            self.expect(&TokenKind::Dedent, "when 分岐本体の後");
            Some(block)
        } else {
            // 短縮形: true 式 NEWLINE
            let e = self.parse_expr()?;
            self.expect(&TokenKind::Newline, "when 短縮分岐の後");
            Some(Block {
                bindings: Vec::new(),
                result: e,
            })
        }
    }

    fn parse_match(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        self.advance(); // match
        let scrutinee = self.parse_expr()?;
        self.expect(&TokenKind::Newline, "match 対象の後");
        self.expect(&TokenKind::Indent, "match 分岐の前");
        let mut arms = Vec::new();
        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            let (ctor, ctor_span) = self.expect_upper("match 分岐のコンストラクタ名")?;
            let binding = if let TokenKind::LowerIdent(b) = self.peek().clone() {
                let sp = self.peek_span();
                self.advance();
                Some((b, sp))
            } else {
                None
            };
            self.expect(&TokenKind::Newline, "match 分岐ヘッダの後");
            self.expect(&TokenKind::Indent, "match 分岐本体の前");
            let body = self.parse_block()?;
            self.expect(&TokenKind::Dedent, "match 分岐本体の後");
            arms.push(MatchArm {
                ctor,
                ctor_span,
                binding,
                body,
            });
        }
        let end = self.peek_span();
        self.expect(&TokenKind::Dedent, "match の末尾");
        Some(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: start.merge(end),
        })
    }

    // ---- Flow ----

    fn parse_flow_decl(&mut self) -> Option<FlowDecl> {
        self.advance(); // F
        let (name, name_span) = self.expect_lower("F 名")?;
        self.expect(&TokenKind::Newline, "F ヘッダの後");
        self.expect(&TokenKind::Indent, "F 本体の前");

        // シグネチャ判定: 行末までに括弧深度0の `>` があるか
        let signature = if self.line_has_signature_arrow() {
            let input = self.parse_type_expr()?;
            self.expect(&TokenKind::Gt, "F シグネチャの入力型の後");
            let output = self.parse_type_expr()?;
            let can_fail = if self.eat(&TokenKind::Bang) {
                self.expect(&TokenKind::KwError, "`!` の後");
                true
            } else {
                false
            };
            self.expect(&TokenKind::Newline, "F シグネチャの後");
            Some(FlowSignature {
                input,
                output,
                can_fail,
            })
        } else {
            None
        };

        let steps = self.parse_flow_steps()?;
        self.expect(&TokenKind::Dedent, "F 本体の末尾");
        if steps.is_empty() {
            self.diags.emit(
                codes::PARSE_UNEXPECTED,
                name_span,
                format!("F `{}` には少なくとも1つのステップが必要です", name),
            );
        }
        Some(FlowDecl {
            name,
            name_span,
            signature,
            steps,
        })
    }

    fn line_has_signature_arrow(&self) -> bool {
        let mut i = self.pos;
        let mut depth = 0u32;
        while i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::Newline | TokenKind::Eof => return false,
                TokenKind::LParen | TokenKind::LBrace | TokenKind::LBracket => depth += 1,
                TokenKind::RParen | TokenKind::RBrace | TokenKind::RBracket => {
                    depth = depth.saturating_sub(1)
                }
                TokenKind::Lt => depth += 1, // List<...> 内の Gt を除外
                TokenKind::Gt if depth == 0 => return true,
                TokenKind::Gt => depth = depth.saturating_sub(1),
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// DEDENT (消費しない) までフローステップを読む。
    fn parse_flow_steps(&mut self) -> Option<Vec<FlowStep>> {
        let mut steps = Vec::new();
        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::KwMatch) {
                steps.push(self.parse_flow_match()?);
                continue;
            }
            let expr = self.parse_expr()?;
            self.expect(&TokenKind::Newline, "F ステップの後");
            match expr {
                Expr::Name { path, span } => steps.push(FlowStep::Call { path, span }),
                Expr::IntLit(..)
                | Expr::DecimalLit(..)
                | Expr::TextLit(..)
                | Expr::CharLit(..)
                | Expr::BoolLit(..)
                | Expr::VoidLit(..)
                | Expr::Construct { .. }
                | Expr::From { .. }
                | Expr::Record { .. }
                | Expr::ListConstruct { .. }
                | Expr::Empty { .. } => steps.push(FlowStep::Initial(expr)),
                other => {
                    self.diags.emit_with_hint(
                        codes::PARSE_FLOW_FORBIDDEN,
                        other.span(),
                        "F には値の構築と R/F の接続だけを書けます",
                        "計算やローカル束縛は R へ分離してください",
                    );
                }
            }
        }
        Some(steps)
    }

    fn parse_flow_match(&mut self) -> Option<FlowStep> {
        let start = self.peek_span();
        self.advance(); // match
        self.expect(&TokenKind::Newline, "F の match の後");
        self.expect(&TokenKind::Indent, "F の match 分岐の前");
        let mut arms = Vec::new();
        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            let (ctor, ctor_span) = self.expect_upper("match 分岐のコンストラクタ名")?;
            self.expect(&TokenKind::Newline, "match 分岐ヘッダの後");
            self.expect(&TokenKind::Indent, "match 分岐本体の前");
            let steps = self.parse_flow_steps()?;
            self.expect(&TokenKind::Dedent, "match 分岐本体の後");
            arms.push(FlowMatchArm {
                ctor,
                ctor_span,
                steps,
            });
        }
        let end = self.peek_span();
        self.expect(&TokenKind::Dedent, "F の match の末尾");
        Some(FlowStep::Match {
            arms,
            span: start.merge(end),
        })
    }

    // ---- 式 ----

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let mut left = self.parse_and()?;
        while self.check(&TokenKind::KwOr) {
            self.advance();
            let right = self.parse_and()?;
            let span = left.span().merge(right.span());
            left = Expr::Binary {
                op: BinOp::Or,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let mut left = self.parse_equality()?;
        while self.check(&TokenKind::KwAnd) {
            self.advance();
            let right = self.parse_equality()?;
            let span = left.span().merge(right.span());
            left = Expr::Binary {
                op: BinOp::And,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn parse_equality(&mut self) -> Option<Expr> {
        let mut left = self.parse_relational()?;
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::NotEq => BinOp::Ne,
                _ => break,
            };
            self.advance();
            let right = self.parse_relational()?;
            let span = left.span().merge(right.span());
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn parse_relational(&mut self) -> Option<Expr> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = left.span().merge(right.span());
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn parse_additive(&mut self) -> Option<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = left.span().merge(right.span());
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn parse_multiplicative(&mut self) -> Option<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            let span = left.span().merge(right.span());
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        match self.peek() {
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span());
                Some(Expr::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::KwNot => {
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(operand.span());
                Some(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }
            _ => self.parse_app(),
        }
    }

    /// `at` 式・R 呼び出し (名前 + 空白区切り引数)。
    fn parse_app(&mut self) -> Option<Expr> {
        if self.check(&TokenKind::KwAt) {
            let start = self.peek_span();
            self.advance();
            let list = self.parse_primary()?;
            let index = self.parse_primary()?;
            let span = start.merge(index.span());
            return Some(Expr::At {
                list: Box::new(list),
                index: Box::new(index),
                span,
            });
        }
        let head = self.parse_primary()?;
        // 名前の直後に一次式が続く場合は呼び出し
        if let Expr::Name { path, span } = head {
            if self.starts_primary() {
                let mut args = Vec::new();
                let mut end = span;
                while self.starts_primary() {
                    let a = self.parse_primary()?;
                    end = end.merge(a.span());
                    args.push(a);
                }
                return Some(Expr::Call {
                    path,
                    args,
                    span: end,
                });
            }
            return Some(Expr::Name { path, span });
        }
        Some(head)
    }

    fn starts_primary(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::IntLit(_)
                | TokenKind::DecimalLit(_)
                | TokenKind::TextLit(_)
                | TokenKind::CharLit(_)
                | TokenKind::KwTrue
                | TokenKind::KwFalse
                | TokenKind::KwVoid
                | TokenKind::UpperIdent(_)
                | TokenKind::LowerIdent(_)
                | TokenKind::LParen
        )
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        match self.peek().clone() {
            TokenKind::IntLit(v) => {
                self.advance();
                Some(Expr::IntLit(v, start))
            }
            TokenKind::DecimalLit(d) => {
                self.advance();
                Some(Expr::DecimalLit(d, start))
            }
            TokenKind::TextLit(s) => {
                self.advance();
                Some(Expr::TextLit(s, start))
            }
            TokenKind::CharLit(c) => {
                self.advance();
                Some(Expr::CharLit(c, start))
            }
            TokenKind::KwTrue => {
                self.advance();
                Some(Expr::BoolLit(true, start))
            }
            TokenKind::KwFalse => {
                self.advance();
                Some(Expr::BoolLit(false, start))
            }
            TokenKind::KwVoid => {
                self.advance();
                Some(Expr::VoidLit(start))
            }
            TokenKind::LParen => {
                self.advance();
                self.skip_newlines();
                let e = self.parse_expr()?;
                self.skip_newlines();
                self.expect(&TokenKind::RParen, "括弧式の後");
                Some(e)
            }
            TokenKind::UpperIdent(name) => {
                self.advance();
                self.parse_construct_tail(None, name, start)
            }
            TokenKind::LowerIdent(first) => {
                self.advance();
                let mut path = vec![(first, start)];
                let mut span = start;
                loop {
                    if !self.check(&TokenKind::Dot) {
                        break;
                    }
                    match self.peek_at(1).clone() {
                        TokenKind::LowerIdent(seg) => {
                            self.advance(); // .
                            let sp = self.peek_span();
                            self.advance();
                            span = span.merge(sp);
                            path.push((seg, sp));
                        }
                        TokenKind::UpperIdent(name) => {
                            // 修飾付き構築: std.RangeInput { ... } など
                            self.advance(); // .
                            let name_span = self.peek_span();
                            self.advance();
                            let qual = path
                                .iter()
                                .map(|(s, _)| s.as_str())
                                .collect::<Vec<_>>()
                                .join(".");
                            return self.parse_construct_tail(Some(qual), name, name_span);
                        }
                        _ => break,
                    }
                }
                // std.empty<T> の判別
                if path.last().is_some_and(|(s, _)| s == "empty") && self.check(&TokenKind::Lt) {
                    let saved = self.pos;
                    self.advance(); // <
                    if let Some(elem) = self.try_parse_type_expr_quiet() {
                        if self.check(&TokenKind::Gt) {
                            let end = self.peek_span();
                            self.advance();
                            return Some(Expr::Empty {
                                path,
                                elem,
                                span: span.merge(end),
                            });
                        }
                    }
                    self.pos = saved;
                }
                Some(Expr::Name { path, span })
            }
            _ => {
                self.diags.emit(
                    codes::PARSE_UNEXPECTED,
                    start,
                    format!(
                        "式が必要ですが、{}が見つかりました",
                        self.peek().describe()
                    ),
                );
                None
            }
        }
    }

    /// UpperIdent の後続を見て、レコード構築 / リスト用途型構築 / 用途型・ADT 構築を判別する。
    fn parse_construct_tail(
        &mut self,
        qualifier: Option<String>,
        name: String,
        name_span: Span,
    ) -> Option<Expr> {
        match self.peek() {
            TokenKind::KwFrom => {
                // `A from x` — 用途型どうしの表現保持変換
                self.advance();
                let value = self.parse_primary()?;
                let span = name_span.merge(value.span());
                Some(Expr::From {
                    qualifier,
                    name,
                    name_span,
                    value: Box::new(value),
                    span,
                })
            }
            TokenKind::LBrace => {
                self.advance();
                let mut fields = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.check(&TokenKind::RBrace) {
                        break;
                    }
                    if self.check(&TokenKind::Eof) {
                        self.expect(&TokenKind::RBrace, "レコード構築");
                        return None;
                    }
                    let (fname, fspan) = self.expect_lower("レコードのフィールド名")?;
                    self.expect(&TokenKind::Eq, "フィールド名の後");
                    // フィールド値は次行へ折り返せる (§24 の `values =` など)
                    self.skip_newlines();
                    let value = self.parse_expr()?;
                    fields.push(RecordFieldInit {
                        name: fname,
                        name_span: fspan,
                        value,
                    });
                    if !self.check(&TokenKind::RBrace) && !self.eat(&TokenKind::Newline) {
                        self.expect(&TokenKind::Newline, "フィールド初期化の後");
                        return None;
                    }
                }
                let end = self.peek_span();
                self.advance(); // }
                Some(Expr::Record {
                    qualifier,
                    name,
                    name_span,
                    fields,
                    span: name_span.merge(end),
                })
            }
            TokenKind::LParen => {
                self.advance();
                let mut elems = Vec::new();
                loop {
                    self.skip_newlines();
                    if self.check(&TokenKind::RParen) {
                        break;
                    }
                    if self.check(&TokenKind::Eof) {
                        self.expect(&TokenKind::RParen, "リスト用途型構築");
                        return None;
                    }
                    let e = self.parse_expr()?;
                    elems.push(e);
                    if !self.check(&TokenKind::RParen) && !self.eat(&TokenKind::Newline) {
                        self.expect(&TokenKind::Newline, "リスト要素の後");
                        return None;
                    }
                }
                let end = self.peek_span();
                self.advance(); // )
                Some(Expr::ListConstruct {
                    qualifier,
                    name,
                    name_span,
                    elems,
                    span: name_span.merge(end),
                })
            }
            _ if self.starts_primary() => {
                let arg = self.parse_primary()?;
                let span = name_span.merge(arg.span());
                Some(Expr::Construct {
                    qualifier,
                    name,
                    name_span,
                    arg: Some(Box::new(arg)),
                    span,
                })
            }
            _ => Some(Expr::Construct {
                qualifier,
                name,
                name_span,
                arg: None,
                span: name_span,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::span::SourceFile;

    fn parse_ok(src: &str) -> Program {
        let file = SourceFile::new("t.tcrf", src);
        let mut diags = Diagnostics::new();
        let toks = lexer::lex(&file, &mut diags);
        let prog = parse(toks, &mut diags);
        assert!(
            diags.is_empty(),
            "unexpected diagnostics:\n{}",
            diags.render(&file)
        );
        prog
    }

    fn parse_err(src: &str) -> Diagnostics {
        let file = SourceFile::new("t.tcrf", src);
        let mut diags = Diagnostics::new();
        let toks = lexer::lex(&file, &mut diags);
        let _ = parse(toks, &mut diags);
        assert!(!diags.is_empty(), "expected diagnostics but none");
        diags
    }

    #[test]
    fn hello_world() {
        let p = parse_ok("import std\n\nF main\n  Text \"Hello, World!\"\n  std.printLine\n");
        assert_eq!(p.imports.len(), 1);
        assert_eq!(p.imports[0].path, vec!["std"]);
        assert_eq!(p.decls.len(), 1);
        let Decl::Flow(f) = &p.decls[0] else {
            panic!()
        };
        assert_eq!(f.name, "main");
        assert!(f.signature.is_none());
        assert_eq!(f.steps.len(), 2);
        assert!(matches!(f.steps[0], FlowStep::Initial(Expr::Construct { .. })));
        assert!(matches!(&f.steps[1], FlowStep::Call { path, .. } if path.len() == 2));
    }

    #[test]
    fn type_decls() {
        let p = parse_ok(
            "T UserId [Text]\n\nT Product {\n  id    UserId\n  name  Text\n}\n\nT Grade\n  | Excellent\n  | Passed\n  | Failed\n",
        );
        assert_eq!(p.decls.len(), 3);
        assert!(matches!(
            &p.decls[0],
            Decl::Type(TypeDecl {
                kind: TypeDeclKind::Usage(_),
                ..
            })
        ));
        let Decl::Type(rec) = &p.decls[1] else { panic!() };
        let TypeDeclKind::Record(fields) = &rec.kind else {
            panic!()
        };
        assert_eq!(fields.len(), 2);
        let Decl::Type(adt) = &p.decls[2] else { panic!() };
        let TypeDeclKind::Adt(ctors) = &adt.kind else {
            panic!()
        };
        assert_eq!(ctors.len(), 3);
        assert!(ctors.iter().all(|c| c.payload.is_none()));
    }

    #[test]
    fn adt_multi_payload_error() {
        let d = parse_err("T Point\n  | Point Decimal Decimal\n");
        assert!(d
            .items
            .iter()
            .any(|i| i.code == codes::PARSE_ADT_MULTI_PAYLOAD));
    }

    #[test]
    fn const_decl() {
        let p = parse_ok("C standardTaxRate = TaxRate 0.10\n");
        let Decl::Const(c) = &p.decls[0] else { panic!() };
        assert_eq!(c.name, "standardTaxRate");
        assert!(matches!(&c.value, Expr::Construct { name, .. } if name == "TaxRate"));
    }

    #[test]
    fn rule_with_bindings() {
        let p = parse_ok(
            "R calculateTotal price\n  Price > TotalAmount\n\n  tax : TaxAmount =\n    price * standardTaxRate\n\n  total : TotalAmount =\n    price + tax\n\n  total\n",
        );
        let Decl::Rule(r) = &p.decls[0] else { panic!() };
        assert_eq!(r.name, "calculateTotal");
        assert_eq!(r.params[0].0, "price");
        let RuleKind::Normal {
            can_fail, body, ..
        } = &r.kind
        else {
            panic!()
        };
        assert!(!can_fail);
        assert_eq!(body.bindings.len(), 2);
        assert!(matches!(&body.result, Expr::Name { path, .. } if path[0].0 == "total"));
    }

    #[test]
    fn transition_rule() {
        let p = parse_ok("R pay order\n  UnpaidOrder => PaidOrder\n");
        let Decl::Rule(r) = &p.decls[0] else { panic!() };
        assert!(matches!(r.kind, RuleKind::Transition { .. }));
    }

    #[test]
    fn rule_with_when_recursion() {
        let p = parse_ok(
            "R countdown value\n  Int > Void\n\n  when value <= 0\n    true\n      Void\n\n    false\n      countdown (value - 1)\n",
        );
        let Decl::Rule(r) = &p.decls[0] else { panic!() };
        let RuleKind::Normal { body, .. } = &r.kind else {
            panic!()
        };
        let Expr::When {
            then_block,
            else_block,
            ..
        } = &body.result
        else {
            panic!("expected when, got {:?}", body.result)
        };
        assert!(matches!(then_block.result, Expr::VoidLit(_)));
        assert!(matches!(&else_block.result, Expr::Call { args, .. } if args.len() == 1));
    }

    #[test]
    fn when_short_form() {
        let p = parse_ok(
            "R judge score\n  Score > Grade\n\n  when score >= 80\n    true  Excellent\n    false Passed\n",
        );
        let Decl::Rule(r) = &p.decls[0] else { panic!() };
        let RuleKind::Normal { body, .. } = &r.kind else {
            panic!()
        };
        assert!(matches!(body.result, Expr::When { .. }));
    }

    #[test]
    fn match_with_payload() {
        let p = parse_ok(
            "R resultText result\n  PaymentResult > Text\n\n  match result\n\n    Paid record\n      formatPayment record\n\n    Rejected reason\n      formatReason reason\n",
        );
        let Decl::Rule(r) = &p.decls[0] else { panic!() };
        let RuleKind::Normal { body, .. } = &r.kind else {
            panic!()
        };
        let Expr::Match { arms, .. } = &body.result else {
            panic!()
        };
        assert_eq!(arms.len(), 2);
        assert_eq!(arms[0].binding.as_ref().unwrap().0, "record");
    }

    #[test]
    fn flow_with_signature_and_match() {
        let p = parse_ok(
            "F handlePayment\n  PaymentResult > Void\n\n  match\n    Paid\n      createReceipt\n      printReceipt\n\n    Rejected\n      rejectionText\n      std.printLine\n",
        );
        let Decl::Flow(f) = &p.decls[0] else { panic!() };
        assert!(f.signature.is_some());
        assert_eq!(f.steps.len(), 1);
        let FlowStep::Match { arms, .. } = &f.steps[0] else {
            panic!()
        };
        assert_eq!(arms.len(), 2);
        assert_eq!(arms[0].steps.len(), 2);
    }

    #[test]
    fn flow_record_initial() {
        let p = parse_ok("F main\n  Limit {\n    value = 100\n  }\n  findPrimes\n");
        let Decl::Flow(f) = &p.decls[0] else { panic!() };
        assert_eq!(f.steps.len(), 2);
        assert!(matches!(&f.steps[0], FlowStep::Initial(Expr::Record { .. })));
    }

    #[test]
    fn flow_forbids_arithmetic() {
        let d = parse_err("F main\n  1 + 2\n  std.printLine\n");
        assert!(d.items.iter().any(|i| i.code == codes::PARSE_FLOW_FORBIDDEN));
    }

    #[test]
    fn import_after_decl_is_error() {
        let d = parse_err("T UserId [Text]\nimport std\n");
        assert!(d
            .items
            .iter()
            .any(|i| i.code == codes::PARSE_IMPORT_POSITION));
    }

    #[test]
    fn empty_with_type_arg() {
        let p = parse_ok("C x = std.empty<Int>\n");
        let Decl::Const(c) = &p.decls[0] else { panic!() };
        assert!(matches!(&c.value, Expr::Empty { .. }));
    }

    #[test]
    fn list_usage_construction() {
        let p = parse_ok("C ns = Numbers(\n  Number 10.0\n  Number 20.0\n)\n");
        let Decl::Const(c) = &p.decls[0] else { panic!() };
        let Expr::ListConstruct { elems, .. } = &c.value else {
            panic!()
        };
        assert_eq!(elems.len(), 2);
    }

    #[test]
    fn paren_construct_arg() {
        let p = parse_ok("F main\n  UnpaidOrder (OrderId \"O001\")\n  pay\n  ship\n");
        let Decl::Flow(f) = &p.decls[0] else { panic!() };
        // 括弧付き引数は要素1個の ListConstruct として読み、型解決で再解釈する
        let FlowStep::Initial(Expr::ListConstruct { name, elems, .. }) = &f.steps[0] else {
            panic!("got {:?}", f.steps[0])
        };
        assert_eq!(name, "UnpaidOrder");
        assert_eq!(elems.len(), 1);
    }

    #[test]
    fn sieve_parses() {
        let src = r#"import std

T Limit {
  value Int
}

T FilterInput {
  divisor Int
  values  List<Int>
}

R removeMultiples input
  FilterInput > List<Int> ! Error

  when std.isEmpty input.values
    true
      std.empty<Int>

    false
      value : Int =
        std.first input.values

      remaining : List<Int> =
        std.rest input.values

      filtered : List<Int> =
        removeMultiples FilterInput {
          divisor = input.divisor
          values  = remaining
        }

      when value % input.divisor == 0
        true
          filtered

        false
          std.prepend value filtered

F main
  Limit {
    value = 100
  }
  findPrimes
  primesText
  std.printLine
"#;
        let p = parse_ok(src);
        assert_eq!(p.decls.len(), 4);
    }

    #[test]
    fn qualified_record_construct() {
        let p = parse_ok("C r = std.RangeInput {\n  first = 2\n  last  = 5\n}\n");
        let Decl::Const(c) = &p.decls[0] else { panic!() };
        let Expr::Record {
            qualifier, name, ..
        } = &c.value
        else {
            panic!()
        };
        assert_eq!(qualifier.as_deref(), Some("std"));
        assert_eq!(name, "RangeInput");
    }
}
