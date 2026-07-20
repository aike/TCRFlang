# 型駆動開発言語TCRF

TCRFは、**型を中心にプログラムを設計する手法を修得するためのプログラミング言語**です。

静的型付け言語を生かすには、型を中心に設計すること（以下、型駆動開発）が重要です。しかしながら、たとえばJavaScriptに慣れたプログラマーは、TypeScriptでも処理手順を中心に設計し、型を付けるにしても汎用的な型を付けてしまうことがあります。  
現代的な静的型付け言語は目的ごとに細かく型を定義して型駆動の開発をすることで、プログラムの安全性と可読性を高めることができます。すなわちプログラマーは言語仕様ではなく、設計手法を学ぶ必要があります。  
そこで、型駆動開発の手法を学ぶための教育用言語としてTCRFを作りました。
TCRFでは型を中心とした設計が強制されるため、自然に型駆動開発を学ぶことができます。


TCRF は T (型) / C (定数) / R (変換) / F (接続) の 4 種類の宣言だけでプログラムを組み立てる、
型中心設計の言語です。言語仕様は同梱のドキュメントを参照してください。

- [TCRF言語入門](TCRF_language_introduction.md)
- [よくある質問](TCRF_FAQ.md)
- [標準ライブラリ仕様](TCRF_standard_library_spec.md) 
- [言語仕様](TCRF_language_implementation_spec.md)

## 実行環境

- **Rust 1.85 以降** (edition 2024)。開発・動作確認は Rust 1.97.1 で行っています。
- Windows / macOS / Linux で動作します (開発環境は Windows 11)。
- 外部依存は [`rust_decimal`](https://crates.io/crates/rust_decimal) のみです
  (`Decimal` 型を 10 進固定小数点で正確に扱うため)。

Rust が未導入の場合は [rustup](https://rustup.rs/) でインストールしてください。

## ビルド

```console
cargo build --release
```

バイナリは `target/release/tcrf` (Windows では `tcrf.exe`) に生成されます。

## テスト

```console
cargo test
```

ユニットテスト (字句・構文・型検査・評価器) と、`examples/` を実際に実行して
標準出力・終了コードを検証する統合テストが走ります。

## 実行

```console
# 検査して実行
tcrf run <file.tcrf>

# 型検査のみ (実行しない)
tcrf check <file.tcrf>
```

ビルドせずに cargo 経由で実行することもできます。

```console
cargo run --release -- run examples/hello.tcrf
```

### 実行前のプログラム全体検査

本処理系はインタープリタですが、`run` は実行の前処理として
プログラム全体を検査します。検査対象は読み込んだすべてのソースファイル
(実行対象・import したモジュール・`std.tcrf`) の全宣言で、
`main` から呼ばれない R や、どこからも参照されていない T もすべて
型検査されます。エラーが 1 件でもあれば何も実行されません
(実行された経路だけが検査されるスクリプト言語的な動作ではありません)。
`check` はこの検査だけを行うモードです。

### 終了コード

| コード | 意味 |
|---|---|
| 0 | 正常終了 |
| 1 | 実行時 Error が `main` まで伝播した |
| 2 | コンパイルエラー (字句・構文・名前解決・型検査) または使い方の誤り |

コンパイルエラーは `ファイル:行:桁: error[Exxx]: メッセージ` の形式で、
該当行の抜粋と修正のヒント付きで標準エラー出力に表示されます。

## サンプル

`examples/` に仕様書・入門書の代表例を収録しています。

| ファイル | 内容 | 出力 |
|---|---|---|
| `hello.tcrf` | Hello World | `Hello, World!` |
| `tax.tcrf` | 用途型を使った税込み計算 | `1100` |
| `grade.tcrf` | when / match / 代数データ型による成績判定 | `Passed` |
| `order.tcrf` | 表現保持型遷移 (`=>`) による注文状態の遷移 | `Order shipped` |
| `sieve.tcrf` | エラトステネスのふるい (再帰・レコード・リスト) | `[2, 3, 5, ..., 97]` |
| `discount.tcrf` | ユーザー定義モジュール (`pricing.tcrf`) の import | `800` |

```console
cargo run --release -- run examples/sieve.tcrf
```

## モジュールの探索

`import` したユーザー定義モジュールは `.tcrf` ファイルから読み込みます
(仕様書 §22.1 参照)。

- `import mylib` → `mylib.tcrf`、`import utils.math` → `utils/math.tcrf`
- 探索順: (1) 実行した `tcrf.exe` のあるディレクトリ直下の `lib/` (最優先) →
  (2) import を書いたファイルのあるディレクトリ →
  (3) 環境変数 `TCRF_PATH` のディレクトリ (Windows は `;`、Unix 系は `:` 区切り)
- `std` とその配下は予約名で、一般のファイル探索はされません。代わりに
  `tcrf.exe` 直下の `lib/std.tcrf` (標準ライブラリ宣言ファイル) が読み込まれ、
  `std.xxx` の型検査はそこに書かれた型シグネチャに従います。
  ファイルが無ければ初回実行時に自動生成されます (内容は本リポジトリの
  `lib/std.tcrf` と同じ)

```console
# 例: ライブラリ置き場を TCRF_PATH で追加する
$env:TCRF_PATH = "D:\tcrf\libs"      # PowerShell
export TCRF_PATH=~/tcrf/libs         # bash
```

## 実装の範囲

- 実行モデルは木構造 (AST) インタープリタです。
- 型検査はプログラムの実行に先立ちプログラム全体に対して行い、型検査に失敗した場合は実行せずに終了します。
- 標準ライブラリは仕様書の例を動かすためのサブセットを組み込みで提供します
  (console / list / range / format / text / number / validate の主要関数)。
- 深い再帰への対策として、大きめのスタック (256MB) の専用スレッドで実行し、
  呼び出し深度が上限 (10,000) を超えた場合は実行時 Error として穏当に停止します。

## ライセンス

MIT License — 詳細は [LICENSE](LICENSE) を参照してください。
