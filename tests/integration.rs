//! examples/ の end-to-end 実行と、代表的なコンパイルエラーの検証。

use std::path::PathBuf;
use std::process::Command;

fn run(mode: &str, path: &str) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_tcrf"))
        .args([mode, path])
        .output()
        .expect("tcrf を起動できません");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    )
}

fn example(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name);
    p.to_string_lossy().into_owned()
}

/// 一時ファイルにソースを書いて実行する (ネガティブテスト用)。
fn run_src(mode: &str, name: &str, src: &str) -> (i32, String, String) {
    let path = std::env::temp_dir().join(format!("tcrf_test_{}.tcrf", name));
    std::fs::write(&path, src).unwrap();
    let r = run(mode, &path.to_string_lossy());
    std::fs::remove_file(&path).ok();
    r
}

#[test]
fn hello_prints_hello_world() {
    let (code, stdout, stderr) = run("run", &example("hello.tcrf"));
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "Hello, World!\n");
}

#[test]
fn tax_prints_total() {
    let (code, stdout, stderr) = run("run", &example("tax.tcrf"));
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "1100\n");
}

#[test]
fn grade_prints_passed() {
    let (code, stdout, stderr) = run("run", &example("grade.tcrf"));
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "Passed\n");
}

#[test]
fn order_prints_shipped() {
    let (code, stdout, stderr) = run("run", &example("order.tcrf"));
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "Order shipped\n");
}

#[test]
fn sieve_prints_primes_up_to_100() {
    let (code, stdout, stderr) = run("run", &example("sieve.tcrf"));
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(
        stdout,
        "[2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97]\n"
    );
}

#[test]
fn discount_example_uses_module() {
    // examples/discount.tcrf が同ディレクトリの pricing.tcrf を import する
    let (code, stdout, stderr) = run("run", &example("discount.tcrf"));
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "800\n");
}

#[test]
fn check_mode_runs_nothing() {
    let (code, stdout, _) = run("check", &example("hello.tcrf"));
    assert_eq!(code, 0);
    assert_eq!(stdout, "");
}

#[test]
fn compile_error_exits_2_with_code() {
    // 未払い注文を直接 ship する型遷移違反 (E402)
    let src = r#"T OrderId [Text]
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]

R ship order
  PaidOrder => ShippedOrder

R report order
  ShippedOrder > Void

  Void

F main
  UnpaidOrder (OrderId "X")
  ship
  report
"#;
    let (code, _, stderr) = run_src("check", "flow_mismatch", src);
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("E402"), "stderr: {}", stderr);
}

#[test]
fn missing_error_mark_is_e410() {
    let src = r#"import std

R firstOf values
  List<Int> > Int

  std.first values

F main
  Void
"#;
    let (code, _, stderr) = run_src("check", "missing_mark", src);
    assert_eq!(code, 2);
    assert!(stderr.contains("E410"), "stderr: {}", stderr);
}

#[test]
fn runtime_error_exits_1() {
    let src = r#"import std

T Input {
  first Int
  last  Int
}

R build x
  Void > List<Int> ! Error

  std.inclusive Input {
    first = 10
    last  = 1
  }

R asText values
  List<Int> > Text

  std.intList values

F main
  Void
  build
  asText
  std.printLine
"#;
    let (code, stdout, stderr) = run_src("run", "runtime_error", src);
    assert_eq!(code, 1, "stdout: {} stderr: {}", stdout, stderr);
    assert!(stderr.contains("実行時エラー"), "stderr: {}", stderr);
    assert!(stderr.contains("R build"), "stderr: {}", stderr);
}

#[test]
fn from_expression_preserves_value() {
    // `A from x` は値をそのまま持つ A 型を作る。`=>` の展開形と同じ動作
    let src = r#"import std

T OrderId [Text]
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]

R pay order
  UnpaidOrder > PaidOrder

  paid : PaidOrder =
    PaidOrder from order

  paid

