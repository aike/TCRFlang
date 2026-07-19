# TCRF言語 実装仕様書

- 対象バージョン: TCRF 0.4
- ステータス: 実装用ドラフト
- 文字コード: UTF-8
- 推奨拡張子: `.tcrf`

## 1. 言語の目的

TCRFは、静的型を中心にプログラムを設計する練習を目的とした言語である。

| 記号 | 名称 | 役割 |
|---|---|---|
| `T` | Type | 型を定義する |
| `C` | Constant | 定数を定義する |
| `R` | Rule | 一つの変換の内部を記述する |
| `F` | Flow | 複数の変換を接続する |

`import`はT/C/R/Fとは異なり、モジュール依存関係を指定する。

基本原則:

- 値の意味や状態を型で表す
- 値は不変
- 暗黙型変換なし
- `null`なし
- クラス、メソッド、継承なし
- Rは変換の実装
- Fは変換の接続
- エラーは組み込み`Error`型だけを使う

## 2. ソースファイル

- UTF-8
- LFまたはCRLF
- 拡張子`.tcrf`
- 大文字小文字を区別
- インデントは半角スペース
- 標準インデント幅は2
- タブによるインデントは禁止
- コメントは`#`から行末まで

推奨順序:

```text
import
T
C
R
F
```

### 2.1 コメント

コメントの仕様はPythonと同一の行コメントのみとする。

- 行コメントのみ。`#`からその行の末尾 (改行の直前) までがコメント。
  ブロックコメントは提供しない
- 文字列リテラル`"..."`・文字リテラル`'.'`の内側の`#`はコメントにならない
- コメントはトークンを生成せず、空白と同様に無視される
- 空白とコメントだけの行 (コメント行) は空行と同様に扱い、
  インデント構造 (INDENT/DEDENT) に関与しない。
  ブロックの途中に任意のインデント量で書ける
- 括弧`( ) { } [ ]`内の複数行構造の行末にも書ける
- タブ禁止はコメント行にも適用する
  (行頭インデントにタブがあればコメント行でも字句エラー)
- ドキュメンテーションコメント (`##`など) の特別扱いはしない

```text
# 設定値 (行全体のコメント)
C standardTaxRate = TaxRate 0.10  # 行末コメント

T RangeInput {
  first Int   # 開始値
  last  Int   # 終了値 (括弧内でも可)
}
```

## 3. 識別子

型名と代数データ型コンストラクタは大文字で始める。

```text
Price
PaymentRejected
```

値名、定数名、R名、F名、フィールド名、import別名は小文字で始める。

```text
price
standardTaxRate
calculateTotal
main
```

## 4. 組み込み型

```text
Int
Decimal
Text
Char
Bool
Void
List<Value>
Error
```

`Value`は説明上の型変数であり、ユーザーコードで直接使用できない。

`Void`は、C言語の`int main(void)`に近い感覚で、実質的な入力なし、または意味のある戻り値なしを表す。

`Error`は失敗経路専用であり、通常値として扱えない。

## 5. import

### 5.1 基本構文

```text
import module.name
import module.name as alias
```

`import`はすべてのT/C/R/F宣言より前に置く。

### 5.2 標準ライブラリ集約モジュール

教育用途では、標準ライブラリ全体を次のように読み込む。

```text
import std
```

主要機能は`std`で修飾して参照する。

```text
std.printLine
std.first
std.inclusive
```

Hello Worldは次のようになる。

```text
import std

F main
  Text "Hello, World!"
  std.printLine
```

### 5.3 個別モジュール

標準ライブラリの個別モジュールも利用できる。

```text
import std.console as console
import std.list as list
```

```text
console.printLine
list.first
```

入門教材では`import std`を推奨する。

### 5.4 一般モジュール

別名付きimportでは別名による修飾を必須とする。

```text
import order.pricing as pricing

pricing.calculate
```

別名なしの一般モジュールは、モジュール名の最終要素を修飾名として利用する。

```text
import order.pricing

pricing.calculate
```

