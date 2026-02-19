//! Watch stream implementation

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WatchEventType {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEvent {
    pub event_type: WatchEventType,
    pub key: String,
    pub value: Option<Vec<u8>>,
    pub prev_value: Option<Vec<u8>>,
    pub revision: i64,
}

impl WatchEvent {
    pub fn added(key: String, value: Vec<u8>, revision: i64) -> Self {
        Self {
            event_type: WatchEventType::Added,
            key,
            value: Some(value),
            prev_value: None,
            revision,
        }
    }

    pub fn modified(key: String, value: Vec<u8>, prev_value: Vec<u8>, revision: i64) -> Self {
        Self {
            event_type: WatchEventType::Modified,
            key,
            value: Some(value),
            prev_value: Some(prev_value),
            revision,
        }
    }

    pub fn deleted(key: String, prev_value: Vec<u8>, revision: i64) -> Self {
        Self {
            event_type: WatchEventType::Deleted,
            key,
            value: None,
            prev_value: Some(prev_value),
            revision,
        }
    }
}

/// Watch event broadcaster
#[derive(Clone)]
pub struct WatchBroadcaster {
    tx: broadcast::Sender<WatchEvent>,
}

impl WatchBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn send(&self, event: WatchEvent) {
        // Ignore errors if no receivers
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self, prefix: &str) -> Watcher {
        Watcher {
            rx: BroadcastStream::new(self.tx.subscribe()),
            prefix: prefix.to_string(),
        }
    }
}

impl Default for WatchBroadcaster {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// A watcher that receives events for a specific prefix
pub struct Watcher {
    rx: BroadcastStream<WatchEvent>,
    prefix: String,
}

impl Watcher {
    /// Receive the next event
    pub async fn next(&mut self) -> Option<WatchEvent> {
        loop {
            match self.rx.next().await {
                Some(Ok(event)) => {
                    if event.key.starts_with(&self.prefix) {
                        return Some(event);
                    }
                }
                Some(Err(_)) => continue, // Lagged, skip
                None => return None,
            }
        }
    }
}
