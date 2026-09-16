# GameMasterStudio 開発方針・要件まとめ

## 1. 概要

GameMasterStudio は、ゲーム開発向けのマスターデータ管理ツール。

CSV + Git をベースにしつつ、CSV を単なるテキストファイルとしてではなく、Primary Key を持つ構造化された Master Data として扱う。

最大の特徴は、Git 上で CSV が conflict した場合でも、Primary Key を基準に Row / Column / Cell 単位まで変更内容を理解し、衝突していない変更を自動マージできること。

Tauri を利用した GUI 付きのローカル Desktop Application として実装する。

ユーザーは原則として CSV や Git を直接操作せず、GameMasterStudio の GUI を通して編集・同期・Commit・Merge を行う。

---

# 2. プロダクトの中心思想

GameMasterStudio の本質は CSV Editor ではない。

Git ではテキストとして扱われるゲームマスターデータを、以下の構造として理解する。

```text
Master
  ├─ Schema / Columns
  └─ Rows
       ├─ Primary Key
       └─ Cells
```

Git 上の text conflict を、以下の意味的な conflict に変換する。

```text
Master Conflict
Row Conflict
Cell Conflict
Comment Conflict
```

Primary Key を基準に 3-way merge を行い、意味的に衝突していない変更は自動解決する。

---

# 3. 技術スタック

基本方針:

- Desktop Application: Tauri
- Frontend Language: TypeScript
- Frontend: React
- Build Tool: Vite
- Grid: AG Grid Enterprise
- Backend / Domain Logic: Rust
- Frontend ↔ Backend: Tauri Commands / IPC
- Git: OS にインストール済みの Git CLI を Rust 側から利用
- CSV: Rust の `csv` crate を利用する
- Serialization: `serde` / `serde_json` / `serde_yaml` を利用する
- 日時: Rust 側で RFC 3339 / UTC を扱える crate を利用する
- Frontend State:
  - Data fetching / cache: TanStack Query
  - UI state: Zustand
- Frontend validation: Zod を利用可能
- Package manager: pnpm
- UI parts: Radix UI / shadcn 系を利用可能
- Styling: Tailwind CSS
- File watching: 不要

Tauri を利用

React は UI を担当し、Filesystem / Git / CSV / Merge Engine など Repository に対する操作は Rust 側に集約する。

```text
GameMasterStudio

React + AG Grid
      │
      │ Tauri Commands / IPC
      ▼
Rust
  ├── filesystem
  ├── git CLI
  ├── csv
  ├── project configuration
  ├── comments / metadata
  └── merge engine
```

Git CLI を React から直接実行しない。

AG Grid Enterprise license はアプリケーション側に埋め込み、Repository の設定としては保存しない。

---

# 4. Git 方針

Git をライブラリで再実装しない。

ユーザー環境にインストールされている Git CLI を直接利用する。

GameMasterStudio GUI から、通常のマスターデータ開発に必要な Git 操作を行う。

主な対象:

- Fetch
- Branch 作成
- Branch 切替
- Git status
- Commit
- Push
- Remote 更新取得
- Merge
- Conflict 解決
- Resolve 後の Stage
- History 確認

GameMasterStudio を万能 Git GUI にはしない。

初期対象外:

- interactive rebase
- cherry-pick
- bisect
- reflog 操作
- 高度な reset
- その他複雑な Git 操作

ユーザーは原則として Git CLI や外部 Git GUI を直接触らない運用とする。

---

# 5. Git Conflict の取得方法

Conflict marker が埋め込まれた CSV を直接解析しない。

Git Index にある 3-way merge の各 stage を利用する。

```text
Stage 1 = Base
Stage 2 = Side A
Stage 3 = Side B
```

それぞれを独立した CSV / Master Data として読み込み、GameMasterStudio の Merge Engine で解析する。

---

# 6. Primary Key

Primary Key は最初から複数 Column に対応する。

例:

```yaml
primaryKey:
  - enemy_id
```

複合 Primary Key:

```yaml
primaryKey:
  - stage_id
  - wave_id
```

Row identity は Primary Key を構成する値の tuple とする。

単一 Primary Key を特別扱いしない。

Primary Key を構成する Cell は通常の Cell Editor から編集不可とする。

Primary Key の変更は通常の Cell 編集として扱わない。

初期版では Primary Key の Rename / Re-key 操作は持たない。

---

# 7. CSV 仕様

GameMasterStudio が読み書きする CSV 形式は固定する。

汎用 CSV Editor として、任意形式の CSV を扱う必要はない。

