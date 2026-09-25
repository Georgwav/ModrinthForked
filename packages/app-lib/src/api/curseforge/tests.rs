//! Parsing and planning tests on responses shaped like the documented
//! CurseForge API and real modpacks.ch answers, plus a live check of the
//! CurseForge API that runs only when a key is set.

use super::bundle::{
    Bundle, BundleFile, ManualDownload, ZipOverrides, match_loader_version,
    pack_index, parse_loader_id, safe_file_name, safe_relative_path,
    write_mrpack,
};
use super::client::{self, DataResponse, PagedResponse};
use super::modpack::{Manifest, read_manifest, resolve_files};
use super::mods::{
    CurseForgeClass, CurseForgeSearchQuery, CurseForgeSort, file_from_api,
    newest_fitting_file, search_results_from_api,
};
use crate::api::ftb;
use crate::pack::install_from::{PackDependency, PackFormat};
use crate::state::ModLoader;
use std::io::{Read, Write};

fn fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/api/curseforge/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn query(class: CurseForgeClass) -> CurseForgeSearchQuery {
    CurseForgeSearchQuery {
        class,
        query: Some("jei items".to_string()),
        game_version: Some("1.20.1".to_string()),
        loader: Some("forge".to_string()),
        sort: CurseForgeSort::Popularity,
        index: 20,
        page_size: Some(2),
    }
}

#[test]
fn search_results_parse() {
    let response: PagedResponse<client::Mod> = fixture("cf_search.json");
    let results =
        search_results_from_api(response, &query(CurseForgeClass::Mod));
    assert_eq!(results.total, 10_000, "the API pages up to 10,000 results");
    assert_eq!(results.index, 20);
    assert_eq!(results.projects.len(), 2);

    let jei = &results.projects[0];
    assert_eq!(jei.id, 238222);
    assert_eq!(jei.name, "Just Enough Items (JEI)");
    assert_eq!(jei.authors, ["mezz"]);
    assert_eq!(jei.downloads, 372_145_884);
    assert_eq!(
        jei.icon_url.as_deref(),
        Some(
            "https://media.forgecdn.net/avatars/thumbnails/29/69/256/256/635838945588716414.jpeg"
        )
    );
    assert_eq!(
        jei.website_url.as_deref(),
        Some("https://www.curseforge.com/minecraft/mc-mods/jei")
    );
    assert_eq!(jei.game_versions, ["1.20.1", "1.21.1"]);
    assert_eq!(jei.loaders, ["forge", "fabric", "neoforge"]);
    assert!(jei.allow_distribution);

    let wireless = &results.projects[1];
    assert_eq!(wireless.icon_url, None);
    assert!(wireless.allow_distribution, "null means allowed");
}

#[test]
fn search_path_has_filters() {
    let path = super::mods::search_path(&query(CurseForgeClass::Modpack));
    assert_eq!(
        path,
        "/v1/mods/search?gameId=432&classId=4471&sortField=2&sortOrder=desc&index=20&pageSize=2&searchFilter=jei%20items&gameVersion=1.20.1&modLoaderType=1"
    );
}

#[test]
fn newest_file_for_instance() {
    let files = || fixture::<PagedResponse<client::File>>("cf_files.json").data;

    // The newest Forge file for 1.20.1; the server pack and the 1.19.2 file
    // don't count although they are newer.
    let file = newest_fitting_file(files(), "1.20.1", ModLoader::Forge)
        .expect("a Forge file");
    assert_eq!(file.id, 5846804);
    assert_eq!(
        file.sha1(),
        Some("a1b2c3d4e5f60718293a4b5c6d7e8f9012345678")
    );
    assert_eq!(file.dependencies.len(), 2);

    // Tagged for both Forge and NeoForge.
    let file = newest_fitting_file(files(), "1.20.1", ModLoader::NeoForge)
        .expect("a NeoForge file");
    assert_eq!(file.id, 5846804);

    let file = newest_fitting_file(files(), "1.20.1", ModLoader::Fabric)
        .expect("a Fabric file");
    assert_eq!(file.id, 5846810);
    // Quilt runs Fabric mods.
    let file = newest_fitting_file(files(), "1.20.1", ModLoader::Quilt)
        .expect("a Fabric file for Quilt");
    assert_eq!(file.id, 5846810);

    assert!(newest_fitting_file(files(), "1.21", ModLoader::Forge).is_none());
    assert!(
        newest_fitting_file(files(), "1.20.1", ModLoader::Vanilla).is_none()
    );
}

