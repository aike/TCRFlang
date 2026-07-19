# TCRF言語入門

- 対象バージョン: TCRF 0.4
- 文書種別: 入門教材
- 対象読者: 一般的なプログラミング言語を一つ以上使ったことがある人

---

## 1. TCRFとは

TCRFは、**型を中心にプログラムを設計する手法をマスターするための教育用プログラミング言語**です。

TCRFという名前は、プログラムを構成する四つの宣言から付けられています。

| 記号 | 名前 | 役割 |
|---|---|---|
| `T` | Type | 型を定義する |
| `C` | Constant | 定数を定義する |
| `R` | Rule | 一つの変換を記述する |
| `F` | Flow | 複数の変換を接続する |

TCRFでは、最初に処理手順を考えるのではなく、次の順で設計します。

1. どのような意味の値が存在するかを考える
2. その値を型として定義する
3. ある型から別の型への変換を定義する
4. 変換を接続してプログラム全体を作る

---

## 2. Hello World

TCRF言語のHello Worldプログラムは次のようになります。

```text
import std

F main
  Text "Hello, World!"
  std.printLine
```

実行結果:

```text
Hello, World!
```

---

### import std

```text
import std
```

標準ライブラリの集約モジュールを読み込みます。

TCRFは教育目的の言語なので、基本的な標準機能を`std`直下から利用できます。

```text
std.printLine
std.first
std.inclusive
```

### F main

```text
F main
```

`main`はプログラムの実行開始点です。

TCRFでは、実行開始点をFlowとして記述します。

### Text値

```text
Text "Hello, World!"
```

`Text`型の値を作ります。

### std.printLine

```text
std.printLine
```

直前の`Text`値を受け取って画面へ表示します。

std.printLineの型の流れは次のとおりです。

```text
Text
  > Void
```

---

## 3. 型中心設計

Hello Worldだけでは、TCRFの特徴である型中心設計のメリットは分かりません。

そこで、次の例を見ます。

> 支払い済みの注文だけ発送できる

```text
T OrderId [Text]

T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]

R pay order
  UnpaidOrder => PaidOrder

R ship order
  PaidOrder => ShippedOrder

F main
  UnpaidOrder (OrderId "O001")
  pay
  ship
```

この例では、注文の状態をBoolや文字列ではなく、型で表しています。

```text
UnpaidOrder
PaidOrder
ShippedOrder
```

---

### 注文例の型の流れ

Fの部分を見ます。

```text
F main
  UnpaidOrder (OrderId "O001")
  pay
  ship
```

値の型は次のように変化します。

```text
UnpaidOrder
  > PaidOrder
  > ShippedOrder
```

`pay`は未払い注文を支払い済み注文へ進めます。
`=>`は内部表現を変えずに型が遷移することを表す型シグネチャです。型のキャストに近い意味ですが、内部表現が同じ型同士でないと使えません。
実際の注文システムではここにビジネスロジックが書かれますが、この例では省略しています。

```text
R pay order
  UnpaidOrder => PaidOrder
```

`ship`は支払い済み注文を発送済み注文へ進めます。

```text
R ship order
  PaidOrder => ShippedOrder
```

---

### 未払い注文の発送を事前に検知

次のコードはコンパイルできません。

```text
F main
  UnpaidOrder (OrderId "O001")
  ship
```

`ship`の入力型は`PaidOrder`です。

```text
R ship order
  PaidOrder => ShippedOrder
```

しかし、実際に渡される値は`UnpaidOrder`です。

```text
UnpaidOrder != PaidOrder
```

この不一致は、プログラムを実行する前に検出されます。

---

### 型中心設計による保証

一般的な設計では、注文を次のように表すことがあります。

```text
T Order {
  id      OrderId
  paid    Bool
  shipped Bool
}
```

この設計では、次のような状態も作れてしまいます。

```text
paid    = false
shipped = true
```

未払いなのに発送済みです。

このような状態を防ぐためには、発送処理の中で毎回確認する必要があります。

```text
when order.paid
  true
    ship order

  false
    ...
```

TCRFでは、そもそも状態ごとに型を分けます。

```text
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]
```

さらに、`ship`の入力型を`PaidOrder`だけに限定します。

```text
R ship order
  PaidOrder => ShippedOrder
```

その結果、未払い注文を発送するコードそのものが型検査を通らなくなります。

---

## 4. TCRFの設計手順

TCRFでは、次の順序で設計すると言語の特徴を活かせます。

