use std::collections::HashMap;
use std::io::BufRead;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::config::{CommandDef, DeckConfig};
use crate::events::{Event, EventBus, LogStream};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed,
}

type SharedChild = Arc<Mutex<Box<dyn Child + Send + Sync>>>;

struct ManagedProcess {
    state: ProcessState,
    pid: Option<u32>,
    exit_code: Option<i32>,
    child: Option<SharedChild>,
    /// Keeps the PTY master (and therefore the slave's controlling
    /// terminal) alive for as long as the process runs. Never read after
    /// spawn in v1; a later resize/replay feature would use it.
    _master: Option<Box<dyn MasterPty + Send>>,
    /// Spawns of the same command id are numbered so a stale exit
    /// notification from a process we already replaced can't clobber the
    /// new one's state.
    generation: u64,
}

impl Default for ManagedProcess {
    fn default() -> Self {
        Self {
            state: ProcessState::Stopped,
            pid: None,
            exit_code: None,
            child: None,
            _master: None,
            generation: 0,
        }
    }
}

pub struct Supervisor {
    project_dir: std::path::PathBuf,
    commands: HashMap<String, CommandDef>,
    bus: Arc<EventBus>,
    processes: Mutex<HashMap<String, ManagedProcess>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessStatus {
    pub state: ProcessState,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
}

impl Supervisor {
    pub fn new(project_dir: std::path::PathBuf, config: &DeckConfig, bus: Arc<EventBus>) -> Arc<Self> {
        let commands = config.commands.iter().map(|c| (c.id.clone(), c.clone())).collect();
        Arc::new(Self {
            project_dir,
            commands,
            bus,
            processes: Mutex::new(HashMap::new()),
        })
    }

    pub fn status_of(&self, id: &str) -> ProcessStatus {
        let procs = self.processes.lock().unwrap();
        match procs.get(id) {
            Some(p) => ProcessStatus { state: p.state, pid: p.pid, exit_code: p.exit_code },
            None => ProcessStatus { state: ProcessState::Stopped, pid: None, exit_code: None },
        }
    }

    pub fn auto_start_all(self: &Arc<Self>) {
        let ids: Vec<String> = self.commands.values().filter(|c| c.auto_start).map(|c| c.id.clone()).collect();
        for id in ids {
            if let Err(err) = self.run(&id) {
                tracing::warn!("auto-start failed for {id}: {err:#}");
            }
        }
    }

    fn publish_state(&self, id: &str, state: ProcessState, pid: Option<u32>, exit_code: Option<i32>) {
        self.bus.publish(Event::ProcessState {
            command_id: id.to_string(),
            state,
            pid,
            exit_code,
            ts: chrono::Utc::now(),
        });
    }

    pub fn run(self: &Arc<Self>, id: &str) -> anyhow::Result<()> {
        let def = self
            .commands
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown command id: {id}"))?;

        {
            let procs = self.processes.lock().unwrap();
            if let Some(p) = procs.get(id) {
                if matches!(p.state, ProcessState::Running | ProcessState::Starting) {
                    return Err(anyhow::anyhow!("{id} is already running"));
                }
            }
        }

        let generation = {
            let mut procs = self.processes.lock().unwrap();
            let gen = procs.get(id).map(|p| p.generation + 1).unwrap_or(1);
            procs.insert(
                id.to_string(),
                ManagedProcess { state: ProcessState::Starting, generation: gen, ..Default::default() },
            );
            gen
        };
        self.publish_state(id, ProcessState::Starting, None, None);

        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows: 40, cols: 160, pixel_width: 0, pixel_height: 0 })?;

        let mut cmd = CommandBuilder::new(if cfg!(windows) { "cmd" } else { "/bin/sh" });
        if cfg!(windows) {
            cmd.arg("/C");
        } else {
            cmd.arg("-c");
        }
        cmd.arg(&def.cmd);
        let cwd = def
            .cwd
            .as_ref()
            .map(|c| self.project_dir.join(c))
            .unwrap_or_else(|| self.project_dir.clone());
        cmd.cwd(&cwd);
        for (k, v) in &def.env {
            cmd.env(k, v);
        }

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let pid = child.process_id();
        let shared_child: SharedChild = Arc::new(Mutex::new(child));

        let reader = pair.master.try_clone_reader()?;

        {
            let mut procs = self.processes.lock().unwrap();
            if let Some(p) = procs.get_mut(id) {
                if p.generation == generation {
                    p.state = ProcessState::Running;
                    p.pid = pid;
                    p.child = Some(shared_child.clone());
                    p._master = Some(pair.master);
                }
            }
        }
        self.publish_state(id, ProcessState::Running, pid, None);

        spawn_log_reader(self.bus.clone(), id.to_string(), reader);
        spawn_poller(self.clone(), id.to_string(), generation, shared_child);

