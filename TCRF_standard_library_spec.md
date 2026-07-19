# TCRF標準ライブラリ仕様書

- 対象バージョン: TCRF Standard Library 0.2
- 対応言語仕様: TCRF 0.4
- ステータス: 実装用ドラフト

## 1. 概要

本書はTCRF標準ライブラリを定義する。

言語構文、型システム、評価規則は「TCRF言語 実装仕様書」で定義する。

標準ライブラリは次を提供する。

| モジュール | 役割 |
|---|---|
| `std.console` | コンソール入出力 |
| `std.list` | リスト操作 |
| `std.range` | 整数範囲生成 |
| `std.format` | Textへの整形 |
| `std.text` | Text操作 |
| `std.number` | 数値変換 |
| `std.validate` | 条件検証 |

説明中の`A`、`B`は標準ライブラリ内部の型変数であり、ユーザー定義ジェネリクスではない。

失敗可能なRはすべて`! Error`を使う。

## 1.1 宣言ファイル std.tcrf

標準ライブラリの公開Rの型シグネチャは、処理系バイナリのあるディレクトリ直下の
`lib/std.tcrf`(標準ライブラリ宣言ファイル)として提供する。
ファイルが無い場合、処理系は内蔵の既定内容から自動生成する。

`std.xxx`の型検査はこのファイルに書かれたシグネチャに従う
(言語仕様書 §22.2)。

宣言は本体のないR (関数名・引数名・型シグネチャのみ) で書き、
処理本体の状態を関数ごとのコメントで示す。

```text
# ビルトイン実装
R first values
  List<A> > A ! Error

# ビルトイン実装
R prepend value values
  A, List<A> > List<A>

# 未実装
R sort values
  List<A> > List<A>
```

- 「ビルトイン実装」— 宣言名に対応する処理系組み込みの実装で実行される
- 「未実装」— 宣言のみ。呼び出すとコンパイルエラー
- 入力型のカンマ区切りは複数引数の組み込み式 (本書の「概念的な型」) を表す
- `std.empty<T>`と`at`は言語組み込み式のため、このファイルには現れない


## 2. std集約モジュール

教育用途では、標準ライブラリを次のように読み込む。

```text
import std
```

主要機能は`std`直下から参照する。

```text
std.print
std.printLine
std.readText
std.debug
std.empty<Int>
std.isEmpty
std.first
std.rest
std.prepend
std.length
std.reverse
std.append
std.contains
std.sumInt
std.sumDecimal
std.inclusive
std.exclusive
std.int
std.decimal
std.bool
std.intList
std.decimalList
std.textList
std.trim
std.lower
std.upper
std.concat
std.textContains
std.parseInt
std.parseDecimal
std.toDecimal
std.floor
std.round
std.absInt
std.absDecimal
std.require
std.requireNotEmpty
```

`std`は、個別モジュールの主要機能を再公開する教育向け集約モジュールである。

標準ライブラリは小規模であることを前提とし、大規模開発向けの細かな名前空間分割より、初学者が覚える構文の少なさを優先する。

個別モジュール形式も利用できる。

```text
import std
import std
```

入門教材では`import std`を推奨する。


## 2. std.console

import:

```text
import std
```

### print

```text
print
  Text > Void
```

改行せずに出力する。

### printLine

```text
printLine
  Text > Void
```

出力後に改行する。

### readText

```text
readText
  Void > Text ! Error
```

標準入力から1行読む。行末改行は含めない。

### debug

```text
debug
  A > Void
```

任意値を処理系依存形式で表示する。開発用。

## 3. std.list

import:

```text
import std
```

### empty

```text
std.empty<A>
```

空の`List<A>`を返す組み込み式。

```text
std.empty<Int>
std.empty<Product>
```

型引数は必須。

### isEmpty

```text
isEmpty
  List<A> > Bool
```

リスト用途型も受け取れる。

### first

```text
first
  List<A> > A ! Error
```

先頭要素を返す。空リストはError。

### rest

```text
rest
  List<A> > List<A> ! Error
```

先頭を除いた新しいリストを返す。空リストはError。

1要素リストでは空リストを返す。

### prepend

```text
std.prepend value values
```

2引数を取る標準組み込み式。

概念的な型:

```text
A, List<A> > List<A>
```

元リストは変更しない。

### length

```text
length
  List<A> > Int
```

### reverse

```text
reverse
  List<A> > List<A>
```

### append

```text
std.append left right
```

概念的な型:

```text
List<A>, List<A> > List<A>
```

### contains

```text
std.contains value values
```

概念的な型:

```text
A, List<A> > Bool
```

Aに`==`がある場合だけ利用可能。

### sumInt

```text
sumInt
  List<Int> > Int
```

空リストは0。

### sumDecimal

```text
sumDecimal
  List<Decimal> > Decimal
```