#[test]
fn files_for_the_versions_list() {
    let files = fixture::<PagedResponse<client::File>>("cf_files.json").data;
    let website = Some("https://www.curseforge.com/minecraft/mc-mods/jei");
    let shown = files
        .into_iter()
        .map(|file| file_from_api(file, website, Some(true)))
        .collect::<Vec<_>>();
    assert_eq!(shown[0].game_versions, ["1.20.1"]);
    assert_eq!(shown[0].loaders, ["forge"]);
    assert!(shown[0].downloadable);
    assert_eq!(shown[2].release_type, "beta");
    assert!(!shown[2].downloadable, "no download URL");
    assert_eq!(
        shown[2].page_url,
        "https://www.curseforge.com/minecraft/mc-mods/jei/files/5846810"
    );

    let files = fixture::<PagedResponse<client::File>>("cf_files.json").data;
    let blocked = file_from_api(files[0].clone(), website, Some(false));
    assert!(!blocked.downloadable, "the author doesn't allow downloads");
    let no_site = file_from_api(files[0].clone(), None, None);
    assert_eq!(
        no_site.page_url,
        "https://www.curseforge.com/projects/238222"
    );
}

#[test]
fn modpack_manifest_and_files() {
    let manifest: Manifest = fixture("cf_manifest.json");
    assert_eq!(manifest.minecraft.version, "1.20.1");
    assert_eq!(
        manifest.loader(),
        Some((ModLoader::Forge, "47.2.0".to_string())),
        "the primary loader wins"
    );

    let files: DataResponse<Vec<client::File>> = fixture("cf_pack_files.json");
    let projects: DataResponse<Vec<client::Mod>> = fixture("cf_mods.json");
    let (downloads, manual) =
        resolve_files(&manifest, &files.data, &projects.data);

    assert_eq!(
        downloads,
        [
            BundleFile {
                path: "mods/jei-1.20.1-forge-15.2.0.27.jar".to_string(),
                urls: vec![
                    "https://edge.forgecdn.net/files/4712/866/jei-1.20.1-forge-15.2.0.27.jar"
                        .to_string()
                ],
                sha1: Some(
                    "b0f5ae8a2d3ee2d0e0a4bd7a04ed7b0d6a13c6f1".to_string()
                ),
                size: 1260121,
            },
            BundleFile {
                path: "resourcepacks/Faithful 32x - 1.20.1.zip".to_string(),
                urls: vec![
                    "https://edge.forgecdn.net/files/7/0/Faithful 32x - 1.20.1.zip"
                        .to_string()
                ],
                sha1: Some(
                    "1111111111111111111111111111111111111111".to_string()
                ),
                size: 5000000,
            },
        ]
    );
    assert_eq!(
        manual,
        [
            ManualDownload {
                name: "jei-1.20.1-fabric-15.20.0.105.jar".to_string(),
                url: "https://www.curseforge.com/minecraft/mc-mods/jei/files/5846810"
                    .to_string(),
                folder: "mods".to_string(),
            },
            ManualDownload {
                name: "Complementary Shaders".to_string(),
                url: "https://www.curseforge.com/minecraft/shaders/complementary-shaders/files/8000"
                    .to_string(),
                folder: "shaderpacks".to_string(),
            },
            ManualDownload {
                name: "Project 4000".to_string(),
                url: "https://www.curseforge.com/projects/4000".to_string(),
                folder: "mods".to_string(),
            },
        ],
        "optional files are left out"
    );
}

