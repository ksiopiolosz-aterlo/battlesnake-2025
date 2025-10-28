//! Analyzes replay performance to measure optimization improvements
//!
//! This tool replays game logs and analyzes:
//! - Computation time distribution
//! - Search depth achieved
//! - Move decision changes
//! - Timeout rate

use starter_snake_rust::config::Config;
use starter_snake_rust::replay::{LogEntry, ReplayEngine};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <log_file.jsonl>", args[0]);
        eprintln!("       {} <directory>", args[0]);
        std::process::exit(1);
    }

    let path = Path::new(&args[1]);

    if path.is_file() {
        analyze_file(path);
    } else if path.is_dir() {
        analyze_directory(path);
    } else {
        eprintln!("Error: {} is not a file or directory", path.display());
        std::process::exit(1);
    }
}

fn analyze_directory(dir: &Path) {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("           REPLAY PERFORMANCE ANALYSIS");
    println!("           Directory: {}", dir.display());
    println!("═══════════════════════════════════════════════════════════\n");

    // Find all .jsonl files
    let entries = std::fs::read_dir(dir).expect("Failed to read directory");
    let mut game_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|ext| ext == "jsonl").unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();

    game_files.sort();

    if game_files.is_empty() {
        eprintln!("No .jsonl files found in directory");
        return;
    }

    let mut all_stats = Vec::new();

    for file in &game_files {
        println!("\nAnalyzing: {}", file.file_name().unwrap().to_string_lossy());
        println!("─────────────────────────────────────────────────────────");

        if let Some(stats) = analyze_file_internal(file) {
            all_stats.push(stats);
        }
    }

    // Aggregate statistics
    if !all_stats.is_empty() {
        print_aggregate_stats(&all_stats, game_files.len());
    }
}

fn analyze_file(file: &Path) {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("           REPLAY PERFORMANCE ANALYSIS");
    println!("           File: {}", file.display());
    println!("═══════════════════════════════════════════════════════════\n");

    if let Some(stats) = analyze_file_internal(file) {
        print_detailed_stats(&stats);
    }
}

#[derive(Debug, Clone)]
struct PerformanceStats {
    total_turns: usize,
    matches: usize,
    mismatches: usize,
    depths: Vec<u8>,
    times: Vec<u128>,
    timeout_count: usize,
}

fn analyze_file_internal(file: &Path) -> Option<PerformanceStats> {
    let config = Config::load_or_default();
    let engine = ReplayEngine::new(config.clone(), false);

    let entries = match engine.load_log_file(file) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("Error loading log file: {}", err);
            return None;
        }
    };

    if entries.is_empty() {
        eprintln!("No entries found in log file");
        return None;
    }

    let mut depths = Vec::new();
    let mut times = Vec::new();
    let mut matches = 0;
    let mut mismatches = 0;
    let mut timeout_count = 0;

    let timeout_threshold_ms = config.timing.response_time_budget_ms as u128;

    for entry in &entries {
        // Replay this turn
        match engine.replay_entry(entry) {
            Ok(result) => {
                if result.original_move == result.replayed_move {
                    matches += 1;
                } else {
                    mismatches += 1;
                }

                depths.push(result.search_depth);
                times.push(result.computation_time_ms);

                if result.computation_time_ms >= timeout_threshold_ms {
                    timeout_count += 1;
                }
            }
            Err(err) => {
                eprintln!("Error replaying turn {}: {}", entry.turn, err);
            }
        }
    }

    Some(PerformanceStats {
        total_turns: entries.len(),
        matches,
        mismatches,
        depths,
        times,
        timeout_count,
    })
}

