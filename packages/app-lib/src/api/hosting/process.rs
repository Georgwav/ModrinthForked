//! Running servers: starting and stopping them, their console and how much
//! CPU and memory they use.

use super::PublicAccess;
use super::public::{self, PublicAddress, Reachability};
use super::setup::{java_command, platform_args_file, server_java};
use super::{LaunchTarget, get_server, input, server_dir};
use crate::util::io::{self, IOError};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};

/// Console lines kept per server.
const CONSOLE_LINES: usize = 5000;
/// How long a server gets to save and stop before it is killed.
const STOP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerState {
    Offline,
    Starting,
    Running,
    Stopping,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleStream {
    Output,
    Error,
    /// A command typed in the app.
    Input,
    /// A note from the app (started, stopped, crashed).
    App,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsoleLine {
    /// Increases with every line; ask for lines after the last one seen.
    pub seq: u64,
    pub stream: ConsoleStream,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerStatus {
    pub state: ServerState,
    pub started: Option<DateTime<Utc>>,
    pub players: Vec<String>,
    /// Percent of one CPU core, like task managers show per process.
    pub cpu_percent: Option<f32>,
    pub memory_bytes: Option<u64>,
    /// How the last run ended, when it's not running.
    pub exit_code: Option<i32>,
    /// Where players outside this network join, when public access is on.
    pub public_address: Option<PublicAddress>,
    /// Why the server couldn't be made public.
    pub public_error: Option<String>,
    /// Whether the public address works from outside this network.
    pub reachability: Reachability,
    /// Where players on the same network join.
    pub lan_address: Option<String>,
}

struct Session {
    state: ServerState,
    started: Option<DateTime<Utc>>,
    pid: Option<u32>,
    stdin: Option<Arc<tokio::sync::Mutex<ChildStdin>>>,
    console: VecDeque<ConsoleLine>,
    next_seq: u64,
    players: BTreeSet<String>,
    exit_code: Option<i32>,
    system: Option<System>,
    public: Option<PublicAddress>,
    public_error: Option<String>,
    reachability: Reachability,
    port: u16,
}

impl Session {
    fn new() -> Self {
        Self {
            state: ServerState::Offline,
            started: None,
            pid: None,
            stdin: None,
            console: VecDeque::new(),
            next_seq: 0,
            players: BTreeSet::new(),
            exit_code: None,
            system: None,
            public: None,
            public_error: None,
            reachability: Reachability::Unchecked,
            port: 25565,
        }
    }

    fn push(&mut self, stream: ConsoleStream, text: String) {
        if stream == ConsoleStream::Output {
            self.track(&text);
        }
        self.console.push_back(ConsoleLine {
            seq: self.next_seq,
            stream,
            text,
        });
        self.next_seq += 1;
        while self.console.len() > CONSOLE_LINES {
            self.console.pop_front();
        }
    }

    /// Follows the log for the server being ready and players coming and
    /// going.
    fn track(&mut self, line: &str) {
        if self.state == ServerState::Starting
            && line.contains("Done (")
            && line.contains(")! For help")
        {
            self.state = ServerState::Running;
        }
        if let Some(name) = player_event(line, " joined the game") {
            self.players.insert(name);
        } else if let Some(name) = player_event(line, " left the game") {
            self.players.remove(&name);
        }
    }
}

/// `[12:00:00] [Server thread/INFO]: Steve joined the game` -> `Steve`.
fn player_event(line: &str, event: &str) -> Option<String> {
    let before = line.trim_end().strip_suffix(event)?;
    let name = before.rsplit(": ").next()?.trim();
    (!name.is_empty()
        && name.len() <= 16
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
    .then(|| name.to_string())
}

static SESSIONS: LazyLock<DashMap<String, Arc<Mutex<Session>>>> =
    LazyLock::new(DashMap::new);

fn session(id: &str) -> Arc<Mutex<Session>> {
    SESSIONS
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(Session::new())))
        .clone()
}

pub(super) fn is_running(id: &str) -> bool {
    SESSIONS.get(id).is_some_and(|session| {
        session
            .lock()
            .map(|s| s.state)
            .unwrap_or(ServerState::Offline)
            != ServerState::Offline
    })
}

pub(super) fn forget(id: &str) {
    SESSIONS.remove(id);
}

/// Starts a server. The Minecraft EULA has to be accepted first.
#[tracing::instrument]
pub async fn start_server(id: &str) -> crate::Result<()> {
    let server = get_server(id).await?;
    if is_running(id) {
        return Err(input("The server is already running"));
    }
    if !server.eula_accepted {
        return Err(input(
            "Accept the Minecraft EULA (https://aka.ms/MinecraftEULA) to start the server",
        ));
    }
    let dir = server_dir(id).await?;
    io::write(
        dir.join("eula.txt"),
        "# Accepted in Threadrinth (https://aka.ms/MinecraftEULA)\neula=true\n",
    )
    .await?;

    let session = session(id);
    {
        let mut s = lock(&session);
        s.state = ServerState::Starting;
        s.started = Some(Utc::now());
        s.players.clear();
        s.exit_code = None;
        s.public = None;
        s.public_error = None;
        s.reachability = Reachability::Unchecked;
        s.port = server.port;
        s.push(ConsoleStream::App, "Starting the server…".to_string());
    }

    let result = spawn(&server, &dir, &session).await;
    if let Err(error) = &result {
        let mut s = lock(&session);
        s.state = ServerState::Offline;
        s.push(ConsoleStream::App, format!("Could not start: {error}"));
    } else if server.public_access == PublicAccess::Auto {
        let session = session.clone();
        let (id, port) = (server.id.clone(), server.port);
        tokio::spawn(async move {
            let result = public::open(&id, port).await;
            let mut s = lock(&session);
            match result {
                Ok(address) => {
                    s.push(
                        ConsoleStream::App,
                        format!("Players can join at {}", address.address),
                    );
                    if s.state == ServerState::Offline {
                        // Stopped while opening; undo it.
                        tokio::spawn(async move {
                            public::close(&address, port).await;
                        });
                    } else {
                        s.public = Some(address);
                        let session = session.clone();
                        tokio::spawn(async move {
                            check_when_running(&session).await;
                        });
                    }
                }
                Err(error) => {
                    s.push(ConsoleStream::App, error.clone());
                    s.public_error = Some(error);
                }
            }
        });
    }
    result
}

async fn spawn(
    server: &super::HostedServer,
    dir: &std::path::Path,
    session: &Arc<Mutex<Session>>,
) -> crate::Result<()> {
    let java = server_java(&server.game_version).await?;
    let mut command = java_command(&java);
    command
        .arg(format!("-Xmx{}M", server.memory_mb))
        .arg(format!("-Xms{}M", server.memory_mb.min(1024)));
    match &server.launch {
        LaunchTarget::Jar { path } => {
            command.arg("-jar").arg(path);
        }
        LaunchTarget::ArgsFile { path } => {
            command.arg(format!("@{}", platform_args_file(path)));
        }
    }
    command
        .arg("nogui")
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child: Child = command
        .spawn()
        .map_err(|error| IOError::with_path(error, &java))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    {
        let mut s = lock(session);
        s.pid = child.id();
        s.stdin = child
            .stdin
            .take()
            .map(|stdin| Arc::new(tokio::sync::Mutex::new(stdin)));
        s.system = Some(System::new());
    }
    if let Some(stdout) = stdout {
        tokio::spawn(read_lines(
            stdout,
            ConsoleStream::Output,
            session.clone(),
        ));
    }
    if let Some(stderr) = stderr {
        tokio::spawn(read_lines(stderr, ConsoleStream::Error, session.clone()));
    }
    let session = session.clone();
    let port = server.port;
    tokio::spawn(async move {
        let status = child.wait().await;
        let public = lock(&session).public.take();
        if let Some(public) = public {
            public::close(&public, port).await;
        }
        let mut s = lock(&session);
        s.state = ServerState::Offline;
        s.pid = None;
        s.stdin = None;
        s.system = None;
        s.players.clear();
        match status {
            Ok(status) => {
                s.exit_code = status.code();
                let note = if status.success() {
                    "The server stopped.".to_string()
                } else {
                    format!("The server stopped unexpectedly ({status}).")
                };
                s.push(ConsoleStream::App, note);
            }
            Err(error) => {
                s.push(ConsoleStream::App, format!("Lost the server: {error}"));
            }
        }
    });
    Ok(())
}

async fn read_lines(
    stream: impl AsyncRead + Unpin,
    kind: ConsoleStream,
    session: Arc<Mutex<Session>>,
) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        lock(&session).push(kind, line);
    }
}

