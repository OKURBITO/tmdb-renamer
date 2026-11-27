use regex::Regex;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::WalkDir;

const VIDEO_EXT: &[&str] = &[".mkv", ".mp4", ".avi", ".ts", ".m2ts", ".mov", ".wmv", ".flv"];
const SUB_EXT: &[&str] = &[".ass", ".ssa", ".srt", ".vtt", ".sub", ".idx", ".sup"];

const EXTRA_DIRS: &[&str] = &[
    "cds", "scans", "fonts", "ncop&nced", "ncop", "nced", "pv", "pvs",
    "menu", "menus", "previews", "preview", "music", "ost", "图集",
    "sps", "sp", "extras", "extra",
    "特典映像", "特典", "总集篇", "广播剧", "特别动画",
    "sample", "samples", "trailers", "trailer", "featurettes",
];

const EXTRA_FILE_TAGS: &[&str] = &[
    "[spmv]", "[mmr]", "[mystery camp]", "[tabisuru shima rin]", "nced", "ncop",
];

const SUB_LANG_TOKENS: &[&str] = &[
    "sc", "tc", "chs", "cht", "zh", "cn", "gb", "big5", "chi", "zho",
    "hans", "hant", "tw", "hk", "sg",
    "en", "eng", "us", "uk",
    "jp", "jpn", "ja", "jap",
    "kr", "kor", "ko",
    "fr", "fre", "fra", "de", "ger", "deu", "es", "spa",
    "ru", "rus", "pt", "por", "it", "ita", "ar", "ara", "th", "tha", "vi", "vie",
    "forced", "default", "sdh", "cc", "hi", "commentary",
    "scjp", "tcjp", "jpsc", "jptc", "chsjp", "chtjp", "bilingual",
    "简", "繁", "简体", "繁体", "简日", "繁日", "中英", "双语", "中字", "内封",
];

const WIN_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

const COMMON_RESOLUTIONS: &[i32] = &[480, 576, 720, 1080, 1440, 2160, 4320];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Main,
    Extra,
}

#[derive(Debug, Clone)]
pub struct RenameItem {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub kind: ItemKind,
}

#[derive(Debug, Clone)]
pub struct FolderPlan {
    pub src_dir: PathBuf,
    pub src_dirs: Vec<PathBuf>,
    pub target_parent_dir: PathBuf,
    pub folder_name: String,
    pub query: String,
    pub year_hint: Option<String>,
    pub media_type: String, // "tv" or "movie"
    pub display_title: String,
    pub tmdb_id: i64,
    pub season_override: Option<i32>,
    pub items: Vec<RenameItem>,
    pub collisions: Vec<String>,
    pub status: String,
    pub executed: bool,
}

impl FolderPlan {
    pub fn seasons_display(&self) -> String {
        let mut seasons = BTreeSet::new();
        for it in &self.items {
            if it.kind == ItemKind::Main {
                if let Some(parent_name) = it.dst.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()) {
                    if parent_name.starts_with("Season ") {
                        seasons.insert(parent_name.to_string());
                    }
                }
            }
        }
        if seasons.is_empty() {
            "-".to_string()
        } else {
            seasons.into_iter().collect::<Vec<_>>().join(", ")
        }
    }

    pub fn counts(&self) -> (usize, usize) {
        let m = self.items.iter().filter(|x| x.kind == ItemKind::Main).count();
        let e = self.items.len() - m;
        (m, e)
    }

    pub fn is_ready(&self) -> bool {
        !self.items.is_empty() && self.collisions.is_empty() && !self.executed
    }
}

pub fn norm_path_str(p: &str) -> PathBuf {
    let trimmed = p.trim();
    if trimmed.is_empty() {
        return PathBuf::new();
    }
    let is_unc = trimmed.starts_with("\\\\") || trimmed.starts_with("//");
    let stripped = trimmed.trim_end_matches(|c| c == '/' || c == '\\');
    if stripped.is_empty() {
        return PathBuf::from(trimmed);
    }
    if is_unc {
        let rest = stripped.trim_start_matches(|c| c == '/' || c == '\\');
        #[cfg(windows)]
        {
            PathBuf::from(format!("\\\\{}", rest.replace('/', "\\")))
        }
        #[cfg(not(windows))]
        {
            PathBuf::from(format!("//{}", rest.replace('\\', "/")))
        }
    } else {
        #[cfg(windows)]
        {
            let mut s = stripped.replace('/', "\\");
            if s.len() == 2 && s.as_bytes()[1] == b':' {
                s.push('\\');
            }
            PathBuf::from(s)
        }
        #[cfg(not(windows))]
        {
            PathBuf::from(stripped)
        }
    }
}

pub fn path_key(p: &Path) -> String {
    p.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

pub fn sanitize_filename(name: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_control() {
            continue;
        }
        match ch {
            ':' => out.push('：'),
            '/' | '\\' | '*' | '|' => out.push(' '),
            '?' => out.push('？'),
            '"' => out.push('\''),
            '<' => out.push('('),
            '>' => out.push(')'),
            _ => out.push(ch),
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut cleaned = collapsed.trim_matches(|c| c == ' ' || c == '.').to_string();
    if cleaned.is_empty() {
        return "_".to_string();
    }
    if WIN_RESERVED.contains(&cleaned.to_uppercase().as_str()) {
        cleaned.push('_');
    }
    if cleaned.chars().count() > max_len {
        cleaned = cleaned
            .chars()
            .take(max_len)
            .collect::<String>()
            .trim_end_matches(|c| c == ' ' || c == '.')
            .to_string();
    }
    cleaned
}

fn cn_num_to_i32(s: &str) -> Option<i32> {
    if let Ok(v) = s.parse::<i32>() {
        return Some(v);
    }
    match s {
        "一" => Some(1),
        "二" => Some(2),
        "三" => Some(3),
        "四" => Some(4),
        "五" => Some(5),
        "六" => Some(6),
        "七" => Some(7),
        "八" => Some(8),
        "九" => Some(9),
        "十" => Some(10),
        "十一" => Some(11),
        "十二" => Some(12),
        _ => None,
    }
}

pub fn clean_query_name(folder_name: &str) -> (String, Option<String>, String) {
    static RE_MOVIE_WORD: OnceLock<Regex> = OnceLock::new();
    static RE_BRACKET_YEAR: OnceLock<Regex> = OnceLock::new();
    static RE_PLAIN_YEAR: OnceLock<Regex> = OnceLock::new();
    static RE_LEAD_GROUP: OnceLock<Regex> = OnceLock::new();
    static RE_BRACKET_TECH: OnceLock<Regex> = OnceLock::new();
    static RE_SEASON_CUT: OnceLock<Regex> = OnceLock::new();
    static RE_TECH_CUT: OnceLock<Regex> = OnceLock::new();
    static RE_TRAIL_YEAR: OnceLock<Regex> = OnceLock::new();
    static RE_TRAIL_GROUP: OnceLock<Regex> = OnceLock::new();

    let orig = folder_name.trim();
    let re_movie = RE_MOVIE_WORD.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z])movie(?:[^A-Za-z]|$)").unwrap());
    let guessed = if orig.contains("剧场版")
        || orig.contains("电影版")
        || orig.contains("The Movie")
        || re_movie.is_match(orig)
    {
        "movie".to_string()
    } else {
        "auto".to_string()
    };

    let re_byr = RE_BRACKET_YEAR.get_or_init(|| Regex::new(r"[\(\[（【]((?:19|20)\d{2})[\)\]）】]").unwrap());
    let re_pyr = RE_PLAIN_YEAR.get_or_init(|| Regex::new(r"[\s._-]((?:19|20)\d{2})(?:[\s._-]|$)").unwrap());
    let year = re_byr
        .captures(orig)
        .and_then(|c| c.get(1))
        .or_else(|| re_pyr.captures(orig).and_then(|c| c.get(1)))
        .map(|m| m.as_str().to_string());

    let re_lead = RE_LEAD_GROUP.get_or_init(|| Regex::new(r"^(?:[\[【][^\]】]+[\]】]\s*)+").unwrap());
    let re_btech = RE_BRACKET_TECH.get_or_init(|| {
        Regex::new(r"(?i)[\(（\[【][^\)）\]】]*?(?:1080|2160|720|4K|BDRip|BDMV|BluRay|Blu-Ray|WEB-DL|WEBRip|HEVC|x265|x264|H\.?265|H\.?264|FLAC|Opus|AAC|DTS|字幕|全\d+集|Ma10p|Hi10p|10bit|CHS|CHT|BIG5|GB)[^\)）\]】]*?[\)）\]】]").unwrap()
    });

    let mut s = re_lead.replace(orig, "").to_string();
    s = re_btech.replace_all(&s, " ").to_string();
    s = re_byr.replace_all(&s, " ").to_string();

    s = s.replace('_', " ");
    let chars: Vec<char> = s.chars().collect();
    let mut dot_cleaned = String::with_capacity(s.len());
    for i in 0..chars.len() {
        if chars[i] == '.' {
            let prev_digit = i > 0 && chars[i - 1].is_ascii_digit();
            let next_digit = i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
            if prev_digit && next_digit {
                dot_cleaned.push('.');
            } else {
                dot_cleaned.push(' ');
            }
        } else {
            dot_cleaned.push(chars[i]);
        }
    }
    s = dot_cleaned;

    let re_scut = RE_SEASON_CUT.get_or_init(|| {
        Regex::new(r"(?i)\s+(?:S\d{1,2}(?:E\d{1,4})?|Season\s*\d+|\d+(?:st|nd|rd|th)\s+Season|第[一二三四五六七八九十\d]+季)(?:\b|[\s\W]).*$").unwrap()
    });
    let re_tcut = RE_TECH_CUT.get_or_init(|| {
        Regex::new(r"(?i)(?:^|\s)(?:2160p|1080p|720p|480p|4K|UHD|BDRip|BDMV|BluRay|Blu-Ray|WEB-DL|WEBRip|HDRip|DVDRip|Remux|HEVC|x265|x264|H265|H264|AV1|10bit|8bit|Ma10p|Hi10p|HDR|HDR10|DV|DoVi|FLAC|Opus|AAC|DTS|DTS-HD|TrueHD|AC3|DDP\d?|LPCM|Complete|Batch|Fin|V2|Dual[\s-]?Audio|Multi[\s-]?Sub)\b.*$").unwrap()
    });
    let re_tyr = RE_TRAIL_YEAR.get_or_init(|| Regex::new(r"\s+(?:19|20)\d{2}\s*$").unwrap());
    let re_tgrp = RE_TRAIL_GROUP.get_or_init(|| Regex::new(r"\s+-[A-Za-z0-9_]+$").unwrap());

    s = re_scut.replace(&s, "").to_string();
    s = re_tcut.replace(&s, "").to_string();
    if s.trim().len() > 4 {
        s = re_tyr.replace(&s, "").to_string();
    }
    s = re_tgrp.replace(&s, "").to_string();

    let final_q = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c| matches!(c, ' ' | '-' | '_' | '.'))
        .to_string();

    (
        if final_q.is_empty() {
            orig.to_string()
        } else {
            final_q
        },
        year,
        guessed,
    )
}