### 1. 最初の入力を決める

注文処理なら、まだ検証されていない入力から始まるかもしれません。

```text
RawOrder
```

### 2. 最終的な出力を決める

最終的に発送済み注文を作りたいとします。

```text
ShippedOrder
```

### 3. 中間状態を列挙する

```text
RawOrder
ValidOrder
UnpaidOrder
PaidOrder
ShippedOrder
```

### 4. 状態をTとして定義する

```text
T RawOrder { ... }
T ValidOrder { ... }
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]
```

### 5. 必要なRをシグネチャだけで書く

```text
R validate order
  RawOrder > ValidOrder ! Error

R create order
  ValidOrder > UnpaidOrder

R pay order
  UnpaidOrder => PaidOrder

R ship order
  PaidOrder => ShippedOrder
```

### 6. Fで接続する

```text
F processOrder
  RawOrder > ShippedOrder ! Error

  validate
  create
  pay
  ship
```

### 7. 最後にRの中身を書く

この順序にすると、細かな実装より先に、プログラム全体の型の流れが決まります。

---

## 5. 型の流れを先に設計する

TCRFでは、次のような型の流れを先に書くことが重要です。

```text
RawOrder
  > ValidOrder
  > UnpaidOrder
  > PaidOrder
  > ShippedOrder
```

この段階では、各Rの中身が完成していなくても構いません。

必要な変換の種類が先に見えるため、次のことを検討しやすくなります。

- どの状態が必要か
- どの状態間の遷移が許可されるか
- どの変換が失敗するか
- どの処理をRとして分離するか
- Fをどの単位で作るか

---

## 10. 型は保証を表す

TCRFの型は、データ形式だけでなく保証を表します。

| 型 | 保証 |
|---|---|
| `RawOrder` | まだ検証されていない |
| `ValidOrder` | 入力内容が妥当 |
| `UnpaidOrder` | 有効な注文だが未払い |
| `PaidOrder` | 支払い済み |
| `ShippedOrder` | 発送済み |

例えば、次のRを見ます。

```text
R ship order
  PaidOrder => ShippedOrder
```

この一行から、次のことが分かります。

- 発送できるのは`PaidOrder`だけ
- 結果は`ShippedOrder`
- 内部表現は変わらない
- この遷移自体は失敗しない

---

## 11. Rのシグネチャを先に書く

TCRFでは、Rの実装より先にシグネチャを書く設計が有効です。

```text
R validate input
  RawInput > ValidInput ! Error
```

この時点で、次のことが決まります。

- 入力は未検証
- 成功すると検証済み
- 失敗する可能性がある

Rの中身がまだなくても、プログラム設計上の役割は明確です。

---

## 12. Fを先に書く

主要なRのシグネチャが決まったら、Fを書きます。

```text
F processOrder
  RawOrder > ShippedOrder ! Error

  validate
  create
  pay
  ship
```

Fを見るだけで、処理の大きな順序が分かります。

詳細を読まなくても、プログラムの構造を把握できます。

---

## 13. TCRFコードを読む順序

TCRFのプログラムは、上から順番にすべて読む必要はありません。

次の順序で読むと理解しやすくなります。

### 1. F mainを読む

```text
F main
  ...
```

プログラム全体が何をするかを確認します。

### 2. mainから呼ばれるFを読む

```text
F processOrder
  ...
```

大きな処理単位の流れを確認します。

### 3. Rのシグネチャを読む

```text
RawOrder > ValidOrder ! Error
```

どの型がどの型へ変換されるかを確認します。

### 4. Tの定義を読む

```text
T ValidOrder { ... }
```

各型が何を保持し、何を保証するかを確認します。

### 5. 必要なRの本体だけ読む

詳細な計算や分岐を確認します。

---

## 14. 読む順序の例

次のプログラムがあるとします。

```text
T RawOrder { ... }
T ValidOrder { ... }
T PaidOrder { ... }
T ShippedOrder { ... }

R validate order
  RawOrder > ValidOrder ! Error
  ...

R pay order
  ValidOrder > PaidOrder ! Error
  ...

R ship order
  PaidOrder > ShippedOrder ! Error
  ...

F processOrder
  RawOrder > ShippedOrder ! Error

  validate
  pay
  ship

F main
  ...
  processOrder
  ...
```

最初に`main`を見ます。

次に`processOrder`を見ます。

その後で、`validate`、`pay`、`ship`のシグネチャを読みます。

