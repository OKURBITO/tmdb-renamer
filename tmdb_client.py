"""TMDB v3 API client supporting both API Key and Bearer Read Access Token."""

import json
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import List, Optional


@dataclass
class TMDBMatch:
    tmdb_id: int
    media_type: str  # "tv" or "movie"
    title: str       # Chinese preferred, fallback to English/original
    original_title: str
    year: str
    overview: str = ""

    @property
    def display_name(self) -> str:
        name = self.title or self.original_title
        return f"{name} ({self.year})" if self.year else name


class TMDBClient:
    BASE_URL = "https://api.themoviedb.org/3"

    def __init__(self, api_key_or_token: str, proxy: str = "", timeout: int = 12):
        self.credential = api_key_or_token.strip()
        self.proxy = proxy.strip()
        self.timeout = timeout
        self._season_cache: dict = {}
        self._tv_seasons_cache: dict = {}

    def get_seasons_names(self, tmdb_id: int) -> dict:
        """Fetch {season_number: season_name} for a TV show (cached per client)."""
        if tmdb_id in self._tv_seasons_cache:
            return self._tv_seasons_cache[tmdb_id]
        try:
            data = self._get(f"/tv/{tmdb_id}", {"language": "zh-CN"})
        except Exception:
            self._tv_seasons_cache[tmdb_id] = {}
            return {}
        mapping = {}
        for s in data.get("seasons", []):
            num = s.get("season_number")
            name = (s.get("name") or "").strip()
            if num is not None and name:
                mapping[int(num)] = name
        self._tv_seasons_cache[tmdb_id] = mapping
        return mapping

    def get_season_episodes(self, tmdb_id: int, season_number: int) -> dict:
        """Fetch {episode_number: chinese_title} for a TV season (cached per client)."""
        cache_key = (tmdb_id, season_number)
        if cache_key in self._season_cache:
            return self._season_cache[cache_key]
        try:
            data = self._get(f"/tv/{tmdb_id}/season/{season_number}", {"language": "zh-CN"})
        except Exception:
            self._season_cache[cache_key] = {}
            return {}
        mapping = {}
        for ep in data.get("episodes", []):
            num = ep.get("episode_number")
            name = (ep.get("name") or "").strip()
            if num is not None and name:
                mapping[int(num)] = name
        self._season_cache[cache_key] = mapping
        return mapping

    def _build_opener(self) -> urllib.request.OpenerDirector:
        if self.proxy:
            handler = urllib.request.ProxyHandler({"http": self.proxy, "https": self.proxy})
            return urllib.request.build_opener(handler)
        return urllib.request.build_opener()

    def _get(self, path: str, params: Optional[dict] = None) -> dict:
        params = dict(params or {})
        headers = {"Accept": "application/json", "User-Agent": "TMDB-Media-Renamer/1.0"}
        is_bearer = len(self.credential) > 40 or self.credential.startswith("eyJ")
        if is_bearer:
            headers["Authorization"] = f"Bearer {self.credential}"
        else:
            params["api_key"] = self.credential

        qs = urllib.parse.urlencode(params)
        url = f"{self.BASE_URL}{path}?{qs}" if qs else f"{self.BASE_URL}{path}"
        req = urllib.request.Request(url, headers=headers, method="GET")
        opener = self._build_opener()
        with opener.open(req, timeout=self.timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))

    def test_connection(self) -> bool:
        data = self._get("/configuration")
        return "images" in data

    def search(self, query: str, media_type: str = "auto", year: Optional[str] = None) -> List[TMDBMatch]:
        if not query.strip():
            return []
        endpoint = "/search/multi"
        if media_type == "tv":
            endpoint = "/search/tv"
        elif media_type == "movie":
            endpoint = "/search/movie"

        params = {"query": query.strip(), "language": "zh-CN", "include_adult": "false"}
        if year and media_type in ("tv", "movie"):
            params["first_air_date_year" if media_type == "tv" else "year"] = year

        data = self._get(endpoint, params)
        results = []
        for item in data.get("results", []):
            mtype = item.get("media_type") or media_type
            if mtype not in ("tv", "movie"):
                continue
            title_zh = (item.get("name") if mtype == "tv" else item.get("title")) or ""
            orig = (item.get("original_name") if mtype == "tv" else item.get("original_title")) or ""
            date_str = (item.get("first_air_date") if mtype == "tv" else item.get("release_date")) or ""
            yr = date_str[:4] if len(date_str) >= 4 else ""
            results.append(
                TMDBMatch(
                    tmdb_id=int(item["id"]),
                    media_type=mtype,
                    title=title_zh.strip() or orig.strip(),
                    original_title=orig.strip(),
                    year=yr,
                    overview=(item.get("overview") or "")[:120],
                )
            )
        return results