fn is_sub_lang_segment(seg: &str) -> bool {
    if seg.is_empty() || seg.len() > 24 {
        return false;
    }
    let parts: Vec<String> = seg
        .split(|c| matches!(c, '-' | '_' | '&' | '+'))
        .filter(|p| !p.is_empty())
        .map(|p| p.to_lowercase())
        .collect();
    !parts.is_empty() && parts.iter().all(|p| SUB_LANG_TOKENS.contains(&p.as_str()))
}

fn split_ext_pair(name: &str) -> (&str, &str) {
    if let Some(pos) = name.rfind('.') {
        if pos > 0 {
            return (&name[..pos], &name[pos..]);
        }
    }
    (name, "")
}

pub fn split_media_ext(fname: &str) -> (String, String) {
    let (base, ext) = split_ext_pair(fname);
    let el = ext.to_lowercase();
    if SUB_EXT.contains(&el.as_str()) {
        let mut cur = base;
        let mut lang_suffix = String::new();
        for _ in 0..2 {
            let (b2, tag) = split_ext_pair(cur);
            if tag.len() > 1 && is_sub_lang_segment(&tag[1..]) {
                lang_suffix = format!("{}{}", tag, lang_suffix);
                cur = b2;
            } else {
                break;
            }
        }
        return (cur.to_string(), format!("{}{}", lang_suffix, el));
    }
    (base.to_string(), el)
}

fn detect_season_in_text(text: &str) -> Option<i32> {
    static RE_SEASON: OnceLock<Regex> = OnceLock::new();
    static RE_SHORT_S: OnceLock<Regex> = OnceLock::new();
    static RE_ORD_S: OnceLock<Regex> = OnceLock::new();
    static RE_CN_S: OnceLock<Regex> = OnceLock::new();

    let re_s = RE_SEASON.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z])Season[\s._-]*0*(\d{1,2})(?:[^A-Za-z0-9]|$)").unwrap());
    if let Some(c) = re_s.captures(text) {
        return c[1].parse().ok();
    }
    let re_short = RE_SHORT_S.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z0-9])S0*(\d{1,2})(?:[\s._\-\[\]()（）]|$)").unwrap());
    if let Some(c) = re_short.captures(text) {
        return c[1].parse().ok();
    }
    let re_ord = RE_ORD_S.get_or_init(|| Regex::new(r"(?i)\b(\d{1,2})(?:st|nd|rd|th)[\s._-]*Season\b").unwrap());
    if let Some(c) = re_ord.captures(text) {
        return c[1].parse().ok();
    }
    let re_cn = RE_CN_S.get_or_init(|| Regex::new(r"第([一二三四五六七八九十\d]+)季").unwrap());
    if let Some(c) = re_cn.captures(text) {
        return cn_num_to_i32(&c[1]);
    }
    None
}

fn is_special_folder_name(text: &str) -> bool {
    static RE_SP: OnceLock<Regex> = OnceLock::new();
    let re_sp = RE_SP.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z])(?:ova|oad|sp|sps|special|specials)(?:[^A-Za-z]|$)").unwrap());
    re_sp.is_match(text) || text.contains("特别篇") || text.contains("番外")
}

pub fn detect_season(rel_parts: &[String], fname: &str, root_folder_name: &str) -> i32 {
    static RE_SXE: OnceLock<Regex> = OnceLock::new();
    let re_sxe = RE_SXE.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z])S0*(\d{1,2})E\d{1,4}(?:\.\d)?(?:v\d+)?(?:[^A-Za-z0-9]|$)").unwrap());
    if let Some(c) = re_sxe.captures(fname) {
        if let Ok(s) = c[1].parse::<i32>() {
            return s;
        }
    }

    let parent_parts = if rel_parts.len() > 1 {
        &rel_parts[..rel_parts.len() - 1]
    } else {
        &[]
    };
    for part in parent_parts.iter().map(|s| s.as_str()).chain(
        if root_folder_name.is_empty() { None } else { Some(root_folder_name) }
    ) {
        if let Some(s) = detect_season_in_text(part) {
            return s;
        }
        if is_special_folder_name(part) {
            return 0;
        }
    }
    1
}

/// Return true when filename itself contains an explicit `SxxExx` tag.
pub fn has_explicit_sxe_season(fname: &str) -> bool {
    static RE_SXE: OnceLock<Regex> = OnceLock::new();
    let re_sxe = RE_SXE.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^A-Za-z])S0*\d{1,2}E\d{1,4}(?:\.\d)?(?:v\d+)?(?:[^A-Za-z0-9]|$)").unwrap()
    });
    re_sxe.is_match(fname)
}

fn is_plausible_ep(ep_str: &str) -> bool {
    let head = ep_str.split('.').next().unwrap_or("");
    let val: i32 = match head.parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    if (1900..=2099).contains(&val) {
        return false;
    }
    if COMMON_RESOLUTIONS.contains(&val) {
        return false;
    }
    true
}

