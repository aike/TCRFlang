//! 木構造評価器。型検査済みプログラムのみ実行する。
//! Error は `Result` の `Err` として自動伝播し、値としては存在しない (§15)。
//! 名前解決は各コードが属するモジュール `m` の名前空間で行う。

use crate::ast::*;
use crate::builtins;
use crate::span::Span;
use crate::types::{Env, FlowInfo, ModuleRef, TypeDefKind, TypeId, ENTRY};
use crate::value::{RecordValue, Value};
use std::collections::HashMap;
use std::rc::Rc;

/// TCRF レベルの呼び出し深度上限。ネイティブスタックが尽きる前に Error にする。
/// (CLI は 256MB スタックのスレッドで実行する前提の値)
const MAX_DEPTH: usize = 10_000;

/// 実行時 Error。ユーザーコードからは中身が見えない (§15) が、
/// main まで伝播したとき診断表示に使う。
#[derive(Debug)]
pub struct RuntimeError {
    pub message: String,
    /// 発生位置が属するモジュール (ModuleUnit の番号)
    pub file: usize,
    pub span: Option<Span>,
    /// 伝播経路の R/F 名 (内側から外側の順)。
    pub trace: Vec<String>,
}

impl RuntimeError {
    fn new(message: impl Into<String>, file: usize, span: Span) -> Self {
        RuntimeError {
            message: message.into(),
            file,
            span: Some(span),
            trace: Vec::new(),
        }
    }
}

type EvalResult = Result<Value, RuntimeError>;

/// `main` を実行する。Error が伝播してきたら `Err` を返す。
pub fn run_main(env: &Env) -> Result<(), RuntimeError> {
    let mut interp = Interp {
        env,
        consts: HashMap::new(),
        depth: 0,
    };
    interp.init_consts()?;
    let info = env.modules[ENTRY]
        .flows
        .get("main")
        .expect("main の存在は検査済み");
    interp.run_flow(ENTRY, info, Value::Void)?;
    Ok(())
}

struct Interp<'e, 'p> {
    env: &'e Env<'p>,
    /// (モジュール, 定数名) → 値
    consts: HashMap<(usize, String), Value>,
    depth: usize,
}

impl<'e, 'p> Interp<'e, 'p> {
    /// 定数をモジュール依存順 → モジュール内依存順に前もって評価する。
    fn init_consts(&mut self) -> Result<(), RuntimeError> {
        for &m in &self.env.order {
            for name in &self.env.modules[m].const_order {
                let Some(c) = self.env.modules[m].consts.get(name.as_str()) else {
                    continue;
                };
                let v = self.eval_expr(m, &c.value, &mut HashMap::new())?;
                self.consts.insert((m, name.clone()), v);
            }
        }
        Ok(())
    }