        Ok(())
    }

    pub fn stop(&self, id: &str) -> anyhow::Result<()> {
        let child = {
            let mut procs = self.processes.lock().unwrap();
            let p = procs
                .get_mut(id)
                .ok_or_else(|| anyhow::anyhow!("unknown command id: {id}"))?;
            if !matches!(p.state, ProcessState::Running | ProcessState::Starting) {
                return Ok(());
            }
            p.state = ProcessState::Stopping;
            p.child.clone()
        };
        self.publish_state(id, ProcessState::Stopping, None, None);
        if let Some(child) = child {
            child.lock().unwrap().kill()?;
        }
        Ok(())
    }

    pub fn restart(self: &Arc<Self>, id: &str) -> anyhow::Result<()> {
        self.stop(id)?;
        // The poller thread finalizes the stopped state asynchronously; give
        // it a moment so the port the old process held is released before
        // we start the new one. Best-effort, not a hard guarantee.
        std::thread::sleep(Duration::from_millis(300));
        self.run(id)
    }

    fn running_pids(&self) -> Vec<(String, u32)> {
        self.processes
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, p)| {
                if p.state == ProcessState::Running {
                    p.pid.map(|pid| (id.clone(), pid))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Polls CPU, memory, and listening ports for every running process on
    /// a fixed interval and publishes a stats event for each. Run as a
    /// background task for the daemon's lifetime.
    pub async fn run_stats_loop(self: Arc<Self>) {
        let mut sys = sysinfo::System::new();
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let targets = self.running_pids();
            if targets.is_empty() {
                continue;
            }
            let pids: Vec<sysinfo::Pid> =
                targets.iter().map(|(_, pid)| sysinfo::Pid::from_u32(*pid)).collect();
            sys.refresh_pids(&pids);
            for (id, pid) in &targets {
                let sysinfo_pid = sysinfo::Pid::from_u32(*pid);
                if let Some(proc_) = sys.process(sysinfo_pid) {
                    self.bus.publish(Event::ProcessStats {
                        command_id: id.clone(),
                        cpu_percent: proc_.cpu_usage(),
                        mem_bytes: proc_.memory(),
                        ports: listening_ports_for_pid(*pid),
                        ts: chrono::Utc::now(),
                    });
                }
            }
        }
    }
}

fn spawn_log_reader(bus: Arc<EventBus>, id: String, reader: Box<dyn std::io::Read + Send>) {
    std::thread::spawn(move || {
        let mut buf_reader = std::io::BufReader::new(reader);
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            match buf_reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf);
                    let line = line.trim_end_matches(['\r', '\n']);
                    if line.is_empty() {
                        continue;
                    }
                    bus.publish(Event::LogLine {
                        command_id: id.clone(),
                        stream: LogStream::Pty,
                        line: line.to_string(),
                        ts: chrono::Utc::now(),
                    });
                }
                Err(_) => break,
            }
        }
    });
}

fn spawn_poller(supervisor: Arc<Supervisor>, id: String, generation: u64, child: SharedChild) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(200));
        let exited = { child.lock().unwrap().try_wait() };
        match exited {
            Ok(Some(status)) => {
                let exit_code = Some(status.exit_code() as i32);
                let final_state = {
                    let mut procs = supervisor.processes.lock().unwrap();
                    let Some(p) = procs.get_mut(&id) else { return };
                    if p.generation != generation {
                        return; // superseded by a newer spawn of the same id
                    }
                    let final_state = if p.state == ProcessState::Stopping || exit_code == Some(0) {
                        ProcessState::Stopped
                    } else {
                        ProcessState::Crashed
                    };
                    p.state = final_state;
                    p.exit_code = exit_code;
                    p.pid = None;
                    p.child = None;
                    p._master = None;
                    final_state
                };
                supervisor.publish_state(&id, final_state, None, exit_code);
                return;
            }
            Ok(None) => continue,
            Err(err) => {
                tracing::warn!("poller for {id} lost track of process: {err:#}");
                return;
            }
        }
    });
}

/// Best-effort TCP listening ports for a pid. Linux-only for v1 (reads
/// /proc directly instead of pulling in a cross-platform netstat crate);
/// other platforms report no ports rather than guessing.
#[cfg(target_os = "linux")]
pub fn listening_ports_for_pid(pid: u32) -> Vec<u16> {
    use std::collections::HashMap as Map;

    let mut inode_to_port: Map<String, u16> = Map::new();
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines().skip(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() < 10 || fields[3] != "0A" {
                    continue; // 0A == TCP_LISTEN
                }
                if let Some(port_hex) = fields[1].split(':').nth(1) {
                    if let Ok(port) = u16::from_str_radix(port_hex, 16) {
                        inode_to_port.insert(fields[9].to_string(), port);
                    }
                }
            }
        }
    }
    if inode_to_port.is_empty() {
        return Vec::new();
    }

    let mut ports = Vec::new();
    if let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) {
        for entry in entries.flatten() {
            if let Ok(link) = std::fs::read_link(entry.path()) {
                if let Some(inode) = link.to_str().and_then(|s| s.strip_prefix("socket:[")).and_then(|s| s.strip_suffix(']')) {
                    if let Some(&port) = inode_to_port.get(inode) {
                        ports.push(port);
                    }
                }
            }
        }
    }
    ports.sort_unstable();
    ports.dedup();
    ports
}

#[cfg(not(target_os = "linux"))]
pub fn listening_ports_for_pid(_pid: u32) -> Vec<u16> {
    Vec::new()
}
