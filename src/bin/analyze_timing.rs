//! Timing Analysis Tool
//!
//! Extracts latency data from game logs to analyze computation time patterns.
//! This helps identify:
//! - Average response times per game
//! - Time budget utilization
//! - Opportunities to search deeper
//! - Correlation between game length and computation time
//!
//! Usage:
//!   cargo run --release --bin analyze_timing -- <log_directory>
//!
//! Output:
//!   - Per-game timing statistics
//!   - Overall timing distribution
//!   - Identifies games with unused time budget
//!   - Suggestions for parameter tuning

use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
struct TimingStats {
    game_name: String,
    total_turns: usize,
    avg_latency_ms: f64,
    max_latency_ms: u64,
    min_latency_ms: u64,
    median_latency_ms: f64,
    samples: Vec<u64>,
}

#[derive(Debug)]
struct TimingSummary {
    total_games: usize,
    overall_avg_ms: f64,
    overall_max_ms: u64,
    overall_min_ms: u64,
    games_under_200ms: usize,
    games_under_300ms: usize,
    games_under_350ms: usize,
    games_near_limit: usize,  // > 350ms
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
    println!("Timing Analysis: Game Log Latencies");
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

    let mut all_games: Vec<TimingStats> = Vec::new();

    for path in &paths {
        let game_name = path.file_name().unwrap().to_str().unwrap().to_string();

        match analyze_game_timing(&path) {
            Ok(stats) => {
                all_games.push(stats);
            }
            Err(e) => {
                eprintln!("Error analyzing {}: {}", game_name, e);
            }
        }
    }

    // Generate summary
    let summary = generate_timing_summary(&all_games);
    print_timing_report(&summary, &all_games);
}

fn analyze_game_timing(path: &Path) -> Result<TimingStats, String> {
    let file = File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let reader = BufReader::new(file);
    let mut latencies: Vec<u64> = Vec::new();
    let mut turn_count = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;

        if line.trim().is_empty() {
            continue;
        }

        let entry: Value = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        turn_count += 1;

        // Extract latency from snakes array
        if let Some(snakes) = entry["board"]["snakes"].as_array() {
            for snake in snakes {
                if let Some(latency_str) = snake["latency"].as_str() {
                    if let Ok(latency) = latency_str.parse::<u64>() {
                        latencies.push(latency);
                    }
                }
            }
        }
    }

    if latencies.is_empty() {
        return Err("No latency data found".to_string());
    }

    let total: u64 = latencies.iter().sum();
    let avg = total as f64 / latencies.len() as f64;
    let max = *latencies.iter().max().unwrap();
    let min = *latencies.iter().min().unwrap();

    // Calculate median
    let mut sorted = latencies.clone();
    sorted.sort();
    let median = if sorted.len() % 2 == 0 {
        let mid = sorted.len() / 2;
        (sorted[mid - 1] + sorted[mid]) as f64 / 2.0
    } else {
        sorted[sorted.len() / 2] as f64
    };

    Ok(TimingStats {
        game_name: path.file_name().unwrap().to_str().unwrap().to_string(),
        total_turns: turn_count,
        avg_latency_ms: avg,
        max_latency_ms: max,
        min_latency_ms: min,
        median_latency_ms: median,
        samples: latencies,
    })
}

fn generate_timing_summary(games: &[TimingStats]) -> TimingSummary {
    let total_games = games.len();

    let all_latencies: Vec<u64> = games.iter()
        .flat_map(|g| g.samples.iter().copied())
        .collect();

    let overall_avg = if !all_latencies.is_empty() {
        all_latencies.iter().sum::<u64>() as f64 / all_latencies.len() as f64
    } else {
        0.0
    };

    let overall_max = games.iter().map(|g| g.max_latency_ms).max().unwrap_or(0);
    let overall_min = games.iter().map(|g| g.min_latency_ms).min().unwrap_or(0);

    let games_under_200ms = games.iter().filter(|g| g.avg_latency_ms < 200.0).count();
    let games_under_300ms = games.iter().filter(|g| g.avg_latency_ms < 300.0).count();
    let games_under_350ms = games.iter().filter(|g| g.avg_latency_ms < 350.0).count();
    let games_near_limit = games.iter().filter(|g| g.avg_latency_ms >= 350.0).count();

    TimingSummary {
        total_games,
        overall_avg_ms: overall_avg,
        overall_max_ms: overall_max,
        overall_min_ms: overall_min,
        games_under_200ms,
        games_under_300ms,
        games_under_350ms,
        games_near_limit,
    }
}