最後に、必要に応じて各Rの本体を読みます。

この読み方により、詳細へ入る前に全体像をつかめます。

---

## 15. T・C・R・Fの役割

TCRFの四つの宣言を整理します。

### T (Type 型)

値の意味、構造、状態を型として定義します。

```text
T Price [Decimal]
```

### C (Constant 定数)

固定値を定義します。

```text
C standardTaxRate = TaxRate 0.10
```

### R (Rule 変換)

一つの意味ある変換を定義します。

```text
R calculateTotal price
  Price > TotalAmount
```

### F (Flow 接続)

RまたはFを接続します。

```text
F main
  Price 1000.0
  calculateTotal
  totalText
  std.printLine
```

---

## 16. RとFの違い

次のように覚えると分かりやすいです。

```text
R = 変換の内部
F = 変換の接続
```

Rでは計算や分岐を書けます。

```text
R calculateTotal price
  Price > TotalAmount

  tax : TaxAmount =
    price * standardTaxRate

  total : TotalAmount =
    price + tax

  total
```

FではRを並べます。

```text
F main
  Price 1000.0
  calculateTotal
  totalText
  std.printLine
```

---

## 17. Fで書かないもの

Fには細かな計算を書きません。

```text
# Fには書かない
tax = price * standardTaxRate
total = price + tax
```

計算はRへ分離します。

```text
R calculateTotal price
  Price > TotalAmount
  ...
```

この制約により、Fを読むといつでも大きな流れが分かります。

---

## 18. 用途型

用途型は、既存の型に新しい意味を与える型です。

```text
T UserId [Text]
T ProductId [Text]
T Price [Decimal]
T Quantity [Int]
```

内部型が同じでも、用途型が異なれば別型です。

```text
UserId != ProductId
```

値は次のように作ります。

```text
UserId "U001"
ProductId "P001"
Price 1200.0
Quantity 3
```

---

## 19. 用途型を使う理由

次の値だけを見ると、その意味は分かりません。

```text
1200.0
```

用途型を付けると意味が明確になります。

```text
Price 1200.0
```

次の二つも区別できます。

```text
UserId "001"
ProductId "001"
```

文字列が同じでも、用途が違うため別型です。

---

## 20. 表現保持型遷移

次のRは、内部表現を変えずに型だけを進めます。

```text
R pay order
  UnpaidOrder => PaidOrder
```

この`=>`を表現保持型遷移と呼びます。

使用条件:

- 入力型と出力型が用途型
- 両者の内部型が同じ
- R本体を持たない
- 暗黙には適用されない

例:

```text
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
```

内部型はどちらも`OrderId`です。

---

## 21. `>`と`=>`

通常の変換には`>`を使います。

```text
R calculateTotal price
  Price > TotalAmount

  ...
```

この場合、R本体で新しい値を計算します。

表現保持型遷移には`=>`を使います。

```text
R pay order
  UnpaidOrder => PaidOrder
```

この場合、R本体はありません。

---

## 22. 失敗する状態遷移

実際の支払い処理が外部決済を伴い、失敗する可能性がある場合は、`=>`ではなく通常のRを使います。

```text
R pay order
  UnpaidOrder > PaidOrder ! Error

  ...
```

`=>`は、失敗せず、内部表現も変わらない単純な状態遷移に限定されます。

---

## 23. レコード型

複数の値をまとめるにはレコード型を使います。

```text
T Product {
  id    ProductId
  name  ProductName
  price UnitPrice
}
```

値は次のように作ります。

```text
Product {
  id    = ProductId "P001"
  name  = ProductName "Keyboard"
  price = UnitPrice 3000.0
}
```

フィールドはドットで参照します。

```text
product.id
product.name
product.price
```

---

## 24. 一要素なら用途型を使う

単に一つの値へ意味を与えるだけなら、レコード型は不要です。

冗長な例:

```text
T OrderId {
  value Text
}
```

短い例:

```text
T OrderId [Text]
```

複数の意味の異なる値をまとめる場合にレコード型を使います。

---

## 25. 代数データ型

複数の可能性のうち一つを表す型です。

```text
T PaymentMethod
  | CreditCard CardInformation
  | BankTransfer BankAccount
  | CashOnDelivery
```

各コンストラクタ行は`|`から始めます。

コンストラクタは、

- ペイロードなし
- ペイロード一つ

のどちらかです。

```text
CashOnDelivery
CreditCard cardInformation
```

