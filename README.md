<div align="center">

<img src="assets/app_icon.png" width="128" alt="TMDB Media Renamer Icon" />

# TMDB 影视媒体库智能重命名工具

基于 **Rust + `eframe` / `egui`** 构建的原生 Windows 桌面应用，专为 **飞牛影视 / Emby / Jellyfin / Plex** 等媒体中心设计。<br/>
轻量单文件可执行程序，无需安装 Python 或额外运行库，原生支持深色/浅色主题与高 DPI 显示。

[![Release](https://img.shields.io/github/v/release/OKURBITO/tmdb-renamer?style=flat-square&color=0ea5e9)](https://github.com/OKURBITO/tmdb-renamer/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%20x64-10b981?style=flat-square)](https://github.com/OKURBITO/tmdb-renamer/releases/latest)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-f97316?style=flat-square)](Cargo.toml)

**简体中文** · [English](README.en.md) · [日本語](README.ja.md)

</div>

---

## 核心特性

- **TMDB 智能识别与鉴权**
  - 支持 **TMDB API Key (v3)** 与 **Read Access Token (Bearer v4)**，内置请求重试（429 / 5xx）与代理支持（HTTP / SOCKS5）。
  - 自动剥离压制组前缀、分辨率与编码标签，匹配 TMDB 官方中文译名（无中文时自动回退英文名）与发行年份。
- **规范化媒体库命名**
  - **剧集 / 番剧**：`剧名 (年份)/Season 01/剧名 S01E01.ext`
    - 自动将 OVA / SP 与未标注季号的小数集归入 `Season 00`。
    - 保留分段后缀（`Part1` / `Part 2`）、版本号后缀识别（如 `S01E01v2`）以及显式标注季号的小数回顾篇（如 `S20E1073.5`）。
    - **智能长篇连续集号对齐**：自动区分 TMDB 按季从 `E01` 重置编号的番剧与全剧连续编号长篇（如《海贼王》），在本地分季目录与 TMDB 边界存在偏移时自动对齐到准确的 TMDB 季号。
    - **可选附加季名与单集标题**：支持 `Season 01（季度名）`（自动跳过 `第 X 季` / `Season X` 泛用名）与 `S01E01 [剧集名称]`。
  - **电影 / 剧场版**：`片名 (年份)/片名 (年份).ext`（支持 `-cd1` / `-part2` 等多碟分段后缀）。
  - **外挂字幕联动**：自动识别 `.ass` / `.ssa` / `.srt` / `.vtt` 等字幕并保留双语、强制字幕语言后缀（如 `.zh-CN.forced.srt`、`.sc.ass`），与正片视频同名同目录输出。
- **非正片特典隔离**
  - 自动将 `CDs`、`Scans`、`Fonts`、`NCOP&NCED`、`PV`、`menu`、`SPs`、`特典映像` 等特典文件夹或花絮文件归档至指定特典目录（并可在 [`config.json`](config.example.json) 的 `custom_extra_dirs` 字段自定义额外特典目录名），避免媒体中心误刮削。
- **可视化预览与安全执行**
  - **批量与单部双模式**：批量扫描同系列多个季目录时自动合并为同一部作品；支持双边预览（改名前 → 改名后）。
  - **手动修正匹配 / 指定 Season**：双击条目或点击「手动修正匹配」可重搜 TMDB 条目，并支持手动覆盖强制指定 `Season 00`～`99`（无需重搜也可直接应用到当前已匹配剧集）。
  - **全量冲突检测**：执行前检查全局目标重名与磁盘已存在文件；冲突项置顶标红，支持弹窗查看完整冲突详情与一键复制。
  - **安全文件操作**：支持仅大小写变更的两段式安全重命名、跨盘移动回退与整理后空目录自动清理。

---

## 快速开始

### 方式一：直接下载可执行文件（推荐）

前往 [GitHub Releases](https://github.com/OKURBITO/tmdb-renamer/releases/latest) 下载最新的 **`tmdb_renamer.exe`**，双击即可运行。

### 使用步骤

1. 打开程序后填写 **TMDB Key**（API Key 或 Read Access Token），国内网络环境如需代理可填写例如 `http://127.0.0.1:7890`，点击「测试连接」确认连通。
2. 选择「待整理目录」（支持本地磁盘或 NAS UNC 路径如 `\\NAS\媒体\anime`），按需开启「归档非正片特典」并设置归档目录。
3. 根据需要勾选模式（「批量目录」/「单部作品」）、类型（「自动」/「剧集」/「电影」）及季名、单集标题选项。
4. 点击 **「扫描并匹配 TMDB（预览）」**，如有识别偏差或冲突可双击条目手动修改或查看冲突详情。
5. 确认就绪条目无冲突后，点击 **「执行重命名」**。

> 程序会在可执行文件同目录生成 `config.json` 用于保存配置与密钥（参考 [`config.example.json`](config.example.json)）。`.gitignore` 已忽略 `config.json`，请勿将含个人密钥的配置文件上传至公开仓库。

---

## 从源码编译

### Windows 原生环境 (`x86_64-pc-windows-msvc`)

```powershell
cargo test --all-targets
cargo build --release
```

构建产物位于 `target/release/tmdb_renamer.exe`。[`build.rs`](build.rs) 会自动检索 Windows Kits 10 中的 `rc.exe` 嵌入 [`assets/app_icon.ico`](assets/app_icon.ico) 图标（若未安装 Windows SDK 会自动提示并跳过图标嵌入，不影响编译）。

### Linux 交叉编译到 Windows (`x86_64-pc-windows-gnu`)

需提前安装 `mingw-w64` 工具链（提供 `x86_64-w64-mingw32-gcc` 与 `x86_64-w64-mingw32-windres`）：

```bash
cargo test --lib
cargo build --release --target x86_64-pc-windows-gnu
```