空リストは0.0。

### リスト用途型

```text
T Numbers [List<Number>]
```

次を直接適用できる。

```text
std.isEmpty numbers
std.first numbers
std.rest numbers
std.length numbers
```

`rest`は用途型ではなく`List<Number>`を返す。

## 4. at

`at`は標準ライブラリRではなく言語組み込み式。

```text
at values index
```

概念的な型:

```text
List<A>, Int > A ! Error
```

- 0始まり
- 負数はError
- 範囲外はError
- リスト用途型を受け取れる

## 5. std.range

import:

```text
import std
```

入力レコード:

```text
T RangeInput {
  first Int
  last  Int
}
```

推奨実装では`std.range.RangeInput`として公開する。

### inclusive

```text
inclusive
  RangeInput > List<Int> ! Error
```

両端を含む。

```text
first = 2
last  = 5
```

結果:

```text
2, 3, 4, 5
```

`first > last`はError。

### exclusive

```text
exclusive
  RangeInput > List<Int> ! Error
```

firstを含み、lastを含まない。

```text
2, 3, 4
```

`first > last`はError。

## 6. std.format

import:

```text
import std
```

### int

```text
int
  Int > Text
```

### decimal

```text
decimal
  Decimal > Text
```

### bool

```text
bool
  Bool > Text
```

### intList

```text
intList
  List<Int> > Text
```

推奨形式:

```text
[2, 3, 5, 7]
```

### decimalList

```text
decimalList
  List<Decimal> > Text
```

### textList

```text
textList
  List<Text> > Text
```

標準formatは任意用途型を自動展開しない。

用途型ごとに専用Rを定義する。

```text
R totalText total
  TotalAmount > Text

  std.decimal total
```

上記を許すには、処理系側に型付き整形規則が登録されていなければならない。

## 7. std.text

### length

```text
length
  Text > Int
```

Unicodeコードポイント数。

### trim

```text
trim
  Text > Text
```

### lower

```text
lower
  Text > Text
```

### upper

```text
upper
  Text > Text
```

### concat

```text
text.concat left right
```

概念的な型:

```text
Text, Text > Text
```

### contains

```text
text.contains text part
```

概念的な型:

```text
Text, Text > Bool
```

### parseInt

```text
parseInt
  Text > Int ! Error
```

### parseDecimal

```text
parseDecimal
  Text > Decimal ! Error
```

## 8. std.number

### toDecimal

```text
toDecimal
  Int > Decimal
```

### floor

```text
floor
  Decimal > Int
```

### round

```text
round
  Decimal > Int
```

### absInt

```text
absInt
  Int > Int
```

### absDecimal

```text
absDecimal
  Decimal > Decimal
```

## 9. std.validate

TCRFには汎用`throw`構文がない。

条件で失敗させる場合は検証Rを使う。

### require

```text
require
  Bool > Void ! Error
```

trueならVoid、falseならError。

```text
R positive value
  Int > Int ! Error

  checked : Void =
    std.require (value > 0)

  value
```

### requireNotEmpty

```text
requireNotEmpty
  List<A> > List<A> ! Error
```

空ならError、非空なら同じリスト。

非空性は型には保存されない。

## 10. 基本型の算術

### Int

```text
Int + Int > Int
Int - Int > Int
Int * Int > Int
Int / Int > Int ! Error
Int % Int > Int ! Error
```

0除算はError。

### Decimal

```text
Decimal + Decimal > Decimal
Decimal - Decimal > Decimal
Decimal * Decimal > Decimal
Decimal / Decimal > Decimal ! Error
```

0除算はError。

### 比較

Int、Decimal、Text、Char、Boolに意味のある比較を提供する。

### 用途型演算

処理系は登録済みの型付き演算規則を持てる。

```text
Price * TaxRate > TaxAmount
Price + TaxAmount > TotalAmount
```

ユーザー定義演算子はTCRF 0.3では提供しない。

## 11. エラトステネスのふるいで必要な機能

```text
std.empty<Int>
std.isEmpty values
std.first values
std.rest values
std.prepend value values
std.inclusive input
std.intList values
std.printLine text
```

## 12. 実装要件

1. 公開Rの型を静的に提供する
2. 失敗可能Rは`! Error`
3. Error種別を公開しない
4. リスト操作は不変
5. Text操作は不変
6. `std.empty<A>`は型引数必須
7. `first`と`rest`は空でError
8. `std.inclusive`は両端を含む
9. `first > last`はError
10. `printLine`はVoid
11. 0除算はError
12. import規則に従う

## 13. 将来候補

- map
- filter
- fold
- sort
- find
- take
- drop
- zip
- ファイル入出力
- JSON
- 日時
- テスト支援
- 非空リスト型

## Hello World

```text
import std

F main
  Text "Hello, World!"
  std.printLine
```
