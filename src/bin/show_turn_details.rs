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
        eprintln!("Usage: show_turn_details <game_file.jsonl> <turn_number>");
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
            println!("              TURN {} FULL DETAILS", entry.turn);
            println!("═══════════════════════════════════════════════════════════\n");

            let our_snake = &entry.board.snakes[0];
            let our_head = our_snake.body[0];

            println!("Our Snake: {} (health={})", our_snake.name, our_snake.health);
            println!("Position: {:?}", our_head);
            println!("Body: {:?}", our_snake.body);
            println!("Chosen move: {}", entry.chosen_move);
            println!();

            // Show food positions
            println!("Food positions:");
            for food in &entry.board.food {
                let dist = manhattan_distance(our_head, *food);
                println!("  {:?} - distance {}", food, dist);
            }
            println!();

            // Show opponent details
            println!("═══════════════════════════════════════════════════════════");
            println!("                    OPPONENTS");
            println!("═══════════════════════════════════════════════════════════\n");

            for (idx, snake) in entry.board.snakes.iter().enumerate().skip(1) {
                let head_dist = manhattan_distance(our_head, snake.body[0]);
                println!("Snake {}: {} (health={})", idx, snake.name, snake.health);
                println!("  Head: {:?} (distance from us: {})", snake.body[0], head_dist);
                println!("  Body segments:");
                for (seg_idx, &segment) in snake.body.iter().enumerate() {
                    let seg_dist = manhattan_distance(our_head, segment);
                    let seg_type = if seg_idx == 0 {
                        "HEAD"
                    } else if seg_idx == snake.body.len() - 1 {
                        "TAIL"
                    } else {
                        "BODY"
                    };
                    println!("    [{}] {:?} {} - distance {}", seg_idx, segment, seg_type, seg_dist);
                }
                println!();
            }

            // Check if any opponent body segments are near the food
            println!("═══════════════════════════════════════════════════════════");
            println!("            NEAR FOOD ANALYSIS");
            println!("═══════════════════════════════════════════════════════════\n");

            for food in &entry.board.food {
                println!("Food at {:?}:", food);

                // Check cells around food (including the position RIGHT would move to)
                let right_pos = Coord { x: our_head.x + 1, y: our_head.y };
                println!("  RIGHT move would put us at {:?} (distance to food: {})",
                    right_pos, manhattan_distance(right_pos, *food));

                // Check if any opponent body segments are near food or our path to food
                for (idx, snake) in entry.board.snakes.iter().enumerate().skip(1) {
                    for (seg_idx, &segment) in snake.body.iter().enumerate() {
                        let dist_to_food = manhattan_distance(segment, *food);
                        let dist_to_right = manhattan_distance(segment, right_pos);

                        if dist_to_food <= 2 || dist_to_right <= 2 {
                            let seg_type = if seg_idx == 0 {
                                "HEAD"
                            } else if seg_idx == snake.body.len() - 1 {
                                "TAIL"
                            } else {
                                "BODY"
                            };
                            println!("    {} {} [{}]: {:?} - dist to food: {}, dist to our RIGHT: {}",
                                snake.name, seg_type, seg_idx, segment, dist_to_food, dist_to_right);
                        }
                    }
                }
                println!();
            }

            break;
        }
    }
}