#[test]
fn loader_ids() {
    assert_eq!(
        parse_loader_id("forge-47.2.0"),
        Some((ModLoader::Forge, "47.2.0".to_string()))
    );
    assert_eq!(
        parse_loader_id("neoforge-20.4.80-beta"),
        Some((ModLoader::NeoForge, "20.4.80-beta".to_string()))
    );
    assert_eq!(
        parse_loader_id("fabric-0.15.0"),
        Some((ModLoader::Fabric, "0.15.0".to_string()))
    );
    assert_eq!(
        parse_loader_id("quilt-0.20.2"),
        Some((ModLoader::Quilt, "0.20.2".to_string()))
    );
    assert_eq!(parse_loader_id("liteloader-1.0"), None);
    assert_eq!(parse_loader_id("forge-"), None);

    let known = ["47.1.106", "47.1.105", "10.13.4.1614"];
    let known = known.iter().copied();
    assert_eq!(
        match_loader_version("1.20.1", "47.1.105", known.clone()),
        "47.1.105"
    );
    assert_eq!(
        match_loader_version("1.20.1", "1.20.1-47.1.106", known.clone()),
        "47.1.106"
    );
    assert_eq!(
        match_loader_version(
            "1.7.10",
            "1.7.10-10.13.4.1614-1.7.10",
            known.clone()
        ),
        "10.13.4.1614"
    );
    assert_eq!(match_loader_version("1.20.1", "0.1", known), "0.1");
}

#[test]
fn paths_stay_in_the_instance() {
    assert_eq!(
        safe_relative_path("./config/a.toml").as_deref(),
        Some("config/a.toml")
    );
    assert_eq!(
        safe_relative_path("mods\\b.jar").as_deref(),
        Some("mods/b.jar")
    );
    assert_eq!(safe_relative_path("../escape"), None);
    assert_eq!(safe_relative_path("config/../../escape"), None);
    assert_eq!(safe_relative_path("./"), None);
    assert_eq!(safe_file_name("a/b.jar"), None);
    assert_eq!(safe_file_name(".."), None);
    assert_eq!(safe_file_name("jei.jar").as_deref(), Some("jei.jar"));
}

fn test_bundle(overrides: Option<ZipOverrides>) -> Bundle {
    Bundle {
        name: "Test Pack".to_string(),
        version: "1.2.3".to_string(),
        summary: None,
        game_version: "1.20.1".to_string(),
        loader: ModLoader::Forge,
        loader_version: Some("47.2.0".to_string()),
        files: vec![
            BundleFile {
                path: "mods/a.jar".to_string(),
                urls: vec!["https://example.com/a.jar".to_string()],
                sha1: Some(
                    "1111111111111111111111111111111111111111".to_string(),
                ),
                size: 10,
            },
            BundleFile {
                path: "mods/A.jar".to_string(),
                urls: vec!["https://example.com/a-again.jar".to_string()],
                sha1: None,
                size: 10,
            },
            BundleFile {
                path: "resourcepacks/Faithful 32x - 1.20.1.zip".to_string(),
                urls: vec!["https://example.com/f.zip".to_string()],
                sha1: None,
                size: 5_000_000_000,
            },
        ],
        overrides,
    }
}

#[test]
fn bundle_is_a_valid_mrpack() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("pack.zip");
    {
        let mut zip =
            zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        zip.write_all(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src/api/curseforge/fixtures/cf_manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        zip.add_directory("custom/", options).unwrap();
        zip.start_file("custom/config/jei.toml", options).unwrap();
        zip.write_all(b"a = 1").unwrap();
        zip.start_file("custom/options.txt", options).unwrap();
        zip.write_all(b"fov:0").unwrap();
        zip.start_file("custom/../evil.txt", options).unwrap();
        zip.write_all(b"no").unwrap();
        zip.start_file("elsewhere.txt", options).unwrap();
        zip.write_all(b"no").unwrap();
        zip.finish().unwrap();
    }
    assert_eq!(
        read_manifest(&source).unwrap().name.as_deref(),
        Some("Test Pack")
    );

    let out = dir.path().join("Test Pack.mrpack");
    write_mrpack(
        &test_bundle(Some(ZipOverrides {
            archive: source,
            prefix: "custom".to_string(),
        })),
        &out,
    )
    .unwrap();

    let mut zip =
        zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let mut names = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            "modrinth.index.json",
            "overrides/config/jei.toml",
            "overrides/options.txt"
        ]
    );
    let mut text = String::new();
    zip.by_name("overrides/config/jei.toml")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "a = 1");

    // The regular modpack installer reads the index.
    let mut index = String::new();
    zip.by_name("modrinth.index.json")
        .unwrap()
        .read_to_string(&mut index)
        .unwrap();
    let pack: PackFormat = serde_json::from_str(&index).unwrap();
    assert_eq!(pack.game, "minecraft");
    assert_eq!(pack.name, "Test Pack");
    assert_eq!(pack.version_id, "1.2.3");
    assert_eq!(pack.files.len(), 2, "same paths are listed once");
    assert_eq!(pack.files[0].path.as_str(), "mods/a.jar");
    assert_eq!(pack.files[1].file_size, u32::MAX);
    assert_eq!(
        pack.dependencies
            .get(&PackDependency::Minecraft)
            .map(String::as_str),
        Some("1.20.1")
    );
    assert_eq!(
        pack.dependencies
            .get(&PackDependency::Forge)
            .map(String::as_str),
        Some("47.2.0")
    );
}

