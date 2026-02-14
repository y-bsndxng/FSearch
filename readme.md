# FSearch
ターミナル上で動作する、**Everything 風の「逐次検索」ファイル検索CLI**です。
起動後すぐに文字入力でき、バックグラウンドでファイル一覧を走査しながら、入力に合わせて検索結果が**リアルタイムに更新**されます。

> ✅ 永続インデックスは作りません（DB保存なし）
> ✅ 走査結果はメモリ内に保持します（起動中のみ）
> ✅ Windows / macOS / Linux 対応（PowerShell / Terminal 上で動作）

---

## 特徴

- **逐次検索（インクリメンタル検索）**
  - 入力したクエリに合わせて結果を即時に絞り込み表示
- **バックグラウンド走査**
  - 起動直後から検索でき、走査の進行に合わせて結果が増える
- **シンプルな部分一致**
  - フルパス文字列への部分一致（ファイル内容検索ではありません）
- **クロスプラットフォーム**
  - Windows / macOS / Linux で動作

---

## 必要環境

- Rust stable（edition 2021）
- 対応OS：Windows / macOS / Linux
- ターミナル：PowerShell / Windows Terminal / macOS Terminal など

---

## インストール & 実行

### 1) ビルドして実行（おすすめ：release）
```bash
cargo run --release
```

### 2) ルート指定して実行
全ドライブ/ルートを対象にすると走査が重いので、まずは検索範囲を絞るのがおすすめです。

Windows:
```powershell
cargo run --release -- --root "C:\Users\YOURNAME"
```

macOS/Linux:
```bash
cargo run --release -- --root ~
# ルート全体
cargo run --release -- --root /
```

### 3) バイナリ作成
```bash
cargo build --release
# 実行ファイル:
# target/release/fsearch_live_bg(.exe)
```

---

## 使い方（操作）

- 文字入力：クエリに追加（逐次検索）
- Backspace：1文字削除
- ESC / Ctrl+C：終了

---

## オプション

```text
--root <PATH>         走査するルートディレクトリ
--ignore-case         大文字/小文字を無視（※メモリ増）
--include-dirs        ディレクトリも結果に含める
--limit <N>           表示件数（デフォルト: 30）
--threads <N>         走査スレッド数（0=自動）
```

### デフォルトのルート
- Windows：`SystemDrive` のルート（通常 `C:\`）
- macOS/Linux：`/`

---

## 注意点（重要）

### 1) 「インデックス無し」なので初回走査は重い
Everything のような高速検索体験は、永続インデックスやOS依存の高速更新機構（例：Windows NTFSのUSNジャーナル等）があることで成立します。
本ツールは **永続インデックスを作らない**ため、初回走査コストは避けられません。

ただし、**走査中でも検索は可能**で、結果は「見つかった分だけ」増えていきます。

### 2) メモリを使います
走査したパスを文字列として保持します。ファイル数が多いほどメモリ使用量が増えます。  
`--ignore-case` は lowercase 版も保持するためメモリがさらに増えます。

### 3) 検索は「文字列の部分一致」
高度なクエリ（AND検索、`ext:rs`、スコアリング等）は未実装です。

---

## パフォーマンスのヒント

- 常に `--release` を使う（デバッグビルドは遅い）
- `--root` を狭める（最重要）
- 表示件数を減らす（`--limit` を小さく）
- `--ignore-case` は便利だがメモリ増（必要時だけ）

---

## 依存クレート（概要）

- `clap`：CLI引数
- `crossterm`：ターミナルUI（raw mode / キー入力）
- `ignore`：高速なディレクトリ走査（並列walker）
- `crossbeam-channel`：走査スレッド ↔ UI スレッド通信
- `anyhow`：エラーハンドリング

---

## ライセンス

必要に応じて追記してください（例：MIT / Apache-2.0）。