公開名を非修飾名として一括展開しない。

### 5.5 衝突と循環

同じ修飾名が導入された場合、および循環importはコンパイルエラーとする。

## 6. 型定義

### 6.1 用途型

```text
T UserId [Text]
T Price [Decimal]
T Products [List<Product>]
```

用途型は内部型と別型である。

```text
UserId != Text
Products != List<Product>
```

暗黙変換は行わない。

### 6.2 レコード型

```text
T Product {
  id    ProductId
  name  ProductName
  price UnitPrice
}
```

フィールド名は型内で一意でなければならない。

### 6.3 代数データ型

```text
T PaymentMethod
  | CreditCard CardInformation
  | BankTransfer BankAccount
  | CashOnDelivery
```

規則:

- 定義に`=`を書かない
- 全コンストラクタ行を`|`で始める
- コンストラクタは0個または1個のペイロードを持つ
- 複数値が必要ならレコード型にまとめる
- コンストラクタを最低1個持つ

禁止:

```text
T Point
  | Point Decimal Decimal
```

許可:

```text
T PointData {
  x Decimal
  y Decimal
}

T Point
  | Point PointData
```

状態型も同じ構文を使う。

```text
T AccountStatus
  | Active
  | Locked
  | Deleted
```

業務上の結果分類も同じ構文を使える。

```text
T ValidationResult
  | Valid ValidData
  | Invalid ValidationMessage
```

ただし、R/Fの失敗経路は組み込み`Error`を使う。

## 7. リスト型

型表記:

```text
List<Int>
List<Product>
List<List<Int>>
```

用途型:

```text
T Numbers [List<Number>]
```

用途型リストの構築:

```text
Numbers(
  Number 10.0
  Number 20.0
  Number 30.0
)
```

空リスト用途型:

```text
Numbers()
```

カンマは使用しない。

生の空リストは標準ライブラリの次の式で生成する。

```text
std.empty<Int>
```

要素参照:

```text
at products index
```

`at`は言語組み込み式である。

- 添字は0始まり
- 添字型は`Int`
- 負の添字は`Error`
- 範囲外は`Error`
- 結果型はリスト要素型
- リスト用途型も受け取れる
- エラーは自動伝播

概念的な型:

```text
List<Value>, Int > Value ! Error
```

`at`は一般Rの複数引数化を意味しない。

## 8. 定数

```text
C standardTaxRate = TaxRate 0.10
```

Cは式の評価結果に名前を付ける。

```text
T UserId [Text]
C administratorId = UserId "U001"
```

右辺はコンパイル時評価可能でなければならない。

許可:

- リテラル
- 用途型構築
- レコード構築
- 代数データ型コンストラクタ
- リスト用途型構築
- 既存Cの参照
- 純粋な組み込み演算

R/F呼び出しと循環参照は禁止。

## 9. Rule

Rは一つの意味ある型変換の内部を記述する。

```text
R calculateTotal price
  Price > TotalAmount

  tax : TaxAmount =
    price * standardTaxRate

  total : TotalAmount =
    price + tax

  total
```

シグネチャ:

```text
Input > Output
Input > Output ! Error
```

入力なし:

```text
R hello
  Void > Message
  Message "Hello"
```

正常戻り値は本体最後の式。

Rで許可:

- 算術
- 比較
- 論理演算
- ローカル束縛
- 型構築
- レコード構築
- 代数データ型構築
- リスト操作
- `when`
- `match`
- 他Rの呼び出し
- 直接再帰
- 相互再帰

型注釈付き束縛:

```text
tax : TaxAmount =
  price * standardTaxRate
```

型推論付き束縛:

```text
count =
  std.length values
```

値は不変であり、再代入とシャドーイングは禁止。

### 9.1 from式と表現保持型遷移

`from`は用途型の値を、内部表現を変えずに別の用途型へ包み直す式である
(unwrapして包み直す操作に相当する)。

```text
A from x
```

