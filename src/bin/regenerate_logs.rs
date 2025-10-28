//! Regenerate game logs with fixed code
//!
//! This tool:
//! 1. Reads original game logs
//! 2. Replays each turn with the fixed algorithm
//! 3. Generates NEW JSONL files with corrected moves
//!
//! Usage:
//!   cargo run --release --bin regenerate_logs -- <input_dir> <output_dir>

use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write as IoWrite};
use std::path::Path;

use starter_snake_rust::config::Config;
use starter_snake_rust::replay::ReplayEngine;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <input_directory> <output_directory>", args[0]);
        eprintln!("Example: {} tests/fixtures/1v1_self/ tests/fixtures/1v1_self_fixed/", args[0]);
        std::process::exit(1);
    }

    let input_dir = &args[1];
    let output_dir = &args[2];

    // Create output directory
    fs::create_dir_all(output_dir).expect("Failed to create output directory");

    println!("============================================================");
    println!("Regenerating Game Logs with Fixed Code");
    println!("============================================================");
    println!();
    println!("Input:  {}", input_dir);
    println!("Output: {}", output_dir);
    println!();

    // Get all JSONL files
    let paths: Vec<_> = fs::read_dir(input_dir)
        .expect("Failed to read input directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();

    if paths.is_empty() {
        eprintln!("No .jsonl files found in: {}", input_dir);
        std::process::exit(1);
    }

    println!("Processing {} game files...", paths.len());
    println!();

    let config = Config::load_or_default();
    let replay_engine = ReplayEngine::new(config, false);

    for input_path in paths {
        let game_name = input_path.file_name().unwrap().to_str().unwrap();
        let output_path = Path::new(output_dir).join(game_name);

        print!("Processing {}... ", game_name);

        match regenerate_game_log(&replay_engine, &input_path, &output_path) {
            Ok(stats) => {
                println!(
                    "✓ {} turns, {} moves corrected",
                    stats.total_turns, stats.moves_corrected
                );
            }
            Err(e) => {
                println!("✗ Error: {}", e);
            }
        }
    }

    println!();
    println!("============================================================");
    println!("Regeneration complete!");
    println!("New logs saved to: {}", output_dir);
    println!("============================================================");
    println!();
}

struct RegenerationStats {
    total_turns: usize,
    moves_corrected: usize,
}

fn regenerate_game_log(
    replay_engine: &ReplayEngine,
    input_path: &Path,
    output_path: &Path,
) -> Result<RegenerationStats, String> {
    use std::collections::HashMap;

    // Read input file and group by turn
    let file = File::open(input_path)
        .map_err(|e| format!("Failed to open input file: {}", e))?;

    let reader = BufReader::new(file);
    let mut turns: HashMap<u64, Vec<(usize, Value)>> = HashMap::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("Failed to read line {}: {}", line_num + 1, e))?;

        if line.trim().is_empty() {
            continue;
        }

        let entry: Value = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse JSON on line {}: {}", line_num + 1, e))?;

        let turn = entry["turn"].as_u64().unwrap_or(0);
        turns.entry(turn).or_insert_with(Vec::new).push((line_num, entry));
    }

    // Process turns in order
    let mut sorted_turns: Vec<_> = turns.into_iter().collect();
    sorted_turns.sort_by_key(|(turn, _)| *turn);

    let mut output_file = File::create(output_path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;

    let mut total_turns = 0;
    let mut moves_corrected = 0;

    for (_turn_num, entries) in sorted_turns {
        // Process entries in file order - entry i corresponds to snake i
        for (snake_idx, (_line_num, mut entry)) in entries.into_iter().enumerate() {
            total_turns += 1;

            // Extract board state
            let board: starter_snake_rust::types::Board =
                serde_json::from_value(entry["board"].clone())
                    .map_err(|e| format!("Failed to parse board: {}", e))?;

            // Get the snake for this entry (i-th entry = i-th snake)
            let our_snake = board
                .snakes
                .get(snake_idx)
                .ok_or_else(|| format!("Snake {} not found (only {} snakes)", snake_idx, board.snakes.len()))?;

            // Replay this turn with fixed code
            match replay_engine.replay_turn(&board, &our_snake.id) {
                Ok((replayed_direction, _score, _depth, _time)) => {
                    let original_move = entry["chosen_move"].as_str().unwrap_or("");
                    let replayed_move = replayed_direction.as_str();

                    // Update the move if different
                    if original_move != replayed_move {
                        entry["chosen_move"] = Value::String(replayed_move.to_string());
                        moves_corrected += 1;
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to replay snake {}: {}", snake_idx, e);
                }
            }

            // Write corrected entry to output
            writeln!(output_file, "{}", serde_json::to_string(&entry).unwrap())
                .map_err(|e| format!("Failed to write entry: {}", e))?;
        }
    }

    Ok(RegenerationStats {
        total_turns,
        moves_corrected,
    })
}
