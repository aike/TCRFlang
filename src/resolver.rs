//! 名前解決: import・型定義・トップレベル名 (C/R/F) の収集と検査。
//! モジュール (loader::Loaded) を依存順に処理し、モジュール別名前空間を構築する。

use crate::ast::*;
use crate::diagnostics::{codes, Diagnostics};
use crate::loader::{Loaded, ModuleUnit};
use crate::span::Span;
use crate::types::*;
use crate::builtins;
use std::collections::{HashMap, HashSet};

const BUILTIN_TYPE_NAMES: [&str; 5] = ["Int", "Decimal", "Text", "Char", "Bool"];

pub fn resolve<'p>(loaded: &'p Loaded, diags: &mut Diagnostics) -> Env<'p> {
    let n = loaded.units.len();
    let mut env = Env {
        types: Vec::new(),
        std_types: HashMap::new(),
        modules: (0..n).map(|_| ModuleEnv::default()).collect(),
        module_names: loaded.units.iter().map(|u| u.name.clone()).collect(),
        order: loaded.order.clone(),
        std_sigs: crate::stdsig::StdSigs::new(),
    };

    register_std_types(&mut env);
    // 標準ライブラリの署名表を宣言ファイル std.tcrf から構築する
    if let Some(id) = loaded.std_unit {
        diags.set_file(id);
        env.std_sigs = crate::stdsig::build(&loaded.units[id], diags);
    }
    // 依存順 (依存が先) に処理するため、各モジュールの解決時には
    // import 先モジュールの名前空間が完成している
    for &m in &loaded.order {
        let unit = &loaded.units[m];
        diags.set_file(m);
        resolve_imports(unit, m, &mut env, diags);
        collect_type_names(unit, m, &mut env, diags);
        resolve_type_bodies(unit, m, &mut env, diags);
        collect_ctors(unit, m, &mut env, diags);
        collect_values(unit, m, &mut env, diags);
        check_consts(unit, m, &mut env, diags);
        check_main(m, &mut env, diags);
    }
    check_recursive_types(&mut env, diags);
    env
}

fn resolve_imports<'p>(unit: &'p ModuleUnit, m: usize, env: &mut Env<'p>, diags: &mut Diagnostics) {
    for (im, target) in unit.program.imports.iter().zip(&unit.import_targets) {
        // 解決失敗は loader が診断済み
        let Some(target) = target else { continue };
        let qual = im
            .alias
            .clone()
            .unwrap_or_else(|| im.path.last().unwrap().clone());
        if env.modules[m].aliases.contains_key(&qual) {
            diags.emit(
                codes::RESOLVE_IMPORT_CONFLICT,
                im.span,
                format!("修飾名 `{}` は既に導入されています", qual),
            );
            continue;
        }
        env.modules[m].aliases.insert(qual, *target);
    }
}

fn register_std_types(env: &mut Env) {
    let id = env.types.len();
    env.types.push(TypeDef {
        name: "RangeInput".to_string(),
        module: ENTRY,
        span: Span::new(0, 0),
        kind: TypeDefKind::Record(vec![
            FieldDef {
                name: "first".to_string(),
                ty: Type::Int,
            },
            FieldDef {
                name: "last".to_string(),
                ty: Type::Int,
            },
        ]),
    });
    env.std_types.insert("RangeInput", id);
}

fn collect_type_names<'p>(unit: &'p ModuleUnit, m: usize, env: &mut Env<'p>, diags: &mut Diagnostics) {
    for decl in &unit.program.decls {
        let Decl::Type(t) = decl else { continue };
        if BUILTIN_TYPE_NAMES.contains(&t.name.as_str()) {
            diags.emit(
                codes::RESOLVE_DUPLICATE,
                t.name_span,
                format!("組み込み型 `{}` は再定義できません", t.name),
            );
            continue;
        }
        if env.modules[m].type_names.contains_key(&t.name) {
            diags.emit(
                codes::RESOLVE_DUPLICATE,
                t.name_span,
                format!("型 `{}` は既に定義されています", t.name),
            );
            continue;
        }
        let id = env.types.len();
        env.types.push(TypeDef {
            name: t.name.clone(),
            module: m,
            span: t.name_span,
            kind: TypeDefKind::Broken,
        });
        env.modules[m].type_names.insert(t.name.clone(), id);
    }
}

