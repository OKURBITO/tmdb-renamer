#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("This GUI binary targets Windows (x86_64-pc-windows-gnu). Run `cargo test --lib` on Linux.");
}

#[cfg(windows)]
fn main() -> eframe::Result<()> {
    gui::run()
}

#[cfg(windows)]
mod gui {
    use eframe::egui::{self, Color32, Margin, RichText, Rounding, Stroke, Vec2};
    use egui_extras::{Column, TableBuilder};
    use std::collections::{HashMap, HashSet};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread;

    use tmdb_renamer::config::AppConfig;
    use tmdb_renamer::parser_rules::{
        attach_episode_titles, attach_season_names, build_multi_folder_items, clean_query_name,
        find_collision_map, norm_path_str, path_key, prune_empty_dirs, safe_move,
        validate_all_plans, FolderPlan, ItemKind,
    };
    use tmdb_renamer::tmdb_client::{TMDBClient, TMDBMatch};

    const APP_ICON_RGBA: &[u8] = include_bytes!("../assets/app_icon_64.rgba");

    #[derive(Clone, Copy)]
    struct Palette {
        bg_canvas: Color32,
        bg_card: Color32,
        bg_subtle: Color32,
        bg_input: Color32,
        border: Color32,
        text_primary: Color32,
        text_secondary: Color32,
        accent: Color32,
        accent_soft: Color32,
        success: Color32,
        success_soft: Color32,
        danger: Color32,
        danger_soft: Color32,
        warning: Color32,
        warning_soft: Color32,
    }

    impl Palette {
        fn dark() -> Self {
            Self {
                bg_canvas: Color32::from_rgb(13, 17, 23),
                bg_card: Color32::from_rgb(21, 27, 36),
                bg_subtle: Color32::from_rgb(28, 36, 48),
                bg_input: Color32::from_rgb(15, 21, 29),
                border: Color32::from_rgb(42, 53, 70),
                text_primary: Color32::from_rgb(235, 241, 248),
                text_secondary: Color32::from_rgb(142, 158, 179),
                accent: Color32::from_rgb(14, 165, 233),
                accent_soft: Color32::from_rgb(14, 48, 70),
                success: Color32::from_rgb(16, 185, 129),
                success_soft: Color32::from_rgb(12, 54, 44),
                danger: Color32::from_rgb(244, 63, 94),
                danger_soft: Color32::from_rgb(66, 20, 32),
                warning: Color32::from_rgb(245, 158, 11),
                warning_soft: Color32::from_rgb(64, 42, 12),
            }
        }

        fn light() -> Self {
            Self {
                bg_canvas: Color32::from_rgb(243, 246, 250),
                bg_card: Color32::from_rgb(255, 255, 255),
                bg_subtle: Color32::from_rgb(238, 243, 249),
                bg_input: Color32::from_rgb(248, 250, 252),
                border: Color32::from_rgb(216, 224, 235),
                text_primary: Color32::from_rgb(20, 29, 43),
                text_secondary: Color32::from_rgb(100, 116, 139),
                accent: Color32::from_rgb(2, 132, 199),
                accent_soft: Color32::from_rgb(224, 242, 254),
                success: Color32::from_rgb(5, 150, 105),
                success_soft: Color32::from_rgb(209, 250, 229),
                danger: Color32::from_rgb(225, 29, 72),
                danger_soft: Color32::from_rgb(255, 228, 230),
                warning: Color32::from_rgb(217, 119, 6),
                warning_soft: Color32::from_rgb(254, 243, 199),
            }
        }
    }

    fn apply_modern_theme(ctx: &egui::Context, dark: bool) {
        let pal = if dark { Palette::dark() } else { Palette::light() };
        let mut visuals = if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        visuals.panel_fill = pal.bg_canvas;
        visuals.window_fill = pal.bg_card;
        visuals.extreme_bg_color = pal.bg_input;
        visuals.faint_bg_color = pal.bg_subtle;
        visuals.window_rounding = Rounding::same(12.0);
        visuals.window_stroke = Stroke::new(1.0_f32, pal.border);
        visuals.selection.bg_fill = pal.accent_soft;
        visuals.selection.stroke = Stroke::new(1.0_f32, pal.accent);

        visuals.widgets.noninteractive.bg_fill = pal.bg_card;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, pal.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, pal.text_primary);
        visuals.widgets.noninteractive.rounding = Rounding::same(8.0);