fn print_timing_report(summary: &TimingSummary, games: &[TimingStats]) {
    println!("============================================================");
    println!("OVERALL SUMMARY");
    println!("============================================================");
    println!("Total games analyzed:        {}", summary.total_games);
    println!("Overall average latency:     {:.1}ms", summary.overall_avg_ms);
    println!("Overall max latency:         {}ms", summary.overall_max_ms);
    println!("Overall min latency:         {}ms", summary.overall_min_ms);
    println!();
    println!("Time Budget Utilization (350ms effective budget):");
    println!("  Avg < 200ms:  {} games ({:.1}%)",
        summary.games_under_200ms,
        100.0 * summary.games_under_200ms as f64 / summary.total_games as f64
    );
    println!("  Avg < 300ms:  {} games ({:.1}%)",
        summary.games_under_300ms,
        100.0 * summary.games_under_300ms as f64 / summary.total_games as f64
    );
    println!("  Avg < 350ms:  {} games ({:.1}%)",
        summary.games_under_350ms,
        100.0 * summary.games_under_350ms as f64 / summary.total_games as f64
    );
    println!("  Avg >= 350ms: {} games ({:.1}%)",
        summary.games_near_limit,
        100.0 * summary.games_near_limit as f64 / summary.total_games as f64
    );
    println!();

    // Sort by average latency
    let mut sorted_games = games.to_vec();
    sorted_games.sort_by(|a, b| a.avg_latency_ms.partial_cmp(&b.avg_latency_ms).unwrap());

    println!("============================================================");
    println!("PER-GAME TIMING STATISTICS");
    println!("============================================================");
    println!("{:<25} {:>8} {:>8} {:>8} {:>8}", "Game", "Avg", "Median", "Max", "Turns");
    println!("{:-<25} {:-<8} {:-<8} {:-<8} {:-<8}", "", "", "", "", "");

    for game in &sorted_games {
        println!("{:<25} {:>7.1}ms {:>7.1}ms {:>7}ms {:>8}",
            game.game_name,
            game.avg_latency_ms,
            game.median_latency_ms,
            game.max_latency_ms,
            game.total_turns
        );
    }
    println!();

    // Find games with significant unused budget
    println!("============================================================");
    println!("OPTIMIZATION OPPORTUNITIES");
    println!("============================================================");

    let unused_budget_games: Vec<_> = sorted_games.iter()
        .filter(|g| g.avg_latency_ms < 250.0)
        .collect();

    if !unused_budget_games.is_empty() {
        println!("Games with >100ms unused time budget (avg < 250ms):");
        println!();
        for game in &unused_budget_games {
            let unused = 350.0 - game.avg_latency_ms;
            println!("  {}: avg {:.1}ms, ~{:.0}ms unused ({:.1}% of budget)",
                game.game_name,
                game.avg_latency_ms,
                unused,
                100.0 * unused / 350.0
            );
        }
        println!();
        println!("These games could potentially search deeper with adjusted");
        println!("time estimation parameters (BRANCHING_FACTOR, BASE_ITERATION_TIME_MS).");
    } else {
        println!("All games are utilizing time budget effectively.");
    }
    println!();

    // Check for timeout risks
    let risky_games: Vec<_> = sorted_games.iter()
        .filter(|g| g.max_latency_ms > 400)
        .collect();

    if !risky_games.is_empty() {
        println!("Games with timeout risk (max > 400ms):");
        println!();
        for game in &risky_games {
            println!("  {}: max {}ms (exceeds 400ms limit!)",
                game.game_name,
                game.max_latency_ms
            );
        }
        println!();
        println!("WARNING: These games had moves that exceeded the time budget.");
        println!("Consider more conservative time estimation or faster pruning.");
    } else {
        println!("No timeout risks detected (all moves < 400ms).");
    }
    println!();

    println!("============================================================");
    println!("RECOMMENDATIONS");
    println!("============================================================");

    if summary.overall_avg_ms < 250.0 {
        println!("✓ Average latency is well below budget ({:.1}ms vs 350ms)", summary.overall_avg_ms);
        println!("  → Consider reducing BRANCHING_FACTOR or BASE_ITERATION_TIME_MS");
        println!("  → This would allow deeper search within time constraints");
    } else if summary.overall_avg_ms < 320.0 {
        println!("✓ Average latency uses budget effectively ({:.1}ms vs 350ms)", summary.overall_avg_ms);
        println!("  → Current time estimation appears well-tuned");
    } else {
        println!("⚠ Average latency is close to budget ({:.1}ms vs 350ms)", summary.overall_avg_ms);
        println!("  → Consider increasing BRANCHING_FACTOR or BASE_ITERATION_TIME_MS");
        println!("  → This provides more safety margin against timeouts");
    }
    println!();

    if summary.games_near_limit > 0 {
        println!("⚠ {} games averaged ≥350ms", summary.games_near_limit);
        println!("  → Review these games for board complexity patterns");
    }
    println!();

    println!("Current parameters (from CLAUDE.md):");
    println!("  EFFECTIVE_BUDGET_MS:      350ms");
    println!("  BASE_ITERATION_TIME_MS:   0.01ms");
    println!("  BRANCHING_FACTOR:         3.5");
    println!("  MIN_TIME_REMAINING_MS:    20ms");
    println!("============================================================");
}
