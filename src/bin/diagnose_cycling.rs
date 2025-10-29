use serde::Deserialize;
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
    if args.len() < 3 {
        eprintln!("Usage: diagnose_cycling <game_file.jsonl> <turn_number>");
        std::process::exit(1);
    }

    let target_turn: u32 = args[2].parse().expect("Turn must be a number");

    let file = File::open(&args[1]).expect("Failed to open file");
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        if line.trim().is_empty() {
            continue;
        }
        let entry: LogEntry = serde_json::from_str(&line).expect("Failed to parse JSON");

        if entry.turn == target_turn {
            println!("═══════════════════════════════════════════════════════════");
            println!("                  TURN {} DIAGNOSIS", entry.turn);
            println!("═══════════════════════════════════════════════════════════\n");

            println!("Chosen move: {}\n", entry.chosen_move);

            println!("Snakes:");
            for (i, snake) in entry.board.snakes.iter().enumerate() {
                let head = snake.body[0];
                println!("  [{}] {} ({})", i, snake.name, snake.id);
                println!("      Head: {:?}, Health: {}, Length: {}",
                    head, snake.health, snake.body.len());
            }

            println!("\nFood:");
            for food in &entry.board.food {
                println!("  {:?}", food);
            }

            println!("\nFood distances from our snake:");
            let our_head = entry.board.snakes[0].body[0];
            for food in &entry.board.food {
                let dist = manhattan_distance(our_head, *food);
                println!("  {:?}: distance = {}", food, dist);
            }

            println!("\nOur snake position: {:?}", our_head);
            println!("Our snake health: {}", entry.board.snakes[0].health);

            // Check if there's an opponent (Geriatric Jagwire)
            for snake in &entry.board.snakes {
                if snake.name.contains("Geriatric") || snake.name.contains("Jagwire") {
                    let opp_head = snake.body[0];
                    let dist_to_opp = manhattan_distance(our_head, opp_head);
                    println!("\nOpponent '{}' found:", snake.name);
                    println!("  Head: {:?}, Health: {}, Length: {}",
                        opp_head, snake.health, snake.body.len());
                    println!("  Distance to us: {}", dist_to_opp);

                    // Check distance to nearest food
                    for food in &entry.board.food {
                        let our_dist = manhattan_distance(our_head, *food);
                        let opp_dist = manhattan_distance(opp_head, *food);
                        if our_dist == 1 {
                            println!("  Distance to food {:?}: {} (ours: {})",
                                food, opp_dist, our_dist);
                        }
                    }
                }
            }

            break;
        }
    }
}
