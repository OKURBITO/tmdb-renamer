use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const BASE_URL: &str = "https://api.themoviedb.org/3";

#[derive(Debug, Clone)]
pub struct TMDBMatch {
    pub tmdb_id: i64,
    pub media_type: String, // "tv" or "movie"
    pub title: String,
    pub original_title: String,
    pub year: String,
    pub overview: String,
}

impl TMDBMatch {
    pub fn display_name(&self) -> String {
        let name = if !self.title.is_empty() {
            &self.title
        } else {
            &self.original_title
        };
        if !self.year.is_empty() {
            format!("{} ({})", name, self.year)
        } else {
            name.to_string()
        }
    }
}

#[derive(Clone)]
pub struct TMDBClient {
    credential: String,
    agent: ureq::Agent,
    max_retries: usize,
    season_cache: Arc<Mutex<HashMap<(i64, i32), HashMap<i32, String>>>>,
    tv_seasons_cache: Arc<Mutex<HashMap<i64, HashMap<i32, String>>>>,
}

impl TMDBClient {
    pub fn new(api_key_or_token: &str, proxy: &str) -> Result<Self, String> {
        let cred = api_key_or_token.trim().to_string();
        if cred.is_empty() {
            return Err("请先输入 TMDB API Key 或 Read Access Token。".to_string());
        }

        let mut builder = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .user_agent("TMDB-Media-Renamer-RS/2.2");

        #[cfg(windows)]
        {
            let tls = native_tls::TlsConnector::new()
                .map_err(|e| format!("TLS 初始化失败: {}", e))?;
            builder = builder.tls_connector(Arc::new(tls));
        }

        let proxy_trimmed = proxy.trim();
        if !proxy_trimmed.is_empty() {
            let p = ureq::Proxy::new(proxy_trimmed)
                .map_err(|e| format!("代理地址无效 ({}): {}", proxy_trimmed, e))?;
            builder = builder.proxy(p);
        }

        Ok(Self {
            credential: cred,
            agent: builder.build(),
            max_retries: 2,
            season_cache: Arc::new(Mutex::new(HashMap::new())),
            tv_seasons_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn get_json(&self, path: &str, params: &[(&str, &str)]) -> Result<Value, String> {
        let is_bearer = self.credential.len() > 40 || self.credential.starts_with("eyJ");
        let mut query_parts: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect();
        if !is_bearer {
            query_parts.push(format!("api_key={}", urlencoding::encode(&self.credential)));
        }

        let url = if query_parts.is_empty() {
            format!("{}{}", BASE_URL, path)
        } else {
            format!("{}{}?{}", BASE_URL, path, query_parts.join("&"))
        };

        let mut last_err = String::new();
        for attempt in 0..=self.max_retries {
            let mut req = self.agent.get(&url).set("Accept", "application/json");
            if is_bearer {
                req = req.set("Authorization", &format!("Bearer {}", self.credential));
            }

            match req.call() {
                Ok(resp) => {
                    return resp
                        .into_json::<Value>()
                        .map_err(|e| format!("JSON 解析失败: {}", e));
                }
                Err(ureq::Error::Status(code, resp)) => {
                    let retry_after = resp
                        .header("Retry-After")
                        .and_then(|s| s.parse::<f64>().ok());
                    last_err = format!("HTTP {}", code);
                    if matches!(code, 429 | 500 | 502 | 503 | 504) && attempt < self.max_retries {
                        let secs = retry_after
                            .map(|v| v.min(5.0))
                            .unwrap_or(0.8 * (attempt + 1) as f64);
                        thread::sleep(Duration::from_secs_f64(secs));
                        continue;
                    }
                    return Err(last_err);
                }
                Err(ureq::Error::Transport(t)) => {
                    last_err = format!("网络错误: {}", t);
                    if attempt < self.max_retries {
                        thread::sleep(Duration::from_secs_f64(0.6 * (attempt + 1) as f64));
                        continue;
                    }
                    return Err(last_err);
                }
            }
        }
        Err(last_err)
    }

    pub fn test_connection(&self) -> Result<bool, String> {
        let data = self.get_json("/configuration", &[])?;
        Ok(data.get("images").is_some())
    }

    /// Fetch {season_number: season_name} for a TV show (cached on success or 404).
    pub fn get_seasons_names(&self, tmdb_id: i64) -> Result<HashMap<i32, String>, String> {
        if let Ok(cache) = self.tv_seasons_cache.lock() {
            if let Some(hit) = cache.get(&tmdb_id) {
                return Ok(hit.clone());
            }
        }

        let path = format!("/tv/{}", tmdb_id);
        let data = match self.get_json(&path, &[("language", "zh-CN")]) {
            Ok(v) => v,
            Err(e) if e == "HTTP 404" => {
                if let Ok(mut cache) = self.tv_seasons_cache.lock() {
                    cache.insert(tmdb_id, HashMap::new());
                }
                return Ok(HashMap::new());
            }
            Err(e) => return Err(e),
        };

        let mut mapping = HashMap::new();
        if let Some(seasons) = data.get("seasons").and_then(|v| v.as_array()) {
            for s in seasons {
                if let (Some(num), Some(name)) = (
                    s.get("season_number").and_then(|v| v.as_i64()),
                    s.get("name").and_then(|v| v.as_str()),
                ) {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        mapping.insert(num as i32, trimmed.to_string());
                    }
                }
            }
        }

        if let Ok(mut cache) = self.tv_seasons_cache.lock() {
            cache.insert(tmdb_id, mapping.clone());
        }
        Ok(mapping)
    }

    pub fn get_season_episodes(
        &self,
        tmdb_id: i64,
        season_number: i32,
    ) -> Result<HashMap<i32, String>, String> {
        if let Ok(cache) = self.season_cache.lock() {
            if let Some(hit) = cache.get(&(tmdb_id, season_number)) {
                return Ok(hit.clone());
            }
        }

        let path = format!("/tv/{}/season/{}", tmdb_id, season_number);
        let data = match self.get_json(&path, &[("language", "zh-CN")]) {
            Ok(v) => v,
            Err(e) if e == "HTTP 404" => {
                if let Ok(mut cache) = self.season_cache.lock() {
                    cache.insert((tmdb_id, season_number), HashMap::new());
                }
                return Ok(HashMap::new());
            }
            Err(e) => return Err(e),
        };

        let mut mapping = HashMap::new();
        if let Some(eps) = data.get("episodes").and_then(|v| v.as_array()) {
            for ep in eps {
                if let (Some(num), Some(name)) = (
                    ep.get("episode_number").and_then(|v| v.as_i64()),
                    ep.get("name").and_then(|v| v.as_str()),
                ) {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        mapping.insert(num as i32, trimmed.to_string());
                    }
                }
            }
        }

        if let Ok(mut cache) = self.season_cache.lock() {
            cache.insert((tmdb_id, season_number), mapping.clone());
        }
        Ok(mapping)
    }

    pub fn search(
        &self,
        query: &str,
        media_type: &str,
        year: Option<&str>,
    ) -> Result<Vec<TMDBMatch>, String> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let endpoint = match media_type {
            "tv" => "/search/tv",
            "movie" => "/search/movie",
            _ => "/search/multi",
        };

        let mut params = vec![
            ("query", q),
            ("language", "zh-CN"),
            ("include_adult", "false"),
        ];
        if let Some(yr) = year {
            if media_type == "tv" {
                params.push(("first_air_date_year", yr));
            } else if media_type == "movie" {
                params.push(("year", yr));
            }
        }

        let data = self.get_json(endpoint, &params)?;
        let mut results = Vec::new();
        if let Some(arr) = data.get("results").and_then(|v| v.as_array()) {
            for item in arr {
                let mtype = item
                    .get("media_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or(media_type);
                if mtype != "tv" && mtype != "movie" {
                    continue;
                }
                let id = match item.get("id").and_then(|v| v.as_i64()) {
                    Some(v) => v,
                    None => continue,
                };
                let title_zh = if mtype == "tv" {
                    item.get("name")
                } else {
                    item.get("title")
                }
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();

                let orig = if mtype == "tv" {
                    item.get("original_name")
                } else {
                    item.get("original_title")
                }
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();

                let date_str = if mtype == "tv" {
                    item.get("first_air_date")
                } else {
                    item.get("release_date")
                }
                .and_then(|v| v.as_str())
                .unwrap_or("");

                let yr = if date_str.len() >= 4 {
                    date_str[..4].to_string()
                } else {
                    String::new()
                };

                let overview: String = item
                    .get("overview")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .chars()
                    .take(120)
                    .collect();

                results.push(TMDBMatch {
                    tmdb_id: id,
                    media_type: mtype.to_string(),
                    title: if !title_zh.is_empty() {
                        title_zh.to_string()
                    } else {
                        orig.to_string()
                    },
                    original_title: orig.to_string(),
                    year: yr,
                    overview,
                });
            }
        }
        Ok(results)
    }
}
