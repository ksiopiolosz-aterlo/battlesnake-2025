//! Binary search tool to find optimal branching factor
//!
//! This tool tests different branching factor values to maximize search depth
//! while keeping timeout rate acceptable.
//!
//! Usage: tune_branching_factor <log_directory> <target_timeout_rate>

use starter_snake_rust::config::Config;
use starter_snake_rust::replay::{LogEntry, ReplayEngine};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 4 {
        eprintln!("Usage: {} <log_directory> <target_timeout_rate_%> <mode>", args[0]);
        eprintln!("  mode: '1v1' or 'multiplayer'");
        eprintln!("Example: {} tests/fixtures/1v1_self/ 2.0 1v1", args[0]);
        eprintln!("Example: {} tests/fixtures/battle_royale_florence/ 10.0 multiplayer", args[0]);
        std::process::exit(1);
    }

    let log_dir = Path::new(&args[1]);
    let target_timeout_rate: f64 = args[2].parse()
        .expect("Target timeout rate must be a number");
    let mode = args[3].as_str();

    if !log_dir.is_dir() {
        eprintln!("Error: {} is not a directory", log_dir.display());
        std::process::exit(1);
    }

    if mode != "1v1" && mode != "multiplayer" {
        eprintln!("Error: mode must be '1v1' or 'multiplayer', got: {}", mode);
        std::process::exit(1);
    }

    println!("\n═══════════════════════════════════════════════════════════");
    println!("        BRANCHING FACTOR OPTIMIZATION TOOL");
    println!("═══════════════════════════════════════════════════════════");
    println!("Mode:                {}", mode);
    println!("Log Directory:       {}", log_dir.display());
    println!("Target Timeout Rate: {:.1}%", target_timeout_rate);
    println!("═══════════════════════════════════════════════════════════\n");

    // Collect all log files
    let log_files = collect_log_files(log_dir);
    println!("Found {} game log files\n", log_files.len());

    // Binary search for optimal branching factor
    let mut low = 1.5;
    let mut high = 4.0;
    let epsilon = 0.05; // Stop when range is smaller than this

    let mut best_factor = 2.25;
    let mut best_depth = 0.0;
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 10;

    println!("Starting binary search (range: {:.2} - {:.2})\n", low, high);

    while (high - low) > epsilon && iterations < MAX_ITERATIONS {
        iterations += 1;

        // Test midpoint
        let mid = (low + high) / 2.0;
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Iteration {}: Testing branching_factor = {:.2}", iterations, mid);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let result = test_branching_factor(&log_files, mid, mode);

        println!("  Average Depth:    {:.2}", result.avg_depth);
        println!("  Timeout Rate:     {:.1}% ({}/{})", result.timeout_rate, result.timeouts, result.total_turns);
        println!("  Average Time:     {}ms", result.avg_time);

        // Determine if this timeout rate is acceptable
        if result.timeout_rate <= target_timeout_rate {
            // Timeout rate acceptable, try higher branching factor for more depth
            println!("  ✓ Timeout rate acceptable, trying higher factor");
            if result.avg_depth > best_depth {
                best_depth = result.avg_depth;
                best_factor = mid;
                println!("  ★ New best: depth {:.2}", best_depth);
            }
            low = mid;
        } else {
            // Too many timeouts, need lower branching factor
            println!("  ✗ Timeout rate too high, trying lower factor");
            high = mid;
        }

        println!();
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("                   OPTIMIZATION RESULTS");
    println!("═══════════════════════════════════════════════════════════");
    println!("Optimal Branching Factor:  {:.2}", best_factor);
    println!("Expected Average Depth:    {:.2}", best_depth);
    println!("Target Timeout Rate:       ≤ {:.1}%", target_timeout_rate);
    println!("Iterations:                {}", iterations);
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Recommended Snake.toml update:");
    println!("```toml");
    if mode == "1v1" {
        println!("[time_estimation.one_vs_one]");
    } else {
        println!("[time_estimation.multiplayer]");
    }
    println!("branching_factor = {:.2}", best_factor);
    println!("```\n");
}

#[derive(Debug)]
struct TestResult {
    total_turns: usize,
    timeouts: usize,
    timeout_rate: f64,
    avg_depth: f64,
    avg_time: u128,
}

fn test_branching_factor(log_files: &[std::path::PathBuf], branching_factor: f64, mode: &str) -> TestResult {
    let mut config = Config::load_or_default();

    // Override branching factor based on mode
    if mode == "1v1" {
        config.time_estimation.one_vs_one.branching_factor = branching_factor;
    } else {
        config.time_estimation.multiplayer.branching_factor = branching_factor;
    }

    let engine = ReplayEngine::new(config.clone(), false);

    let mut total_turns = 0;
    let mut total_timeouts = 0;
    let mut all_depths = Vec::new();
    let mut all_times = Vec::new();

    let timeout_threshold = config.timing.response_time_budget_ms as u128;

    for file in log_files {
        let entries = match engine.load_log_file(file) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in &entries {
            if let Ok(result) = engine.replay_entry(entry) {
                total_turns += 1;
                all_depths.push(result.search_depth);
                all_times.push(result.computation_time_ms);

                if result.computation_time_ms >= timeout_threshold {
                    total_timeouts += 1;
                }
            }
        }
    }

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

    let timeout_rate = if total_turns > 0 {
        100.0 * total_timeouts as f64 / total_turns as f64
    } else {
        0.0
    };

    TestResult {
        total_turns,
        timeouts: total_timeouts,
        timeout_rate,
        avg_depth,
        avg_time,
    }
}

fn collect_log_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let entries = fs::read_dir(dir).expect("Failed to read directory");
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|ext| ext == "jsonl").unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}