- `A`は用途型でなければならない
- `x`の型は用途型でなければならない (これを`B`とする)
- `A`と`B`の内部型が同型のときだけ許され、結果は`x`の値をそのまま持つ`A`型の値
- 内部型が異なる場合、どちらかが用途型でない場合はコンパイルエラー
- 実行時には何も行わない (値は表現を保ったまま流れる)

ローカル束縛と組み合わせる例:

```text
y : A = A from x
```

表現保持型遷移R (`=>`) はfrom式のシンタックスシュガーである。

```text
R pay order
  UnpaidOrder => PaidOrder
```

は次と等価に振る舞う。

```text
R pay order
  UnpaidOrder > PaidOrder

  PaidOrder from order
```

これにより、Rのシグネチャはすべて`Input > Output [! Error]`の形として
一貫して読むことができ、単純な状態遷移は`=>`で簡潔に書ける。

## 10. 再帰

直接再帰と相互再帰を許可する。

```text
R countdown value
  Int > Void

  when value <= 0
    true
      Void

    false
      countdown (value - 1)
```

処理系は停止性を検証しない。

再帰深度制限は処理系依存。

## 11. Flow

FはR/Fを接続し、型の流れを表す。

```text
F main
  Price 1000.0
  calculateTotal
  totalText
  std.printLine
```

Fで許可:

- 最初の値の生成
- Rの接続
- Fの接続
- フロー単位の`match`
- Errorの自動伝播
- 定数参照

Fで禁止:

- 算術式
- ローカル計算
- 変数定義
- フィールドを直接操作する細かな処理
- 途中でのレコード構築
- 任意の式文

Fの最初の値として許可:

- リテラル
- 用途型構築
- レコード型構築
- 代数データ型コンストラクタ
- リスト用途型構築
- 定数
- `Void`

例:

```text
F main
  Limit {
    value = 100
  }
  findPrimes
  primesText
  std.printLine
```

各段階の出力型は次段階の入力型と完全一致しなければならない。

再利用可能Fはシグネチャを持てる。

```text
F processOrder
  RawOrder > ShippedOrder ! Error

  validateOrder
  calculatePrice
  payOrder
  shipOrder
```

## 12. Fのmatch

Fでは、代数データ型によるフロー単位の分岐を許可する。

```text
F handlePayment
  PaymentResult > Void

  match
    Paid
      createReceipt
      printReceipt

    Rejected
      rejectionText
      std.printLine
```

- 全コンストラクタを網羅
- 各分岐はR/Fの接続列
- 各分岐の最終出力型は一致
- ペイロードを持つコンストラクタは分岐入力として利用可能

## 13. when

```text
when condition
  true
    expression1

  false
    expression2
```

短縮形:

```text
when score >= 80
  true  Excellent
  false Passed
```

条件は`Bool`。

両分岐の正常結果型は一致しなければならない。

## 14. match

R内:

```text
match grade

  Excellent
    Text "Excellent"

  Passed
    Text "Passed"

  Failed
    Text "Failed"
```

ペイロードあり:

```text
match result

  Paid record
    formatPayment record

  Rejected reason
    formatReason reason
```

規則:

- 全コンストラクタを網羅
- ワイルドカードなし
- 重複分岐禁止
- ペイロード束縛は最大1個

## 15. Error

TCRFは組み込み`Error`型を1種類だけ持つ。

```text
R parseQuantity raw
  RawQuantity > Quantity ! Error
```

規則:

- ユーザー定義エラー型を失敗経路に使えない
- Errorは種別を持たない
- Errorはペイロードを持たない
- Errorを保存できない
- Errorを比較できない
- Errorをmatchできない
- ErrorをCにできない
- Errorをレコードフィールドにできない
- Errorは自動伝播
- 汎用`throw`構文なし

失敗可能処理を呼ぶR/Fも`! Error`を持たなければならない。

```text
R firstProduct products
  Products > Product ! Error

  at products 0
```

`main`まで伝播すると非0終了コードで終了する。

処理系内部では診断メッセージ、発生位置、呼び出し履歴を保持してよいが、ユーザーコードから参照できない。

