//! 意味解析用の型表現と宣言環境。

use crate::ast::{ConstDecl, FlowDecl, RuleDecl};
use crate::builtins::StdModule;
use crate::span::Span;
use std::collections::HashMap;

pub type TypeId = usize;

/// 解決済みの型。名前付き型は `TypeDef` 表への参照で表す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Decimal,
    Text,
    Char,
    Bool,
    Void,
    List(Box<Type>),
    Named(TypeId),
}

impl Type {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Decimal)
    }

    /// `==` / `!=` を持つ型 (組み込みスカラー)。
    pub fn is_equatable_scalar(&self) -> bool {
        matches!(
            self,
            Type::Int | Type::Decimal | Type::Text | Type::Char | Type::Bool
        )
    }

    /// `<` などの順序比較を持つ型。
    pub fn is_ordered_scalar(&self) -> bool {
        matches!(self, Type::Int | Type::Decimal | Type::Text | Type::Char)
    }
}

#[derive(Debug)]
pub struct TypeDef {
    pub name: String,
    /// 定義があるモジュール (ModuleUnit の番号)
    pub module: usize,
    pub span: Span,
    pub kind: TypeDefKind,
}

#[derive(Debug)]
pub enum TypeDefKind {
    /// 用途型 (内部型)
    Usage(Type),
    Record(Vec<FieldDef>),
    Adt(Vec<CtorDef>),
    /// 解決エラー時のプレースホルダ
    Broken,
}

#[derive(Debug)]
pub struct FieldDef {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug)]
pub struct CtorDef {
    pub name: String,
    pub payload: Option<Type>,
}

#[derive(Debug)]
pub struct RuleInfo<'p> {
    pub decl: &'p RuleDecl,
    pub input: Type,
    pub output: Type,
    pub can_fail: bool,
    /// 表現保持型遷移 (`=>`) か
    pub transition: bool,
}

#[derive(Debug)]
pub struct FlowInfo<'p> {
    pub decl: &'p FlowDecl,
    pub input: Type,
    pub output: Type,
    pub can_fail: bool,
    pub has_signature: bool,
}

/// import の解決先。組み込み std モジュールかユーザー定義モジュール (ModuleUnit の番号)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleRef {
    Std(StdModule),
    User(usize),
}

/// 1モジュール分の名前空間。
#[derive(Default)]
pub struct ModuleEnv<'p> {
    /// import 修飾名 → 解決先モジュール
    pub aliases: HashMap<String, ModuleRef>,
    /// このモジュールで定義された型名 → TypeId (型本体はEnv.typesに共有格納)
    pub type_names: HashMap<String, TypeId>,
    /// ADT コンストラクタ名 → (ADT の TypeId, コンストラクタ番号)
    pub ctors: HashMap<String, (TypeId, usize)>,
    pub consts: HashMap<String, &'p ConstDecl>,
    /// 依存順 (先に評価すべき順) の定数名
    pub const_order: Vec<String>,
    pub rules: HashMap<String, RuleInfo<'p>>,
    pub flows: HashMap<String, FlowInfo<'p>>,
}

/// エントリモジュール (実行対象ファイル) の番号。
pub const ENTRY: usize = 0;

/// 名前解決の結果。typecheck と eval が参照する。
pub struct Env<'p> {
    /// 全モジュール共有の型定義表
    pub types: Vec<TypeDef>,
    /// 組み込み std 型名 (RangeInput) → TypeId
    pub std_types: HashMap<&'static str, TypeId>,
    /// ModuleUnit と同じ並びのモジュール別名前空間 (0 = エントリ)
    pub modules: Vec<ModuleEnv<'p>>,
    /// ドット区切りモジュール名 (エントリは "")。表示用
    pub module_names: Vec<String>,
    /// 依存順 (先に処理すべき順)。最後がエントリ
    pub order: Vec<usize>,
    /// std.tcrf から構築した標準ライブラリの署名表 (std 未使用なら空)
    pub std_sigs: crate::stdsig::StdSigs,
}

impl<'p> Env<'p> {
    pub fn type_def(&self, id: TypeId) -> &TypeDef {
        &self.types[id]
    }

    /// 用途型ならその内部型を返す。
    pub fn usage_inner<'a>(&'a self, ty: &'a Type) -> Option<&'a Type> {
        if let Type::Named(id) = ty {
            if let TypeDefKind::Usage(inner) = &self.types[*id].kind {
                return Some(inner);
            }
        }
        None
    }

    /// リスト、またはリストを内部型とする用途型なら要素型を返す。
    pub fn as_list_elem<'a>(&'a self, ty: &'a Type) -> Option<&'a Type> {
        match ty {
            Type::List(elem) => Some(elem),
            Type::Named(_) => match self.usage_inner(ty) {
                Some(Type::List(elem)) => Some(elem),
                _ => None,
            },
            _ => None,
        }
    }

    /// 診断メッセージ用の型名表示。
    pub fn display(&self, ty: &Type) -> String {
        match ty {
            Type::Int => "Int".to_string(),
            Type::Decimal => "Decimal".to_string(),
            Type::Text => "Text".to_string(),
            Type::Char => "Char".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::Void => "Void".to_string(),
            Type::List(elem) => format!("List<{}>", self.display(elem)),
            Type::Named(id) => {
                let def = &self.types[*id];
                let mname = &self.module_names[def.module];
                if mname.is_empty() {
                    def.name.clone()
                } else {
                    // 別モジュールの型は既定修飾名 (モジュール名の最終要素) 付きで表示する
                    format!("{}.{}", mname.rsplit('.').next().unwrap(), def.name)
                }
            }
        }
    }
}
