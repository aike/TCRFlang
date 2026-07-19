//! モジュールファイルの読み込みと探索 (仕様 §5.4-5.5, §22)。
//!
//! - `import a.b` はファイル `a/b.tcrf` に対応する
//! - 探索基点は優先順に (1) 処理系実行ファイルのあるディレクトリ直下の `lib/`、
//!   (2) import しているファイルのディレクトリ、(3) 環境変数 `TCRF_PATH`
//!   に列挙されたディレクトリ (OS のパス区切りで分割)。最初に見つかったものを採用
//! - `std` とその配下は組み込みで予約されており、ファイル探索しない
//! - モジュールの同一性は正規化したファイルパスで判定する (同じファイルは1回だけ読み込む)
//! - 読み込み中のモジュールへの import は循環 (E309)

use crate::ast::Program;
use crate::builtins;
use crate::diagnostics::{codes, Diagnostics};
use crate::span::{SourceFile, Span};
use crate::types::ModuleRef;
use crate::{lexer, parser};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 処理系に内蔵する標準ライブラリ宣言ファイルの既定内容。
/// 実行時は `<exe>/lib/std.tcrf` を優先し、無ければこの内容から自動生成する。
pub const STD_DECL_SRC: &str = include_str!("../lib/std.tcrf");

/// 読み込んだ1ファイル (= 1モジュール)。
pub struct ModuleUnit {
    /// ドット区切りモジュール名。エントリファイルは ""
    pub name: String,
    pub file: SourceFile,
    pub program: Program,
    /// `program.imports` と同じ並びの解決結果。None = 解決失敗 (診断済み)
    pub import_targets: Vec<Option<ModuleRef>>,
    /// 標準ライブラリ宣言ファイル (std.tcrf) か
    pub is_std: bool,
}

pub struct Loaded {
    /// 発見順。units[0] がエントリ。診断の file 番号もこの並び
    pub units: Vec<ModuleUnit>,
    /// 依存順 (依存が先)。最後がエントリ (0)。std.tcrf ユニットは含まない
    pub order: Vec<usize>,
    /// std.tcrf のユニット番号 (std を import したときだけ Some)
    pub std_unit: Option<usize>,
}

/// エントリファイルを読み込み、import を再帰的に解決する。
/// Err はエントリ自体が読めない場合のみ。モジュール側の問題は diags に出す。
pub fn load_file(path: &str, diags: &mut Diagnostics) -> Result<Loaded, String> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("{} を読み込めません: {}", path, e))?;
    let p = Path::new(path);
    let dir = p
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let canon = std::fs::canonicalize(p).ok();
    let mut loader = Loader::new(diags);
    loader.load(String::new(), path.to_string(), src, dir, canon);
    Ok(Loaded {
        units: loader.units,
        order: loader.order,
        std_unit: loader.std_unit,
    })
}

/// テスト用: メモリ上のソースをエントリとして読み込む。
/// import の探索基点はカレントディレクトリになる。
pub fn load_str(name: &str, src: &str, diags: &mut Diagnostics) -> Loaded {
    let mut loader = Loader::new(diags);
    loader.load(
        String::new(),
        name.to_string(),
        src.to_string(),
        PathBuf::from("."),
        None,
    );
    Loaded {
        units: loader.units,
        order: loader.order,
        std_unit: loader.std_unit,
    }
}

fn search_path_from_env() -> Vec<PathBuf> {
    match std::env::var_os("TCRF_PATH") {
        Some(v) => std::env::split_paths(&v).collect(),
        None => Vec::new(),
    }
}

/// 処理系実行ファイルのあるディレクトリ直下の `lib/` (最優先の探索先)。
fn exe_lib_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("lib"))
}

struct Loader<'d> {
    diags: &'d mut Diagnostics,
    units: Vec<ModuleUnit>,
    order: Vec<usize>,
    /// 正規化パス → ユニット番号
    by_path: HashMap<PathBuf, usize>,
    /// 読み込み中 (DFS スタック上) のユニット
    active: Vec<usize>,
    /// 実行ファイル直下の lib/ (最優先)
    exe_lib: Option<PathBuf>,
    search_path: Vec<PathBuf>,
    /// 読み込み済み std.tcrf のユニット番号
    std_unit: Option<usize>,
}

impl<'d> Loader<'d> {
    fn new(diags: &'d mut Diagnostics) -> Self {
        Loader {
            diags,
            units: Vec::new(),
            order: Vec::new(),
            by_path: HashMap::new(),
            active: Vec::new(),
            exe_lib: exe_lib_dir(),
            search_path: search_path_from_env(),
            std_unit: None,
        }
    }

    /// std.tcrf (標準ライブラリ宣言ファイル) を1回だけ読み込む。
    /// `<exe>/lib/std.tcrf` があればそれを使い、無ければ内蔵コピーから
    /// 自動生成を試み、生成できない場合も内蔵コピーで続行する。
    fn ensure_std_loaded(&mut self) {
        if self.std_unit.is_some() {
            return;
        }
        let (file_name, src) = self.std_source();
        let id = self.units.len();
        self.diags.set_file(id);
        let file = SourceFile::new(file_name, src);
        let tokens = lexer::lex(&file, self.diags);
        let program = parser::parse(tokens, self.diags);
        self.units.push(ModuleUnit {
            name: "std".to_string(),
            file,
            program,
            import_targets: Vec::new(),
            is_std: true,
        });
        self.std_unit = Some(id);
    }