fn print_detailed_stats(stats: &PerformanceStats) {
    let avg_depth = if !stats.depths.is_empty() {
        stats.depths.iter().map(|&d| d as f64).sum::<f64>() / stats.depths.len() as f64
    } else {
        0.0
    };

    let avg_time = if !stats.times.is_empty() {
        stats.times.iter().sum::<u128>() / stats.times.len() as u128
    } else {
        0
    };

    let max_depth = stats.depths.iter().max().copied().unwrap_or(0);
    let min_depth = stats.depths.iter().min().copied().unwrap_or(0);
    let max_time = stats.times.iter().max().copied().unwrap_or(0);
    let min_time = stats.times.iter().min().copied().unwrap_or(0);

    let match_rate = if stats.total_turns > 0 {
        100.0 * stats.matches as f64 / stats.total_turns as f64
    } else {
        0.0
    };

    let timeout_rate = if stats.total_turns > 0 {
        100.0 * stats.timeout_count as f64 / stats.total_turns as f64
    } else {
        0.0
    };

    println!("Total Turns:        {}", stats.total_turns);
    println!("Move Matches:       {} ({:.1}%)", stats.matches, match_rate);
    println!("Move Changes:       {} ({:.1}%)", stats.mismatches, 100.0 - match_rate);
    println!();
    println!("Search Depth:");
    println!("  Average:          {:.2}", avg_depth);
    println!("  Range:            {} - {}", min_depth, max_depth);
    println!();
    println!("Computation Time:");
    println!("  Average:          {}ms", avg_time);
    println!("  Range:            {}ms - {}ms", min_time, max_time);
    println!("  Timeouts (>400ms): {} ({:.1}%)", stats.timeout_count, timeout_rate);
    println!();

    // Depth distribution
    let mut depth_counts = std::collections::HashMap::new();
    for &d in &stats.depths {
        *depth_counts.entry(d).or_insert(0) += 1;
    }
    let mut sorted_depths: Vec<_> = depth_counts.iter().collect();
    sorted_depths.sort_by_key(|(k, _)| *k);

    println!("Depth Distribution:");
    for (depth, count) in sorted_depths {
        let pct = 100.0 * *count as f64 / stats.depths.len() as f64;
        println!("  Depth {:2}: {:4} turns ({:5.1}%)", depth, count, pct);
    }
}

fn print_aggregate_stats(all_stats: &[PerformanceStats], num_games: usize) {
    println!("\n═══════════════════════════════════════════════════════════");
    println!("                 AGGREGATE STATISTICS");
    println!("                    {} Games", num_games);
    println!("═══════════════════════════════════════════════════════════\n");

    let total_turns: usize = all_stats.iter().map(|s| s.total_turns).sum();
    let total_matches: usize = all_stats.iter().map(|s| s.matches).sum();
    let total_mismatches: usize = all_stats.iter().map(|s| s.mismatches).sum();
    let total_timeouts: usize = all_stats.iter().map(|s| s.timeout_count).sum();

    let all_depths: Vec<u8> = all_stats.iter().flat_map(|s| s.depths.clone()).collect();
    let all_times: Vec<u128> = all_stats.iter().flat_map(|s| s.times.clone()).collect();

    let avg_depth = if !all_depths.is_empty() {
        all_depths.iter().map(|&d| d as f64).sum::<f64>() / all_depths.len() as f64
    } else {
        0.0
    };

    let avg_time = if !all_times.is_empty() {
        all_times.iter().sum::<u128>() / all_times.len() as u128
    } else {
        0
    };

    let max_depth = all_depths.iter().max().copied().unwrap_or(0);
    let max_time = all_times.iter().max().copied().unwrap_or(0);

    let match_rate = if total_turns > 0 {
        100.0 * total_matches as f64 / total_turns as f64
    } else {
        0.0
    };

    let timeout_rate = if total_turns > 0 {
        100.0 * total_timeouts as f64 / total_turns as f64
    } else {
        0.0
    };

    println!("Total Turns:        {}", total_turns);
    println!("Move Matches:       {} ({:.1}%)", total_matches, match_rate);
    println!("Move Changes:       {} ({:.1}%)", total_mismatches, 100.0 - match_rate);
    println!();
    println!("Search Performance:");
    println!("  Average Depth:    {:.2}", avg_depth);
    println!("  Max Depth:        {}", max_depth);
    println!("  Average Time:     {}ms", avg_time);
    println!("  Max Time:         {}ms", max_time);
    println!("  Timeout Rate:     {:.1}% ({}/{})", timeout_rate, total_timeouts, total_turns);
    println!();

    // Depth distribution
    let mut depth_counts = std::collections::HashMap::new();
    for &d in &all_depths {
        *depth_counts.entry(d).or_insert(0) += 1;
    }
    let mut sorted_depths: Vec<_> = depth_counts.iter().collect();
    sorted_depths.sort_by_key(|(k, _)| *k);

    println!("Depth Distribution:");
    for (depth, count) in sorted_depths {
        let pct = 100.0 * *count as f64 / all_depths.len() as f64;
        println!("  Depth {:2}: {:4} turns ({:5.1}%)", depth, count, pct);
    }
}
