/// Diagnoses illegal move selections by comparing bot's generate_legal_moves
/// with the moves actually recorded in debug logs.

use starter_snake_rust::bot::Bot;
use starter_snake_rust::config::Config;
use starter_snake_rust::types::{Board, Battlesnake, Direction};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <game_file.jsonl>", args[0]);
        std::process::exit(1);
    }

    let game_file = &args[1];
    let config = Config::load_or_default();

    match diagnose_game(game_file, &config) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn diagnose_game(file_path: &str, config: &Config) -> Result<(), String> {
    let file = File::open(file_path).map_err(|e| format!("Failed to open file: {}", e))?;
    let reader = BufReader::new(file);

    let mut illegal_count = 0;
    let mut total_count = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;
        let entry: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let turn = entry["turn"].as_i64().ok_or("Missing turn")? as i32;
        let chosen_move_str = entry["chosen_move"].as_str().ok_or("Missing chosen_move")?;
        let chosen_move = parse_direction(chosen_move_str)?;

        let board: Board = serde_json::from_value(entry["board"].clone())
            .map_err(|e| format!("Failed to parse board: {}", e))?;

        // Find our snake (first alive snake at turn 0, or by ID tracking)
        let our_snake = board.snakes.iter()
            .find(|s| s.health > 0)
            .ok_or("No alive snake found")?;

        // Get legal moves according to Bot::generate_legal_moves
        let legal_moves = Bot::generate_legal_moves(&board, our_snake, config);

        total_count += 1;

        // Check if chosen move is in the legal moves list
        if !legal_moves.contains(&chosen_move) {
            illegal_count += 1;
            println!("❌ Turn {}: Chose {:?} but legal moves were {:?}", turn, chosen_move, legal_moves);

            // Show details
            if !our_snake.body.is_empty() {
                let head = our_snake.body[0];
                println!("   Head: ({}, {})", head.x, head.y);

                if our_snake.body.len() > 1 {
                    let neck = our_snake.body[1];
                    println!("   Neck: ({}, {})", neck.x, neck.y);

                    // Check if chosen move hits the neck
                    let new_head = match chosen_move {
                        Direction::Up => starter_snake_rust::types::Coord { x: head.x, y: head.y + 1 },
                        Direction::Down => starter_snake_rust::types::Coord { x: head.x, y: head.y - 1 },
                        Direction::Left => starter_snake_rust::types::Coord { x: head.x - 1, y: head.y },
                        Direction::Right => starter_snake_rust::types::Coord { x: head.x + 1, y: head.y },
                    };

                    if new_head == neck {
                        println!("   ⚠️  NECK COLLISION: Move would hit neck!");
                    }
                }
            }
            println!();
        }
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("SUMMARY");
    println!("═══════════════════════════════════════════════════════════");
    println!("Total turns: {}", total_count);
    println!("Illegal moves: {} ({:.1}%)", illegal_count, (illegal_count as f64 / total_count as f64) * 100.0);

    if illegal_count == 0 {
        println!("\n✅ All moves were legal according to Bot::generate_legal_moves!");
        println!("This means the issue is NOT in the bot's move generation logic.");
        println!("The 'illegal moves' detected by validate_moves must be using different rules.");
    } else {
        println!("\n🚨 CRITICAL BUG: Bot returned moves that generate_legal_moves says are illegal!");
        println!("This indicates a race condition or bug in the search/shared state logic.");
    }

    Ok(())
}

fn parse_direction(s: &str) -> Result<Direction, String> {
    match s.to_lowercase().as_str() {
        "up" => Ok(Direction::Up),
        "down" => Ok(Direction::Down),
        "left" => Ok(Direction::Left),
        "right" => Ok(Direction::Right),
        _ => Err(format!("Invalid direction: {}", s)),
    }
}
