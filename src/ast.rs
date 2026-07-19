//! 抽象構文木。
//!
//! パーサは名前解決を行わないため、`Name` / `Construct` などは
//! 後段 (resolver / typecheck) で意味が確定する。

use crate::span::Span;
use rust_decimal::Decimal;

#[derive(Debug)]
pub struct Program {
    pub imports: Vec<ImportDecl>,
    pub decls: Vec<Decl>,
}

#[derive(Debug)]
pub struct ImportDecl {
    /// ドット区切りモジュール名の各要素 (例: ["std", "console"])。
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub span: Span,
}

#[derive(Debug)]
pub enum Decl {
    Type(TypeDecl),
    Const(ConstDecl),
    Rule(RuleDecl),
    Flow(FlowDecl),
}

#[derive(Debug)]
pub struct TypeDecl {
    pub name: String,
    pub name_span: Span,
    pub kind: TypeDeclKind,
}

#[derive(Debug)]
pub enum TypeDeclKind {
    /// 用途型: `T UserId [Text]`
    Usage(TypeExpr),
    /// レコード型: `T Product { ... }`
    Record(Vec<FieldDecl>),
    /// 代数データ型: `T Grade` + `| Ctor [Payload]`
    Adt(Vec<CtorDecl>),
}

#[derive(Debug)]
pub struct FieldDecl {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeExpr,
}

#[derive(Debug)]
pub struct CtorDecl {
    pub name: String,
    pub name_span: Span,
    pub payload: Option<TypeExpr>,
}

#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// `Price` / `std.RangeInput` (qualifier は import 別名)
    Named {
        qualifier: Option<String>,
        name: String,
        span: Span,
    },
    /// `List<T>`
    List { elem: Box<TypeExpr>, span: Span },
    /// `Void`
    Void { span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named { span, .. } | TypeExpr::List { span, .. } | TypeExpr::Void { span } => {
                *span
            }
        }
    }
}

#[derive(Debug)]
pub struct ConstDecl {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
}

#[derive(Debug)]
pub struct RuleDecl {
    pub name: String,
    pub name_span: Span,
    /// パラメータ名。通常 R は最大1個。宣言のみ R (std.tcrf) は複数可
    pub params: Vec<(String, Span)>,
    pub kind: RuleKind,
}

#[derive(Debug)]
pub enum RuleKind {
    /// `Input > Output [! Error]` + 本体
    Normal {
        input: TypeExpr,
        output: TypeExpr,
        can_fail: bool,
        body: Block,
    },
    /// 表現保持型遷移 `Input => Output` (本体なし)
    Transition { input: TypeExpr, output: TypeExpr },
    /// 宣言のみ (本体なし)。標準ライブラリ宣言ファイル std.tcrf 専用。
    /// 入力型はカンマ区切りで複数書ける (`A, List<A> > List<A>`)
    External {
        inputs: Vec<TypeExpr>,
        output: TypeExpr,
        can_fail: bool,
    },
}

/// 束縛の列 + 最終式。R 本体と when/match の分岐本体で共用する。
#[derive(Debug)]
pub struct Block {
    pub bindings: Vec<Binding>,
    pub result: Expr,
}

#[derive(Debug)]
pub struct Binding {
    pub name: String,
    pub name_span: Span,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
}

#[derive(Debug)]
pub struct FlowDecl {
    pub name: String,
    pub name_span: Span,
    pub signature: Option<FlowSignature>,
    pub steps: Vec<FlowStep>,
}

#[derive(Debug)]
pub struct FlowSignature {
    pub input: TypeExpr,
    pub output: TypeExpr,
    pub can_fail: bool,
}

#[derive(Debug)]
pub enum FlowStep {
    /// 値式 (先頭ステップのみ有効; 検査は typecheck で行う)
    Initial(Expr),
    /// R/F/定数の名前参照 (例: `calculateTotal`, `std.printLine`)
    Call { path: Vec<(String, Span)>, span: Span },
    /// フロー単位の match
    Match { arms: Vec<FlowMatchArm>, span: Span },
}

#[derive(Debug)]
pub struct FlowMatchArm {
    pub ctor: String,
    pub ctor_span: Span,
    pub steps: Vec<FlowStep>,
}

#[derive(Debug)]
pub enum Expr {
    IntLit(i64, Span),
    DecimalLit(Decimal, Span),
    TextLit(String, Span),
    CharLit(char, Span),
    BoolLit(bool, Span),
    VoidLit(Span),
    /// 小文字識別子のドット連鎖 (例: `price`, `input.values`, `std.printLine`)。
    /// 先頭要素がローカル値か import 修飾名かは typecheck で確定する。
    Name { path: Vec<(String, Span)>, span: Span },
    /// 名前 + 空白区切り引数列 (R 呼び出し / 組み込み呼び出し)
    Call {
        path: Vec<(String, Span)>,
        args: Vec<Expr>,
        span: Span,
    },
    /// `at list index`
    At {
        list: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `std.empty<T>`
    Empty {
        path: Vec<(String, Span)>,
        elem: TypeExpr,
        span: Span,
    },
    /// `TypeName expr` (用途型構築) / `Ctor [payload]` (ADT 構築)。
    /// どちらかは型解決後に確定する。qualifier は `std.RangeInput` のような修飾。
    Construct {
        qualifier: Option<String>,
        name: String,
        name_span: Span,
        arg: Option<Box<Expr>>,
        span: Span,
    },
    /// `A from x` — 内部型が同じ用途型どうしの表現保持変換 (unwrap して包み直す)。
    /// `=>` 遷移 R の本体はこの式のシンタックスシュガー展開に相当する
    From {
        qualifier: Option<String>,
        name: String,
        name_span: Span,
        value: Box<Expr>,
        span: Span,
    },
    /// `TypeName { field = expr ... }`
    Record {
        qualifier: Option<String>,
        name: String,
        name_span: Span,
        fields: Vec<RecordFieldInit>,
        span: Span,
    },
    /// `TypeName( elem... )` — リスト用途型構築。要素が1個の場合、
    /// 型解決の結果により `Construct` (括弧付き引数) として再解釈されうる。
    ListConstruct {
        qualifier: Option<String>,
        name: String,
        name_span: Span,
        elems: Vec<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    When {
        cond: Box<Expr>,
        then_block: Box<Block>,
        else_block: Box<Block>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
}

#[derive(Debug)]
pub struct RecordFieldInit {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
}

#[derive(Debug)]
pub struct MatchArm {
    pub ctor: String,
    pub ctor_span: Span,
    pub binding: Option<(String, Span)>,
    pub body: Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            Lt => "<",
            Le => "<=",
            Gt => ">",
            Ge => ">=",
            Eq => "==",
            Ne => "!=",
            And => "and",
            Or => "or",
        }
    }
}

impl Expr {
    pub fn span(&self) -> Span {
        use Expr::*;
        match self {
            IntLit(_, s) | DecimalLit(_, s) | TextLit(_, s) | CharLit(_, s) | BoolLit(_, s)
            | VoidLit(s) => *s,
            Name { span, .. }
            | Call { span, .. }
            | At { span, .. }
            | Empty { span, .. }
            | Construct { span, .. }
            | From { span, .. }
            | Record { span, .. }
            | ListConstruct { span, .. }
            | Unary { span, .. }
            | Binary { span, .. }
            | When { span, .. }
            | Match { span, .. } => *span,
        }
    }
}