CSV は GameMasterStudio が生成し、ユーザーが外部から直接編集しない前提とする。

GameMasterStudio は保存時に Master 全体を毎回 serialize して CSV を生成する。

元 CSV の細かい formatting や raw row representation を維持する必要はない。

同じ Master Data からは安定した CSV が生成されるようにする。

CSV の quoting / escaping 等の細かな serialization rule は自前実装せず、採用する Rust CSV library と writer option を固定して任せる。

固定仕様:

- UTF-8
- BOM なし
- comma separator
- header 必須
- LF
- final newline あり
- 使用する CSV library / writer option を固定する

Rust の `csv` crate は以下の設定で利用する。

Reader:

- delimiter: `,`
- header: 必須
- UTF-8 のみ
- BOM はエラー
- 行ごとの Column 数不一致はエラー
- quoting / double quote は標準 CSV rule に従う

Writer:

- delimiter: `,`
- line terminator: LF (`\n`)
- quote style: necessary
- double quote escaping を利用
- header を必ず先頭に出力
- final newline あり
- UTF-8 / BOM なし

Cell 内の改行は quoted field として許可し、保存時は LF に正規化する。

CSV は GameMasterStudio における Git 永続化形式として扱う。

---

# 8. Cell の空値

GameMasterStudio では NULL という値は扱わない。

Cell の値は文字列として扱い、値がない場合は空文字 `""` とする。

Merge Engine 内では、Column や Row が存在しない状態を値としての空文字とは区別する。

例えば内部の比較上は以下を区別する。

```text
ABSENT
""
"some value"
```

`ABSENT` は Merge Engine 上の「その Row / Column / Cell がその Side に存在しない」という状態を表すものであり、CSV に保存する値ではない。

UI に `Set NULL` のような操作は持たない。

---

# 9. Column 追加時

Column を追加した場合、既存 Row の値は初期版ではすべて空文字 `""` にする。

初期版では type / default 等の高度な Schema 設定はまだ実装しなくてよい。

将来的には Schema 機能を追加する。

想定する項目:

- string
- int
- float
- bool
- enum
- reference
- default
- min
- max

ただし初期版の UI・実装対象には含めない。

---

# 10. Merge Engine

Merge Engine は UI から独立した Rust の Domain Logic とする。

CSV を parse した後、Primary Key を基準に Master Data として比較する。

Merge 時の Cell 値は raw string として扱い、型変換しない。

例えば以下は別の値として扱う。

```text
"001"
"1"
"1.0"
```

型・enum・reference 等の意味付けは将来の Validation 層の責務とする。

Master / Row / Column が存在しない状態は Merge Engine 内の `ABSENT` として扱い、空文字とは区別する。

---

# 11. Cell 3-way Merge

Base / Side A / Side B を比較する。

基本ルール:

| Base | Side A | Side B | Result |
|---|---|---|---|
| A | A | A | A |
| A | B | A | B |
| A | A | C | C |
| A | B | B | B |
| A | B | C | Conflict |

以下は自動解決する。

- 片側のみ変更
- 両側が同じ値へ変更
- 変更なし

双方が同一 Cell を異なる値へ変更した場合のみ Cell Conflict とする。

---

# 12. Row Merge

Primary Key を使って同一 Row を判断する。

CSV 上の行番号は identity として扱わない。

基本ルール:

| Base | Side A | Side B | Result |
|---|---|---|---|
| あり | 編集 | 未変更 | 編集採用 |
| あり | 未変更 | 編集 | 編集採用 |
| あり | 編集 | 編集 | Cell merge |
| あり | 削除 | 未変更 | 削除 |
| あり | 未変更 | 削除 | 削除 |
| あり | 削除 | 削除 | 削除 |
| あり | 削除 | 編集 | Conflict |
| あり | 編集 | 削除 | Conflict |
| なし | 追加 | なし | 追加 |
| なし | なし | 追加 | 追加 |
| なし | 追加 | 追加 | Cell merge |

Base に存在しない同一 Primary Key の Row が双方で追加された場合も、Row 全体を即 Conflict にはしない。

共通して同じ値になっている Cell は自動採用し、双方で異なる値になっている Cell のみ Cell Conflict とする。

Delete vs Modify は自動解決しない。

---

# 13. Master Merge

Master 自体の追加・削除も semantic merge の対象とする。

Master rename という概念は持たない。

Master 名や定義の変更を rename として追跡せず、必要な場合は追加・削除として扱う。

基本ルール:

| Base | Side A | Side B | Result |
|---|---|---|---|
| なし | 追加 | なし | Side A を追加 |
| なし | なし | 追加 | Side B を追加 |
| なし | 追加 | 追加 | Master 内を semantic merge |
| あり | 未変更 | 未変更 | そのまま |
| あり | 変更 | 未変更 | Side A を採用 |
| あり | 未変更 | 変更 | Side B を採用 |
| あり | 変更 | 変更 | Master 内を semantic merge |
| あり | 削除 | 未変更 | 削除 |
| あり | 未変更 | 削除 | 削除 |
| あり | 削除 | 削除 | 削除 |
| あり | 削除 | 変更 | Master Conflict |
| あり | 変更 | 削除 | Master Conflict |

双方が同名 Master を追加した場合は、即 Master Conflict にはせず、その Master の Column / Row / Cell を semantic merge する。

Master identity は Project Config の `masters` map の key とする。

初期版では、既存の非空 Master の `path` と `primaryKey` は通常 UI から変更不可とする。
Master 作成時に決定し、その後は read-only とする。

Merge 中に既存 Master の `path` または `primaryKey` が Base と異なる Side を検出した場合は、semantic row merge を行わず `Project Config Conflict` とする。
ユーザーは Master 単位で `Keep Mine` / `Use Incoming` を選択する。
この場合、異なる Primary Key 定義をまたいだ Row の自動対応付けは行わない。

Base に存在しない同一 Master identity が双方で追加され、`path` または `primaryKey` が異なる場合も `Project Config Conflict` とする。
定義が同一の場合のみ、その Master Data を semantic merge する。

---

# 14. Column Merge

Column / Header 自体も Master の構造として認識する。

例:

Base:

```text
enemy_id,name,hp
```

Side A:

```text
enemy_id,name,hp,defense
```

Side B:

```text
enemy_id,name,hp
```

この場合 `defense` の追加は自動採用できる。

Column 削除と、その Column に対する編集が衝突した場合は Conflict とする。

Column rename は semantic operation として扱わない。

Column 名を変更する専用 Rename 機能は初期版では持たない。

Column の順序は表示上の schema order として保持するが、Column reorder UI は初期版では持たない。

初期版の Column 追加は末尾追加のみとする。
3-way merge 時の Column order は以下で固定する。

1. Base に存在し、最終的に残る Column を Base の順序で並べる
2. Side A で新規追加された Column を Side A の順序で追加する
3. Side B で新規追加された Column のうち未追加のものを Side B の順序で追加する

同名 Column が双方で追加された場合は 1 Column として扱い、その Column 内の Cell を semantic merge する。

Column delete の基本ルール:

- 片側 delete / もう片側 unchanged: delete
- 両側 delete: delete
- delete / もう片側でその Column の Cell に変更あり: Column Conflict
- Primary Key Column は delete 不可

Column を削除した場合、その Column に属する Cell Comment も同じ semantic operation で削除する。

---

# 15. Row 順序

Row 順序自体はデータ identity として扱わない。

Primary Key が同じなら、CSV 上の行番号が変わっていても同じ Row とする。

保存時の Row 順序は Primary Key tuple の昇順に固定する。

比較は型変換せず raw string の辞書順とし、複合 Primary Key は config に定義された Column 順の tuple 比較とする。
AG Grid 上の sort / filter / 表示順は CSV の保存順に影響しない。

---

# 16. AG Grid 方針

Master Editor には AG Grid Enterprise を利用する。

AG Grid は表示とユーザー interaction を担当する。

AG Grid 内部の row data を GameMasterStudio の唯一の Source of Truth にしない。

GameMasterStudio 側で Master Data state を保持する。

Grid からの操作は GameMasterStudio の編集処理を経由する。

---

# 17. Comments

コメント機能を持つ。

コメントは以下の 3 種類とする。

- Table Comment
- Row Comment
- Cell Comment

コメントは CSV 内には格納しない。

GameMasterStudio 固有の metadata として Repository 内に保存し、Git 管理する。

コメント本文は数行程度の plain text を想定する。

Markdown の編集・レンダリング機能は持たない。

各 Comment の identity は以下で決める。

Table Comment:

```text
Master
```

Row Comment:

```text
Master
Primary Key tuple
```

Cell Comment:

```text
Master
Primary Key tuple
Column
```

例:

```text
Table Comment
Master: enemy
```

```text
Row Comment
Master: enemy
Primary Key: ["1001"]
```

```text
Cell Comment
Master: enemy
Primary Key: ["1001"]
Column: hp
```

