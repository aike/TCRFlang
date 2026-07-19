//! 型検査。名前的型同一性・双方向検査 (文脈型主導の用途型演算)・
//! Error 検査・F 接続検査・match 網羅性を担当する。
//! モジュールごとに名前空間を切り替えて全モジュールを検査する。

use crate::ast::*;
use crate::builtins;
use crate::diagnostics::{codes, Diagnostics};
use crate::loader::Loaded;
use crate::resolver::resolve_type_expr;
use crate::span::Span;
use crate::types::*;
use std::collections::HashMap;

pub fn typecheck(loaded: &Loaded, env: &Env, diags: &mut Diagnostics) {
    // 定数の型をモジュール依存順 → モジュール内依存順に推論する
    let mut const_types: HashMap<(usize, String), Type> = HashMap::new();
    for &m in &env.order {
        diags.set_file(m);
        let module = &env.modules[m];
        for name in &module.const_order {
            let Some(c) = module.consts.get(name.as_str()) else {
                continue;
            };
            let ty = {
                let mut ck = Checker::new(env, m, diags, &const_types, true, false);
                ck.check_expr(&c.value, None)
            };
            const_types.insert((m, name.clone()), ty);
        }
    }

    for &m in &env.order {
        diags.set_file(m);
        for decl in &loaded.units[m].program.decls {
            match decl {
                Decl::Rule(r) => {
                    if let Some(info) = env.modules[m].rules.get(&r.name) {
                        if std::ptr::eq(info.decl, r) {
                            let mut ck =
                                Checker::new(env, m, diags, &const_types, info.can_fail, false);
                            ck.check_rule(info);
                        }
                    }
                }
                Decl::Flow(f) => {
                    if let Some(info) = env.modules[m].flows.get(&f.name) {
                        if std::ptr::eq(info.decl, f) {
                            let is_main = m == ENTRY && f.name == "main";
                            let mut ck =
                                Checker::new(env, m, diags, &const_types, info.can_fail, is_main);
                            ck.check_flow(info);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

enum TypeLookup {
    Builtin(Type),
    Id(TypeId),
    NotFound,
}

struct Checker<'a, 'p> {
    env: &'a Env<'p>,
    /// 検査中のコードが属するモジュール
    module: usize,
    diags: &'a mut Diagnostics,
    const_types: &'a HashMap<(usize, String), Type>,
    scopes: Vec<HashMap<String, Type>>,
    declared_can_fail: bool,
    is_main: bool,
}

impl<'a, 'p> Checker<'a, 'p> {
    fn new(
        env: &'a Env<'p>,
        module: usize,
        diags: &'a mut Diagnostics,
        const_types: &'a HashMap<(usize, String), Type>,
        declared_can_fail: bool,
        is_main: bool,
    ) -> Self {
        Checker {
            env,
            module,
            diags,
            const_types,
            scopes: Vec::new(),
            declared_can_fail,
            is_main,
        }
    }

    // ---- 共通ヘルパ ----

    fn display(&self, t: &Type) -> String {
        self.env.display(t)
    }

    fn mod_env(&self) -> &'a ModuleEnv<'p> {
        &self.env.modules[self.module]
    }

    fn alias(&self, q: &str) -> Option<ModuleRef> {
        self.mod_env().aliases.get(q).copied()
    }

    /// `_` 開始のトップレベル名はモジュール外に公開されない (§22)。
    fn require_public(&mut self, name: &str, span: Span) -> bool {
        if name.starts_with('_') {
            self.diags.emit_with_hint(
                codes::RESOLVE_UNDEFINED,
                span,
                format!("`{}` はモジュール外に公開されていません", name),
                "アンダースコア開始のトップレベル名は非公開です",
            );
            false
        } else {
            true
        }
    }

    /// 失敗可能な処理の使用を記録する。`! Error` 宣言がなければエラー (§15)。
    /// main は Error を終了コードへ伝播できるため除外する。
    fn mark_fail(&mut self, span: Span, what: &str) {
        if !self.declared_can_fail && !self.is_main {
            self.diags.emit_with_hint(
                codes::TYPE_MISSING_ERROR_MARK,
                span,
                format!("{}は失敗する可能性がありますが、`! Error` が宣言されていません", what),
                "シグネチャに `! Error` を追加してください",
            );
        }
    }

    fn expect_type(&mut self, actual: &Type, expected: Option<&Type>, span: Span) {
        if let Some(exp) = expected {
            if actual != exp {
                self.diags.emit(
                    codes::TYPE_MISMATCH,
                    span,
                    format!(
                        "{} が必要ですが、{} が渡されました",
                        self.display(exp),
                        self.display(actual)
                    ),
                );
            }
        }
    }

    fn lookup_scope(&self, name: &str) -> Option<&Type> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn define(&mut self, name: &str, ty: Type, span: Span) {
        if self.scopes.last().is_some_and(|s| s.contains_key(name)) {
            self.diags.emit(
                codes::TYPE_REASSIGNMENT,
                span,
                format!("値 `{}` は再代入できません (値は不変です)", name),
            );
        } else if self.lookup_scope(name).is_some() {
            self.diags.emit(
                codes::TYPE_SHADOWING,
                span,
                format!("値 `{}` は外側の束縛を隠せません (シャドーイング禁止)", name),
            );
        }
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), ty);
        }
    }

    fn lookup_type(&self, qualifier: Option<&str>, name: &str) -> TypeLookup {
        match qualifier {
            None => match name {
                "Int" => TypeLookup::Builtin(Type::Int),
                "Decimal" => TypeLookup::Builtin(Type::Decimal),
                "Text" => TypeLookup::Builtin(Type::Text),
                "Char" => TypeLookup::Builtin(Type::Char),
                "Bool" => TypeLookup::Builtin(Type::Bool),
                _ => match self.mod_env().type_names.get(name) {
                    Some(&id) => TypeLookup::Id(id),
                    None => TypeLookup::NotFound,
                },
            },
            Some(q) => match self.alias(q) {
                Some(ModuleRef::Std(module))
                    if name == "RangeInput" && builtins::has_range_input(module) =>
                {
                    TypeLookup::Id(self.env.std_types["RangeInput"])
                }
                Some(ModuleRef::User(mid)) => match self.env.modules[mid].type_names.get(name) {
                    Some(&id) => TypeLookup::Id(id),
                    None => TypeLookup::NotFound,
                },
                _ => TypeLookup::NotFound,
            },
        }
    }

    /// std 組み込みの呼び出しを std.tcrf の署名と照合する。
    /// エラー時は診断を出して None (呼び出し側は Type::Void を返す)。
    fn std_sig_check(
        &mut self,
        b: builtins::Builtin,
        label: &str,
        arg_types: &[Type],
        span: Span,
        code: crate::diagnostics::Code,
    ) -> Option<(Type, bool)> {
        let Some(sig) = self.env.std_sigs.get(builtins::sig_name(b)) else {
            self.diags.emit_with_hint(
                codes::RESOLVE_STD_DECL,
                span,
                format!(
                    "std.tcrf に `{}` の宣言がありません",
                    builtins::sig_name(b)
                ),
                "処理系付属の lib/std.tcrf が変更されていないか確認してください",
            );
            return None;
        };
        match crate::stdsig::check(sig, Some(b), arg_types, self.env) {
            Ok(r) => Some(r),
            Err(msg) => {
                self.diags.emit(code, span, format!("`{}`: {}", label, msg));
                None
            }
        }
    }

    /// 構築式のコンストラクタ名を (修飾も考慮して) 引く。
    fn ctor_hit(&self, qualifier: Option<&str>, name: &str) -> Option<(TypeId, usize)> {
        match qualifier {
            None => self.mod_env().ctors.get(name).copied(),
            Some(q) => match self.alias(q) {
                Some(ModuleRef::User(mid)) => self.env.modules[mid].ctors.get(name).copied(),
                _ => None,
            },
        }
    }

    // ---- R ----

    fn check_rule(&mut self, info: &RuleInfo<'p>) {
        match &info.decl.kind {
            RuleKind::Transition { input, output } => {
                let in_inner = self.env.usage_inner(&info.input).cloned();
                let out_inner = self.env.usage_inner(&info.output).cloned();
                match (in_inner, out_inner) {
                    (Some(a), Some(b)) if a == b => {}
                    _ => {
                        self.diags.emit_with_hint(
                            codes::TYPE_TRANSITION,
                            input.span().merge(output.span()),
                            "表現保持型遷移 (`=>`) は、内部型が同じ用途型どうしにだけ使えます",
                            "内部表現が変わる変換には `>` と R 本体を使ってください",
                        );
                    }
                }
            }
            RuleKind::Normal { body, .. } => {
                let mut root = HashMap::new();
                if let Some((p, _)) = info.decl.params.first() {
                    root.insert(p.clone(), info.input.clone());
                }
                self.scopes.push(root);
                let output = info.output.clone();
                self.check_block(body, Some(&output));
                self.scopes.pop();
            }
            RuleKind::External { .. } => unreachable!("resolver で除外済み"),
        }
    }

    fn check_block(&mut self, block: &Block, expected: Option<&Type>) -> Type {
        self.scopes.push(HashMap::new());
        for b in &block.bindings {
            let ann = b
                .ty
                .as_ref()
                .map(|t| resolve_type_expr(self.env, self.module, t, self.diags));
            let vt = self.check_expr(&b.value, ann.as_ref());
            let bound = ann.unwrap_or(vt);
            self.define(&b.name, bound, b.name_span);
        }
        let result = self.check_expr(&block.result, expected);
        self.scopes.pop();
        result
    }

    // ---- 式 ----

    fn check_expr(&mut self, e: &Expr, expected: Option<&Type>) -> Type {
        let actual = match e {
            Expr::IntLit(..) => Type::Int,
            Expr::DecimalLit(..) => Type::Decimal,
            Expr::TextLit(..) => Type::Text,
            Expr::CharLit(..) => Type::Char,
            Expr::BoolLit(..) => Type::Bool,
            Expr::VoidLit(..) => Type::Void,
            Expr::Name { path, span } => self.check_name(path, *span),
            Expr::Call { path, args, span } => self.check_call(path, args, *span),
            Expr::At { list, index, span } => {
                let lt = self.check_expr(list, None);
                self.check_expr(index, Some(&Type::Int));
                self.mark_fail(*span, "`at` ");
                match self.env.as_list_elem(&lt) {
                    Some(elem) => elem.clone(),
                    None => {
                        self.diags.emit(
                            codes::TYPE_MISMATCH,
                            list.span(),
                            format!("`at` にはリストが必要ですが、{} が渡されました", self.display(&lt)),
                        );
                        Type::Void
                    }
                }
            }
            Expr::Empty { path, elem, span } => {
                let valid = path.len() == 2
                    && matches!(
                        self.alias(&path[0].0),
                        Some(ModuleRef::Std(m)) if builtins::has_empty(m)
                    );
                if !valid {
                    self.diags.emit(
                        codes::RESOLVE_UNDEFINED,
                        *span,
                        "`empty<T>` は std または std.list の修飾名で参照してください",
                    );
                }
                Type::List(Box::new(resolve_type_expr(
                    self.env,
                    self.module,
                    elem,
                    self.diags,
                )))
            }
            Expr::Construct {
                qualifier,
                name,
                name_span,
                arg,
                span,
            } => self.check_construct(
                qualifier.as_deref(),
                name,
                *name_span,
                arg.as_deref(),
                *span,
            ),
            Expr::From {
                qualifier,
                name,
                name_span,
                value,
                span,
            } => self.check_from(qualifier.as_deref(), name, *name_span, value, *span),
            Expr::Record {
                qualifier,
                name,
                name_span,
                fields,
                ..
            } => self.check_record(qualifier.as_deref(), name, *name_span, fields),
            Expr::ListConstruct {
                qualifier,
                name,
                name_span,
                elems,
                span,
            } => self.check_list_construct(qualifier.as_deref(), name, *name_span, elems, *span),
            Expr::Unary { op, operand, span } => match op {
                UnaryOp::Not => {
                    self.check_expr(operand, Some(&Type::Bool));
                    Type::Bool
                }
                UnaryOp::Neg => {
                    let t = self.check_expr(operand, None);
                    if !t.is_numeric() {
                        self.diags.emit(
                            codes::TYPE_OPERATOR,
                            *span,
                            format!("単項 `-` は数値に使いますが、{} が渡されました", self.display(&t)),
                        );
                    }
                    t
                }
            },
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => self.check_binary(*op, left, right, expected, *span),
            Expr::When {
                cond,
                then_block,
                else_block,
                span,
            } => {
                self.check_expr(cond, Some(&Type::Bool));
                let t1 = self.check_block(then_block, expected);
                let t2 = self.check_block(else_block, expected);
                if expected.is_none() && t1 != t2 {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        *span,
                        format!(
                            "when の両分岐の型が一致しません: {} と {}",
                            self.display(&t1),
                            self.display(&t2)
                        ),
                    );
                }
                return t1; // 分岐は expected と照合済み
            }
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => return self.check_match(scrutinee, arms, expected, *span),
        };
        // when/match/binary 以外は最後に期待型と照合する
        if !matches!(e, Expr::Binary { .. }) {
            self.expect_type(&actual, expected, e.span());
        }
        actual
    }

    /// レコードフィールド参照の連鎖を辿る。
    fn walk_fields(&mut self, start: Type, rest: &[(String, Span)]) -> Type {
        let mut cur = start;
        for (seg, seg_span) in rest {
            let field_ty = match &cur {
                Type::Named(id) => match &self.env.type_def(*id).kind {
                    TypeDefKind::Record(fields) => {
                        fields.iter().find(|f| f.name == *seg).map(|f| f.ty.clone())
                    }
                    _ => None,
                },
                _ => None,
            };
            match field_ty {
                Some(ft) => cur = ft,
                None => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        *seg_span,
                        format!("{} にフィールド `{}` はありません", self.display(&cur), seg),
                    );
                    return Type::Void;
                }
            }
        }
        cur
    }

    fn check_name(&mut self, path: &[(String, Span)], span: Span) -> Type {
        let (first, first_span) = &path[0];
        if let Some(t) = self.lookup_scope(first).cloned() {
            // ローカル値 + レコードフィールド参照の連鎖
            return self.walk_fields(t, &path[1..]);
        }
        if path.len() == 1 {
            if let Some(t) = self.const_types.get(&(self.module, first.clone())) {
                return t.clone();
            }
            if self.mod_env().rules.contains_key(first) || self.mod_env().flows.contains_key(first)
            {
                self.diags.emit(
                    codes::TYPE_MISMATCH,
                    span,
                    format!("`{}` は R/F であり、値としては参照できません", first),
                );
                return Type::Void;
            }
            self.diags.emit(
                codes::RESOLVE_UNDEFINED,
                *first_span,
                format!("値 `{}` は定義されていません", first),
            );
            return Type::Void;
        }
        match self.alias(first) {
            Some(ModuleRef::Std(_)) => {
                self.diags.emit(
                    codes::TYPE_MISMATCH,
                    span,
                    "組み込み R は引数を付けて呼び出してください",
                );
                Type::Void
            }
            Some(ModuleRef::User(mid)) => {
                let (seg, seg_span) = &path[1];
                if !self.require_public(seg, *seg_span) {
                    return Type::Void;
                }
                if let Some(t) = self.const_types.get(&(mid, seg.clone())) {
                    return self.walk_fields(t.clone(), &path[2..]);
                }
                if self.env.modules[mid].rules.contains_key(seg)
                    || self.env.modules[mid].flows.contains_key(seg)
                {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!("`{}.{}` は R/F であり、値としては参照できません", first, seg),
                    );
                    return Type::Void;
                }
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    *seg_span,
                    format!("`{}.{}` は定義されていません", first, seg),
                );
                Type::Void
            }
            None => {
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    *first_span,
                    format!("値 `{}` は定義されていません", first),
                );
                Type::Void
            }
        }
    }

    fn check_rule_call(
        &mut self,
        name: &str,
        input: Type,
        output: Type,
        can_fail: bool,
        args: &[Expr],
        span: Span,
    ) -> Type {
        if args.len() != 1 {
            self.diags.emit(
                codes::TYPE_MISMATCH,
                span,
                format!("R `{}` は入力を1個だけ取ります", name),
            );
        }
        if let Some(a) = args.first() {
            self.check_expr(a, Some(&input));
        }
        for extra in args.iter().skip(1) {
            self.check_expr(extra, None);
        }
        if can_fail {
            self.mark_fail(span, &format!("R `{}` の呼び出し", name));
        }
        output
    }

    fn check_call(&mut self, path: &[(String, Span)], args: &[Expr], span: Span) -> Type {
        if path.len() == 1 {
            let (name, name_span) = &path[0];
            if let Some(info) = self.mod_env().rules.get(name) {
                let (input, output, can_fail) =
                    (info.input.clone(), info.output.clone(), info.can_fail);
                return self.check_rule_call(name, input, output, can_fail, args, span);
            }
            if self.mod_env().flows.contains_key(name) {
                self.diags.emit(
                    codes::TYPE_MISMATCH,
                    span,
                    format!("F `{}` は R の中から呼べません (F どうしは F で接続します)", name),
                );
                return Type::Void;
            }
            if self.lookup_scope(name).is_some()
                || self.const_types.contains_key(&(self.module, name.clone()))
            {
                self.diags.emit(
                    codes::TYPE_MISMATCH,
                    span,
                    format!("`{}` は値であり、呼び出せません", name),
                );
                return Type::Void;
            }
            self.diags.emit(
                codes::RESOLVE_UNDEFINED,
                *name_span,
                format!("R `{}` は定義されていません", name),
            );
            return Type::Void;
        }

        // 修飾呼び出し (組み込み / import したモジュール)
        let (q, _q_span) = &path[0];
        match self.alias(q) {
            Some(ModuleRef::Std(module)) => {
                if path.len() == 2 {
                    let (fname, fname_span) = &path[1];
                    if let Some(b) = builtins::lookup(module, fname) {
                        let arg_types: Vec<Type> =
                            args.iter().map(|a| self.check_expr(a, None)).collect();
                        let label = format!("{}.{}", q, fname);
                        match self.std_sig_check(b, &label, &arg_types, span, codes::TYPE_MISMATCH)
                        {
                            Some((out, can_fail)) => {
                                if can_fail {
                                    self.mark_fail(span, &format!("`{}.{}` ", q, fname));
                                }
                                return out;
                            }
                            None => return Type::Void,
                        }
                    }
                    // std.tcrf に宣言だけがある (処理系実装なし) 場合
                    if self.env.std_sigs.contains_key(fname.as_str()) {
                        for a in args {
                            self.check_expr(a, None);
                        }
                        self.diags.emit_with_hint(
                            codes::RESOLVE_NOT_IMPLEMENTED,
                            span,
                            format!(
                                "`{}.{}` は std.tcrf に宣言されていますが、この処理系では未実装です",
                                q, fname
                            ),
                            "std.tcrf のコメントで実装状態を確認できます",
                        );
                        return Type::Void;
                    }
                    self.diags.emit(
                        codes::RESOLVE_UNDEFINED,
                        *fname_span,
                        format!("`{}.{}` は定義されていません", q, fname),
                    );
                    return Type::Void;
                }
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    span,
                    format!("`{}` の下にさらに修飾は付けられません", q),
                );
                Type::Void
            }
            Some(ModuleRef::User(mid)) => {
                if path.len() != 2 {
                    self.diags.emit(
                        codes::RESOLVE_UNDEFINED,
                        span,
                        format!("`{}` の下にさらに修飾は付けられません", q),
                    );
                    return Type::Void;
                }
                let (fname, fname_span) = &path[1];
                if !self.require_public(fname, *fname_span) {
                    return Type::Void;
                }
                if let Some(info) = self.env.modules[mid].rules.get(fname) {
                    let (input, output, can_fail) =
                        (info.input.clone(), info.output.clone(), info.can_fail);
                    let label = format!("{}.{}", q, fname);
                    return self.check_rule_call(&label, input, output, can_fail, args, span);
                }
                if self.env.modules[mid].flows.contains_key(fname) {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!("F `{}.{}` は R の中から呼べません", q, fname),
                    );
                    return Type::Void;
                }
                if self.const_types.contains_key(&(mid, fname.clone())) {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!("`{}.{}` は値であり、呼び出せません", q, fname),
                    );
                    return Type::Void;
                }
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    *fname_span,
                    format!("`{}.{}` は定義されていません", q, fname),
                );
                Type::Void
            }
            None => {
                if self.lookup_scope(q).is_some() {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        "フィールド参照の結果は呼び出せません",
                    );
                    return Type::Void;
                }
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    span,
                    format!("修飾名 `{}` は import されていません", q),
                );
                Type::Void
            }
        }
    }

    fn check_construct(
        &mut self,
        qualifier: Option<&str>,
        name: &str,
        name_span: Span,
        arg: Option<&Expr>,
        span: Span,
    ) -> Type {
        // ADT コンストラクタ (自モジュール / import したモジュール)
        if let Some((type_id, ctor_idx)) = self.ctor_hit(qualifier, name) {
            let payload = match &self.env.type_def(type_id).kind {
                TypeDefKind::Adt(ctors) => ctors[ctor_idx].payload.clone(),
                _ => None,
            };
            match (payload, arg) {
                (Some(pt), Some(a)) => {
                    self.check_expr(a, Some(&pt));
                }
                (Some(pt), None) => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!(
                            "コンストラクタ `{}` にはペイロード ({}) が必要です",
                            name,
                            self.display(&pt)
                        ),
                    );
                }
                (None, Some(_)) => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!("コンストラクタ `{}` はペイロードを持ちません", name),
                    );
                }
                (None, None) => {}
            }
            return Type::Named(type_id);
        }
        match self.lookup_type(qualifier, name) {
            TypeLookup::Builtin(t) => {
                match arg {
                    Some(a) => {
                        self.check_expr(a, Some(&t));
                    }
                    None => {
                        self.diags.emit(
                            codes::TYPE_MISMATCH,
                            span,
                            format!("`{}` の構築には値が必要です", name),
                        );
                    }
                }
                t
            }
            TypeLookup::Id(id) => match &self.env.type_def(id).kind {
                TypeDefKind::Usage(inner) => {
                    let inner = inner.clone();
                    match arg {
                        Some(a) => {
                            self.check_expr(a, Some(&inner));
                        }
                        None => {
                            self.diags.emit(
                                codes::TYPE_MISMATCH,
                                span,
                                format!(
                                    "用途型 `{}` の構築には内部値 ({}) が必要です",
                                    name,
                                    self.display(&inner)
                                ),
                            );
                        }
                    }
                    Type::Named(id)
                }
                TypeDefKind::Record(_) => {
                    self.diags.emit(
                        codes::TYPE_RECORD_FIELDS,
                        span,
                        format!("レコード型 `{}` の構築には `{{ フィールド = 値 }}` を使います", name),
                    );
                    Type::Named(id)
                }
                _ => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!("`{}` はこの形では構築できません", name),
                    );
                    Type::Named(id)
                }
            },
            TypeLookup::NotFound => {
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    name_span,
                    format!("型またはコンストラクタ `{}` は定義されていません", name),
                );
                Type::Void
            }
        }
    }

    /// `A from x`: A と x の型が内部型を同じくする用途型どうしのとき、
    /// x の値をそのまま持つ A 型の値を作る (表現保持変換)。
    fn check_from(
        &mut self,
        qualifier: Option<&str>,
        name: &str,
        name_span: Span,
        value: &Expr,
        span: Span,
    ) -> Type {
        let id = match self.lookup_type(qualifier, name) {
            TypeLookup::Id(id) => id,
            TypeLookup::Builtin(_) => {
                self.diags.emit_with_hint(
                    codes::TYPE_TRANSITION,
                    name_span,
                    format!("`from` の対象型は用途型でなければなりませんが、`{}` は組み込み型です", name),
                    "組み込み型の値は `型名 値` で構築してください",
                );
                self.check_expr(value, None);
                return Type::Void;
            }
            TypeLookup::NotFound => {
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    name_span,
                    format!("型 `{}` は定義されていません", name),
                );
                self.check_expr(value, None);
                return Type::Void;
            }
        };
        let TypeDefKind::Usage(inner) = &self.env.type_def(id).kind else {
            self.diags.emit(
                codes::TYPE_TRANSITION,
                name_span,
                format!("`from` の対象型は用途型でなければなりませんが、`{}` は用途型ではありません", name),
            );
            self.check_expr(value, None);
            return Type::Named(id);
        };
        let inner = inner.clone();
        let vt = self.check_expr(value, None);
        match self.env.usage_inner(&vt) {
            Some(vi) if *vi == inner => {}
            _ => {
                self.diags.emit_with_hint(
                    codes::TYPE_TRANSITION,
                    span,
                    format!(
                        "`from` は内部型が同じ用途型どうしにだけ使えます: {} の内部型は {} ですが、{} が渡されました",
                        self.env.type_def(id).name,
                        self.display(&inner),
                        self.display(&vt)
                    ),
                    "内部表現が変わる変換には本体を持つ R を使ってください",
                );
            }
        }
        Type::Named(id)
    }

    fn check_record(
        &mut self,
        qualifier: Option<&str>,
        name: &str,
        name_span: Span,
        inits: &[RecordFieldInit],
    ) -> Type {
        let id = match self.lookup_type(qualifier, name) {
            TypeLookup::Id(id) => id,
            TypeLookup::Builtin(_) | TypeLookup::NotFound => {
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    name_span,
                    format!("レコード型 `{}` は定義されていません", name),
                );
                for init in inits {
                    self.check_expr(&init.value, None);
                }
                return Type::Void;
            }
        };
        let TypeDefKind::Record(fields) = &self.env.type_def(id).kind else {
            self.diags.emit(
                codes::TYPE_RECORD_FIELDS,
                name_span,
                format!("`{}` はレコード型ではありません", name),
            );
            for init in inits {
                self.check_expr(&init.value, None);
            }
            return Type::Named(id);
        };
        let field_types: Vec<(String, Type)> =
            fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect();

        let mut seen: Vec<&str> = Vec::new();
        for init in inits {
            match field_types.iter().find(|(n, _)| *n == init.name) {
                Some((_, ft)) => {
                    if seen.contains(&init.name.as_str()) {
                        self.diags.emit(
                            codes::TYPE_RECORD_FIELDS,
                            init.name_span,
                            format!("フィールド `{}` が重複しています", init.name),
                        );
                    }
                    let ft = ft.clone();
                    self.check_expr(&init.value, Some(&ft));
                }
                None => {
                    self.diags.emit(
                        codes::TYPE_RECORD_FIELDS,
                        init.name_span,
                        format!("`{}` に余分なフィールド `{}` があります", name, init.name),
                    );
                    self.check_expr(&init.value, None);
                }
            }
            seen.push(init.name.as_str());
        }
        for (fname, _) in &field_types {
            if !inits.iter().any(|i| i.name == *fname) {
                self.diags.emit(
                    codes::TYPE_RECORD_FIELDS,
                    name_span,
                    format!("フィールド `{}` が指定されていません", fname),
                );
            }
        }
        Type::Named(id)
    }

    fn check_list_construct(
        &mut self,
        qualifier: Option<&str>,
        name: &str,
        name_span: Span,
        elems: &[Expr],
        span: Span,
    ) -> Type {
        // `Ctor (expr)` — 括弧付きペイロードの ADT 構築として再解釈
        if elems.len() == 1 && self.ctor_hit(qualifier, name).is_some() {
            return self.check_construct(qualifier, name, name_span, Some(&elems[0]), span);
        }
        match self.lookup_type(qualifier, name) {
            TypeLookup::Builtin(t) => {
                // `Text ("x")` のような括弧付き構築
                if elems.len() == 1 {
                    self.check_expr(&elems[0], Some(&t));
                } else {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!("`{}` の構築には値が1個必要です", name),
                    );
                }
                t
            }
            TypeLookup::Id(id) => {
                let def = self.env.type_def(id);
                match &def.kind {
                    TypeDefKind::Usage(inner) => match inner.clone() {
                        Type::List(elem) => {
                            // リスト用途型構築
                            for e in elems {
                                self.check_expr(e, Some(&elem));
                            }
                            Type::Named(id)
                        }
                        other => {
                            // 括弧付き引数の用途型構築
                            if elems.len() == 1 {
                                self.check_expr(&elems[0], Some(&other));
                            } else {
                                self.diags.emit(
                                    codes::TYPE_MISMATCH,
                                    span,
                                    format!(
                                        "用途型 `{}` の内部型はリストではないため、複数要素では構築できません",
                                        name
                                    ),
                                );
                            }
                            Type::Named(id)
                        }
                    },
                    TypeDefKind::Record(_) => {
                        self.diags.emit(
                            codes::TYPE_RECORD_FIELDS,
                            span,
                            format!("レコード型 `{}` の構築には `{{ フィールド = 値 }}` を使います", name),
                        );
                        Type::Named(id)
                    }
                    _ => {
                        self.diags.emit(
                            codes::TYPE_MISMATCH,
                            span,
                            format!("`{}` はこの形では構築できません", name),
                        );
                        Type::Named(id)
                    }
                }
            }
            TypeLookup::NotFound => {
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    name_span,
                    format!("型 `{}` は定義されていません", name),
                );
                Type::Void
            }
        }
    }

    fn check_binary(
        &mut self,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        use BinOp::*;
        match op {
            And | Or => {
                self.check_expr(left, Some(&Type::Bool));
                self.check_expr(right, Some(&Type::Bool));
                let t = Type::Bool;
                self.expect_type(&t, expected, span);
                t
            }
            Lt | Le | Gt | Ge | Eq | Ne => {
                let lt = self.check_expr(left, None);
                let rt = self.check_expr(right, None);
                let li = self.env.usage_inner(&lt).unwrap_or(&lt).clone();
                let ri = self.env.usage_inner(&rt).unwrap_or(&rt).clone();
                let comparable = if matches!(op, Eq | Ne) {
                    li.is_equatable_scalar()
                } else {
                    li.is_ordered_scalar()
                };
                if li != ri || !comparable {
                    self.diags.emit(
                        codes::TYPE_OPERATOR,
                        span,
                        format!(
                            "演算子 `{}` は {} と {} には適用できません",
                            op.symbol(),
                            self.display(&lt),
                            self.display(&rt)
                        ),
                    );
                }
                let t = Type::Bool;
                self.expect_type(&t, expected, span);
                t
            }
            Add | Sub | Mul | Div | Rem => {
                let lt = self.check_expr(left, None);
                let rt = self.check_expr(right, None);

                // 組み込み数値どうし
                if lt == rt && lt.is_numeric() {
                    if op == Rem && lt == Type::Decimal {
                        self.diags.emit(
                            codes::TYPE_OPERATOR,
                            span,
                            "`%` は Decimal には使えません",
                        );
                    }
                    if matches!(op, Div | Rem) {
                        self.mark_fail(span, "0除算の可能性がある演算");
                    }
                    self.expect_type(&lt, expected, span);
                    return lt;
                }

                // 用途型演算 (文脈型主導): 両辺の内部型が同一数値型なら許可し、
                // 結果型は期待型 (束縛の型注釈など) から決める。
                let li = self.env.usage_inner(&lt).unwrap_or(&lt).clone();
                let ri = self.env.usage_inner(&rt).unwrap_or(&rt).clone();
                let has_usage =
                    self.env.usage_inner(&lt).is_some() || self.env.usage_inner(&rt).is_some();
                if has_usage && li == ri && li.is_numeric() {
                    if op == Rem && li == Type::Decimal {
                        self.diags.emit(
                            codes::TYPE_OPERATOR,
                            span,
                            "`%` は Decimal には使えません",
                        );
                    }
                    if matches!(op, Div | Rem) {
                        self.mark_fail(span, "0除算の可能性がある演算");
                    }
                    return match expected {
                        Some(exp) if self.env.usage_inner(exp) == Some(&li) => exp.clone(),
                        Some(exp) => {
                            self.diags.emit(
                                codes::TYPE_OPERATOR,
                                span,
                                format!(
                                    "用途型演算の結果を {} にはできません (内部型 {} の用途型が必要です)",
                                    self.display(exp),
                                    self.display(&li)
                                ),
                            );
                            exp.clone()
                        }
                        None => {
                            self.diags.emit_with_hint(
                                codes::TYPE_CANNOT_INFER,
                                span,
                                "用途型どうしの演算の結果型を推論できません",
                                "`名前 : 型 = 式` の形で結果の型注釈を付けてください",
                            );
                            li
                        }
                    };
                }

                self.diags.emit(
                    codes::TYPE_OPERATOR,
                    span,
                    format!(
                        "演算子 `{}` は {} と {} には適用できません",
                        op.symbol(),
                        self.display(&lt),
                        self.display(&rt)
                    ),
                );
                self.expect_type(&lt, expected, span);
                lt
            }
        }
    }

    fn check_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        let st = self.check_expr(scrutinee, None);
        let adt = match &st {
            Type::Named(id) => match &self.env.type_def(*id).kind {
                TypeDefKind::Adt(ctors) => Some((
                    *id,
                    ctors
                        .iter()
                        .map(|c| (c.name.clone(), c.payload.clone()))
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            },
            _ => None,
        };
        let Some((_, ctors)) = adt else {
            self.diags.emit(
                codes::TYPE_MISMATCH,
                scrutinee.span(),
                format!(
                    "match の対象は代数データ型でなければなりませんが、{} が渡されました",
                    self.display(&st)
                ),
            );
            return expected.cloned().unwrap_or(Type::Void);
        };

        let mut seen: Vec<&str> = Vec::new();
        let mut result: Option<Type> = None;
        for arm in arms {
            let ctor = ctors.iter().find(|(n, _)| *n == arm.ctor);
            let payload = match ctor {
                Some((_, p)) => {
                    if seen.contains(&arm.ctor.as_str()) {
                        self.diags.emit(
                            codes::TYPE_MATCH_DUPLICATE,
                            arm.ctor_span,
                            format!("分岐 `{}` が重複しています", arm.ctor),
                        );
                    }
                    seen.push(arm.ctor.as_str());
                    p.clone()
                }
                None => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        arm.ctor_span,
                        format!(
                            "{} にコンストラクタ `{}` はありません",
                            self.display(&st),
                            arm.ctor
                        ),
                    );
                    None
                }
            };

            let mut arm_scope = HashMap::new();
            match (&arm.binding, payload) {
                (Some((bname, bspan)), Some(pt)) => {
                    if self.lookup_scope(bname).is_some() {
                        self.diags.emit(
                            codes::TYPE_SHADOWING,
                            *bspan,
                            format!("値 `{}` は外側の束縛を隠せません (シャドーイング禁止)", bname),
                        );
                    }
                    arm_scope.insert(bname.clone(), pt);
                }
                (Some((bname, bspan)), None) => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        *bspan,
                        format!("コンストラクタ `{}` にペイロードはありません", bname),
                    );
                }
                (None, _) => {}
            }
            self.scopes.push(arm_scope);
            let t = self.check_block(&arm.body, expected);
            self.scopes.pop();

            match &result {
                None => result = Some(t),
                Some(prev) if expected.is_none() && *prev != t => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        span,
                        format!(
                            "match の分岐の型が一致しません: {} と {}",
                            self.display(prev),
                            self.display(&t)
                        ),
                    );
                }
                _ => {}
            }
        }

        let missing: Vec<&str> = ctors
            .iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| !seen.contains(n))
            .collect();
        if !missing.is_empty() {
            self.diags.emit(
                codes::TYPE_MATCH_NOT_EXHAUSTIVE,
                span,
                format!(
                    "match がすべてのコンストラクタを網羅していません: {} が不足しています",
                    missing.join(", ")
                ),
            );
        }
        result.or_else(|| expected.cloned()).unwrap_or(Type::Void)
    }

    // ---- F ----

    fn check_flow(&mut self, info: &FlowInfo<'p>) {
        let final_t =
            self.check_flow_steps(&info.decl.steps, info.input.clone(), true);
        if final_t != info.output {
            self.diags.emit(
                codes::TYPE_FLOW_MISMATCH,
                info.decl.name_span,
                format!(
                    "F `{}` の最終出力型 {} がシグネチャの出力型 {} と一致しません",
                    info.decl.name,
                    self.display(&final_t),
                    self.display(&info.output)
                ),
            );
        }
    }

    fn check_flow_steps(&mut self, steps: &[FlowStep], start: Type, mut at_start: bool) -> Type {
        let mut current = start;
        for step in steps {
            match step {
                FlowStep::Initial(expr) => {
                    if !at_start || current != Type::Void {
                        self.diags.emit(
                            codes::TYPE_FLOW_MISMATCH,
                            expr.span(),
                            "値の構築はフローの先頭でだけできます",
                        );
                    }
                    current = self.check_expr(expr, None);
                }
                FlowStep::Call { path, span } => {
                    current = self.apply_flow_call(path, *span, current, at_start);
                }
                FlowStep::Match { arms, span } => {
                    current = self.check_flow_match(arms, *span, current);
                }
            }
            at_start = false;
        }
        current
    }

    /// R/F の接続 1 段分: 前段出力型と入力型の一致検査。
    fn check_connect(
        &mut self,
        kind: &str,
        name: &str,
        input: Type,
        output: Type,
        can_fail: bool,
        current: &Type,
        span: Span,
    ) -> Type {
        if *current != input {
            self.diags.emit(
                codes::TYPE_FLOW_MISMATCH,
                span,
                format!(
                    "前段の出力型 {} が {} `{}` の入力型 {} と一致しません",
                    self.display(current),
                    kind,
                    name,
                    self.display(&input)
                ),
            );
        }
        if can_fail {
            self.mark_fail(span, &format!("{} `{}` の接続", kind, name));
        }
        output
    }

    fn apply_flow_call(
        &mut self,
        path: &[(String, Span)],
        span: Span,
        current: Type,
        at_start: bool,
    ) -> Type {
        if path.len() == 1 {
            let (name, name_span) = &path[0];
            if let Some(info) = self.mod_env().rules.get(name) {
                let (input, output, can_fail) =
                    (info.input.clone(), info.output.clone(), info.can_fail);
                return self.check_connect("R", name, input, output, can_fail, &current, span);
            }
            if let Some(info) = self.mod_env().flows.get(name) {
                let (input, output, can_fail) =
                    (info.input.clone(), info.output.clone(), info.can_fail);
                return self.check_connect("F", name, input, output, can_fail, &current, span);
            }
            if let Some(t) = self.const_types.get(&(self.module, name.clone())) {
                if !at_start || current != Type::Void {
                    self.diags.emit(
                        codes::TYPE_FLOW_MISMATCH,
                        span,
                        "定数はフローの最初の値としてだけ使えます",
                    );
                }
                return t.clone();
            }
            self.diags.emit(
                codes::RESOLVE_UNDEFINED,
                *name_span,
                format!("R/F `{}` は定義されていません", name),
            );
            return current;
        }

        let (q, _) = &path[0];
        match self.alias(q) {
            Some(ModuleRef::Std(module)) => {
                if path.len() == 2 {
                    let (fname, fname_span) = &path[1];
                    if let Some(b) = builtins::lookup(module, fname) {
                        // 複数引数の組み込みは F のステップとしては使えない
                        if self
                            .env
                            .std_sigs
                            .get(builtins::sig_name(b))
                            .is_some_and(|sig| sig.inputs.len() != 1)
                        {
                            self.diags.emit(
                                codes::TYPE_FLOW_MISMATCH,
                                span,
                                format!(
                                    "`{}.{}` は複数の引数を取るため、F のステップとしては使えません",
                                    q, fname
                                ),
                            );
                            return current;
                        }
                        let label = format!("{}.{}", q, fname);
                        let args = [current.clone()];
                        match self.std_sig_check(b, &label, &args, span, codes::TYPE_FLOW_MISMATCH)
                        {
                            Some((out, can_fail)) => {
                                if can_fail {
                                    self.mark_fail(span, &format!("`{}.{}` の接続", q, fname));
                                }
                                return out;
                            }
                            None => return current,
                        }
                    }
                    if self.env.std_sigs.contains_key(fname.as_str()) {
                        self.diags.emit_with_hint(
                            codes::RESOLVE_NOT_IMPLEMENTED,
                            span,
                            format!(
                                "`{}.{}` は std.tcrf に宣言されていますが、この処理系では未実装です",
                                q, fname
                            ),
                            "std.tcrf のコメントで実装状態を確認できます",
                        );
                        return current;
                    }
                    self.diags.emit(
                        codes::RESOLVE_UNDEFINED,
                        *fname_span,
                        format!("`{}.{}` は定義されていません", q, fname),
                    );
                    return current;
                }
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    span,
                    format!("`{}` の下にさらに修飾は付けられません", q),
                );
                current
            }
            Some(ModuleRef::User(mid)) => {
                if path.len() != 2 {
                    self.diags.emit(
                        codes::RESOLVE_UNDEFINED,
                        span,
                        format!("`{}` の下にさらに修飾は付けられません", q),
                    );
                    return current;
                }
                let (fname, fname_span) = &path[1];
                if !self.require_public(fname, *fname_span) {
                    return current;
                }
                let label = format!("{}.{}", q, fname);
                if let Some(info) = self.env.modules[mid].rules.get(fname) {
                    let (input, output, can_fail) =
                        (info.input.clone(), info.output.clone(), info.can_fail);
                    return self.check_connect("R", &label, input, output, can_fail, &current, span);
                }
                if let Some(info) = self.env.modules[mid].flows.get(fname) {
                    let (input, output, can_fail) =
                        (info.input.clone(), info.output.clone(), info.can_fail);
                    return self.check_connect("F", &label, input, output, can_fail, &current, span);
                }
                if let Some(t) = self.const_types.get(&(mid, fname.clone())) {
                    if !at_start || current != Type::Void {
                        self.diags.emit(
                            codes::TYPE_FLOW_MISMATCH,
                            span,
                            "定数はフローの最初の値としてだけ使えます",
                        );
                    }
                    return t.clone();
                }
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    *fname_span,
                    format!("`{}.{}` は定義されていません", q, fname),
                );
                current
            }
            None => {
                self.diags.emit(
                    codes::RESOLVE_UNDEFINED,
                    span,
                    format!("修飾名 `{}` は import されていません", q),
                );
                current
            }
        }
    }

    fn check_flow_match(&mut self, arms: &[FlowMatchArm], span: Span, current: Type) -> Type {
        let ctors = match &current {
            Type::Named(id) => match &self.env.type_def(*id).kind {
                TypeDefKind::Adt(cs) => Some(
                    cs.iter()
                        .map(|c| (c.name.clone(), c.payload.clone()))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            },
            _ => None,
        };
        let Some(ctors) = ctors else {
            self.diags.emit(
                codes::TYPE_MISMATCH,
                span,
                format!(
                    "F の match は代数データ型に対してだけ使えますが、現在の型は {} です",
                    self.display(&current)
                ),
            );
            return current;
        };

        let mut seen: Vec<&str> = Vec::new();
        let mut result: Option<Type> = None;
        for arm in arms {
            let payload = match ctors.iter().find(|(n, _)| *n == arm.ctor) {
                Some((_, p)) => {
                    if seen.contains(&arm.ctor.as_str()) {
                        self.diags.emit(
                            codes::TYPE_MATCH_DUPLICATE,
                            arm.ctor_span,
                            format!("分岐 `{}` が重複しています", arm.ctor),
                        );
                    }
                    seen.push(arm.ctor.as_str());
                    p.clone()
                }
                None => {
                    self.diags.emit(
                        codes::TYPE_MISMATCH,
                        arm.ctor_span,
                        format!(
                            "{} にコンストラクタ `{}` はありません",
                            self.display(&current),
                            arm.ctor
                        ),
                    );
                    None
                }
            };
            let start = payload.unwrap_or(Type::Void);
            let t = self.check_flow_steps(&arm.steps, start, true);
            match &result {
                None => result = Some(t),
                Some(prev) if *prev != t => {
                    self.diags.emit(
                        codes::TYPE_FLOW_MISMATCH,
                        arm.ctor_span,
                        format!(
                            "match 分岐の最終出力型が一致しません: {} と {}",
                            self.display(prev),
                            self.display(&t)
                        ),
                    );
                }
                _ => {}
            }
        }

        let missing: Vec<&str> = ctors
            .iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| !seen.contains(n))
            .collect();
        if !missing.is_empty() {
            self.diags.emit(
                codes::TYPE_MATCH_NOT_EXHAUSTIVE,
                span,
                format!(
                    "match がすべてのコンストラクタを網羅していません: {} が不足しています",
                    missing.join(", ")
                ),
            );
        }
        result.unwrap_or(current)
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::{codes, Code, Diagnostics};
    use crate::loader::{self, Loaded};
    use crate::resolver;

    fn compile(src: &str) -> (Diagnostics, Loaded) {
        let mut diags = Diagnostics::new();
        let loaded = loader::load_str("t.tcrf", src, &mut diags);
        let env = resolver::resolve(&loaded, &mut diags);
        super::typecheck(&loaded, &env, &mut diags);
        (diags, loaded)
    }

    fn assert_ok(src: &str) {
        let (d, l) = compile(src);
        assert!(
            d.is_empty(),
            "unexpected diagnostics:\n{}",
            d.render(&l.units[0].file)
        );
    }

    fn assert_has(src: &str, code: Code) {
        let (d, l) = compile(src);
        assert!(
            d.items.iter().any(|i| i.code == code),
            "expected {} but got:\n{}",
            code.0,
            d.render(&l.units[0].file)
        );
    }

    const TAX: &str = r#"import std

T Price [Decimal]
T TaxRate [Decimal]
T TaxAmount [Decimal]
T TotalAmount [Decimal]

C standardTaxRate = TaxRate 0.10

R calculateTotal price
  Price > TotalAmount

  tax : TaxAmount =
    price * standardTaxRate

  total : TotalAmount =
    price + tax

  total

R totalText total
  TotalAmount > Text

  std.decimal total

F main
  Price 1000.0
  calculateTotal
  totalText
  std.printLine
"#;

    #[test]
    fn tax_example_checks() {
        assert_ok(TAX);
    }

    #[test]
    fn hello_world_checks() {
        assert_ok("import std\n\nF main\n  Text \"Hello, World!\"\n  std.printLine\n");
    }

    #[test]
    fn grade_example_checks() {
        assert_ok(
            r#"import std

T Score [Int]

T Grade
  | Excellent
  | Passed
  | Failed

R judge score
  Score > Grade

  when score >= 80
    true
      Excellent

    false
      when score >= 60
        true
          Passed

        false
          Failed

R gradeText grade
  Grade > Text

  match grade

    Excellent
      Text "Excellent"

    Passed
      Text "Passed"

    Failed
      Text "Failed"

F main
  Score 75
  judge
  gradeText
  std.printLine
"#,
        );
    }

    #[test]
    fn transition_example_checks() {
        assert_ok(
            r#"T OrderId [Text]

T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]

R pay order
  UnpaidOrder => PaidOrder

R ship order
  PaidOrder => ShippedOrder

R report order
  ShippedOrder > Void

  Void

F main
  UnpaidOrder (OrderId "O001")
  pay
  ship
  report
"#,
        );
    }

    #[test]
    fn ship_unpaid_is_flow_mismatch() {
        assert_has(
            r#"T OrderId [Text]
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]

R ship order
  PaidOrder => ShippedOrder

R report order
  ShippedOrder > Void
  Void

F main
  UnpaidOrder (OrderId "O001")
  ship
  report
"#,
            codes::TYPE_FLOW_MISMATCH,
        );
    }

    #[test]
    fn from_expression_checks() {
        // `A from x` と、それによる => 相当の明示的な書き換え
        assert_ok(
            r#"T OrderId [Text]
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]

R pay order
  UnpaidOrder > PaidOrder

  paid : PaidOrder =
    PaidOrder from order

  paid

F main
  UnpaidOrder (OrderId "O001")
  pay
  report

R report order
  PaidOrder > Void

  Void
"#,
        );
    }

    #[test]
    fn from_needs_same_inner() {
        assert_has(
            r#"T A [Text]
T B [Int]

R conv x
  B > A

  A from x

F main
  Void
"#,
            codes::TYPE_TRANSITION,
        );
    }

    #[test]
    fn from_target_must_be_usage_type() {
        assert_has(
            r#"T P {
  a Int
}

T Q [Int]

R conv x
  Q > P

  P from x

F main
  Void
"#,
            codes::TYPE_TRANSITION,
        );
    }

    #[test]
    fn from_operand_must_be_usage_type() {
        // 素の Int からの from は不可 (通常の構築 `A x` を使う)
        assert_has(
            r#"T A [Int]
T B [Int]

R conv x
  Int > A

  A from x

F main
  Void
"#,
            codes::TYPE_TRANSITION,
        );
    }

    #[test]
    fn transition_needs_same_inner() {
        assert_has(
            "T A [Text]\nT B [Int]\n\nR conv x\n  A => B\n\nF main\n  Void\n",
            codes::TYPE_TRANSITION,
        );
    }

    #[test]
    fn missing_error_mark() {
        assert_has(
            r#"import std

R firstOf values
  List<Int> > Int

  std.first values

F main
  Void
"#,
            codes::TYPE_MISSING_ERROR_MARK,
        );
    }

    #[test]
    fn main_can_call_failable_without_mark() {
        assert_ok(
            r#"import std

T Limit [Int]

R build limit
  Limit > List<Int> ! Error

  std.inclusive std.RangeInput {
    first = 1
    last  = 3
  }

R listText values
  List<Int> > Text

  std.intList values

F main
  Limit 3
  build
  listText
  std.printLine
"#,
        );
    }

    #[test]
    fn non_exhaustive_match() {
        assert_has(
            r#"T Grade
  | Excellent
  | Passed
  | Failed

R gradeText grade
  Grade > Text

  match grade

    Excellent
      Text "E"

F main
  Void
"#,
            codes::TYPE_MATCH_NOT_EXHAUSTIVE,
        );
    }

    #[test]
    fn shadowing_is_error() {
        assert_has(
            "R f x\n  Int > Int\n\n  x = 10\n\n  x\n\nF main\n  Void\n",
            codes::TYPE_SHADOWING,
        );
    }

    #[test]
    fn reassignment_is_error() {
        assert_has(
            "R f x\n  Int > Int\n\n  y = 10\n  y = 20\n\n  y\n\nF main\n  Void\n",
            codes::TYPE_REASSIGNMENT,
        );
    }

    #[test]
    fn record_missing_field() {
        assert_has(
            r#"T P {
  a Int
  b Int
}

R f x
  Int > P

  P {
    a = 1
  }

F main
  Void
"#,
            codes::TYPE_RECORD_FIELDS,
        );
    }

    #[test]
    fn usage_arithmetic_needs_annotation() {
        assert_has(
            r#"T Price [Decimal]
T TaxRate [Decimal]

C rate = TaxRate 0.10

R f price
  Price > Decimal

  x =
    price * rate

  0.0

F main
  Void
"#,
            codes::TYPE_CANNOT_INFER,
        );
    }

    #[test]
    fn missing_main() {
        assert_has("T A [Int]\n", codes::TYPE_MAIN);
    }

    #[test]
    fn duplicate_import() {
        assert_has(
            "import std\nimport std\n\nF main\n  Void\n",
            codes::RESOLVE_IMPORT_CONFLICT,
        );
    }

    #[test]
    fn recursive_type_is_error() {
        assert_has(
            "T Node {\n  next Node\n}\n\nF main\n  Void\n",
            codes::RESOLVE_RECURSIVE_TYPE,
        );
    }

    #[test]
    fn const_cycle_is_error() {
        assert_has(
            "C a = b\nC b = a\n\nF main\n  Void\n",
            codes::RESOLVE_CONST_CYCLE,
        );
    }

    #[test]
    fn sieve_example_checks() {
        assert_ok(
            r#"import std

T Limit {
  value Int
}

T Candidates {
  values List<Int>
}

T Primes {
  values List<Int>
}

T FilterInput {
  divisor Int
  values  List<Int>
}

T RangeInput {
  first Int
  last  Int
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

R sieve candidates
  Candidates > Primes ! Error

  when std.isEmpty candidates.values
    true
      Primes {
        values = std.empty<Int>
      }

    false
      prime : Int =
        std.first candidates.values

      remaining : List<Int> =
        std.rest candidates.values

      filtered : List<Int> =
        removeMultiples FilterInput {
          divisor = prime
          values  = remaining
        }

      restPrimes : Primes =
        sieve Candidates {
          values = filtered
        }

      Primes {
        values =
          std.prepend prime restPrimes.values
      }

R findPrimes limit
  Limit > Primes ! Error

  values : List<Int> =
    std.inclusive RangeInput {
      first = 2
      last  = limit.value
    }

  sieve Candidates {
    values = values
  }

R primesText primes
  Primes > Text

  std.intList primes.values

F main
  Limit {
    value = 100
  }
  findPrimes
  primesText
  std.printLine
"#,
        );
    }

    #[test]
    fn flow_match_checks() {
        assert_ok(
            r#"import std

T PaymentRecord [Text]
T RejectionReason [Text]

T PaymentResult
  | Paid PaymentRecord
  | Rejected RejectionReason

R createReceipt record
  PaymentRecord > Text

  Text "receipt"

R rejectionText reason
  RejectionReason > Text

  Text "rejected"

F handlePayment
  PaymentResult > Void

  match
    Paid
      createReceipt
      std.printLine

    Rejected
      rejectionText
      std.printLine

F main
  Paid (PaymentRecord "P1")
  handlePayment
"#,
        );
    }
}
