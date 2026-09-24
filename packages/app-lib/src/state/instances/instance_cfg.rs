//! Prism-style `instance.cfg` files.
//!
//! Every instance folder carries an `instance.cfg` describing the instance, so
//! the folder is self-describing and the launcher can rebuild its instance list
//! by scanning the instances directory instead of trusting only `app.db`.
//!
//! The file uses the same INI layout as Prism Launcher (`[General]` section,
//! `name`, `InstanceType`, `totalTimePlayed`, ...). Modrinth-specific data is
//! stored in `Modrinth*` keys, which Prism ignores. Unknown keys and sections
//! are preserved when the file is rewritten.

use crate::state::{InstanceLink, ModLoader};
use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub(crate) const INSTANCE_CFG_FILE_NAME: &str = "instance.cfg";
const GENERAL_SECTION: &str = "General";

const KEY_CONFIG_VERSION: &str = "ConfigVersion";
const KEY_INSTANCE_TYPE: &str = "InstanceType";
const KEY_NAME: &str = "name";
const KEY_LAST_LAUNCH_TIME: &str = "lastLaunchTime";
const KEY_TOTAL_TIME_PLAYED: &str = "totalTimePlayed";
const KEY_ID: &str = "ModrinthInstanceId";
const KEY_GAME_VERSION: &str = "ModrinthGameVersion";
const KEY_LOADER: &str = "ModrinthLoader";
const KEY_LOADER_VERSION: &str = "ModrinthLoaderVersion";
const KEY_CREATED: &str = "ModrinthCreated";
const KEY_MODIFIED: &str = "ModrinthModified";
const KEY_SUBMITTED_TIME_PLAYED: &str = "ModrinthSubmittedTimePlayed";
const KEY_RECENT_TIME_PLAYED: &str = "ModrinthRecentTimePlayed";
const KEY_LINK: &str = "ModrinthLink";

/// Folders and files that mark a directory as a Minecraft instance.
const INSTANCE_MARKERS: &[&str] = &[
    "mods",
    "saves",
    "config",
    "options.txt",
    "resourcepacks",
    "shaderpacks",
    "profile.json",
    "mmc-pack.json",
];

/// The instance description stored in `instance.cfg`.
#[derive(Clone, Debug)]
pub(crate) struct InstanceCfg {
    pub id: Option<String>,
    pub name: String,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub created: Option<DateTime<Utc>>,
    pub modified: Option<DateTime<Utc>>,
    pub last_played: Option<DateTime<Utc>>,
    pub submitted_time_played: u64,
    pub recent_time_played: u64,
    pub link: Option<InstanceLink>,
}

impl InstanceCfg {
    /// Reads the description from a parsed file. Returns `None` when the file
    /// has no Minecraft version, e.g. a native Prism instance whose version
    /// lives in `mmc-pack.json` instead.
    fn from_document(doc: &CfgDocument) -> Option<Self> {
        let game_version = doc.get(KEY_GAME_VERSION)?.trim().to_string();
        if game_version.is_empty() {
            return None;
        }
        let loader = doc
            .get(KEY_LOADER)
            .and_then(parse_loader)
            .unwrap_or(ModLoader::Vanilla);
        let total_time_played = doc
            .get(KEY_TOTAL_TIME_PLAYED)
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let submitted_time_played = doc
            .get(KEY_SUBMITTED_TIME_PLAYED)
            .and_then(|value| value.trim().parse::<u64>().ok());
        let recent_time_played = doc
            .get(KEY_RECENT_TIME_PLAYED)
            .and_then(|value| value.trim().parse::<u64>().ok());
        // Files written by Prism only carry the total, which is treated as
        // already submitted so it is never reported twice.
        let (submitted_time_played, recent_time_played) =
            match (submitted_time_played, recent_time_played) {
                (None, None) => (total_time_played, 0),
                (submitted, recent) => {
                    (submitted.unwrap_or(0), recent.unwrap_or(0))
                }
            };

        Some(Self {
            id: non_empty(doc.get(KEY_ID)),
            name: doc.get(KEY_NAME).unwrap_or_default().trim().to_string(),
            game_version,
            loader,
            loader_version: non_empty(doc.get(KEY_LOADER_VERSION)),
            created: doc.get(KEY_CREATED).and_then(parse_date),
            modified: doc.get(KEY_MODIFIED).and_then(parse_date),
            last_played: doc
                .get(KEY_LAST_LAUNCH_TIME)
                .and_then(|value| value.trim().parse::<i64>().ok())
                .filter(|millis| *millis > 0)
                .and_then(|millis| Utc.timestamp_millis_opt(millis).single()),
            submitted_time_played,
            recent_time_played,
            link: doc
                .get(KEY_LINK)
                .and_then(|value| serde_json::from_str(value).ok()),
        })
    }

