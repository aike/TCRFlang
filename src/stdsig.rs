//! 標準ライブラリ宣言ファイル std.tcrf の署名表。
//! `import std` の型検査は、ここで構築した署名に従って行う。

use crate::ast::{Decl, RuleKind, TypeExpr};
use crate::builtins::Builtin;
use crate::diagnostics::{codes, Diagnostics};
use crate::loader::ModuleUnit;
use crate::types::{Env, Type, TypeDefKind};
use std::collections::HashMap;

/// std.tcrf のシグネチャに書ける型。
#[derive(Debug, Clone, PartialEq)]
pub enum SigType {
    /// 型変数 (大文字1文字。A, B など)
    Var(String),
    List(Box<SigType>),
    /// 組み込みスカラー / Void
    Concrete(Type),
    /// std.RangeInput。first Int / last Int を持つレコードを構造的に受理する
    RangeInput,
}

#[derive(Debug)]
pub struct StdSig {
    pub name: String,
    pub inputs: Vec<SigType>,
    pub output: SigType,
    pub can_fail: bool,
}

pub type StdSigs = HashMap<String, StdSig>;

/// std.tcrf の AST から署名表を作る。
/// 呼び出し前に diags.set_file を std.tcrf のユニット番号に合わせること。
pub fn build(unit: &ModuleUnit, diags: &mut Diagnostics) -> StdSigs {
    let mut sigs = StdSigs::new();
    if let Some(im) = unit.program.imports.first() {
        diags.emit(
            codes::RESOLVE_STD_DECL,
            im.span,
            "std.tcrf では import を使えません",
        );
    }
    for decl in &unit.program.decls {
        match decl {
            Decl::Type(t) => {
                // RangeInput の再掲 (ドキュメント) だけ許す。定義自体は組み込みを使う
                if t.name != "RangeInput" {
                    diags.emit(
                        codes::RESOLVE_STD_DECL,
                        t.name_span,
                        "std.tcrf に書ける型宣言は RangeInput だけです",
                    );
                }
            }
            Decl::Const(c) => {
                diags.emit(codes::RESOLVE_STD_DECL, c.name_span, "std.tcrf に C は書けません");
            }
            Decl::Flow(f) => {
                diags.emit(codes::RESOLVE_STD_DECL, f.name_span, "std.tcrf に F は書けません");
            }
            Decl::Rule(r) => {
                let RuleKind::External {
                    inputs,
                    output,
                    can_fail,
                } = &r.kind
                else {
                    diags.emit(
                        codes::RESOLVE_STD_DECL,
                        r.name_span,
                        "std.tcrf の R は本体のない宣言 (シグネチャのみ) にしてください",
                    );
                    continue;
                };
                if sigs.contains_key(&r.name) {
                    diags.emit(
                        codes::RESOLVE_STD_DECL,
                        r.name_span,
                        format!("`{}` は既に宣言されています", r.name),
                    );
                    continue;
                }
                // パラメータ名の個数は入力型と揃える (Void 入力はパラメータなし)
                let void_input = inputs.len() == 1 && matches!(inputs[0], TypeExpr::Void { .. });
                if !(r.params.len() == inputs.len() || (r.params.is_empty() && void_input)) {
                    diags.emit(
                        codes::RESOLVE_STD_DECL,
                        r.name_span,
                        "パラメータ名の個数が入力型の個数と一致しません",
                    );
                }
                let mut ok = true;
                let ins: Vec<SigType> = inputs.iter().map(|t| sig_type(t, diags, &mut ok)).collect();
                let out = sig_type(output, diags, &mut ok);
                if ok {
                    sigs.insert(
                        r.name.clone(),
                        StdSig {
                            name: r.name.clone(),
                            inputs: ins,
                            output: out,
                            can_fail: *can_fail,
                        },
                    );
                }
            }
        }
    }
    sigs
}

fn sig_type(t: &TypeExpr, diags: &mut Diagnostics, ok: &mut bool) -> SigType {
    match t {
        TypeExpr::Void { .. } => SigType::Concrete(Type::Void),
        TypeExpr::List { elem, .. } => SigType::List(Box::new(sig_type(elem, diags, ok))),
        TypeExpr::Named {
            qualifier: None,
            name,
            span,
        } => match name.as_str() {
            "Int" => SigType::Concrete(Type::Int),
            "Decimal" => SigType::Concrete(Type::Decimal),
            "Text" => SigType::Concrete(Type::Text),
            "Char" => SigType::Concrete(Type::Char),
            "Bool" => SigType::Concrete(Type::Bool),
            "RangeInput" => SigType::RangeInput,
            _ if name.len() == 1 && name.chars().all(|c| c.is_ascii_uppercase()) => {
                SigType::Var(name.clone())
            }
            _ => {
                diags.emit(
                    codes::RESOLVE_STD_DECL,
                    *span,
                    format!(
                        "std.tcrf の型に `{}` は使えません (組み込み型・List・型変数 A〜Z・RangeInput のみ)",
                        name
                    ),
                );
                *ok = false;
                SigType::Concrete(Type::Void)
            }
        },
        TypeExpr::Named {
            qualifier: Some(_),
            span,
            ..
        } => {
            diags.emit(
                codes::RESOLVE_STD_DECL,
                *span,
                "std.tcrf の型に修飾名は使えません",
            );
            *ok = false;
            SigType::Concrete(Type::Void)
        }
    }
}