## 16. Void

`Void`は入力なし、または意味のある戻り値なしを表す。

```text
R hello
  Void > Message
```

```text
R printMessage message
  Message > Void
```

処理系内部では単一値型として実装してよい。

値式としても使える。

```text
F main
  Void
  startApplication
```

## 17. レコード構築

```text
Product {
  id    = ProductId "P001"
  name  = ProductName "Keyboard"
  price = UnitPrice 3000.0
}
```

- 全フィールドを1回ずつ指定
- 欠落、重複、余分はコンパイルエラー

## 18. 代数データ型構築

ペイロードなし:

```text
Cancelled
```

ペイロードあり:

```text
Paid paymentRecord
```

複数ペイロードは禁止。

## 19. 演算子

算術:

```text
+
-
*
/
%
```

比較:

```text
==
!=
<
<=
>
>=
```

論理:

```text
and
or
not
```

優先順位:

1. フィールド参照`.`
2. `at`とR呼び出し
3. 単項`-`、`not`
4. `*`、`/`、`%`
5. `+`、`-`
6. `<`、`<=`、`>`、`>=`
7. `==`、`!=`
8. `and`
9. `or`

用途型同士の演算は、処理系または標準ライブラリに登録された型付き演算規則がある場合だけ許可する。

```text
Price * TaxRate > TaxAmount
Price + TaxAmount > TotalAmount
```

ユーザー定義演算子は初期仕様では提供しない。

## 20. 型システム

- 名前付き型は名前で同一性を持つ
- 暗黙変換なし
- 一般的な`unwrap`なし
- 用途型内部値は型付き演算または専用Rで利用
- ユーザー定義ジェネリクスなし
- 再帰型なし
- `List<Value>`だけ処理系組み込みの型パラメータを持つ

### 20.1 検査の範囲と時期

処理系の実行方式 (インタープリタかコンパイラか) にかかわらず、
型検査はプログラムの実行に先立ち、プログラム全体に対して行う。

- 検査対象は、読み込まれたすべてのソースファイル
  (実行対象ファイル・importされたモジュール・宣言ファイル`std.tcrf`) の全宣言である
- `main`から到達しない (一度も呼ばれない) RやFも、
  どこからも参照されていないTやCも検査から除外しない
- 検査に1件でも失敗した場合、プログラムのいかなる部分も実行しない

すなわち、実際に実行された経路だけを検査する動的な方式は認めない。
未使用の宣言に含まれる型エラーも、実行前に必ず報告される。

型推論を許可:

- R内部ローカル束縛
- 式の中間結果
- F内の現在値

型推論を許可しない:

- T内部型
- R入出力型
- Error有無
- 公開境界

## 21. main

実行可能プログラムには`F main`がちょうど1個必要。

シグネチャ省略時:

```text
Void > Void
```

最終正常出力は`Void`。

`main`は`! Error`を持てる。

## 22. モジュール

- 1ファイル1モジュール
- モジュール名はドット区切り
- アンダースコア開始のトップレベル名は非公開
- 別名importはドット修飾

```text
std.printLine
std.first
std.inclusive
```

### 22.1 モジュールファイルの探索

ユーザー定義モジュールは`.tcrf`ファイルとして提供する。
モジュール名はファイルパスに次の規則で対応する。

- `import name` → ファイル`name.tcrf`
- `import a.b.c` → ファイル`a/b/c.tcrf` (ドットはディレクトリ区切りに対応)

探索は次の順で行い、最初に見つかったファイルを採用する。

1. 実行した処理系バイナリ (`tcrf.exe`など) があるディレクトリ直下の`lib/`ディレクトリ
   (処理系に同梱する共通ライブラリの置き場。最優先)
2. importを書いたファイルがあるディレクトリ
3. 環境変数`TCRF_PATH`に列挙されたディレクトリ (OSのパス区切り文字
   — Windowsは`;`、Unix系は`:` — で区切り、先頭から順に)

どこにも見つからない場合はコンパイルエラー
(モジュールが見つからない。診断には探索したパスを含める)。