R ship order
  PaidOrder => ShippedOrder

R shippedText order
  ShippedOrder > Text

  Text "shipped"

F main
  UnpaidOrder (OrderId "O001")
  pay
  ship
  shippedText
  std.printLine
"#;
    let (code, stdout, stderr) = run_src("run", "from_expr", src);
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "shipped\n");
}

#[test]
fn comments_do_not_change_program_behavior() {
    // 行全体・行末・括弧内・ブロック途中のコメントを混ぜても出力は同じ (§2.1)
    let src = r##"# プログラム全体のコメント
import std  # 標準ライブラリ

T Total [Int]  # 用途型

T Pair {        # レコード
  a Int         # 1つめ
  b Int         # 2つめ
}

R add p
  # ブロック途中のコメント (浅い)
  Pair > Total
      # ブロック途中のコメント (深い)
  Total (p.a + p.b)  # 構築して返す

R toText t
  Total > Text

  std.int t  # "#" を含む文字列も平気: "a # b"

F main
  Pair {
    a = 40  # 行末
    b = 2
  }
  add
  toText
  std.printLine
# 末尾コメント"##;
    let (code, stdout, stderr) = run_src("run", "comments", src);
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "42\n");
}

#[test]
fn tab_indent_is_lex_error() {
    let src = "F main\n\tVoid\n";
    let (code, _, stderr) = run_src("check", "tab_indent", src);
    assert_eq!(code, 2);
    assert!(stderr.contains("E102"), "stderr: {}", stderr);
}

// ---- ユーザー定義モジュール ----

/// テストごとに独立したディレクトリへ複数ファイルを書き、エントリを実行する。
fn run_project(mode: &str, name: &str, files: &[(&str, &str)]) -> (i32, String, String) {
    let dir = std::env::temp_dir().join(format!("tcrf_test_{}", name));
    std::fs::remove_dir_all(&dir).ok();
    for (fname, src) in files {
        let path = dir.join(fname);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, src).unwrap();
    }
    let entry = dir.join(files[0].0);
    let r = run(mode, &entry.to_string_lossy());
    std::fs::remove_dir_all(&dir).ok();
    r
}

const MYLIB: &str = r#"T Doubled [Int]

C base = 100

C _hidden = 1

R double x
  Int > Doubled

  Doubled (x + x)

R _secret x
  Int > Int

  x
"#;

#[test]
fn user_library_import_works() {
    // エントリと同じディレクトリの mylib.tcrf を import して R・C・型を使う
    let main = r#"import std
import mylib

R asText d
  mylib.Doubled > Text

  std.int d

F main
  21
  mylib.double
  asText
  std.printLine
"#;
    let (code, stdout, stderr) = run_project(
        "run",
        "user_import",
        &[("main.tcrf", main), ("mylib.tcrf", MYLIB)],
    );
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "42\n");
}

#[test]
fn user_library_alias_and_const() {
    // `as` 別名と、モジュール公開定数の C からの参照
    let main = r#"import std
import mylib as ml

C start = ml.base

F main
  start
  ml.double
  toText
  std.printLine

R toText d
  ml.Doubled > Text

  std.int d
"#;
    let (code, stdout, stderr) = run_project(
        "run",
        "alias_const",
        &[("main.tcrf", main), ("mylib.tcrf", MYLIB)],
    );
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "200\n");
}

#[test]
fn dotted_module_maps_to_subdirectory() {
    // import utils.math → utils/math.tcrf。既定修飾名は最終要素 math
    let main = r#"import std
import utils.math

F main
  5
  math.square
  math.intText
  std.printLine
"#;
    let lib = r#"import std

R square x
  Int > Int

  x * x

R intText x
  Int > Text

  std.int x
"#;
    let (code, stdout, stderr) = run_project(
        "run",
        "dotted",
        &[("main.tcrf", main), ("utils/math.tcrf", lib)],
    );
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "25\n");
}