pub fn extract_episode_range(fname: &str) -> (Option<String>, Option<i32>) {
    static RE_SXE: OnceLock<Regex> = OnceLock::new();
    static RE_CN: OnceLock<Regex> = OnceLock::new();
    static RE_EP: OnceLock<Regex> = OnceLock::new();
    static RE_E2: OnceLock<Regex> = OnceLock::new();
    static RE_DASH: OnceLock<Regex> = OnceLock::new();
    static RE_BRACKET: OnceLock<Regex> = OnceLock::new();
    static RE_TRAIL: OnceLock<Regex> = OnceLock::new();
    static RE_LEAD: OnceLock<Regex> = OnceLock::new();

    let (stem, _) = split_media_ext(fname);

    let parse_valid_end = |s_str: &str, m2: Option<regex::Match<'_>>| -> Option<i32> {
        let e2: i32 = m2?.as_str().parse().ok()?;
        let e1: i32 = s_str.parse().ok()?;
        if e2 > e1 && e2 - e1 <= 12 && is_plausible_ep(&e2.to_string()) {
            Some(e2)
        } else {
            None
        }
    };

    let re_sxe = RE_SXE.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z])S\d{1,2}E(\d{1,4}(?:\.\d)?)(?:-E?(\d{1,4}))?(?:v\d+)?(?:[^A-Za-z0-9]|$)").unwrap());
    if let Some(c) = re_sxe.captures(&stem) {
        let e1 = c[1].to_string();
        let e2 = parse_valid_end(&e1, c.get(2));
        return (Some(e1), e2);
    }

    let re_cn = RE_CN.get_or_init(|| Regex::new(r"第(\d{1,4}(?:\.\d)?)[话話集]").unwrap());
    if let Some(c) = re_cn.captures(&stem) {
        return (Some(c[1].to_string()), None);
    }

    let re_ep = RE_EP.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z])EP\.?\s*(\d{1,4}(?:\.\d)?)(?:v\d+)?(?:[^A-Za-z0-9]|$)").unwrap());
    if let Some(c) = re_ep.captures(&stem) {
        return (Some(c[1].to_string()), None);
    }

    let re_e2 = RE_E2.get_or_init(|| Regex::new(r"(?i)(?:^|[^A-Za-z])E(\d{2,4}(?:\.\d)?)(?:-E?(\d{1,4}))?(?:v\d+)?(?:[^A-Za-z0-9]|$)").unwrap());
    if let Some(c) = re_e2.captures(&stem) {
        let e1 = c[1].to_string();
        let e2 = parse_valid_end(&e1, c.get(2));
        return (Some(e1), e2);
    }

    let re_dash = RE_DASH.get_or_init(|| Regex::new(r"(?i)\s-\s(\d{1,4}(?:\.\d)?)(?:-E?(\d{1,4}))?(?:v\d+)?(?:\s|\[|\(|_|\.|$)").unwrap());
    if let Some(c) = re_dash.captures(&stem) {
        if is_plausible_ep(&c[1]) {
            let e1 = c[1].to_string();
            let e2 = parse_valid_end(&e1, c.get(2));
            return (Some(e1), e2);
        }
    }

    let re_br = RE_BRACKET.get_or_init(|| Regex::new(r"(?i)\[(\d{1,4}(?:\.\d)?)(?:v\d+)?\]").unwrap());
    for c in re_br.captures_iter(&stem) {
        if is_plausible_ep(&c[1]) {
            return (Some(c[1].to_string()), None);
        }
    }

    let re_tr = RE_TRAIL.get_or_init(|| Regex::new(r"(?i)(?:^|[\s._-])(\d{2,3}(?:\.\d)?)(?:v\d+)?(?:\s*[\[\(].*)?$").unwrap());
    if let Some(c) = re_tr.captures(&stem) {
        if is_plausible_ep(&c[1]) {
            return (Some(c[1].to_string()), None);
        }
    }

    let re_ld = RE_LEAD.get_or_init(|| Regex::new(r"^(\d{1,3}(?:\.\d)?)(?:v\d+)?(?:[\s._-]|$)").unwrap());
    if let Some(c) = re_ld.captures(&stem) {
        if is_plausible_ep(&c[1]) {
            return (Some(c[1].to_string()), None);
        }
    }

    (None, None)
}

pub fn extract_episode(fname: &str) -> Option<String> {
    extract_episode_range(fname).0
}

/// Preserve multi-part episode tags like 'Part1' / 'Part 2' to avoid collapsing split episodes.
pub fn extract_part_tag(fname: &str) -> String {
    static RE_PART: OnceLock<Regex> = OnceLock::new();
    let (stem, _) = split_media_ext(fname);
    let re = RE_PART.get_or_init(|| Regex::new(r"(?i)(?:^|[\s._-])Part\s*0*(\d+)(?:[^A-Za-z0-9]|$)").unwrap());
    if let Some(c) = re.captures(&stem) {
        return format!(" - Part{}", &c[1]);
    }
    String::new()
}

pub fn fmt_episode(ep_str: &str) -> String {
    if let Some((a, b)) = ep_str.split_once('.') {
        let n: i32 = a.parse().unwrap_or(0);
        format!("{:02}.{}", n, b)
    } else {
        let n: i32 = ep_str.parse().unwrap_or(0);
        format!("{:02}", n)
    }
}

pub fn is_extra_rel(rel_parts: &[String]) -> bool {
    is_extra_rel_with_custom(rel_parts, &[])
}

pub fn is_extra_rel_with_custom(rel_parts: &[String], custom_extra_dirs: &[String]) -> bool {
    static RE_EXTRA_WORD: OnceLock<Regex> = OnceLock::new();
    if rel_parts.len() > 1 {
        for p in &rel_parts[..rel_parts.len() - 1] {
            let pl = p.to_lowercase();
            if EXTRA_DIRS.contains(&pl.as_str())
                || pl.contains("[fonts]")
                || custom_extra_dirs.iter().any(|c| !c.trim().is_empty() && c.trim().to_lowercase() == pl)
            {
                return true;
            }
        }
    }
    let last = match rel_parts.last() {
        Some(v) => v,
        None => return false,
    };
    let fn_l = last.to_lowercase();
    if EXTRA_FILE_TAGS.iter().any(|t| fn_l.contains(t)) {
        return true;
    }
    let (stem, _) = split_media_ext(last);
    let re_ew = RE_EXTRA_WORD.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^A-Za-z])(?:sample|trailer|teaser|pv\d*|cm\d*|menu\d*|ncop\d*|nced\d*)(?:[^A-Za-z0-9]|$)").unwrap()
    });
    re_ew.is_match(&stem)
}

fn extract_movie_part(stem: &str) -> String {
    static RE_PART: OnceLock<Regex> = OnceLock::new();
    let re = RE_PART.get_or_init(|| {
        Regex::new(r"(?i)(?:^|[^A-Za-z])(cd|disc|disk|part|pt)[\s._-]*(\d{1,2})(?:[^A-Za-z0-9]|$)").unwrap()
    });
    if let Some(c) = re.captures(stem) {
        let mut label = c[1].to_lowercase();
        if label == "pt" {
            label = "part".to_string();
        }
        if let Ok(n) = c[2].parse::<i32>() {
            return format!("-{}{}", label, n);
        }
    }
    String::new()
}