fn resolve_type_bodies<'p>(unit: &'p ModuleUnit, m: usize, env: &mut Env<'p>, diags: &mut Diagnostics) {
    for decl in &unit.program.decls {
        let Decl::Type(t) = decl else { continue };
        let Some(&id) = env.modules[m].type_names.get(&t.name) else {
            continue;
        };
        // 重複定義の2個目以降は飛ばす (最初の定義が id を持つ)
        if env.types[id].span != t.name_span {
            continue;
        }
        let kind = match &t.kind {
            TypeDeclKind::Usage(inner) => {
                TypeDefKind::Usage(resolve_type_expr(env, m, inner, diags))
            }
            TypeDeclKind::Record(fields) => {
                let mut seen = HashSet::new();
                let mut out = Vec::new();
                for f in fields {
                    if !seen.insert(f.name.clone()) {
                        diags.emit(
                            codes::RESOLVE_DUPLICATE,
                            f.name_span,
                            format!("フィールド `{}` が重複しています", f.name),
                        );
                        continue;
                    }
                    out.push(FieldDef {
                        name: f.name.clone(),
                        ty: resolve_type_expr(env, m, &f.ty, diags),
                    });
                }
                TypeDefKind::Record(out)
            }
            TypeDeclKind::Adt(ctors) => {
                let mut out = Vec::new();
                for c in ctors {
                    out.push(CtorDef {
                        name: c.name.clone(),
                        payload: c
                            .payload
                            .as_ref()
                            .map(|p| resolve_type_expr(env, m, p, diags)),
                    });
                }
                TypeDefKind::Adt(out)
            }
        };
        env.types[id].kind = kind;
    }
}

/// 型式を解決済みの型へ変換する。失敗時は診断を出して `Void` を返す。
/// `m` は型式が書かれているモジュール。
pub fn resolve_type_expr(env: &Env, m: usize, t: &TypeExpr, diags: &mut Diagnostics) -> Type {
    match t {
        TypeExpr::Void { .. } => Type::Void,
        TypeExpr::List { elem, .. } => {
            Type::List(Box::new(resolve_type_expr(env, m, elem, diags)))
        }
        TypeExpr::Named {
            qualifier: None,
            name,
            span,
        } => match name.as_str() {
            "Int" => Type::Int,
            "Decimal" => Type::Decimal,
            "Text" => Type::Text,
            "Char" => Type::Char,
            "Bool" => Type::Bool,
            _ => match env.modules[m].type_names.get(name) {
                Some(&id) => Type::Named(id),
                None => {
                    diags.emit(
                        codes::RESOLVE_UNDEFINED,
                        *span,
                        format!("型 `{}` は定義されていません", name),
                    );
                    Type::Void
                }
            },
        },
        TypeExpr::Named {
            qualifier: Some(q),
            name,
            span,
        } => {
            let Some(&target) = env.modules[m].aliases.get(q) else {
                diags.emit_with_hint(
                    codes::RESOLVE_UNDEFINED,
                    *span,
                    format!("修飾名 `{}` は import されていません", q),
                    format!("`import std` などで `{}` を導入してください", q),
                );
                return Type::Void;
            };
            match target {
                ModuleRef::Std(module) => {
                    if name == "RangeInput" && builtins::has_range_input(module) {
                        Type::Named(env.std_types["RangeInput"])
                    } else {
                        diags.emit(
                            codes::RESOLVE_UNDEFINED,
                            *span,
                            format!("型 `{}.{}` は定義されていません", q, name),
                        );
                        Type::Void
                    }
                }
                ModuleRef::User(mid) => match env.modules[mid].type_names.get(name) {
                    Some(&id) => Type::Named(id),
                    None => {
                        diags.emit(
                            codes::RESOLVE_UNDEFINED,
                            *span,
                            format!("型 `{}.{}` は定義されていません", q, name),
                        );
                        Type::Void
                    }
                },
            }
        }
    }
}

