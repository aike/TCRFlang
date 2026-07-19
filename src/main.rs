//! TCRF 処理系の CLI。
//!
//! 使い方:
//!   tcrf run <file.tcrf>    検査して実行する
//!   tcrf check <file.tcrf>  検査のみ行う
//!
//! 終了コード: 0 = 成功, 1 = 実行時 Error, 2 = コンパイルエラー/使い方誤り

use tcrf::diagnostics::Diagnostics;
use tcrf::{eval, loader, resolver, typecheck};

/// 深い再帰の例でもネイティブスタックが尽きないよう、大きめのスタックで実行する。
const STACK_SIZE: usize = 256 * 1024 * 1024;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (mode, path) = match args.as_slice() {
        [_, mode, path] if mode == "run" || mode == "check" => (mode.clone(), path.clone()),
        _ => {
            eprintln!("使い方: tcrf run <file.tcrf> | tcrf check <file.tcrf>");
            std::process::exit(2);
        }
    };

    let handle = std::thread::Builder::new()
        .name("tcrf".to_string())
        .stack_size(STACK_SIZE)
        .spawn(move || compile_and_run(&mode, &path))
        .expect("実行スレッドを作れません");
    let code = handle.join().unwrap_or(2);
    std::process::exit(code);
}

fn compile_and_run(mode: &str, path: &str) -> i32 {
    let mut diags = Diagnostics::new();
    let loaded = match loader::load_file(path, &mut diags) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("エラー: {}", msg);
            return 2;
        }
    };
    let env = resolver::resolve(&loaded, &mut diags);
    typecheck::typecheck(&loaded, &env, &mut diags);

    let files: Vec<&tcrf::span::SourceFile> = loaded.units.iter().map(|u| &u.file).collect();
    if !diags.is_empty() {
        eprint!("{}", diags.render_multi(&files));
        eprintln!("エラー: {} 件の問題が見つかりました", diags.len());
        return 2;
    }

    if mode == "check" {
        return 0;
    }

    match eval::run_main(&env) {
        Ok(()) => 0,
        Err(e) => {
            let file = files.get(e.file).copied().unwrap_or(files[0]);
            match e.span {
                Some(span) => {
                    let (line, col) = file.line_col(span.start);
                    eprintln!(
                        "{}:{}:{}: 実行時エラー: {}",
                        file.name, line, col, e.message
                    );
                }
                None => eprintln!("{}: 実行時エラー: {}", file.name, e.message),
            }
            for name in &e.trace {
                eprintln!("  {} から伝播", name);
            }
            1
        }
    }
}
