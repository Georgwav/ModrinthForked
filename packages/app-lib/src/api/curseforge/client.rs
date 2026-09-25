//! The CurseForge REST API: the key, the response shapes and requests.
//!
//! Docs: <https://docs.curseforge.com/rest-api/>

use crate::State;
use crate::util::fetch::fetch_advanced;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub(crate) const API_URL: &str = "https://api.curseforge.com";
/// Minecraft's id on CurseForge.
pub(crate) const GAME_ID: u32 = 432;
pub(crate) const CLASS_MODS: u32 = 6;
pub(crate) const CLASS_MODPACKS: u32 = 4471;
pub(crate) const CLASS_RESOURCE_PACKS: u32 = 12;
pub(crate) const CLASS_SHADERS: u32 = 6552;
pub(crate) const CLASS_DATA_PACKS: u32 = 6945;
/// `relationType` of a dependency the file can't work without.
pub(crate) const RELATION_REQUIRED: u32 = 3;
/// `algo` of a SHA-1 file hash.
const HASH_SHA1: u32 = 1;

/// The API key this build was made with, or for development the
/// `CURSEFORGE_API_KEY` environment variable. Never committed.
pub fn api_key() -> Option<String> {
    option_env!("CURSEFORGE_API_KEY")
        .map(str::to_string)
        .or_else(|| std::env::var("CURSEFORGE_API_KEY").ok())
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

pub(crate) fn not_set_up() -> crate::Error {
    crate::ErrorKind::InputError(
        "CurseForge isn't set up in this build".to_string(),
    )
    .into()
}

#[derive(Deserialize, Debug)]
pub(crate) struct DataResponse<T> {
    pub data: T,
}

#[derive(Deserialize, Debug)]
pub(crate) struct PagedResponse<T> {
    pub data: Vec<T>,
    #[serde(default)]
    pub pagination: Option<Pagination>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pagination {
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub page_size: u32,
    #[serde(default)]
    pub result_count: u32,
    #[serde(default)]
    pub total_count: u64,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Mod {
    pub id: u32,
    pub name: String,
    pub slug: String,
    pub links: Option<ModLinks>,
    pub summary: String,
    pub download_count: f64,
    pub class_id: Option<u32>,
    pub authors: Vec<ModAuthor>,
    pub logo: Option<ModAsset>,
    pub main_file_id: u32,
    pub latest_files_indexes: Vec<FileIndex>,
    pub date_modified: Option<String>,
    pub date_created: Option<String>,
    pub allow_mod_distribution: Option<bool>,
    pub categories: Vec<ModCategory>,
    pub thumbs_up_count: u64,
    pub screenshots: Vec<ModAsset>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ModCategory {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ModLinks {
    pub website_url: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct ModAuthor {
    pub name: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ModAsset {
    pub thumbnail_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct FileIndex {
    pub game_version: String,
    pub mod_loader: Option<u32>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct File {
    pub id: u32,
    pub mod_id: u32,
    pub is_available: Option<bool>,
    pub display_name: String,
    pub file_name: String,
    pub release_type: u32,
    pub hashes: Vec<FileHash>,
    pub file_date: String,
    pub file_length: u64,
    pub download_url: Option<String>,
    pub game_versions: Vec<String>,
    pub dependencies: Vec<FileDependency>,
    pub is_server_pack: Option<bool>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub(crate) struct FileHash {
    pub value: String,
    pub algo: u32,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct FileDependency {
    pub mod_id: u32,
    pub relation_type: u32,
}

impl File {
    pub fn sha1(&self) -> Option<&str> {
        self.hashes
            .iter()
            .find(|hash| hash.algo == HASH_SHA1 && hash.value.len() == 40)
            .map(|hash| hash.value.as_str())
    }
}

/// The CurseForge `ModLoaderType` for a loader name.
pub(crate) fn mod_loader_type(loader: &str) -> Option<u32> {
    match loader.to_ascii_lowercase().as_str() {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

/// The loader name for a CurseForge `ModLoaderType`.
pub(crate) fn mod_loader_name(loader_type: u32) -> Option<&'static str> {
    match loader_type {
        1 => Some("forge"),
        4 => Some("fabric"),
        5 => Some("quilt"),
        6 => Some("neoforge"),
        _ => None,
    }
}

async fn request<T: DeserializeOwned>(
    method: Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> crate::Result<T> {
    let key = api_key().ok_or_else(not_set_up)?;
    let state = State::get().await?;
    let bytes = fetch_advanced(
        method,
        &format!("{API_URL}{path}"),
        None,
        body,
        Some(("x-api-key", &key)),
        None,
        None,
        None,
        &state.api_semaphore,
        &state.pool,
    )
    .await?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) async fn get<T: DeserializeOwned>(path: &str) -> crate::Result<T> {
    request(Method::GET, path, None).await
}

pub(crate) async fn post<T: DeserializeOwned>(
    path: &str,
    body: serde_json::Value,
) -> crate::Result<T> {
    request(Method::POST, path, Some(body)).await
}

/// Several mods by id. The API takes a batch at a time.
pub(crate) async fn get_mods(ids: &[u32]) -> crate::Result<Vec<Mod>> {
    let mut mods = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let response: DataResponse<Vec<Mod>> = post(
            "/v1/mods",
            serde_json::json!({ "modIds": chunk, "filterPcOnly": true }),
        )
        .await?;
        mods.extend(response.data);
    }
    Ok(mods)
}

/// Several files by id.
pub(crate) async fn get_files(ids: &[u32]) -> crate::Result<Vec<File>> {
    let mut files = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let response: DataResponse<Vec<File>> =
            post("/v1/mods/files", serde_json::json!({ "fileIds": chunk }))
                .await?;
        files.extend(response.data);
    }
    Ok(files)
}

/// A project's description, as HTML.
pub(crate) async fn get_description(id: u32) -> crate::Result<String> {
    Ok(
        get::<DataResponse<String>>(&format!("/v1/mods/{id}/description"))
            .await?
            .data,
    )
}

pub(crate) async fn get_mod(id: u32) -> crate::Result<Mod> {
    Ok(get::<DataResponse<Mod>>(&format!("/v1/mods/{id}"))
        .await?
        .data)
}

/// The website page of a file, for downloading it by hand.
pub(crate) fn file_page_url(
    website_url: Option<&str>,
    mod_id: u32,
    file_id: u32,
) -> String {
    match website_url {
        Some(url) if !url.is_empty() => {
            format!("{}/files/{file_id}", url.trim_end_matches('/'))
        }
        _ => project_page_url(mod_id),
    }
}

/// A project's page when only its id is known; CurseForge redirects it.
pub(crate) fn project_page_url(mod_id: u32) -> String {
    format!("https://www.curseforge.com/projects/{mod_id}")
}
