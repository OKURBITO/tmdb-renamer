"""TMDB Media Renamer GUI application (tkinter)."""

import json
import os
import threading
import tkinter as tk
from tkinter import filedialog, messagebox, simpledialog, ttk
from typing import Dict, List

from parser_rules import FolderPlan, attach_episode_titles, attach_season_names, build_folder_items, clean_query_name, find_collisions
from tmdb_client import TMDBClient, TMDBMatch

CONFIG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json")


class TMDBRenamerApp(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("TMDB 影视媒体库智能重命名工具")
        self.geometry("1120x740")
        self.minsize(960, 640)

        self.plans: List[FolderPlan] = []
        self.candidates_map: Dict[int, List[TMDBMatch]] = {}

        self._load_config()
        self._build_ui()

    def _load_config(self):
        self.cfg = {
            "api_key": "",
            "proxy": "",
            "media_dir": "",
            "extras_dir": "",
            "archive_extras": True,
            "append_ep_title": False,
            "append_season_name": False,
            "scan_mode": "batch",
            "media_type": "auto",
        }
        if os.path.exists(CONFIG_PATH):
            try:
                with open(CONFIG_PATH, "r", encoding="utf-8") as f:
                    self.cfg.update(json.load(f))
            except Exception:
                pass

    def _save_config(self):
        data = {
            "api_key": self.var_api_key.get().strip(),
            "proxy": self.var_proxy.get().strip(),
            "media_dir": self.var_media_dir.get().strip(),
            "extras_dir": self.var_extras_dir.get().strip(),
            "archive_extras": self.var_archive_extras.get(),
            "append_ep_title": self.var_append_ep_title.get(),
            "append_season_name": self.var_append_season_name.get(),
            "scan_mode": self.var_scan_mode.get(),
            "media_type": self.var_media_type.get(),
        }
        try:
            with open(CONFIG_PATH, "w", encoding="utf-8") as f:
                json.dump(data, f, ensure_ascii=False, indent=2)
        except Exception:
            pass

    def _build_ui(self):
        pad = {"padx": 8, "pady": 4}

        # --- Top Config Frame ---
        cfg_frame = ttk.LabelFrame(self, text="1. TMDB 与目录配置", padding=8)
        cfg_frame.pack(fill="x", **pad)

        # Row 0: API Key & Proxy
        ttk.Label(cfg_frame, text="TMDB API Key / Token:").grid(row=0, column=0, sticky="w")
        self.var_api_key = tk.StringVar(value=self.cfg["api_key"])
        ttk.Entry(cfg_frame, textvariable=self.var_api_key, width=42, show="*").grid(row=0, column=1, sticky="we", padx=4)
        ttk.Button(cfg_frame, text="测试连接", command=self._on_test_conn).grid(row=0, column=2, padx=4)

        ttk.Label(cfg_frame, text="代理 (可选):").grid(row=0, column=3, sticky="w", padx=(10, 0))
        self.var_proxy = tk.StringVar(value=self.cfg["proxy"])
        ttk.Entry(cfg_frame, textvariable=self.var_proxy, width=24).grid(row=0, column=4, sticky="w", padx=4)

        # Row 1: Target Folder
        ttk.Label(cfg_frame, text="待整理文件夹:").grid(row=1, column=0, sticky="w", pady=(6, 0))
        self.var_media_dir = tk.StringVar(value=self.cfg["media_dir"])
        ttk.Entry(cfg_frame, textvariable=self.var_media_dir).grid(row=1, column=1, columnspan=3, sticky="we", padx=4, pady=(6, 0))
        ttk.Button(cfg_frame, text="选择文件夹...", command=self._pick_media_dir).grid(row=1, column=4, sticky="w", padx=4, pady=(6, 0))

        # Row 2: Scan Mode & Media Type
        opts = ttk.Frame(cfg_frame)
        opts.grid(row=2, column=0, columnspan=5, sticky="we", pady=(6, 0))

        ttk.Label(opts, text="扫描模式:").pack(side="left")
        self.var_scan_mode = tk.StringVar(value=self.cfg["scan_mode"])
        ttk.Radiobutton(opts, text="批量（每个子文件夹为一部作品）", variable=self.var_scan_mode, value="batch").pack(side="left", padx=6)
        ttk.Radiobutton(opts, text="单部（所选文件夹本身为一部作品）", variable=self.var_scan_mode, value="single").pack(side="left", padx=6)

        ttk.Label(opts, text="   类型:").pack(side="left", padx=(12, 2))
        self.var_media_type = tk.StringVar(value=self.cfg["media_type"])
        ttk.Combobox(opts, textvariable=self.var_media_type, values=["auto", "tv", "movie"], width=8, state="readonly").pack(side="left")

        # Row 3: Optional Naming Enhancements
        opt_row2 = ttk.Frame(cfg_frame)
        opt_row2.grid(row=3, column=0, columnspan=5, sticky="we", pady=(6, 0))
        self.var_append_season_name = tk.BooleanVar(value=self.cfg.get("append_season_name", False))
        ttk.Checkbutton(opt_row2, text="Season 目录后追加 TMDB 季名（如 Season 01（化物语））", variable=self.var_append_season_name).pack(side="left")

        self.var_append_ep_title = tk.BooleanVar(value=self.cfg.get("append_ep_title", False))
        ttk.Checkbutton(opt_row2, text="SxxExx 后追加 TMDB 单集标题（如 [黑仪重蟹 其之一]）", variable=self.var_append_ep_title).pack(side="left", padx=(16, 0))

        # Row 4: Extras Archive
        ext_row = ttk.Frame(cfg_frame)
        ext_row.grid(row=4, column=0, columnspan=5, sticky="we", pady=(6, 0))
        self.var_archive_extras = tk.BooleanVar(value=self.cfg["archive_extras"])
        ttk.Checkbutton(ext_row, text="归档非正片特典 (CDs/PV/NCOP/menu/Scans) 到:", variable=self.var_archive_extras).pack(side="left")
        self.var_extras_dir = tk.StringVar(value=self.cfg["extras_dir"])
        ttk.Entry(ext_row, textvariable=self.var_extras_dir, width=48).pack(side="left", padx=6, fill="x", expand=True)
        ttk.Button(ext_row, text="选择...", command=self._pick_extras_dir).pack(side="left")

        cfg_frame.columnconfigure(1, weight=1)

        # --- Action Bar ---
        act_frame = ttk.Frame(self)
        act_frame.pack(fill="x", **pad)
        self.btn_scan = ttk.Button(act_frame, text="🔍 扫描并匹配 TMDB（预览）", command=self._on_scan)
        self.btn_scan.pack(side="left")
        ttk.Button(act_frame, text="✏️ 手动修改选中项 TMDB 匹配", command=self._on_edit_selected).pack(side="left", padx=8)
        self.btn_conflicts = ttk.Button(act_frame, text="⚠️ 查看冲突详情", command=self._on_show_conflicts, state="disabled")
        self.btn_conflicts.pack(side="left", padx=4)
        self.btn_exec = ttk.Button(act_frame, text="✅ 执行重命名", command=self._on_execute, state="disabled")
        self.btn_exec.pack(side="right")

        self.var_status = tk.StringVar(value="请填入 TMDB API Key 并选择待整理文件夹")
        ttk.Label(act_frame, textvariable=self.var_status, foreground="#005a9e").pack(side="left", padx=12)

        # --- Paned Window: Folder Matches + File Details ---
        paned = ttk.PanedWindow(self, orient="vertical")
        paned.pack(fill="both", expand=True, **pad)

        # Top tree: Folders
        top_box = ttk.LabelFrame(paned, text="2. 识别结果汇总（双击某行可重新搜索/指定 TMDB 剧名）", padding=4)
        cols = ("orig", "target", "seasons", "type", "tmdb_id", "files", "status")
        self.tree_folders = ttk.Treeview(top_box, columns=cols, show="headings", height=10)
        headers = {
            "orig": ("原文件夹名", 230),
            "target": ("重命名后主目录 (剧名/片名 + 年份)", 230),
            "seasons": ("内部季目录", 140),
            "type": ("类型", 60),
            "tmdb_id": ("TMDB ID", 80),
            "files": ("文件数(正片/特典)", 120),
            "status": ("状态", 170),
        }
        for c, (text, w) in headers.items():
            self.tree_folders.heading(c, text=text)
            self.tree_folders.column(c, width=w, anchor="w")
        sb1 = ttk.Scrollbar(top_box, orient="vertical", command=self.tree_folders.yview)
        self.tree_folders.configure(yscrollcommand=sb1.set)
        self.tree_folders.pack(side="left", fill="both", expand=True)
        sb1.pack(side="right", fill="y")
        self.tree_folders.tag_configure("conflict", foreground="#b00020", background="#fdecea")
        self.tree_folders.bind("<<TreeviewSelect>>", self._on_folder_select)
        self.tree_folders.bind("<Double-1>", lambda _e: self._on_edit_selected())
        paned.add(top_box, weight=3)

        # Bottom tree: File-level preview
        bot_box = ttk.LabelFrame(paned, text="3. 选中作品的文件级重命名预览（改名前 → 改名后；冲突项置顶标红）", padding=4)
        fcols = ("kind", "src", "dst")
        self.tree_files = ttk.Treeview(bot_box, columns=fcols, show="headings", height=10)
        self.tree_files.tag_configure("conflict", foreground="#b00020", background="#fdecea")
        self.tree_files.heading("kind", text="分类")
        self.tree_files.column("kind", width=70, anchor="center")
        self.tree_files.heading("src", text="改名前路径")
        self.tree_files.column("src", width=460, anchor="w")
        self.tree_files.heading("dst", text="改名后路径")
        self.tree_files.column("dst", width=460, anchor="w")
        sb2 = ttk.Scrollbar(bot_box, orient="vertical", command=self.tree_files.yview)
        self.tree_files.configure(yscrollcommand=sb2.set)
        self.tree_files.pack(side="left", fill="both", expand=True)
        sb2.pack(side="right", fill="y")
        paned.add(bot_box, weight=3)

    def _pick_media_dir(self):
        p = filedialog.askdirectory(title="选择待整理文件夹")
        if p:
            self.var_media_dir.set(p)
            if not self.var_extras_dir.get().strip():
                self.var_extras_dir.set(os.path.join(os.path.dirname(p), "特典归档"))

    def _pick_extras_dir(self):
        p = filedialog.askdirectory(title="选择特典归档目录")
        if p:
            self.var_extras_dir.set(p)

    def _get_client(self) -> TMDBClient:
        key = self.var_api_key.get().strip()
        if not key:
            raise ValueError("请先输入 TMDB API Key 或 Read Access Token。")
        return TMDBClient(key, proxy=self.var_proxy.get().strip())

    def _on_test_conn(self):
        self._save_config()
        try:
            client = self._get_client()
        except ValueError as e:
            messagebox.showwarning("提示", str(e))
            return

        self.var_status.set("正在测试 TMDB 连接...")
        def worker():
            try:
                ok = client.test_connection()
                self.after(0, lambda: self._finish_test(ok, ""))
            except Exception as ex:
                self.after(0, lambda: self._finish_test(False, str(ex)))
        threading.Thread(target=worker, daemon=True).start()

    def _finish_test(self, ok: bool, err: str):
        if ok:
            self.var_status.set("TMDB 连接成功！")
            messagebox.showinfo("成功", "TMDB API 连接正常！")
        else:
            self.var_status.set("TMDB 连接失败")
            messagebox.showerror("连接失败", f"无法连接 TMDB：\n{err}\n\n如在国内网络环境，可填写本地 HTTP 代理后重试。")

    def _on_scan(self):
        self._save_config()
        media_dir = self.var_media_dir.get().strip()
        if not media_dir or not os.path.isdir(media_dir):
            messagebox.showwarning("提示", "请选择有效的待整理文件夹。")
            return
        try:
            client = self._get_client()
        except ValueError as e:
            messagebox.showwarning("提示", str(e))
            return

        self.btn_scan.config(state="disabled")
        self.btn_exec.config(state="disabled")
        self.var_status.set("正在扫描目录并查询 TMDB...")
        self.tree_folders.delete(*self.tree_folders.get_children())
        self.tree_files.delete(*self.tree_files.get_children())

        mode = self.var_scan_mode.get()
        mtype_pref = self.var_media_type.get()
        archive_extras = self.var_archive_extras.get()
        append_ep_title = self.var_append_ep_title.get()
        append_season_name = self.var_append_season_name.get()
        extras_dir = self.var_extras_dir.get().strip()

        def worker():
            plans: List[FolderPlan] = []
            cand_map: Dict[int, List[TMDBMatch]] = {}
            targets = [media_dir] if mode == "single" else [
                os.path.join(media_dir, d)
                for d in sorted(os.listdir(media_dir))
                if os.path.isdir(os.path.join(media_dir, d))
            ]

            for idx, src_dir in enumerate(targets):
                fname = os.path.basename(src_dir)
                query, yr, guessed = clean_query_name(fname)
                qtype = guessed if mtype_pref == "auto" else mtype_pref
                parent_dir = os.path.dirname(src_dir)

                try:
                    matches = client.search(query, media_type=qtype, year=yr)
                    if not matches and yr:
                        matches = client.search(query, media_type=qtype, year=None)
                    if not matches and qtype != "auto":
                        matches = client.search(query, media_type="auto", year=None)
                except Exception as ex:
                    fp = FolderPlan(src_dir=src_dir, folder_name=fname, query=query, year_hint=yr, media_type="tv", display_title=fname, status=f"请求出错: {ex}")
                    plans.append(fp)
                    continue

                cand_map[idx] = matches
                if matches:
                    best = matches[0]
                    display = best.display_name
                    items, cols = build_folder_items(src_dir, parent_dir, display, best.media_type, archive_extras, extras_dir)
                    status = f"⚠️ 命名冲突 ({len(cols)})" if cols else "就绪"
                    fp = FolderPlan(
                        src_dir=src_dir, folder_name=fname, query=query, year_hint=yr,
                        media_type=best.media_type, display_title=display, tmdb_id=best.tmdb_id,
                        items=items, collisions=cols, status=status
                    )
                else:
                    fp = FolderPlan(
                        src_dir=src_dir, folder_name=fname, query=query, year_hint=yr,
                        media_type="tv", display_title=fname, status="未匹配（双击手搜）"
                    )
                plans.append(fp)
                self.after(0, lambda i=idx+1, n=len(targets): self.var_status.set(f"正在匹配 TMDB: {i}/{n}..."))

            # Auto-merge batch rows that matched the same TMDB TV show (e.g. S1 + S2 subfolders)
            merged_plans: List[FolderPlan] = []
            tv_groups: Dict[int, List[FolderPlan]] = {}
            for fp in plans:
                if fp.tmdb_id and fp.media_type == "tv":
                    tv_groups.setdefault(fp.tmdb_id, []).append(fp)
                else:
                    merged_plans.append(fp)

            for tid, grp in tv_groups.items():
                if len(grp) == 1:
                    merged_plans.append(grp[0])
                    continue
                first = grp[0]
                parent_dir = os.path.dirname(first.src_dir)
                combined_items = []
                for g in grp:
                    combined_items.extend(g.items)
                # Normalize continuous episode numbers across merged seasons
                from parser_rules import RenameItem
                season_eps: Dict[int, List[int]] = {}
                parsed_rows = []
                for it in combined_items:
                    if it.kind == "main":
                        m = os.path.basename(it.dst)
                        import re as _re
                        sm = _re.search(r"S(\d{2})E(\d{2,4})", m)
                        if sm:
                            s_num, e_num = int(sm.group(1)), int(sm.group(2))
                            if s_num >= 1:
                                season_eps.setdefault(s_num, []).append(e_num)
                            parsed_rows.append((it, s_num, e_num))
                            continue
                    parsed_rows.append((it, None, None))
                offsets: Dict[int, int] = {}
                prev_max = 0
                for s_num in sorted(season_eps):
                    mn, mx = min(season_eps[s_num]), max(season_eps[s_num])
                    if s_num > 1 and prev_max > 0 and mn == prev_max + 1:
                        offsets[s_num] = prev_max
                    prev_max = mx
                final_items: List[RenameItem] = []
                for it, s_num, e_num in parsed_rows:
                    if s_num in offsets and e_num is not None:
                        new_e = f"{e_num - offsets[s_num]:02d}"
                        new_dst = _re.sub(rf"S{s_num:02d}E\d{{2,4}}", f"S{s_num:02d}E{new_e}", it.dst)
                        final_items.append(RenameItem(src=it.src, dst=new_dst, kind=it.kind))
                    else:
                        final_items.append(it)
                seen_dst: Dict[str, int] = {}
                for it in final_items:
                    k = os.path.normcase(it.dst)
                    seen_dst[k] = seen_dst.get(k, 0) + 1
                cols = [k for k, v in seen_dst.items() if v > 1]
                merged_plans.append(FolderPlan(
                    src_dir=parent_dir,
                    folder_name=" + ".join(g.folder_name for g in grp),
                    query=first.query,
                    year_hint=first.year_hint,
                    media_type="tv",
                    display_title=first.display_title,
                    tmdb_id=tid,
                    items=final_items,
                    collisions=cols,
                    status=f"⚠️ 命名冲突 ({len(cols)})" if cols else "就绪 (已自动合并多季)",
                ))

            # If batch mode was used on a single show's root folder (all subfolders merged into 1 TV show),
            # lift target_parent_dir to os.path.dirname(media_dir) so it renames the show folder itself
            # (e.g. 临时/[VCB-Studio] 胆大党 -> 临时/胆大党 (2024)/Season 01 & Season 02).
            if mode == "batch" and len(merged_plans) == 1 and merged_plans[0].src_dir == media_dir and merged_plans[0].tmdb_id:
                only = merged_plans[0]
                lifted_items, lifted_cols = build_folder_items(
                    media_dir,
                    os.path.dirname(media_dir),
                    only.display_title,
                    only.media_type,
                    archive_extras,
                    extras_dir,
                )
                only.folder_name = os.path.basename(media_dir)
                only.items = lifted_items
                only.collisions = lifted_cols
                only.status = f"⚠️ 命名冲突 ({len(lifted_cols)})" if lifted_cols else "就绪 (已自动合并多季)"

            if append_ep_title or append_season_name:
                for mp in merged_plans:
                    if mp.tmdb_id and mp.media_type == "tv" and mp.items:
                        if append_ep_title:
                            mp.items, c1 = attach_episode_titles(mp.items, client.get_season_episodes, mp.tmdb_id)
                            mp.collisions = c1
                        if append_season_name:
                            mp.items, c2 = attach_season_names(mp.items, client.get_seasons_names, mp.tmdb_id)
                            mp.collisions = c2
                        if mp.collisions:
                            mp.status = f"⚠️ 命名冲突 ({len(mp.collisions)})"

            self.after(0, lambda: self._populate_plans(merged_plans, cand_map))

        threading.Thread(target=worker, daemon=True).start()

    def _populate_plans(self, plans: List[FolderPlan], cand_map: Dict[int, List[TMDBMatch]]):
        self.plans = plans
        self.candidates_map = cand_map
        self.tree_folders.delete(*self.tree_folders.get_children())
        for idx, p in enumerate(plans):
            m_cnt = sum(1 for x in p.items if x.kind == "main")
            e_cnt = sum(1 for x in p.items if x.kind == "extra")
            seasons = sorted({
                os.path.basename(os.path.dirname(x.dst))
                for x in p.items
                if x.kind == "main" and os.path.basename(os.path.dirname(x.dst)).startswith("Season ")
            })
            seasons_disp = ", ".join(seasons) if seasons else "-"
            row_tags = ("conflict",) if p.collisions else ()
            self.tree_folders.insert("", "end", iid=str(idx), tags=row_tags, values=(
                p.folder_name, p.display_title, seasons_disp, p.media_type, p.tmdb_id or "-", f"{m_cnt} / {e_cnt}", p.status
            ))
        if plans:
            self.tree_folders.selection_set("0")
            self._on_folder_select()
        self.btn_scan.config(state="normal")
        ready = sum(1 for p in plans if p.items and not p.collisions)
        self.btn_exec.config(state="normal" if ready > 0 else "disabled")
        self.var_status.set(f"扫描完成：共 {len(plans)} 个目录，{ready} 个就绪可重命名")

    def _on_folder_select(self, _event=None):
        sel = self.tree_folders.selection()
        if not sel:
            self.btn_conflicts.config(state="disabled")
            return
        idx = int(sel[0])
        p = self.plans[idx]
        conflict_map = find_collisions(p.items)
        conflict_keys = {os.path.normcase(k) for k in conflict_map}
        self.btn_conflicts.config(state="normal" if conflict_map else "disabled")

        self.tree_files.delete(*self.tree_files.get_children())
        conflict_items = [it for it in p.items if os.path.normcase(it.dst) in conflict_keys]
        mains = [it for it in p.items if it.kind == "main" and os.path.normcase(it.dst) not in conflict_keys]
        extras = [it for it in p.items if it.kind == "extra" and os.path.normcase(it.dst) not in conflict_keys][:200]
        for it in conflict_items + mains + extras:
            rel_src = os.path.relpath(it.src, os.path.dirname(p.src_dir))
            rel_dst = os.path.relpath(it.dst, os.path.dirname(p.src_dir)) if it.kind == "main" else it.dst
            is_conf = os.path.normcase(it.dst) in conflict_keys
            tag = "⚠️ 冲突" if is_conf else ("正片/字幕" if it.kind == "main" else "特典归档")
            row_tags = ("conflict",) if is_conf else ()
            self.tree_files.insert("", "end", tags=row_tags, values=(tag, rel_src, rel_dst))

    def _on_show_conflicts(self):
        sel = self.tree_folders.selection()
        if not sel:
            return
        p = self.plans[int(sel[0])]
        conflict_map = find_collisions(p.items)
        if not conflict_map:
            messagebox.showinfo("冲突详情", "当前选中作品没有命名冲突。")
            return

        win = tk.Toplevel(self)
        win.title(f"命名冲突详情 - {p.display_title}（共 {len(conflict_map)} 处冲突）")
        win.geometry("860x520")
        win.transient(self)

        lines = [f"作品目录：{p.src_dir}", f"冲突目标数：{len(conflict_map)}", "=" * 72, ""]
        base_dir = os.path.dirname(p.src_dir)
        for i, (dst_path, srcs) in enumerate(sorted(conflict_map.items()), 1):
            rel_dst = os.path.relpath(dst_path, base_dir)
            lines.append(f"[冲突 #{i}] 目标路径相同：{rel_dst}")
            for j, sp in enumerate(srcs, 1):
                rel_src = os.path.relpath(sp, base_dir)
                lines.append(f"  来源 {j}: {rel_src}")
            lines.append("")
        content = "\n".join(lines)

        txt = tk.Text(win, wrap="none", font=("Consolas", 10))
        ysb = ttk.Scrollbar(win, orient="vertical", command=txt.yview)
        xsb = ttk.Scrollbar(win, orient="horizontal", command=txt.xview)
        txt.configure(yscrollcommand=ysb.set, xscrollcommand=xsb.set)
        txt.insert("1.0", content)
        txt.config(state="disabled")

        btn_bar = ttk.Frame(win, padding=6)
        btn_bar.pack(side="bottom", fill="x")
        def copy_text():
            self.clipboard_clear()
            self.clipboard_append(content)
            messagebox.showinfo("已复制", "冲突详情已复制到剪贴板。", parent=win)
        ttk.Button(btn_bar, text="📋 复制全部冲突详情", command=copy_text).pack(side="left")
        ttk.Button(btn_bar, text="关闭", command=win.destroy).pack(side="right")

        ysb.pack(side="right", fill="y")
        xsb.pack(side="bottom", fill="x")
        txt.pack(side="left", fill="both", expand=True)

    def _on_edit_selected(self):
        sel = self.tree_folders.selection()
        if not sel:
            messagebox.showinfo("提示", "请先在列表中选中一行。")
            return
        idx = int(sel[0])
        p = self.plans[idx]
        new_q = simpledialog.askstring("手动搜索 TMDB", "输入用于搜索 TMDB 的剧名/片名：", initialvalue=p.query, parent=self)
        if not new_q:
            return
        try:
            client = self._get_client()
            matches = client.search(new_q, media_type=self.var_media_type.get())
        except Exception as ex:
            messagebox.showerror("错误", f"搜索失败：{ex}")
            return
        if not matches:
            messagebox.showinfo("无结果", f"TMDB 未找到：{new_q}")
            return

        # Simple picker dialog if multiple matches
        choice = matches[0]
        if len(matches) > 1:
            msg = "\n".join(f"{i+1}. [{m.media_type}] {m.display_name} (ID:{m.tmdb_id})" for i, m in enumerate(matches[:8]))
            num = simpledialog.askinteger("选择 TMDB 条目", f"请输入序号 (1-{min(8, len(matches))})：\n\n{msg}", initialvalue=1, minvalue=1, maxvalue=min(8, len(matches)), parent=self)
            if not num:
                return
            choice = matches[num - 1]

        items, cols = build_folder_items(
            p.src_dir, os.path.dirname(p.src_dir), choice.display_name, choice.media_type,
            self.var_archive_extras.get(), self.var_extras_dir.get().strip()
        )
        if self.var_append_ep_title.get() and choice.media_type == "tv" and items:
            items, cols = attach_episode_titles(items, client.get_season_episodes, choice.tmdb_id)
        if self.var_append_season_name.get() and choice.media_type == "tv" and items:
            items, cols = attach_season_names(items, client.get_seasons_names, choice.tmdb_id)
        p.query = new_q
        p.media_type = choice.media_type
        p.display_title = choice.display_name
        p.tmdb_id = choice.tmdb_id
        p.items = items
        p.collisions = cols
        p.status = f"⚠️ 命名冲突 ({len(cols)})" if cols else "就绪"

        m_cnt = sum(1 for x in p.items if x.kind == "main")
        e_cnt = sum(1 for x in p.items if x.kind == "extra")
        seasons = sorted({
            os.path.basename(os.path.dirname(x.dst))
            for x in p.items
            if x.kind == "main" and os.path.basename(os.path.dirname(x.dst)).startswith("Season ")
        })
        seasons_disp = ", ".join(seasons) if seasons else "-"
        row_tags = ("conflict",) if p.collisions else ()
        self.tree_folders.item(str(idx), tags=row_tags, values=(
            p.folder_name, p.display_title, seasons_disp, p.media_type, p.tmdb_id, f"{m_cnt} / {e_cnt}", p.status
        ))
        self._on_folder_select()
        ready = sum(1 for x in self.plans if x.items and not x.collisions)
        self.btn_exec.config(state="normal" if ready > 0 else "disabled")

    def _on_execute(self):
        ready_plans = [p for p in self.plans if p.items and not p.collisions]
        if not ready_plans:
            return
        total_files = sum(len(p.items) for p in ready_plans)
        if not messagebox.askyesno(
            "确认执行",
            f"即将重命名/整理 {len(ready_plans)} 个目录（共 {total_files} 个文件），冲突数为 0。\n\n确定要开始执行吗？"
        ):
            return

        moved = 0
        errors = []
        for p in ready_plans:
            for it in p.items:
                try:
                    if it.src == it.dst:
                        continue
                    if os.path.exists(it.dst):
                        errors.append(f"目标已存在: {it.dst}")
                        continue
                    os.makedirs(os.path.dirname(it.dst), exist_ok=True)
                    os.rename(it.src, it.dst)
                    moved += 1
                except Exception as ex:
                    errors.append(f"{it.src} -> {ex}")
            # Prune empty dirs inside src_dir
            for r, dirs, _ in os.walk(p.src_dir, topdown=False):
                for d in dirs:
                    dp = os.path.join(r, d)
                    try:
                        if not os.listdir(dp):
                            os.rmdir(dp)
                    except OSError:
                        pass
            try:
                if os.path.isdir(p.src_dir) and not os.listdir(p.src_dir):
                    os.rmdir(p.src_dir)
            except OSError:
                pass
            p.status = "已完成"

        self._populate_plans(self.plans, self.candidates_map)
        if errors:
            messagebox.showwarning("部分完成", f"成功移动 {moved} 个文件，发生 {len(errors)} 个错误：\n" + "\n".join(errors[:5]))
        else:
            messagebox.showinfo("全部完成", f"重命名完成！共整理 {moved} 个文件。")


if __name__ == "__main__":
    app = TMDBRenamerApp()
    app.mainloop()