複合 Primary Key:

```text
Row Comment
Master: stage_enemy
Primary Key: ["stage_001", "wave_03"]
```

```text
Cell Comment
Master: stage_enemy
Primary Key: ["stage_001", "wave_03"]
Column: enemy_id
```

Comment は 1 identity につき 1 件とし、thread / reply 機能は持たない。

Comment metadata は Repository root 配下の以下に保存する。

```text
.gamemasterstudio/
  project.yaml
  comments/
    <master-id>.json
```

Master ごとに 1 JSON file とする。

例:

```json
{
  "version": 1,
  "table": null,
  "rows": [
    {
      "primaryKey": ["1001"],
      "comment": { "body": "Boss候補", "createdBy": {}, "createdAt": "...", "updatedBy": {}, "updatedAt": "..." }
    }
  ],
  "cells": [
    {
      "primaryKey": ["1001"],
      "column": "hp",
      "comment": { "body": "Boss調整後に再確認", "createdBy": {}, "createdAt": "...", "updatedBy": {}, "updatedAt": "..." }
    }
  ]
}
```

JSON は UTF-8 / BOM なし / LF / 2 spaces indent / final newline ありで canonical serialization する。
`rows` は Primary Key tuple 順、`cells` は Primary Key tuple → Column 名順に並べる。

Comment が 1 件も存在しない Master については comment file を置かない。
最後の Comment を削除した場合は file 自体を削除する。

---

# 18. Comment Metadata

コメントには作者と日時を保存する。

Table / Row / Cell Comment で共通の Comment metadata を利用する。

ユーザー Identity は GameMasterStudio 独自アカウントではなく Git config から取得する。

```text
git config user.name
git config user.email
```

コメントに最低限保持する情報:

```ts
type Comment = {
  body: string

  createdBy: {
    name: string
    email: string
  }

  createdAt: string

  updatedBy: {
    name: string
    email: string
  }

  updatedAt: string
}
```

createdBy / createdAt は編集後も維持する。

updatedBy / updatedAt はコメント編集時に更新する。

日時保存形式は RFC 3339 とする。

内部保存は UTC、`Z` suffix、millisecond precision とし、GUI では local timezone 表示とする。

Comment body の改行は LF に正規化する。
空文字または whitespace のみの Comment は保存せず、既存 Comment なら削除として扱う。

GameMasterStudio 独自の:

- Login
- Password
- User database
- Account management

は作らない。

---

# 19. Comment Conflict

コメントも semantic merge 対象にする。

Table / Row / Cell Comment は、それぞれの identity を基準に比較する。

異なる identity のコメント変更は自動 merge できる。

Comment の semantic content は `body` とする。
作者・日時 metadata の差だけでは Comment Conflict にしない。

同じ Comment identity についての基本ルール:

- 片側のみ追加: 追加
- 双方追加し body が同じ: 自動 merge
- 双方追加し body が異なる: Comment Conflict
- 片側のみ変更 / もう片側 unchanged: 変更採用
- 双方変更し body が同じ: 自動 merge
- 双方変更し body が異なる: Comment Conflict
- delete / unchanged: delete
- delete / delete: delete
- delete / modify: Comment Conflict

双方の body が同じ場合に metadata が異なるときは、`updatedAt` が新しい Comment metadata を採用する。
`updatedAt` も同一なら Side A を採用する。

Conflict Resolver では例えば以下を選択可能にする。

```text
Keep Mine
Keep Incoming
Combine / Edit manually
```

Comment Conflict の対象は Table Comment / Row Comment / Cell Comment のすべてとする。

---

# 20. Project Configuration

GameMasterStudio の共有設定は Repository 内の `.gamemasterstudio/project.yaml` に保存して Git 管理する。

Master identity は `masters` map の key とする。
Master identity は `^[A-Za-z0-9][A-Za-z0-9_-]*$` に制限する。

CSV path は Repository root からの relative path とし、absolute path と `..` を禁止する。
path separator は `/` に正規化する。
複数 Master が同じ CSV path を指すことは禁止する。

例:

```yaml
version: 1

git:
  protectedBranches:
    - main

masters:
  enemy:
    path: masters/enemy.csv
    primaryKey:
      - enemy_id

  stage_enemy:
    path: masters/stage_enemy.csv
    primaryKey:
      - stage_id
      - wave_id
```

共有対象:

- Master definition
- CSV path
- Primary Key
- Protected Branch patterns
- 将来追加される Validation / Schema
- その他 Master の意味に関係する設定

