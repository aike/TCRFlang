//! コンパイラ診断 (仕様 §23)。
//! エラーコード・ファイル名・行・桁・メッセージ・修正候補を保持する。

use crate::span::{SourceFile, Span};

/// 診断コード。カテゴリごとに百番台を分ける。
/// - E1xx: 字句 (インデント含む)
/// - E2xx: 構文
/// - E3xx: 名前解決・import
/// - E4xx: 型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Code(pub &'static str);

pub mod codes {
    use super::Code;

    // 字句
    pub const LEX_INVALID_CHAR: Code = Code("E101");
    pub const LEX_TAB_INDENT: Code = Code("E102");
    pub const LEX_BAD_INDENT: Code = Code("E103");
    pub const LEX_UNTERMINATED_TEXT: Code = Code("E104");
    pub const LEX_BAD_ESCAPE: Code = Code("E105");
    pub const LEX_BAD_NUMBER: Code = Code("E106");
    pub const LEX_BAD_CHAR: Code = Code("E107");

    // 構文
    pub const PARSE_UNEXPECTED: Code = Code("E201");
    pub const PARSE_IMPORT_POSITION: Code = Code("E202");
    pub const PARSE_ADT_MULTI_PAYLOAD: Code = Code("E203");
    pub const PARSE_FLOW_FORBIDDEN: Code = Code("E204");

    // 名前解決・import
    pub const RESOLVE_UNDEFINED: Code = Code("E301");
    pub const RESOLVE_DUPLICATE: Code = Code("E302");
    pub const RESOLVE_IMPORT_CONFLICT: Code = Code("E303");
    pub const RESOLVE_MODULE_NOT_FOUND: Code = Code("E304");
    pub const RESOLVE_NAMING: Code = Code("E305");
    pub const RESOLVE_CONST_CYCLE: Code = Code("E306");
    pub const RESOLVE_CONST_CALL: Code = Code("E307");
    pub const RESOLVE_RECURSIVE_TYPE: Code = Code("E308");
    pub const RESOLVE_IMPORT_CYCLE: Code = Code("E309");
    pub const RESOLVE_EXTERNAL_RULE: Code = Code("E310");
    pub const RESOLVE_STD_DECL: Code = Code("E311");
    pub const RESOLVE_NOT_IMPLEMENTED: Code = Code("E312");

    // 型
    pub const TYPE_MISMATCH: Code = Code("E401");
    pub const TYPE_FLOW_MISMATCH: Code = Code("E402");
    pub const TYPE_RECORD_FIELDS: Code = Code("E403");
    pub const TYPE_MATCH_NOT_EXHAUSTIVE: Code = Code("E404");
    pub const TYPE_MATCH_DUPLICATE: Code = Code("E405");
    pub const TYPE_ERROR_MISUSE: Code = Code("E406");
    pub const TYPE_REASSIGNMENT: Code = Code("E407");
    pub const TYPE_SHADOWING: Code = Code("E408");
    pub const TYPE_MAIN: Code = Code("E409");
    pub const TYPE_MISSING_ERROR_MARK: Code = Code("E410");
    pub const TYPE_OPERATOR: Code = Code("E411");
    pub const TYPE_CANNOT_INFER: Code = Code("E412");
    pub const TYPE_TRANSITION: Code = Code("E413");
    pub const TYPE_UNKNOWN_TYPE: Code = Code("E414");
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: Code,
    /// 対象ファイル (ModuleUnit の番号)。単一ファイル時は 0
    pub file: usize,
    pub span: Span,
    pub message: String,
    pub hint: Option<String>,
}

/// 診断の収集器。複数件をためて一括レンダリングする。
#[derive(Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
    current_file: usize,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    /// 以後の emit が属するファイルを設定する (処理中のモジュールを切り替えるたびに呼ぶ)。
    pub fn set_file(&mut self, file: usize) {
        self.current_file = file;
    }

    pub fn emit(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.items.push(Diagnostic {
            code,
            file: self.current_file,
            span,
            message: message.into(),
            hint: None,
        });
    }

    pub fn emit_with_hint(
        &mut self,
        code: Code,
        span: Span,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) {
        self.items.push(Diagnostic {
            code,
            file: self.current_file,
            span,
            message: message.into(),
            hint: Some(hint.into()),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `file:line:col: error[Exxx]: message` 形式でレンダリングする。
    /// 該当行と桁位置のマーカーも付ける。単一ファイル用。
    pub fn render(&self, file: &SourceFile) -> String {
        self.render_multi(&[file])
    }

    /// 複数ファイル (モジュール) にまたがる診断のレンダリング。
    /// `files[d.file]` を各診断の対象ファイルとする。
    pub fn render_multi(&self, files: &[&SourceFile]) -> String {
        let mut out = String::new();
        let mut items: Vec<&Diagnostic> = self.items.iter().collect();
        items.sort_by_key(|d| (d.file, d.span.start));
        for d in items {
            let file = files.get(d.file).copied().unwrap_or(files[0]);
            let (line, col) = file.line_col(d.span.start);
            out.push_str(&format!(
                "{}:{}:{}: error[{}]: {}\n",
                file.name, line, col, d.code.0, d.message
            ));
            let text = file.line_text(line);
            out.push_str(&format!("  {}\n", text));
            let pad: String = text
                .chars()
                .take((col - 1) as usize)
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect();
            out.push_str(&format!("  {}^\n", pad));
            if let Some(hint) = &d.hint {
                out.push_str(&format!("  hint: {}\n", hint));
            }
        }
        out
    }
}
