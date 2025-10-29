use chrono::{DateTime, NaiveDateTime, Utc};
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::SinkExt;
use paperclip::actix::Apiv2Schema;
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tracing::*;

const MAX_ENTRIES: usize = 200000;

#[derive(Clone, Serialize, Apiv2Schema)]
pub struct JournalEntry {
    cursor: String,
    realtime_timestamp: u64,
    timestamp: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    boot_id: Option<String>,
}

#[derive(Clone, Serialize, Apiv2Schema)]
pub struct JournalResponse {
    pub entries: Vec<JournalEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

struct JournalService {
    entries: Vec<JournalEntry>,
    last_cursor: Option<String>,
    senders: Vec<Sender<String>>,
    error: Option<String>,
    #[allow(dead_code)]
    main_loop_thread: thread::JoinHandle<()>,
}

lazy_static! {
    static ref JOURNAL_SERVICE: Arc<Mutex<JournalService>> = Arc::new(Mutex::new(JournalService {
        entries: Vec::new(),
        last_cursor: None,
        senders: Vec::new(),
        error: Some("journalctl stream not initialized".to_string()),
        main_loop_thread: thread::spawn(run_main_loop),
    }));
}

pub fn ask_for_client() -> Receiver<String> {
    let (mut sender, receiver) = channel(10240);

    let mut journal_service = JOURNAL_SERVICE.lock().unwrap();
    let snapshot = JournalResponse {
        entries: journal_service.entries.clone(),
        error: journal_service.error.clone(),
    };
    if let Ok(serialized) = serde_json::to_string(&snapshot) {
        let _ = futures::executor::block_on(sender.send(serialized));
    }
    journal_service.senders.push(sender);

    receiver
}

pub fn entries(start: Option<usize>, size: Option<usize>) -> JournalResponse {
    let journal_service = JOURNAL_SERVICE.lock().unwrap();

    let entries = journal_service
        .entries
        .iter()
        .skip(start.unwrap_or_default())
        .take(size.unwrap_or(journal_service.entries.len()))
        .cloned()
        .collect::<Vec<_>>();

    JournalResponse {
        entries,
        error: journal_service.error.clone(),
    }
}

fn run_main_loop() {
    loop {
        match stream_journal() {
            Ok(()) => {
                set_error(Some("journalctl terminated".to_string()));
                thread::sleep(Duration::from_secs(3));
            }
            Err(error) => {
                warn!("{error}");
                set_error(Some(error));
                thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

fn stream_journal() -> Result<(), String> {
    debug!("Starting journalctl follower thread");
    let mut child = Command::new("journalctl")
        .args(["-o", "json", "--boot", "--lines=all", "--follow"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| format!("Failed to spawn journalctl: {err}"))?;

    clear_error();

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "journalctl stdout is unavailable".to_string())?;
    let reader = BufReader::new(stdout);

    for line in reader.lines() {
        let line = line.map_err(|err| format!("Failed to read journalctl output: {err}"))?;
        if line.trim().is_empty() {
            continue;
        }

        match parse_entry(&line) {
            Ok(entry) => add_entry(entry),
            Err(error) => debug!("Skipping journal entry: {error}"),
        }
    }

    let status = child
        .wait()
        .map_err(|err| format!("Failed to wait on journalctl: {err}"))?;
    Err(format!("journalctl exited with status: {status}"))
}

fn parse_entry(line: &str) -> Result<JournalEntry, String> {
    let value: Value = serde_json::from_str(line)
        .map_err(|err| format!("Invalid journalctl json payload: {err}"))?;

    let cursor = value
        .get("__CURSOR")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing __CURSOR".to_string())?
        .to_string();

    let realtime_timestamp = value
        .get("__REALTIME_TIMESTAMP")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or_default();

    let timestamp = format_timestamp(realtime_timestamp);

    let message = value
        .get("MESSAGE")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let priority = value
        .get("PRIORITY")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u8>().ok());

    let identifier = value
        .get("SYSLOG_IDENTIFIER")
        .or_else(|| value.get("_SYSTEMD_UNIT"))
        .or_else(|| value.get("_COMM"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    let pid = value
        .get("_PID")
        .or_else(|| value.get("SYSLOG_PID"))
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<u32>().ok());

    let unit = value
        .get("_SYSTEMD_UNIT")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    let hostname = value
        .get("_HOSTNAME")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    let boot_id = value
        .get("_BOOT_ID")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    Ok(JournalEntry {
        cursor,
        realtime_timestamp,
        timestamp,
        message,
        priority,
        identifier,
        pid,
        unit,
        hostname,
        boot_id,
    })
}

fn add_entry(entry: JournalEntry) {
    let mut journal_service = JOURNAL_SERVICE.lock().unwrap();

    if journal_service
        .last_cursor
        .as_ref()
        .map(|cursor| cursor == &entry.cursor)
        .unwrap_or(false)
    {
        return;
    }

    journal_service.last_cursor = Some(entry.cursor.clone());
    journal_service.entries.push(entry.clone());

    if journal_service.entries.len() > MAX_ENTRIES {
        let overflow = journal_service.entries.len() - MAX_ENTRIES;
        journal_service.entries.drain(0..overflow);
    }

    let serialized = serde_json::to_string(&JournalResponse {
        entries: vec![entry],
        error: None,
    })
    .unwrap_or_else(|err| {
        warn!("Failed to serialize journal entry for websocket: {err}");
        "{}".to_string()
    });

    journal_service.senders.retain(|sender| {
        let mut sender = sender.clone();
        futures::executor::block_on(sender.send(serialized.clone())).is_ok()
    });
}

fn set_error(error: Option<String>) {
    let mut journal_service = JOURNAL_SERVICE.lock().unwrap();
    journal_service.error = error.clone();

    if let Some(error) = error {
        let serialized = serde_json::to_string(&JournalResponse {
            entries: Vec::new(),
            error: Some(error.clone()),
        })
        .unwrap_or_else(|err| {
            warn!("Failed to serialize journal error payload: {err}");
            "{}".to_string()
        });

        journal_service.senders.retain(|sender| {
            let mut sender = sender.clone();
            futures::executor::block_on(sender.send(serialized.clone())).is_ok()
        });
    }
}

fn clear_error() {
    let mut journal_service = JOURNAL_SERVICE.lock().unwrap();
    journal_service.error = None;
}

fn format_timestamp(microseconds: u64) -> String {
    let seconds = (microseconds / 1_000_000) as i64;
    let nanos = ((microseconds % 1_000_000) * 1_000) as u32;
    match NaiveDateTime::from_timestamp_opt(seconds, nanos) {
        Some(naive) => DateTime::<Utc>::from_utc(naive, Utc).to_rfc3339(),
        None => "unknown".to_string(),
    }
}