#[test]
fn missing_module_is_e304_with_search_paths() {
    let main = "import nosuchlib\n\nF main\n  Void\n";
    let (code, _, stderr) = run_project("check", "missing_mod", &[("main.tcrf", main)]);
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("E304"), "stderr: {}", stderr);
    assert!(stderr.contains("探索したパス"), "stderr: {}", stderr);
}

#[test]
fn unknown_std_submodule_is_e304() {
    // std 配下は予約されており、ファイル探索されない
    let (code, _, stderr) = run_src("check", "std_submodule", "import std.mylib\n\nF main\n  Void\n");
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("E304"), "stderr: {}", stderr);
    assert!(stderr.contains("std.mylib"), "stderr: {}", stderr);
}

#[test]
fn import_cycle_is_e309() {
    let a = "import b\n\nF main\n  Void\n";
    let b = "import a\n\nR id x\n  Int > Int\n\n  x\n";
    let (code, _, stderr) = run_project("check", "cycle", &[("a.tcrf", a), ("b.tcrf", b)]);
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("E309"), "stderr: {}", stderr);
    assert!(stderr.contains("循環"), "stderr: {}", stderr);
}

#[test]
fn underscore_names_are_private() {
    let main = r#"import mylib

F main
  1
  mylib._secret
"#;
    let (code, _, stderr) = run_project(
        "check",
        "private",
        &[("main.tcrf", main), ("mylib.tcrf", MYLIB)],
    );
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("公開されていません"), "stderr: {}", stderr);
}

#[test]
fn module_cannot_define_main() {
    let main = "import mylib\n\nF main\n  Void\n";
    let lib = "F main\n  Void\n";
    let (code, _, stderr) = run_project(
        "check",
        "mod_main",
        &[("main.tcrf", main), ("mylib.tcrf", lib)],
    );
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("E409"), "stderr: {}", stderr);
    assert!(
        stderr.contains("モジュールには `F main` を定義できません"),
        "stderr: {}",
        stderr
    );
}

#[test]
fn tcrf_path_is_searched_after_local_dir() {
    // ライブラリを別ディレクトリに置き、TCRF_PATH 経由で見つける
    let libdir = std::env::temp_dir().join("tcrf_test_libpath_lib");
    let maindir = std::env::temp_dir().join("tcrf_test_libpath_main");
    std::fs::remove_dir_all(&libdir).ok();
    std::fs::remove_dir_all(&maindir).ok();
    std::fs::create_dir_all(&libdir).unwrap();
    std::fs::create_dir_all(&maindir).unwrap();
    std::fs::write(libdir.join("mylib.tcrf"), MYLIB).unwrap();
    let main = r#"import std
import mylib

R toText d
  mylib.Doubled > Text

  std.int d

F main
  3
  mylib.double
  toText
  std.printLine
"#;
    let entry = maindir.join("main.tcrf");
    std::fs::write(&entry, main).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tcrf"))
        .args(["run", &entry.to_string_lossy()])
        .env("TCRF_PATH", &libdir)
        .output()
        .expect("tcrf を起動できません");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n");
    std::fs::remove_dir_all(&libdir).ok();
    std::fs::remove_dir_all(&maindir).ok();
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "6\n");
}

// ---- 標準ライブラリ宣言ファイル std.tcrf ----

#[test]
fn std_tcrf_is_auto_created_next_to_exe() {
    // std を import するプログラムを実行すると <exe>/lib/std.tcrf が自動生成される
    let (code, _, stderr) = run("run", &example("hello.tcrf"));
    assert_eq!(code, 0, "stderr: {}", stderr);
    let std_file = exe_lib_dir().join("std.tcrf");
    assert!(std_file.is_file(), "{} がありません", std_file.display());
    let content = std::fs::read_to_string(&std_file).unwrap();
    assert!(content.contains("R printLine"), "std.tcrf の内容が不正");
}

