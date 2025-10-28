//! Split multi-game JSONL files into individual game files.
//!
//! This utility takes a JSONL file containing multiple games and splits them into
//! separate files in tests/fixtures/<basename>/.
//!
//! For regular games (1v1 vs different opponent):
//! - Splits on turn 0
//!
//! For self-play games (Rusty vs Rusty):
//! - Groups by unique snake ID pairs
//! - Keeps both players' log entries in the same file
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --bin split_games -- 1v1_loopy_bot.jsonl
//! cargo run --release --bin split_games -- 1v1_self.jsonl
//! ```
//!
//! This will create:
//! - tests/fixtures/1v1_loopy_bot/game_01.jsonl
//! - tests/fixtures/1v1_loopy_bot/game_02.jsonl
//! - ... and so on
//!
//! Each output file contains a complete game from turn 0 to the end.

use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <jsonl_file>", args[0]);
        eprintln!("Example: {} 1v1_loopy_bot.jsonl", args[0]);
        std::process::exit(1);
    }

    let input_file = &args[1];

    match split_games(input_file) {
        Ok(count) => {
            println!("Successfully split {} games from {}", count, input_file);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn split_games(input_file: &str) -> Result<usize, Box<dyn std::error::Error>> {
    // Extract the base name without extension
    let input_path = Path::new(input_file);
    let base_name = input_path
        .file_stem()
        .ok_or("Invalid file name")?
        .to_str()
        .ok_or("Invalid UTF-8 in file name")?;

    // Create output directory
    let output_dir = PathBuf::from("tests/fixtures").join(base_name);
    fs::create_dir_all(&output_dir)?;

    println!("Reading from: {}", input_file);
    println!("Writing to: {}", output_dir.display());

    // Open input file
    let file = File::open(input_file)?;
    let reader = BufReader::new(file);

    // Detect if this is self-play by checking first few entries
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines() {
        lines.push(line?);
    }

    let is_self_play = detect_self_play(&lines)?;

    if is_self_play {
        println!("Detected self-play format (grouping by snake ID pairs)");
        split_self_play_games(&output_dir, &lines)
    } else {
        println!("Detected regular format (splitting on turn 0)");
        split_regular_games(&output_dir, &lines)
    }
}

fn detect_self_play(lines: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    // Check first 10 turn-0 entries to see if we have duplicate turn 0s with same snake IDs
    let mut turn_0_count = 0;
    let mut first_game_ids: Option<(String, String)> = None;

    for line in lines.iter().take(100) {
        let json: serde_json::Value = serde_json::from_str(line)?;
        if json["turn"].as_u64() == Some(0) {
            turn_0_count += 1;

            // Extract snake IDs
            if let Some(snakes) = json["board"]["snakes"].as_array() {
                if snakes.len() >= 2 {
                    let id1 = snakes[0]["id"].as_str().unwrap_or("").to_string();
                    let id2 = snakes[1]["id"].as_str().unwrap_or("").to_string();
                    let mut ids = vec![id1, id2];
                    ids.sort();

                    if let Some(ref first_ids) = first_game_ids {
                        // If we see turn 0 again with same snake IDs, it's self-play
                        if &(ids[0].clone(), ids[1].clone()) == first_ids {
                            return Ok(true);
                        }
                    } else {
                        first_game_ids = Some((ids[0].clone(), ids[1].clone()));
                    }
                }
            }

            if turn_0_count >= 5 {
                break;
            }
        }
    }

    Ok(false)
}

fn split_self_play_games(
    output_dir: &Path,
    lines: &[String],
) -> Result<usize, Box<dyn std::error::Error>> {
    // Group lines by unique snake ID pairs
    let mut games: HashMap<(String, String), Vec<String>> = HashMap::new();

    for line in lines {
        let json: serde_json::Value = serde_json::from_str(line)?;

        // Extract snake IDs
        if let Some(snakes) = json["board"]["snakes"].as_array() {
            if snakes.len() >= 2 {
                let id1 = snakes[0]["id"].as_str().unwrap_or("").to_string();
                let id2 = snakes[1]["id"].as_str().unwrap_or("").to_string();
                let mut ids = vec![id1, id2];
                ids.sort();
                let game_key = (ids[0].clone(), ids[1].clone());

                games.entry(game_key).or_insert_with(Vec::new).push(line.clone());
            }
        }
    }

    // Sort games by their maximum turn number for consistent ordering
    let mut game_vec: Vec<((String, String), Vec<String>)> = games.into_iter().collect();
    game_vec.sort_by_key(|(_, lines)| {
        lines.iter().filter_map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|json| json["turn"].as_u64())
        }).max().unwrap_or(0)
    });

    // Write each game
    for (idx, (game_id, game_lines)) in game_vec.iter().enumerate() {
        println!("Writing game {} ({} entries, snake pair: {} & {})",
                 idx + 1,
                 game_lines.len(),
                 &game_id.0[..12],
                 &game_id.1[..12]);

        write_game(output_dir, idx, game_lines)?;
    }

    Ok(game_vec.len())
}

fn split_regular_games(
    output_dir: &Path,
    lines: &[String],
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut current_game: Vec<String> = Vec::new();
    let mut game_count = 0;

    for line in lines {
        // Parse the line to check the turn number
        let turn = extract_turn(line)?;

        // If turn is 0, we're starting a new game
        if turn == 0 {
            // Write the previous game if it exists
            if !current_game.is_empty() {
                write_game(output_dir, game_count, &current_game)?;
                game_count += 1;
                current_game.clear();
            }
        }

        // Add line to current game
        current_game.push(line.clone());
    }

    // Write the last game
    if !current_game.is_empty() {
        write_game(output_dir, game_count, &current_game)?;
        game_count += 1;
    }

    Ok(game_count)
}

fn extract_turn(line: &str) -> Result<u32, Box<dyn std::error::Error>> {
    // Parse JSON to extract turn number
    let json: serde_json::Value = serde_json::from_str(line)?;
    let turn = json["turn"]
        .as_u64()
        .ok_or("Missing or invalid 'turn' field")?;
    Ok(turn as u32)
}

fn write_game(
    output_dir: &Path,
    game_number: usize,
    lines: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = format!("game_{:02}.jsonl", game_number + 1);
    let output_path = output_dir.join(filename);

    println!("Writing game {} ({} turns)", game_number + 1, lines.len());

    let mut file = File::create(output_path)?;
    for line in lines {
        writeln!(file, "{}", line)?;
    }

    Ok(())
}
