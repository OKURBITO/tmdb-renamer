# TMDB 影视媒体库智能重命名工具 (Rust 版 v2.2.1)

使用 Rust + `eframe`/`egui` 重构的原生桌面应用，专为 Emby / Jellyfin / Plex / 飞牛影视等媒体库设计，轻量单文件可执行程序，原生支持深色/浅色主题与高 DPI 显示。

## 功能特点

1. **TMDB 自动识别**：支持填写 TMDB API Key（v3）或 Read Access Token（Bearer），内置请求重试（429/5xx）与代理支持；自动去除压制组前缀与画质参数并在 TMDB 匹配官方中文名（无中文时自动回退英文）及发行年份。
2. **标准格式重命名**：
   - 剧集 / 番剧：`剧名 (年份)/Season 01/剧名 S01E01.ext`（OVA/SP 及小数集自动归入 `Season 00`）；保留多段标记（`Part1` / `Part 2`），并自动识别跨季连续集号做相对集号归一化。
   - 可选开启 **「Season 后追加季名（如 化物语）」**（自动过滤泛用 `第 X 季` / `Season X` 命名）及 **「SxxExx 后追加单集标题」**（如 `[黑仪重蟹 其之一]`）。
   - 电影 / 剧场版：`片名 (年份)/片名 (年份).ext`（支持 `CD1` / `Part2` 等分段后缀）。
   - 外挂字幕（`.ass` / `.srt` 等）自动保留双语/强制字幕后缀（如 `.zh-CN.forced.srt`）并与视频同名同目录。
3. **非正片特典隔离**：自动将 `CDs`、`Scans`、`Fonts`、`NCOP&NCED`、`PV`、`menu`、`SPs` 等目录与非正片花絮移至指定特典归档目录，避免媒体中心误刮削。
4. **多季自动合并与全量冲突校验**：
   - 批量扫描同一 TMDB 剧集 ID 的分季文件夹时自动合并。
   - 执行前校验全局目标冲突与磁盘已存在文件；冲突行置顶标红，点击 **「查看冲突详情」** 可查看完整路径对照与来源列表，并一键复制详情。
   - 支持双边表格预览、双击条目手动重搜/改选 TMDB 匹配项、仅大小写变更的两段式安全重命名与跨盘移动，以及整理后空目录清理。

## 配置与使用

1. 启动程序后会在可执行文件同目录生成 `config.json`（该文件包含个人 API Key，已加入 `.gitignore`，切勿提交到公开仓库；可参考 [`config.example.json`](config.example.json)）。
2. 填入你的 TMDB API Key 或 Read Access Token（国内网络如直连超时可填本地代理，如 `http://127.0.0.1:7890` 或 `socks5://...`），点击「测试连接」。
3. 选择「待整理目录」（支持「批量目录」或「单部作品」模式），按需勾选特典归档、季名与单集标题选项。
4. 点击「扫描并匹配 TMDB（预览）」，确认无冲突后点击「执行重命名」。

## 编译与测试

### Windows 原生环境 (`x86_64-pc-windows-msvc`)

```powershell
cargo test --all-targets
cargo build --release
```

输出可执行文件位于 `target/release/tmdb_renamer.exe`。`build.rs` 会自动查找 Windows SDK 的 `rc.exe` 嵌入程序图标（未安装时自动跳过且不影响编译）。

### Linux 交叉编译到 Windows (`x86_64-pc-windows-gnu`)

需安装 MinGW 工具链（`mingw-w64`，提供 `x86_64-w64-mingw32-gcc` 与 `x86_64-w64-mingw32-windres`）：

```bash
cargo test --lib
cargo build --release --target x86_64-pc-windows-gnu
```
