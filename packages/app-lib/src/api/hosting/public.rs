//! Making a hosted server reachable from the internet without setting up
//! port forwarding by hand: the router's automatic port forwarding (UPnP)
//! first, then a playit.gg tunnel. playit.gg needs a free account, linked
//! once by confirming in the browser.

use super::servers_dir;
use crate::util::io;
use igd_next::PortMappingProtocol;
use igd_next::aio::tokio::search_gateway;
use playit_agent_core::network::origin_lookup::OriginLookup;
use playit_agent_core::network::tcp::tcp_settings::TcpSettings;
use playit_agent_core::network::udp::udp_settings::UdpSettings;
use playit_agent_core::playit_agent::{PlayitAgent, PlayitAgentSettings};
use playit_api_client::PlayitApi;
use playit_api_client::api::{
    AgentTunnelV1, ClaimAgentType, ClaimSetupResponse, ReqClaimExchange,
    ReqClaimSetup, ReqTunnelsDelete,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::Mutex;

const PLAYIT_API: &str = "https://api.playit.gg";
const PLAYIT_FILE: &str = ".playit.json";
/// The playit.gg agent this app runs (the `playit-agent-core` tag in
/// Cargo.toml), reported like the official agent does.
const PLAYIT_AGENT_VERSION: &str = "playit 1.0.10";
/// How long a link code waits for the confirmation in the browser.
const LINK_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Where a server can be reached from outside this network.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PublicAddress {
    pub address: String,
    pub via: PublicVia,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicVia {
    Upnp,
    Playit,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayitLink {
    pub linked: bool,
    /// The page to confirm the link on, while linking.
    pub link_url: Option<String>,
    /// Why the last link attempt failed.
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PlayitSecret {
    secret_key: String,
}

#[derive(Default)]
struct Playit {
    link_code: Option<String>,
    link_error: Option<String>,
    agent: Option<Arc<AtomicBool>>,
}

static PLAYIT: LazyLock<Mutex<Playit>> =
    LazyLock::new(|| Mutex::new(Playit::default()));

/// Opens the server to the internet: UPnP when the router supports it and
/// has a public address, otherwise playit.gg when linked.
pub(super) async fn open(
    server_id: &str,
    port: u16,
) -> Result<PublicAddress, String> {
    let upnp = match upnp_open(port).await {
        Ok(address) => {
            return Ok(PublicAddress {
                address,
                via: PublicVia::Upnp,
            });
        }
        Err(error) => error,
    };
    let Some(secret) = playit_secret().await else {
        return Err(format!(
            "Automatic port forwarding didn't work ({upnp}). Link playit.gg to get a public address."
        ));
    };
    playit_open(&secret, server_id, port)
        .await
        .map(|address| PublicAddress {
            address,
            via: PublicVia::Playit,
        })
        .map_err(|error| format!("playit.gg: {error}"))
}

/// Undoes what `open` did when the server stops.
pub(super) async fn close(address: &PublicAddress, port: u16) {
    if address.via == PublicVia::Upnp {
        let _ = upnp_close(port).await;
    }
}

/// Whether players outside this network can join a server.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reachability {
    Unchecked,
    Checking,
    Reachable,
    Unreachable,
}

#[derive(Deserialize)]
struct McStatus {
    online: bool,
}

/// Asks mcstatus.io to ping the server from outside this network, which
/// shows whether port forwarding really works (routers often can't reach
/// their own public address from the inside).
pub(super) async fn check_reachable(address: &str) -> Result<bool, String> {
    let url = format!(
        "https://api.mcstatus.io/v2/status/java/{}?query=false",
        urlencoding::encode(address)
    );
    let status: McStatus = crate::util::fetch::REQWEST_CLIENT
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;
    Ok(status.online)
}

/// This computer's address in the local network, for players on the same
/// Wi-Fi.
pub(super) fn lan_address(port: u16) -> Option<String> {
    let ip = local_ip_towards(SocketAddr::from(([1, 1, 1, 1], 80)))?;
    (!ip.is_loopback() && !ip.is_unspecified())
        .then(|| address_with_port(&ip.to_string(), port))
}

// --- UPnP ---------------------------------------------------------------

async fn upnp_open(port: u16) -> Result<String, String> {
    let gateway = search_gateway(igd_next::SearchOptions {
        timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    })
    .await
    .map_err(|_| "the router doesn't offer it".to_string())?;
    let external = gateway
        .get_external_ip()
        .await
        .map_err(|error| error.to_string())?;
    if !is_public(external) {
        return Err(
            "the router has no public address of its own (shared by the internet provider)"
                .to_string(),
        );
    }
    let local_ip = local_ip_towards(gateway.addr)
        .ok_or_else(|| "couldn't find this computer's address".to_string())?;
    let local = SocketAddr::new(local_ip, port);
    // Permanent mappings (lease 0) are refused by some routers; fall back to
    // a day, renewed on the next start.
    let mut result = gateway
        .add_port(
            PortMappingProtocol::TCP,
            port,
            local,
            0,
            "Threadrinth server",
        )
        .await;
    if result.is_err() {
        result = gateway
            .add_port(
                PortMappingProtocol::TCP,
                port,
                local,
                24 * 60 * 60,
                "Threadrinth server",
            )
            .await;
    }
    result.map_err(|error| error.to_string())?;
    Ok(address_with_port(&external.to_string(), port))
}

async fn upnp_close(port: u16) -> Result<(), String> {
    let gateway = search_gateway(igd_next::SearchOptions {
        timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    })
    .await
    .map_err(|error| error.to_string())?;
    gateway
        .remove_port(PortMappingProtocol::TCP, port)
        .await
        .map_err(|error| error.to_string())
}

/// The address this computer uses to reach `target` (no packet is sent).
fn local_ip_towards(target: SocketAddr) -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(target).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// Whether other people can reach this address (not a private,
/// carrier-grade NAT or otherwise reserved one).
fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(ip) => !(ip.is_loopback() || ip.is_unspecified()),
    }
}

/// `host:port`, or just `host` for Minecraft's default port.
fn address_with_port(host: &str, port: u16) -> String {
    if port == 25565 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

// --- playit.gg ----------------------------------------------------------

async fn secret_path() -> Option<std::path::PathBuf> {
    servers_dir().await.ok().map(|dir| dir.join(PLAYIT_FILE))
}

async fn playit_secret() -> Option<String> {
    let bytes = io::read(secret_path().await?).await.ok()?;
    serde_json::from_slice::<PlayitSecret>(&bytes)
        .ok()
        .map(|secret| secret.secret_key)
        .filter(|key| !key.trim().is_empty())
}

/// Whether playit.gg is linked, or the page to confirm the link on.
pub async fn playit_link_status() -> PlayitLink {
    let playit = PLAYIT.lock().await;
    PlayitLink {
        linked: playit_secret().await.is_some(),
        link_url: playit.link_code.as_deref().map(link_url),
        error: playit.link_error.clone(),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn link_url(code: &str) -> String {
    format!("https://playit.gg/claim/{code}")
}

/// Starts linking a playit.gg account: returns the page to open, where the
/// user signs in (or makes a free account) and confirms. The link finishes
/// in the background; `playit_link_status` shows when it's done.
pub async fn start_playit_link() -> crate::Result<String> {
    let mut playit = PLAYIT.lock().await;
    if let Some(code) = &playit.link_code {
        return Ok(link_url(code));
    }
    let mut bytes = [0_u8; 5];
    rand::thread_rng().fill_bytes(&mut bytes);
    let code = to_hex(&bytes);
    playit.link_code = Some(code.clone());
    playit.link_error = None;
    drop(playit);

    let url = link_url(&code);
    tokio::spawn(async move {
        let result = tokio::time::timeout(LINK_TIMEOUT, finish_link(&code))
            .await
            .unwrap_or_else(|_| {
                Err("The link wasn't confirmed in time".to_string())
            });
        let mut playit = PLAYIT.lock().await;
        playit.link_code = None;
        if let Err(error) = result {
            tracing::warn!("Linking playit.gg failed: {error}");
            playit.link_error = Some(error);
        }
    });
    Ok(url)
}

async fn finish_link(code: &str) -> Result<(), String> {
    let api = PlayitApi::create(PLAYIT_API.to_string(), None);
    loop {
        let setup = api
            .claim_setup(ReqClaimSetup {
                code: code.to_string(),
                agent_type: ClaimAgentType::SelfManaged,
                version: PLAYIT_AGENT_VERSION.to_string(),
            })
            .await
            .map_err(|error| format!("{error:?}"))?;
        match setup {
            ClaimSetupResponse::UserAccepted => break,
            ClaimSetupResponse::UserRejected => {
                return Err("The link was declined".to_string());
            }
            ClaimSetupResponse::WaitingForUserVisit
            | ClaimSetupResponse::WaitingForUser => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    let secret_key = loop {
        match api
            .claim_exchange(ReqClaimExchange {
                code: code.to_string(),
            })
            .await
        {
            Ok(secret) => break secret.secret_key,
            Err(error) => {
                tracing::debug!("Waiting for the playit.gg key: {error:?}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };
    let path = secret_path()
        .await
        .ok_or_else(|| "The app isn't ready".to_string())?;
    if let Some(parent) = path.parent() {
        io::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    io::write(
        &path,
        serde_json::to_vec(&PlayitSecret { secret_key })
            .map_err(|error| error.to_string())?,
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Forgets the linked playit.gg account and stops its tunnels here.
pub async fn unlink_playit() -> crate::Result<()> {
    let mut playit = PLAYIT.lock().await;
    if let Some(running) = playit.agent.take() {
        running.store(false, Ordering::SeqCst);
    }
    playit.link_code = None;
    playit.link_error = None;
    if let Some(path) = secret_path().await
        && path.is_file()
    {
        io::remove_file(&path).await?;
    }
    Ok(())
}

/// Makes sure the agent runs and a tunnel to `port` exists, and returns
/// the tunnel's address.
async fn playit_open(
    secret: &str,
    server_id: &str,
    port: u16,
) -> Result<String, String> {
    // playit.gg only makes tunnels for agents it has seen, with a current
    // version, so the agent connects first.
    ensure_agent(secret).await?;
    let api =
        PlayitApi::create(PLAYIT_API.to_string(), Some(secret.to_string()));
    let name = format!("threadrinth-{server_id}");
    let run_data = api
        .v1_agents_rundata()
        .await
        .map_err(|error| format!("{error:?}"))?;

    let existing = run_data.tunnels.iter().find(|tunnel| tunnel.name == name);
    if let Some(tunnel) = existing
        && tunnel_local_port(tunnel) != Some(port)
    {
        // The server's port changed.
        let _ = api
            .tunnels_delete(ReqTunnelsDelete {
                tunnel_id: tunnel.id,
            })
            .await;
    }
    if existing.is_none_or(|tunnel| tunnel_local_port(tunnel) != Some(port)) {
        let body = tunnel_create_body(&name, run_data.agent_id, port);
        // The agent's first connection can take a moment to register.
        let mut attempts = 0;
        loop {
            match create_tunnel(secret, &body).await {
                Ok(()) => break,
                Err(error) if attempts < 5 => {
                    tracing::debug!("Creating the playit.gg tunnel: {error}");
                    attempts += 1;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    // A new tunnel takes a moment to get its address.
    for _ in 0..30 {
        let run_data = api
            .v1_agents_rundata()
            .await
            .map_err(|error| format!("{error:?}"))?;
        if let Some(tunnel) =
            run_data.tunnels.iter().find(|tunnel| tunnel.name == name)
            && !tunnel.display_address.is_empty()
        {
            if let Some(reason) = &tunnel.disabled_reason {
                return Err(format!("the tunnel is disabled: {reason}"));
            }
            return Ok(tunnel.display_address.clone());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err("the tunnel didn't get an address".to_string())
}

/// The body playit.gg's website sends to `/v1/tunnels/create`: a Minecraft
/// Java tunnel on the free network to this agent's `localhost:port`. The
/// published API client still has an older shape the API rejects.
fn tunnel_create_body(
    name: &str,
    agent_id: uuid::Uuid,
    port: u16,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "protocol": { "type": "tunnel-type", "details": "minecraft-java" },
        "origin": {
            "type": "agent",
            "data": {
                "agent_id": agent_id,
                "config": {
                    "fields": [
                        { "name": "local_ip", "value": Ipv4Addr::LOCALHOST.to_string() },
                        { "name": "local_port", "value": port.to_string() },
                    ],
                },
            },
        },
        "endpoint": {
            "type": "region",
            "details": { "region": "global", "port": null },
        },
        "enabled": true,
    })
}

async fn create_tunnel(
    secret: &str,
    body: &serde_json::Value,
) -> Result<(), String> {
    let response: serde_json::Value = crate::util::fetch::REQWEST_CLIENT
        .post(format!("{PLAYIT_API}/v1/tunnels/create"))
        .header("Authorization", format!("Agent-Key {}", secret.trim()))
        .json(body)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| error.to_string())?
        .json()
        .await
        .map_err(|error| error.to_string())?;
    if response["status"] == "success" {
        Ok(())
    } else {
        Err(playit_error_text(&response["data"]))
    }
}

/// A readable message from a playit.gg `fail`/`error` answer.
fn playit_error_text(data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(error) => error
            .get("message")
            .and_then(|x| x.as_str())
            .map_or_else(|| data.to_string(), str::to_string),
        other => other.to_string(),
    }
}

fn tunnel_local_port(tunnel: &AgentTunnelV1) -> Option<u16> {
    tunnel
        .agent_config
        .fields
        .iter()
        .find(|field| field.name == "local_port")
        .and_then(|field| field.value.parse().ok())
}

/// Runs the playit.gg agent in the app (once), which carries the tunnels'
/// traffic to the servers.
async fn ensure_agent(secret: &str) -> Result<(), String> {
    let mut playit = PLAYIT.lock().await;
    if playit
        .agent
        .as_ref()
        .is_some_and(|running| running.load(Ordering::SeqCst))
    {
        return Ok(());
    }
    let api =
        PlayitApi::create(PLAYIT_API.to_string(), Some(secret.to_string()));
    let lookup = Arc::new(OriginLookup::default());
    if let Ok(run_data) = api.v1_agents_rundata().await {
        lookup.update_from_run_data(&run_data).await;
    }
    let agent = PlayitAgent::new(
        PlayitAgentSettings {
            api_url: PLAYIT_API.to_string(),
            secret_key: secret.to_string(),
            tcp_settings: TcpSettings::default(),
            udp_settings: UdpSettings::default(),
        },
        lookup.clone(),
    )
    .await
    .map_err(|error| format!("{error:?}"))?;
    let cancel = agent.cancellation_token();
    let running = Arc::new(AtomicBool::new(true));
    tokio::spawn(agent.run());

    // Keep the tunnel list current while the agent runs, and stop it on
    // unlink.
    let refresh = running.clone();
    tokio::spawn(async move {
        while refresh.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_secs(10)).await;
            if let Ok(run_data) = api.v1_agents_rundata().await {
                lookup.update_from_run_data(&run_data).await;
            }
        }
        cancel.cancel();
    });
    playit.agent = Some(running);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_addresses() {
        assert!(is_public("8.8.8.8".parse().unwrap()));
        assert!(!is_public("192.168.1.1".parse().unwrap()));
        assert!(!is_public("10.0.0.1".parse().unwrap()));
        assert!(
            !is_public("100.72.1.1".parse().unwrap()),
            "carrier-grade NAT"
        );
        assert!(is_public("100.128.0.1".parse().unwrap()));
    }

    #[test]
    fn tunnel_body_matches_the_website() {
        let body =
            tunnel_create_body("threadrinth-a", uuid::Uuid::nil(), 25570);
        assert_eq!(body["protocol"]["details"], "minecraft-java");
        assert_eq!(body["origin"]["type"], "agent");
        assert_eq!(
            body["origin"]["data"]["config"]["fields"][1]["value"],
            "25570"
        );
        assert_eq!(body["endpoint"]["details"]["region"], "global");
        assert_eq!(
            playit_error_text(
                &serde_json::json!({"type":"validation","message":"failed to parse body"})
            ),
            "failed to parse body"
        );
        assert_eq!(
            playit_error_text(&serde_json::json!("AgentNotFound")),
            "AgentNotFound"
        );
    }

    #[test]
    fn default_port_is_left_out() {
        assert_eq!(address_with_port("1.2.3.4", 25565), "1.2.3.4");
        assert_eq!(address_with_port("1.2.3.4", 25570), "1.2.3.4:25570");
    }

    /// Talks to playit.gg: a fresh link code waits for the user.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn playit_link_codes_wait_for_the_user() {
        let api = PlayitApi::create(PLAYIT_API.to_string(), None);
        let mut bytes = [0_u8; 5];
        rand::thread_rng().fill_bytes(&mut bytes);
        let setup = api
            .claim_setup(ReqClaimSetup {
                code: to_hex(&bytes),
                agent_type: ClaimAgentType::SelfManaged,
                version: PLAYIT_AGENT_VERSION.to_string(),
            })
            .await
            .unwrap();
        assert_eq!(setup, ClaimSetupResponse::WaitingForUserVisit);
    }
}