/// Sends a console command (without the leading slash).
pub async fn send_command(id: &str, command: &str) -> crate::Result<()> {
    let command = command.trim().trim_start_matches('/');
    if command.is_empty() {
        return Ok(());
    }
    let session = session(id);
    let stdin = {
        let mut s = lock(&session);
        let Some(stdin) = s.stdin.clone() else {
            return Err(input("The server isn't running"));
        };
        s.push(ConsoleStream::Input, format!("> {command}"));
        stdin
    };
    let mut stdin = stdin.lock().await;
    stdin.write_all(format!("{command}\n").as_bytes()).await?;
    stdin.flush().await?;
    Ok(())
}

/// Asks the server to save and stop; kills it if it hasn't stopped after a
/// minute.
pub async fn stop_server(id: &str) -> crate::Result<()> {
    if !is_running(id) {
        return Ok(());
    }
    {
        let session = session(id);
        let mut s = lock(&session);
        s.state = ServerState::Stopping;
    }
    if send_command(id, "stop").await.is_err() {
        return kill_server(id).await;
    }
    let id = id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(STOP_TIMEOUT).await;
        if is_running(&id) {
            let _ = kill_server(&id).await;
        }
    });
    Ok(())
}

/// Ends the server process right away, without saving.
pub async fn kill_server(id: &str) -> crate::Result<()> {
    let pid = SESSIONS
        .get(id)
        .and_then(|session| session.lock().ok().and_then(|s| s.pid));
    if let Some(pid) = pid {
        let system = System::new();
        let mut system = system;
        system.refresh_processes(
            ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
            true,
        );
        if let Some(process) = system.process(Pid::from_u32(pid)) {
            process.kill();
        }
    }
    Ok(())
}

