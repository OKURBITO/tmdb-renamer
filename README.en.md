<div align="center">

<img src="assets/app_icon.png" width="128" alt="TMDB Media Renamer Icon" />

# TMDB Media Renamer

A native Windows desktop utility built with **Rust + `eframe` / `egui`** for organizing anime, TV series, and movies to follow **TMDB / Emby / Jellyfin / Plex / fnOS** naming standards.<br/>
Ships as a single lightweight executable with zero runtime dependencies, dark/light themes, and high-DPI support.

[![Release](https://img.shields.io/github/v/release/OKURBITO/tmdb-renamer?style=flat-square&color=0ea5e9)](https://github.com/OKURBITO/tmdb-renamer/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%20x64-10b981?style=flat-square)](https://github.com/OKURBITO/tmdb-renamer/releases/latest)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-f97316?style=flat-square)](Cargo.toml)

[简体中文](README.md) · **English** · [日本語](README.ja.md)

</div>

---

## Key Features

- **TMDB Auto-Matching & Auth**
  - Supports both **TMDB API Key (v3)** and **Read Access Token (Bearer v4)**, with automatic retries (429 / 5xx) and HTTP / SOCKS5 proxy support.
  - Strips fansub/encoder tags, resolutions, and codec noise from folder names to match localized TMDB titles and release years.
- **Standard Media Library Naming**
  - **TV Shows / Anime**: `Title (Year)/Season 01/Title S01E01.ext`
    - Routes OVA / SP folders and untagged decimal specials to `Season 00`.
    - Preserves split-part suffixes (`Part1` / `Part 2`), recognizes revision suffixes (`S01E01v2`), and respects explicit season tags on decimal recaps (e.g., `S20E1073.5`).
    - **TMDB Continuous-Episode Reconciliation**: Automatically distinguishes 1-based seasonal shows from continuous long-running series (such as *One Piece*), preserving continuous episode numbers and realigning drifted local season folders to match TMDB.
    - **Optional Season & Episode Titles**: Appends TMDB season names (`Season 01（Monogatari）`, skipping generic `Season N` / `第 N 季`) and episode titles (`S01E01 [Title]`).
  - **Movies**: `Title (Year)/Title (Year).ext` (preserving multi-disc suffixes such as `-cd1`, `-part2`).
  - **Subtitle Pairing**: Detects `.ass`, `.ssa`, `.srt`, `.vtt`, etc., preserving language/forced suffixes (e.g., `.zh-CN.forced.srt`, `.sc.ass`) alongside the matched video file.
- **Extras & Bonus Archiving**
  - Automatically isolates bonus directories (`CDs`, `Scans`, `Fonts`, `NCOP&NCED`, `PV`, `menu`, `SPs`, `特典映像`, etc., plus user-defined `custom_extra_dirs` in [`config.json`](config.example.json)) into a dedicated archive path to prevent scraper pollution.
- **Interactive Preview & Safe Execution**
  - **Batch & Single Modes**: Auto-merges split season folders belonging to the same TMDB TV ID in batch mode, with side-by-side before/after file previews.
  - **Manual Match & Season Override**: Double-click any row (or click **Manual Match**) to re-search TMDB or override the target season (`S00`–`S99`) directly onto an existing match.
  - **Global Collision Detection**: Checks cross-folder and existing-disk conflicts before renaming, pins conflicting rows to the top in red, and offers a copyable conflict details dialog.
  - **Safe File Moves**: Performs two-step case-only renames on Windows, cross-volume move fallback, and automatic empty source directory pruning.

---

## Quick Start

### Option 1: Download Prebuilt Binary (Recommended)

Download **`tmdb_renamer.exe`** from [GitHub Releases](https://github.com/OKURBITO/tmdb-renamer/releases/latest) and run it directly.

### Usage

1. Enter your **TMDB API Key** or **Read Access Token** (and optional proxy such as `http://127.0.0.1:7890`), then click **Test Connection**.
2. Choose the source media directory (supports local drives and Windows UNC NAS shares) and optionally configure the extras archive directory.
3. Select **Batch** or **Single** mode, media type (**Auto** / **TV** / **Movie**), and optional Season/Episode title toggles.
4. Click **Scan & Match TMDB (Preview)**, double-click any entry to manually adjust matches or force a season if needed, and verify 0 conflicts.
5. Click **Execute Rename** to apply the changes.

> Settings are persisted to `config.json` next to the executable (see [`config.example.json`](config.example.json)). Never commit `config.json` with personal credentials; it is already excluded by [`.gitignore`](.gitignore).

---

## Building from Source

### Native Windows (`x86_64-pc-windows-msvc`)

```powershell
cargo test --all-targets
cargo build --release
```

The compiled binary is written to `target/release/tmdb_renamer.exe`. [`build.rs`](build.rs) automatically locates `rc.exe` from Windows Kits 10 to embed [`assets/app_icon.ico`](assets/app_icon.ico) (and skips resource embedding with a warning if no Windows SDK is present).

### Cross-Compiling from Linux (`x86_64-pc-windows-gnu`)

Requires `mingw-w64` (`x86_64-w64-mingw32-gcc` and `x86_64-w64-mingw32-windres`):

```bash
cargo test --lib
cargo build --release --target x86_64-pc-windows-gnu
```