    fn write_to_document(&self, doc: &mut CfgDocument) {
        if doc.get(KEY_CONFIG_VERSION).is_none() {
            doc.set(KEY_CONFIG_VERSION, "1.2");
        }
        if doc.get(KEY_INSTANCE_TYPE).is_none() {
            doc.set(KEY_INSTANCE_TYPE, "OneSix");
        }
        doc.set(KEY_NAME, &self.name);
        doc.set(
            KEY_LAST_LAUNCH_TIME,
            &self
                .last_played
                .map_or(0, |value| value.timestamp_millis())
                .to_string(),
        );
        doc.set(
            KEY_TOTAL_TIME_PLAYED,
            &self
                .submitted_time_played
                .saturating_add(self.recent_time_played)
                .to_string(),
        );
        doc.set_optional(KEY_ID, self.id.as_deref());
        doc.set(KEY_GAME_VERSION, &self.game_version);
        doc.set(KEY_LOADER, self.loader.as_str());
        doc.set_optional(KEY_LOADER_VERSION, self.loader_version.as_deref());
        doc.set_optional(
            KEY_CREATED,
            self.created.map(|value| value.to_rfc3339()).as_deref(),
        );
        doc.set_optional(
            KEY_MODIFIED,
            self.modified.map(|value| value.to_rfc3339()).as_deref(),
        );
        doc.set(
            KEY_SUBMITTED_TIME_PLAYED,
            &self.submitted_time_played.to_string(),
        );
        doc.set(KEY_RECENT_TIME_PLAYED, &self.recent_time_played.to_string());
        doc.set_optional(
            KEY_LINK,
            self.link
                .as_ref()
                .and_then(|link| serde_json::to_string(link).ok())
                .as_deref(),
        );
    }
}

/// The result of reading an instance folder's `instance.cfg`.
pub(crate) enum CfgRead {
    Missing,
    /// The file exists but has no Modrinth data (for example a file written by
    /// Prism itself).
    Foreign,
    Parsed(Box<InstanceCfg>),
}

pub(crate) fn instance_cfg_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(INSTANCE_CFG_FILE_NAME)
}

pub(crate) async fn read_instance_cfg(
    instance_dir: &Path,
) -> crate::Result<CfgRead> {
    let path = instance_cfg_path(instance_dir);
    let contents = match tokio::fs::read(&path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CfgRead::Missing);
        }
        Err(error) => {
            return Err(
                crate::util::io::IOError::with_path(error, &path).into()
            );
        }
    };
    let doc = CfgDocument::parse(&String::from_utf8_lossy(&contents));

    Ok(match InstanceCfg::from_document(&doc) {
        Some(cfg) => CfgRead::Parsed(Box::new(cfg)),
        None => CfgRead::Foreign,
    })
}

