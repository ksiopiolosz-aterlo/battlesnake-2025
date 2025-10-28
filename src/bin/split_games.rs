//! Split multi-game JSONL files into individual game files.
//!
//! This utility takes a JSONL file containing multiple games (where each game starts
//! with turn 0) and splits them into separate files in tests/fixtures/<basename>/.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --bin split_games -- 1v1_loopy_bot.jsonl
//! ```
//!
//! This will create:
//! - tests/fixtures/1v1_loopy_bot/game_01.jsonl
//! - tests/fixtures/1v1_loopy_bot/game_02.jsonl
//! - tests/fixtures/1v1_loopy_bot/game_03.jsonl
//! - ... and so on
//!
//! Each output file contains a complete game from turn 0 to the end.

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

    let mut current_game: Vec<String> = Vec::new();
    let mut game_count = 0;

    for line in reader.lines() {
        let line = line?;

        // Parse the line to check the turn number
        let turn = extract_turn(&line)?;

        // If turn is 0, we're starting a new game
        if turn == 0 {
            // Write the previous game if it exists
            if !current_game.is_empty() {
                write_game(&output_dir, game_count, &current_game)?;
                game_count += 1;
                current_game.clear();
            }
        }

        // Add line to current game
        current_game.push(line);
    }

    // Write the last game
    if !current_game.is_empty() {
        write_game(&output_dir, game_count, &current_game)?;
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