/// 呼び出しの引数型を署名と照合し、(結果型, 失敗可能性) を返す。
/// `b` は対応する組み込み実装 (署名で表せない追加制約の検査に使う)。
pub fn check(
    sig: &StdSig,
    b: Option<Builtin>,
    args: &[Type],
    env: &Env,
) -> Result<(Type, bool), String> {
    if args.len() != sig.inputs.len() {
        return Err(format!(
            "`{}` は引数を{}個取りますが、{}個渡されました",
            sig.name,
            sig.inputs.len(),
            args.len()
        ));
    }
    let mut bind: HashMap<&str, Type> = HashMap::new();
    for (s, a) in sig.inputs.iter().zip(args) {
        match_top(s, a, env, &mut bind)?;
    }
    // contains は要素型に `==` が必要 (署名では表せない制約)
    if b == Some(Builtin::Contains) {
        if let Some(elem) = bind.values().next() {
            let inner = env.usage_inner(elem).unwrap_or(elem);
            if !inner.is_equatable_scalar() {
                return Err(format!(
                    "{} には `==` が定義されていません",
                    env.display(elem)
                ));
            }
        }
    }
    let out = instantiate(&sig.output, &bind, env)
        .ok_or_else(|| "出力型の型変数が入力から決まりません".to_string())?;
    Ok((out, sig.can_fail))
}

fn bind_var<'s>(
    v: &'s str,
    arg: &Type,
    env: &Env,
    bind: &mut HashMap<&'s str, Type>,
) -> Result<(), String> {
    match bind.get(v) {
        Some(prev) if prev != arg => Err(format!(
            "型変数 {} が {} と {} の両方に一致しません",
            v,
            env.display(prev),
            env.display(arg)
        )),
        Some(_) => Ok(()),
        None => {
            bind.insert(v, arg.clone());
            Ok(())
        }
    }
}

/// first Int / last Int のレコードか (RangeInput の構造的判定)。
fn is_range_record(t: &Type, env: &Env) -> bool {
    let Type::Named(id) = t else { return false };
    match &env.type_def(*id).kind {
        TypeDefKind::Record(fields) => {
            fields.len() == 2
                && fields[0].name == "first"
                && fields[0].ty == Type::Int
                && fields[1].name == "last"
                && fields[1].ty == Type::Int
        }
        _ => false,
    }
}

/// 引数の最上位での照合。リスト・スカラーの用途型は内部型へ透過する。
fn match_top<'s>(
    s: &'s SigType,
    arg: &Type,
    env: &Env,
    bind: &mut HashMap<&'s str, Type>,
) -> Result<(), String> {
    match s {
        // 型変数はその型のまま束縛する (透過しない)
        SigType::Var(v) => bind_var(v, arg, env, bind),
        SigType::Concrete(t) => {
            let a = env.usage_inner(arg).unwrap_or(arg);
            if a == t {
                Ok(())
            } else {
                Err(format!(
                    "{} が必要ですが、{} が渡されました",
                    env.display(t),
                    env.display(arg)
                ))
            }
        }
        SigType::List(inner) => match env.as_list_elem(arg) {
            Some(elem) => match_exact(inner, elem, env, bind),
            None => Err(format!(
                "List が必要ですが、{} が渡されました",
                env.display(arg)
            )),
        },
        SigType::RangeInput => {
            if is_range_record(arg, env) {
                Ok(())
            } else {
                Err(format!(
                    "first Int / last Int を持つレコード (std.RangeInput) が必要ですが、{} が渡されました",
                    env.display(arg)
                ))
            }
        }
    }
}