/// Writes `cfg` into the folder's `instance.cfg`, keeping any keys the file
/// already had. The file is only touched when its contents change, and is
/// replaced atomically so a crash never leaves a half-written file behind.
pub(crate) async fn write_instance_cfg(
    instance_dir: &Path,
    cfg: &InstanceCfg,
) -> crate::Result<bool> {
    let path = instance_cfg_path(instance_dir);
    let existing = match tokio::fs::read(&path).await {
        Ok(contents) => Some(String::from_utf8_lossy(&contents).into_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(
                crate::util::io::IOError::with_path(error, &path).into()
            );
        }
    };
    let mut doc = existing
        .as_deref()
        .map(CfgDocument::parse)
        .unwrap_or_default();
    cfg.write_to_document(&mut doc);
    let rendered = doc.render();

    if existing.as_deref() == Some(rendered.as_str()) {
        return Ok(false);
    }

    let temp_path = instance_dir.join(format!(".{INSTANCE_CFG_FILE_NAME}.tmp"));
    tokio::fs::write(&temp_path, rendered.as_bytes())
        .await
        .map_err(|error| {
            crate::util::io::IOError::with_path(error, &temp_path)
        })?;
    if let Err(error) = tokio::fs::rename(&temp_path, &path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(crate::util::io::IOError::with_path(error, &path).into());
    }

    Ok(true)
}

/// Whether a folder in the instances directory looks like a Minecraft
/// instance.
pub(crate) fn looks_like_instance(dir: &Path) -> bool {
    INSTANCE_MARKERS
        .iter()
        .any(|marker| dir.join(marker).exists())
}

/// Whether a folder is a native Prism/MultiMC instance, whose game files live
/// in a nested `.minecraft` folder. Those use a different layout and are left
/// untouched.
pub(crate) fn is_nested_prism_instance(dir: &Path) -> bool {
    (dir.join(".minecraft").is_dir() || dir.join("minecraft").is_dir())
        && !dir.join("mods").exists()
        && !dir.join("saves").exists()
}

/// Works out an instance description for a folder that has no usable
/// `instance.cfg`. Returns `None` when the Minecraft version cannot be
/// determined, since guessing it could upgrade and damage worlds.
pub(crate) fn infer_instance_cfg(
    dir: &Path,
    name: &str,
) -> Option<InstanceCfg> {
    let (game_version, loader, loader_version) = infer_from_legacy_profile(dir)
        .or_else(|| infer_from_mmc_pack(dir))
        .or_else(|| {
            let game_version = infer_game_version_from_worlds(dir)?;
            Some((game_version, infer_loader_from_mods(dir), None))
        })
        // Without a world there is nothing a wrong version could upgrade, so
        // weaker hints are fine from here on.
        .or_else(|| infer_from_logs(dir))
        .or_else(|| {
            let game_version = infer_game_version_from_mods(dir)?;
            Some((game_version, infer_loader_from_mods(dir), None))
        })?;

    Some(InstanceCfg {
        id: None,
        name: name.to_string(),
        game_version,
        loader,
        loader_version,
        created: None,
        modified: None,
        last_played: None,
        submitted_time_played: 0,
        recent_time_played: 0,
        link: None,
    })
}

type InferredVersion = (String, ModLoader, Option<String>);

/// Old Modrinth App builds stored instances as `profile.json`.
fn infer_from_legacy_profile(dir: &Path) -> Option<InferredVersion> {
    let contents = std::fs::read(dir.join("profile.json")).ok()?;
    let profile: serde_json::Value = serde_json::from_slice(&contents).ok()?;
    let metadata = profile.get("metadata")?;
    let game_version = metadata.get("game_version")?.as_str()?.to_string();
    let loader = metadata
        .get("loader")
        .and_then(|value| value.as_str())
        .and_then(parse_loader)
        .unwrap_or(ModLoader::Vanilla);
    let loader_version = metadata
        .get("loader_version")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    Some((game_version, loader, loader_version))
}

