use starter_snake_rust::config::Config;
use starter_snake_rust::types::{Board, Coord, Direction};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <game.jsonl> <turn_number>", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let target_turn: u32 = args[2].parse().expect("Turn number must be a valid integer");

    let config = Config::load_or_default();
    let file = File::open(file_path).expect("Failed to open file");
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let entry: serde_json::Value = serde_json::from_str(&line).expect("Failed to parse JSON");

        let turn = entry["turn"].as_u64().unwrap() as u32;
        if turn != target_turn {
            continue;
        }

        let chosen_move_str = entry["chosen_move"].as_str().unwrap();
        let board: Board = serde_json::from_value(entry["board"].clone()).expect("Failed to parse board");

        if board.snakes.is_empty() {
            eprintln!("No snakes found in board state");
            std::process::exit(1);
        }

        let snake = &board.snakes[0];
        if snake.body.is_empty() {
            eprintln!("Snake has no body segments");
            std::process::exit(1);
        }

        let head = snake.body[0];
        let health = snake.health;

        println!("═══════════════════════════════════════════════════════════");
        println!("Decision Diagnosis for Turn {}", turn);
        println!("═══════════════════════════════════════════════════════════");
        println!();
        println!("Snake State:");
        println!("  Head: ({}, {})", head.x, head.y);
        println!("  Health: {}", health);
        println!("  Length: {}", snake.length);
        println!();

        // Show opponent information
        println!("Opponents:");
        for (idx, opp) in board.snakes.iter().enumerate().skip(1) {
            if opp.health <= 0 || opp.body.is_empty() {
                continue;
            }
            let opp_head = opp.body[0];
            let dist_to_us = manhattan_distance(head, opp_head);
            println!("  Snake {}: Head=({},{}), Length={}, Health={}, Distance from us={}",
                idx, opp_head.x, opp_head.y, opp.length, opp.health, dist_to_us);
        }
        println!();

        // Find nearest food
        if !board.food.is_empty() {
            let (nearest_food, distance) = board.food.iter()
                .map(|&food| (food, manhattan_distance(head, food)))
                .min_by_key(|(_, dist)| *dist)
                .unwrap();
            println!("Nearest Food:");
            println!("  Position: ({}, {})", nearest_food.x, nearest_food.y);
            println!("  Distance from us: {}", distance);

            // Check if food is "guarded" (opponent within distance 2 AND longer/equal)
            let mut is_guarded = false;
            for (idx, opp) in board.snakes.iter().enumerate().skip(1) {
                if opp.health <= 0 || opp.body.is_empty() {
                    continue;
                }
                let opp_head = opp.body[0];
                let dist_to_food = manhattan_distance(opp_head, nearest_food);
                if dist_to_food <= 2 && opp.length >= snake.length {
                    println!("  ⚠️  GUARDED by Snake {} (dist={}, length={})", idx, dist_to_food, opp.length);
                    is_guarded = true;
                }
            }
            if !is_guarded {
                println!("  ✓ SAFE (no threats within distance 2)");
            }

            // Calculate expected urgency multiplier
            if distance <= 2 {
                let urgency_mult = if distance == 1 && !is_guarded {
                    if health < 70 {
                        5.0
                    } else {
                        3.0
                    }
                } else {
                    1.0
                };
                println!("  Expected urgency multiplier: {}x", urgency_mult);
                let base_bonus = config.scores.immediate_food_bonus;
                let total_food_score = (base_bonus as f32 * urgency_mult * config.scores.weight_health) as i32;
                println!("  Expected food score: {} × {} × {} = {}",
                    base_bonus, urgency_mult, config.scores.weight_health, total_food_score);
            }
            println!();
        }

        println!("Move Chosen: {}", chosen_move_str);
        println!();

        // Evaluate all possible moves
        println!("═══════════════════════════════════════════════════════════");
        println!("Move Evaluation Breakdown");
        println!("═══════════════════════════════════════════════════════════");
        println!();

        let directions = vec![
            (Direction::Up, "up"),
            (Direction::Down, "down"),
            (Direction::Left, "left"),
            (Direction::Right, "right"),
        ];

        for (dir, name) in directions {
            let next_pos = apply_direction(head, dir);

            // Check if move is legal
            if !is_move_legal(&board, next_pos, snake) {
                println!("{}: ILLEGAL (out of bounds or collision)", name.to_uppercase());
                println!();
                continue;
            }

            // For detailed breakdown, we'd need to expose internal evaluation functions
            // For now, show basic information
            let on_food = board.food.contains(&next_pos);
            let chosen = name == chosen_move_str;

            println!("{}{}: {}",
                name.to_uppercase(),
                if chosen { " [CHOSEN]" } else { "" },
                if on_food { "TAKES FOOD" } else { "no food" }
            );
            println!("  Next position: ({}, {})", next_pos.x, next_pos.y);

            if on_food {
                println!("  ⚠️  THIS MOVE TAKES FOOD BUT WAS {}",
                    if chosen { "CHOSEN ✓" } else { "NOT CHOSEN!" }
                );
            }

            println!();
        }

        println!("═══════════════════════════════════════════════════════════");
        println!("Configuration Weights");
        println!("═══════════════════════════════════════════════════════════");
        println!("weight_health: {}", config.scores.weight_health);
        println!("weight_space: {}", config.scores.weight_space);
        println!("weight_attack: {}", config.scores.weight_attack);
        println!("immediate_food_bonus: {}", config.scores.immediate_food_bonus);
        println!("immediate_food_distance: {}", config.scores.immediate_food_distance);
        println!("escape_route_penalty_base: {}", config.scores.escape_route_penalty_base);
        println!("corner_danger_base: {}", config.scores.corner_danger_base);
        println!("wall_penalty_base: {}", config.scores.wall_penalty_base);
        println!("tail_chasing_penalty_per_segment: {}", config.scores.tail_chasing_penalty_per_segment);
        println!("articulation_point_penalty: {}", config.scores.articulation_point_penalty);

        return;
    }

    eprintln!("Turn {} not found in {}", target_turn, file_path);
    std::process::exit(1);
}

fn manhattan_distance(a: Coord, b: Coord) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn apply_direction(pos: Coord, dir: Direction) -> Coord {
    match dir {
        Direction::Up => Coord { x: pos.x, y: pos.y + 1 },
        Direction::Down => Coord { x: pos.x, y: pos.y - 1 },
        Direction::Left => Coord { x: pos.x - 1, y: pos.y },
        Direction::Right => Coord { x: pos.x + 1, y: pos.y },
    }
}

fn is_move_legal(board: &Board, next_pos: Coord, our_snake: &starter_snake_rust::types::Battlesnake) -> bool {
    // Check bounds
    if next_pos.x < 0 || next_pos.x >= board.width as i32 ||
       next_pos.y < 0 || next_pos.y >= board.height as i32 {
        return false;
    }

    // Check collision with all snake bodies (except tails that will move)
    for snake in &board.snakes {
        if snake.health <= 0 {
            continue;
        }
        // Check all segments except tail (which will move)
        let check_segments = if snake.body.len() > 1 {
            &snake.body[..snake.body.len() - 1]
        } else {
            &snake.body[..]
        };

        if check_segments.contains(&next_pos) {
            return false;
        }
    }

    // Check if it's our neck (can't reverse)
    if our_snake.body.len() > 1 {
        let neck = our_snake.body[1];
        if next_pos == neck {
            return false;
        }
    }

    true
}