pub fn build_multi_folder_items(
    src_dirs: &[PathBuf],
    target_parent_dir: &Path,
    target_folder_name: &str,
    media_type: &str,
    archive_extras: bool,
    extras_root_dir: Option<&Path>,
    season_override: Option<i32>,
    apply_continuous_offset: bool,
    custom_extra_dirs: &[String],
) -> (Vec<RenameItem>, Vec<String>) {
    static RE_YEAR_SUFFIX: OnceLock<Regex> = OnceLock::new();
    let safe_folder = sanitize_filename(target_folder_name, 160);
    let re_yr = RE_YEAR_SUFFIX.get_or_init(|| Regex::new(r"\s*\(\d{4}\)$").unwrap());
    let base_title = re_yr.replace(&safe_folder, "").trim().to_string();
    let multi_src = src_dirs.len() > 1;

    let mut items: Vec<RenameItem> = Vec::new();
    let mut raw_tv: Vec<(PathBuf, i32, String, Option<i32>, String, String)> = Vec::new();

    for src_dir in src_dirs {
        let src_folder_name = src_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let mut files: Vec<PathBuf> = WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .collect();
        files.sort();

        for src_path in files {
            let fname = match src_path.file_name().and_then(|s| s.to_str()) {
                Some(n) if !n.starts_with('.') => n.to_string(),
                _ => continue,
            };
            let rel = match src_path.strip_prefix(src_dir) {
                Ok(r) => r.to_path_buf(),
                Err(_) => continue,
            };
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();

            let (_, ext_only) = split_ext_pair(&fname);
            let ext_l = ext_only.to_lowercase();
            let (stem, full_ext) = split_media_ext(&fname);

            let extra_rel = if multi_src {
                PathBuf::from(&src_folder_name).join(&rel)
            } else {
                rel.clone()
            };

            let is_media = VIDEO_EXT.contains(&ext_l.as_str()) || SUB_EXT.contains(&ext_l.as_str());
            if is_extra_rel_with_custom(&parts, custom_extra_dirs) || !is_media {
                if archive_extras {
                    if let Some(ex_root) = extras_root_dir {
                        if !ex_root.as_os_str().is_empty() {
                            items.push(RenameItem {
                                src: src_path,
                                dst: ex_root.join(&safe_folder).join(&extra_rel),
                                kind: ItemKind::Extra,
                            });
                        }
                    }
                }
                continue;
            }

            if media_type == "movie" {
                let part_suffix = extract_movie_part(&stem);
                let dst_name = format!("{}{}{}", safe_folder, part_suffix, full_ext);
                items.push(RenameItem {
                    src: src_path,
                    dst: target_parent_dir.join(&safe_folder).join(dst_name),
                    kind: ItemKind::Main,
                });
            } else {
                let (ep, ep_end) = match extract_episode_range(&fname) {
                    (Some(e), e2) => (e, e2),
                    (None, _) => {
                        if archive_extras {
                            if let Some(ex_root) = extras_root_dir {
                                if !ex_root.as_os_str().is_empty() {
                                    items.push(RenameItem {
                                        src: src_path,
                                        dst: ex_root.join(&safe_folder).join(&extra_rel),
                                        kind: ItemKind::Extra,
                                    });
                                }
                            }
                        }
                        continue;
                    }
                };
                let s = if let Some(forced_s) = season_override {
                    forced_s
                } else {
                    let mut detected = detect_season(&parts, &fname, &src_folder_name);
                    if ep.contains('.') && !has_explicit_sxe_season(&fname) {
                        detected = 0;
                    }
                    detected
                };
                let part_tag = extract_part_tag(&fname);
                raw_tv.push((src_path, s, ep, ep_end, part_tag, full_ext));
            }
        }
    }

    if !raw_tv.is_empty() {
        let mut season_eps: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
        for (_, s, ep, _, _, _) in &raw_tv {
            if *s >= 1 && !ep.contains('.') {
                if let Ok(n) = ep.parse::<i32>() {
                    season_eps.entry(*s).or_default().push(n);
                }
            }
        }
        let mut offsets: HashMap<i32, i32> = HashMap::new();
        if apply_continuous_offset {
            let mut prev_max = 0;
            for (s, eps) in &season_eps {
                let mn = *eps.iter().min().unwrap_or(&0);
                let mx = *eps.iter().max().unwrap_or(&0);
                if *s > 1 && prev_max > 0 && mn == prev_max + 1 {
                    offsets.insert(*s, prev_max);
                }
                prev_max = mx;
            }
        }

        for (src_path, s, ep, ep_end, part_tag, full_ext) in raw_tv {
            let (ep_fmt, end_suffix) = if let Some(&off) = offsets.get(&s) {
                if !ep.contains('.') {
                    let n = ep.parse::<i32>().unwrap_or(0) - off;
                    let end_s = ep_end
                        .map(|e2| format!("-E{:02}", (e2 - off).max(1)))
                        .unwrap_or_default();
                    (format!("{:02}", n), end_s)
                } else {
                    (fmt_episode(&ep), String::new())
                }
            } else {
                let end_s = ep_end.map(|e2| format!("-E{:02}", e2)).unwrap_or_default();
                (fmt_episode(&ep), end_s)
            };

            let dst_name = format!(
                "{} S{:02}E{}{}{}{}",
                base_title, s, ep_fmt, end_suffix, part_tag, full_ext
            );
            let dst_path = target_parent_dir
                .join(&safe_folder)
                .join(format!("Season {:02}", s))
                .join(dst_name);
            items.push(RenameItem {
                src: src_path,
                dst: dst_path,
                kind: ItemKind::Main,
            });
        }
    }

    items.sort_by(|a, b| {
        let ka = if a.kind == ItemKind::Main { 0 } else { 1 };
        let kb = if b.kind == ItemKind::Main { 0 } else { 1 };
        (ka, a.dst.to_string_lossy().to_string()).cmp(&(kb, b.dst.to_string_lossy().to_string()))
    });

    let cols = find_item_collisions(&items);
    (items, cols)
}