    fn std_source(&self) -> (String, String) {
        if let Some(lib) = &self.exe_lib {
            let path = lib.join("std.tcrf");
            if let Ok(src) = std::fs::read_to_string(&path) {
                return (path.display().to_string(), src);
            }
            // 無ければ自動生成する (一時ファイル経由で並行実行と衝突しないように)
            let _ = std::fs::create_dir_all(lib);
            let tmp = lib.join(format!("std.tcrf.tmp-{}", std::process::id()));
            if std::fs::write(&tmp, STD_DECL_SRC).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
                let _ = std::fs::remove_file(&tmp);
            }
            if let Ok(src) = std::fs::read_to_string(&path) {
                return (path.display().to_string(), src);
            }
        }
        ("std.tcrf (内蔵)".to_string(), STD_DECL_SRC.to_string())
    }

    fn load(
        &mut self,
        module_name: String,
        file_name: String,
        src: String,
        dir: PathBuf,
        canon: Option<PathBuf>,
    ) -> usize {
        let id = self.units.len();
        if let Some(c) = &canon {
            self.by_path.insert(c.clone(), id);
        }
        self.diags.set_file(id);
        let file = SourceFile::new(file_name, src);
        let tokens = lexer::lex(&file, self.diags);
        let program = parser::parse(tokens, self.diags);
        self.units.push(ModuleUnit {
            name: module_name,
            file,
            program,
            import_targets: Vec::new(),
            is_std: false,
        });
        self.active.push(id);

        let imports: Vec<(Vec<String>, Span)> = self.units[id]
            .program
            .imports
            .iter()
            .map(|im| (im.path.clone(), im.span))
            .collect();
        let mut targets = Vec::with_capacity(imports.len());
        for (path, span) in imports {
            targets.push(self.resolve_import(id, &path, span, &dir));
        }
        self.units[id].import_targets = targets;
        self.active.pop();
        self.order.push(id);
        id
    }

    fn resolve_import(
        &mut self,
        from: usize,
        path: &[String],
        span: Span,
        dir: &Path,
    ) -> Option<ModuleRef> {
        // std とその配下は予約名。ファイル探索せず、宣言ファイル std.tcrf を読み込む
        if path[0] == "std" {
            match builtins::module_from_path(path) {
                Some(m) => {
                    self.ensure_std_loaded();
                    return Some(ModuleRef::Std(m));
                }
                None => {
                    self.diags.set_file(from);
                    self.diags.emit_with_hint(
                        codes::RESOLVE_MODULE_NOT_FOUND,
                        span,
                        format!("モジュール `{}` が見つかりません", path.join(".")),
                        "std 配下は組み込みで、std.console / std.list / std.range / std.format / std.text / std.number / std.validate があります",
                    );
                    return None;
                }
            }
        }

        // a.b → a/b.tcrf を、実行ファイル直下の lib/ → import 元のディレクトリ
        // → TCRF_PATH の順に探す
        let rel: PathBuf = path.iter().collect::<PathBuf>().with_extension("tcrf");
        let mut bases = Vec::new();
        if let Some(lib) = &self.exe_lib {
            bases.push(lib.clone());
        }
        bases.push(dir.to_path_buf());
        bases.extend(self.search_path.iter().cloned());
        let Some(found) = bases.iter().map(|b| b.join(&rel)).find(|c| c.is_file()) else {
            self.diags.set_file(from);
            let searched: Vec<String> = bases
                .iter()
                .map(|b| b.join(&rel).display().to_string())
                .collect();
            self.diags.emit_with_hint(
                codes::RESOLVE_MODULE_NOT_FOUND,
                span,
                format!("モジュール `{}` が見つかりません", path.join(".")),
                format!("探索したパス: {}", searched.join(", ")),
            );
            return None;
        };

        let canon = std::fs::canonicalize(&found).unwrap_or_else(|_| found.clone());
        if let Some(&tid) = self.by_path.get(&canon) {
            if self.active.contains(&tid) {
                self.diags.set_file(from);
                self.diags.emit_with_hint(
                    codes::RESOLVE_IMPORT_CYCLE,
                    span,
                    format!("モジュール `{}` の import が循環しています", path.join(".")),
                    "モジュール間の依存は一方向にしてください",
                );
                return None;
            }
            return Some(ModuleRef::User(tid));
        }

        let src = match std::fs::read_to_string(&found) {
            Ok(s) => s,
            Err(e) => {
                self.diags.set_file(from);
                self.diags.emit(
                    codes::RESOLVE_MODULE_NOT_FOUND,
                    span,
                    format!(
                        "モジュール `{}` ({}) を読み込めません: {}",
                        path.join("."),
                        found.display(),
                        e
                    ),
                );
                return None;
            }
        };
        let mdir = canon
            .parent()
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let tid = self.load(
            path.join("."),
            found.display().to_string(),
            src,
            mdir,
            Some(canon),
        );
        // 再帰読み込みで診断ファイルが切り替わっているので戻す
        self.diags.set_file(from);
        Some(ModuleRef::User(tid))
    }
}
