//! Analyze battle royale game logs to diagnose IDAPOS and search depth issues
//!
//! This tool examines multi-snake games to understand:
//! - Which snakes are being considered "active" by IDAPOS
//! - Why search depth is low
//! - What moves are available vs what moves are being chosen
//!
//! Usage:
//!   cargo run --release --bin analyze_battle_royale -- <game_file>

use starter_snake_rust::config::Config;
use starter_snake_rust::types::{Battlesnake, Coord, Direction};
use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <game_file.jsonl>", args[0]);
        std::process::exit(1);
    }

    let config = Config::load_or_default();
    let game_file = &args[1];

    println!("═══════════════════════════════════════════════════════════");
    println!("       BATTLE ROYALE ANALYSIS");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("Game file: {}", game_file);
    println!("IDAPOS head distance multiplier: {}", config.idapos.head_distance_multiplier);
    println!();

    // Load and analyze each turn
    let file = File::open(game_file).expect("Failed to open file");
    let reader = BufReader::new(file);

    for (idx, line) in reader.lines().enumerate() {
        let line = line.expect("Failed to read line");
        let entry: serde_json::Value = serde_json::from_str(&line)
            .expect("Failed to parse JSON");

        let turn = entry["turn"].as_i64().unwrap_or(-1);
        let chosen_move = entry["chosen_move"].as_str().unwrap_or("unknown");

        // Parse board state
        let board_json = &entry["board"];
        let snakes: Vec<Battlesnake> = serde_json::from_value(board_json["snakes"].clone())
            .expect("Failed to parse snakes");

        let width = board_json["width"].as_u64().unwrap_or(11) as i32;
        let height = board_json["height"].as_u64().unwrap_or(11) as i32;

        println!("─────────────────────────────────────────────────────────");
        println!("Turn {}: Chosen move = {}", turn, chosen_move);
        println!("─────────────────────────────────────────────────────────");

        // Find our snake (Rusty)
        let our_snake_idx = snakes.iter().position(|s| s.name == "Rusty");
        if our_snake_idx.is_none() {
            println!("  ⚠️  Our snake (Rusty) not found - game over");
            println!();
            continue;
        }
        let our_snake_idx = our_snake_idx.unwrap();
        let our_snake = &snakes[our_snake_idx];

        println!("  Our snake: {} at {:?}, health={}, length={}",
                 our_snake.name, our_snake.head, our_snake.health, our_snake.length);
        println!();

        // Show all snakes
        println!("  All snakes:");
        for (idx, snake) in snakes.iter().enumerate() {
            let marker = if idx == our_snake_idx { "→" } else { " " };
            let dist = manhattan_distance(our_snake.head, snake.head);
            println!("    {} [{:1}] {} at {:?}, health={}, length={}, dist={}",
                     marker, idx, snake.name, snake.head, snake.health, snake.length, dist);
        }
        println!();

        // Analyze IDAPOS locality masking
        // Simulate different search depths to see which snakes would be active
        for depth in [1, 2, 3, 4, 5, 6] {
            let active_snakes = determine_active_snakes(&snakes, our_snake_idx, depth, &config);
            let active_names: Vec<String> = active_snakes.iter()
                .map(|&i| format!("{}[{}]", snakes[i].name, i))
                .collect();
            println!("  Depth {}: {} active snakes: {}",
                     depth, active_snakes.len(), active_names.join(", "));
        }
        println!();

        // Analyze available moves
        let legal_moves = generate_legal_moves(&snakes, our_snake_idx, width, height);
        println!("  Legal moves: {:?} (count: {})", legal_moves, legal_moves.len());

        // Check if we're about to hit a wall
        let head = our_snake.head;
        if chosen_move == "up" && head.y >= height - 1 {
            println!("  ⚠️  CRITICAL: Moving UP will hit the top wall!");
        } else if chosen_move == "down" && head.y <= 0 {
            println!("  ⚠️  CRITICAL: Moving DOWN will hit the bottom wall!");
        } else if chosen_move == "left" && head.x <= 0 {
            println!("  ⚠️  CRITICAL: Moving LEFT will hit the left wall!");
        } else if chosen_move == "right" && head.x >= width - 1 {
            println!("  ⚠️  CRITICAL: Moving RIGHT will hit the right wall!");
        }

        // Analyze space control for each move
        println!();
        println!("  Space control analysis:");
        for dir in &legal_moves {
            let next_pos = apply_direction(head, *dir);
            let reachable = flood_fill(&snakes, our_snake_idx, next_pos, width, height);
            println!("    {:?}: {} reachable cells from {:?}",
                     dir, reachable, next_pos);
        }

        println!();
    }
}

