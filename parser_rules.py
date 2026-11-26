"""Folder/filename parsing and collision-free rename plan builder."""

import os
import re
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

VIDEO_EXT = {".mkv", ".mp4", ".avi", ".ts", ".m2ts", ".mov", ".wmv", ".flv"}
SUB_EXT = {".ass", ".ssa", ".srt", ".vtt", ".sub", ".idx", ".sup"}
EXTRA_DIRS = {
    "cds", "scans", "fonts", "ncop&nced", "ncop", "pv", "menu", "menus",
    "previews", "music", "ost", "图集", "做饭", "变身&技能 合集", "舞蹈",
    "谈话", "sps", "sp", "extras", "特典映像", "总集篇", "广播剧", "特别动画",
    "影家剧场", "小菜一碟",
}
EXTRA_FILE_TAGS = ("[spmv]", "[mmr]", "[mystery camp]", "[tabisuru shima rin]", "nced", "ncop")
CN_NUM = {"一": 1, "二": 2, "三": 3, "四": 4, "五": 5, "六": 6, "七": 7, "八": 8, "九": 9, "十": 10, "十一": 11, "十二": 12}

INVALID_WIN_CHARS = r'<>:"/\|?*'


def sanitize_filename(name: str) -> str:
    """Replace Windows-forbidden characters while keeping readable punctuation."""
    trans = str.maketrans({":": "：", "/": " ", "\\": " ", "?": "？", "*": " ", '"': "'", "<": "(", ">": ")", "|": " "})
    cleaned = name.translate(trans)
    return re.sub(r"\s+", " ", cleaned).strip(" .")


def clean_query_name(folder_name: str) -> Tuple[str, Optional[str], str]:
    """Extract (clean_search_query, optional_year, guessed_type) from a messy folder name."""
    s = folder_name.strip()
    guessed = "auto"
    if any(k in s for k in ("剧场版", "电影版", "Movie", "movie", "The Movie")):
        guessed = "movie"

    yr_match = re.search(r"[\(\[（]((?:19|20)\d{2})[\)\]）]", s)
    year = yr_match.group(1) if yr_match else None

    # Strip leading [ReleaseGroup] tags
    s = re.sub(r"^(?:\[[^\]]+\]\s*)+", "", s)
    # Strip trailing [tags] / （tags） / (tags)
    s = re.sub(r"[\(（\[].*?(?:1080|2160|4K|BDRip|WEB-DL|HEVC|x265|x264|FLAC|字幕|全\d+集|Ma10p|Hi10p).*?[\)）\]]", "", s, flags=re.I)
    # Strip season markers like S2, 第一季, 剧场版
    s = re.sub(r"\s+(?:S\d{1,2}|第[一二三四五六七八九十\d]+季|Season\s*\d+)\b.*$", "", s, flags=re.I)
    s = re.sub(r"[\(\[（](?:19|20)\d{2}[\)\]）]", "", s)
    s = re.sub(r"\s+", " ", s).strip(" -_.")
    return s or folder_name, year, guessed


def split_media_ext(fname: str) -> Tuple[str, str]:
    """Return (stem, full_ext), preserving subtitle language tag like .sc.ass or .chs.srt."""
    base, ext = os.path.splitext(fname)
    el = ext.lower()
    if el in SUB_EXT:
        b2, lang = os.path.splitext(base)
        if lang and len(lang) <= 12 and re.match(r"^\.[A-Za-z0-9_-]+$", lang):
            return b2, lang + el
        return base, el
    return base, el


def detect_season(rel_parts: List[str], fname: str, root_folder_name: str = "") -> int:
    for part in list(rel_parts[:-1]) + ([root_folder_name] if root_folder_name else []):
        m = re.search(r"(?:Season|S)\s*0*(\d{1,2})\b", part, re.I)
        if m:
            return int(m.group(1))
        m = re.search(r"(\d)(?:st|nd|rd|th)\s*Season\b", part, re.I)
        if m:
            return int(m.group(1))
        m = re.search(r"第([一二三四五六七八九十\d]+)季", part)
        if m:
            val = m.group(1)
            return CN_NUM[val] if val in CN_NUM else int(val)
        if any(k in part.lower() for k in ("ova", "oad", "sp", "特别篇", "番外")):
            return 0
    m = re.search(r"S0*(\d{1,2})E\d{1,4}", fname, re.I)
    if m:
        return int(m.group(1))
    return 1