#[test]
fn vanilla_bundle_has_no_loader() {
    let mut bundle = test_bundle(None);
    bundle.loader = ModLoader::Vanilla;
    let index = pack_index(&bundle);
    let dependencies = index["dependencies"].as_object().unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies["minecraft"], "1.20.1");
}

#[test]
fn ftb_pack_parses() {
    let pack: ftb::ApiPack = fixture("ftb_pack.json");
    let pack = ftb::pack_from_api(pack);
    assert_eq!(pack.id, 103);
    assert_eq!(pack.name, "FTB Skies");
    assert_eq!(pack.authors, ["FTB Team"]);
    assert_eq!(
        pack.icon_url.as_deref(),
        Some("https://apps.modpacks.ch/modpacks/art/99/FTB Skies 512x512.png")
    );
    assert_eq!(
        pack.website_url,
        "https://www.feed-the-beast.com/modpacks/103-ftb-skies"
    );
    assert_eq!(pack.versions.len(), 2);
    let newest = &pack.versions[0];
    assert_eq!(newest.id, 12254);
    assert_eq!(newest.name, "1.6.0");
    assert_eq!(newest.release_type, "release");
    assert_eq!(newest.game_version.as_deref(), Some("1.19.2"));
    assert_eq!(newest.loader.as_deref(), Some("forge"));
    assert_eq!(newest.loader_version.as_deref(), Some("43.4.2"));

    assert!(ftb::pack_matches(&pack, Some("1.19.2"), Some("forge")));
    assert!(ftb::pack_matches(&pack, None, None));
    assert!(!ftb::pack_matches(&pack, Some("1.20.1"), None));
    assert!(!ftb::pack_matches(&pack, None, Some("fabric")));
}

#[test]
fn ftb_version_becomes_a_bundle() {
    let pack: ftb::ApiPack = fixture("ftb_pack.json");
    let version: ftb::ApiVersion = fixture("ftb_version.json");
    assert_eq!(
        version.files[1].curseforge,
        Some(ftb::ApiCurseForgeRef {
            project: 559313,
            file: 3940200
        }),
        "ids given as text"
    );

    let (downloads, curseforge) = ftb::split_files(&version.files);
    let paths = downloads
        .iter()
        .map(|x| x.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "mods/AE2WTLib-12.9.7.jar",
            "config/Advancedperipherals/metaphysics.toml",
            "default-server.properties",
            "mods/Ding-1.19.2-Forge-1.5.0.jar",
        ],
        "server-only files and paths outside the instance are left out"
    );
    assert_eq!(
        downloads[0].sha1.as_deref(),
        Some("8030049d3a7d71b9e9be56e64e04c92d6f37d6fb")
    );
    assert_eq!(downloads[0].size, 194174);
    assert_eq!(
        downloads[0].urls,
        ["https://edge.forgecdn.net/files/4655/495/AE2WTLib-12.9.7.jar"]
    );
    assert_eq!(
        curseforge,
        [(
            "mods/blocked-mod-1.0.jar".to_string(),
            ftb::ApiCurseForgeRef {
                project: 559313,
                file: 3940200
            }
        )]
    );

    let bundle = ftb::bundle_from_version(&pack, &version, downloads).unwrap();
    assert_eq!(bundle.name, "FTB Skies");
    assert_eq!(bundle.version, "1.6.0");
    assert_eq!(bundle.game_version, "1.19.2");
    assert_eq!(bundle.loader, ModLoader::Forge);
    assert_eq!(bundle.loader_version.as_deref(), Some("43.4.2"));
    let index = pack_index(&bundle);
    let pack: PackFormat = serde_json::from_value(index).unwrap();
    assert_eq!(pack.files.len(), 4);
}

