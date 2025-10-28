//! Validate that all moves in game logs are legal
//!
//! Checks every move in every game file to ensure:
//! 1. Move stays within board bounds
//! 2. Move doesn't go into snake's own neck
//! 3. Reports any illegal moves found

use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Debug)]
struct Coord {
    x: i32,
    y: i32,
}

fn apply_move(coord: &Coord, direction: &str) -> Coord {
    match direction {
        "up" => Coord { x: coord.x, y: coord.y + 1 },
        "down" => Coord { x: coord.x, y: coord.y - 1 },
        "left" => Coord { x: coord.x - 1, y: coord.y },
        "right" => Coord { x: coord.x + 1, y: coord.y },
        _ => coord.clone(),
    }
}

impl Clone for Coord {
    fn clone(&self) -> Self {
        Coord { x: self.x, y: self.y }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <directory>", args[0]);
        eprintln!("Example: {} tests/fixtures/1v1_self/", args[0]);
        std::process::exit(1);
    }

    let dir_path = &args[1];

    // Get all JSON files in directory
    let paths: Vec<_> = fs::read_dir(dir_path)
        .expect("Failed to read directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();

    if paths.is_empty() {
        eprintln!("No .jsonl files found in: {}", dir_path);
        std::process::exit(1);
    }

    println!("Validating {} game files...", paths.len());
    println!("========================================\n");

    let mut total_entries = 0;
    let mut total_illegal = 0;

    for path in paths {
        let file = File::open(&path).expect("Failed to open file");
        let reader = BufReader::new(file);

        let game_name = path.file_name().unwrap().to_str().unwrap();
        let mut game_illegal = 0;

        // Group log entries by turn number
        let mut turns: std::collections::HashMap<u64, Vec<(usize, Value)>> = std::collections::HashMap::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line = line.expect("Failed to read line");
            let json: Value = serde_json::from_str(&line).expect("Failed to parse JSON");

            let turn = json["turn"].as_u64().unwrap_or(0);
            turns.entry(turn).or_insert_with(Vec::new).push((line_num, json));
        }

        // Process turns in order
        let mut sorted_turns: Vec<_> = turns.into_iter().collect();
        sorted_turns.sort_by_key(|(turn, _)| *turn);

        for (_turn_num, entries) in sorted_turns {
            // Process entries in the order they appear in the file
            // Entry i corresponds to snake at index i
            for (snake_idx, (line_num, json)) in entries.iter().enumerate() {
                total_entries += 1;

                let turn = json["turn"].as_u64().unwrap_or(0);
                let chosen_move = json["chosen_move"].as_str().unwrap_or("");
                let width = json["board"]["width"].as_i64().unwrap_or(11) as i32;
                let height = json["board"]["height"].as_i64().unwrap_or(11) as i32;

                // The i-th entry for this turn corresponds to snake at index i
                if let Some(snakes) = json["board"]["snakes"].as_array() {
                    if let Some(snake) = snakes.get(snake_idx) {
                        if let Some(head) = snake["head"].as_object() {
                            let head_x = head["x"].as_i64().unwrap_or(0) as i32;
                            let head_y = head["y"].as_i64().unwrap_or(0) as i32;
                            let head_coord = Coord { x: head_x, y: head_y };

                            // Apply the chosen move
                            let new_coord = apply_move(&head_coord, chosen_move);

                            // Check bounds
                            if new_coord.x < 0 || new_coord.x >= width ||
                               new_coord.y < 0 || new_coord.y >= height {
                                println!("ILLEGAL MOVE: {} line {} turn {} snake {}:",
                                         game_name, line_num + 1, turn, snake_idx);
                                println!("  Position ({},{}) + {} = ({},{}) [bounds: 0-{}, 0-{}]",
                                         head_x, head_y, chosen_move, new_coord.x, new_coord.y,
                                         width - 1, height - 1);
                                game_illegal += 1;
                                total_illegal += 1;
                            }

                            // Check neck collision
                            if let Some(body) = snake["body"].as_array() {
                                if body.len() > 1 {
                                    if let Some(neck) = body[1].as_object() {
                                        let neck_x = neck["x"].as_i64().unwrap_or(0) as i32;
                                        let neck_y = neck["y"].as_i64().unwrap_or(0) as i32;

                                        if new_coord.x == neck_x && new_coord.y == neck_y {
                                            println!("NECK COLLISION: {} line {} turn {} snake {}:",
                                                     game_name, line_num + 1, turn, snake_idx);
                                            println!("  Position ({},{}) + {} = neck at ({},{})",
                                                     head_x, head_y, chosen_move, neck_x, neck_y);
                                            game_illegal += 1;
                                            total_illegal += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if game_illegal > 0 {
            println!("  {} had {} illegal moves\n", game_name, game_illegal);
        }
    }

    println!("\n========================================");
    println!("Validation complete:");
    println!("  Total entries checked: {}", total_entries);
    println!("  Illegal moves found: {}", total_illegal);

    if total_illegal == 0 {
        println!("\n✅ All moves are legal!");
    } else {
        println!("\n❌ Found illegal moves!");
        std::process::exit(1);
    }
}