---

## 26. 複数ペイロードはレコードへまとめる

次の構文は使えません。

```text
T Point
  | Point Decimal Decimal
```

レコード型にまとめます。

```text
T PointData {
  x Decimal
  y Decimal
}

T Point
  | Point PointData
```

各値にフィールド名が付くため、意味が明確になります。

---

## 27. 状態型の二つの表現方法

状態を表す方法は二つあります。

### 代数データ型

```text
T AccountStatus
  | Active
  | Locked
  | Deleted
```

単純な状態の列挙に向いています。

### 状態ごとの別型

```text
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]
```

状態ごとに許される処理を型で制限したい場合に向いています。

---

## 28. C：定数

定数は`C`で定義します。

```text
T TaxRate [Decimal]

C standardTaxRate = TaxRate 0.10
```

`TaxRate 0.10`は値の構築です。

`standardTaxRate`はその値に付けた名前です。

---

## 29. 通常のR

税込み合計を計算するRです。

```text
T Price [Decimal]
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
```

Rの最後の式が戻り値です。

---

## 30. ローカル値

型を明示できます。

```text
tax : TaxAmount =
  price * standardTaxRate
```

型推論も使えます。

```text
count =
  std.length values
```

重要な意味を持つ値には、型を明示すると読みやすくなります。

---

## 31. 値は不変

TCRFでは、すべての値は不変です。

次のような再代入はできません。

```text
x = 10
x = 20
```

既存値を変更する代わりに、新しい値を作ります。

---

## 32. Rの入力は原則一つ

Rは0個または1個の明示入力を持ちます。

複数の値が必要ならレコード型にまとめます。

```text
T TransferRequest {
  from   Account
  to     Account
  amount TransferAmount
}
```

```text
R transfer request
  TransferRequest > TransferRecord ! Error
```

---

## 33. when

Bool条件の分岐には`when`を使います。

```text
R judge score
  Score > Grade

  when score >= 80
    true
      Excellent

    false
      Passed
```

短い場合:

```text
when score >= 80
  true  Excellent
  false Passed
```

両分岐の結果型は同じでなければなりません。

---

## 34. match

代数データ型の分岐には`match`を使います。

```text
T Grade
  | Excellent
  | Passed
  | Failed
```

```text
R gradeText grade
  Grade > Text

  match grade

    Excellent
      Text "Excellent"

    Passed
      Text "Passed"

    Failed
      Text "Failed"
```

すべてのコンストラクタを処理する必要があります。

---

## 35. ペイロードを持つmatch

```text
T PaymentResult
  | Paid PaymentRecord
  | Rejected RejectionReason
```

```text
R resultText result
  PaymentResult > Text

  match result

    Paid record
      formatPayment record

    Rejected reason
      formatRejection reason
```

分岐内では、ペイロードを名前として利用できます。

---

## 36. Fの初期値

Fの最初では値を構築できます。

用途型:

```text
F main
  Price 1000.0
  calculateTotal
```

レコード型:

```text
F main
  Limit {
    value = 100
  }
  findPrimes
```

代数データ型:

```text
F main
  CashOnDelivery
  processPayment
```

---

## 37. 再利用可能なF

`main`以外のFも定義できます。

```text
F processOrder
  RawOrder > ShippedOrder ! Error

  validate
  create
  pay
  ship
```

このF自体も、別のFから接続できます。

---

## 38. Fのmatch

Fでも代数データ型による分岐ができます。

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

Rのmatchは式を返します。

Fのmatchは、RまたはFの接続列を持ちます。

---

## 39. Error

TCRFには、組み込みの`Error`型が一つだけあります。

失敗可能なRは次のように書きます。

```text
R parseQuantity raw
  RawQuantity > Quantity ! Error
```

このシグネチャは、

- 成功時は`Quantity`
- 失敗時は`Error`

を表します。

---

## 40. Errorを単純にしている理由

TCRFでは、エラー種別をプログラムから区別できません。

次の違いは、すべて`Error`です。

- 数値変換の失敗
- 範囲外
- リスト添字の不正
- 入力読み取りの失敗

これは、エラー処理そのものではなく、型中心設計の学習へ集中するためです。

---

## 41. Errorの自動伝播

```text
R firstProduct products
  Products > Product ! Error

  at products 0
```

`at products 0`が失敗した場合、`firstProduct`も自動的に失敗します。