Project name は初期版では Repository directory の basename を表示名として利用し、Project Config には重複保存しない。

PC 固有情報は Repository に保存しない。

例:

- UI preference
- Recent repositories
- Window / local settings

PC 固有情報は Tauri の app data directory に JSON として保存する。
Repository 配下には置かない。

---

# 21. 外部変更

GameMasterStudio 管理中の CSV / Repository を外部ツールから変更することはサポート対象外とする。

以下を前提にしない。

- Excel から CSV 編集
- VS Code から CSV 編集
- Terminal から Git checkout
- 外部 Git GUI から Merge

そのため初期版では以下は不要。

- File Watcher
- 外部変更検知
- 外部変更との同期機構
- 保存前の file hash / mtime による外部変更チェック

GameMasterStudio 自身が CSV 保存・Git 操作を行ったタイミングで内部状態を更新する。

初期版は explicit Save button を持たず、編集操作は logical operation 完了時に自動保存する。

対象:

- Cell edit
- Row add / delete / duplicate
- Column add / delete
- Comment add / edit / delete
- Revert / Undo / Redo

保存は対象 Master / metadata file を canonical serialization し、同一 directory の temporary file 経由で atomic replace する。
書き込みに失敗した場合は、その編集操作を成功扱いにせず UI state を直前状態へ戻してエラー表示する。

Undo / Redo は current session の operation history として保持し、Repository reopen / Branch switch / application restart で clear する。
Undo / Redo history 自体は Repository に永続化しない。

---

# 22. Validation

将来的には Validation Engine を持つ。

Merge Engine とは独立させる。

想定:

- required
- integer
- float
- boolean
- enum
- min / max
- unique
- reference

ただし初期版では高度な Schema / Validation 機能は実装しなくてよい。

Primary Key については最低限:

- empty 不可
- duplicate 不可

は扱う。

CSV parse failure、Primary Key Column 不在、Primary Key の empty / duplicate など、Master として正しく解釈できない状態はエラー扱いとする。

初期版では Repair Mode は持たず、その Master を通常編集できない状態としてエラーを表示する。

---

# 23. テスト

初期開発では包括的な UI Test / E2E Test Suite は作らない。

ただし以下の pure Domain Logic には Rust unit test を必須とする。

- Cell / Row / Column / Master 3-way merge
- ABSENT と empty string の区別
- 複合 Primary Key
- canonical CSV serialization
- Comment semantic merge
- Project Config conflict 判定

Merge Engine 等の Domain Logic は UI から分離する。

---

# 24. 基本ユーザーフロー

GameMasterStudio の日常利用フロー:

```text
Repository を開く
      ↓
Remote の最新状態を確認 / 更新
      ↓
Branch 作成 or 切替
      ↓
Master を編集
      ↓
Comment
      ↓
変更内容を確認
      ↓
Commit
      ↓
Push
      ↓
Remote 更新があれば Merge
      ↓
自動解決可能な差分は自動 merge
      ↓
本当に競合した Cell / Row / Comment のみ手動解決
```

ユーザーに Git の詳細を意識させすぎない。
ただし、ユーザーは開発者なので、ある程度gitに寄せたほうが分かりやすい。

---

# 25. 必要画面

初期版では以下を中心に構成する。

## 25.1 Project Launcher

起動直後。

主な役割:

- Recent Projects
- Open Repository
- Create / Initialize Project
- Git Identity 確認

例:

```text
GameMasterStudio

Recent Projects

MyGame
/path/to/MyGame

AnotherGame
/path/to/AnotherGame

[Open Repository]
[Create Repository]
```

---

## 25.2 Project Home / Dashboard

Repository を開いた直後の画面。

表示:

- Project name
- Current Branch
- Remote status
- Local Changes
- Master 一覧

例:

```text
MyGame

Branch
feature/enemy-balance

Remote
↓ 2 commits
↑ 1 commit

Local Changes
3 masters modified
24 cell changes

Masters
--------------------------------
Enemy       2,430 rows    Modified
Item        1,240 rows
Skill         850 rows
Stage         320 rows
```

主要操作:

- Master を開く
- Fetch / Update
- Branch 切替
- Branch 作成
- Changes を見る
- Commit
- Push


---

## 25.3 Master Editor

GameMasterStudio の中心画面。

利用時間の大半をここで過ごす想定。

AG Grid Enterprise を使用。

例:

```text
Enemy

enemy_id | name    | hp   | attack | note
-------------------------------------------
1001     | Slime   | 100  | 20     |
1002     | Goblin  | 200  | 30     |
1003     | Dragon  | 5000 | 800    |
```

