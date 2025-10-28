//! Profiles slow turns identified by find_slow_turns tool
//!
//! This tool replays specific slow turns with profiling enabled to understand
//! where computation time is spent.
//!
//! Usage: profile_slow_turns <slow_turn_export_dir>

use starter_snake_rust::config::Config;
use starter_snake_rust::replay::ReplayEngine;
use starter_snake_rust::simple_profiler;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <slow_turn_export_dir>", args[0]);
        eprintln!("Example: {} /tmp/slow_turns_florence", args[0]);
        std::process::exit(1);
    }

    let export_dir = Path::new(&args[1]);

    if !export_dir.is_dir() {
        eprintln!("Error: {} is not a directory", export_dir.display());
        std::process::exit(1);
    }

    // Enable profiling via environment variable
    env::set_var("BATTLESNAKE_PROFILE", "1");

    println!("\n═══════════════════════════════════════════════════════════");
    println!("           SLOW TURN PROFILING TOOL");
    println!("═══════════════════════════════════════════════════════════");
    println!("Export Dir:  {}", export_dir.display());
    println!("═══════════════════════════════════════════════════════════\n");

    // Collect all exported JSON files
    let entries = fs::read_dir(export_dir).expect("Failed to read directory");
    let mut json_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|ext| ext == "json").unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();

    json_files.sort();

    if json_files.is_empty() {
        eprintln!("No JSON files found in directory");
        return;
    }

    println!("Found {} exported turns\n", json_files.len());

    let config = Config::load_or_default();
    let engine = ReplayEngine::new(config.clone(), false);

    for (idx, file) in json_files.iter().enumerate() {
        let file_name = file.file_name().unwrap().to_string_lossy();

        println!("═══════════════════════════════════════════════════════════");
        println!("  Turn {} of {}: {}", idx + 1, json_files.len(), file_name);
        println!("═══════════════════════════════════════════════════════════");

        // Read the log entry
        let json_str = fs::read_to_string(file).expect("Failed to read file");
        let entry: starter_snake_rust::replay::LogEntry =
            serde_json::from_str(&json_str).expect("Failed to parse JSON");

        // Reset profiler
        simple_profiler::reset();

        // Replay the turn
        match engine.replay_entry(&entry) {
            Ok(result) => {
                println!("Original Move:   {:?}", result.original_move);
                println!("Replayed Move:   {:?}", result.replayed_move);
                println!("Match:           {}", if result.original_move == result.replayed_move { "✓" } else { "✗" });
                println!("Search Depth:    {}", result.search_depth);
                println!("Computation:     {}ms", result.computation_time_ms);
                println!();

                // Merge thread-local data and print profile report
                simple_profiler::merge_thread_local();
                simple_profiler::print_report(result.computation_time_ms as u64);
            }
            Err(err) => {
                eprintln!("Error replaying turn: {}", err);
            }
        }

        println!("\n");
    }
}