#[test]
fn declared_but_unimplemented_std_is_e312() {
    // std.tcrf に宣言のみある (未実装コメント付き) sort の呼び出し
    let src = r#"import std

R sorted values
  List<Int> > List<Int>

  std.sort values

F main
  Void
"#;
    let (code, _, stderr) = run_src("check", "unimplemented", src);
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("E312"), "stderr: {}", stderr);
    assert!(stderr.contains("未実装"), "stderr: {}", stderr);
}

#[test]
fn external_rule_outside_std_is_e310() {
    // 本体のない R 宣言はユーザーコードでは書けない
    let src = "R f x\n  Int > Int\n\nF main\n  Void\n";
    let (code, _, stderr) = run_src("check", "external_rule", src);
    assert_eq!(code, 2, "stderr: {}", stderr);
    assert!(stderr.contains("E310"), "stderr: {}", stderr);
}

/// 実行バイナリ (CARGO_BIN_EXE_tcrf) のあるディレクトリ直下の lib/。
fn exe_lib_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_BIN_EXE_tcrf"))
        .parent()
        .unwrap()
        .join("lib")
}

#[test]
fn exe_lib_dir_is_searched() {
    // tcrf.exe 直下の lib/ に置いたモジュールを import できる
    let lib = exe_lib_dir();
    std::fs::create_dir_all(&lib).unwrap();
    let modfile = lib.join("exeonlylib.tcrf");
    std::fs::write(&modfile, "R triple x\n  Int > Int\n\n  x * 3\n").unwrap();
    let main = r#"import std
import exeonlylib

R toText x
  Int > Text

  std.int x

F main
  5
  exeonlylib.triple
  toText
  std.printLine
"#;
    let (code, stdout, stderr) = run_src("run", "exe_lib", main);
    std::fs::remove_file(&modfile).ok();
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "15\n");
}

#[test]
fn exe_lib_dir_has_highest_priority() {
    // 同名モジュールがエントリと同じディレクトリにもある場合、exe 直下 lib/ が勝つ
    let lib = exe_lib_dir();
    std::fs::create_dir_all(&lib).unwrap();
    let modfile = lib.join("priolib.tcrf");
    std::fs::write(
        &modfile,
        "R which x\n  Void > Text\n\n  Text \"from exe lib\"\n",
    )
    .unwrap();
    let main = r#"import std
import priolib

F main
  Void
  priolib.which
  std.printLine
"#;
    let local = "R which x\n  Void > Text\n\n  Text \"from local dir\"\n";
    let (code, stdout, stderr) = run_project(
        "run",
        "prio",
        &[("main.tcrf", main), ("priolib.tcrf", local)],
    );
    std::fs::remove_file(&modfile).ok();
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "from exe lib\n");
}

#[test]
fn diamond_import_shares_one_module_instance() {
    // エントリと中間モジュールの両方が mylib を import しても型は同一
    let main = r#"import std
import mylib
import middle

F main
  7
  mylib.double
  middle.describe
  std.printLine
"#;
    let middle = r#"import std
import mylib

R describe d
  mylib.Doubled > Text

  std.int d
"#;
    let (code, stdout, stderr) = run_project(
        "run",
        "diamond",
        &[
            ("main.tcrf", main),
            ("middle.tcrf", middle),
            ("mylib.tcrf", MYLIB),
        ],
    );
    assert_eq!(code, 0, "stderr: {}", stderr);
    assert_eq!(stdout, "14\n");
}

#[test]
fn usage_type_arithmetic_needs_usage_result() {
    // 用途型どうしの演算結果を素の Decimal にはできない (E411)
    let src = r#"T Price [Decimal]

R double price
  Price > Decimal

  price + price

F main
  Void
"#;
    let (code, _, stderr) = run_src("check", "usage_mismatch", src);
    assert_eq!(code, 2);
    assert!(stderr.contains("E411"), "stderr: {}", stderr);
}