/// Align multi-season episode numbers and drifted seasons using TMDB metadata:
/// - For seasonal TMDB series (seasons starting at E01), subtracts continuous absolute offsets
///   when local Season 2+ continues from Season 1's max episode.
/// - For continuous-numbered TMDB series (e.g. One Piece where S02 starts at 62, S21 starts at 892),
///   keeps continuous episode numbers intact and relocates episodes whose local Season folder drifted
///   into the unique matching TMDB Season.
pub fn reconcile_tv_episodes<FS, FE>(
    items: Vec<RenameItem>,
    mut seasons_fetcher: FS,
    mut season_fetcher: FE,
    tmdb_id: i64,
) -> (Vec<RenameItem>, Vec<String>, Option<String>)
where
    FS: FnMut(i64) -> Result<HashMap<i32, String>, String>,
    FE: FnMut(i64, i32) -> Result<HashMap<i32, String>, String>,
{
    if tmdb_id == 0 {
        let cols = find_item_collisions(&items);
        return (items, cols, None);
    }

    static RE_TOK: OnceLock<Regex> = OnceLock::new();
    let re_tok = RE_TOK.get_or_init(|| Regex::new(r"S(\d{2})E(\d{2,4})(\.\d)?(?:-E(\d{2,4}))?").unwrap());

    struct ParsedTok {
        start: usize,
        end: usize,
        s_num: i32,
        ep_int: i32,
        dec: String,
        ep_end: Option<i32>,
    }

    let mut local_seasons: BTreeSet<i32> = BTreeSet::new();
    let mut season_eps: BTreeMap<i32, Vec<i32>> = BTreeMap::new();
    let mut parsed: Vec<(RenameItem, Option<ParsedTok>)> = Vec::with_capacity(items.len());

    for it in items {
        let mut hit = None;
        if it.kind == ItemKind::Main {
            if let Some(bn) = it.dst.file_name().and_then(|s| s.to_str()) {
                if let Some(c) = re_tok.captures(bn) {
                    let full = c.get(0).unwrap();
                    if let (Ok(s_num), Ok(ep_int)) = (c[1].parse::<i32>(), c[2].parse::<i32>()) {
                        let dec = c.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
                        let ep_end = c.get(4).and_then(|m| m.as_str().parse::<i32>().ok());
                        if s_num >= 1 {
                            local_seasons.insert(s_num);
                            if dec.is_empty() {
                                season_eps.entry(s_num).or_default().push(ep_int);
                            }
                        }
                        hit = Some(ParsedTok {
                            start: full.start(),
                            end: full.end(),
                            s_num,
                            ep_int,
                            dec,
                            ep_end,
                        });
                    }
                }
            }
        }
        parsed.push((it, hit));
    }

    if local_seasons.is_empty() {
        let out: Vec<RenameItem> = parsed.into_iter().map(|(it, _)| it).collect();
        let cols = find_item_collisions(&out);
        return (out, cols, None);
    }

    let tmdb_seasons: Vec<i32> = match seasons_fetcher(tmdb_id) {
        Ok(map) => {
            let mut ks: Vec<i32> = map.keys().copied().filter(|&k| k > 0).collect();
            ks.sort_unstable();
            ks
        }
        Err(_) => {
            let out: Vec<RenameItem> = parsed.into_iter().map(|(it, _)| it).collect();
            let cols = find_item_collisions(&out);
            return (out, cols, None);
        }
    };

    let mut tmdb_ep_maps: HashMap<i32, HashMap<i32, String>> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    for &s_num in &local_seasons {
        if tmdb_seasons.contains(&s_num) {
            match season_fetcher(tmdb_id, s_num) {
                Ok(m) => {
                    tmdb_ep_maps.insert(s_num, m);
                }
                Err(e) => warnings.push(format!("Season {:02} 集列表获取失败: {}", s_num, e)),
            }
        }
    }

    // Peek Season 02 if needed to tell continuous long-running shows apart from 1-based seasonal shows.
    if !tmdb_ep_maps.contains_key(&2) && tmdb_seasons.contains(&2) {
        let has_miss_in_s1 = parsed.iter().any(|(_, h)| {
            h.as_ref()
                .map(|p| {
                    p.s_num == 1
                        && !tmdb_ep_maps
                            .get(&1)
                            .map(|m| m.contains_key(&p.ep_int))
                            .unwrap_or(false)
                })
                .unwrap_or(false)
        });
        if has_miss_in_s1 {
            if let Ok(m) = season_fetcher(tmdb_id, 2) {
                tmdb_ep_maps.insert(2, m);
            }
        }
    }

    let tmdb_is_continuous = tmdb_ep_maps
        .iter()
        .any(|(&ts, m)| ts > 1 && m.keys().copied().min().unwrap_or(1) > 1);

    let mut offsets: HashMap<i32, i32> = HashMap::new();
    let mut relocations: HashMap<(i32, i32), i32> = HashMap::new();

    if !tmdb_is_continuous {
        let mut prev_local_max = 0;
        let mut prev_tmdb_max = 0;
        for (s, eps) in &season_eps {
            let mn = *eps.iter().min().unwrap_or(&0);
            let mx = *eps.iter().max().unwrap_or(&0);
            let tmdb_max = tmdb_ep_maps
                .get(s)
                .and_then(|m| m.keys().copied().max())
                .unwrap_or(0);
            if *s > 1 {
                let candidate_off = if prev_tmdb_max > 0 && mn > prev_tmdb_max {
                    Some(prev_tmdb_max)
                } else if prev_local_max > 0 && mn == prev_local_max + 1 {
                    Some(prev_local_max)
                } else {
                    None
                };
                if let (Some(off), Some(m)) = (candidate_off, tmdb_ep_maps.get(s)) {
                    if m.contains_key(&(mn - off)) && !m.contains_key(&mn) {
                        offsets.insert(*s, off);
                    }
                }
            }
            prev_local_max = mx;
            prev_tmdb_max = if tmdb_max > 0 { prev_tmdb_max + tmdb_max } else { 0 };
        }

        // Flat absolute pack (all in Season 01 with max_ep > TMDB S01 max) -> split across TMDB S01..SN
        if local_seasons.len() == 1 && local_seasons.contains(&1) && tmdb_seasons.len() > 1 {
            let local_max = season_eps
                .get(&1)
                .and_then(|v| v.iter().max().copied())
                .unwrap_or(0);
            let s1_max = tmdb_ep_maps
                .get(&1)
                .and_then(|m| m.keys().copied().max())
                .unwrap_or(0);
            if s1_max > 0 && local_max > s1_max {
                for &ts in &tmdb_seasons {
                    if let std::collections::hash_map::Entry::Vacant(e) = tmdb_ep_maps.entry(ts) {
                        if let Ok(m) = season_fetcher(tmdb_id, ts) {
                            e.insert(m);
                        }
                    }
                }
                let mut cum_ranges: Vec<(i32, i32, i32, i32)> = Vec::new();
                let mut cum_off = 0i32;
                let mut valid_chain = true;
                for &ts in &tmdb_seasons {
                    if let Some(m) = tmdb_ep_maps.get(&ts) {
                        let ts_min = m.keys().copied().filter(|&k| k > 0).min().unwrap_or(0);
                        let ts_max = m.keys().copied().filter(|&k| k > 0).max().unwrap_or(0);
                        if ts_max <= 0 || ts_min != 1 {
                            valid_chain = false;
                            break;
                        }
                        cum_ranges.push((ts, cum_off + 1, cum_off + ts_max, cum_off));
                        cum_off += ts_max;
                    } else {
                        valid_chain = false;
                        break;
                    }
                }
                if valid_chain && cum_off >= local_max {
                    for &ep_int in season_eps.get(&1).into_iter().flatten() {
                        if let Some(&(ts, _, _, c_off)) = cum_ranges
                            .iter()
                            .find(|&&(_, rs, re, _)| (rs..=re).contains(&ep_int))
                        {
                            if ts != 1 || c_off != 0 {
                                relocations.insert((1, ep_int), ts);
                                offsets.insert(ts, c_off);
                            }
                        }
                    }
                    // Also handle decimal recaps in flat pack if any
                    for (_, hit) in &parsed {
                        if let Some(p) = hit {
                            if p.s_num == 1 && !p.dec.is_empty() {
                                if let Some(&(ts, _, _, c_off)) = cum_ranges
                                    .iter()
                                    .find(|&&(_, rs, re, _)| (rs..=re).contains(&p.ep_int))
                                {
                                    if ts != 1 || c_off != 0 {
                                        relocations.insert((1, p.ep_int), ts);
                                        offsets.insert(ts, c_off);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        let has_missing = parsed.iter().any(|(_, h)| {
            h.as_ref()
                .map(|p| {
                    p.s_num >= 1
                        && !tmdb_ep_maps
                            .get(&p.s_num)
                            .map(|m| m.contains_key(&p.ep_int))
                            .unwrap_or(false)
                })
                .unwrap_or(false)
        });
        if has_missing {
            for &ts in &tmdb_seasons {
                if !tmdb_ep_maps.contains_key(&ts) {
                    if let Ok(m) = season_fetcher(tmdb_id, ts) {
                        tmdb_ep_maps.insert(ts, m);
                    }
                }
            }
            let mut ep_to_seasons: HashMap<i32, Vec<i32>> = HashMap::new();
            for (&ts, m) in &tmdb_ep_maps {
                if ts > 0 {
                    for &ep_n in m.keys() {
                        ep_to_seasons.entry(ep_n).or_default().push(ts);
                    }
                }
            }
            for (_, hit) in &parsed {
                if let Some(p) = hit {
                    if p.s_num >= 1 {
                        let ok_in_cur = tmdb_ep_maps
                            .get(&p.s_num)
                            .map(|m| m.contains_key(&p.ep_int))
                            .unwrap_or(false);
                        if !ok_in_cur {
                            if let Some(cands) = ep_to_seasons.get(&p.ep_int) {
                                if cands.len() == 1 {
                                    relocations.insert((p.s_num, p.ep_int), cands[0]);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut out = Vec::with_capacity(parsed.len());
    for (it, hit) in parsed {
        if let Some(p) = hit {
            let new_s = relocations.get(&(p.s_num, p.ep_int)).copied().unwrap_or(p.s_num);
            let off = offsets.get(&new_s).copied().unwrap_or(0);
            let new_ep = (p.ep_int - off).max(1);
            let new_ep_end = p.ep_end.map(|e2| (e2 - off).max(1));

            if new_s != p.s_num || new_ep != p.ep_int {
                let bn = it.dst.file_name().unwrap().to_string_lossy();
                let end_str = new_ep_end.map(|e2| format!("-E{:02}", e2)).unwrap_or_default();
                let new_tok = format!("S{:02}E{:02}{}{}", new_s, new_ep, p.dec, end_str);
                let new_bn = format!("{}{}{}", &bn[..p.start], new_tok, &bn[p.end..]);

                let old_s_folder = format!("Season {:02}", p.s_num);
                let new_s_folder = format!("Season {:02}", new_s);
                let new_dst = match (
                    it.dst.parent(),
                    it.dst.parent().and_then(|d| d.parent()),
                    it.dst.parent().and_then(|d| d.file_name()).and_then(|s| s.to_str()),
                ) {
                    (Some(_), Some(show_root), Some(sf)) if sf == old_s_folder => {
                        show_root.join(new_s_folder).join(&new_bn)
                    }
                    (Some(dir), _, _) => dir.join(&new_bn),
                    _ => PathBuf::from(&new_bn),
                };

                out.push(RenameItem {
                    src: it.src,
                    dst: new_dst,
                    kind: it.kind,
                });
                continue;
            }
        }
        out.push(it);
    }

    out.sort_by(|a, b| {
        let ka = if a.kind == ItemKind::Main { 0 } else { 1 };
        let kb = if b.kind == ItemKind::Main { 0 } else { 1 };
        (ka, a.dst.to_string_lossy().to_string()).cmp(&(kb, b.dst.to_string_lossy().to_string()))
    });

    let cols = find_item_collisions(&out);
    let warn = if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("；"))
    };
    (out, cols, warn)
}

/// Return {display_dst: [src_paths...]} for destinations targeted by >1 sources.
pub fn find_collision_map(items: &[RenameItem]) -> BTreeMap<PathBuf, Vec<PathBuf>> {
    let mut grouped: HashMap<String, (PathBuf, Vec<PathBuf>)> = HashMap::new();
    for it in items {
        let k = path_key(&it.dst);
        grouped
            .entry(k)
            .or_insert_with(|| (it.dst.clone(), Vec::new()))
            .1
            .push(it.src.clone());
    }
    let mut out = BTreeMap::new();
    for (_k, (dst, srcs)) in grouped {
        if srcs.len() > 1 {
            out.insert(dst, srcs);
        }
    }
    out
}

pub fn find_item_collisions(items: &[RenameItem]) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for it in items {
        *counts.entry(path_key(&it.dst)).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .filter_map(|(k, v)| if v > 1 { Some(k) } else { None })
        .collect()
}

pub fn validate_all_plans(plans: &mut [FolderPlan], check_disk: bool) {
    let mut global_dst: HashMap<String, usize> = HashMap::new();
    let mut all_srcs: HashSet<String> = HashSet::new();
    for p in plans.iter() {
        if p.executed {
            continue;
        }
        for it in &p.items {
            *global_dst.entry(path_key(&it.dst)).or_insert(0) += 1;
            all_srcs.insert(path_key(&it.src));
        }
    }

    for p in plans.iter_mut() {
        if p.executed || p.items.is_empty() {
            continue;
        }
        let mut cols: Vec<String> = Vec::new();
        let mut disk_hits = 0usize;
        for it in &p.items {
            let dk = path_key(&it.dst);
            let sk = path_key(&it.src);
            if global_dst.get(&dk).copied().unwrap_or(0) > 1 {
                if !cols.contains(&dk) {
                    cols.push(dk);
                }
            } else if check_disk && dk != sk && !all_srcs.contains(&dk) && it.dst.exists() {
                if !cols.contains(&dk) {
                    cols.push(dk);
                    disk_hits += 1;
                }
            }
        }
        p.collisions = cols;
        if !p.collisions.is_empty() {
            if disk_hits == p.collisions.len() {
                p.status = format!("⚠ 目标已存在 ({})", p.collisions.len());
            } else {
                p.status = format!("⚠ 命名冲突 ({})", p.collisions.len());
            }
        } else if p.status.starts_with('⚠') {
            p.status = "就绪".to_string();
        }
    }
}

pub fn attach_episode_titles<F>(
    items: Vec<RenameItem>,
    mut season_fetcher: F,
    tmdb_id: i64,
) -> (Vec<RenameItem>, Vec<String>, Option<String>)
where
    F: FnMut(i64, i32) -> Result<HashMap<i32, String>, String>,
{
    if tmdb_id == 0 {
        let cols = find_item_collisions(&items);
        return (items, cols, None);
    }

    static RE_SE: OnceLock<Regex> = OnceLock::new();
    let re_se = RE_SE.get_or_init(|| {
        Regex::new(r"(S(\d{2})E(\d{2,4})(?:-E\d{2,4})?(?:\s*-\s*Part\d+)?)(?:\s+\[[^\]]+\])?").unwrap()
    });

    let mut seasons_needed = BTreeSet::new();
    let mut parsed: Vec<(RenameItem, Option<(usize, usize, String, i32, i32)>)> = Vec::with_capacity(items.len());

    for it in items {
        let mut hit = None;
        if it.kind == ItemKind::Main {
            if let Some(bn) = it.dst.file_name().and_then(|s| s.to_str()) {
                if let Some(c) = re_se.captures(bn) {
                    let full_m = c.get(0).unwrap();
                    let tok_m = c.get(1).unwrap();
                    let ep_end = c.get(3).unwrap().end();
                    let after_ep = &bn[ep_end..];
                    let is_decimal = after_ep.starts_with('.')
                        && after_ep.as_bytes().get(1).map(|b| b.is_ascii_digit()).unwrap_or(false);
                    if !is_decimal {
                        if let (Ok(s_num), Ok(ep_num)) = (c[2].parse::<i32>(), c[3].parse::<i32>()) {
                            seasons_needed.insert(s_num);
                            hit = Some((full_m.start(), full_m.end(), tok_m.as_str().to_string(), s_num, ep_num));
                        }
                    }
                }
            }
        }
        parsed.push((it, hit));
    }

    let mut ep_map: HashMap<(i32, i32), String> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();
    for s_num in seasons_needed {
        match season_fetcher(tmdb_id, s_num) {
            Ok(map) => {
                for (ep_num, title) in map {
                    ep_map.insert((s_num, ep_num), title);
                }
            }
            Err(e) => warnings.push(format!("Season {:02} 集名获取失败: {}", s_num, e)),
        }
    }

    let mut out = Vec::with_capacity(parsed.len());
    for (it, hit) in parsed {
        if let Some((start, end, se_tok, s_num, ep_num)) = hit {
            if let Some(raw_t) = ep_map.get(&(s_num, ep_num)) {
                if !raw_t.is_empty() {
                    let safe_t = sanitize_filename(raw_t, 80);
                    let bn = it.dst.file_name().unwrap().to_string_lossy();
                    let new_bn = format!("{}{} [{}]{}", &bn[..start], se_tok, safe_t, &bn[end..]);
                    let new_dst = it.dst.parent().map(|p| p.join(&new_bn)).unwrap_or_else(|| PathBuf::from(&new_bn));
                    out.push(RenameItem {
                        src: it.src,
                        dst: new_dst,
                        kind: it.kind,
                    });
                    continue;
                }
            }
        }
        out.push(it);
    }

    let cols = find_item_collisions(&out);
    let warn = if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("；"))
    };
    (out, cols, warn)
}

/// Append '（TMDB Season Name）' to Season XX folder when TMDB has a non-generic season name.
pub fn attach_season_names<F>(
    items: Vec<RenameItem>,
    mut seasons_fetcher: F,
    tmdb_id: i64,
) -> (Vec<RenameItem>, Vec<String>, Option<String>)
where
    F: FnMut(i64) -> Result<HashMap<i32, String>, String>,
{
    if tmdb_id == 0 {
        let cols = find_item_collisions(&items);
        return (items, cols, None);
    }

    let season_names = match seasons_fetcher(tmdb_id) {
        Ok(m) => m,
        Err(e) => {
            let cols = find_item_collisions(&items);
            return (items, cols, Some(format!("季名获取失败: {}", e)));
        }
    };
    if season_names.is_empty() {
        let cols = find_item_collisions(&items);
        return (items, cols, None);
    }

    static RE_SEASON_DIR: OnceLock<Regex> = OnceLock::new();
    static RE_GENERIC_NAME: OnceLock<Regex> = OnceLock::new();
    let re_sdir = RE_SEASON_DIR.get_or_init(|| Regex::new(r"^Season (\d{2})$").unwrap());
    let re_gen = RE_GENERIC_NAME.get_or_init(|| Regex::new(r"(?i)^(?:第\s*\d+\s*季|Season\s*\d+)$").unwrap());

    let mut out = Vec::with_capacity(items.len());
    for it in items {
        if it.kind == ItemKind::Main {
            if let (Some(d_dir), Some(bn)) = (it.dst.parent(), it.dst.file_name()) {
                if let (Some(parent_dir), Some(season_folder)) =
                    (d_dir.parent(), d_dir.file_name().and_then(|s| s.to_str()))
                {
                    if let Some(c) = re_sdir.captures(season_folder) {
                        if let Ok(s_num) = c[1].parse::<i32>() {
                            if let Some(raw_name) = season_names.get(&s_num) {
                                let trimmed = raw_name.trim();
                                if !trimmed.is_empty() && !re_gen.is_match(trimmed) {
                                    let safe_name = sanitize_filename(trimmed, 80);
                                    let new_season_folder = format!("{}（{}）", season_folder, safe_name);
                                    out.push(RenameItem {
                                        src: it.src,
                                        dst: parent_dir.join(new_season_folder).join(bn),
                                        kind: it.kind,
                                    });
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
        out.push(it);
    }

    let cols = find_item_collisions(&out);
    (out, cols, None)
}

pub fn safe_move(src: &Path, dst: &Path) -> Result<bool, String> {
    if src == dst {
        return Ok(false);
    }
    if path_key(src) == path_key(dst) {
        let parent = src.parent().unwrap_or_else(|| Path::new("."));
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = parent.join(format!(".__tmp_rename_{}__", stamp));
        fs::rename(src, &tmp).map_err(|e| format!("临时重命名失败: {}", e))?;
        if let Err(e) = fs::rename(&tmp, dst) {
            let _ = fs::rename(&tmp, src);
            return Err(format!("大小写重命名失败: {}", e));
        }
        return Ok(true);
    }

    if dst.exists() {
        return Err(format!("目标已存在: {}", dst.display()));
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    match fs::rename(src, dst) {
        Ok(()) => Ok(true),
        Err(_) => {
            // Atomic cross-drive / SMB copy via temporary file to prevent half-written files on interruption
            let parent = dst.parent().unwrap_or_else(|| Path::new("."));
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let tmp_dst = parent.join(format!(".__tmdb_copy_tmp_{}__", stamp));
            if let Err(e) = fs::copy(src, &tmp_dst) {
                let _ = fs::remove_file(&tmp_dst);
                return Err(format!("跨盘复制失败: {}", e));
            }
            if let Err(e) = fs::rename(&tmp_dst, dst) {
                let _ = fs::remove_file(&tmp_dst);
                return Err(format!("跨盘落盘重命名失败: {}", e));
            }
            if let Err(e) = fs::remove_file(src) {
                let _ = fs::remove_file(dst);
                return Err(format!("删除源文件失败: {}", e));
            }
            Ok(true)
        }
    }
}

pub fn prune_empty_dirs(src_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for sdir in src_dirs {
        if !sdir.is_dir() {
            continue;
        }
        let mut dirs: Vec<PathBuf> = WalkDir::new(sdir)
            .contents_first(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
            .map(|e| e.into_path())
            .collect();
        dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        for dp in dirs {
            // Drop ReadDir handle before RemoveDirectoryW to avoid SMB STATUS_DELETE_PENDING / sharing glitches
            let is_empty = fs::read_dir(&dp)
                .map(|mut rd| rd.next().is_none())
                .unwrap_or(false);
            if is_empty && fs::remove_dir(&dp).is_ok() {
                removed.push(dp);
            }
        }
    }
    removed
}


pub fn append_rename_history(history_path: &Path, moves: &[(PathBuf, PathBuf)]) {
    if moves.is_empty() {
        return;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let entries: Vec<serde_json::Value> = moves
        .iter()
        .map(|(src, dst)| {
            serde_json::json!({
                "src": src.to_string_lossy(),
                "dst": dst.to_string_lossy(),
            })
        })
        .collect();
    let line = serde_json::json!({
        "timestamp_unix": ts,
        "count": entries.len(),
        "moves": entries,
    });
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{}", line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_query_name_cases() {
        let (q, yr, g) = clean_query_name(
            "A.Ninja.and.an.Assassin.Under.One.Roof.S01.1080p.BluRay.Opus.2.0.x265-YURASUKA",
        );
        assert_eq!(q, "A Ninja and an Assassin Under One Roof");
        assert_eq!(yr, None);
        assert_eq!(g, "auto");

        let (q, yr, _) = clean_query_name("[VCB-Studio] 胆大党 (2024) [1080p HEVC]");
        assert_eq!(q, "胆大党");
        assert_eq!(yr.as_deref(), Some("2024"));

        let (q, yr, _) = clean_query_name("Spy.x.Family.2022.S02.1080p.WEB-DL.x265-Group");
        assert_eq!(q, "Spy x Family");
        assert_eq!(yr.as_deref(), Some("2022"));
    }

    #[test]
    fn test_detect_season_cases() {
        assert_eq!(
            detect_season(&["Spy.x.Family.S01E01.mkv".into()], "Spy.x.Family.S01E01.mkv", "Spy x Family (2022)"),
            1
        );
        assert_eq!(
            detect_season(&["ep01.mkv".into()], "ep01.mkv", "Some.Show.1080p.BluRay.Opus 2.0"),
            1
        );
        assert_eq!(detect_season(&["OVA".into(), "01.mkv".into()], "01.mkv", "Show"), 0);
        assert_eq!(detect_season(&["ep01.mkv".into()], "ep01.mkv", "Show 第二季"), 2);
        assert!(!is_extra_rel(&["爆炸".into(), "[DBD-Raws][Explosion][01].mkv".into()]));
        assert!(is_extra_rel_with_custom(&["爆炸".into(), "[DBD-Raws][Explosion][01].mkv".into()], &["爆炸".into()]));
    }

    #[test]
    fn test_extract_episode_and_part_cases() {
        assert_eq!(extract_episode("[SubGroup] Show Name [2024] - 05 [1080p].mkv").as_deref(), Some("05"));
        assert_eq!(extract_episode("86 -Eighty Six- 01.mkv").as_deref(), Some("01"));
        assert_eq!(extract_episode("Show.S00E05.5.mkv").as_deref(), Some("05.5"));
        assert_eq!(extract_episode("第07集.mkv").as_deref(), Some("07"));
        assert_eq!(
            extract_episode("[Judas] Gintama - Mr. Ginpachi - S01E01v2.mkv").as_deref(),
            Some("01")
        );
        assert_eq!(extract_episode("Show EP03v2.mkv").as_deref(), Some("03"));
        assert_eq!(extract_episode("Show E12v3.mkv").as_deref(), Some("12"));
        assert_eq!(extract_episode("04v2.mkv").as_deref(), Some("04"));
        assert_eq!(
            detect_season(&["[Judas] Gintama - Mr. Ginpachi - S02E01v2.mkv".into()], "[Judas] Gintama - Mr. Ginpachi - S02E01v2.mkv", "Show"),
            2
        );
        assert_eq!(extract_part_tag("Show S01E01 Part 2.mkv"), " - Part2");
        assert_eq!(extract_part_tag("Show S01E01.Part1.mkv"), " - Part1");
    }

    #[test]
    fn test_split_media_ext_cases() {
        assert_eq!(
            split_media_ext("A.Ninja.S01E01.1080p.BluRay.x265.srt"),
            ("A.Ninja.S01E01.1080p.BluRay.x265".into(), ".srt".into())
        );
        assert_eq!(
            split_media_ext("Show S01E01.zh-CN.forced.srt"),
            ("Show S01E01".into(), ".zh-CN.forced.srt".into())
        );
    }

    #[test]
    fn test_attach_episode_and_season_names() {
        let items = vec![
            RenameItem {
                src: PathBuf::from("a.mkv"),
                dst: PathBuf::from("/tmp/物语系列 (2009)/Season 01/物语系列 S01E01 - Part1.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("b.mkv"),
                dst: PathBuf::from("/tmp/物语系列 (2009)/Season 02/物语系列 S02E01.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("c.mkv"),
                dst: PathBuf::from("/tmp/物语系列 (2009)/Season 00/物语系列 S00E05.5.mkv"),
                kind: ItemKind::Main,
            },
        ];
        let (out1, cols1, warn1) = attach_episode_titles(
            items,
            |_tid, _s| {
                let mut m = HashMap::new();
                m.insert(1, "黑仪重蟹 其之一".into());
                m.insert(5, "第五话".into());
                Ok(m)
            },
            123,
        );
        assert!(cols1.is_empty() && warn1.is_none());
        assert_eq!(
            out1[0].dst.file_name().unwrap().to_str().unwrap(),
            "物语系列 S01E01 - Part1 [黑仪重蟹 其之一].mkv"
        );
        assert_eq!(out1[2].dst.file_name().unwrap().to_str().unwrap(), "物语系列 S00E05.5.mkv");

        let (out2, cols2, warn2) = attach_season_names(
            out1,
            |_tid| {
                let mut m = HashMap::new();
                m.insert(1, "化物语".into());
                m.insert(2, "第 2 季".into()); // generic -> should be skipped
                Ok(m)
            },
            123,
        );
        assert!(cols2.is_empty() && warn2.is_none());
        assert_eq!(
            out2[0].dst.parent().unwrap().file_name().unwrap().to_str().unwrap(),
            "Season 01（化物语）"
        );
        assert_eq!(
            out2[1].dst.parent().unwrap().file_name().unwrap().to_str().unwrap(),
            "Season 02"
        );
    }

    #[test]
    fn test_season_override_overrides_filename_s01() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("tmdb_renamer_season_override_{}", stamp));
        let src_dir = root.join("数码宝贝大冒险02 (2000)");
        let s02_dir = src_dir.join("Season 02");
        fs::create_dir_all(&s02_dir).unwrap();
        let sample = s02_dir.join("数码宝贝大冒险02 S01E46.mkv");
        fs::write(&sample, b"").unwrap();

        let (items_auto, cols_auto) = build_multi_folder_items(
            std::slice::from_ref(&src_dir),
            &root,
            "数码宝贝 (1999)",
            "tv",
            false,
            None,
            None,
            true,
            &[],
        );
        assert!(cols_auto.is_empty());
        assert!(items_auto[0]
            .dst
            .to_string_lossy()
            .contains("Season 01"));

        let (items_forced, cols_forced) = build_multi_folder_items(
            std::slice::from_ref(&src_dir),
            &root,
            "数码宝贝 (1999)",
            "tv",
            false,
            None,
            Some(2),
            true,
            &[],
        );
        assert!(cols_forced.is_empty());
        let dst_str = items_forced[0].dst.to_string_lossy().replace('\\', "/");
        assert!(dst_str.ends_with("数码宝贝 (1999)/Season 02/数码宝贝 S02E46.mkv"), "got {}", dst_str);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_reconcile_continuous_and_seasonal_offsets() {
        // 1. Long-running continuous TMDB show (like One Piece) with drifted season folders & .5 recap
        let items = vec![
            RenameItem {
                src: PathBuf::from("01.mkv"),
                dst: PathBuf::from("/tmp/航海王 (1999)/Season 01/航海王 S01E01.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("61.mkv"),
                dst: PathBuf::from("/tmp/航海王 (1999)/Season 01/航海王 S01E61.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("62.mkv"),
                dst: PathBuf::from("/tmp/航海王 (1999)/Season 02/航海王 S02E62.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("892.mkv"),
                dst: PathBuf::from("/tmp/航海王 (1999)/Season 20/航海王 S20E892.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("1073.5.mkv"),
                dst: PathBuf::from("/tmp/航海王 (1999)/Season 20/航海王 S20E1073.5.mkv"),
                kind: ItemKind::Main,
            },
        ];

        let seasons_map = |_tid| {
            let mut m = HashMap::new();
            m.insert(1, "第1季 东海篇".to_string());
            m.insert(2, "第2季 伟大航路突入篇".to_string());
            m.insert(20, "第20季 世界会议篇".to_string());
            m.insert(21, "第21季 和之国篇".to_string());
            Ok(m)
        };
        let ep_fetcher = |_tid, s| {
            let mut m = HashMap::new();
            match s {
                1 => {
                    m.insert(1, "我是路飞！".into());
                    m.insert(61, "愤怒的结局！".into());
                }
                2 => {
                    m.insert(62, "第一道障碍？".into());
                }
                20 => {
                    m.insert(878, "世界震惊".into());
                }
                21 => {
                    m.insert(892, "和之国".into());
                    m.insert(1073, "无路可逃！".into());
                }
                _ => {}
            }
            Ok(m)
        };

        let (rec, cols, warn) = reconcile_tv_episodes(items, seasons_map, ep_fetcher, 37854);
        assert!(cols.is_empty() && warn.is_none());
        let (titled, cols2, _) = attach_episode_titles(rec, ep_fetcher, 37854);
        assert!(cols2.is_empty());

        let paths: Vec<String> = titled
            .iter()
            .map(|x| x.dst.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.iter().any(|p| p.ends_with("Season 02/航海王 S02E62 [第一道障碍？].mkv")), "{:?}", paths);
        assert!(paths.iter().any(|p| p.ends_with("Season 21/航海王 S21E892 [和之国].mkv")), "{:?}", paths);
        assert!(paths.iter().any(|p| p.ends_with("Season 21/航海王 S21E1073.5.mkv")), "{:?}", paths);

        // 2. Standard 1-based seasonal TMDB show where local Season 2 used absolute numbering 13..14
        let s_items = vec![
            RenameItem {
                src: PathBuf::from("12.mkv"),
                dst: PathBuf::from("/tmp/Show (2020)/Season 01/Show S01E12.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("13.mkv"),
                dst: PathBuf::from("/tmp/Show (2020)/Season 02/Show S02E13.mkv"),
                kind: ItemKind::Main,
            },
        ];
        let (s_rec, _, _) = reconcile_tv_episodes(
            s_items,
            |_| Ok(HashMap::from([(1, "第1季".into()), (2, "第2季".into())])),
            |_, s| {
                if s == 1 {
                    Ok(HashMap::from([(1, "Ep1".into()), (12, "Ep12".into())]))
                } else {
                    Ok(HashMap::from([(1, "S2Ep1".into()), (2, "S2Ep2".into())]))
                }
            },
            999,
        );
        assert!(s_rec.iter().any(|x| x.dst.to_string_lossy().ends_with("Show S02E01.mkv")));
    }

    #[test]
    fn test_decimal_ep_preserves_explicit_season() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("tmdb_renamer_dec_{}", stamp));
        let s20 = root.join("海贼王 (1999)").join("Season 20");
        fs::create_dir_all(&s20).unwrap();
        fs::write(s20.join("海贼王 S20E1073.5.mkv"), b"").unwrap();
        let (items, _) = build_multi_folder_items(
            std::slice::from_ref(&root.join("海贼王 (1999)")),
            &root,
            "航海王 (1999)",
            "tv",
            false,
            None,
            None,
            false,
            &[],
        );
        let dst = items[0].dst.to_string_lossy().replace('\\', "/");
        assert!(dst.ends_with("Season 20/航海王 S20E1073.5.mkv"), "got {}", dst);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_flat_absolute_pack_splits_into_tmdb_seasons_like_gintama() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("tmdb_renamer_gintama_{}", stamp));
        let src = root.join("银魂");
        fs::create_dir_all(&src).unwrap();
        for name in [
            "[Anime Time] Gintama - 001-002.mkv",
            "[Anime Time] Gintama - 003.mkv",
            "[Anime Time] Gintama - 050.mkv",
            "[Anime Time] Gintama - 100.mkv",
        ] {
            fs::write(src.join(name), b"").unwrap();
        }

        let (items, cols) = build_multi_folder_items(
            std::slice::from_ref(&src),
            &root,
            "银魂 (2006)",
            "tv",
            false,
            None,
            None,
            false,
            &[],
        );
        assert!(cols.is_empty());

        let seasons_fn = |_tid| {
            Ok(HashMap::from([
                (1, "其之壹".into()),
                (2, "其之贰".into()),
                (3, "其之叁".into()),
            ]))
        };
        let ep_fn = |_tid, s| match s {
            1 => Ok((1..=49).map(|i| (i, format!("S1E{}", i))).collect()),
            2 => Ok((1..=50).map(|i| (i, format!("S2E{}", i))).collect()),
            3 => Ok((1..=51).map(|i| (i, format!("S3E{}", i))).collect()),
            _ => Ok(HashMap::new()),
        };

        let (rec, cols2, warn) = reconcile_tv_episodes(items, seasons_fn, ep_fn, 57041);
        assert!(cols2.is_empty() && warn.is_none());
        let (titled, cols3, _) = attach_episode_titles(rec, ep_fn, 57041);
        assert!(cols3.is_empty());

        let paths: Vec<String> = titled
            .iter()
            .map(|x| x.dst.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(paths.iter().any(|p| p.ends_with("Season 01/银魂 S01E01-E02 [S1E1].mkv")), "{:?}", paths);
        assert!(paths.iter().any(|p| p.ends_with("Season 01/银魂 S01E03 [S1E3].mkv")), "{:?}", paths);
        assert!(paths.iter().any(|p| p.ends_with("Season 02/银魂 S02E01 [S2E1].mkv")), "{:?}", paths);
        assert!(paths.iter().any(|p| p.ends_with("Season 03/银魂 S03E01 [S3E1].mkv")), "{:?}", paths);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_reconcile_seasonal_offset_with_missing_episode() {
        // S01 has 1..12, S02 starts at 14 (missing ep 13) -> should still offset by 12 to S02E02
        let items = vec![
            RenameItem {
                src: PathBuf::from("12.mkv"),
                dst: PathBuf::from("/tmp/Show (2020)/Season 01/Show S01E12.mkv"),
                kind: ItemKind::Main,
            },
            RenameItem {
                src: PathBuf::from("14.mkv"),
                dst: PathBuf::from("/tmp/Show (2020)/Season 02/Show S02E14.mkv"),
                kind: ItemKind::Main,
            },
        ];
        let (rec, cols, _) = reconcile_tv_episodes(
            items,
            |_| Ok(HashMap::from([(1, "第1季".into()), (2, "第2季".into())])),
            |_, s| {
                if s == 1 {
                    Ok((1..=12).map(|i| (i, format!("Ep{}", i))).collect())
                } else {
                    Ok((1..=12).map(|i| (i, format!("S2Ep{}", i))).collect())
                }
            },
            888,
        );
        assert!(cols.is_empty());
        assert!(
            rec.iter().any(|x| x.dst.to_string_lossy().ends_with("Show S02E02.mkv")),
            "{:?}",
            rec.iter().map(|x| &x.dst).collect::<Vec<_>>()
        );
    }
}