/// 再帰型 (仕様 §20 で禁止) の検出。モジュール間は import が DAG なので
/// 閉路は同一モジュール内でのみ生じるが、検査自体は全型に対して行う。
fn check_recursive_types(env: &mut Env, diags: &mut Diagnostics) {
    fn refs(ty: &Type, out: &mut Vec<TypeId>) {
        match ty {
            Type::Named(id) => out.push(*id),
            Type::List(elem) => refs(elem, out),
            _ => {}
        }
    }
    let n = env.types.len();
    let mut edges: Vec<Vec<TypeId>> = vec![Vec::new(); n];
    for (id, def) in env.types.iter().enumerate() {
        let mut out = Vec::new();
        match &def.kind {
            TypeDefKind::Usage(inner) => refs(inner, &mut out),
            TypeDefKind::Record(fields) => {
                for f in fields {
                    refs(&f.ty, &mut out);
                }
            }
            TypeDefKind::Adt(ctors) => {
                for c in ctors {
                    if let Some(p) = &c.payload {
                        refs(p, &mut out);
                    }
                }
            }
            TypeDefKind::Broken => {}
        }
        edges[id] = out;
    }
    // 各ノードから DFS で自分自身に戻る閉路を探す
    let mut reported = HashSet::new();
    for start in 0..n {
        if reported.contains(&start) {
            continue;
        }
        let mut stack = vec![start];
        let mut visited = HashSet::new();
        while let Some(cur) = stack.pop() {
            for &next in &edges[cur] {
                if next == start {
                    diags.set_file(env.types[start].module);
                    diags.emit_with_hint(
                        codes::RESOLVE_RECURSIVE_TYPE,
                        env.types[start].span,
                        format!("型 `{}` は再帰的に定義されています", env.types[start].name),
                        "TCRF では再帰型を定義できません",
                    );
                    reported.insert(start);
                    stack.clear();
                    break;
                }
                if visited.insert(next) {
                    stack.push(next);
                }
            }
        }
    }
}

fn collect_ctors<'p>(unit: &'p ModuleUnit, m: usize, env: &mut Env<'p>, diags: &mut Diagnostics) {
    for decl in &unit.program.decls {
        let Decl::Type(t) = decl else { continue };
        let TypeDeclKind::Adt(ctors) = &t.kind else {
            continue;
        };
        let Some(&id) = env.modules[m].type_names.get(&t.name) else {
            continue;
        };
        if env.types[id].span != t.name_span {
            continue;
        }
        for (idx, c) in ctors.iter().enumerate() {
            if env.modules[m].ctors.contains_key(&c.name)
                || env.modules[m].type_names.contains_key(&c.name)
            {
                diags.emit(
                    codes::RESOLVE_DUPLICATE,
                    c.name_span,
                    format!("名前 `{}` は既に使われています", c.name),
                );
                continue;
            }
            env.modules[m].ctors.insert(c.name.clone(), (id, idx));
        }
    }
}

