//! Timeout Investigation Tool
//!
//! Identifies specific turns with latency exceeding the time budget.
//! Extracts board state for these turns to understand what conditions
//! cause time estimation failures.
//!
//! Usage:
//!   cargo run --release --bin find_timeouts -- <log_directory> [threshold_ms]
//!
//! Output:
//!   - List of all turns exceeding threshold (default: 400ms)
//!   - Exports board states to JSON for detailed analysis
//!   - Statistical analysis of timeout patterns

use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::path::Path;

#[derive(Debug, Clone)]
struct TimeoutEntry {
    game_name: String,
    turn: u64,
    latency_ms: u64,
    num_snakes: usize,
    num_food: usize,
    board_width: u64,
    board_height: u64,
    board_state: Value,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <log_directory> [threshold_ms]", args[0]);
        eprintln!("Example: {} tests/fixtures/1v1_self/ 400", args[0]);
        std::process::exit(1);
    }

    let log_dir = &args[1];
    let threshold_ms: u64 = if args.len() >= 3 {
        args[2].parse().expect("Invalid threshold value")
    } else {
        400
    };

    println!("============================================================");
    println!("Timeout Investigation: Latencies > {}ms", threshold_ms);
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

    let mut all_timeouts: Vec<TimeoutEntry> = Vec::new();

    for path in &paths {
        let game_name = path.file_name().unwrap().to_str().unwrap().to_string();

        match find_timeouts_in_game(&path, threshold_ms) {
            Ok(mut timeouts) => {
                all_timeouts.append(&mut timeouts);
            }
            Err(e) => {
                eprintln!("Error analyzing {}: {}", game_name, e);
            }
        }
    }

    if all_timeouts.is_empty() {
        println!("✓ No timeouts found (all moves < {}ms)", threshold_ms);
        return;
    }

    println!("Found {} timeout instances across {} files",
        all_timeouts.len(),
        all_timeouts.iter().map(|t| &t.game_name).collect::<std::collections::HashSet<_>>().len()
    );
    println!();

    // Sort by latency descending
    all_timeouts.sort_by_key(|t| std::cmp::Reverse(t.latency_ms));

    print_timeout_report(&all_timeouts);
    export_timeout_states(&all_timeouts);
}

fn find_timeouts_in_game(path: &Path, threshold_ms: u64) -> Result<Vec<TimeoutEntry>, String> {
    let file = File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let reader = BufReader::new(file);
    let game_name = path.file_name().unwrap().to_str().unwrap().to_string();
    let mut timeouts = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;

        if line.trim().is_empty() {
            continue;
        }

        let entry: Value = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let turn = entry["turn"].as_u64().unwrap_or(0);

        // Check latency in snakes array
        if let Some(snakes) = entry["board"]["snakes"].as_array() {
            for snake in snakes {
                if let Some(latency_str) = snake["latency"].as_str() {
                    if let Ok(latency) = latency_str.parse::<u64>() {
                        if latency > threshold_ms {
                            let num_snakes = snakes.len();
                            let num_food = entry["board"]["food"]
                                .as_array()
                                .map(|f| f.len())
                                .unwrap_or(0);
                            let board_width = entry["board"]["width"].as_u64().unwrap_or(11);
                            let board_height = entry["board"]["height"].as_u64().unwrap_or(11);

                            timeouts.push(TimeoutEntry {
                                game_name: game_name.clone(),
                                turn,
                                latency_ms: latency,
                                num_snakes,
                                num_food,
                                board_width,
                                board_height,
                                board_state: entry.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(timeouts)
}

fn print_timeout_report(timeouts: &[TimeoutEntry]) {
    println!("============================================================");
    println!("TIMEOUT INSTANCES (sorted by latency)");
    println!("============================================================");
    println!("{:<25} {:>6} {:>8} {:>8} {:>6} {:>8}",
        "Game", "Turn", "Latency", "Snakes", "Food", "Board");
    println!("{:-<25} {:-<6} {:-<8} {:-<8} {:-<6} {:-<8}",
        "", "", "", "", "", "");

    for timeout in timeouts {
        println!("{:<25} {:>6} {:>7}ms {:>8} {:>6} {:>4}x{:<2}",
            timeout.game_name,
            timeout.turn,
            timeout.latency_ms,
            timeout.num_snakes,
            timeout.num_food,
            timeout.board_width,
            timeout.board_height
        );
    }
    println!();

    // Statistical analysis
    println!("============================================================");
    println!("PATTERN ANALYSIS");
    println!("============================================================");

    let avg_latency = timeouts.iter().map(|t| t.latency_ms).sum::<u64>() as f64 / timeouts.len() as f64;
    let max_latency = timeouts.iter().map(|t| t.latency_ms).max().unwrap_or(0);
    let min_latency = timeouts.iter().map(|t| t.latency_ms).min().unwrap_or(0);

    println!("Latency statistics:");
    println!("  Average: {:.1}ms", avg_latency);
    println!("  Maximum: {}ms", max_latency);
    println!("  Minimum: {}ms", min_latency);
    println!();

    // Analyze by snake count
    let by_snakes: std::collections::HashMap<usize, Vec<&TimeoutEntry>> = timeouts.iter()
        .fold(std::collections::HashMap::new(), |mut acc, t| {
            acc.entry(t.num_snakes).or_insert_with(Vec::new).push(t);
            acc
        });

    println!("By number of snakes:");
    for (num, entries) in by_snakes.iter() {
        println!("  {} snakes: {} timeouts ({:.1}%)",
            num,
            entries.len(),
            100.0 * entries.len() as f64 / timeouts.len() as f64
        );
    }
    println!();

    // Analyze by turn number
    let early_game = timeouts.iter().filter(|t| t.turn < 50).count();
    let mid_game = timeouts.iter().filter(|t| t.turn >= 50 && t.turn < 150).count();
    let late_game = timeouts.iter().filter(|t| t.turn >= 150).count();

    println!("By game phase:");
    println!("  Early game (turn < 50):    {} timeouts ({:.1}%)",
        early_game, 100.0 * early_game as f64 / timeouts.len() as f64);
    println!("  Mid game (50 ≤ turn < 150): {} timeouts ({:.1}%)",
        mid_game, 100.0 * mid_game as f64 / timeouts.len() as f64);
    println!("  Late game (turn ≥ 150):     {} timeouts ({:.1}%)",
        late_game, 100.0 * late_game as f64 / timeouts.len() as f64);
    println!();

    println!("============================================================");
}

fn export_timeout_states(timeouts: &[TimeoutEntry]) {
    // Export the top 10 worst timeouts for detailed analysis
    let top_timeouts: Vec<_> = timeouts.iter().take(10).collect();

    for (idx, timeout) in top_timeouts.iter().enumerate() {
        let filename = format!("/tmp/{}_{}.json",
            timeout.game_name.replace(".jsonl", ""),
            timeout.turn
        );

        match File::create(&filename) {
            Ok(mut file) => {
                if let Err(e) = writeln!(file, "{}",
                    serde_json::to_string_pretty(&timeout.board_state).unwrap()
                ) {
                    eprintln!("Failed to write {}: {}", filename, e);
                } else if idx == 0 {
                    println!("Exported top 10 timeout board states to /tmp/");
                }
            }
            Err(e) => {
                eprintln!("Failed to create {}: {}", filename, e);
            }
        }
    }
    println!();
}