/// Prism/MultiMC store the game and loader versions in `mmc-pack.json`.
fn infer_from_mmc_pack(dir: &Path) -> Option<InferredVersion> {
    let contents = std::fs::read(dir.join("mmc-pack.json")).ok()?;
    let pack: serde_json::Value = serde_json::from_slice(&contents).ok()?;
    let components = pack.get("components")?.as_array()?;
    let mut game_version = None;
    let mut loader = ModLoader::Vanilla;
    let mut loader_version = None;

    for component in components {
        let Some(uid) = component.get("uid").and_then(|value| value.as_str())
        else {
            continue;
        };
        let version = component
            .get("version")
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let component_loader = match uid {
            "net.minecraft" => {
                game_version = version;
                continue;
            }
            "net.fabricmc.fabric-loader" => ModLoader::Fabric,
            "org.quiltmc.quilt-loader" => ModLoader::Quilt,
            "net.minecraftforge" => ModLoader::Forge,
            "net.neoforged" => ModLoader::NeoForge,
            _ => continue,
        };
        loader = component_loader;
        loader_version = version;
    }

    Some((game_version?, loader, loader_version))
}

/// Reads the version that last saved the most recently played world.
fn infer_game_version_from_worlds(dir: &Path) -> Option<String> {
    let saves = std::fs::read_dir(dir.join("saves")).ok()?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;

    for entry in saves.flatten() {
        let level_dat = entry.path().join("level.dat");
        let Ok(modified) = level_dat
            .metadata()
            .and_then(|metadata| metadata.modified())
        else {
            continue;
        };
        if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
            newest = Some((modified, level_dat));
        }
    }

    let (_, level_dat) = newest?;
    let raw = std::fs::read(level_dat).ok()?;
    let (root, _) = quartz_nbt::io::read_nbt(
        &mut Cursor::new(raw),
        quartz_nbt::io::Flavor::GzCompressed,
    )
    .ok()?;
    let data = root.get::<_, &quartz_nbt::NbtCompound>("Data").ok()?;
    let version = data.get::<_, &quartz_nbt::NbtCompound>("Version").ok()?;
    let name = version.get::<_, &str>("Name").ok()?;

    (!name.is_empty()).then(|| name.to_string())
}

/// Reads the versions the game logged at its last start (`logs/latest.log`),
/// or the version in the newest crash report.
fn infer_from_logs(dir: &Path) -> Option<InferredVersion> {
    if let Some(found) = read_prefix(&dir.join("logs/latest.log"))
        .and_then(|log| parse_log_versions(&log))
    {
        return Some(found);
    }
    let newest_report = std::fs::read_dir(dir.join("crash-reports"))
        .ok()?
        .flatten()
        .filter(|entry| {
            entry.path().extension().is_some_and(|ext| ext == "txt")
        })
        .max_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok())?;
    let report = read_prefix(&newest_report.path())?;
    let version = CRASH_REPORT_VERSION.captures(&report)?[1].to_string();
    Some((version, infer_loader_from_mods(dir), None))
}

fn read_prefix(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(512 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

static FABRIC_LOG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Loading Minecraft (\S+) with (Fabric|Quilt) Loader (\S+)")
        .unwrap()
});
static FML_ARG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"--fml\.(mcVersion|forgeVersion|neoForgeVersion), ([^,\]\s]+)")
        .unwrap()
});
static CRASH_REPORT_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Minecraft Version: (\S+)").unwrap());

pub(crate) fn parse_log_versions(log: &str) -> Option<InferredVersion> {
    if let Some(captures) = FABRIC_LOG.captures(log) {
        let loader = if &captures[2] == "Quilt" {
            ModLoader::Quilt
        } else {
            ModLoader::Fabric
        };
        return Some((
            captures[1].to_string(),
            loader,
            Some(captures[3].to_string()),
        ));
    }

    let (mut game_version, mut forge, mut neoforge) = (None, None, None);
    for captures in FML_ARG.captures_iter(log) {
        let value = Some(captures[2].to_string());
        match &captures[1] {
            "mcVersion" => game_version = game_version.or(value),
            "forgeVersion" => forge = forge.or(value),
            _ => neoforge = neoforge.or(value),
        }
    }
    let game_version = game_version?;
    Some(match (neoforge, forge) {
        (Some(version), _) => {
            (game_version, ModLoader::NeoForge, Some(version))
        }
        (None, forge) => (game_version, ModLoader::Forge, forge),
    })
}