失敗可能な処理を呼ぶRやFも、`! Error`を持つ必要があります。

---

## 42. Errorは通常値ではない

Errorは次のようには扱えません。

- 変数へ保存
- 比較
- match
- レコードへ格納
- 定数として定義

Errorが`main`まで伝播すると、プログラムは非0終了コードで終了します。

---

## 43. 条件による失敗

汎用的な`throw`構文はありません。

条件によって失敗させるには`std.require`を使います。

```text
R positive value
  Int > Int ! Error

  checked : Void =
    std.require (value > 0)

  value
```

条件が`false`ならErrorになります。

---

## 44. リスト型

同じ型の値を複数持つには`List<Value>`を使います。

```text
List<Int>
List<Product>
List<List<Int>>
```

用途型として包むこともできます。

```text
T Products [List<Product>]
T Numbers [List<Number>]
```

---

## 45. リスト用途型の構築

```text
T Numbers [List<Number>]

Numbers(
  Number 10.0
  Number 20.0
  Number 30.0
)
```

カンマは使いません。

空のリスト用途型:

```text
Numbers()
```

生の空リスト:

```text
std.empty<Int>
```

---

## 46. at

リストの要素参照には`at`を使います。

```text
at products index
```

先頭要素:

```text
at products 0
```

規則:

- 添字は0始まり
- 負の添字はError
- 範囲外はError
- リスト用途型も受け取れる

---

## 47. リストの基本操作

```text
std.isEmpty values
std.first values
std.rest values
std.prepend value values
std.length values
std.reverse values
```

リストは不変です。

`std.prepend`は元のリストを変更せず、新しいリストを返します。

---

## 48. 再帰

Rは自分自身を呼び出せます。

```text
R countdown value
  Int > Void

  when value <= 0
    true
      Void

    false
      countdown (value - 1)
```

相互再帰も利用できます。

処理系は停止性を検証しません。

---

## 49. 範囲生成

整数範囲は`std.inclusive`または`std.exclusive`で作ります。

```text
std.inclusive std.RangeInput {
  first = 2
  last  = 5
}
```

結果:

```text
2, 3, 4, 5
```

`std.exclusive`では最後の値を含みません。

---

## 50. 型付き演算

用途型に対する演算は、型の組み合わせが登録されている場合だけ使えます。

```text
Price * TaxRate > TaxAmount
Price + TaxAmount > TotalAmount
```

次のような意味のない組み合わせは使えません。

```text
ProductId + TaxRate
```

---

## 51. 暗黙型変換はない

次の型を定義します。

```text
T UserId [Text]
```

`Text`を`UserId`として使うには明示的に構築します。

```text
UserId "U001"
```

次の二つは別型です。

```text
"U001"
UserId "U001"
```

---

## 52. 一般的なunwrapはない

用途型の内部値を自由に取り出す`unwrap`は提供しません。

内部値の利用は次に限定されます。

- 型付き演算
- 専用R
- リスト用途型への限定操作
- 表現保持型遷移

これは、用途型の意味を簡単に失わせないためです。

---

## 53. 組み込み型

ここまでの例で必要な型を先に使ってきました。

ここで、組み込み型をまとめます。

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

---

## 54. Int

整数型です。

```text
0
42
-10
```

基本演算:

```text
+
-
*
/
%
```

0除算はErrorです。

---

## 55. Decimal

小数を扱う型です。

```text
0.10
12.5
-3.25
```

基本演算:

```text
+
-
*
/
```

0除算はErrorです。

---

## 56. Text

文字列型です。

```text
"Hello"
```

主な標準機能:

```text
std.trim
std.lower
std.upper
std.concat
std.parseInt
std.parseDecimal
```

---

## 57. Char

一文字を表す型です。

```text
'A'
'\n'
```

Unicode文字を扱います。

---

## 58. Bool

真偽値です。

```text
true
false
```

論理演算:

```text
and
or
not
```

---

## 59. Void

実質的な入力なし、または意味のある戻り値なしを表します。

```text
R hello
  Void > Text
```

```text
R printMessage message
  Text > Void
```

---

## 60. 標準ライブラリ

TCRFでは、教育用途の集約モジュールを使えます。

```text
import std
```

主要な機能は`std`直下にあります。

---

## 61. コンソール

```text
std.print
std.printLine
std.readText
std.debug
```

Hello World:

```text
import std

F main
  Text "Hello, World!"
  std.printLine
```

---

## 62. 整形