### 22.2 予約モジュール名 std と宣言ファイル std.tcrf

`std`およびその配下 (`std.console`など) は標準ライブラリとして予約されており、
§22.1の一般モジュール探索の対象にしない。

代わりに`import std`(系)は、実行した処理系バイナリのあるディレクトリ直下の
`lib/std.tcrf`(標準ライブラリ宣言ファイル)を読み込む。

- `lib/std.tcrf`が存在しない場合、処理系は内蔵の既定内容から自動生成する
  (生成できない環境では内蔵内容をそのまま使う)
- `std.xxx`の呼び出し・接続の型検査は、`std.tcrf`に書かれた型シグネチャに従う
- 処理本体は宣言名に対応する処理系組み込み実装で実行される。
  組み込み実装のない宣言 (コメントで「未実装」と記された関数) を呼び出すと
  コンパイルエラーとする
- `std.tcrf`の宣言は本体のないR (シグネチャのみ) で書く。
  この形式のRを`std.tcrf`以外のファイルに書くとコンパイルエラー
- 宣言の型シグネチャには、組み込み型・`List`・型変数 (大文字1文字。`A`など)・
  `RangeInput`だけを書ける。入力型をカンマで並べると複数引数の組み込み式を表す
  (例: `A, List<A> > List<A>`)

書式の詳細と全宣言は標準ライブラリ仕様書および付属の`lib/std.tcrf`を参照。

### 22.3 モジュールの同一性と読み込み

- モジュールの同一性は正規化した絶対ファイルパスで判定する。
  同じファイルは経路が複数あっても1回だけ読み込まれ、
  そこで定義された型はすべてのimport元で同一の型として扱われる
  (いわゆるダイヤモンドimportで型が分裂しない)
- モジュールファイル内のimportも同じ規則で解決する。
  その際の探索基点 (上記1) はそのモジュールファイルがあるディレクトリ
- 読み込み中のモジュールへ到達するimportは循環importとして
  コンパイルエラー (§5.5)

### 22.4 モジュールと main

`F main`は実行対象として指定したファイルにのみ定義できる。
モジュールとして読み込まれたファイルに`F main`があればコンパイルエラーとする。

### 22.5 公開範囲

トップレベルのT/C/R/Fはアンダースコア開始のものを除きすべて公開される。
アンダースコア開始の名前をモジュール外から参照した場合はコンパイルエラーとする。
公開名の参照は常に修飾名 (`修飾名.名前`) で行い、非修飾名への一括展開はしない (§5.4)。

## 23. コンパイラ診断

最低限含める情報:

- エラーコード
- ファイル名
- 行
- 桁
- メッセージ
- 可能なら修正候補

必須検出:

- 字句エラー
- インデントエラー
- 構文エラー
- 未定義名
- 重複定義
- import衝突
- import循環
- 型不一致
- F接続不一致
- レコード不備
- 非網羅match
- 複数ペイロード
- Error不正利用
- 再代入
- シャドーイング
- main欠落、重複、戻り型不一致
- 失敗可能処理を非失敗R/Fから呼ぶこと

## 24. エラトステネスのふるい

```text
import std
import std
import std
import std

T Limit {
  value Int
}

T Candidates {
  values List<Int>
}

T Primes {
  values List<Int>
}

T FilterInput {
  divisor Int
  values  List<Int>
}

T RangeInput {
  first Int
  last  Int
}

R removeMultiples input
  FilterInput > List<Int> ! Error

  when std.isEmpty input.values
    true
      std.empty<Int>

    false
      value : Int =
        std.first input.values

      remaining : List<Int> =
        std.rest input.values

      filtered : List<Int> =
        removeMultiples FilterInput {
          divisor = input.divisor
          values  = remaining
        }

      when value % input.divisor == 0
        true
          filtered

        false
          std.prepend value filtered

R sieve candidates
  Candidates > Primes ! Error

  when std.isEmpty candidates.values
    true
      Primes {
        values = std.empty<Int>
      }

    false
      prime : Int =
        std.first candidates.values

      remaining : List<Int> =
        std.rest candidates.values

      filtered : List<Int> =
        removeMultiples FilterInput {
          divisor = prime
          values  = remaining
        }

      restPrimes : Primes =
        sieve Candidates {
          values = filtered
        }

      Primes {
        values =
          std.prepend prime restPrimes.values
      }

R findPrimes limit
  Limit > Primes ! Error

  values : List<Int> =
    std.inclusive RangeInput {
      first = 2
      last  = limit.value
    }

  sieve Candidates {
    values = values
  }

R primesText primes
  Primes > Text

  std.intList primes.values

F main
  Limit {
    value = 100
  }
  findPrimes
  primesText
  std.printLine
```