/// 型の内側での照合 (透過なしの完全一致)。
fn match_exact<'s>(
    s: &'s SigType,
    arg: &Type,
    env: &Env,
    bind: &mut HashMap<&'s str, Type>,
) -> Result<(), String> {
    match s {
        SigType::Var(v) => bind_var(v, arg, env, bind),
        SigType::Concrete(t) => {
            if arg == t {
                Ok(())
            } else {
                Err(format!(
                    "{} が必要ですが、{} が渡されました",
                    env.display(t),
                    env.display(arg)
                ))
            }
        }
        SigType::List(inner) => match arg {
            Type::List(elem) => match_exact(inner, elem, env, bind),
            _ => Err(format!(
                "List が必要ですが、{} が渡されました",
                env.display(arg)
            )),
        },
        SigType::RangeInput => {
            if is_range_record(arg, env) {
                Ok(())
            } else {
                Err(format!(
                    "std.RangeInput が必要ですが、{} が渡されました",
                    env.display(arg)
                ))
            }
        }
    }
}

fn instantiate(s: &SigType, bind: &HashMap<&str, Type>, env: &Env) -> Option<Type> {
    match s {
        SigType::Var(v) => bind.get(v.as_str()).cloned(),
        SigType::Concrete(t) => Some(t.clone()),
        SigType::List(inner) => Some(Type::List(Box::new(instantiate(inner, bind, env)?))),
        SigType::RangeInput => Some(Type::Named(*env.std_types.get("RangeInput")?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Diagnostics;
    use crate::loader::{self, ModuleUnit};
    use crate::resolver;
    use crate::span::SourceFile;
    use crate::{lexer, parser};

    /// 署名宣言ソースから StdSigs を作る。
    fn sigs_of(src: &str) -> StdSigs {
        let file = SourceFile::new("std.tcrf", src);
        let mut diags = Diagnostics::new();
        let toks = lexer::lex(&file, &mut diags);
        let program = parser::parse(toks, &mut diags);
        let unit = ModuleUnit {
            name: "std".to_string(),
            file,
            program,
            import_targets: Vec::new(),
            is_std: true,
        };
        let sigs = build(&unit, &mut diags);
        assert!(diags.is_empty(), "std.tcrf diagnostics: {:?}", diags.items);
        sigs
    }

    /// 型環境: Price [Decimal] 用途型を持つ最小プログラム。
    fn with_env<R>(f: impl FnOnce(&Env, Type) -> R) -> R {
        let mut diags = Diagnostics::new();
        let loaded = loader::load_str("t.tcrf", "T Price [Decimal]\n\nF main\n  Void\n", &mut diags);
        let env = resolver::resolve(&loaded, &mut diags);
        let price = Type::Named(env.modules[0].type_names["Price"]);
        f(&env, price)
    }

    #[test]
    fn var_binds_and_instantiates_output() {
        let sigs = sigs_of("R first values\n  List<A> > A ! Error\n");
        with_env(|env, price| {
            let (out, can_fail) =
                check(&sigs["first"], None, &[Type::List(Box::new(price.clone()))], env).unwrap();
            assert_eq!(out, price);
            assert!(can_fail);
        });
    }

    #[test]
    fn usage_type_is_transparent_for_lists_and_scalars() {
        let sigs = sigs_of("R decimal value\n  Decimal > Text\n");
        with_env(|env, price| {
            // 用途型 Price は内部型 Decimal として受理される
            let (out, _) = check(&sigs["decimal"], None, &[price], env).unwrap();
            assert_eq!(out, Type::Text);
        });
    }

    #[test]
    fn multi_arg_var_must_be_consistent() {
        let sigs = sigs_of("R prepend value values\n  A, List<A> > List<A>\n");
        with_env(|env, price| {
            let list = Type::List(Box::new(price.clone()));
            let ok = check(&sigs["prepend"], None, &[price.clone(), list.clone()], env);
            assert!(ok.is_ok());
            // 要素型が合わないリストへの prepend は拒否
            let bad = check(
                &sigs["prepend"],
                None,
                &[Type::Int, Type::List(Box::new(price))],
                env,
            );
            assert!(bad.is_err());
        });
    }

    #[test]
    fn arity_mismatch_is_error() {
        let sigs = sigs_of("R concat left right\n  Text, Text > Text\n");
        with_env(|env, _| {
            let e = check(&sigs["concat"], None, &[Type::Text], env).unwrap_err();
            assert!(e.contains("2個"), "{}", e);
        });
    }

    #[test]
    fn body_rule_is_rejected_in_std_decl() {
        let file = SourceFile::new("std.tcrf", "R f x\n  Int > Int\n\n  x\n");
        let mut diags = Diagnostics::new();
        let toks = lexer::lex(&file, &mut diags);
        let program = parser::parse(toks, &mut diags);
        let unit = ModuleUnit {
            name: "std".to_string(),
            file,
            program,
            import_targets: Vec::new(),
            is_std: true,
        };
        build(&unit, &mut diags);
        assert!(diags
            .items
            .iter()
            .any(|i| i.code == codes::RESOLVE_STD_DECL));
    }
}