static EXACT_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[~=]?(\d+(?:\.\d+)+)$").unwrap());
static TOML_EXACT_MINECRAFT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)modId\s*=\s*"minecraft".*?versionRange\s*=\s*"\[(\d+(?:\.\d+)+)\]""#,
    )
    .unwrap()
});

/// The Minecraft version most mods in `mods/` are pinned to.
fn infer_game_version_from_mods(dir: &Path) -> Option<String> {
    let mut votes: HashMap<String, usize> = HashMap::new();
    for entry in std::fs::read_dir(dir.join("mods")).ok()?.flatten().take(64) {
        if let Some(version) = pinned_minecraft_version(&entry.path()) {
            *votes.entry(version).or_default() += 1;
        }
    }
    votes
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(version, _)| version)
}

fn pinned_minecraft_version(jar: &Path) -> Option<String> {
    if !jar
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
    {
        return None;
    }
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(jar).ok()?).ok()?;
    let mut read = |name: &str| -> Option<String> {
        let mut file = archive.by_name(name).ok()?;
        let mut text = String::new();
        file.read_to_string(&mut text).ok()?;
        Some(text)
    };

    if let Some(text) = read("fabric.mod.json") {
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        let requirement = json.get("depends")?.get("minecraft")?;
        let requirement = match requirement {
            serde_json::Value::String(value) => value.clone(),
            serde_json::Value::Array(values) if values.len() == 1 => {
                values[0].as_str()?.to_string()
            }
            _ => return None,
        };
        return Some(
            EXACT_VERSION.captures(requirement.trim())?[1].to_string(),
        );
    }
    let toml = read("META-INF/neoforge.mods.toml")
        .or_else(|| read("META-INF/mods.toml"))?;
    Some(TOML_EXACT_MINECRAFT.captures(&toml)?[1].to_string())
}

/// Detects the mod loader from the metadata files inside the mod jars.
fn infer_loader_from_mods(dir: &Path) -> ModLoader {
    let Ok(mods) = std::fs::read_dir(dir.join("mods")) else {
        return ModLoader::Vanilla;
    };
    let mut counts = [0usize; 4];
    let loaders = [
        ModLoader::Fabric,
        ModLoader::Quilt,
        ModLoader::Forge,
        ModLoader::NeoForge,
    ];

    for entry in mods.flatten().take(64) {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            continue;
        }
        let Ok(archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
            continue;
        };
        let has = |name: &str| archive.index_for_name(name).is_some();

        let index = if has("quilt.mod.json") {
            1
        } else if has("fabric.mod.json") {
            0
        } else if has("META-INF/neoforge.mods.toml") {
            3
        } else if has("META-INF/mods.toml") {
            2
        } else {
            continue;
        };
        counts[index] += 1;
    }

    counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .max_by_key(|(_, count)| **count)
        .map_or(ModLoader::Vanilla, |(index, _)| loaders[index])
}

pub(crate) fn parse_loader(value: &str) -> Option<ModLoader> {
    match value.trim().to_ascii_lowercase().as_str() {
        "vanilla" => Some(ModLoader::Vanilla),
        "forge" => Some(ModLoader::Forge),
        "fabric" => Some(ModLoader::Fabric),
        "quilt" => Some(ModLoader::Quilt),
        "neoforge" | "neo" => Some(ModLoader::NeoForge),
        _ => None,
    }
}

fn parse_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// A minimal INI document compatible with the QSettings files Prism writes.
/// Lines that are not touched are written back unchanged.
#[derive(Debug, Default)]
struct CfgDocument {
    lines: Vec<CfgLine>,
}