    fn enter(&mut self, m: usize, span: Span) -> Result<(), RuntimeError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            return Err(RuntimeError::new(
                "呼び出しが深すぎます (再帰が停止しない可能性があります)",
                m,
                span,
            ));
        }
        Ok(())
    }

    // ---- F ----

    fn run_flow(&mut self, m: usize, info: &FlowInfo<'p>, input: Value) -> EvalResult {
        self.enter(m, info.decl.name_span)?;
        let r = self.run_flow_steps(m, &info.decl.steps, input);
        self.depth -= 1;
        r.map_err(|mut e| {
            e.trace.push(format!("F {}", info.decl.name));
            e
        })
    }

    fn run_flow_steps(&mut self, m: usize, steps: &[FlowStep], mut current: Value) -> EvalResult {
        for step in steps {
            match step {
                FlowStep::Initial(expr) => {
                    current = self.eval_expr(m, expr, &mut HashMap::new())?;
                }
                FlowStep::Call { path, span } => {
                    current = self.apply_flow_call(m, path, *span, current)?;
                }
                FlowStep::Match { arms, .. } => {
                    let Value::Adt {
                        type_id,
                        ctor,
                        payload,
                    } = current
                    else {
                        unreachable!("F の match 対象は ADT (検査済み)")
                    };
                    let name = self.ctor_name(type_id, ctor);
                    let arm = arms
                        .iter()
                        .find(|a| a.ctor == name)
                        .expect("網羅性は検査済み");
                    let start = payload.map(|p| (*p).clone()).unwrap_or(Value::Void);
                    current = self.run_flow_steps(m, &arm.steps, start)?;
                }
            }
        }
        Ok(current)
    }

    fn apply_flow_call(
        &mut self,
        m: usize,
        path: &[(String, Span)],
        span: Span,
        current: Value,
    ) -> EvalResult {
        if path.len() == 1 {
            let name = &path[0].0;
            if self.env.modules[m].rules.contains_key(name) {
                return self.call_rule(m, name, current, span);
            }
            if let Some(info) = self.env.modules[m].flows.get(name) {
                return self.run_flow(m, info, current);
            }
            if let Some(v) = self.consts.get(&(m, name.clone())) {
                return Ok(v.clone());
            }
            unreachable!("F ステップの名前は検査済み: {}", name)
        }
        let fname = &path[1].0;
        match self.env.modules[m].aliases[&path[0].0] {
            ModuleRef::Std(module) => {
                let b = builtins::lookup(module, fname).expect("組み込み名は検査済み");
                builtins::eval(b, &[current], self.env)
                    .map_err(|msg| RuntimeError::new(msg, m, span))
            }
            ModuleRef::User(mid) => {
                if self.env.modules[mid].rules.contains_key(fname) {
                    return self.call_rule(mid, fname, current, span);
                }
                if let Some(info) = self.env.modules[mid].flows.get(fname) {
                    return self.run_flow(mid, info, current);
                }
                if let Some(v) = self.consts.get(&(mid, fname.clone())) {
                    return Ok(v.clone());
                }
                unreachable!("F ステップの修飾名は検査済み: {}.{}", path[0].0, fname)
            }
        }
    }

    // ---- R ----

    /// `m` は R が定義されているモジュール。本体はそのモジュールの名前空間で評価する。
    fn call_rule(&mut self, m: usize, name: &str, arg: Value, span: Span) -> EvalResult {
        let info = &self.env.modules[m].rules[name];
        match &info.decl.kind {
            // 表現保持型遷移: 実行時は値をそのまま流す
            RuleKind::Transition { .. } => Ok(arg),
            RuleKind::Normal { body, .. } => {
                self.enter(m, span)?;
                let mut scope = HashMap::new();
                if let Some((p, _)) = info.decl.params.first() {
                    scope.insert(p.clone(), arg);
                }
                let r = self.eval_block(m, body, &mut scope);
                self.depth -= 1;
                r.map_err(|mut e| {
                    e.trace.push(format!("R {}", name));
                    e
                })
            }
            RuleKind::External { .. } => unreachable!("resolver で除外済み"),
        }
    }

    fn eval_block(
        &mut self,
        m: usize,
        block: &Block,
        scope: &mut HashMap<String, Value>,
    ) -> EvalResult {
        for b in &block.bindings {
            let v = self.eval_expr(m, &b.value, scope)?;
            scope.insert(b.name.clone(), v);
        }
        self.eval_expr(m, &block.result, scope)
    }

    // ---- 式 ----

    fn eval_expr(&mut self, m: usize, e: &Expr, scope: &mut HashMap<String, Value>) -> EvalResult {
        match e {
            Expr::IntLit(n, _) => Ok(Value::Int(*n)),
            Expr::DecimalLit(d, _) => Ok(Value::Decimal(*d)),
            Expr::TextLit(s, _) => Ok(Value::text(s.clone())),
            Expr::CharLit(c, _) => Ok(Value::Char(*c)),
            Expr::BoolLit(b, _) => Ok(Value::Bool(*b)),
            Expr::VoidLit(_) => Ok(Value::Void),
            Expr::Name { path, .. } => self.eval_name(m, path, scope),
            Expr::Call { path, args, span } => self.eval_call(m, path, args, *span, scope),
            Expr::At { list, index, span } => {
                let lv = self.eval_expr(m, list, scope)?;
                let iv = self.eval_expr(m, index, scope)?;
                let (Value::List(xs), Value::Int(i)) = (&lv, &iv) else {
                    unreachable!("at の引数型は検査済み")
                };
                if *i < 0 || *i as usize >= xs.len() {
                    return Err(RuntimeError::new(
                        format!("添字 {} は範囲外です (要素数 {})", i, xs.len()),
                        m,
                        *span,
                    ));
                }
                Ok(xs[*i as usize].clone())
            }
            Expr::Empty { .. } => Ok(Value::List(Rc::new(Vec::new()))),
            Expr::Construct {
                qualifier,
                name,
                arg,
                ..
            } => self.eval_construct(m, qualifier.as_deref(), name, arg.as_deref(), scope),
            // `A from x` は表現保持: 値をそのまま流す
            Expr::From { value, .. } => self.eval_expr(m, value, scope),
            Expr::Record {
                qualifier,
                name,
                fields,
                ..
            } => self.eval_record(m, qualifier.as_deref(), name, fields, scope),
            Expr::ListConstruct {
                qualifier,
                name,
                elems,
                ..
            } => self.eval_list_construct(m, qualifier.as_deref(), name, elems, scope),
            Expr::Unary { op, operand, span } => {
                let v = self.eval_expr(m, operand, scope)?;
                match (op, v) {
                    (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnaryOp::Neg, Value::Int(n)) => n
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| RuntimeError::new("Int の範囲を超えました", m, *span)),
                    (UnaryOp::Neg, Value::Decimal(d)) => Ok(Value::Decimal(-d)),
                    _ => unreachable!("単項演算の型は検査済み"),
                }
            }
            Expr::Binary {
                op,
                left,
                right,
                span,
            } => self.eval_binary(m, *op, left, right, *span, scope),
            Expr::When {
                cond,
                then_block,
                else_block,
                ..
            } => {
                let Value::Bool(c) = self.eval_expr(m, cond, scope)? else {
                    unreachable!("when の条件は Bool (検査済み)")
                };
                if c {
                    self.eval_block(m, then_block, scope)
                } else {
                    self.eval_block(m, else_block, scope)
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let Value::Adt {
                    type_id,
                    ctor,
                    payload,
                } = self.eval_expr(m, scrutinee, scope)?
                else {
                    unreachable!("match の対象は ADT (検査済み)")
                };
                let name = self.ctor_name(type_id, ctor);
                let arm = arms
                    .iter()
                    .find(|a| a.ctor == name)
                    .expect("網羅性は検査済み");
                if let (Some((bname, _)), Some(p)) = (&arm.binding, &payload) {
                    scope.insert(bname.clone(), (**p).clone());
                }
                self.eval_block(m, &arm.body, scope)
            }
        }
    }

    fn eval_name(
        &mut self,
        m: usize,
        path: &[(String, Span)],
        scope: &HashMap<String, Value>,
    ) -> EvalResult {
        let first = &path[0].0;
        if let Some(v) = scope.get(first) {
            let mut cur = v.clone();
            for (seg, _) in &path[1..] {
                cur = self.field_of(cur, seg);
            }
            return Ok(cur);
        }
        if path.len() == 1 {
            return Ok(self.consts[&(m, first.clone())].clone());
        }
        // import したモジュールの定数 (+ フィールド連鎖)
        let ModuleRef::User(mid) = self.env.modules[m].aliases[first] else {
            unreachable!("修飾名参照は検査済み")
        };
        let mut cur = self.consts[&(mid, path[1].0.clone())].clone();
        for (seg, _) in &path[2..] {
            cur = self.field_of(cur, seg);
        }
        Ok(cur)
    }

    fn field_of(&self, v: Value, name: &str) -> Value {
        let Value::Record(rec) = &v else {
            unreachable!("フィールド参照の対象はレコード (検査済み)")
        };
        let idx = self.field_index(rec.type_id, name);
        rec.fields[idx].clone()
    }

    fn eval_call(
        &mut self,
        m: usize,
        path: &[(String, Span)],
        args: &[Expr],
        span: Span,
        scope: &mut HashMap<String, Value>,
    ) -> EvalResult {
        if path.len() == 1 {
            let arg = self.eval_expr(m, &args[0], scope)?;
            return self.call_rule(m, &path[0].0, arg, span);
        }
        match self.env.modules[m].aliases[&path[0].0] {
            ModuleRef::Std(module) => {
                let b = builtins::lookup(module, &path[1].0).expect("組み込み名は検査済み");
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval_expr(m, a, scope)?);
                }
                builtins::eval(b, &vals, self.env)
                    .map_err(|msg| RuntimeError::new(msg, m, span))
            }
            ModuleRef::User(mid) => {
                let arg = self.eval_expr(m, &args[0], scope)?;
                self.call_rule(mid, &path[1].0, arg, span)
            }
        }
    }

    /// 構築式のコンストラクタ名を (修飾も考慮して) 引く。typecheck と同じ規則。
    fn ctor_hit(&self, m: usize, qualifier: Option<&str>, name: &str) -> Option<(TypeId, usize)> {
        match qualifier {
            None => self.env.modules[m].ctors.get(name).copied(),
            Some(q) => match self.env.modules[m].aliases.get(q) {
                Some(ModuleRef::User(mid)) => self.env.modules[*mid].ctors.get(name).copied(),
                _ => None,
            },
        }
    }

    fn type_id_of(&self, m: usize, qualifier: Option<&str>, name: &str) -> Option<TypeId> {
        match qualifier {
            None => self.env.modules[m].type_names.get(name).copied(),
            Some(q) => match self.env.modules[m].aliases.get(q) {
                Some(ModuleRef::Std(_)) => self.env.std_types.get(name).copied(),
                Some(ModuleRef::User(mid)) => self.env.modules[*mid].type_names.get(name).copied(),
                None => None,
            },
        }
    }

    fn eval_construct(
        &mut self,
        m: usize,
        qualifier: Option<&str>,
        name: &str,
        arg: Option<&Expr>,
        scope: &mut HashMap<String, Value>,
    ) -> EvalResult {
        if let Some((type_id, ctor)) = self.ctor_hit(m, qualifier, name) {
            let payload = match arg {
                Some(a) => Some(Rc::new(self.eval_expr(m, a, scope)?)),
                None => None,
            };
            return Ok(Value::Adt {
                type_id,
                ctor,
                payload,
            });
        }
        // 用途型・組み込み型の構築は実行時には透過
        self.eval_expr(m, arg.expect("構築引数は検査済み"), scope)
    }

    fn eval_record(
        &mut self,
        m: usize,
        qualifier: Option<&str>,
        name: &str,
        inits: &[RecordFieldInit],
        scope: &mut HashMap<String, Value>,
    ) -> EvalResult {
        let type_id = self
            .type_id_of(m, qualifier, name)
            .expect("レコード構築の型は検査済み");
        let TypeDefKind::Record(fields) = &self.env.type_def(type_id).kind else {
            unreachable!("レコード構築の型は検査済み")
        };
        let nfields = fields.len();
        // 記述順に評価し、宣言順に格納する
        let mut slots: Vec<Option<Value>> = vec![None; nfields];
        for init in inits {
            let idx = self.field_index(type_id, &init.name);
            let v = self.eval_expr(m, &init.value, scope)?;
            slots[idx] = Some(v);
        }
        let fields = slots
            .into_iter()
            .map(|s| s.expect("フィールドの過不足は検査済み"))
            .collect();
        Ok(Value::Record(Rc::new(RecordValue { type_id, fields })))
    }

    fn eval_list_construct(
        &mut self,
        m: usize,
        qualifier: Option<&str>,
        name: &str,
        elems: &[Expr],
        scope: &mut HashMap<String, Value>,
    ) -> EvalResult {
        // `Ctor (expr)` の再解釈 (typecheck と同じ規則)
        if elems.len() == 1 && self.ctor_hit(m, qualifier, name).is_some() {
            return self.eval_construct(m, qualifier, name, Some(&elems[0]), scope);
        }
        let is_list_usage = self.type_id_of(m, qualifier, name).is_some_and(|id| {
            matches!(
                &self.env.type_def(id).kind,
                TypeDefKind::Usage(crate::types::Type::List(_))
            )
        });
        if is_list_usage {
            let mut out = Vec::with_capacity(elems.len());
            for e in elems {
                out.push(self.eval_expr(m, e, scope)?);
            }
            return Ok(Value::List(Rc::new(out)));
        }
        // 非リスト用途型・組み込み型の括弧付き構築は透過
        self.eval_expr(m, &elems[0], scope)
    }

    fn eval_binary(
        &mut self,
        m: usize,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        span: Span,
        scope: &mut HashMap<String, Value>,
    ) -> EvalResult {
        use BinOp::*;
        // and / or は短絡評価
        if matches!(op, And | Or) {
            let Value::Bool(l) = self.eval_expr(m, left, scope)? else {
                unreachable!("論理演算の型は検査済み")
            };
            match (op, l) {
                (And, false) => return Ok(Value::Bool(false)),
                (Or, true) => return Ok(Value::Bool(true)),
                _ => {}
            }
            return self.eval_expr(m, right, scope);
        }

        let lv = self.eval_expr(m, left, scope)?;
        let rv = self.eval_expr(m, right, scope)?;
        match op {
            Eq => Ok(Value::Bool(lv.equals(&rv))),
            Ne => Ok(Value::Bool(!lv.equals(&rv))),
            Lt | Le | Gt | Ge => {
                let ord = compare(&lv, &rv);
                Ok(Value::Bool(match op {
                    Lt => ord.is_lt(),
                    Le => ord.is_le(),
                    Gt => ord.is_gt(),
                    Ge => ord.is_ge(),
                    _ => unreachable!(),
                }))
            }
            Add | Sub | Mul | Div | Rem => match (&lv, &rv) {
                (Value::Int(a), Value::Int(b)) => {
                    let (a, b) = (*a, *b);
                    if matches!(op, Div | Rem) && b == 0 {
                        return Err(RuntimeError::new("0 で除算しました", m, span));
                    }
                    let r = match op {
                        Add => a.checked_add(b),
                        Sub => a.checked_sub(b),
                        Mul => a.checked_mul(b),
                        Div => a.checked_div(b),
                        Rem => a.checked_rem(b),
                        _ => unreachable!(),
                    };
                    r.map(Value::Int)
                        .ok_or_else(|| RuntimeError::new("Int の範囲を超えました", m, span))
                }
                (Value::Decimal(a), Value::Decimal(b)) => {
                    if op == Div && b.is_zero() {
                        return Err(RuntimeError::new("0 で除算しました", m, span));
                    }
                    let r = match op {
                        Add => a.checked_add(*b),
                        Sub => a.checked_sub(*b),
                        Mul => a.checked_mul(*b),
                        Div => a.checked_div(*b),
                        _ => unreachable!("Decimal の % は検査で拒否済み"),
                    };
                    r.map(Value::Decimal)
                        .ok_or_else(|| RuntimeError::new("Decimal の範囲を超えました", m, span))
                }
                _ => unreachable!("算術演算の型は検査済み"),
            },
            And | Or => unreachable!("処理済み"),
        }
    }

    // ---- ヘルパ ----

    fn ctor_name(&self, type_id: TypeId, ctor: usize) -> String {
        let TypeDefKind::Adt(ctors) = &self.env.type_def(type_id).kind else {
            unreachable!("ADT 値の型は検査済み")
        };
        ctors[ctor].name.clone()
    }

    fn field_index(&self, type_id: TypeId, name: &str) -> usize {
        let TypeDefKind::Record(fields) = &self.env.type_def(type_id).kind else {
            unreachable!("レコード値の型は検査済み")
        };
        fields
            .iter()
            .position(|f| f.name == name)
            .expect("フィールド名は検査済み")
    }
}

