//! 標準ライブラリ (`std`) の組み込み実装。
//! 型シグネチャは宣言ファイル std.tcrf (stdsig) が持ち、
//! ここは名前解決 (`lookup`) と実行 (`eval`) を担当する。

use crate::types::Env;
use crate::value::Value;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::io::{BufRead, Write as _};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdModule {
    Std,
    Console,
    List,
    Range,
    Format,
    Text,
    Number,
    Validate,
}

/// import パス (例: ["std", "console"]) からモジュールを引く。
pub fn module_from_path(path: &[String]) -> Option<StdModule> {
    let segs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
    match segs.as_slice() {
        ["std"] => Some(StdModule::Std),
        ["std", "console"] => Some(StdModule::Console),
        ["std", "list"] => Some(StdModule::List),
        ["std", "range"] => Some(StdModule::Range),
        ["std", "format"] => Some(StdModule::Format),
        ["std", "text"] => Some(StdModule::Text),
        ["std", "number"] => Some(StdModule::Number),
        ["std", "validate"] => Some(StdModule::Validate),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Print,
    PrintLine,
    ReadText,
    Debug,
    IsEmpty,
    First,
    Rest,
    Prepend,
    Length,
    Reverse,
    Append,
    Contains,
    SumInt,
    SumDecimal,
    Inclusive,
    Exclusive,
    FmtInt,
    FmtDecimal,
    FmtBool,
    IntList,
    DecimalList,
    TextList,
    TextLength,
    Trim,
    Lower,
    Upper,
    Concat,
    TextContains,
    ParseInt,
    ParseDecimal,
    ToDecimal,
    Floor,
    Round,
    AbsInt,
    AbsDecimal,
    Require,
    RequireNotEmpty,
}

/// モジュール内の公開名から組み込みを引く (`empty` は言語側で特別扱い)。
pub fn lookup(module: StdModule, name: &str) -> Option<Builtin> {
    use Builtin::*;
    use StdModule::*;
    match (module, name) {
        (Std | Console, "print") => Some(Print),
        (Std | Console, "printLine") => Some(PrintLine),
        (Std | Console, "readText") => Some(ReadText),
        (Std | Console, "debug") => Some(Debug),
        (Std | List, "isEmpty") => Some(IsEmpty),
        (Std | List, "first") => Some(First),
        (Std | List, "rest") => Some(Rest),
        (Std | List, "prepend") => Some(Prepend),
        (Std | List, "length") => Some(Length),
        (Std | List, "reverse") => Some(Reverse),
        (Std | List, "append") => Some(Append),
        (Std | List, "contains") => Some(Contains),
        (Std | List, "sumInt") => Some(SumInt),
        (Std | List, "sumDecimal") => Some(SumDecimal),
        (Std | Range, "inclusive") => Some(Inclusive),
        (Std | Range, "exclusive") => Some(Exclusive),
        (Std | Format, "int") => Some(FmtInt),
        (Std | Format, "decimal") => Some(FmtDecimal),
        (Std | Format, "bool") => Some(FmtBool),
        (Std | Format, "intList") => Some(IntList),
        (Std | Format, "decimalList") => Some(DecimalList),
        (Std | Format, "textList") => Some(TextList),
        (Std, "trim") | (Text, "trim") => Some(Trim),
        (Std, "lower") | (Text, "lower") => Some(Lower),
        (Std, "upper") | (Text, "upper") => Some(Upper),
        (Std, "concat") | (Text, "concat") => Some(Concat),
        (Std, "textContains") | (Text, "contains") => Some(TextContains),
        (Std, "parseInt") | (Text, "parseInt") => Some(ParseInt),
        (Std, "parseDecimal") | (Text, "parseDecimal") => Some(ParseDecimal),
        (Std, "textLength") | (Text, "length") => Some(TextLength),
        (Std | Number, "toDecimal") => Some(ToDecimal),
        (Std | Number, "floor") => Some(Floor),
        (Std | Number, "round") => Some(Round),
        (Std | Number, "absInt") => Some(AbsInt),
        (Std | Number, "absDecimal") => Some(AbsDecimal),
        (Std | Validate, "require") => Some(Require),
        (Std | Validate, "requireNotEmpty") => Some(RequireNotEmpty),
        _ => None,
    }
}

/// `std.empty<T>` を提供するモジュールか。
pub fn has_empty(module: StdModule) -> bool {
    matches!(module, StdModule::Std | StdModule::List)
}

/// `RangeInput` 型を公開するモジュールか。
pub fn has_range_input(module: StdModule) -> bool {
    matches!(module, StdModule::Std | StdModule::Range)
}

/// std.tcrf での宣言名。型検査はこの名前で署名表 (stdsig) を引く。
pub fn sig_name(b: Builtin) -> &'static str {
    use Builtin::*;
    match b {
        Print => "print",
        PrintLine => "printLine",
        ReadText => "readText",
        Debug => "debug",
        IsEmpty => "isEmpty",
        First => "first",
        Rest => "rest",
        Prepend => "prepend",
        Length => "length",
        Reverse => "reverse",
        Append => "append",
        Contains => "contains",
        SumInt => "sumInt",
        SumDecimal => "sumDecimal",
        Inclusive => "inclusive",
        Exclusive => "exclusive",
        FmtInt => "int",
        FmtDecimal => "decimal",
        FmtBool => "bool",
        IntList => "intList",
        DecimalList => "decimalList",
        TextList => "textList",
        TextLength => "textLength",
        Trim => "trim",
        Lower => "lower",
        Upper => "upper",
        Concat => "concat",
        TextContains => "textContains",
        ParseInt => "parseInt",
        ParseDecimal => "parseDecimal",
        ToDecimal => "toDecimal",
        Floor => "floor",
        Round => "round",
        AbsInt => "absInt",
        AbsDecimal => "absDecimal",
        Require => "require",
        RequireNotEmpty => "requireNotEmpty",
    }
}