```text
std.int
std.decimal
std.bool
std.intList
std.decimalList
std.textList
```

例:

```text
R countText count
  Int > Text

  std.int count
```

---

## 63. 数値変換

```text
std.toDecimal
std.floor
std.round
std.absInt
std.absDecimal
```

例:

```text
std.toDecimal 10
```

結果型は`Decimal`です。

---

## 64. 個別モジュール

標準ライブラリは個別モジュールとしても利用できます。

```text
import std.console as console
import std.list as list
```

```text
console.printLine
list.first
```

入門教材では、原則として次を推奨します。

```text
import std
```

---

## 65. 税込み金額の完全例

```text
import std

T Price [Decimal]
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

R totalText total
  TotalAmount > Text

  std.decimal total

F main
  Price 1000.0
  calculateTotal
  totalText
  std.printLine
```

Fだけを見ると、次の流れが分かります。

```text
Price
  > TotalAmount
  > Text
  > Void
```

---

## 66. 成績判定の完全例

```text
import std

T Score [Int]

T Grade
  | Excellent
  | Passed
  | Failed

R judge score
  Score > Grade

  when score >= 80
    true
      Excellent

    false
      when score >= 60
        true
          Passed

        false
          Failed

R gradeText grade
  Grade > Text

  match grade

    Excellent
      Text "Excellent"

    Passed
      Text "Passed"

    Failed
      Text "Failed"

F main
  Score 75
  judge
  gradeText
  std.printLine
```

---

## 67. 注文処理の拡張例

```text
T RawOrderId [Text]
T OrderId [Text]

T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]
```

```text
R validateOrderId raw
  RawOrderId > OrderId ! Error

  ...
```

```text
R createOrder id
  OrderId > UnpaidOrder

  UnpaidOrder id
```

```text
R pay order
  UnpaidOrder => PaidOrder

R ship order
  PaidOrder => ShippedOrder
```

```text
F processOrder
  RawOrderId > ShippedOrder ! Error

  validateOrderId
  createOrder
  pay
  ship
```

型の流れ:

```text
RawOrderId
  > OrderId
  > UnpaidOrder
  > PaidOrder
  > ShippedOrder
```

---

## 68. よくある設計ミス

### 基本型をそのまま使いすぎる

```text
T Product {
  id   Text
  name Text
}
```

改善:

```text
T ProductId [Text]
T ProductName [Text]

T Product {
  id   ProductId
  name ProductName
}
```

### 状態を複数Boolで表す

```text
T Order {
  paid    Bool
  shipped Bool
}
```

改善:

```text
T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]
```

### 検証結果をBoolだけで返す

```text
R isValid input
  RawInput > Bool
```

改善:

```text
R validate input
  RawInput > ValidInput ! Error
```

### Fに詳細を書く

詳細はRへ分離します。

### 巨大なRを書く

中間型と小さなRへ分けます。

---

## 69. 命名規則

型名とコンストラクタ名は大文字で始めます。

```text
Price
PaidOrder
CashOnDelivery
```

値名、定数名、R名、F名、フィールド名は小文字で始めます。

```text
price
standardTaxRate
calculateTotal
main
```

---

## 70. コメント

コメントは`#`から行末までです。

```text
# comment

T Price [Decimal] # price type
```

---

## 71. インデント

ブロックはインデントで表します。

```text
R calculateTotal price
  Price > TotalAmount

  tax : TaxAmount =
    price * standardTaxRate

  tax
```

標準インデント幅は2スペースです。

タブによるインデントは使えません。

---

## 72. 最後に確認する四つの質問

TCRFでコードを書くときは、次の四つを確認します。

```text
現在の値は何型か。
その型は何を保証しているか。
次にどの型へ変換するのか。
その変換は失敗する可能性があるか。
```

この四つが明確であれば、型中心設計の意図がコードに表れます。

---

## 73. まとめ

TCRFでは、プログラムを次の四つに分けます。

```text
T = 型
C = 定数
R = 一つの変換
F = 変換の接続
```

設計時は次の順序を推奨します。

```text
入力と出力を決める
中間状態を型として列挙する
Rのシグネチャを書く
Fで接続する
最後にRの中身を書く
```

コードを読むときは次の順序を推奨します。

```text
F main
主要なF
Rのシグネチャ
Tの定義
必要なRの本体
```

TCRFの最大の特徴は、処理の前後にある保証を型として表し、許可された型変換だけを接続してプログラムを作ることです。