/// 順序比較 (検査済みの同型スカラーのみ)。
fn compare(l: &Value, r: &Value) -> std::cmp::Ordering {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Decimal(a), Value::Decimal(b)) => a.cmp(b),
        (Value::Text(a), Value::Text(b)) => a.cmp(b),
        (Value::Char(a), Value::Char(b)) => a.cmp(b),
        _ => unreachable!("比較演算の型は検査済み"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Diagnostics;
    use crate::{loader, resolver, typecheck};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn with_env<R>(src: &str, f: impl FnOnce(&Env) -> R) -> R {
        let mut diags = Diagnostics::new();
        let loaded = loader::load_str("t.tcrf", src, &mut diags);
        let env = resolver::resolve(&loaded, &mut diags);
        typecheck::typecheck(&loaded, &env, &mut diags);
        assert!(
            diags.is_empty(),
            "diagnostics:\n{}",
            diags.render(&loaded.units[0].file)
        );
        f(&env)
    }

    fn call(env: &Env, rule: &str, arg: Value) -> EvalResult {
        let mut interp = Interp {
            env,
            consts: HashMap::new(),
            depth: 0,
        };
        interp.init_consts()?;
        interp.call_rule(ENTRY, rule, arg, Span::new(0, 0))
    }

    #[test]
    fn tax_calculation_runs() {
        let src = r#"T Price [Decimal]
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

F main
  Void
"#;
        with_env(src, |env| {
            let v = call(
                env,
                "calculateTotal",
                Value::Decimal(Decimal::from_str("1000.0").unwrap()),
            )
            .unwrap();
            let Value::Decimal(d) = v else { panic!("Decimal 以外: {:?}", v) };
            assert_eq!(d.normalize(), Decimal::from(1100));
        });
    }

    #[test]
    fn division_by_zero_is_error() {
        let src = r#"R div x
  Int > Int ! Error

  x / 0

F main
  Void
"#;
        with_env(src, |env| {
            let e = call(env, "div", Value::Int(1)).unwrap_err();
            assert!(e.message.contains("0 で除算"), "{}", e.message);
            assert_eq!(e.trace, vec!["R div".to_string()]);
        });
    }

    #[test]
    fn match_selects_arm_with_payload() {
        let src = r#"T Result
  | Ok Int
  | Ng

R pick r
  Result > Int

  match r

    Ok amount
      amount + 0

    Ng
      -1

F main
  Void
"#;
        with_env(src, |env| {
            let &(type_id, ctor) = env.modules[ENTRY].ctors.get("Ok").unwrap();
            let v = call(
                env,
                "pick",
                Value::Adt {
                    type_id,
                    ctor,
                    payload: Some(Rc::new(Value::Int(42))),
                },
            )
            .unwrap();
            assert!(matches!(v, Value::Int(42)));
        });
    }

    #[test]
    fn deep_recursion_is_graceful_error() {
        // CLI と同じ大スタックのスレッドで、深度上限が先に働くことを確認する
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                let src = r#"R loop x
  Int > Int ! Error

  loop (x + 1)

F main
  Void
"#;
                with_env(src, |env| {
                    let e = call(env, "loop", Value::Int(0)).unwrap_err();
                    assert!(e.message.contains("深すぎます"), "{}", e.message);
                });
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn transition_is_identity() {
        let src = r#"T OrderId [Text]
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]

R pay order
  UnpaidOrder => PaidOrder

F main
  Void
"#;
        with_env(src, |env| {
            let v = call(env, "pay", Value::text("O001")).unwrap();
            let Value::Text(s) = v else { panic!() };
            assert_eq!(s.as_str(), "O001");
        });
    }
}