主な操作:

- Cell 編集
- Row 追加
- Row 削除
- Duplicate Row
- Column 追加
- Copy / Paste
- Fill
- Search
- Filter
- Sort
- Clear Cell (Empty String)
- Add / Edit Cell Comment
- Row Comment
- Table Comment

Primary Key Column は AG Grid の左側に pin して常時表示する。
Primary Key Cell は read-only とする。

複合 Primary Key は Project Config の定義順で左側に並べて pin する。

---

# 26. Inspector

コメント専用画面は作らず、Master Editor の右側 Inspector に統合する。

Cell を選択した場合、その Cell に関係する Table / Row / Cell Comment を確認・編集できるようにする。

例:

```text
Inspector

Master: Enemy
ID: 1001
Column: hp

Value
120

Table Comment
全体調整中

Row Comment
Boss候補

Cell Comment
Boss調整後に再確認

Created
Taro
2026/09/15 18:20

Updated
Hanako
2026/09/15 18:43
```

Row を選択した場合は Table / Row Comment、Table 全体に対しては Table Comment を扱えるようにする。

コメントは plain text とし、Markdown preview 等は持たない。

---

# 27. Change Review

Commit 前に semantic diff を確認する画面。

Git text diff をユーザーに見せることを中心にしない。

例:

```text
Changes

Enemy
  Modified 12 rows
  Added     3 rows
  Deleted   1 row

Item
  Modified 4 rows

Comments
  Added     2
  Modified  1
```

詳細:

```text
Enemy / 1001

hp
100 → 120

attack
20 → 25
```

Row 追加:

```text
Enemy / 3001
+ Added Row
```

Row 削除:

```text
Enemy / 2001
- Deleted Row
```

Comment:

```text
Enemy / 1001 / hp

"Boss tuning"
    ↓
"Boss tuning after QA"
```

Change Review から semantic change 単位で Revert 可能にする。

Revert の単位:

- Cell modification
- Added Row / Deleted Row
- Added Column / Deleted Column
- Table / Row / Cell Comment

Revert は Working Tree を HEAD の状態へ戻す編集操作であり、Partial Stage / Partial Commit ではない。

---

# 28. Commit / Sync

Change Review から開く Dialog 程度でよい。

表示:

```text
21 cell changes
3 rows added
1 row deleted
2 comments changed
```

Commit message を入力。

操作:

```text
Commit
Commit & Push
```

GameMasterStudio 管理対象の変更は、初期版ではすべて一括 Commit とする。

Partial Stage / Partial Commit は提供しない。

Branch / Remote status はアプリ上部に常時表示する。

Git 操作は以下に固定する。

Fetch:

```text
git fetch --all --prune
```

Fetch は Working Tree に変更があっても実行可能。

Update:

1. Fetch
2. current branch の upstream (`@{upstream}`) を確認
3. Working Branch では `git merge --no-edit @{upstream}`
4. Protected Branch では `git merge --ff-only @{upstream}`

`git pull` と rebase は利用しない。

Branch switch / Branch create-and-switch / Update / manual Merge は、tracked Working Tree と Index が clean で、merge state でない場合のみ許可する。
自動 stash は行わない。

Push:

- upstream がある場合は通常の `git push`
- upstream がなく `origin` が存在する場合は `git push -u origin <current-branch>`
- force push は提供しない

Commit message は trim 後 non-empty 必須。
Commit では GameMasterStudio 管理対象 file のみ stage し、`git add -A` で Repository 全体を stage しない。

管理対象:

- `.gamemasterstudio/project.yaml`
- `.gamemasterstudio/comments/**`
- Base / HEAD / Working Config のいずれかで参照される Master CSV path

管理対象外の tracked file に変更がある場合、その変更は Commit しない。
Branch switch / Update / Merge は Repository 全体の tracked changes が clean になるまで block する。

Commit & Push で Push が失敗した場合、成功済みの Commit は rollback せず、Push error として表示する。

---

# 29. Conflict Resolver

Conflict が発生した場合の専用画面。

GameMasterStudio が自動解決可能なものは先に自動解決する。

ユーザーには、本当に判断が必要な Conflict のみ見せる。

表示例:

```text
Conflicts 3

Enemy
  1001 / hp
  2040 / attack

Stage
  stage_01 + wave_03 / enemy_count
```

詳細:

```text
Enemy / 1001 / hp

Base
100

Your Branch
120

Incoming
150

[Use 120]
[Use 150]

Custom
[________]

[Resolve]
```

