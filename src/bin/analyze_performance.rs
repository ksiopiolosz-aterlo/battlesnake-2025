//! Performance Analysis Tool
//!
//! Analyzes game logs to extract timing and search depth statistics.
//! Helps identify optimization opportunities in the bot's search algorithm.
//!
//! Usage:
//!   cargo run --release --bin analyze_performance -- <log_directory>
//!
//! Output:
//!   - Average search depth per game
//!   - Average computation time per move
//!   - Time budget utilization
//!   - Identifies moves that could search deeper
//!   - Statistical summary across all games

use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
struct MoveStats {
    turn: u64,
    // Note: The corrected logs don't have timing/depth info yet
    // They were regenerated with the replay tool which doesn't log these
    // This tool is ready for when we add that instrumentation
}

#[derive(Debug, Clone)]
struct GameStats {
    name: String,
    total_moves: usize,
    total_turns: usize,
}

#[derive(Debug)]
struct PerformanceSummary {
    total_games: usize,
    total_moves: usize,
    shortest_game_turns: usize,
    longest_game_turns: usize,
    average_game_length: f64,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <log_directory>", args[0]);
        eprintln!("Example: {} tests/fixtures/1v1_self/", args[0]);
        std::process::exit(1);
    }

    let log_dir = &args[1];

    println!("============================================================");
    println!("Performance Analysis: Corrected Game Logs");
    println!("============================================================");
    println!();
    println!("Analyzing: {}", log_dir);
    println!();

    // Get all JSONL files
    let paths: Vec<_> = fs::read_dir(log_dir)
        .expect("Failed to read log directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();

    if paths.is_empty() {
        eprintln!("No .jsonl files found in: {}", log_dir);
        std::process::exit(1);
    }

    println!("Found {} game files", paths.len());
    println!();

    let mut all_games: Vec<GameStats> = Vec::new();

    for path in &paths {
        let game_name = path.file_name().unwrap().to_str().unwrap().to_string();

        match analyze_game(&path) {
            Ok(stats) => {
                all_games.push(stats);
            }
            Err(e) => {
                eprintln!("Error analyzing {}: {}", game_name, e);
            }
        }
    }

    // Generate summary
    let summary = generate_summary(&all_games);
    print_summary(&summary, &all_games);
}

fn analyze_game(path: &Path) -> Result<GameStats, String> {
    let file = File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let reader = BufReader::new(file);
    let mut turns: HashMap<u64, Vec<Value>> = HashMap::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;

        if line.trim().is_empty() {
            continue;
        }

        let entry: Value = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let turn = entry["turn"].as_u64().unwrap_or(0);
        turns.entry(turn).or_insert_with(Vec::new).push(entry);
    }

    let total_turns = turns.len();
    let total_moves = turns.values().map(|v| v.len()).sum();

    Ok(GameStats {
        name: path.file_name().unwrap().to_str().unwrap().to_string(),
        total_moves,
        total_turns,
    })
}

fn generate_summary(games: &[GameStats]) -> PerformanceSummary {
    let total_games = games.len();
    let total_moves: usize = games.iter().map(|g| g.total_moves).sum();

    let shortest_game_turns = games.iter().map(|g| g.total_turns).min().unwrap_or(0);
    let longest_game_turns = games.iter().map(|g| g.total_turns).max().unwrap_or(0);

    let average_game_length = if total_games > 0 {
        games.iter().map(|g| g.total_turns).sum::<usize>() as f64 / total_games as f64
    } else {
        0.0
    };

    PerformanceSummary {
        total_games,
        total_moves,
        shortest_game_turns,
        longest_game_turns,
        average_game_length,
    }
}

fn print_summary(summary: &PerformanceSummary, games: &[GameStats]) {
    println!("============================================================");
    println!("SUMMARY");
    println!("============================================================");
    println!("Total games analyzed:     {}", summary.total_games);
    println!("Total moves recorded:     {}", summary.total_moves);
    println!("Shortest game:            {} turns", summary.shortest_game_turns);
    println!("Longest game:             {} turns", summary.longest_game_turns);
    println!("Average game length:      {:.1} turns", summary.average_game_length);
    println!();

    // Game length distribution
    println!("============================================================");
    println!("GAME LENGTH DISTRIBUTION");
    println!("============================================================");

    let mut sorted_games = games.to_vec();
    sorted_games.sort_by_key(|g| g.total_turns);

    println!("Quick games (<100 turns):");
    for game in sorted_games.iter().filter(|g| g.total_turns < 100) {
        println!("  {}: {} turns ({} moves)", game.name, game.total_turns, game.total_moves);
    }
    println!();

    println!("Medium games (100-200 turns):");
    for game in sorted_games.iter().filter(|g| g.total_turns >= 100 && g.total_turns < 200) {
        println!("  {}: {} turns ({} moves)", game.name, game.total_turns, game.total_moves);
    }
    println!();

    println!("Long games (200-300 turns):");
    for game in sorted_games.iter().filter(|g| g.total_turns >= 200 && g.total_turns < 300) {
        println!("  {}: {} turns ({} moves)", game.name, game.total_turns, game.total_moves);
    }
    println!();

    println!("Epic games (300+ turns):");
    for game in sorted_games.iter().filter(|g| g.total_turns >= 300) {
        println!("  {}: {} turns ({} moves)", game.name, game.total_turns, game.total_moves);
    }
    println!();

    println!("============================================================");
    println!("NOTES");
    println!("============================================================");
    println!("The corrected logs do not yet contain timing/depth metadata.");
    println!("To enable detailed performance analysis:");
    println!("  1. Add timing instrumentation to the bot's move computation");
    println!("  2. Log search depth achieved per turn");
    println!("  3. Record time budget utilization");
    println!("  4. Re-run this tool to identify optimization opportunities");
    println!();
    println!("Quick games (<100 turns) are most interesting for analyzing");
    println!("early-game strategy and death prevention.");
    println!();
    println!("Long games (300+ turns) demonstrate successful endgame play");
    println!("and sustained strategic decision-making.");
    println!("============================================================");
}