/// std.tcrf の宣言が組み込み実装と食い違っていた場合のエラー。
/// (宣言ファイルは編集できるため、実行時の値形状は保証されない)
fn mismatch() -> String {
    "std.tcrf の宣言と組み込み実装の型が一致しません".to_string()
}

/// 実行。失敗は Err (メッセージは処理系内部用)。
pub fn eval(b: Builtin, args: &[Value], env: &Env) -> Result<Value, String> {
    use Builtin::*;

    let arg = |i: usize| -> Result<&Value, String> { args.get(i).ok_or_else(mismatch) };
    let as_text = |i: usize| -> Result<Rc<String>, String> {
        match arg(i)? {
            Value::Text(s) => Ok(s.clone()),
            _ => Err(mismatch()),
        }
    };
    let as_list = |i: usize| -> Result<Rc<Vec<Value>>, String> {
        match arg(i)? {
            Value::List(xs) => Ok(xs.clone()),
            _ => Err(mismatch()),
        }
    };
    let as_int = |i: usize| -> Result<i64, String> {
        match arg(i)? {
            Value::Int(n) => Ok(*n),
            _ => Err(mismatch()),
        }
    };
    let as_decimal = |i: usize| -> Result<Decimal, String> {
        match arg(i)? {
            Value::Decimal(d) => Ok(*d),
            _ => Err(mismatch()),
        }
    };
    let as_bool = |i: usize| -> Result<bool, String> {
        match arg(i)? {
            Value::Bool(v) => Ok(*v),
            _ => Err(mismatch()),
        }
    };
    let elem_int = |v: &Value| -> Result<i64, String> {
        match v {
            Value::Int(n) => Ok(*n),
            _ => Err(mismatch()),
        }
    };
    let elem_decimal = |v: &Value| -> Result<Decimal, String> {
        match v {
            Value::Decimal(d) => Ok(*d),
            _ => Err(mismatch()),
        }
    };

    match b {
        Print => {
            print!("{}", as_text(0)?);
            std::io::stdout().flush().ok();
            Ok(Value::Void)
        }
        PrintLine => {
            println!("{}", as_text(0)?);
            Ok(Value::Void)
        }
        ReadText => {
            let mut line = String::new();
            let n = std::io::stdin()
                .lock()
                .read_line(&mut line)
                .map_err(|e| format!("標準入力の読み取りに失敗しました: {}", e))?;
            if n == 0 {
                return Err("標準入力が終端に達しました".to_string());
            }
            while line.ends_with('\n') || line.ends_with('\r') {
                line.pop();
            }
            Ok(Value::text(line))
        }
        Debug => {
            println!("{}", debug_value(arg(0)?, env));
            Ok(Value::Void)
        }
        IsEmpty => Ok(Value::Bool(as_list(0)?.is_empty())),
        First => {
            let xs = as_list(0)?;
            xs.first()
                .cloned()
                .ok_or_else(|| "空リストの先頭要素は取れません".to_string())
        }
        Rest => {
            let xs = as_list(0)?;
            if xs.is_empty() {
                Err("空リストの残り要素は取れません".to_string())
            } else {
                Ok(Value::List(Rc::new(xs[1..].to_vec())))
            }
        }
        Prepend => {
            let xs = as_list(1)?;
            let mut out = Vec::with_capacity(xs.len() + 1);
            out.push(arg(0)?.clone());
            out.extend(xs.iter().cloned());
            Ok(Value::List(Rc::new(out)))
        }
        Length => Ok(Value::Int(as_list(0)?.len() as i64)),
        Reverse => {
            let xs = as_list(0)?;
            Ok(Value::List(Rc::new(xs.iter().rev().cloned().collect())))
        }
        Append => {
            let l = as_list(0)?;
            let r = as_list(1)?;
            let mut out = Vec::with_capacity(l.len() + r.len());
            out.extend(l.iter().cloned());
            out.extend(r.iter().cloned());
            Ok(Value::List(Rc::new(out)))
        }
        Contains => {
            let xs = as_list(1)?;
            let needle = arg(0)?;
            Ok(Value::Bool(xs.iter().any(|x| x.equals(needle))))
        }
        SumInt => {
            let mut sum: i64 = 0;
            for v in as_list(0)?.iter() {
                sum = sum
                    .checked_add(elem_int(v)?)
                    .ok_or_else(|| "Int の範囲を超えました".to_string())?;
            }
            Ok(Value::Int(sum))
        }
        SumDecimal => {
            let mut sum = Decimal::ZERO;
            for v in as_list(0)?.iter() {
                sum += elem_decimal(v)?;
            }
            Ok(Value::Decimal(sum))
        }
        Inclusive | Exclusive => {
            let Value::Record(rec) = arg(0)? else {
                return Err(mismatch());
            };
            let (first, last) = match rec.fields.as_slice() {
                [Value::Int(f), Value::Int(l)] => (*f, *l),
                _ => return Err(mismatch()),
            };
            if first > last {
                return Err(format!(
                    "first ({}) > last ({}) の範囲は作れません",
                    first, last
                ));
            }
            let end = if b == Inclusive { last } else { last - 1 };
            let mut out = Vec::new();
            let mut i = first;
            while i <= end {
                out.push(Value::Int(i));
                i += 1;
            }
            Ok(Value::List(Rc::new(out)))
        }
        FmtInt => Ok(Value::text(as_int(0)?.to_string())),
        FmtDecimal => Ok(Value::text(format_decimal(as_decimal(0)?))),
        FmtBool => Ok(Value::text(if as_bool(0)? { "true" } else { "false" })),
        IntList => {
            let mut items = Vec::new();
            for v in as_list(0)?.iter() {
                items.push(elem_int(v)?.to_string());
            }
            Ok(Value::text(format!("[{}]", items.join(", "))))
        }
        DecimalList => {
            let mut items = Vec::new();
            for v in as_list(0)?.iter() {
                items.push(format_decimal(elem_decimal(v)?));
            }
            Ok(Value::text(format!("[{}]", items.join(", "))))
        }
        TextList => {
            let mut items = Vec::new();
            for v in as_list(0)?.iter() {
                match v {
                    Value::Text(s) => items.push(format!("\"{}\"", s)),
                    _ => return Err(mismatch()),
                }
            }
            Ok(Value::text(format!("[{}]", items.join(", "))))
        }
        TextLength => Ok(Value::Int(as_text(0)?.chars().count() as i64)),
        Trim => Ok(Value::text(as_text(0)?.trim().to_string())),
        Lower => Ok(Value::text(as_text(0)?.to_lowercase())),
        Upper => Ok(Value::text(as_text(0)?.to_uppercase())),
        Concat => Ok(Value::text(format!("{}{}", as_text(0)?, as_text(1)?))),
        TextContains => Ok(Value::Bool(as_text(0)?.contains(as_text(1)?.as_str()))),
        ParseInt => {
            let s = as_text(0)?;
            s.trim()
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| format!("Int として解釈できません: \"{}\"", s))
        }
        ParseDecimal => {
            let s = as_text(0)?;
            Decimal::from_str(s.trim())
                .map(Value::Decimal)
                .map_err(|_| format!("Decimal として解釈できません: \"{}\"", s))
        }
        ToDecimal => Ok(Value::Decimal(Decimal::from(as_int(0)?))),
        Floor => {
            let d = as_decimal(0)?.floor();
            Ok(Value::Int(d.to_i64().unwrap_or(i64::MAX)))
        }
        Round => {
            let d = as_decimal(0)?
                .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
            Ok(Value::Int(d.to_i64().unwrap_or(i64::MAX)))
        }
        AbsInt => Ok(Value::Int(as_int(0)?.abs())),
        AbsDecimal => Ok(Value::Decimal(as_decimal(0)?.abs())),
        Require => {
            if as_bool(0)? {
                Ok(Value::Void)
            } else {
                Err("require の条件が false でした".to_string())
            }
        }
        RequireNotEmpty => {
            let xs = as_list(0)?;
            if xs.is_empty() {
                Err("リストが空です".to_string())
            } else {
                Ok(arg(0)?.clone())
            }
        }
    }
}