自動解決件数を表示する。

```text
Automatically merged: 37
Needs decision: 3
```

全て解決後:

```text
[Complete Merge]
```

Conflict Resolver の途中の解決状態を GameMasterStudio 独自に永続化する必要はない。

途中でアプリが終了しても Repository が破損した扱いにはしない。再起動後は Git の merge state / index stages から Conflict Resolver を再構築し、必要な解決をやり直す。

Merge conflict に GameMasterStudio 管理対象外の path が含まれる場合、初期版ではその Merge を自動で `git merge --abort` し、GameMasterStudio では解決できない旨を表示する。
pre-merge で clean Working Tree を必須とするため、abort によって merge 開始前の状態へ戻せることを前提とする。

---

# 30. Row Conflict UI

Delete vs Modify など、Cell 単位では解決できない Row Conflict も扱う。

例:

```text
Row Conflict

Your Branch
Deleted Enemy 1001

Incoming
Modified Enemy 1001
hp: 100 → 120

[Delete Row]
[Keep Modified Row]
```

---

# 31. Project / Master Settings

初期版ではシンプルでよい。

Project Settings:

- Repository
- Protected Branches
- Git Identity
- その他 project-level settings

Master Settings:

- CSV path
- Primary Key
- Column list

初期版では既存の非空 Master の CSV path / Primary Key は read-only とする。
Master 作成時またはまだ Row が 0 件の初期設定時のみ変更可能とする。

例:

```text
Enemy

CSV
masters/enemy.csv

Primary Key
[x] enemy_id
[ ] name
[ ] hp
```

複合 Primary Key は複数選択可。

Column 管理:

```text
Columns

enemy_id    Primary Key
name
hp
attack

[Add Column]
```

Add Column:

```text
Column Name
defense

Existing Rows
Empty String

[Add]
```

初期版では追加 Column の既存 Row 値は空文字固定。

---

# 32. History

初期版で大規模な Git History 画面は必須ではない。

将来的には以下があるとよい。

- Master History
- Cell History
- Comment History

Cell 単位で「誰がいつ何を変更したか」を確認できる機能は GameMasterStudio と相性がよい。

History を実装する場合、独自の変更履歴 database は作らず Git commit history から再構築する。
Master / Cell / Comment の履歴は commit snapshot を semantic diff し、Primary Key と Column の意味に変換して表示する。

---

# 33. Branch 運用

Protected Branch を Project Config で設定可能にする。

例:

```text
main
develop
release/*
```

Protected Branch pattern は local branch name に対して case-sensitive glob match する。
`release/*` のような pattern を利用可能とする。

新規 Project 初期化時は remote default branch が取得できる場合、その branch だけを protectedBranches の初期値にする。
取得できない場合は `main` を初期値にする。

Protected Branch 上では以下を禁止する。

- Master / Comment の編集
- Commit
- Merge commit の作成

許可する操作:

- Fetch
- fast-forward only Update
- History / Change Review の閲覧
- Working Branch 作成

編集を開始しようとした場合は Working Branch 作成を促す。

---

# 34. Master Editor の重要操作

今後詳細を詰める対象:

- Add Row
- Duplicate Row
- Delete Row
- Add Column
- Delete Column
- Clear Cell (Empty String)
- Multi-cell Paste
- Fill
- Table Comment
- Row Comment
- Cell Comment
- Semantic Change Review
- Undo / Redo

初期版の Primary Key 入力は手入力のみとする。
auto increment / custom generation rule は Later とする。

Add Row:

- Row 作成前に全 Primary Key component の入力を必須とする
- empty / duplicate は確定前に拒否する
- Primary Key 以外の初期値は空文字

Duplicate Row:

- 非 Primary Key Cell の値をコピーする
- 新しい Primary Key tuple の手入力を必須とする
- Row / Cell Comment はコピーしない

Delete Row:

- Row と、その Row identity に属する Row Comment / Cell Comment を同時に削除する

Add Column:

- 末尾追加のみ
- Column name は non-empty、前後 whitespace なし、CR/LF なし、同一 Master 内で unique
- 既存 Row の値は空文字

Delete Column:

- Primary Key Column は削除不可
- confirmation 必須
- 対象 Column の Cell Comment も削除する

Multi-cell Paste / Fill:

- Primary Key Cell を target に含む操作は全体を reject し、部分適用しない

Clear Cell:

- 空文字 `""` を設定する
- Primary Key Cell では利用不可

Undo / Redo:

- logical edit operation 単位
- current session のみ
- undo / redo 実行時も canonical file を自動保存する

---

# 35. 今後追加予定の Schema 機能

初期版では不要だが、設計上後から追加できるようにする。

```text
string
int
float
bool
enum
reference
default
min
max
```

特に `reference` は将来的に重要。

例:

```text
enemy.drop_item_id
        ↓
item.item_id
```

単なる文字列 / 数値入力ではなく、Master 間参照を Select / Autocomplete で編集できるようにする。

---

# 36. 初期開発優先順位


## Phase 1

- Project を開く
- Project Config
- Master 一覧
- CSV parse / serialize
- 複合 Primary Key
- AG Grid Master Editor
- Cell 編集
- Row 追加 / 削除
- Column 追加 / 削除
- Empty String
- Table / Row / Cell Comment
- Auto Save
- Undo / Redo
- CSV 保存

## Phase 2

- Git status
- Branch
- Commit
- Push
- Fetch / Update
- Change Review
- Semantic Diff
- Semantic Revert

## Phase 3

- Git Conflict stage 取得
- 3-way Merge Engine
- Master / Row / Cell auto merge
- Conflict Resolver
- Resolve & Stage

## Phase 4

- Comment semantic merge
- History
- Protected Branch
- UX 改善

## Later

- Full Schema / Validation
- enum
- reference
- default
- min / max
- Cell History
- Primary Key generation policies

---

# 37. 開発時に優先すること

以下は GameMasterStudio の製品仕様として守る。

- Primary Key は複数 Column 対応
- Row identity は Primary Key
- CSV 行番号を identity にしない
- NULL は扱わず、Cell の空値は空文字として扱う
- CSV format は完全固定
- CSV は毎回 canonical serialization
- CSV / Git は原則 GUI からのみ操作
- Git conflict marker を直接 parse しない
- Git index stages を使う
- Cell 単位で 3-way merge
- Delete vs Modify は Conflict
- AG Grid は UI layer
- Comment は Table / Row / Cell に付けられる
- Comment に created/updated user/time を保存
- User Identity は Git config を利用
- 初期版では高度な Schema / Validation は不要
- 初期版では Test Suite なし
- Git より Master Data を中心にした UX にする

---


# 38. 初期版で固定する追加ルール

## 38.1 Side A / Side B

3-way merge における Side A は current branch / ours、Side B は incoming / theirs とする。
UI 表記は原則として `Your Branch` / `Incoming` を利用する。

## 38.2 ABSENT を含む双方追加

Base に Row が存在せず、双方が同一 Primary Key の Row を追加した場合、Column merge 後の各 Cell を以下で扱う。

| Base | Side A | Side B | Result |
|---|---|---|---|
| ABSENT | value | ABSENT | value |
| ABSENT | ABSENT | value | value |
| ABSENT | value | same value | value |
| ABSENT | value A | value B | Cell Conflict |

空文字 `""` は value であり ABSENT ではない。
したがって `""` と `"foo"` は異なる値として Cell Conflict になる。

Column 自体が片側にしか存在しないことによる Cell ABSENT は、先に Column merge で構造を確定し、その Column を追加した Side の値を採用する。

## 38.3 Frontend / Backend state

Repository / Master Data の authoritative state と永続化処理は Rust 側に置く。
React は Rust から取得した snapshot / view model を表示する。

TanStack Query は Rust command の data fetching / mutation / cache に利用し、Zustand は selection、dialog、panel、filter など UI state のみに利用する。
AG Grid row data / TanStack Query cache / Zustand のいずれも Repository data の唯一の Source of Truth にはしない。

## 38.4 Git Identity

Git identity は Repository context で以下を取得する。

```text
git config user.name
git config user.email
```

Repository-local config があればそれを優先する Git 標準挙動に従う。

name / email のどちらかが未設定の場合、Repository の閲覧は可能だが Master / Comment の編集と Commit を disable する。
Project Settings から Repository-local (`--local`) の `user.name` / `user.email` を設定できるようにする。

## 38.5 Merge safety

Conflict Resolver は Git index stage 1 / 2 / 3 を唯一の conflict input とし、working tree に書かれた conflict marker は解析しない。

Master CSV を semantic merge する前に、その Master の Project Config definition が確定している必要がある。
Project Config Conflict が未解決の Master は Row / Cell merge を開始しない。

Merge 完了時は解決済みの canonical CSV / metadata を Working Tree に書き出し、対象 file を stage した後 `git commit` で merge commit を完成させる。

---