        visuals.widgets.inactive.bg_fill = pal.bg_subtle;
        visuals.widgets.inactive.weak_bg_fill = pal.bg_subtle;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, pal.border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, pal.text_primary);
        visuals.widgets.inactive.rounding = Rounding::same(7.0);

        visuals.widgets.hovered.bg_fill = if dark {
            Color32::from_rgb(38, 49, 66)
        } else {
            Color32::from_rgb(226, 234, 244)
        };
        visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, pal.accent);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, pal.text_primary);
        visuals.widgets.hovered.rounding = Rounding::same(7.0);

        visuals.widgets.active.bg_fill = pal.accent_soft;
        visuals.widgets.active.weak_bg_fill = pal.accent_soft;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, pal.accent);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, pal.text_primary);
        visuals.widgets.active.rounding = Rounding::same(7.0);

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(8.0, 7.0);
        style.spacing.button_padding = Vec2::new(12.0, 5.5);
        style.spacing.window_margin = Margin::same(14.0);
        style.spacing.interact_size.y = 26.0;
        ctx.set_style(style);
    }

    fn card_frame(pal: Palette) -> egui::Frame {
        egui::Frame::none()
            .fill(pal.bg_card)
            .stroke(Stroke::new(1.0_f32, pal.border))
            .rounding(Rounding::same(10.0))
            .inner_margin(Margin::symmetric(14.0, 11.0))
    }

    fn pill_badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
        egui::Frame::none()
            .fill(bg)
            .rounding(Rounding::same(99.0))
            .inner_margin(Margin::symmetric(8.0, 2.0))
            .show(ui, |ui| {
                ui.label(RichText::new(text).color(fg).size(12.0).strong());
            });
    }

    fn segment_btn(ui: &mut egui::Ui, selected: bool, label: &str, pal: Palette) -> bool {
        let (fill, text_col, stroke) = if selected {
            (pal.accent_soft, pal.accent, Stroke::new(1.0_f32, pal.accent))
        } else {
            (pal.bg_input, pal.text_secondary, Stroke::new(1.0_f32, pal.border))
        };
        let btn = egui::Button::new(RichText::new(label).color(text_col).size(12.5))
            .fill(fill)
            .stroke(stroke)
            .rounding(Rounding::same(6.0));
        ui.add(btn).clicked()
    }

    fn toggle_pill(ui: &mut egui::Ui, value: &mut bool, label: &str, pal: Palette) {
        let (fill, text_col, stroke, prefix) = if *value {
            (pal.accent_soft, pal.accent, Stroke::new(1.0_f32, pal.accent), "● ")
        } else {
            (pal.bg_input, pal.text_secondary, Stroke::new(1.0_f32, pal.border), "○ ")
        };
        let btn = egui::Button::new(RichText::new(format!("{}{}", prefix, label)).color(text_col).size(12.5))
            .fill(fill)
            .stroke(stroke)
            .rounding(Rounding::same(99.0));
        if ui.add(btn).clicked() {
            *value = !*value;
        }
    }

    fn primary_btn(ui: &mut egui::Ui, enabled: bool, label: &str, bg: Color32) -> bool {
        let fill = if enabled {
            bg
        } else {
            Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), 80)
        };
        let btn = egui::Button::new(RichText::new(label).color(Color32::WHITE).strong().size(13.0))
            .fill(fill)
            .stroke(Stroke::NONE)
            .rounding(Rounding::same(8.0))
            .min_size(Vec2::new(0.0, 30.0));
        ui.add_enabled(enabled, btn).clicked()
    }

    enum WorkerMsg {
        Status(String),
        TestConnDone(Result<bool, String>),
        ScanDone {
            plans: Vec<FolderPlan>,
            warnings: Vec<String>,
        },
        ManualSearchDone {
            idx: usize,
            query: String,
            matches: Result<Vec<TMDBMatch>, String>,
        },
        ManualApplyDone {
            idx: usize,
            plan: FolderPlan,
            warning: Option<String>,
        },
        ExecDone {
            plans: Vec<FolderPlan>,
            moved: usize,
            errors: Vec<String>,
        },
    }

    struct ManualModal {
        idx: usize,
        query: String,
        season_override: Option<i32>,
        season_input: i32,
        current_match: Option<TMDBMatch>,
        searching: bool,
        matches: Vec<TMDBMatch>,
        error: Option<String>,
    }

    struct ConflictModal {
        title: String,
        content: String,
        copied_tip: bool,
    }

    struct RenamerApp {
        cfg: AppConfig,
        show_api_key: bool,
        config_collapsed: bool,
        plans: Vec<FolderPlan>,
        selected_idx: Option<usize>,
        status: String,
        busy: bool,
        tx: Sender<WorkerMsg>,
        rx: Receiver<WorkerMsg>,
        manual_modal: Option<ManualModal>,
        conflict_modal: Option<ConflictModal>,
        confirm_exec: bool,
        popup_title: Option<String>,
        popup_body: String,
        logo_tex: Option<egui::TextureHandle>,
    }

    fn install_cjk_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        let candidates = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\msyh.ttf",
            r"C:\Windows\Fonts\msyhl.ttc",
            r"C:\Windows\Fonts\simhei.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
        ];
        for path in candidates {
            if let Ok(bytes) = fs::read(path) {
                fonts
                    .font_data
                    .insert("win_cjk".to_owned(), egui::FontData::from_owned(bytes));
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "win_cjk".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("win_cjk".to_owned());
                break;
            }
        }
        ctx.set_fonts(fonts);
    }

    pub fn run() -> eframe::Result<()> {
        let icon = egui::IconData {
            rgba: APP_ICON_RGBA.to_vec(),
            width: 64,
            height: 64,
        };
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1200.0, 800.0])
                .with_min_inner_size([960.0, 640.0])
                .with_title("TMDB 影视媒体库智能重命名工具")
                .with_icon(std::sync::Arc::new(icon)),
            ..Default::default()
        };

        eframe::run_native(
            "TMDB Media Renamer",
            options,
            Box::new(|cc| {
                install_cjk_fonts(&cc.egui_ctx);
                let app = RenamerApp::new(&cc.egui_ctx);
                apply_modern_theme(&cc.egui_ctx, app.cfg.dark_mode);
                Ok(Box::new(app))
            }),
        )
    }

    impl RenamerApp {
        fn new(ctx: &egui::Context) -> Self {
            let (tx, rx) = mpsc::channel();
            let img = egui::ColorImage::from_rgba_unmultiplied([64, 64], APP_ICON_RGBA);
            let logo_tex = Some(ctx.load_texture("app_logo", img, egui::TextureOptions::LINEAR));
            Self {
                cfg: AppConfig::load(),
                show_api_key: false,
                config_collapsed: false,
                plans: Vec::new(),
                selected_idx: None,
                status: "就绪 · 请配置 TMDB Key 并选择媒体目录".to_string(),
                busy: false,
                tx,
                rx,
                manual_modal: None,
                conflict_modal: None,
                confirm_exec: false,
                popup_title: None,
                popup_body: String::new(),
                logo_tex,
            }
        }

        fn show_popup(&mut self, title: &str, body: String) {
            self.popup_title = Some(title.to_string());
            self.popup_body = body;
        }

        fn ready_count(&self) -> usize {
            self.plans.iter().filter(|p| p.is_ready()).count()
        }

        fn conflict_plan_count(&self) -> usize {
            self.plans.iter().filter(|p| !p.collisions.is_empty()).count()
        }

        fn open_conflict_modal(&mut self, idx: usize) {
            let p = match self.plans.get(idx) {
                Some(p) => p,
                None => return,
            };
            if p.collisions.is_empty() {
                self.show_popup("冲突详情", "当前选中作品没有命名冲突。".to_string());
                return;
            }

            let base_dir = &p.target_parent_dir;
            let rel_disp = |path: &Path| -> String {
                path.strip_prefix(base_dir)
                    .map(|r| r.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string_lossy().to_string())
            };

            let dup_map = find_collision_map(&p.items);
            let dup_keys: HashSet<String> = dup_map.keys().map(|k| path_key(k)).collect();
            let col_keys: HashSet<String> = p.collisions.iter().cloned().collect();

            let mut lines = vec![
                format!("作品目录：{}", p.src_dir.display()),
                format!("冲突目标数：{}", p.collisions.len()),
                "=".repeat(72),
                String::new(),
            ];

            let mut seq = 0usize;
            for (dst_path, srcs) in &dup_map {
                seq += 1;
                lines.push(format!("[冲突 #{}] 目标路径相同：{}", seq, rel_disp(dst_path)));
                for (j, sp) in srcs.iter().enumerate() {
                    lines.push(format!("  来源 {}: {}", j + 1, rel_disp(sp)));
                }
                lines.push(String::new());
            }

            let mut seen_extra = HashSet::new();
            for it in &p.items {
                let dk = path_key(&it.dst);
                if col_keys.contains(&dk) && !dup_keys.contains(&dk) && seen_extra.insert(dk) {
                    seq += 1;
                    let reason = if it.dst.exists() {
                        "磁盘上目标文件已存在"
                    } else {
                        "与其他待整理条目目标路径冲突"
                    };
                    lines.push(format!("[冲突 #{}] {}：{}", seq, reason, rel_disp(&it.dst)));
                    lines.push(format!("  来源: {}", rel_disp(&it.src)));
                    lines.push(String::new());
                }
            }

            self.conflict_modal = Some(ConflictModal {
                title: format!("命名冲突详情 · {}（{} 处）", p.display_title, p.collisions.len()),
                content: lines.join("\n"),
                copied_tip: false,
            });
        }

        fn start_test_conn(&mut self, ctx: egui::Context) {
            self.cfg.save();
            let client = match TMDBClient::new(&self.cfg.api_key, &self.cfg.proxy) {
                Ok(c) => c,
                Err(e) => {
                    self.show_popup("提示", e);
                    return;
                }
            };
            self.busy = true;
            self.status = "正在测试 TMDB 连接...".to_string();
            let tx = self.tx.clone();
            thread::spawn(move || {
                let res = client.test_connection();
                let _ = tx.send(WorkerMsg::TestConnDone(res));
                ctx.request_repaint();
            });
        }

        fn start_scan(&mut self, ctx: egui::Context) {
            self.cfg.save();
            let media_dir = norm_path_str(&self.cfg.media_dir);
            if media_dir.as_os_str().is_empty() || !media_dir.is_dir() {
                self.show_popup("提示", "请选择有效的待整理文件夹。".to_string());
                return;
            }
            let client = match TMDBClient::new(&self.cfg.api_key, &self.cfg.proxy) {
                Ok(c) => c,
                Err(e) => {
                    self.show_popup("提示", e);
                    return;
                }
            };

            self.busy = true;
            self.plans.clear();
            self.selected_idx = None;
            self.status = "正在扫描目录并查询 TMDB...".to_string();

            let mode = self.cfg.scan_mode.clone();
            let mtype_pref = self.cfg.media_type.clone();
            let archive_extras = self.cfg.archive_extras;
            let append_ep_title = self.cfg.append_ep_title;
            let append_season_name = self.cfg.append_season_name;
            let extras_dir = norm_path_str(&self.cfg.extras_dir);
            let tx = self.tx.clone();

            thread::spawn(move || {
                let mut targets: Vec<PathBuf> = if mode == "single" {
                    vec![media_dir.clone()]
                } else {
                    let mut dirs = Vec::new();
                    if let Ok(rd) = fs::read_dir(&media_dir) {
                        for entry in rd.flatten() {
                            let p = entry.path();
                            if p.is_dir() {
                                dirs.push(p);
                            }
                        }
                    }
                    dirs.sort();
                    dirs
                };

                let total = targets.len();
                let mut plans: Vec<FolderPlan> = Vec::new();
                let mut warnings: Vec<String> = Vec::new();
                let ex_opt = if extras_dir.as_os_str().is_empty() {
                    None
                } else {
                    Some(extras_dir.as_path())
                };

                for (idx, src_dir) in targets.drain(..).enumerate() {
                    let fname = src_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    let (query, yr, guessed) = clean_query_name(&fname);
                    let qtype = if mtype_pref == "auto" {
                        guessed.as_str()
                    } else {
                        mtype_pref.as_str()
                    };
                    let parent_dir = src_dir.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();

                    let search_res = (|| -> Result<Vec<TMDBMatch>, String> {
                        let mut m = client.search(&query, qtype, yr.as_deref())?;
                        if m.is_empty() && yr.is_some() {
                            m = client.search(&query, qtype, None)?;
                        }
                        if m.is_empty() && qtype != "auto" {
                            m = client.search(&query, "auto", None)?;
                        }
                        Ok(m)
                    })();

                    match search_res {
                        Err(ex) => {
                            plans.push(FolderPlan {
                                src_dir: src_dir.clone(),
                                src_dirs: vec![src_dir],
                                target_parent_dir: parent_dir,
                                folder_name: fname.clone(),
                                query,
                                year_hint: yr,
                                media_type: "tv".to_string(),
                                display_title: fname,
                                tmdb_id: 0,
                                season_override: None,
                                items: Vec::new(),
                                collisions: Vec::new(),
                                status: format!("请求出错: {}", ex),
                                executed: false,
                            });
                        }
                        Ok(matches) if !matches.is_empty() => {
                            let best = &matches[0];
                            let display = best.display_name();
                            let (items, cols) = build_multi_folder_items(
                                &[src_dir.clone()],
                                &parent_dir,
                                &display,
                                &best.media_type,
                                archive_extras,
                                ex_opt,
                                None,
                            );
                            let status = if !cols.is_empty() {
                                format!("⚠ 命名冲突 ({})", cols.len())
                            } else if items.is_empty() {
                                "⚠ 未识别到媒体文件".to_string()
                            } else {
                                "就绪".to_string()
                            };
                            plans.push(FolderPlan {
                                src_dir: src_dir.clone(),
                                src_dirs: vec![src_dir],
                                target_parent_dir: parent_dir,
                                folder_name: fname,
                                query,
                                year_hint: yr,
                                media_type: best.media_type.clone(),
                                display_title: display,
                                tmdb_id: best.tmdb_id,
                                season_override: None,
                                items,
                                collisions: cols,
                                status,
                                executed: false,
                            });
                        }
                        Ok(_) => {
                            plans.push(FolderPlan {
                                src_dir: src_dir.clone(),
                                src_dirs: vec![src_dir],
                                target_parent_dir: parent_dir,
                                folder_name: fname.clone(),
                                query,
                                year_hint: yr,
                                media_type: "tv".to_string(),
                                display_title: fname,
                                tmdb_id: 0,
                                season_override: None,
                                items: Vec::new(),
                                collisions: Vec::new(),
                                status: "未匹配（双击手搜）".to_string(),
                                executed: false,
                            });
                        }
                    }

                    let _ = tx.send(WorkerMsg::Status(format!(
                        "正在匹配 TMDB: {}/{}...",
                        idx + 1,
                        total
                    )));
                    ctx.request_repaint();
                }

                // Auto-merge batch TV folders sharing the same TMDB ID
                let mut merged_plans: Vec<FolderPlan> = Vec::new();
                let mut tv_groups: HashMap<i64, Vec<FolderPlan>> = HashMap::new();
                let mut tv_order: Vec<i64> = Vec::new();
                for fp in plans {
                    if fp.tmdb_id != 0 && fp.media_type == "tv" {
                        if !tv_groups.contains_key(&fp.tmdb_id) {
                            tv_order.push(fp.tmdb_id);
                        }
                        tv_groups.entry(fp.tmdb_id).or_default().push(fp);
                    } else {
                        merged_plans.push(fp);
                    }
                }

                for tid in tv_order {
                    let grp = tv_groups.remove(&tid).unwrap_or_default();
                    if grp.len() == 1 {
                        merged_plans.push(grp.into_iter().next().unwrap());
                        continue;
                    }
                    let first = &grp[0];
                    let grp_src_dirs: Vec<PathBuf> = grp.iter().map(|g| g.src_dir.clone()).collect();
                    let parent_dir = first.target_parent_dir.clone();
                    let (items, cols) = build_multi_folder_items(
                        &grp_src_dirs,
                        &parent_dir,
                        &first.display_title,
                        "tv",
                        archive_extras,
                        ex_opt,
                        None,
                    );
                    let status = if !cols.is_empty() {
                        format!("⚠ 命名冲突 ({})", cols.len())
                    } else {
                        "就绪 (已合并多季)".to_string()
                    };
                    let joined_name = grp
                        .iter()
                        .map(|g| g.folder_name.as_str())
                        .collect::<Vec<_>>()
                        .join(" + ");
                    merged_plans.push(FolderPlan {
                        src_dir: grp_src_dirs[0].clone(),
                        src_dirs: grp_src_dirs,
                        target_parent_dir: parent_dir,
                        folder_name: joined_name,
                        query: first.query.clone(),
                        year_hint: first.year_hint.clone(),
                        media_type: "tv".to_string(),
                        display_title: first.display_title.clone(),
                        tmdb_id: tid,
                        season_override: None,
                        items,
                        collisions: cols,
                        status,
                        executed: false,
                    });
                }

                if mode == "batch"
                    && merged_plans.len() == 1
                    && merged_plans[0].tmdb_id != 0
                    && merged_plans[0].src_dirs.len() == total
                    && total > 1
                {
                    let only = &mut merged_plans[0];
                    let lifted_parent = media_dir.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
                    let (lifted_items, lifted_cols) = build_multi_folder_items(
                        &[media_dir.clone()],
                        &lifted_parent,
                        &only.display_title,
                        &only.media_type,
                        archive_extras,
                        ex_opt,
                        None,
                    );
                    only.src_dir = media_dir.clone();
                    only.src_dirs = vec![media_dir.clone()];
                    only.target_parent_dir = lifted_parent;
                    only.folder_name = media_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    only.items = lifted_items;
                    only.collisions = lifted_cols;
                    only.status = if !only.collisions.is_empty() {
                        format!("⚠ 命名冲突 ({})", only.collisions.len())
                    } else {
                        "就绪 (已合并多季)".to_string()
                    };
                }

                if append_ep_title || append_season_name {
                    for mp in &mut merged_plans {
                        if mp.tmdb_id != 0 && mp.media_type == "tv" && !mp.items.is_empty() {
                            if append_ep_title {
                                let old_items = std::mem::take(&mut mp.items);
                                let (new_items, new_cols, warn) = attach_episode_titles(
                                    old_items,
                                    |tid, s| client.get_season_episodes(tid, s),
                                    mp.tmdb_id,
                                );
                                mp.items = new_items;
                                mp.collisions = new_cols;
                                if let Some(w) = warn {
                                    warnings.push(format!("{}: {}", mp.display_title, w));
                                }
                            }
                            if append_season_name {
                                let old_items = std::mem::take(&mut mp.items);
                                let (new_items, new_cols, warn) = attach_season_names(
                                    old_items,
                                    |tid| client.get_seasons_names(tid),
                                    mp.tmdb_id,
                                );
                                mp.items = new_items;
                                mp.collisions = new_cols;
                                if let Some(w) = warn {
                                    warnings.push(format!("{}: {}", mp.display_title, w));
                                }
                            }
                        }
                    }
                }

                validate_all_plans(&mut merged_plans, true);
                let _ = tx.send(WorkerMsg::ScanDone {
                    plans: merged_plans,
                    warnings,
                });
                ctx.request_repaint();
            });
        }

        fn open_manual_modal(&mut self, idx: usize) {
            if self.busy {
                return;
            }
            if let Some(p) = self.plans.get(idx) {
                if p.executed {
                    self.show_popup("提示", "该条目已执行重命名，如需继续整理请重新扫描。".to_string());
                    return;
                }
                let current_match = if p.tmdb_id != 0 {
                    Some(TMDBMatch {
                        tmdb_id: p.tmdb_id,
                        media_type: p.media_type.clone(),
                        title: p.display_title.clone(),
                        original_title: String::new(),
                        year: String::new(),
                        overview: String::new(),
                    })
                } else {
                    None
                };
                self.manual_modal = Some(ManualModal {
                    idx,
                    query: p.query.clone(),
                    season_override: p.season_override,
                    season_input: p.season_override.unwrap_or(2),
                    current_match,
                    searching: false,
                    matches: Vec::new(),
                    error: None,
                });
            }
        }

        fn start_manual_search(&mut self, ctx: egui::Context) {
            let (idx, query) = match &mut self.manual_modal {
                Some(m) => {
                    let q = m.query.trim().to_string();
                    if q.is_empty() {
                        return;
                    }
                    m.searching = true;
                    m.error = None;
                    m.matches.clear();
                    (m.idx, q)
                }
                None => return,
            };
            let client = match TMDBClient::new(&self.cfg.api_key, &self.cfg.proxy) {
                Ok(c) => c,
                Err(e) => {
                    if let Some(m) = &mut self.manual_modal {
                        m.searching = false;
                        m.error = Some(e);
                    }
                    return;
                }
            };
            let mtype = self.cfg.media_type.clone();
            let tx = self.tx.clone();
            thread::spawn(move || {
                let res = client.search(&query, &mtype, None);
                let _ = tx.send(WorkerMsg::ManualSearchDone {
                    idx,
                    query,
                    matches: res,
                });
                ctx.request_repaint();
            });
        }

        fn apply_manual_choice(
            &mut self,
            idx: usize,
            query: String,
            choice: TMDBMatch,
            season_override: Option<i32>,
            ctx: egui::Context,
        ) {
            let mut plan = match self.plans.get(idx) {
                Some(p) => p.clone(),
                None => return,
            };
            let client = match TMDBClient::new(&self.cfg.api_key, &self.cfg.proxy) {
                Ok(c) => c,
                Err(e) => {
                    self.show_popup("提示", e);
                    return;
                }
            };
            self.manual_modal = None;
            self.busy = true;
            self.status = format!("正在生成预览: {}...", choice.display_name());

            let eff_season = if choice.media_type == "movie" {
                None
            } else {
                season_override
            };

            let archive_extras = self.cfg.archive_extras;
            let append_ep_title = self.cfg.append_ep_title;
            let append_season_name = self.cfg.append_season_name;
            let extras_dir = norm_path_str(&self.cfg.extras_dir);
            let tx = self.tx.clone();

            thread::spawn(move || {
                let ex_opt = if extras_dir.as_os_str().is_empty() {
                    None
                } else {
                    Some(extras_dir.as_path())
                };
                let (mut items, mut cols) = build_multi_folder_items(
                    &plan.src_dirs,
                    &plan.target_parent_dir,
                    &choice.display_name(),
                    &choice.media_type,
                    archive_extras,
                    ex_opt,
                    eff_season,
                );
                let mut warns: Vec<String> = Vec::new();
                if choice.media_type == "tv" && !items.is_empty() {
                    if append_ep_title {
                        let (ni, nc, w) = attach_episode_titles(
                            items,
                            |tid, s| client.get_season_episodes(tid, s),
                            choice.tmdb_id,
                        );
                        items = ni;
                        cols = nc;
                        if let Some(msg) = w {
                            warns.push(msg);
                        }
                    }
                    if append_season_name {
                        let (ni, nc, w) = attach_season_names(
                            items,
                            |tid| client.get_seasons_names(tid),
                            choice.tmdb_id,
                        );
                        items = ni;
                        cols = nc;
                        if let Some(msg) = w {
                            warns.push(msg);
                        }
                    }
                }
                plan.query = query;
                plan.media_type = choice.media_type.clone();
                plan.display_title = choice.display_name();
                plan.tmdb_id = choice.tmdb_id;
                plan.season_override = eff_season;
                plan.items = items;
                plan.collisions = cols;
                plan.status = if !plan.collisions.is_empty() {
                    format!("⚠ 命名冲突 ({})", plan.collisions.len())
                } else if plan.items.is_empty() {
                    "⚠ 未识别到媒体文件".to_string()
                } else if let Some(fs) = eff_season {
                    format!("就绪 (指定 S{:02})", fs)
                } else {
                    "就绪".to_string()
                };

                let _ = tx.send(WorkerMsg::ManualApplyDone {
                    idx,
                    plan,
                    warning: if warns.is_empty() { None } else { Some(warns.join("；")) },
                });
                ctx.request_repaint();
            });
        }

        fn start_execute(&mut self, ctx: egui::Context) {
            let mut plans_snapshot = self.plans.clone();
            let total_files: usize = plans_snapshot
                .iter()
                .filter(|p| p.is_ready())
                .map(|p| p.items.len())
                .sum();
            if total_files == 0 {
                return;
            }
            self.busy = true;
            self.status = format!("正在执行重命名: 0/{}...", total_files);
            let tx = self.tx.clone();

            thread::spawn(move || {
                let mut moved = 0usize;
                let mut processed = 0usize;
                let mut errors: Vec<String> = Vec::new();

                for p in &mut plans_snapshot {
                    if !p.is_ready() {
                        continue;
                    }
                    let mut plan_errs = 0usize;
                    for it in &p.items {
                        processed += 1;
                        match safe_move(&it.src, &it.dst) {
                            Ok(true) => moved += 1,
                            Ok(false) => {}
                            Err(e) => {
                                plan_errs += 1;
                                errors.push(format!("{} -> {}", it.src.display(), e));
                            }
                        }
                        if processed % 5 == 0 || processed == total_files {
                            let _ = tx.send(WorkerMsg::Status(format!(
                                "正在执行重命名: {}/{}...",
                                processed, total_files
                            )));
                            ctx.request_repaint();
                        }
                    }
                    prune_empty_dirs(&p.src_dirs);
                    p.executed = true;
                    p.status = if plan_errs == 0 {
                        "已完成".to_string()
                    } else {
                        format!("⚠ 部分失败 ({})", plan_errs)
                    };
                }

                let _ = tx.send(WorkerMsg::ExecDone {
                    plans: plans_snapshot,
                    moved,
                    errors,
                });
                ctx.request_repaint();
            });
        }

        fn poll_messages(&mut self) {
            while let Ok(msg) = self.rx.try_recv() {
                match msg {
                    WorkerMsg::Status(s) => self.status = s,
                    WorkerMsg::TestConnDone(res) => {
                        self.busy = false;
                        match res {
                            Ok(true) => {
                                self.status = "TMDB 连接成功".to_string();
                                self.show_popup("连接成功", "TMDB API 连接正常！".to_string());
                            }
                            Ok(false) => {
                                self.status = "TMDB 连接失败".to_string();
                                self.show_popup("连接失败", "TMDB 返回了异常响应，请检查 API Key。".to_string());
                            }
                            Err(e) => {
                                self.status = "TMDB 连接失败".to_string();
                                self.show_popup(
                                    "连接失败",
                                    format!("无法连接 TMDB：\n{}\n\n如在国内网络环境，可填写本地 HTTP 代理后重试。", e),
                                );
                            }
                        }
                    }
                    WorkerMsg::ScanDone { plans, warnings } => {
                        self.busy = false;
                        self.plans = plans;
                        self.selected_idx = if self.plans.is_empty() { None } else { Some(0) };
                        let ready = self.ready_count();
                        self.status = format!("扫描完成 · 共 {} 个条目，{} 个就绪", self.plans.len(), ready);
                        if !warnings.is_empty() {
                            self.show_popup("部分季名/集名获取失败", warnings.into_iter().take(6).collect::<Vec<_>>().join("\n"));
                        }
                    }
                    WorkerMsg::ManualSearchDone { idx, query, matches } => {
                        if let Some(m) = &mut self.manual_modal {
                            if m.idx == idx {
                                m.searching = false;
                                m.query = query;
                                match matches {
                                    Ok(list) => {
                                        if list.is_empty() {
                                            m.error = Some("TMDB 未找到匹配结果，请尝试精简关键词。".to_string());
                                        } else {
                                            m.matches = list;
                                        }
                                    }
                                    Err(e) => m.error = Some(format!("搜索失败：{}", e)),
                                }
                            }
                        }
                    }
                    WorkerMsg::ManualApplyDone { idx, plan, warning } => {
                        self.busy = false;
                        if idx < self.plans.len() {
                            let title = plan.display_title.clone();
                            self.plans[idx] = plan;
                            validate_all_plans(&mut self.plans, true);
                            let ready = self.ready_count();
                            self.status = format!("已更新匹配：{} · 当前 {} 个就绪", title, ready);
                        }
                        if let Some(w) = warning {
                            self.show_popup("季名/集名提示", w);
                        }
                    }
                    WorkerMsg::ExecDone { plans, moved, errors } => {
                        self.busy = false;
                        self.plans = plans;
                        if errors.is_empty() {
                            self.status = format!("全部完成 · 共整理 {} 个文件", moved);
                            self.show_popup("全部完成", format!("重命名完成！共整理 {} 个文件。", moved));
                        } else {
                            self.status = format!("执行结束 · 成功 {} 个，失败 {} 个", moved, errors.len());
                            let preview = errors.iter().take(8).cloned().collect::<Vec<_>>().join("\n");
                            self.show_popup(
                                "部分完成",
                                format!("成功移动 {} 个文件，发生 {} 个错误：\n{}", moved, errors.len(), preview),
                            );
                        }
                    }
                }
            }
        }
    }

    impl eframe::App for RenamerApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            self.poll_messages();
            let pal = if self.cfg.dark_mode {
                Palette::dark()
            } else {
                Palette::light()
            };

            // Modern Top Header Bar
            egui::TopBottomPanel::top("modern_header")
                .frame(
                    egui::Frame::none()
                        .fill(pal.bg_card)
                        .stroke(Stroke::new(1.0_f32, pal.border))
                        .inner_margin(Margin::symmetric(16.0, 10.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if let Some(tex) = &self.logo_tex {
                            ui.image((tex.id(), Vec2::new(28.0, 28.0)));
                        }
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("TMDB 影视媒体库智能重命名")
                                        .size(16.0)
                                        .strong()
                                        .color(pal.text_primary),
                                );
                                pill_badge(ui, "v2.2", pal.accent, pal.accent_soft);
                            });
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let theme_lbl = if self.cfg.dark_mode { "☀ 浅色" } else { "🌙 深色" };
                            if ui.button(theme_lbl).clicked() {
                                self.cfg.dark_mode = !self.cfg.dark_mode;
                                apply_modern_theme(ctx, self.cfg.dark_mode);
                                self.cfg.save();
                            }
                            let toggle_lbl = if self.config_collapsed { "展开配置 ▾" } else { "收起配置 ▴" };
                            if ui.button(toggle_lbl).clicked() {
                                self.config_collapsed = !self.config_collapsed;
                            }
                            ui.add_space(8.0);
                            if self.busy {
                                ui.spinner();
                            }
                            pill_badge(ui, &self.status, pal.accent, pal.accent_soft);
                        });
                    });
                });

            // Central Modern Workspace
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(pal.bg_canvas).inner_margin(Margin::same(14.0)))
                .show(ctx, |ui| {
                    // Collapsible Configuration Card
                    if !self.config_collapsed {
                        card_frame(pal).show(ui, |ui| {
                            // Row 1: Credentials & Proxy
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("TMDB Key").color(pal.text_secondary).size(12.5));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.cfg.api_key)
                                        .password(!self.show_api_key)
                                        .hint_text("输入 API Key 或 Read Access Token")
                                        .desired_width(290.0),
                                );
                                let eye = if self.show_api_key { "隐藏" } else { "显示" };
                                if ui.button(eye).clicked() {
                                    self.show_api_key = !self.show_api_key;
                                }
                                if ui.add_enabled(!self.busy, egui::Button::new("测试连接")).clicked() {
                                    self.start_test_conn(ctx.clone());
                                }
                                ui.add_space(12.0);
                                ui.label(RichText::new("代理").color(pal.text_secondary).size(12.5));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.cfg.proxy)
                                        .hint_text("可选，如 http://127.0.0.1:7890")
                                        .desired_width(ui.available_width().max(160.0)),
                                );
                            });

                            // Row 2: Source Directory
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("待整理目录").color(pal.text_secondary).size(12.5));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.cfg.media_dir)
                                        .hint_text("选择本地或 NAS 共享目录")
                                        .desired_width((ui.available_width() - 108.0).max(200.0)),
                                );
                                if ui.add_enabled(!self.busy, egui::Button::new("选择目录...")).clicked() {
                                    if let Some(p) = rfd::FileDialog::new().set_title("选择待整理文件夹").pick_folder() {
                                        let s = norm_path_str(&p.to_string_lossy());
                                        self.cfg.media_dir = s.to_string_lossy().to_string();
                                        if self.cfg.extras_dir.trim().is_empty() {
                                            if let Some(parent) = s.parent() {
                                                self.cfg.extras_dir = parent.join("特典归档").to_string_lossy().to_string();
                                            }
                                        }
                                    }
                                }
                            });

                            // Row 3: Extras Archive Directory
                            ui.horizontal(|ui| {
                                toggle_pill(ui, &mut self.cfg.archive_extras, "归档非正片特典", pal);
                                ui.add_enabled(
                                    self.cfg.archive_extras,
                                    egui::TextEdit::singleline(&mut self.cfg.extras_dir)
                                        .hint_text("CDs / PV / NCOP / Scans 归档路径")
                                        .desired_width((ui.available_width() - 90.0).max(200.0)),
                                );
                                if ui
                                    .add_enabled(!self.busy && self.cfg.archive_extras, egui::Button::new("选择..."))
                                    .clicked()
                                {
                                    if let Some(p) = rfd::FileDialog::new().set_title("选择特典归档目录").pick_folder() {
                                        self.cfg.extras_dir =
                                            norm_path_str(&p.to_string_lossy()).to_string_lossy().to_string();
                                    }
                                }
                            });

                            // Row 4: Segmented Mode/Type + Naming Pills
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("模式").color(pal.text_secondary).size(12.5));
                                if segment_btn(ui, self.cfg.scan_mode == "batch", "批量目录", pal) {
                                    self.cfg.scan_mode = "batch".to_string();
                                }
                                if segment_btn(ui, self.cfg.scan_mode == "single", "单部作品", pal) {
                                    self.cfg.scan_mode = "single".to_string();
                                }

                                ui.add_space(10.0);
                                ui.label(RichText::new("类型").color(pal.text_secondary).size(12.5));
                                for (val, lbl) in [("auto", "自动"), ("tv", "剧集"), ("movie", "电影")] {
                                    if segment_btn(ui, self.cfg.media_type == val, lbl, pal) {
                                        self.cfg.media_type = val.to_string();
                                    }
                                }

                                ui.add_space(10.0);
                                toggle_pill(
                                    ui,
                                    &mut self.cfg.append_season_name,
                                    "Season 后追加季名（如 化物语）",
                                    pal,
                                );
                                toggle_pill(
                                    ui,
                                    &mut self.cfg.append_ep_title,
                                    "SxxExx 后追加单集标题",
                                    pal,
                                );
                            });
                        });
                        ui.add_space(8.0);
                    }

                    // Action Toolbar + KPI Summary
                    ui.horizontal(|ui| {
                        if primary_btn(ui, !self.busy, "扫描并匹配 TMDB（预览）", pal.accent) {
                            self.start_scan(ctx.clone());
                        }
                        let can_edit = !self.busy && self.selected_idx.is_some();
                        if ui
                            .add_enabled(
                                can_edit,
                                egui::Button::new("手动修正匹配").min_size(Vec2::new(0.0, 30.0)),
                            )
                            .clicked()
                        {
                            if let Some(idx) = self.selected_idx {
                                self.open_manual_modal(idx);
                            }
                        }

                        let has_conflicts = self
                            .selected_idx
                            .and_then(|i| self.plans.get(i))
                            .map(|p| !p.collisions.is_empty())
                            .unwrap_or(false);
                        let conf_btn = egui::Button::new(
                            RichText::new("查看冲突详情").color(if has_conflicts {
                                pal.danger
                            } else {
                                pal.text_secondary
                            }),
                        )
                        .fill(if has_conflicts { pal.danger_soft } else { pal.bg_subtle })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if has_conflicts { pal.danger } else { pal.border },
                        ))
                        .min_size(Vec2::new(0.0, 30.0));
                        if ui.add_enabled(has_conflicts, conf_btn).clicked() {
                            if let Some(idx) = self.selected_idx {
                                self.open_conflict_modal(idx);
                            }
                        }

                        if !self.plans.is_empty() {
                            ui.add_space(8.0);
                            let ready = self.ready_count();
                            let conf_cnt = self.conflict_plan_count();
                            let total_files: usize = self.plans.iter().map(|p| p.items.len()).sum();
                            pill_badge(
                                ui,
                                &format!("共 {} 部 · {} 文件", self.plans.len(), total_files),
                                pal.text_secondary,
                                pal.bg_subtle,
                            );
                            pill_badge(ui, &format!("就绪 {}", ready), pal.success, pal.success_soft);
                            if conf_cnt > 0 {
                                pill_badge(ui, &format!("冲突 {}", conf_cnt), pal.danger, pal.danger_soft);
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let ready = self.ready_count();
                            let exec_lbl = if ready > 0 {
                                format!("执行重命名 ({})", ready)
                            } else {
                                "执行重命名".to_string()
                            };
                            if primary_btn(ui, !self.busy && ready > 0, &exec_lbl, pal.success) {
                                self.confirm_exec = true;
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // Split Data Tables
                    let avail_h = ui.available_height();
                    let top_h = (avail_h * 0.44).max(165.0);

                    // Top Card: Folder Matches
                    card_frame(pal).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("识别结果汇总").strong().size(13.5).color(pal.text_primary));
                            ui.label(
                                RichText::new("双击任意行可手动重搜 / 指定 TMDB 条目")
                                    .size(12.0)
                                    .color(pal.text_secondary),
                            );
                        });
                        ui.add_space(4.0);
                        let mut double_clicked: Option<usize> = None;

                        ui.push_id("folders_table", |ui| {
                            TableBuilder::new(ui)
                                .striped(true)
                                .resizable(true)
                                .max_scroll_height(top_h - 46.0)
                                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                .column(Column::initial(220.0).at_least(120.0))
                                .column(Column::initial(220.0).at_least(120.0))
                                .column(Column::initial(180.0).at_least(100.0))
                                .column(Column::initial(56.0))
                                .column(Column::initial(78.0))
                                .column(Column::initial(95.0))
                                .column(Column::remainder().at_least(145.0))
                                .sense(egui::Sense::click())
                                .header(26.0, |mut header| {
                                    for h in ["原文件夹名", "重命名后主目录", "内部季目录", "类型", "TMDB ID", "正片 / 特典", "状态"] {
                                        header.col(|ui| {
                                            ui.label(RichText::new(h).strong().size(12.0).color(pal.text_secondary));
                                        });
                                    }
                                })
                                .body(|mut body| {
                                    for (idx, p) in self.plans.iter().enumerate() {
                                        let is_sel = self.selected_idx == Some(idx);
                                        let is_conf = !p.collisions.is_empty();
                                        let (m_cnt, e_cnt) = p.counts();
                                        let seasons = p.seasons_display();
                                        let row_fg = if is_conf { pal.danger } else { pal.text_primary };

                                        body.row(28.0, |mut row| {
                                            row.set_selected(is_sel);
                                            row.col(|ui| { ui.colored_label(row_fg, &p.folder_name); });
                                            row.col(|ui| { ui.colored_label(row_fg, &p.display_title); });
                                            row.col(|ui| { ui.colored_label(pal.text_secondary, &seasons); });
                                            row.col(|ui| {
                                                let lbl = if p.media_type == "movie" { "电影" } else { "剧集" };
                                                pill_badge(ui, lbl, pal.accent, pal.accent_soft);
                                            });
                                            row.col(|ui| {
                                                ui.colored_label(
                                                    pal.text_secondary,
                                                    if p.tmdb_id != 0 { p.tmdb_id.to_string() } else { "-".to_string() },
                                                );
                                            });
                                            row.col(|ui| { ui.colored_label(row_fg, format!("{} / {}", m_cnt, e_cnt)); });
                                            row.col(|ui| {
                                                let (fg, bg) = if is_conf || p.status.starts_with("请求") {
                                                    (pal.danger, pal.danger_soft)
                                                } else if p.executed {
                                                    (pal.success, pal.success_soft)
                                                } else if p.status.starts_with("未匹配") || p.status.starts_with('⚠') {
                                                    (pal.warning, pal.warning_soft)
                                                } else {
                                                    (pal.success, pal.success_soft)
                                                };
                                                pill_badge(ui, &p.status, fg, bg);
                                            });
                                            let resp = row.response();
                                            if resp.clicked() {
                                                self.selected_idx = Some(idx);
                                            }
                                            if resp.double_clicked() {
                                                self.selected_idx = Some(idx);
                                                double_clicked = Some(idx);
                                            }
                                        });
                                    }
                                });
                        });

                        if let Some(idx) = double_clicked {
                            self.open_manual_modal(idx);
                        }
                    });

                    ui.add_space(8.0);

                    // Bottom Card: File-Level Preview
                    card_frame(pal).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("文件级重命名预览").strong().size(13.5).color(pal.text_primary));
                            ui.label(
                                RichText::new("改名前 → 改名后 · 冲突项自动置顶标红")
                                    .size(12.0)
                                    .color(pal.text_secondary),
                            );
                        });
                        ui.add_space(4.0);
                        let rem_h = (ui.available_height() - 8.0).max(120.0);

                        if let Some(idx) = self.selected_idx {
                            if let Some(p) = self.plans.get(idx) {
                                let base_ref = &p.target_parent_dir;
                                let col_keys: HashSet<String> = p.collisions.iter().cloned().collect();

                                let mut preview_items: Vec<(_, bool)> = Vec::new();
                                for it in &p.items {
                                    if col_keys.contains(&path_key(&it.dst)) {
                                        preview_items.push((it, true));
                                    }
                                }
                                for it in &p.items {
                                    if it.kind == ItemKind::Main && !col_keys.contains(&path_key(&it.dst)) {
                                        preview_items.push((it, false));
                                    }
                                }
                                for it in p
                                    .items
                                    .iter()
                                    .filter(|x| x.kind == ItemKind::Extra && !col_keys.contains(&path_key(&x.dst)))
                                    .take(200)
                                {
                                    preview_items.push((it, false));
                                }

                                ui.push_id("files_table", |ui| {
                                    TableBuilder::new(ui)
                                        .striped(true)
                                        .resizable(true)
                                        .max_scroll_height(rem_h)
                                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                                        .column(Column::initial(92.0))
                                        .column(Column::initial(470.0).at_least(180.0))
                                        .column(Column::remainder().at_least(180.0))
                                        .header(26.0, |mut header| {
                                            for h in ["分类", "改名前路径", "改名后路径"] {
                                                header.col(|ui| {
                                                    ui.label(RichText::new(h).strong().size(12.0).color(pal.text_secondary));
                                                });
                                            }
                                        })
                                        .body(|body| {
                                            body.rows(25.0, preview_items.len(), |mut row| {
                                                let (it, is_conf) = preview_items[row.index()];
                                                let rel_src = it
                                                    .src
                                                    .strip_prefix(base_ref)
                                                    .map(|r| r.to_string_lossy().to_string())
                                                    .unwrap_or_else(|_| it.src.to_string_lossy().to_string());
                                                let rel_dst = if it.kind == ItemKind::Main {
                                                    it.dst
                                                        .strip_prefix(base_ref)
                                                        .map(|r| r.to_string_lossy().to_string())
                                                        .unwrap_or_else(|_| it.dst.to_string_lossy().to_string())
                                                } else {
                                                    it.dst.to_string_lossy().to_string()
                                                };
                                                let fg = if is_conf { pal.danger } else { pal.text_primary };
                                                row.col(|ui| {
                                                    if is_conf {
                                                        pill_badge(ui, "⚠ 冲突", pal.danger, pal.danger_soft);
                                                    } else if it.kind == ItemKind::Main {
                                                        pill_badge(ui, "正片/字幕", pal.accent, pal.accent_soft);
                                                    } else {
                                                        pill_badge(ui, "特典归档", pal.text_secondary, pal.bg_subtle);
                                                    }
                                                });
                                                row.col(|ui| { ui.colored_label(fg, rel_src); });
                                                row.col(|ui| { ui.colored_label(fg, rel_dst); });
                                            });
                                        });
                                });
                            }
                        } else {
                            ui.add_space(24.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("暂无预览内容 · 请点击「扫描并匹配 TMDB」开始")
                                        .size(13.0)
                                        .color(pal.text_secondary),
                                );
                            });
                        }
                    });
                });

            // Conflict Detail Modal
            let mut close_conflict = false;
            if let Some(cm) = &mut self.conflict_modal {
                egui::Window::new(&cm.title)
                    .collapsible(false)
                    .resizable(true)
                    .default_size([840.0, 500.0])
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        egui::ScrollArea::both().max_height(400.0).show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut cm.content.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if primary_btn(ui, true, "复制全部冲突详情", pal.accent) {
                                ctx.copy_text(cm.content.clone());
                                cm.copied_tip = true;
                            }
                            if cm.copied_tip {
                                pill_badge(ui, "已复制到剪贴板", pal.success, pal.success_soft);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("关闭").clicked() {
                                    close_conflict = true;
                                }
                            });
                        });
                    });
            }
            if close_conflict {
                self.conflict_modal = None;
            }

            // Manual TMDB Search Modal
            let mut trigger_search = false;
            let mut chosen: Option<(usize, String, TMDBMatch, Option<i32>)> = None;
            let mut close_modal = false;

            if let Some(modal) = &mut self.manual_modal {
                egui::Window::new("手动修正匹配 / 指定 Season")
                    .collapsible(false)
                    .resizable(true)
                    .default_width(620.0)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        // Season override controls (for series folded under another season on TMDB)
                        egui::Frame::none()
                            .fill(pal.bg_subtle)
                            .stroke(Stroke::new(1.0_f32, pal.border))
                            .rounding(Rounding::same(8.0))
                            .inner_margin(Margin::symmetric(10.0, 8.0))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new("目标 Season").strong().size(12.5).color(pal.text_primary));
                                    if segment_btn(ui, modal.season_override.is_none(), "自动识别", pal) {
                                        modal.season_override = None;
                                    }
                                    if segment_btn(ui, modal.season_override.is_some(), "手动指定", pal) {
                                        modal.season_override = Some(modal.season_input.clamp(0, 99));
                                    }
                                    for s in 0..=5 {
                                        if segment_btn(ui, modal.season_override == Some(s), &format!("S{:02}", s), pal) {
                                            modal.season_input = s;
                                            modal.season_override = Some(s);
                                        }
                                    }
                                    let dv = ui.add(
                                        egui::DragValue::new(&mut modal.season_input)
                                            .range(0..=99)
                                            .prefix("Season "),
                                    );
                                    if dv.changed() {
                                        modal.season_input = modal.season_input.clamp(0, 99);
                                        modal.season_override = Some(modal.season_input);
                                    }
                                });
                                if let Some(cur) = &modal.current_match {
                                    if cur.media_type == "tv" {
                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            let lbl = match modal.season_override {
                                                Some(s) => format!("直接应用 Season {:02} 到当前条目（{}）", s, cur.title),
                                                None => format!("恢复自动识别季号并刷新（{}）", cur.title),
                                            };
                                            if primary_btn(ui, true, &lbl, pal.success) {
                                                chosen = Some((
                                                    modal.idx,
                                                    modal.query.clone(),
                                                    cur.clone(),
                                                    modal.season_override,
                                                ));
                                            }
                                        });
                                    }
                                }
                            });
                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut modal.query)
                                    .hint_text("输入中文名、英文名或原名...")
                                    .desired_width(470.0),
                            );
                            if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                || primary_btn(ui, !modal.searching, "搜索", pal.accent)
                            {
                                trigger_search = true;
                            }
                        });

                        if modal.searching {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("正在查询 TMDB...").color(pal.text_secondary));
                            });
                        }
                        if let Some(err) = &modal.error {
                            ui.add_space(6.0);
                            pill_badge(ui, err, pal.danger, pal.danger_soft);
                        }

                        if !modal.matches.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new("点击选择要应用的 TMDB 条目（将同时应用上方 Season 设置）：").color(pal.text_secondary).size(12.0));
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                for m in modal.matches.iter().take(10) {
                                    egui::Frame::none()
                                        .fill(pal.bg_subtle)
                                        .stroke(Stroke::new(1.0_f32, pal.border))
                                        .rounding(Rounding::same(8.0))
                                        .inner_margin(Margin::symmetric(10.0, 8.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let lbl = if m.media_type == "movie" { "电影" } else { "剧集" };
                                                pill_badge(ui, lbl, pal.accent, pal.accent_soft);
                                                ui.label(RichText::new(m.display_name()).strong().color(pal.text_primary));
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if primary_btn(ui, true, "选择", pal.success) {
                                                        chosen = Some((
                                                            modal.idx,
                                                            modal.query.clone(),
                                                            m.clone(),
                                                            modal.season_override,
                                                        ));
                                                    }
                                                    ui.label(
                                                        RichText::new(format!("ID: {}", m.tmdb_id))
                                                            .size(11.5)
                                                            .color(pal.text_secondary),
                                                    );
                                                });
                                            });
                                            if !m.overview.is_empty() {
                                                ui.add_space(2.0);
                                                ui.label(RichText::new(&m.overview).size(11.5).color(pal.text_secondary));
                                            }
                                        });
                                    ui.add_space(5.0);
                                }
                            });
                        }

                        ui.add_space(6.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("取消").clicked() {
                                close_modal = true;
                            }
                        });
                    });
            }

            if trigger_search {
                self.start_manual_search(ctx.clone());
            }
            if let Some((idx, q, c, s_ov)) = chosen {
                self.apply_manual_choice(idx, q, c, s_ov, ctx.clone());
            }
            if close_modal {
                self.manual_modal = None;
            }

            // Confirm Execution Modal
            if self.confirm_exec {
                let ready_plans = self.ready_count();
                let total_files: usize = self
                    .plans
                    .iter()
                    .filter(|p| p.is_ready())
                    .map(|p| p.items.len())
                    .sum();
                let mut do_exec = false;
                let mut cancel = false;
                egui::Window::new("确认执行重命名")
                    .collapsible(false)
                    .resizable(false)
                    .default_width(400.0)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!(
                            "即将整理 {} 个条目（共 {} 个文件），当前冲突数为 0。\n\n确定要开始执行吗？",
                            ready_plans, total_files
                        ));
                        ui.add_space(10.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if primary_btn(ui, true, "确定执行", pal.success) {
                                do_exec = true;
                            }
                            if ui.button("取消").clicked() {
                                cancel = true;
                            }
                        });
                    });
                if do_exec {
                    self.confirm_exec = false;
                    self.start_execute(ctx.clone());
                } else if cancel {
                    self.confirm_exec = false;
                }
            }

            // Info / Error Modal
            if let Some(title) = self.popup_title.clone() {
                let mut close = false;
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .default_width(420.0)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(&self.popup_body);
                        ui.add_space(10.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if primary_btn(ui, true, "确定", pal.accent) {
                                close = true;
                            }
                        });
                    });
                if close {
                    self.popup_title = None;
                }
            }
        }
    }
}
