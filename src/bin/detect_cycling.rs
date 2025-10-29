use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Debug, Deserialize)]
struct LogEntry {
    turn: u32,
    chosen_move: String,
    board: Board,
}

#[derive(Debug, Deserialize)]
struct Board {
    snakes: Vec<Snake>,
    food: Vec<Coord>,
}

#[derive(Debug, Deserialize)]
struct Snake {
    id: String,
    name: String,
    health: i32,
    body: Vec<Coord>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
struct Coord {
    x: i32,
    y: i32,
}

fn manhattan_distance(a: Coord, b: Coord) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: detect_cycling <game_file.jsonl>");
        std::process::exit(1);
    }

    let file = File::open(&args[1]).expect("Failed to open file");
    let reader = BufReader::new(file);

    let mut entries: Vec<LogEntry> = Vec::new();
    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        if line.trim().is_empty() {
            continue;
        }
        let entry: LogEntry = serde_json::from_str(&line).expect("Failed to parse JSON");
        entries.push(entry);
    }

    println!("Analyzing {} turns for cycling behavior...\n", entries.len());

    // Track move sequences
    let mut move_sequence: Vec<String> = Vec::new();
    let mut cycling_events: Vec<(u32, String, Coord, i32, i32)> = Vec::new();

    for entry in &entries {
        let our_snake = &entry.board.snakes[0]; // Assume we're snake 0
        let head = our_snake.body[0];

        // Find nearest food
        let nearest_food = entry.board.food.iter()
            .min_by_key(|&&food| manhattan_distance(head, food));

        if let Some(&food_pos) = nearest_food {
            let food_dist = manhattan_distance(head, food_pos);

            move_sequence.push(entry.chosen_move.clone());

            // Keep last 4 moves
            if move_sequence.len() > 4 {
                move_sequence.remove(0);
            }

            // Check for cycling: if we have 4 moves that form a loop (up, right, down, left or similar)
            if move_sequence.len() == 4 && food_dist == 1 {
                let pattern = move_sequence.join(",");

                // Common cycling patterns
                let is_cycle =
                    pattern == "up,right,down,left" ||
                    pattern == "right,down,left,up" ||
                    pattern == "down,left,up,right" ||
                    pattern == "left,up,right,down" ||
                    pattern == "up,left,down,right" ||
                    pattern == "left,down,right,up" ||
                    pattern == "down,right,up,left" ||
                    pattern == "right,up,left,down";

                if is_cycle {
                    cycling_events.push((
                        entry.turn,
                        pattern.clone(),
                        food_pos,
                        our_snake.health,
                        food_dist,
                    ));
                }
            }

            // Also detect if we're at distance 1 from food for multiple turns without eating
            if food_dist == 1 && our_snake.health < 100 {
                // Check if we've been at distance 1 for the last 3+ turns
                let mut at_distance_one_count = 0;
                for i in (0..entries.len()).rev() {
                    if i >= entry.turn as usize {
                        break;
                    }
                    let prev_entry = &entries[i];
                    let prev_head = prev_entry.board.snakes[0].body[0];
                    if manhattan_distance(prev_head, food_pos) == 1 {
                        at_distance_one_count += 1;
                    } else {
                        break;
                    }
                }

                if at_distance_one_count >= 3 {
                    println!("⚠️  Turn {}: At distance 1 from food {:?} for {} consecutive turns (health={})",
                        entry.turn, food_pos, at_distance_one_count + 1, our_snake.health);
                    println!("    Recent moves: {:?}", move_sequence);
                }
            }
        }
    }

    if cycling_events.is_empty() {
        println!("✅ No clear cycling patterns detected!");
    } else {
        println!("\n🔄 CYCLING EVENTS DETECTED:\n");
        for (turn, pattern, food_pos, health, dist) in &cycling_events {
            println!("Turn {}: Pattern [{}] - Food at {:?} (dist={}, health={})",
                turn, pattern, food_pos, dist, health);
        }
        println!("\nTotal cycling events: {}", cycling_events.len());
    }
}