def extract_episode(fname: str) -> Optional[str]:
    m = re.search(r"S\d{1,2}E(\d{1,4}(?:\.\d(?!\d))?)", fname, re.I)
    if m:
        return m.group(1)
    m = re.search(r"\[(\d{1,4}(?:\.\d(?!\d))?)(?:v\d+)?\]", fname, re.I)
    if m:
        return m.group(1)
    m = re.search(r"\s-\s(\d{1,4}(?:\.\d(?!\d))?)(?:v\d+)?(?:\s|\[|\(|_|\.|$)", fname, re.I)
    if m:
        return m.group(1)
    m = re.search(r"第(\d{1,3})[话話集]", fname)
    if m:
        return m.group(1)
    m = re.search(r"\bEP?(\d{1,3})\b", fname, re.I)
    if m:
        return m.group(1)
    m = re.match(r"^(\d{1,3})(?:\D|$)", fname)
    if m:
        return m.group(1)
    return None


def fmt_episode(ep_str: str) -> str:
    if "." in ep_str:
        a, b = ep_str.split(".", 1)
        return f"{int(a):02d}.{b}"
    return f"{int(ep_str):02d}"


def is_extra_rel(rel_parts: List[str]) -> bool:
    if any(p.lower() in EXTRA_DIRS or "[fonts]" in p.lower() for p in rel_parts[:-1]):
        return True
    fn_l = rel_parts[-1].lower()
    return any(tag in fn_l for tag in EXTRA_FILE_TAGS)


def find_collisions(items: List[RenameItem]) -> Dict[str, List[str]]:
    """Return {dst_path: [src_paths...]} for any destination targeted by multiple sources."""
    seen: Dict[str, List[str]] = {}
    display_dst: Dict[str, str] = {}
    for it in items:
        k = os.path.normcase(it.dst)
        seen.setdefault(k, []).append(it.src)
        display_dst.setdefault(k, it.dst)
    return {display_dst[k]: srcs for k, srcs in seen.items() if len(srcs) > 1}


def extract_part_tag(fname: str) -> str:
    """Preserve multi-part episode tags like 'Part1' / 'Part 2' to avoid collapsing split episodes."""
    m = re.search(r"(?:^|[\s._-])Part\s*0*(\d+)\b", fname, re.I)
    return f" - Part{m.group(1)}" if m else ""


@dataclass
class RenameItem:
    src: str
    dst: str
    kind: str  # "main" or "extra"


@dataclass
class FolderPlan:
    src_dir: str
    folder_name: str
    query: str
    year_hint: Optional[str]
    media_type: str  # "tv" or "movie"
    display_title: str
    tmdb_id: int = 0
    items: List[RenameItem] = field(default_factory=list)
    collisions: List[str] = field(default_factory=list)
    status: str = "待匹配"


def build_folder_items(
    src_dir: str,
    target_parent_dir: str,
    target_folder_name: str,
    media_type: str,
    archive_extras: bool,
    extras_root_dir: str,
    episode_titles: Optional[Dict[Tuple[int, int], str]] = None,
) -> Tuple[List[RenameItem], List[str]]:
    """Build collision-checked RenameItem list for a single show/movie folder."""
    safe_folder = sanitize_filename(target_folder_name)
    base_title = re.sub(r"\s*\(\d{4}\)$", "", safe_folder).strip()
    items: List[RenameItem] = []

    src_folder_name = os.path.basename(os.path.normpath(src_dir))
    raw_tv_entries: List[Tuple[str, int, str, str, str]] = []

    for r, _dirs, files in os.walk(src_dir):
        for fname in sorted(files):
            if fname.startswith("."):
                continue
            src_path = os.path.join(r, fname)
            rel = os.path.relpath(src_path, src_dir)
            parts = rel.replace("\\", "/").split("/")
            ext_l = os.path.splitext(fname)[1].lower()
            _, full_ext = split_media_ext(fname)

            if is_extra_rel(parts) or (ext_l not in VIDEO_EXT and ext_l not in SUB_EXT):
                if archive_extras and extras_root_dir:
                    dst_path = os.path.join(extras_root_dir, safe_folder, rel)
                    items.append(RenameItem(src=src_path, dst=dst_path, kind="extra"))
                continue

            if media_type == "movie":
                dst_path = os.path.join(target_parent_dir, safe_folder, f"{safe_folder}{full_ext}")
                items.append(RenameItem(src=src_path, dst=dst_path, kind="main"))
            else:
                s = detect_season(parts, fname, root_folder_name=src_folder_name)
                ep = extract_episode(fname)
                if ep is None:
                    if archive_extras and extras_root_dir:
                        dst_path = os.path.join(extras_root_dir, safe_folder, rel)
                        items.append(RenameItem(src=src_path, dst=dst_path, kind="extra"))
                    continue
                if "." in ep:
                    s = 0
                part_tag = extract_part_tag(fname)
                raw_tv_entries.append((src_path, s, ep, part_tag, full_ext))

    # Normalize continuous episode numbering across seasons (e.g. S2 starting at ep 13 after S1 1..12)
    if raw_tv_entries:
        season_eps: Dict[int, List[int]] = {}
        for _, s, ep, _, _ in raw_tv_entries:
            if s >= 1 and "." not in ep:
                season_eps.setdefault(s, []).append(int(ep))
        offsets: Dict[int, int] = {}
        prev_max = 0
        for s in sorted(season_eps):
            mn, mx = min(season_eps[s]), max(season_eps[s])
            if s > 1 and prev_max > 0 and mn == prev_max + 1:
                offsets[s] = prev_max
            prev_max = mx
        for src_path, s, ep, part_tag, full_ext in raw_tv_entries:
            if s in offsets and "." not in ep:
                ep_num = int(ep) - offsets[s]
                ep_fmt = f"{ep_num:02d}"
            else:
                ep_num = int(ep) if "." not in ep else None
                ep_fmt = fmt_episode(ep)
            title_suffix = ""
            if episode_titles and ep_num is not None:
                raw_t = episode_titles.get((s, ep_num), "")
                if raw_t:
                    title_suffix = f" [{sanitize_filename(raw_t)}]"
            dst_path = os.path.join(
                target_parent_dir,
                safe_folder,
                f"Season {s:02d}",
                f"{base_title} S{s:02d}E{ep_fmt}{part_tag}{title_suffix}{full_ext}",
            )
            items.append(RenameItem(src=src_path, dst=dst_path, kind="main"))

    collisions = list(find_collisions(items).keys())
    # Put main episodes/movies before extras so previews always show main media first
    items.sort(key=lambda x: (0 if x.kind == "main" else 1, x.dst))
    return items, collisions


