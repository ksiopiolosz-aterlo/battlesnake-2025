// Debug logging module for asynchronous game state logging
//
// This module provides fire-and-forget async logging to avoid blocking
// the main request/response cycle. Each turn's state is written to a JSONL file.

use log::error;
use serde::Serialize;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::types::{Board, Direction};

/// Represents a single debug log entry
#[derive(Debug, Serialize)]
struct DebugLogEntry {
    turn: i32,
    chosen_move: String,
    board: Board,
    timestamp: String,
}

/// Shared debug logger state
/// Uses Arc<Mutex<File>> to allow concurrent async writes from multiple tasks
#[derive(Clone)]
pub struct DebugLogger {
    file: Arc<Mutex<Option<File>>>,
    enabled: bool,
}

impl DebugLogger {
    /// Creates a new debug logger
    /// If enabled is true, initializes the log file (truncating if it exists)
    pub async fn new(enabled: bool, log_file_path: &str) -> Self {
        if !enabled {
            return DebugLogger {
                file: Arc::new(Mutex::new(None)),
                enabled: false,
            };
        }

        // Initialize the log file
        match OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_file_path)
            .await
        {
            Ok(file) => {
                log::info!("Debug logging enabled: {}", log_file_path);
                DebugLogger {
                    file: Arc::new(Mutex::new(Some(file))),
                    enabled: true,
                }
            }
            Err(e) => {
                error!("Failed to create debug log file '{}': {}", log_file_path, e);
                DebugLogger {
                    file: Arc::new(Mutex::new(None)),
                    enabled: false,
                }
            }
        }
    }

    /// Creates a disabled debug logger (no-op)
    pub fn disabled() -> Self {
        DebugLogger {
            file: Arc::new(Mutex::new(None)),
            enabled: false,
        }
    }

    /// Logs a move decision asynchronously (fire-and-forget)
    /// This spawns a tokio task that writes to the file without blocking
    pub fn log_move(&self, turn: i32, board: Board, chosen_move: Direction) {
        if !self.enabled {
            return;
        }

        let file_handle = self.file.clone();
        let chosen_move_str = chosen_move.as_str().to_string();

        // Spawn fire-and-forget task
        tokio::spawn(async move {
            Self::log_move_internal(file_handle, turn, board, chosen_move_str).await;
        });
    }

    /// Internal async function that performs the actual file write
    async fn log_move_internal(
        file_handle: Arc<Mutex<Option<File>>>,
        turn: i32,
        board: Board,
        chosen_move: String,
    ) {
        let mut file_guard = file_handle.lock().await;

        if let Some(file) = file_guard.as_mut() {
            let entry = DebugLogEntry {
                turn,
                chosen_move,
                board,
                timestamp: chrono::Utc::now().to_rfc3339(),
            };

            match serde_json::to_string(&entry) {
                Ok(json_line) => {
                    let line_with_newline = format!("{}\n", json_line);
                    if let Err(e) = file.write_all(line_with_newline.as_bytes()).await {
                        error!("Failed to write debug log entry: {}", e);
                    } else {
                        // Flush to ensure data is written to disk
                        if let Err(e) = file.flush().await {
                            error!("Failed to flush debug log: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to serialize debug log entry: {}", e);
                }
            }
        }
    }
}
