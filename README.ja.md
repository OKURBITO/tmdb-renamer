<div align="center">

<img src="assets/app_icon.png" width="128" alt="TMDB Media Renamer Icon" />

# TMDB メディアライブラリ スマートリネーマー

**Rust + `eframe` / `egui`** で構築されたネイティブ Windows デスクトップアプリです。**Emby / Jellyfin / Plex / fnOS** などのメディアサーバー向けに、アニメ・ドラマ・映画を TMDB 準拠のディレクトリ構成とファイル名へ一括整理します。<br/>
単一実行ファイルで動作し、追加ランタイム不要・ダーク/ライトテーマ・高 DPI 表示に対応しています。

[![Release](https://img.shields.io/github/v/release/OKURBITO/tmdb-renamer?style=flat-square&color=0ea5e9)](https://github.com/OKURBITO/tmdb-renamer/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%20x64-10b981?style=flat-square)](https://github.com/OKURBITO/tmdb-renamer/releases/latest)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-f97316?style=flat-square)](Cargo.toml)

[简体中文](README.md) · [English](README.en.md) · **日本語**

</div>

---

## 主な機能

- **TMDB 自動マッチング & 認証**
  - **TMDB API Key (v3)** と **Read Access Token (Bearer v4)** の両方に対応し、リトライ（429 / 5xx）および HTTP / SOCKS5 プロキシをサポート。
  - エンコードグループ名や解像度・コーデック表記を自動除去し、TMDB の公式タイトル・公開年を取得します。
- **標準化されたリネーム規則**
  - **TV シリーズ / アニメ**：`タイトル (年)/Season 01/タイトル S01E01.ext`
    - OVA・SP フォルダやシーズン指定のない小数話数を `Season 00` へ自動分類。
    - `Part1` / `Part 2` 分割表記、`S01E01v2` などのリビジョン表記、および明示的なシーズン表記付き小数話数（例：`S20E1073.5`）を保持。
    - **長期連続話数の自動整合**：シーズンごとに `E01` から始まる作品と、通算話数（『ONE PIECE』など）で登録されている作品を TMDB から自動判別し、ローカルの Season フォルダずれも正しい TMDB シーズンへ自動補正します。
    - **シーズン名・各話タイトル付与（任意）**：`Season 01（化物語）` や `S01E01 [タイトル]` 形式に対応（汎用的な `Season N` / `第 N 季` 名はスキップ）。
  - **映画 / 劇場版**：`タイトル (年)/タイトル (年).ext`（`-cd1` / `-part2` 等の分割タグ対応）。
  - **外部字幕ファイル連動**：`.ass` / `.ssa` / `.srt` / `.vtt` などを検出し、`.zh-CN.forced.srt` や `.sc.ass` などの言語サフィックスを維持したまま映像と揃えて配置します。
- **特典映像・ボーナスコンテンツの分離**
  - `CDs`、`Scans`、`Fonts`、`NCOP&NCED`、`PV`、`menu`、`SPs`、`特典映像` などの非本編フォルダ・ファイル（および [`config.json`](config.example.json) の `custom_extra_dirs` で追加した任意フォルダ名）を指定のアーカイブ先へ自動退避し、メディアサーバーの誤スクレイピングを防ぎます。
- **プレビュー & 安全なファイル操作**
  - **一括 / 単体スキャンモード**：一括スキャン時に同一 TMDB ID の複数シーズンフォルダを自動統合。変換前後のファイルパス一覧をプレビューできます。
  - **手動マッチング & Season 強制指定**：行をダブルクリックして TMDB を再検索できるほか、作品単位で `Season 00`～`99` を手動指定して即座にプレビューへ反映できます。
  - **重複・競合チェック**：実行前に宛先重複や既存ファイルとの衝突を検出し、競合行を赤字で最上部に表示。詳細確認とワンクリックコピーに対応。
  - **安全な移動・クリーンアップ**：Windows 大文字/小文字のみの 2 段階リネーム、別ドライブ間のコピー後削除フォールバック、空になった元フォルダの自動削除を行います。

---

## クイックスタート

### 方法 1：ビルド済みバイナリを使用（推奨）

[GitHub Releases](https://github.com/OKURBITO/tmdb-renamer/releases/latest) から最新の **`tmdb_renamer.exe`** をダウンロードしてそのまま起動してください。

### 使い方

1. **TMDB Key**（API Key または Read Access Token）と必要に応じたプロキシ（例：`http://127.0.0.1:7890`）を入力し、接続テストを実行します。
2. 整理対象フォルダ（ローカルドライブおよび Windows UNC NAS パス対応）を選択し、必要に応じて特典アーカイブ先を指定します。
3. スキャンモード・種別・シーズン名/各話タイトル付与の有無を選びます。
4. **スキャン & TMDB マッチング（プレビュー）** をクリックし、競合がないこと（必要に応じて手動修正や Season 指定）を確認したうえで実行します。

> 設定は実行ファイルと同じフォルダの `config.json` に保存されます（[`config.example.json`](config.example.json) 参照）。`.gitignore` により除外されていますが、API キーを含む設定ファイルを公開リポジトリへコミットしないようご注意ください。

---

## ソースからのビルド

### Windows ネイティブ環境 (`x86_64-pc-windows-msvc`)

```powershell
cargo test --all-targets
cargo build --release
```

生成物は `target/release/tmdb_renamer.exe` に出力されます。[`build.rs`](build.rs) は Windows Kits 10 の `rc.exe` を自動検出して [`assets/app_icon.ico`](assets/app_icon.ico) を埋め込みます（未導入の場合は警告を出してアイコン埋め込みをスキップします）。

### Linux からのクロスコンパイル (`x86_64-pc-windows-gnu`)

`mingw-w64`（`x86_64-w64-mingw32-gcc` および `x86_64-w64-mingw32-windres`）が必要です：

```bash
cargo test --lib
cargo build --release --target x86_64-pc-windows-gnu
```