/// `std.decimal` の表示形式: 末尾の余分な 0 を落とす。
pub fn format_decimal(d: Decimal) -> String {
    d.normalize().to_string()
}

/// `std.debug` 用の処理系依存表示。
pub fn debug_value(v: &Value, env: &Env) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Decimal(d) => d.to_string(),
        Value::Text(s) => format!("\"{}\"", s),
        Value::Char(c) => format!("'{}'", c),
        Value::Bool(b) => b.to_string(),
        Value::Void => "Void".to_string(),
        Value::List(xs) => {
            let items: Vec<String> = xs.iter().map(|x| debug_value(x, env)).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Record(rec) => {
            let def = env.type_def(rec.type_id);
            let crate::types::TypeDefKind::Record(fields) = &def.kind else {
                return format!("{} {{ ... }}", def.name);
            };
            let items: Vec<String> = fields
                .iter()
                .zip(rec.fields.iter())
                .map(|(f, v)| format!("{} = {}", f.name, debug_value(v, env)))
                .collect();
            format!("{} {{ {} }}", def.name, items.join(", "))
        }
        Value::Adt {
            type_id,
            ctor,
            payload,
        } => {
            let def = env.type_def(*type_id);
            let crate::types::TypeDefKind::Adt(ctors) = &def.kind else {
                return def.name.clone();
            };
            let name = &ctors[*ctor].name;
            match payload {
                Some(p) => format!("{} {}", name, debug_value(p, env)),
                None => name.clone(),
            }
        }
    }
}