#[derive(Debug)]
enum CfgLine {
    Section(String),
    Entry { key: String, value: String },
    Other(String),
}

impl CfgDocument {
    fn parse(contents: &str) -> Self {
        let lines = contents
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    CfgLine::Section(trimmed[1..trimmed.len() - 1].to_string())
                } else if !trimmed.starts_with(';')
                    && !trimmed.starts_with('#')
                    && let Some((key, value)) = line.split_once('=')
                {
                    CfgLine::Entry {
                        key: key.trim().to_string(),
                        value: unescape_value(value.trim()),
                    }
                } else {
                    CfgLine::Other(line.to_string())
                }
            })
            .collect();

        Self { lines }
    }

    /// Index range of the lines holding the instance settings: the
    /// `[General]` section, or the top of the file for older files without
    /// sections.
    fn settings_range(&self) -> Option<(usize, usize)> {
        let has_sections = self
            .lines
            .iter()
            .any(|line| matches!(line, CfgLine::Section(_)));
        let start = if has_sections {
            self.lines.iter().position(|line| {
                matches!(line, CfgLine::Section(name) if name == GENERAL_SECTION)
            })? + 1
        } else {
            0
        };
        let end = self.lines[start..]
            .iter()
            .position(|line| matches!(line, CfgLine::Section(_)))
            .map_or(self.lines.len(), |offset| start + offset);

        Some((start, end))
    }

    fn get(&self, key: &str) -> Option<&str> {
        let (start, end) = self.settings_range()?;
        self.lines[start..end].iter().find_map(|line| match line {
            CfgLine::Entry { key: k, value } if k == key => {
                Some(value.as_str())
            }
            _ => None,
        })
    }

    fn set(&mut self, key: &str, value: &str) {
        // New files get a [General] section like Prism writes; older files
        // without sections keep their layout.
        let range = if self.lines.is_empty() {
            None
        } else {
            self.settings_range()
        };
        let (start, end) = match range {
            Some(range) => range,
            None => {
                self.lines
                    .insert(0, CfgLine::Section(GENERAL_SECTION.into()));
                (1, 1)
            }
        };

        if let Some(CfgLine::Entry {
            value: existing, ..
        }) = self.lines[start..end].iter_mut().find(
            |line| matches!(line, CfgLine::Entry { key: k, .. } if k == key),
        ) {
            *existing = value.to_string();
            return;
        }

        // Insert after the last entry, before any trailing blank lines.
        let insert_at = self.lines[start..end]
            .iter()
            .rposition(|line| matches!(line, CfgLine::Entry { .. }))
            .map_or(start, |offset| start + offset + 1);
        self.lines.insert(
            insert_at,
            CfgLine::Entry {
                key: key.to_string(),
                value: value.to_string(),
            },
        );
    }

    fn set_optional(&mut self, key: &str, value: Option<&str>) {
        match value {
            Some(value) => self.set(key, value),
            None => self.remove(key),
        }
    }

    fn remove(&mut self, key: &str) {
        let Some((start, end)) = self.settings_range() else {
            return;
        };
        if let Some(offset) = self.lines[start..end].iter().position(
            |line| matches!(line, CfgLine::Entry { key: k, .. } if k == key),
        ) {
            self.lines.remove(start + offset);
        }
    }

    fn render(&self) -> String {
        let mut output = String::new();
        for line in &self.lines {
            match line {
                CfgLine::Section(name) => {
                    output.push('[');
                    output.push_str(name);
                    output.push(']');
                }
                CfgLine::Entry { key, value } => {
                    output.push_str(key);
                    output.push('=');
                    output.push_str(&escape_value(value));
                }
                CfgLine::Other(text) => output.push_str(text),
            }
            output.push('\n');
        }
        output
    }
}

