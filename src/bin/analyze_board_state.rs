use serde::Deserialize;
use std::collections::HashSet;
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
        eprintln!("Usage: analyze_board_state <game_file.jsonl> <turn_number>");
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
            println!("          TURN {} BOARD STATE ANALYSIS", entry.turn);
            println!("═══════════════════════════════════════════════════════════\n");

            let our_snake = &entry.board.snakes[0];
            let our_head = our_snake.body[0];

            println!("Our Snake: {} (health={})", our_snake.name, our_snake.health);
            println!("Position: {:?}", our_head);
            println!("Body: {:?}", our_snake.body);
            println!();

            // Find nearest food
            let nearest_food = entry.board.food.iter()
                .min_by_key(|&&f| manhattan_distance(our_head, f));

            if let Some(&food_pos) = nearest_food {
                println!("Nearest food: {:?} (distance={})", food_pos, manhattan_distance(our_head, food_pos));
                println!();
            }

            // Collect all body positions
            let mut occupied: HashSet<Coord> = HashSet::new();
            for snake in &entry.board.snakes {
                for &segment in &snake.body {
                    occupied.insert(segment);
                }
            }

            // Analyze legal moves
            println!("═══════════════════════════════════════════════════════════");
            println!("                  LEGAL MOVES ANALYSIS");
            println!("═══════════════════════════════════════════════════════════\n");

            let directions = [
                ("up", Coord { x: our_head.x, y: our_head.y + 1 }),
                ("down", Coord { x: our_head.x, y: our_head.y - 1 }),
                ("left", Coord { x: our_head.x - 1, y: our_head.y }),
                ("right", Coord { x: our_head.x + 1, y: our_head.y }),
            ];

            for (dir_name, new_pos) in &directions {
                let marker = if *dir_name == entry.chosen_move { " ← CHOSEN" } else { "" };

                // Check bounds
                if new_pos.x < 0 || new_pos.x > 10 || new_pos.y < 0 || new_pos.y > 10 {
                    println!("{:>6}: ILLEGAL (out of bounds){}", dir_name, marker);
                    continue;
                }

                // Check neck
                if our_snake.body.len() > 1 && *new_pos == our_snake.body[1] {
                    println!("{:>6}: ILLEGAL (neck collision){}", dir_name, marker);
                    continue;
                }

                // Check if occupied (excluding tails which will move)
                let mut blocked_by = None;
                for (idx, snake) in entry.board.snakes.iter().enumerate() {
                    let body_check = if snake.body.len() > 1 {
                        &snake.body[..snake.body.len() - 1]
                    } else {
                        &snake.body[..]
                    };

                    if body_check.contains(new_pos) {
                        blocked_by = Some((idx, snake.name.clone()));
                        break;
                    }
                }

                if let Some((idx, name)) = blocked_by {
                    println!("{:>6}: ILLEGAL (body collision with {} [{}]){}",
                        dir_name, name, idx, marker);
                    continue;
                }

                // Legal move - calculate food distance
                let food_dist = if let Some(&food_pos) = nearest_food {
                    manhattan_distance(*new_pos, food_pos)
                } else {
                    999
                };

                println!("{:>6}: LEGAL (food dist={}){}",
                    dir_name, food_dist, marker);
            }

            // Check cells around food
            if let Some(&food_pos) = nearest_food {
                println!("\n═══════════════════════════════════════════════════════════");
                println!("         CELLS BETWEEN US AND FOOD");
                println!("═══════════════════════════════════════════════════════════\n");

                // Check path to food
                let path_coords = vec![
                    Coord { x: 1, y: 0 }, // One step right
                    food_pos, // The food itself
                ];

                for coord in &path_coords {
                    let status = if occupied.contains(coord) {
                        // Find which snake
                        let mut owner = "Unknown".to_string();
                        for (idx, snake) in entry.board.snakes.iter().enumerate() {
                            if snake.body.contains(coord) {
                                owner = format!("{} [{}]", snake.name, idx);
                                // Find position in body
                                if let Some(pos) = snake.body.iter().position(|&s| s == *coord) {
                                    if pos == 0 {
                                        owner.push_str(" HEAD");
                                    } else if pos == snake.body.len() - 1 {
                                        owner.push_str(" TAIL");
                                    } else {
                                        owner.push_str(&format!(" BODY[{}]", pos));
                                    }
                                }
                                break;
                            }
                        }
                        format!("OCCUPIED by {}", owner)
                    } else {
                        "EMPTY".to_string()
                    };

                    println!("  {:?}: {}", coord, status);
                }

                // Check cells adjacent to food
                println!("\nCells adjacent to food {:?}:", food_pos);
                let adjacent = vec![
                    Coord { x: food_pos.x, y: food_pos.y + 1 },
                    Coord { x: food_pos.x, y: food_pos.y - 1 },
                    Coord { x: food_pos.x - 1, y: food_pos.y },
                    Coord { x: food_pos.x + 1, y: food_pos.y },
                ];

                for coord in &adjacent {
                    if coord.x < 0 || coord.x > 10 || coord.y < 0 || coord.y > 10 {
                        println!("  {:?}: OUT OF BOUNDS", coord);
                        continue;
                    }

                    let status = if occupied.contains(coord) {
                        let mut owner = "Unknown".to_string();
                        for (idx, snake) in entry.board.snakes.iter().enumerate() {
                            if snake.body.contains(coord) {
                                owner = format!("{} [{}]", snake.name, idx);
                                break;
                            }
                        }
                        format!("OCCUPIED by {}", owner)
                    } else {
                        "EMPTY".to_string()
                    };

                    println!("  {:?}: {}", coord, status);
                }
            }

            println!("\n═══════════════════════════════════════════════════════════");
            println!("CONCLUSION: This shows which moves are legal and what");
            println!("occupies cells near the food. If 'right' is legal but wasn't");
            println!("chosen, the search tree predicted death/trap after that move.");
            println!("═══════════════════════════════════════════════════════════\n");

            break;
        }
    }
}