fn collect_values<'p>(unit: &'p ModuleUnit, m: usize, env: &mut Env<'p>, diags: &mut Diagnostics) {
    let mut taken: HashMap<String, Span> = HashMap::new();
    let dup = |name: &str, span: Span, taken: &mut HashMap<String, Span>, diags: &mut Diagnostics| -> bool {
        if taken.contains_key(name) {
            diags.emit(
                codes::RESOLVE_DUPLICATE,
                span,
                format!("名前 `{}` は既に定義されています", name),
            );
            true
        } else {
            taken.insert(name.to_string(), span);
            false
        }
    };

    for decl in &unit.program.decls {
        match decl {
            Decl::Const(c) => {
                if dup(&c.name, c.name_span, &mut taken, diags) {
                    continue;
                }
                env.modules[m].consts.insert(c.name.clone(), c);
            }
            Decl::Rule(r) => {
                if matches!(r.kind, RuleKind::External { .. }) {
                    diags.emit_with_hint(
                        codes::RESOLVE_EXTERNAL_RULE,
                        r.name_span,
                        "本体のない R 宣言は標準ライブラリ宣言ファイル (std.tcrf) でだけ使えます",
                        "R には本体 (束縛の列と最終式) が必要です",
                    );
                    continue;
                }
                if dup(&r.name, r.name_span, &mut taken, diags) {
                    continue;
                }
                let info = match &r.kind {
                    RuleKind::Normal {
                        input,
                        output,
                        can_fail,
                        ..
                    } => RuleInfo {
                        decl: r,
                        input: resolve_type_expr(env, m, input, diags),
                        output: resolve_type_expr(env, m, output, diags),
                        can_fail: *can_fail,
                        transition: false,
                    },
                    RuleKind::Transition { input, output } => RuleInfo {
                        decl: r,
                        input: resolve_type_expr(env, m, input, diags),
                        output: resolve_type_expr(env, m, output, diags),
                        can_fail: false,
                        transition: true,
                    },
                    RuleKind::External { .. } => unreachable!("上で除外済み"),
                };
                env.modules[m].rules.insert(r.name.clone(), info);
            }
            Decl::Flow(f) => {
                if dup(&f.name, f.name_span, &mut taken, diags) {
                    continue;
                }
                let info = match &f.signature {
                    Some(sig) => FlowInfo {
                        decl: f,
                        input: resolve_type_expr(env, m, &sig.input, diags),
                        output: resolve_type_expr(env, m, &sig.output, diags),
                        can_fail: sig.can_fail,
                        has_signature: true,
                    },
                    None => FlowInfo {
                        decl: f,
                        input: Type::Void,
                        output: Type::Void,
                        can_fail: false,
                        has_signature: false,
                    },
                };
                env.modules[m].flows.insert(f.name.clone(), info);
            }
            Decl::Type(_) => {}
        }
    }
}

/// C の右辺の制約 (§8): 許可された式形のみ・R/F 呼び出し禁止・循環禁止。
fn check_consts<'p>(unit: &'p ModuleUnit, m: usize, env: &mut Env<'p>, diags: &mut Diagnostics) {
    // 依存グラフを作りながら式形を検査する (依存は同一モジュール内のみ。
    // 他モジュールの定数は依存順の関係で常に先に評価済み)
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for decl in &unit.program.decls {
        let Decl::Const(c) = decl else { continue };
        if !env.modules[m]
            .consts
            .get(&c.name)
            .is_some_and(|stored| std::ptr::eq(*stored, c))
        {
            continue;
        }
        let mut refs = Vec::new();
        walk_const_expr(&c.value, env, m, diags, &mut refs);
        deps.insert(c.name.clone(), refs);
    }

    // 循環検出 + トポロジカル順
    let mut order = Vec::new();
    let mut state: HashMap<String, u8> = HashMap::new(); // 1=訪問中, 2=完了
    fn visit(
        name: &str,
        m: usize,
        deps: &HashMap<String, Vec<String>>,
        state: &mut HashMap<String, u8>,
        order: &mut Vec<String>,
        env: &Env,
        diags: &mut Diagnostics,
    ) {
        match state.get(name) {
            Some(2) => return,
            Some(1) => {
                let span = env.modules[m]
                    .consts
                    .get(name)
                    .map(|c| c.name_span)
                    .unwrap_or(Span::new(0, 0));
                diags.emit(
                    codes::RESOLVE_CONST_CYCLE,
                    span,
                    format!("定数 `{}` の定義が循環しています", name),
                );
                return;
            }
            _ => {}
        }
        state.insert(name.to_string(), 1);
        if let Some(rs) = deps.get(name) {
            for r in rs {
                if deps.contains_key(r) {
                    visit(r, m, deps, state, order, env, diags);
                }
            }
        }
        state.insert(name.to_string(), 2);
        order.push(name.to_string());
    }
    let mut names: Vec<String> = deps.keys().cloned().collect();
    names.sort();
    for name in names {
        visit(&name, m, &deps, &mut state, &mut order, env, diags);
    }
    env.modules[m].const_order = order;
}