def attach_episode_titles(
    items: List[RenameItem],
    season_fetcher,
    tmdb_id: int,
) -> Tuple[List[RenameItem], List[str]]:
    """Append ' [TMDB Episode Title]' after SxxExx on main TV items and re-check collisions."""
    if not tmdb_id:
        return items, []
    seasons_needed = set()
    parsed = []
    for it in items:
        if it.kind == "main":
            bn = os.path.basename(it.dst)
            m = re.search(r"(S(\d{2})E(\d{2,4})(?:\.\d)?(?:\s*-\s*Part\d+)?)", bn)
            if m:
                s_num = int(m.group(2))
                ep_num = int(m.group(3))
                seasons_needed.add(s_num)
                parsed.append((it, m.group(1), s_num, ep_num))
                continue
        parsed.append((it, None, None, None))

    ep_map: Dict[Tuple[int, int], str] = {}
    for s_num in sorted(seasons_needed):
        for ep_num, title in season_fetcher(tmdb_id, s_num).items():
            ep_map[(s_num, ep_num)] = title

    out: List[RenameItem] = []
    for it, se_tok, s_num, ep_num in parsed:
        if se_tok and s_num is not None and ep_num is not None:
            raw_t = ep_map.get((s_num, ep_num), "")
            if raw_t:
                safe_t = sanitize_filename(raw_t)
                d_dir, bn = os.path.split(it.dst)
                # Insert " [title]" right after SxxExx (and before any extension/language suffix)
                new_bn = bn.replace(se_tok, f"{se_tok} [{safe_t}]", 1)
                out.append(RenameItem(src=it.src, dst=os.path.join(d_dir, new_bn), kind=it.kind))
                continue
        out.append(it)

    return out, list(find_collisions(out).keys())


def attach_season_names(
    items: List[RenameItem],
    seasons_fetcher,
    tmdb_id: int,
) -> Tuple[List[RenameItem], List[str]]:
    """Append '（TMDB Season Name）' to Season XX folder when TMDB has a non-generic name."""
    if not tmdb_id:
        return items, []
    season_names = seasons_fetcher(tmdb_id)
    if not season_names:
        return items, []

    out: List[RenameItem] = []
    for it in items:
        if it.kind == "main":
            d_dir, bn = os.path.split(it.dst)
            parent_dir, season_folder = os.path.split(d_dir)
            m = re.match(r"^Season (\d{2})$", season_folder)
            if m:
                s_num = int(m.group(1))
                raw_name = (season_names.get(s_num) or "").strip()
                is_generic = bool(re.match(r"^(?:第\s*\d+\s*季|Season\s*\d+)$", raw_name, re.I))
                if raw_name and not is_generic:
                    safe_name = sanitize_filename(raw_name)
                    new_season_folder = f"{season_folder}（{safe_name}）"
                    out.append(RenameItem(src=it.src, dst=os.path.join(parent_dir, new_season_folder, bn), kind=it.kind))
                    continue
        out.append(it)

    return out, list(find_collisions(out).keys())
