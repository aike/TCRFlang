# TCRF言語入門

- 対象バージョン: TCRF 0.4
- 文書種別: 入門教材
- 対象読者: 一般的なプログラミング言語を一つ以上使ったことがある人

本書は考え方の習得を目的とした入門書です。構文や標準ライブラリの正確な定義は
「TCRF言語 実装仕様書」および「TCRF標準ライブラリ仕様書」を参照してください。

---

## 1. TCRFとは

TCRFは、**型を中心にプログラムを設計する手法を修得するための教育用プログラミング言語**です。

静的型付け言語を生かすには、型を中心に設計すること（以下、型駆動開発）が重要です。しかしながら、たとえばJavaScriptに慣れたプログラマーは、TypeScriptでも処理手順を中心に設計し、型を付けるにしても汎用的な型を付けてしまうことがあります。  
現代的な静的型付け言語は型駆動の開発をすることで、プログラムの安全性と可読性を高めることができます。すなわちこれは言語仕様の問題ではなく、プログラマーの設計手法の問題です。  
そこで、型駆動開発の手法を学ぶための教育用言語としてTCRFを作りました。
TCRFでは型を中心とした設計が強制されるため、自然に型駆動開発を学ぶことができます。

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

- `import std` — 標準ライブラリの集約モジュールを読み込みます。
  基本的な機能は`std.printLine`のように`std`直下から利用できます
- `F main` — プログラムの実行開始点です。TCRFでは実行開始点をFlowとして記述します
- `Text "Hello, World!"` — `Text`型の値を作ります
- `std.printLine` — 直前の`Text`値を受け取って画面へ表示します

std.printLineの型の流れは次のとおりです。

```text
Text
  > Void
```

---

## 3. 型駆動開発

Hello Worldだけでは、TCRFの特徴である型駆動開発のメリットは分かりません。

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

### 未払い注文の発送を事前に検知

次のコードはコンパイルできません。

```text
F main
  UnpaidOrder (OrderId "O001")
  ship
```

`ship`の入力型は`PaidOrder`です。しかし、実際に渡される値は`UnpaidOrder`です。

```text
UnpaidOrder != PaidOrder
```

この不一致は、プログラムを実行する前に検出されます。

### 型駆動開発による保証

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

## 6. 型は保証を表す

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

## 7. シグネチャを先に、Fを先に書く

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

## 8. TCRFコードを読む順序

TCRFのプログラムは、上から順番にすべて読む必要はありません。
次の順序で読むと理解しやすくなります。

1. **F mainを読む** — プログラム全体が何をするかを確認する
2. **mainから呼ばれるFを読む** — 大きな処理単位の流れを確認する
3. **Rのシグネチャを読む** — どの型がどの型へ変換されるかを確認する
4. **Tの定義を読む** — 各型が何を保持し、何を保証するかを確認する
5. **必要なRの本体だけ読む** — 詳細な計算や分岐を確認する

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

最初に`main`を見ます。次に`processOrder`を見ます。
その後で、`validate`、`pay`、`ship`のシグネチャを読みます。
最後に、必要に応じて各Rの本体を読みます。

この読み方により、詳細へ入る前に全体像をつかめます。

---

## 9. T・C・R・Fの役割

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

## 10. RとFの違い

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

Fには細かな計算を書きません。

```text
# Fには書かない
tax = price * standardTaxRate
total = price + tax
```

計算はRへ分離します。この制約により、Fを読むといつでも大きな流れが分かります。

---

## 11. 用途型

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
Price 1200.0
```

次の値だけを見ると、その意味は分かりません。

```text
1200.0
```

用途型を付けると意味が明確になり、`UserId "001"`と`ProductId "001"`のように
文字列が同じでも用途が違う値を型で区別できます。

単に一つの値へ意味を与えるだけなら、レコード型ではなく用途型を使います。

```text
# 冗長
T OrderId {
  value Text
}

# 簡潔
T OrderId [Text]
```

なお、暗黙型変換はありません。`"U001"`と`UserId "U001"`は別型で、
`Text`を`UserId`として使うには明示的に構築します。
また、用途型の内部値を自由に取り出す一般的な`unwrap`も提供しません。
内部値の利用は型付き演算・専用R・表現保持変換などに限定されます。
これは、用途型の意味を簡単に失わせないためです。

---

## 12. 表現保持型遷移とfrom

内部表現を変えずに型だけを進めるRは`=>`で書けます。

```text
R pay order
  UnpaidOrder => PaidOrder
