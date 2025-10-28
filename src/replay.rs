// Replay module for analyzing historical game states and debugging decision-making
//
// This module provides functionality to:
// 1. Parse JSONL debug logs
// 2. Replay the algorithm on historical states
// 3. Compare expected vs actual moves
// 4. Generate detailed analysis reports

use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crate::bot::Bot;
use crate::config::Config;
use crate::types::{Board, Direction};

/// Represents a single log entry from the debug JSONL file
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogEntry {
    pub turn: i32,
    pub chosen_move: String,
    pub board: Board,
    pub timestamp: String,
}

/// Result of replaying a single turn
#[derive(Debug, Clone)]
pub struct ReplayResult {
    pub turn: i32,
    pub original_move: Direction,
    pub replayed_move: Direction,
    pub matches: bool,
    pub original_score: i32,
    pub replayed_score: i32,
    pub search_depth: u8,
    pub computation_time_ms: u128,
}

/// Statistics for a complete replay session
#[derive(Debug, Default)]
pub struct ReplayStats {
    pub total_turns: usize,
    pub matches: usize,
    pub mismatches: usize,
    pub match_rate: f64,
}

/// Replay engine for analyzing debug logs
pub struct ReplayEngine {
    config: Config,
    verbose: bool,
}

impl ReplayEngine {
    /// Creates a new replay engine with the given configuration
    pub fn new(config: Config, verbose: bool) -> Self {
        ReplayEngine { config, verbose }
    }

    /// Loads all log entries from a JSONL file
    pub fn load_log_file<P: AsRef<Path>>(
        &self,
        log_path: P,
    ) -> Result<Vec<LogEntry>, String> {
        let file = File::open(log_path.as_ref())
            .map_err(|e| format!("Failed to open log file: {}", e))?;

        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| format!("Failed to read line {}: {}", line_num + 1, e))?;

            if line.trim().is_empty() {
                continue;
            }

            let entry: LogEntry = serde_json::from_str(&line).map_err(|e| {
                format!(
                    "Failed to parse JSON on line {}: {}",
                    line_num + 1,
                    e
                )
            })?;

