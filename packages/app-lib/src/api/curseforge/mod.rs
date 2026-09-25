//! Threadrinth's CurseForge tab: searching CurseForge, installing its mods
//! into instances and its modpacks (and Feed the Beast's, see
//! [`crate::api::ftb`]) as new instances. Kept apart from the Modrinth code
//! so upstream updates merge cleanly.

pub(crate) mod bundle;
pub(crate) mod client;
mod modpack;
mod mods;

#[cfg(test)]
pub(crate) mod e2e_tests;
#[cfg(test)]
mod tests;

pub use bundle::{ManualDownload, PackInstallReport};
pub use client::api_key;
pub use modpack::install_modpack;
pub(crate) use mods::is_downloadable;
pub use mods::{
    CurseForgeClass, CurseForgeFile, CurseForgeFiles, CurseForgeModInstall,
    CurseForgeProject, CurseForgeSearchQuery, CurseForgeSearchResults,
    CurseForgeSort, get_description, get_files, get_project, install_mod,
    search,
};

/// Whether this build can talk to CurseForge.
pub fn is_available() -> bool {
    api_key().is_some()
}