```

この`=>`を表現保持型遷移と呼びます。使用条件:

- 入力型と出力型が用途型
- 両者の内部型が同じ
- R本体を持たない

式の形で書きたい場合は`from`を使います。`A from x`は、`x`の値をそのまま持つ
`A`型の値を作ります (内部型が同じ用途型どうしに限る)。

```text
paid : PaidOrder =
  PaidOrder from order
```

実は`=>`は`from`のシンタックスシュガーで、上の`pay`は次と等価です。

```text
R pay order
  UnpaidOrder > PaidOrder

  PaidOrder from order
```

実際の支払い処理が外部決済を伴い、失敗する可能性がある場合は、
`=>`ではなく通常のRを使います。

```text
R pay order
  UnpaidOrder > PaidOrder ! Error

  ...
```

`=>`は、失敗せず、内部表現も変わらない単純な状態遷移に限定されます。

---

## 13. レコード型と代数データ型

複数の値をまとめるにはレコード型を使います。

```text
T Product {
  id    ProductId
  name  ProductName
  price UnitPrice
}
```

値の構築とフィールド参照:

```text
Product {
  id    = ProductId "P001"
  name  = ProductName "Keyboard"
  price = UnitPrice 3000.0
}

product.id
product.price
```

複数の可能性のうち一つを表すには代数データ型を使います。

```text
T PaymentMethod
  | CreditCard CardInformation
  | BankTransfer BankAccount
  | CashOnDelivery
```

各コンストラクタ行は`|`から始め、ペイロードは0個または1個です。
複数の値を持たせたいときはレコード型にまとめてから持たせます。

```text
T PointData {
  x Decimal
  y Decimal
}

T Point
  | Point PointData
```

状態を表す方法は二つあります。

- **代数データ型** — `Active | Locked | Deleted`のような単純な状態の列挙に向く
- **状態ごとの別型** — `UnpaidOrder / PaidOrder / ShippedOrder`のように、
  状態ごとに許される処理を型で制限したい場合に向く

---

## 14. Rの書き方

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

- Rの最後の式が戻り値です
- ローカル値は`名前 : 型 = 式`で型を明示するか、`名前 = 式`で推論に任せます。
  重要な意味を持つ値には型を明示すると読みやすくなります
- すべての値は不変です。再代入もシャドーイングもできません
- Rの明示入力は0個または1個です。複数の値が必要なら
  `T TransferRequest { from Account, to Account, ... }`のように
  レコード型にまとめて渡します

用途型に対する演算は、型の組み合わせに意味がある場合だけ許されます。

```text
Price * TaxRate > TaxAmount
Price + TaxAmount > TotalAmount
```

`ProductId + TaxRate`のような意味のない組み合わせは型エラーです。

---

## 15. 分岐: whenとmatch

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

短い場合は1行にまとめられます。

```text
when score >= 80
  true  Excellent
  false Passed
```

両分岐の結果型は同じでなければなりません。

代数データ型の分岐には`match`を使います。すべてのコンストラクタを
処理する必要があります (網羅性検査)。

```text
R resultText result
  PaymentResult > Text

  match result

    Paid record
      formatPayment record

    Rejected reason
      formatRejection reason
```

`Paid record`のように書くと、分岐内でペイロードを名前として利用できます。

---

## 16. Fの機能

Fの最初のステップでは値を構築できます。

```text
F main
  Price 1000.0
  calculateTotal
```

レコード構築 (`Limit { value = 100 }`) や
代数データ型のコンストラクタも初期値にできます。

`main`以外のFも定義でき、別のFから接続できます。

```text
F processOrder
  RawOrder > ShippedOrder ! Error

  validate
  create
  pay
  ship
```

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

Rのmatchは式を返しますが、FのmatchはRまたはFの接続列を持ちます。

---

## 17. Error

TCRFには、組み込みの`Error`型が一つだけあります。
失敗可能なRは次のように書きます。

```text
R parseQuantity raw
  RawQuantity > Quantity ! Error