            entries.push(entry);
        }

        info!("Loaded {} log entries", entries.len());
        Ok(entries)
    }

    /// Replays the algorithm on a single board state
    /// Returns the move that would be chosen and the score
    pub fn replay_turn(
        &self,
        board: &Board,
        our_snake_id: &str,
    ) -> Result<(Direction, i32, u8, u128), String> {
        // Find our snake in the board
        let our_snake = board
            .snakes
            .iter()
            .find(|s| s.id == our_snake_id)
            .ok_or_else(|| format!("Snake with id '{}' not found in board state", our_snake_id))?;

        let start_time = Instant::now();

        // Use Bot's internal computation logic
        let shared = Arc::new(crate::bot::SharedSearchState::new());
        let shared_clone = shared.clone();
        let board_clone = board.clone();
        let our_snake_clone = our_snake.clone();
        let config_clone = self.config.clone();

        // Run computation synchronously (we're already in a non-async context)
        std::thread::spawn(move || {
            Bot::compute_best_move_internal(
                &board_clone,
                &our_snake_clone,
                shared_clone,
                start_time,
                &config_clone,
            )
        });

        // Wait for completion or timeout
        let effective_budget = self.config.timing.effective_budget_ms();
        let poll_interval = std::time::Duration::from_millis(10);

        loop {
            std::thread::sleep(poll_interval);
            let elapsed = start_time.elapsed().as_millis() as u64;

            if elapsed >= effective_budget || shared.search_complete.load(Ordering::Acquire) {
                break;
            }
        }

        let computation_time = start_time.elapsed().as_millis();
        let (move_idx, score) = shared.get_best();
        let depth = shared.current_depth.load(Ordering::Acquire);

        let direction = Bot::index_to_direction(move_idx, &self.config);

        Ok((direction, score, depth, computation_time))
    }

    /// Replays a single log entry and compares the result
    pub fn replay_entry(&self, entry: &LogEntry) -> Result<ReplayResult, String> {
        if self.verbose {
            info!("Replaying turn {}...", entry.turn);
        }

        // Assume the first snake in the log is our snake (the one that made the logged move)
        let our_snake = entry
            .board
            .snakes
            .first()
            .ok_or("No snakes found in board state")?;

        let original_move = Self::parse_direction(&entry.chosen_move)?;

        let (replayed_move, replayed_score, search_depth, computation_time) =
            self.replay_turn(&entry.board, &our_snake.id)?;

        let matches = original_move == replayed_move;

        let result = ReplayResult {
            turn: entry.turn,
            original_move,
            replayed_move,
            matches,
            original_score: 0, // We don't log scores in the original debug output
            replayed_score,
            search_depth,
            computation_time_ms: computation_time,
        };

        if self.verbose {
            if matches {
                info!(
                    "Turn {}: ✓ MATCH - {} (score: {}, depth: {}, time: {}ms)",
                    entry.turn,
                    replayed_move.as_str(),
                    replayed_score,
                    search_depth,
                    computation_time
                );
            } else {
                warn!(
                    "Turn {}: ✗ MISMATCH - Original: {}, Replayed: {} (score: {}, depth: {}, time: {}ms)",
                    entry.turn,
                    original_move.as_str(),
                    replayed_move.as_str(),
                    replayed_score,
                    search_depth,
                    computation_time
                );
            }
        }

        Ok(result)
    }

    /// Replays all entries in a log file
    pub fn replay_all(&self, entries: &[LogEntry]) -> Result<Vec<ReplayResult>, String> {
        let mut results = Vec::new();

        for entry in entries {
            match self.replay_entry(entry) {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Failed to replay turn {}: {}", entry.turn, e);
                }
            }
        }

        Ok(results)
    }

    /// Replays specific turns from a log file
    pub fn replay_turns(
        &self,
        entries: &[LogEntry],
        turn_numbers: &[i32],
    ) -> Result<Vec<ReplayResult>, String> {
        let mut results = Vec::new();

        for turn_num in turn_numbers {
            let entry = entries
                .iter()
                .find(|e| e.turn == *turn_num)
                .ok_or_else(|| format!("Turn {} not found in log file", turn_num))?;

            match self.replay_entry(entry) {
                Ok(result) => results.push(result),
                Err(e) => {
                    warn!("Failed to replay turn {}: {}", turn_num, e);
                }
            }
        }

        Ok(results)
    }

    /// Generates statistics from replay results
    pub fn generate_stats(&self, results: &[ReplayResult]) -> ReplayStats {
        let total_turns = results.len();
        let matches = results.iter().filter(|r| r.matches).count();
        let mismatches = total_turns - matches;
        let match_rate = if total_turns > 0 {
            (matches as f64 / total_turns as f64) * 100.0
        } else {
            0.0
        };

        ReplayStats {
            total_turns,
            matches,
            mismatches,
            match_rate,
        }
    }

    /// Prints a detailed report of replay results
    pub fn print_report(&self, results: &[ReplayResult]) {
        let stats = self.generate_stats(results);

        println!("\n═══════════════════════════════════════════════════════════");
        println!("                    REPLAY REPORT");
        println!("═══════════════════════════════════════════════════════════");
        println!("Total Turns:    {}", stats.total_turns);
        println!("Matches:        {} ({:.1}%)", stats.matches, stats.match_rate);
        println!("Mismatches:     {}", stats.mismatches);
        println!("═══════════════════════════════════════════════════════════\n");

        if !results.is_empty() {
            let avg_time: f64 = results.iter().map(|r| r.computation_time_ms as f64).sum::<f64>()
                / results.len() as f64;
            let avg_depth: f64 =
                results.iter().map(|r| r.search_depth as f64).sum::<f64>() / results.len() as f64;

            println!("Average Search Depth:       {:.1}", avg_depth);
            println!("Average Computation Time:   {:.1}ms\n", avg_time);
        }

        // Show mismatches in detail
        let mismatches: Vec<_> = results.iter().filter(|r| !r.matches).collect();
        if !mismatches.is_empty() {
            println!("═══════════════════════════════════════════════════════════");
            println!("                  DETAILED MISMATCHES");
            println!("═══════════════════════════════════════════════════════════");

            for result in mismatches {
                println!(
                    "Turn {}: {} → {} (score: {}, depth: {}, time: {}ms)",
                    result.turn,
                    result.original_move.as_str(),
                    result.replayed_move.as_str(),
                    result.replayed_score,
                    result.search_depth,
                    result.computation_time_ms
                );
            }
            println!();
        }
    }

    /// Validates that specific expected moves were made
    pub fn validate_expected_moves(
        &self,
        entries: &[LogEntry],
        expected_moves: &[(i32, Vec<Direction>)], // (turn, acceptable_moves)
    ) -> Result<(), String> {
        for (turn, acceptable) in expected_moves {
            let entry = entries
                .iter()
                .find(|e| e.turn == *turn)
                .ok_or_else(|| format!("Turn {} not found in log", turn))?;

            let actual_move = Self::parse_direction(&entry.chosen_move)?;

            if !acceptable.contains(&actual_move) {
                return Err(format!(
                    "Turn {}: Expected one of {:?}, but got {}",
                    turn,
                    acceptable.iter().map(|d| d.as_str()).collect::<Vec<_>>(),
                    actual_move.as_str()
                ));
            }
        }

        Ok(())
    }

    /// Helper to parse direction string
    fn parse_direction(s: &str) -> Result<Direction, String> {
        match s.to_lowercase().as_str() {
            "up" => Ok(Direction::Up),
            "down" => Ok(Direction::Down),
            "left" => Ok(Direction::Left),
            "right" => Ok(Direction::Right),
            _ => Err(format!("Invalid direction: {}", s)),
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_direction() {
        // Test valid directions
        assert_eq!(
            ReplayEngine::parse_direction("up").unwrap(),
            Direction::Up
        );
        assert_eq!(
            ReplayEngine::parse_direction("down").unwrap(),
            Direction::Down
        );
        assert_eq!(
            ReplayEngine::parse_direction("left").unwrap(),
            Direction::Left
        );
        assert_eq!(
            ReplayEngine::parse_direction("right").unwrap(),
            Direction::Right
        );

        // Test case insensitivity
        assert_eq!(
            ReplayEngine::parse_direction("UP").unwrap(),
            Direction::Up
        );
        assert_eq!(
            ReplayEngine::parse_direction("Down").unwrap(),
            Direction::Down
        );

        // Test invalid direction
        assert!(ReplayEngine::parse_direction("invalid").is_err());
    }
}