/// Stops every running server, for when the app closes.
pub async fn stop_all_servers() {
    let ids = SESSIONS
        .iter()
        .map(|entry| entry.key().clone())
        .collect::<Vec<_>>();
    for id in &ids {
        let _ = stop_server(id).await;
    }
    for _ in 0..100 {
        if !ids.iter().any(|id| is_running(id)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for id in &ids {
        let _ = kill_server(id).await;
    }
}

/// Console lines after `after` (all kept lines when `None`).
pub fn server_console(id: &str, after: Option<u64>) -> Vec<ConsoleLine> {
    let Some(session) = SESSIONS.get(id).map(|s| s.clone()) else {
        return Vec::new();
    };
    let s = lock(&session);
    s.console
        .iter()
        .filter(|line| after.is_none_or(|after| line.seq > after))
        .cloned()
        .collect()
}

pub fn server_status(id: &str) -> ServerStatus {
    let session = session(id);
    let mut s = lock(&session);
    let (cpu_percent, memory_bytes) = match (s.pid, s.system.as_mut()) {
        (Some(pid), Some(system)) => {
            let pid = Pid::from_u32(pid);
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );
            system.process(pid).map_or((None, None), |process| {
                (Some(process.cpu_usage()), Some(process.memory()))
            })
        }
        _ => (None, None),
    };
    ServerStatus {
        state: s.state,
        started: s.started.filter(|_| s.state != ServerState::Offline),
        players: s.players.iter().cloned().collect(),
        cpu_percent,
        memory_bytes,
        exit_code: s.exit_code,
        public_address: s.public.clone(),
        public_error: s.public_error.clone(),
        reachability: s.reachability,
        lan_address: (s.state != ServerState::Offline)
            .then(|| public::lan_address(s.port))
            .flatten(),
    }
}

/// Checks the public address once the server accepts players.
async fn check_when_running(session: &Arc<Mutex<Session>>) {
    // Big modpacks can take minutes to start.
    for _ in 0..450 {
        match lock(session).state {
            ServerState::Running => break,
            ServerState::Starting => {}
            ServerState::Stopping | ServerState::Offline => return,
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    check_reachability(session).await;
}

async fn check_reachability(session: &Arc<Mutex<Session>>) {
    let address = {
        let mut s = lock(session);
        let Some(address) = s.public.as_ref().map(|x| x.address.clone()) else {
            return;
        };
        s.reachability = Reachability::Checking;
        address
    };
    let result = public::check_reachable(&address).await;
    let mut s = lock(session);
    s.reachability = match result {
        Ok(true) => Reachability::Reachable,
        Ok(false) => {
            s.push(
                ConsoleStream::App,
                format!(
                    "{address} couldn't be reached from the internet. The router may block it; linking playit.gg gives an address that works."
                ),
            );
            Reachability::Unreachable
        }
        Err(error) => {
            tracing::warn!("Checking {address} failed: {error}");
            Reachability::Unchecked
        }
    };
}

/// Checks again whether players outside this network can join.
pub async fn check_server_reachability(id: &str) {
    check_reachability(&session(id)).await;
}

fn lock(session: &Mutex<Session>) -> std::sync::MutexGuard<'_, Session> {
    session
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_players_and_readiness() {
        let mut session = Session::new();
        session.state = ServerState::Starting;
        session.push(
            ConsoleStream::Output,
            "[12:00:00] [Server thread/INFO]: Done (3.2s)! For help, type \"help\""
                .to_string(),
        );
        assert_eq!(session.state, ServerState::Running);
        session.push(
            ConsoleStream::Output,
            "[12:00:01] [Server thread/INFO]: Steve_1 joined the game"
                .to_string(),
        );
        session.push(
            ConsoleStream::Output,
            "[12:00:02] [Server thread/INFO]: <Alex> I joined the game"
                .to_string(),
        );
        assert_eq!(session.players.iter().collect::<Vec<_>>(), ["Steve_1"]);
        session.push(
            ConsoleStream::Output,
            "[12:00:03] [Server thread/INFO]: Steve_1 left the game"
                .to_string(),
        );
        assert!(session.players.is_empty());
    }

    #[test]
    fn keeps_the_last_lines() {
        let mut session = Session::new();
        for index in 0..(CONSOLE_LINES + 10) {
            session.push(ConsoleStream::Output, index.to_string());
        }
        assert_eq!(session.console.len(), CONSOLE_LINES);
        assert_eq!(session.console.front().unwrap().seq, 10);
    }
}