```

成功時は`Quantity`、失敗時は`Error`です。

エラー種別をプログラムから区別することはできません。数値変換の失敗も
範囲外もすべて同じ`Error`です。これは、エラー処理そのものではなく、
型駆動開発の学習へ集中するための割り切りです。

Errorは自動的に伝播します。

```text
R firstProduct products
  Products > Product ! Error

  at products 0
```

`at products 0`が失敗した場合、`firstProduct`も自動的に失敗します。
失敗可能な処理を呼ぶRやFも`! Error`を持つ必要があります。

Errorは通常の値ではありません。変数への保存、比較、match、
レコードへの格納はできません。Errorが`main`まで伝播すると、
プログラムは非0終了コードで終了します。

汎用的な`throw`構文はなく、条件によって失敗させるには`std.require`を使います。

```text
R positive value
  Int > Int ! Error

  checked : Void =
    std.require (value > 0)

  value
```

---

## 18. リストと再帰

同じ型の値を複数持つには`List<Value>`を使います。
用途型として包むこともできます。

```text
List<Int>
T Products [List<Product>]
```

リスト用途型の構築にはカンマを使いません。

```text
Numbers(
  Number 10.0
  Number 20.0
)

std.empty<Int>    # 生の空リスト
```

要素参照は`at`です。添字は0始まりで、負や範囲外はErrorです。

```text
at products 0
```

基本操作は標準ライブラリにあります。リストは不変で、
`std.prepend`などは新しいリストを返します。

```text
std.isEmpty values
std.first values
std.rest values
std.prepend value values
std.length values
```

繰り返しはRの再帰で書きます。相互再帰も使えます。

```text
R countdown value
  Int > Void

  when value <= 0
    true
      Void

    false
      countdown (value - 1)
```

整数範囲は`std.inclusive` / `std.exclusive`で生成します。

---

## 19. 組み込み型と標準ライブラリ

組み込み型は次のとおりです。

```text
Int  Decimal  Text  Char  Bool  Void  List<Value>  Error
```

`Int`と`Decimal`の`/`と`%`は0除算がErrorになります。
`Void`は「実質的な入力なし」「意味のある戻り値なし」を表します。

標準ライブラリは`import std`で集約モジュールとして読み込むのが基本です。
コンソール入出力 (`std.printLine`)、整形 (`std.int`、`std.decimal`)、
数値変換 (`std.toDecimal`、`std.round`)、テキスト操作、検証 (`std.require`)
などを提供します。

`import std.console as console`のような個別モジュールも使えますが、
入門段階では`import std`を推奨します。

関数の一覧と正確な型シグネチャは「TCRF標準ライブラリ仕様書」と、
処理系に付属する宣言ファイル`lib/std.tcrf`を参照してください。

---

## 20. 完全例: 税込み金額

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

## 21. 完全例: 注文処理

```text
T RawOrderId [Text]
T OrderId [Text]

T UnpaidOrder [OrderId]
T PaidOrder [OrderId]
T ShippedOrder [OrderId]

R validateOrderId raw
  RawOrderId > OrderId ! Error

  ...

R createOrder id
  OrderId > UnpaidOrder

  UnpaidOrder id

R pay order
  UnpaidOrder => PaidOrder

R ship order
  PaidOrder => ShippedOrder

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

## 22. よくある設計ミス

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

検証に成功したという事実が`ValidInput`型として残り、
以降の処理で再検証が不要になります。

### Fに詳細を書く

詳細はRへ分離します。

### 巨大なRを書く

中間型と小さなRへ分けます。

---

## 23. 表記ルール

- 型名とコンストラクタ名は大文字で始めます (`Price`、`CashOnDelivery`)
- 値名、定数名、R名、F名、フィールド名は小文字で始めます
  (`price`、`calculateTotal`、`main`)
- コメントは`#`から行末までです
- ブロックはインデント (標準2スペース) で表します。タブは使えません

```text
# 税率        ← 行コメント
T Price [Decimal]  # 行末コメント
```

---

## 24. 最後に確認する四つの質問

TCRFでコードを書くときは、次の四つを確認します。

```text
現在の値は何型か。
その型は何を保証しているか。
次にどの型へ変換するのか。
その変換は失敗する可能性があるか。
```

この四つが明確であれば、型駆動開発の意図がコードに表れます。

---

## 25. まとめ

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