/// Runs against the real CurseForge API when `CURSEFORGE_API_KEY` is set
/// (CI passes it from a secret); otherwise it only says it was skipped.
#[tokio::test]
async fn curseforge_live_api() {
    let Some(key) = client::api_key() else {
        println!(
            "CURSEFORGE_API_KEY isn't set; skipping the live CurseForge test"
        );
        return;
    };
    let http = reqwest::Client::new();
    let get = |path: String| {
        let http = http.clone();
        let key = key.clone();
        async move {
            http.get(format!("{}{path}", client::API_URL))
                .header("x-api-key", key)
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .bytes()
                .await
                .unwrap()
        }
    };

    // Search: JEI for Forge 1.20.1.
    let search = CurseForgeSearchQuery {
        class: CurseForgeClass::Mod,
        query: Some("Just Enough Items".to_string()),
        game_version: Some("1.20.1".to_string()),
        loader: Some("forge".to_string()),
        sort: CurseForgeSort::Popularity,
        index: 0,
        page_size: Some(10),
    };
    let response: PagedResponse<client::Mod> =
        serde_json::from_slice(&get(super::mods::search_path(&search)).await)
            .unwrap();
    let results = search_results_from_api(response, &search);
    assert!(results.total > 0);
    let jei = results
        .projects
        .iter()
        .find(|x| x.id == 238222)
        .expect("JEI found");
    assert!(jei.downloads > 1_000_000);

    // The file an install into a Forge 1.20.1 instance picks.
    let files: PagedResponse<client::File> = serde_json::from_slice(
        &get(super::mods::files_path(
            238222,
            Some("1.20.1"),
            Some("forge"),
            0,
        ))
        .await,
    )
    .unwrap();
    let file = newest_fitting_file(files.data, "1.20.1", ModLoader::Forge)
        .expect("a JEI file for Forge 1.20.1");
    assert!(file.download_url.is_some());
    assert!(file.sha1().is_some());

    // The same file by id, as modpack installs resolve it.
    let response = http
        .post(format!("{}/v1/mods/files", client::API_URL))
        .header("x-api-key", &key)
        .json(&serde_json::json!({ "fileIds": [file.id] }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let by_id: DataResponse<Vec<client::File>> = response.json().await.unwrap();
    assert_eq!(by_id.data.len(), 1);
    assert_eq!(by_id.data[0].sha1(), file.sha1());

    // Modpacks search.
    let search = CurseForgeSearchQuery {
        class: CurseForgeClass::Modpack,
        query: None,
        game_version: None,
        loader: None,
        sort: CurseForgeSort::Popularity,
        index: 0,
        page_size: Some(5),
    };
    let response: PagedResponse<client::Mod> =
        serde_json::from_slice(&get(super::mods::search_path(&search)).await)
            .unwrap();
    assert!(!response.data.is_empty());
    assert!(response.data.iter().all(|x| x.class_id == Some(4471)));
    println!("live CurseForge API: ok");
}

/// The Feed the Beast API answers in the shapes the app reads.
#[tokio::test]
#[ignore = "uses the modpacks.ch API"]
async fn ftb_live_api() {
    let get = |path: &str| {
        let url = format!("https://api.modpacks.ch/public{path}");
        async move { reqwest::get(url).await.unwrap().text().await.unwrap() }
    };
    let popular: ftb::PackList =
        serde_json::from_str(&get("/modpack/popular/installs/5").await)
            .unwrap();
    assert_eq!(popular.packs.len(), 5);
    let pack: ftb::ApiPack = serde_json::from_str(
        &get(&format!("/modpack/{}", popular.packs[0])).await,
    )
    .unwrap();
    let pack = ftb::pack_from_api(pack);
    assert!(!pack.versions.is_empty());
    let version = &pack.versions[0];
    assert!(version.game_version.is_some());
    let details: ftb::ApiVersion = serde_json::from_str(
        &get(&format!("/modpack/{}/{}", pack.id, version.id)).await,
    )
    .unwrap();
    assert_eq!(details.status, "success");
    let (downloads, _) = ftb::split_files(&details.files);
    assert!(!downloads.is_empty());
    let none: ftb::PackList =
        serde_json::from_str(&get("/modpack/search/5?term=zzzqqqxx").await)
            .unwrap();
    assert!(none.packs.is_empty());
    println!(
        "live FTB API: ok ({} files in {})",
        downloads.len(),
        pack.name
    );
}