fn manhattan_distance(a: Coord, b: Coord) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn determine_active_snakes(
    snakes: &[Battlesnake],
    our_idx: usize,
    remaining_depth: i32,
    config: &Config,
) -> Vec<usize> {
    let mut active = vec![our_idx];
    let our_head = snakes[our_idx].head;

    // Calculate locality threshold with maximum cap
    let base_threshold = config.idapos.head_distance_multiplier * remaining_depth;
    let locality_threshold = std::cmp::min(base_threshold, config.idapos.max_locality_distance);

    for (idx, snake) in snakes.iter().enumerate() {
        if idx == our_idx || snake.body.is_empty() {
            continue;
        }

        let head_dist = manhattan_distance(our_head, snake.head);
        if head_dist <= locality_threshold {
            active.push(idx);
            continue;
        }

        for &segment in &snake.body {
            if manhattan_distance(our_head, segment) <= locality_threshold {
                active.push(idx);
                break;
            }
        }
    }

    active
}

fn generate_legal_moves(
    snakes: &[Battlesnake],
    snake_idx: usize,
    width: i32,
    height: i32,
) -> Vec<Direction> {
    let snake = &snakes[snake_idx];
    if snake.body.is_empty() {
        return vec![];
    }

    let head = snake.body[0];
    let neck = if snake.body.len() > 1 {
        Some(snake.body[1])
    } else {
        None
    };

    let all_dirs = vec![Direction::Up, Direction::Down, Direction::Left, Direction::Right];

    all_dirs.into_iter().filter(|&dir| {
        let next = apply_direction(head, dir);

        // Can't reverse onto neck
        if let Some(n) = neck {
            if next == n {
                return false;
            }
        }

        // Must stay in bounds
        if next.x < 0 || next.x >= width || next.y < 0 || next.y >= height {
            return false;
        }

        // Can't collide with bodies (except tails which will move)
        for other in snakes {
            if other.body.is_empty() {
                continue;
            }
            let body_check = if other.body.len() > 1 {
                &other.body[..other.body.len() - 1]
            } else {
                &other.body[..]
            };
            if body_check.contains(&next) {
                return false;
            }
        }

        true
    }).collect()
}

fn apply_direction(coord: Coord, dir: Direction) -> Coord {
    match dir {
        Direction::Up => Coord { x: coord.x, y: coord.y + 1 },
        Direction::Down => Coord { x: coord.x, y: coord.y - 1 },
        Direction::Left => Coord { x: coord.x - 1, y: coord.y },
        Direction::Right => Coord { x: coord.x + 1, y: coord.y },
    }
}

fn flood_fill(
    snakes: &[Battlesnake],
    snake_idx: usize,
    start: Coord,
    width: i32,
    height: i32,
) -> usize {
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    visited.insert(start);

    while let Some(pos) = stack.pop() {
        for dir in &[Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
            let next = apply_direction(pos, *dir);

            // Check bounds
            if next.x < 0 || next.x >= width || next.y < 0 || next.y >= height {
                continue;
            }

            if visited.contains(&next) {
                continue;
            }

            // Check if blocked by snake bodies
            let mut blocked = false;
            for other in snakes {
                if other.body.is_empty() {
                    continue;
                }
                // Don't count the tail as blocking (it will move)
                let body_check = if other.body.len() > 1 {
                    &other.body[..other.body.len() - 1]
                } else {
                    &[]
                };
                if body_check.contains(&next) {
                    blocked = true;
                    break;
                }
            }

            if !blocked {
                visited.insert(next);
                stack.push(next);
            }
        }
    }

    visited.len()
}
