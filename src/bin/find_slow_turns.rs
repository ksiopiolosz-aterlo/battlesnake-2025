//! Identifies turns with slowest computation times during replay
//!
//! This tool replays games and identifies the N slowest turns,
//! exporting their board states for detailed analysis.
//!
//! Usage: find_slow_turns <log_directory> <count> [--export-dir /tmp/slow_turns]

use starter_snake_rust::config::Config;
use starter_snake_rust::replay::ReplayEngine;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
struct SlowTurn {
    file_name: String,
    turn: i32,
    computation_time_ms: u128,
    search_depth: u8,
    num_snakes: usize,
    board_width: i32,
    board_height: u32,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <log_directory> <count> [--export-dir <dir>]", args[0]);
        eprintln!("Example: {} tests/fixtures/battle_royale_florence/ 20 --export-dir /tmp/slow_turns", args[0]);
        std::process::exit(1);
    }

    let log_dir = Path::new(&args[1]);
    let count: usize = args[2].parse().expect("Count must be a number");

    let export_dir = if args.len() >= 5 && args[3] == "--export-dir" {
        Some(Path::new(&args[4]))
    } else {
        None
    };

    if !log_dir.is_dir() {
        eprintln!("Error: {} is not a directory", log_dir.display());
        std::process::exit(1);
    }

    println!("\n═══════════════════════════════════════════════════════════");
    println!("           SLOW TURN IDENTIFICATION TOOL");
    println!("═══════════════════════════════════════════════════════════");
    println!("Log Directory:  {}", log_dir.display());
    println!("Top N slowest:  {}", count);
    if let Some(dir) = export_dir {
        println!("Export Dir:     {}", dir.display());
    }
    println!("═══════════════════════════════════════════════════════════\n");

    // Collect all log files
    let log_files = collect_log_files(log_dir);
    println!("Found {} game log files\n", log_files.len());

    // Analyze all turns and collect slow ones
    let mut all_slow_turns = Vec::new();

    let config = Config::load_or_default();
    let engine = ReplayEngine::new(config.clone(), false);

    for file in &log_files {
        let file_name = file.file_name().unwrap().to_string_lossy().to_string();

        let entries = match engine.load_log_file(file) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("Error loading {}: {}", file_name, err);
                continue;
            }
        };

        for entry in &entries {
            match engine.replay_entry(entry) {
                Ok(result) => {
                    let num_snakes = entry.board.snakes.iter().filter(|s| s.health > 0).count();

                    all_slow_turns.push(SlowTurn {
                        file_name: file_name.clone(),
                        turn: entry.turn,
                        computation_time_ms: result.computation_time_ms,
                        search_depth: result.search_depth,
                        num_snakes,
                        board_width: entry.board.width,
                        board_height: entry.board.height,
                    });
                }
                Err(err) => {
                    eprintln!("Error replaying {}:{}: {}", file_name, entry.turn, err);
                }
            }
        }
    }

    // Sort by computation time (descending)
    all_slow_turns.sort_by(|a, b| b.computation_time_ms.cmp(&a.computation_time_ms));

    // Take top N
    let slow_turns: Vec<_> = all_slow_turns.iter().take(count).collect();

    // Print summary
    println!("═══════════════════════════════════════════════════════════");
    println!("              TOP {} SLOWEST TURNS", count);
    println!("═══════════════════════════════════════════════════════════\n");

    println!("{:<20} {:<8} {:<10} {:<8} {:<10} {:<12}",
        "File", "Turn", "Time (ms)", "Depth", "Snakes", "Board Size");
    println!("{}", "─".repeat(78));

    for slow_turn in &slow_turns {
        println!("{:<20} {:<8} {:<10} {:<8} {:<10} {}x{}",
            slow_turn.file_name,
            slow_turn.turn,
            slow_turn.computation_time_ms,
            slow_turn.search_depth,
            slow_turn.num_snakes,
            slow_turn.board_width,
            slow_turn.board_height);
    }

    // Export board states if requested
    if let Some(export_path) = export_dir {
        println!("\n═══════════════════════════════════════════════════════════");
        println!("              EXPORTING BOARD STATES");
        println!("═══════════════════════════════════════════════════════════\n");

        // Create export directory
        fs::create_dir_all(export_path).expect("Failed to create export directory");

        for (idx, slow_turn) in slow_turns.iter().enumerate() {
            let file_path = log_dir.join(&slow_turn.file_name);
            let entries = engine.load_log_file(&file_path).unwrap();

            if let Some(entry) = entries.iter().find(|e| e.turn == slow_turn.turn) {
                let export_file = export_path.join(format!(
                    "{:02}_{}_{}.json",
                    idx + 1,
                    slow_turn.file_name.replace(".jsonl", ""),
                    slow_turn.turn
                ));

                let json = serde_json::to_string_pretty(&entry).expect("Failed to serialize");
                fs::write(&export_file, json).expect("Failed to write export file");

                println!("Exported: {}", export_file.display());
            }
        }

        println!("\n✓ Exported {} board states", slow_turns.len());
    }

    // Statistics
    println!("\n═══════════════════════════════════════════════════════════");
    println!("                    STATISTICS");
    println!("═══════════════════════════════════════════════════════════\n");

    let avg_time: u128 = slow_turns.iter().map(|t| t.computation_time_ms).sum::<u128>() / slow_turns.len() as u128;
    let avg_depth: f64 = slow_turns.iter().map(|t| t.search_depth as f64).sum::<f64>() / slow_turns.len() as f64;
    let avg_snakes: f64 = slow_turns.iter().map(|t| t.num_snakes as f64).sum::<f64>() / slow_turns.len() as f64;

    let depth_0_count = slow_turns.iter().filter(|t| t.search_depth == 0).count();
    let depth_0_pct = 100.0 * depth_0_count as f64 / slow_turns.len() as f64;

    println!("Average Time:       {}ms", avg_time);
    println!("Average Depth:      {:.2}", avg_depth);
    println!("Average Snakes:     {:.2}", avg_snakes);
    println!("Depth 0 turns:      {} ({:.1}%)", depth_0_count, depth_0_pct);
    println!();

    // Snake count distribution
    let mut snake_counts = std::collections::HashMap::new();
    for turn in &slow_turns {
        *snake_counts.entry(turn.num_snakes).or_insert(0) += 1;
    }
    let mut sorted_counts: Vec<_> = snake_counts.iter().collect();
    sorted_counts.sort_by_key(|(k, _)| *k);

    println!("Snake Count Distribution:");
    for (snakes, count) in sorted_counts {
        let pct = 100.0 * *count as f64 / slow_turns.len() as f64;
        println!("  {} snakes: {} turns ({:.1}%)", snakes, count, pct);
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