fn needs_quoting(value: &str) -> bool {
    value != value.trim()
        || value.chars().any(|c| {
            matches!(c, '"' | '\\' | ';' | ',' | '=' | '#' | '\n' | '\r' | '\t')
        })
}

fn escape_value(value: &str) -> String {
    if !needs_quoting(value) {
        return value.to_string();
    }
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for c in value.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

fn unescape_value(value: &str) -> String {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return value.to_string();
    };
    let mut output = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            output.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => output.push('\n'),
            Some('r') => output.push('\r'),
            Some('t') => output.push('\t'),
            Some(other) => output.push(other),
            None => output.push('\\'),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> InstanceCfg {
        InstanceCfg {
            id: Some("local:1234".to_string()),
            name: "My \"Pack\"; v2".to_string(),
            game_version: "1.20.1".to_string(),
            loader: ModLoader::Fabric,
            loader_version: Some("0.15.11".to_string()),
            created: parse_date("2024-01-02T03:04:05+00:00"),
            modified: parse_date("2024-02-02T03:04:05+00:00"),
            last_played: Utc.timestamp_millis_opt(1_700_000_000_000).single(),
            submitted_time_played: 100,
            recent_time_played: 20,
            link: Some(InstanceLink::Unmanaged),
        }
    }

    #[test]
    fn round_trips_through_document() {
        let mut doc = CfgDocument::default();
        sample().write_to_document(&mut doc);
        let rendered = doc.render();
        let parsed =
            InstanceCfg::from_document(&CfgDocument::parse(&rendered)).unwrap();

        assert!(rendered.starts_with("[General]\n"));
        assert!(rendered.contains("totalTimePlayed=120\n"));
        assert!(rendered.contains("InstanceType=OneSix\n"));
        assert_eq!(parsed.id.as_deref(), Some("local:1234"));
        assert_eq!(parsed.name, "My \"Pack\"; v2");
        assert_eq!(parsed.game_version, "1.20.1");
        assert_eq!(parsed.loader, ModLoader::Fabric);
        assert_eq!(parsed.loader_version.as_deref(), Some("0.15.11"));
        assert_eq!(parsed.created, sample().created);
        assert_eq!(parsed.modified, sample().modified);
        assert_eq!(parsed.last_played, sample().last_played);
        assert_eq!(parsed.submitted_time_played, 100);
        assert_eq!(parsed.recent_time_played, 20);
        assert!(matches!(parsed.link, Some(InstanceLink::Unmanaged)));
    }

    #[test]
    fn preserves_unknown_keys_and_sections() {
        let original = "[General]\nConfigVersion=1.2\nnotes=hello\nname=Old\n\n[UI]\ncolor=blue\n";
        let mut doc = CfgDocument::parse(original);
        sample().write_to_document(&mut doc);
        let rendered = doc.render();

        assert!(rendered.contains("notes=hello\n"));
        assert!(rendered.contains("[UI]\ncolor=blue\n"));
        assert!(!rendered.contains("name=Old"));
        assert_eq!(rendered.matches("ConfigVersion").count(), 1);
    }

    #[test]
    fn prism_file_without_modrinth_keys_is_foreign() {
        let doc = CfgDocument::parse(
            "[General]\nInstanceType=OneSix\nname=Prism\ntotalTimePlayed=50\n",
        );
        assert!(InstanceCfg::from_document(&doc).is_none());
    }

    #[test]
    fn prism_playtime_is_treated_as_submitted() {
        let doc = CfgDocument::parse(
            "InstanceType=OneSix\nname=Old style\nModrinthGameVersion=1.8.9\ntotalTimePlayed=50\n",
        );
        let cfg = InstanceCfg::from_document(&doc).unwrap();
        assert_eq!(cfg.name, "Old style");
        assert_eq!(cfg.loader, ModLoader::Vanilla);
        assert_eq!(cfg.submitted_time_played, 50);
        assert_eq!(cfg.recent_time_played, 0);
    }

    #[test]
    fn infers_versions_from_mmc_pack() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("mmc-pack.json"),
            r#"{"components":[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.neoforged","version":"21.1.1"}]}"#,
        )
        .unwrap();

        let cfg = infer_instance_cfg(dir.path(), "Pack").unwrap();
        assert_eq!(cfg.game_version, "1.21.1");
        assert_eq!(cfg.loader, ModLoader::NeoForge);
        assert_eq!(cfg.loader_version.as_deref(), Some("21.1.1"));
    }

    #[test]
    fn unknown_version_is_not_guessed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("mods")).unwrap();

        assert!(looks_like_instance(dir.path()));
        assert!(infer_instance_cfg(dir.path(), "Pack").is_none());
    }

    #[tokio::test]
    async fn write_is_skipped_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_instance_cfg(dir.path(), &sample()).await.unwrap());
        assert!(!write_instance_cfg(dir.path(), &sample()).await.unwrap());
        assert!(matches!(
            read_instance_cfg(dir.path()).await.unwrap(),
            CfgRead::Parsed(_)
        ));
    }

    #[test]
    fn reads_versions_from_fabric_log() {
        let log = "[main/INFO]: Loading Minecraft 1.21.11 with Fabric Loader 0.17.2\n";
        assert_eq!(
            parse_log_versions(log),
            Some((
                "1.21.11".to_string(),
                ModLoader::Fabric,
                Some("0.17.2".to_string())
            ))
        );
    }

    #[test]
    fn reads_versions_from_fml_arguments() {
        let neoforge = "Launched with [--fml.neoForgeVersion, 21.11.3, --fml.mcVersion, 1.21.11, --fml.neoFormVersion, 20251201]";
        assert_eq!(
            parse_log_versions(neoforge),
            Some((
                "1.21.11".to_string(),
                ModLoader::NeoForge,
                Some("21.11.3".to_string())
            ))
        );
        let forge = "[--fml.forgeVersion, 47.3.0, --fml.mcVersion, 1.20.1]";
        assert_eq!(
            parse_log_versions(forge),
            Some((
                "1.20.1".to_string(),
                ModLoader::Forge,
                Some("47.3.0".to_string())
            ))
        );
        assert_eq!(parse_log_versions("no versions here"), None);
    }

    fn write_jar(path: &Path, name: &str, contents: &str) {
        use std::io::Write;
        let mut jar = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        jar.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        jar.write_all(contents.as_bytes()).unwrap();
        jar.finish().unwrap();
    }

    #[test]
    fn infers_version_from_pinned_mods() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir(&mods).unwrap();
        write_jar(
            &mods.join("a.jar"),
            "fabric.mod.json",
            r#"{"depends":{"minecraft":"~26.1.2"}}"#,
        );
        write_jar(
            &mods.join("b.jar"),
            "fabric.mod.json",
            r#"{"depends":{"minecraft":"26.1.2"}}"#,
        );
        write_jar(
            &mods.join("c.jar"),
            "fabric.mod.json",
            r#"{"depends":{"minecraft":">=1.21"}}"#,
        );

        let cfg = infer_instance_cfg(dir.path(), "Pack").unwrap();
        assert_eq!(cfg.game_version, "26.1.2");
        assert_eq!(cfg.loader, ModLoader::Fabric);
    }

    #[test]
    fn infers_version_from_latest_log() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("logs")).unwrap();
        std::fs::write(
            dir.path().join("logs/latest.log"),
            "Loading Minecraft 1.21.11 with Quilt Loader 0.29.0",
        )
        .unwrap();

        let cfg = infer_instance_cfg(dir.path(), "Pack").unwrap();
        assert_eq!(cfg.game_version, "1.21.11");
        assert_eq!(cfg.loader, ModLoader::Quilt);
        assert_eq!(cfg.loader_version.as_deref(), Some("0.29.0"));
    }
}
