use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::supervisor::ProcessState;

/// Everything that happens in the daemon is one of these. The UI is a
/// projection over the event stream, not a separate source of truth.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    ProcessState {
        command_id: String,
        state: ProcessState,
        pid: Option<u32>,
        exit_code: Option<i32>,
        ts: chrono::DateTime<chrono::Utc>,
    },
    ProcessStats {
        command_id: String,
        cpu_percent: f32,
        mem_bytes: u64,
        ports: Vec<u16>,
        ts: chrono::DateTime<chrono::Utc>,
    },
    LogLine {
        command_id: String,
        stream: LogStream,
        line: String,
        ts: chrono::DateTime<chrono::Utc>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    Pty,
    System,
}

const RING_CAPACITY: usize = 4096;

/// Append-only ring buffer plus a broadcast channel so late-joining
/// WebSocket clients can replay recent history before switching to live
/// updates.
pub struct EventBus {
    buffer: Mutex<VecDeque<Event>>,
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            buffer: Mutex::new(VecDeque::with_capacity(RING_CAPACITY)),
            sender,
        }
    }

    pub fn publish(&self, event: Event) {
        {
            let mut buf = self.buffer.lock().unwrap();
            if buf.len() == RING_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(event.clone());
        }
        // No receivers yet is fine; broadcast::send errors only mean "nobody
        // is listening right now", which is not an error for us.
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub fn replay(&self) -> Vec<Event> {
        self.buffer.lock().unwrap().iter().cloned().collect()
    }
}
