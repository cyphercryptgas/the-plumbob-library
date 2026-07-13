//! A small, honest CurseForge client. Three endpoints, blocking (always
//! called inside `spawn_blocking`), rustls, generous timeouts, and error
//! messages a person can act on. Only anonymous fingerprints and mod ids
//! ever go over the wire; the API key rides in a header and lives nowhere
//! but the local database.

use serde::Deserialize;
use std::time::Duration;

const BASE: &str = "https://api.curseforge.com";

pub struct CurseClient {
    http: reqwest::blocking::Client,
    key: String,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GamesPage {
    data: Vec<Game>,
    pagination: Pagination,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    index: i64,
    page_size: i64,
    total_count: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Game {
    id: i64,
    name: String,
    slug: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintData {
    exact_matches: Vec<FingerprintMatch>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintMatch {
    file: CurseFile,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseFile {
    pub id: i64,
    pub mod_id: i64,
    pub file_name: String,
    pub file_date: String,
    pub file_fingerprint: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseFileDetail {
    pub id: i64,
    pub file_name: String,
    #[serde(default)]
    pub file_length: Option<i64>,
    /// None when the author has disabled third-party API distribution.
    #[serde(default)]
    pub download_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseMod {
    pub id: i64,
    pub game_id: i64,
    pub name: String,
    #[serde(default)]
    pub links: ModLinks,
    #[serde(default)]
    pub latest_files: Vec<CurseFile>,
    #[serde(default)]
    pub authors: Vec<Author>,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub name: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLinks {
    #[serde(default)]
    pub website_url: Option<String>,
}

impl CurseClient {
    pub fn new(key: &str) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent("MotherlodeManager")
            .build()
            .map_err(|e| format!("Could not start the HTTP client: {e}"))?;
        Ok(Self {
            http,
            key: key.to_string(),
        })
    }

    fn friendly(status: reqwest::StatusCode) -> String {
        match status.as_u16() {
            401 => "CurseForge rejected the API key. Check it in Settings → \
                    Connections."
                .to_string(),
            // Cloudflare answers request storms with 403 — it looks like a
            // bad key and isn't. It clears on its own, usually within an
            // hour, and all progress is cached.
            403 => "CurseForge is blocking requests right now — this often \
                    follows a very large run and clears on its own (usually \
                    within the hour). Everything found so far is cached; try \
                    again later."
                .to_string(),
            429 => "CurseForge is rate-limiting requests — wait a minute and \
                    try again."
                .to_string(),
            s => format!("CurseForge answered with HTTP {s}."),
        }
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let resp = self
            .http
            .get(format!("{BASE}{path}"))
            .header("x-api-key", &self.key)
            .send()
            .map_err(|e| format!("Could not reach CurseForge: {e}"))?;
        if !resp.status().is_success() {
            return Err(Self::friendly(resp.status()));
        }
        resp.json()
            .map_err(|e| format!("CurseForge sent an unexpected reply: {e}"))
    }

    fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, String> {
        let resp = self
            .http
            .post(format!("{BASE}{path}"))
            .header("x-api-key", &self.key)
            .json(body)
            .send()
            .map_err(|e| format!("Could not reach CurseForge: {e}"))?;
        if !resp.status().is_success() {
            return Err(Self::friendly(resp.status()));
        }
        resp.json()
            .map_err(|e| format!("CurseForge sent an unexpected reply: {e}"))
    }

    /// Find The Sims 4's game id by walking the games list — never
    /// hardcoded, so a catalogue reshuffle can't strand the radar.
    pub fn find_sims4_game_id(&self) -> Result<i64, String> {
        let mut index = 0i64;
        loop {
            let page: GamesPage =
                self.get(&format!("/v1/games?index={index}&pageSize=50"))?;
            if let Some(g) = page.data.iter().find(|g| {
                g.slug.eq_ignore_ascii_case("sims4")
                    || g.slug.eq_ignore_ascii_case("the-sims-4")
                    || g.name.eq_ignore_ascii_case("The Sims 4")
            }) {
                return Ok(g.id);
            }
            index += page.data.len() as i64;
            if index >= page.pagination.total_count || page.data.is_empty() {
                return Err(
                    "Couldn't find The Sims 4 in CurseForge's game list."
                        .to_string(),
                );
            }
            let _ = page.pagination.index;
            let _ = page.pagination.page_size;
        }
    }

    /// Exact-match a batch of fingerprints for one game.
    pub fn match_fingerprints(
        &self,
        game_id: i64,
        fingerprints: &[u32],
    ) -> Result<Vec<CurseFile>, String> {
        let body = serde_json::json!({ "fingerprints": fingerprints });
        let env: Envelope<FingerprintData> =
            self.post(&format!("/v1/fingerprints/{game_id}"), &body)?;
        Ok(env.data.exact_matches.into_iter().map(|m| m.file).collect())
    }

    /// Resolve mod names, links, and latest files for a batch of mod ids.
    pub fn get_mods(&self, mod_ids: &[i64]) -> Result<Vec<CurseMod>, String> {
        let body = serde_json::json!({ "modIds": mod_ids });
        let env: Envelope<Vec<CurseMod>> = self.post("/v1/mods", &body)?;
        Ok(env.data)
    }
}

impl CurseClient {
    /// Free-text search within one game — the name radar's workhorse.
    pub fn search_mods(
        &self,
        game_id: i64,
        term: &str,
        page_size: u32,
    ) -> Result<Vec<CurseMod>, String> {
        let resp = self
            .http
            .get(format!("{BASE}/v1/mods/search"))
            .header("x-api-key", &self.key)
            .query(&[
                ("gameId", game_id.to_string()),
                ("searchFilter", term.to_string()),
                ("pageSize", page_size.to_string()),
            ])
            .send()
            .map_err(|e| format!("Could not reach CurseForge: {e}"))?;
        if !resp.status().is_success() {
            return Err(Self::friendly(resp.status()));
        }
        let env: Envelope<Vec<CurseMod>> = resp
            .json()
            .map_err(|e| format!("CurseForge sent an unexpected reply: {e}"))?;
        Ok(env.data)
    }

    /// A popular Sims 4 mod straight from CurseForge, carrying a
    /// fingerprint *they* computed — the perfect probe input.
    pub fn sample_mod(&self, game_id: i64) -> Result<Option<CurseMod>, String> {
        let env: Envelope<Vec<CurseMod>> =
            self.get(&format!("/v1/mods/search?gameId={game_id}&pageSize=5"))?;
        Ok(env
            .data
            .into_iter()
            .find(|m| m.latest_files.iter().any(|f| f.file_fingerprint != 0)))
    }
}

impl CurseClient {
    pub fn get_file(&self, mod_id: i64, file_id: i64) -> Result<CurseFileDetail, String> {
        let env: Envelope<CurseFileDetail> =
            self.get(&format!("/v1/mods/{mod_id}/files/{file_id}"))?;
        Ok(env.data)
    }

    /// Fetch the file bytes from the CDN url the API handed us. No API
    /// key travels here — the url is pre-signed.
    pub fn download(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
        let resp = self
            .http
            .get(url)
            .send()
            .map_err(|e| format!("Download failed to start: {e}"))?;
        if !resp.status().is_success() {
            return Err(Self::friendly(resp.status()));
        }
        if let Some(len) = resp.content_length() {
            if len > max_bytes {
                return Err(format!(
                    "That file is {len} bytes — larger than the {max_bytes}-byte safety cap."
                ));
            }
        }
        let bytes = resp
            .bytes()
            .map_err(|e| format!("Download interrupted: {e}"))?;
        if bytes.is_empty() {
            return Err("The download came back empty.".to_string());
        }
        if bytes.len() as u64 > max_bytes {
            return Err("The download exceeded the safety cap.".to_string());
        }
        Ok(bytes.to_vec())
    }
}