## 25. 簡易EBNF

```ebnf
program =
  { import-declaration },
  { top-level-declaration } ;

top-level-declaration =
    type-declaration
  | constant-declaration
  | rule-declaration
  | flow-declaration ;

import-declaration =
  "import", module-name, [ "as", alias-name ], NEWLINE ;

type-declaration =
    distinct-type-declaration
  | record-type-declaration
  | algebraic-data-type-declaration ;

distinct-type-declaration =
  "T", type-name, "[", type-expression, "]", NEWLINE ;

record-type-declaration =
  "T", type-name, "{", NEWLINE,
  INDENT, field-declaration, { field-declaration }, DEDENT,
  "}", NEWLINE ;

algebraic-data-type-declaration =
  "T", type-name, NEWLINE,
  INDENT,
    constructor-declaration,
    { constructor-declaration },
  DEDENT ;

constructor-declaration =
  "|", constructor-name, [ type-expression ], NEWLINE ;

constant-declaration =
  "C", value-name, "=", expression, NEWLINE ;

rule-declaration =
    normal-rule-declaration
  | representation-preserving-rule-declaration ;

normal-rule-declaration =
  "R", rule-name, [ parameter-name ], NEWLINE,
  INDENT,
    rule-signature,
    { rule-statement },
    expression,
  DEDENT ;

(* from式を使ったnormal-rule-declarationへの糖衣 (§9.1) *)
representation-preserving-rule-declaration =
  "R", rule-name, parameter-name, NEWLINE,
  INDENT,
    type-expression, "=>", type-expression, NEWLINE,
  DEDENT ;

rule-signature =
  type-expression, ">", type-expression,
  [ "!", "Error" ],
  NEWLINE ;

flow-declaration =
  "F", flow-name, NEWLINE,
  INDENT,
    [ flow-signature ],
    flow-step,
    { flow-step },
  DEDENT ;

flow-signature =
  type-expression, ">", type-expression,
  [ "!", "Error" ],
  NEWLINE ;

type-expression =
    builtin-type
  | type-name
  | "List", "<", type-expression, ">" ;

at-expression =
  "at", expression, expression ;

from-expression =
  type-name, "from", primary-expression ;
```

注: コメント `comment = "#", { 改行以外の任意文字 } ;` は
字句解析の段階で空白と同様に除去されるため、上記の文法規則には現れない (§2.1)。

## 26. 実装適合条件

準拠処理系は最低限次を実装する。

1. T/C/R/Fの構文上の区別
2. `import module [as alias]`
3. 用途型、レコード型、代数データ型
4. ADT全行の`|`
5. ADTペイロード最大1個
6. `List<Value>`
7. `at list index`
8. Rの直接・相互再帰
9. Fの制限
10. F初期値としてのレコード構築
11. 単一Errorと自動伝播
12. Errorの通常値利用禁止
13. Void
14. 値の不変性
15. 暗黙型変換なし
16. `F main`
17. 実行前型検査
18. ソース位置付き診断
19. `from`式と、`=>`のfrom式への糖衣展開と等価な振る舞い

## 27. 今後の検討事項

- 型付き算術規則のユーザー定義
- 非空リスト型
- F専用の条件分岐
- リストパターン
- 副作用型
- パッケージ管理
- テスト構文
