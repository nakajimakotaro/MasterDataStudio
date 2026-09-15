# GameMasterStudio — Phase 1–4

ゲームのマスターデータを編集する Tauri 2 デスクトップアプリです。React / AG Grid Enterprise は表示と操作を担当し、Rust が Repository データ、CSV、コメント、編集履歴、保存処理を管理します。

## 起動

必要なもの: Node.js 20.19+ / 22.12+、pnpm、Rust stable、Git CLI、お使いの OS の [Tauri 開発環境](https://v2.tauri.app/start/prerequisites/)。

```sh
pnpm install
pnpm tauri dev
```

`pnpm dev` はブラウザ用の画面プレビューです。Repository を操作するには `pnpm tauri dev` を使用してください。

### AG Grid Enterprise license

`.env.example` を `.env.local` にコピーし、取得済みのライセンスキーを設定します。

```dotenv
VITE_AG_GRID_LICENSE_KEY=your-license-key
```

キーはアプリのビルドに埋め込みます。管理するゲームの Repository には保存しません。キー未設定時は AG Grid の評価モードになり、ウォーターマークが表示されます。ライセンス自体はこの Repository に含みません。

## 使い方

1. 起動画面で **Repository を開く** を選択します。未設定の場合は **Project を初期化** で既存のフォルダを選択します。Git Repository がなければ `git init -b main` を実行します。既存 Repository のサブフォルダを選ぶと、その Repository のルートを使用します。
2. `main` などの保護ブランチでは閲覧専用です。**Branch を作成** で Working Branch に切り替えます。初回 Commit 前も同じです。
3. Git Identity が未設定の場合、**Project Settings** で名前とメールアドレスを設定します。Repository-local の Git config に保存します。未設定でも閲覧できます。
4. **Master を作成** で ID、CSV path、Column 一覧、Primary Key を入力します。Columns / Primary Key はそれぞれ 1 行に 1 つ入力します。複合キーの順序は Primary Key の入力順です。
5. Master を開き、セルのダブルクリックで編集します。Row 追加・複製・削除、Column 追加・削除はツールバーから操作します。
6. セルを選択し、右側 Inspector から Table / Row / Cell Comment を編集します。入力欄を離れるか `⌘/Ctrl + Enter` で確定・自動保存します。本文を空白だけにすると削除します。
7. Undo / Redo はツールバー、またはグリッドにフォーカスがある状態で `⌘/Ctrl + Z` / `⌘/Ctrl + Shift + Z`。入力欄の編集中は通常のテキスト Undo です。

CSV とコメントは操作確定時に自動保存されます。Save ボタンはありません。

### 編集ルール

- Primary Key は左に固定し、保存後もセル編集・貼り付け・オートフィルができます。複合キーを常に tuple として扱い、操作後に空・重複があれば全体を拒否します。キーを変更しても行・セルのコメントを引き継ぎます。
- セルはすべて文字列。空値は `""` で、数値への自動変換や NULL はありません。
- CSV は UTF-8 / BOM なし、comma、必須 header、LF、final newline あり。`csv = 1.4.0` の固定 Writer 設定を使用します。
- 保存順は Primary Key tuple の raw string 辞書順。Grid の sort / filter は保存順に影響しません。
- Column は末尾追加、初期値は空文字。削除は確認ダイアログを経由します。
- コピーは Grid の `⌘/Ctrl + C`。複数セル貼り付けはフォーカスしたセルを左上にして適用します。範囲選択後の「選択範囲を埋める」で同じ値を一括設定できます。
- **オートフィル**: セルまたは範囲を選択し、右下の小さな四角を上下左右にドラッグします。1セルなら値をそのままコピーし、`1, 2` や `10, 20` のように複数の数値を選ぶと続きの連番を埋めます。文字列の範囲は繰り返します。`Alt/Option` を押しながらドラッグすると数値の連番とコピーを切り替えられます。検索・並び替え後の表示順で適用し、既存の行・列の範囲内で操作できます。範囲を縮めてもセルは消去しません。保存済みセルの変更はドラッグ1回で1つの Undo 操作になります。新規行は他の編集と同様に「新規行を保存」で確定します。
- 貼り付け・Fill・Clear は論理操作ごとに一括適用。Primary Key の連番変更や入れ替えは、全セルの変更後に空・重複がないか検証します。テーブルを超える貼り付けも拒否します。
- Row / Column を削除すると、対応するコメントも同時に削除します。Row 複製はコメントを複製しません。
- Row をクリックし、Shift を押しながら終了行をクリックすると、その間の行をまとめて選択できます。`⌘/Ctrl + クリック` または左端のチェックボックスで個別に選択・解除できます。左上のチェックボックスまたは表内で `⌘/Ctrl + A` を使うと、検索・フィルターに一致する全行を選択できます。
- 選択後、ツールバーの複製ボタンまたは選択行の右クリックメニューから一括複製できます。100 行選択すれば 100 行すべてのセル値をコピーし、複製した行を選択します。複製・追加した未保存行は、並び替え中でも追加順で表の一番下に表示します。Primary Key を重複しない値に変更して「新規行を保存」で確定してください。保存後は通常の並び替えに従います。
- Undo / Redo は Project 全体の操作履歴で、直近 100 操作。Project の開き直し、終了で消えます。新しい編集で Redo を消去します。
- 空の Master では Settings から path / Primary Key を変更できます。Row があれば読み取り専用です。
- CSV / コメントを正しく読み込めない Master はエラー表示で編集を無効化します。他の Master は引き続き操作できます。

## Project Config

Repository 内に以下を保存します。

```text
gamemasterstudio/
  project.yaml
  comments/
    enemy.json      # コメントが存在するときだけ
masters/
  enemy.csv
```

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

Master ID は `^[A-Za-z0-9][A-Za-z0-9_-]*$`。CSV path は Repository 相対の `.csv` パスです。絶対パス、`..`、予約ディレクトリ、symlink、重複 path を拒否します。Windows と macOS 間の可搬性のため、大文字小文字だけが違う CSV path / Master ID も重複として扱います。

コメントは 2 spaces indent / LF / final newline ありの JSON。Row コメントはキー tuple、Cell コメントはキー tuple → Column 名順に並べます。作者は Repository context の `git config user.name` / `user.email`、日時は UTC RFC 3339 のミリ秒精度です。GUI はローカル時刻を表示します。

最近開いた Project は Tauri の app data directory の `settings.json` に保存します。Repository には置きません。

## 構成

```text
src/                     React UI、TanStack Query の IPC cache、Zustand の UI state
src-tauri/               Tauri commands、Project session、ローカル設定
crates/core/src/
  config.rs              Project Config の読み書きと検証
  csv_data.rs            CSV と Primary Key
  comments.rs            コメントと作者・日時
  project.rs             編集操作、snapshot、Undo / Redo、Git Identity
  storage.rs             path 検証、一時ファイル、atomic replace と rollback
  merge.rs               UI / Git 非依存の 3-way Merge Engine
  merge_git.rs           Git index stages、Merge session、Resolve / Stage / Commit
  history.rs             Git commit 一覧と snapshot の semantic diff
src/ConflictResolver.tsx 競合一覧、Base / Your Branch / Incoming、手動解決
crates/core/tests/       Phase 1–4 の Domain / Repository テスト
```

AG Grid は [read-only edit](https://www.ag-grid.com/javascript-data-grid/value-setters/#read-only-edit) を使用します。編集要求を Rust に送り、保存成功後の snapshot で画面を更新します。IPC の編集は直列化し、revision でも古い状態の操作を拒否します。保存失敗時は Rust のデータと履歴を進めません。

複数ファイルの変更は、すべての一時ファイルと復元用コピーを用意してから置換します。通常の書き込み失敗では既に置換したファイルも復元します。各ファイルの置換は atomic ですが、複数ファイル全体のプロセスクラッシュ・電源断に対するトランザクション保証はありません。

## 検証・ビルド

```sh
pnpm test
pnpm check
pnpm build
pnpm tauri build
# macOS でアプリのみを開発用ビルド
pnpm tauri build --debug --bundles app
```

Rust テストでは canonical CSV、複合キー、設定・path 検証、コメントの作者維持と削除、Undo / Redo の保存、書き込み失敗時の復元、破損 Master の分離などを検証します。包括的な UI / E2E テストスイートは追加していません。

## 実装範囲

Project の初期化・読み込み、設定、Master 一覧・作成、CSV parse / serialize、複合 Primary Key、AG Grid Editor、Cell / Row / Column 編集、空文字、3 種類のコメント、自動保存、Undo / Redo を実装しています。

Phase 2 の Git status / Branch / Commit / Push / Fetch / Update / Change Review / Semantic Diff / Revert と、Phase 3 の 3-way Merge / Conflict Resolver / Resolve & Stage を実装しています。Phase 4 の Comment semantic merge、History、Protected Branch 設定と UX 改善も実装しています。

仕様どおり、外部ツールによる編集中の Repository 変更、File Watcher、Repair Mode、高度な Schema / Validation は対象外です。


## Phase 3: Merge / Conflict Resolver

1. tracked Working Tree と Index を clean にして、Working Branch 上で上部の **Merge** を選びます。Incoming に local branch、remote branch（`origin/feature` など）、commit を指定できます。upstream を取り込む場合は **Update** を使用します。
2. 自動解決できる変更は Rust がマージします。全件を自動解決できれば Merge commit まで完了します。判断が必要な場合は **Conflict Resolver** が開き、自動解決件数と残り件数を表示します。
3. Base / Your Branch / Incoming を比較し、**Keep Mine** または **Use Incoming** を選択します。Cell Conflict は **Custom** に任意の文字列（空文字を含む）を入力できます。`ABSENT` は削除・不存在であり、空文字とは異なります。
4. 全件解決後、**Complete Merge** で canonical CSV / metadata / Config を保存し、対象 file を Stage して Merge commit を作成します。message を省略すると Git の Merge message を使います。
5. **Merge を中止** は確認後に `git merge --abort` を実行し、Merge 開始前へ戻します。

### Merge の判定と保存

- Git の競合 path は `git ls-files --unmerged -z` と stage 1 / 2 / 3 の blob から読みます。Working Tree の conflict marker は解析しません。競合していない path の比較には Base / HEAD / MERGE_HEAD の snapshot を使います。
- Working Branch の Merge には `--no-commit --no-edit` を使います。Git がテキストとして自動解決しても、Commit 前に Master の定義や Primary Key の意味を確認します。fast-forward はそのまま適用し、Protected Branch の Update は `--ff-only` です。
- 複合 Primary Key tuple を Row identity とし、値を raw string として 3-way 比較します。片側のみの変更・同じ値への変更は自動採用し、同じ Cell の異なる値だけを競合として残します。
- Row / Column / Master の delete vs modify を扱います。Column 順序は Base → Your Branch の追加 → Incoming の追加。削除が確定した Row / Column に属するコメントは除去します。
- Master の path / Primary Key が食い違う場合は、Master の定義とデータをまとめて選択します。異なる Master が同じ CSV path を使用するなど、組み合わせた Config が無効になる場合は Project 全体の選択を求めます。
- コメントは Table / Row / Cell の identity ごとに本文を semantic merge します。同じ identity の本文が競合したときだけ選択・手動編集を求めます。
- 解決途中の選択は Rust session に保持します。Complete Merge までは Working Tree / index stages を書き換えません。再起動・Project 再オープン時は Git の Merge state から再構築し、選択をやり直せます。
- Merge 中の通常編集・Undo / Redo・通常 Commit・Branch 切替・Update は禁止します。Fetch と Identity 設定は利用できます。
- Stage / Commit 失敗時は保存前のファイルと Index を復元し、session の解決内容を保持して再試行できます。Merge に管理対象外の競合が含まれる場合、アプリが開始した Merge は自動で中止します。管理対象外でも競合せず Git がマージした変更は Merge commit に含まれます。
- 複数の merge base がある Merge、壊れた入力、通常ファイル以外の競合は対応しません。アプリから開始した場合は自動中止し、再オープン時は理由と中止操作を表示します。

`phase_three.rs` では Cell の全組み合わせ（ABSENT / 空文字 / raw string）、複合キー、双方追加、構造削除、Project Config Conflict に加え、一時 Git Repository で index stage の読み取り、再起動、Update、管理対象外 conflict の中止、Commit hook 失敗後の復元と再試行を検証します。


## Phase 4: Comments / History / Protected Branch

### Change Review

編集画面と共通の Grid で変更を確認します。左で Master を切り替え、追加は緑、変更は黄、削除は赤で表示します。削除した行・列も変更前の値を表示し、Primary Key は左に固定します。

- 表は閲覧専用です。セルを選ぶと右側に変更前後と関連する差分、個別の **Revert** を表示します。列の追加・削除は列見出しからも選択できます。
- **変更のある行のみ**、検索、列フィルターで対象を絞り込めます。色に加えて追加・変更・削除のラベルを表示します。
- **テーブル・設定** で Master 全体、定義、Table コメントの変更を確認できます。Project 設定の変更も左の一覧から選択できます。
- **Commit へ** から既存の Commit 画面へ進みます。最初の Commit 前は Revert を無効にします。
- 差分の表示ロジックは `pnpm test:review`（Node.js 22.18 以降）で検証できます。

### Comment semantic merge

- Table / 複合 Primary Key の Row / Primary Key + Column の Cell ごとに比較します。別々のコメントの変更は自動採用します。
- 本文のみを意味上の変更として扱い、作者・日時だけでは競合しません。同じ本文は `updatedAt` が新しい側のコメントを採用し、同時刻なら Your Branch を優先します。
- 同じ本文への変更、片側の追加・変更、delete / unchanged を自動解決します。異なる本文への双方変更や追加、delete / modify は個別に判断します。
- Conflict Resolver には本文と更新者・ローカル日時を表示します。**Keep Mine / Use Incoming / Combine / Edit manually** で解決でき、手動本文が空白のみなら削除します。
- 手動編集の作成者・作成日時は Base（新規追加なら Your Branch、なければ Incoming）から維持し、更新者は Repository の Git Identity、更新日時は解決時の UTC ミリ秒精度にします。
- Row / Column の削除が確定した対象のコメントは削除します。最後のコメントが消えた場合、JSON file 自体を削除します。解決途中の再オープンでは index stages から判断をやり直します。

### History

サイドバーの **History** から現在の Branch のコミット履歴を 50 件ずつ読み込みます。日時・作者・Commit ID・メッセージを確認し、コミットを選ぶと Master / Row / Column / Cell / Comment / Project 設定の変更を表示します。コミットメッセージ・作者で全履歴を検索でき、変更詳細は Master・種類・列・キーや値で絞り込めます。差分は変更前後の表を100件ずつ表示し、対象を選択すると値の全文を確認できます。

Git の first-parent 履歴を表示し、Merge commit は先頭の親との差分、最初の commit は空の Project との差分を表示します。ページ送りは続きのコミットを指すカーソルを使用し、新しい HEAD が追加されても重複・取りこぼしなく前後に移動できます。「最新の履歴へ」で再取得します。履歴は Git snapshot から復元し、独自 database や Working Tree / Index の変更は行いません。壊れた snapshot は理由を表示します。Cell 単体の履歴検索は Later の対象です。

### 大量の変更・競合の操作

- History は50コミット、差分は100件単位で取得・表示します。表示済みページを積み増さず、Rust の差分キャッシュも1コミット分に限定します。
- Conflict Resolver は初期状態で未解決だけを表示します。Master ごとの残件数、種類・列・状態・キーや値の検索で対象を絞り、100件ずつ比較できます。
- 「このページの未解決を選択」「絞り込み内の未解決を全選択」、または個別チェックで対象を選び、Keep Mine / Use Incoming を一括適用できます。全選択は解決済みの判断を含みません。解決済みの判断を変更する場合は状態を切り替えて個別に選択します。
- 適用前に件数・対象・削除になる件数・既存の解決を変更する件数を確認します。一括解決は1回の要求で検証し、マージ全体の再計算も1回です。revision や対象IDが古い場合は部分適用せず拒否します。構造競合の解決で新しい競合が現れた場合は、残件数と一覧に反映されます。
- Change Review は変更のある行を初期表示し、グリッドの仮想化を維持します。右側の詳細も100件単位です。
- 初回の差分計算・マージ計算には入力全体が必要です。競合データは引き続き Project snapshot に保持します。途中の解決内容はセッション内に保持し、Project を閉じた場合は再度解決が必要です。

`pnpm test:review` は5万件の競合の検索・選択を、`cargo test -p gamemasterstudio-core` は2万コミットの全ページ走査、5万セル差分の取得・絞り込み、2万セル競合の一括解決と実際のMerge commitまでを検証します。

### Protected Branch / UX

- **Project Settings → Protected Branches** で 1 行に 1 pattern を設定し、Project Config に自動保存します。`*` / `?` / `[abc]` などの case-sensitive glob に対応し、不正・重複 pattern を拒否します。
- 設定は保護対象外の Working Branch で変更し、Commit で共有します。現在の Branch を新たに保護する設定は、変更を Commit できなくなるため拒否します。
- 保護ブランチでは初回 Commit 前も Master / Comment / 設定の編集、Undo / Redo / Revert、Commit、手動 Merge を禁止します。Fetch、fast-forward only Update、History / Change Review の閲覧、Working Branch 作成は利用できます。
- 上部に upstream と ahead / behind、Push を表示します。保護ブランチから開く Branch dialog は新規作成を初期選択します。
- Master 一覧に Modified を表示し、Change Review にはコメントの対象・変更前後の本文と保護設定変更も表示します。差分を読み込めない場合はエラーを表示します。
- Dialog 切替時に入力状態を初期化し、閲覧専用では Revert / Master 作成などの操作を無効化します。

`phase_four.rs` ではコメントの body 比較の全組み合わせ、日時・作者の採用、複合キーの各コメント対象、構造削除、Git conflict の復元と手動解決、履歴のページ送り・読み取り専用性、保護設定の保存・Undo / Redo、保護ブランチの fast-forward 制限を検証します。