fn walk_const_expr(e: &Expr, env: &Env, m: usize, diags: &mut Diagnostics, refs: &mut Vec<String>) {
    match e {
        Expr::IntLit(..)
        | Expr::DecimalLit(..)
        | Expr::TextLit(..)
        | Expr::CharLit(..)
        | Expr::BoolLit(..)
        | Expr::VoidLit(..)
        | Expr::Empty { .. } => {}
        Expr::Name { path, span } => {
            if path.len() == 1 {
                let name = &path[0].0;
                if env.modules[m].consts.contains_key(name) {
                    refs.push(name.clone());
                } else if env.modules[m].rules.contains_key(name)
                    || env.modules[m].flows.contains_key(name)
                {
                    diags.emit(
                        codes::RESOLVE_CONST_CALL,
                        *span,
                        "C の右辺で R/F は使えません",
                    );
                } else {
                    diags.emit(
                        codes::RESOLVE_UNDEFINED,
                        *span,
                        format!("定数 `{}` は定義されていません", name),
                    );
                }
            } else if path.len() == 2 {
                // import したモジュールの公開定数の参照は許可する
                match env.modules[m].aliases.get(&path[0].0) {
                    Some(ModuleRef::User(mid)) => {
                        let name = &path[1].0;
                        if name.starts_with('_') {
                            diags.emit_with_hint(
                                codes::RESOLVE_UNDEFINED,
                                path[1].1,
                                format!("`{}` はモジュール外に公開されていません", name),
                                "アンダースコア開始のトップレベル名は非公開です",
                            );
                        } else if !env.modules[*mid].consts.contains_key(name) {
                            diags.emit(
                                codes::RESOLVE_CONST_CALL,
                                *span,
                                "C の右辺で参照できるモジュール名は公開定数だけです",
                            );
                        }
                        // 他モジュールの定数は依存順で常に先に評価されるため依存辺は不要
                    }
                    _ => {
                        diags.emit(
                            codes::RESOLVE_CONST_CALL,
                            *span,
                            "C の右辺ではリテラル・型構築・既存 C の参照・純粋な組み込み演算だけを使えます",
                        );
                    }
                }
            } else {
                diags.emit(
                    codes::RESOLVE_CONST_CALL,
                    *span,
                    "C の右辺ではリテラル・型構築・既存 C の参照・純粋な組み込み演算だけを使えます",
                );
            }
        }
        Expr::Construct { arg, .. } => {
            if let Some(a) = arg {
                walk_const_expr(a, env, m, diags, refs);
            }
        }
        Expr::From { value, .. } => walk_const_expr(value, env, m, diags, refs),
        Expr::Record { fields, .. } => {
            for f in fields {
                walk_const_expr(&f.value, env, m, diags, refs);
            }
        }
        Expr::ListConstruct { elems, .. } => {
            for el in elems {
                walk_const_expr(el, env, m, diags, refs);
            }
        }
        Expr::Unary { operand, .. } => walk_const_expr(operand, env, m, diags, refs),
        Expr::Binary { left, right, .. } => {
            walk_const_expr(left, env, m, diags, refs);
            walk_const_expr(right, env, m, diags, refs);
        }
        Expr::Call { span, .. } | Expr::At { span, .. } => {
            diags.emit(
                codes::RESOLVE_CONST_CALL,
                *span,
                "C の右辺で R/F や失敗しうる式は呼べません",
            );
        }
        Expr::When { span, .. } | Expr::Match { span, .. } => {
            diags.emit(
                codes::RESOLVE_CONST_CALL,
                *span,
                "C の右辺で when / match は使えません",
            );
        }
    }
}

fn check_main(m: usize, env: &mut Env, diags: &mut Diagnostics) {
    if m != ENTRY {
        // モジュールとして読み込まれたファイルには main を書けない
        if let Some(info) = env.modules[m].flows.get("main") {
            diags.emit_with_hint(
                codes::TYPE_MAIN,
                info.decl.name_span,
                "モジュールには `F main` を定義できません",
                "`F main` は実行対象のファイルにだけ書けます",
            );
        }
        return;
    }
    match env.modules[ENTRY].flows.get("main") {
        None => {
            diags.emit(
                codes::TYPE_MAIN,
                Span::new(0, 0),
                "実行可能プログラムには `F main` がちょうど1個必要です",
            );
        }
        Some(info) => {
            if info.has_signature && (info.input != Type::Void || info.output != Type::Void) {
                diags.emit(
                    codes::TYPE_MAIN,
                    info.decl.name_span,
                    "`F main` のシグネチャは `Void > Void` でなければなりません",
                );
            }
        }
    }
}
