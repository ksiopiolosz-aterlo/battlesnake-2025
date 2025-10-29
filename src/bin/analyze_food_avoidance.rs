use starter_snake_rust::types::{Board, Coord, Direction};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Debug)]
struct FoodAvoidanceEvent {
    turn: u32,
    health: i32,
    head: Coord,
    nearest_food: Coord,
    distance: i32,
    chosen_move: Direction,
    move_toward_food: Option<Direction>,
}

fn manhattan_distance(a: Coord, b: Coord) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn direction_toward(from: Coord, to: Coord) -> Option<Direction> {
    let dx = to.x - from.x;
    let dy = to.y - from.y;

    if dx.abs() > dy.abs() {
        if dx > 0 {
            Some(Direction::Right)
        } else {
            Some(Direction::Left)
        }
    } else if dy.abs() > 0 {
        if dy > 0 {
            Some(Direction::Up)
        } else {
            Some(Direction::Down)
        }
    } else {
        None
    }
}

fn direction_to_string(dir: &Direction) -> &str {
    match dir {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <game.jsonl>", args[0]);
        std::process::exit(1);
    }

    let file_path = &args[1];
    let file = File::open(file_path).expect("Failed to open file");
    let reader = BufReader::new(file);

    let mut avoidance_events = Vec::new();

    for line in reader.lines() {
        let line = line.expect("Failed to read line");
        let entry: serde_json::Value = serde_json::from_str(&line).expect("Failed to parse JSON");

        let turn = entry["turn"].as_u64().unwrap() as u32;
        let chosen_move_str = entry["chosen_move"].as_str().unwrap();
        let chosen_move = match chosen_move_str {
            "up" => Direction::Up,
            "down" => Direction::Down,
            "left" => Direction::Left,
            "right" => Direction::Right,
            _ => continue,
        };

        let board: Board = serde_json::from_value(entry["board"].clone()).expect("Failed to parse board");

        if board.snakes.is_empty() {
            continue;
        }

        let snake = &board.snakes[0];
        if snake.body.is_empty() || board.food.is_empty() {
            continue;
        }

        let head = snake.body[0];
        let health = snake.health;

        // Find nearest food
        let (nearest_food, distance) = board.food.iter()
            .map(|&food| (food, manhattan_distance(head, food)))
            .min_by_key(|(_, dist)| *dist)
            .unwrap();

        // Check if food is within distance 3 and health is below 70
        if distance <= 3 && health < 70 {
            let move_toward = direction_toward(head, nearest_food);

            // Check if bot moved away from food
            if let Some(toward) = move_toward {
                if direction_to_string(&toward) != chosen_move_str {
                    avoidance_events.push(FoodAvoidanceEvent {
                        turn,
                        health,
                        head,
                        nearest_food,
                        distance,
                        chosen_move,
                        move_toward_food: Some(toward),
                    });
                }
            }
        }
    }

    println!("Food Avoidance Analysis for: {}", file_path);
    println!("═══════════════════════════════════════════════════════════");
    println!("Found {} potential food avoidance events", avoidance_events.len());
    println!();

    for event in &avoidance_events {
        println!("Turn {}: Health={}, Head=({},{}), Food=({},{}) dist={}",
            event.turn,
            event.health,
            event.head.x, event.head.y,
            event.nearest_food.x, event.nearest_food.y,
            event.distance
        );
        println!("  Chose: {}, Should move: {}",
            direction_to_string(&event.chosen_move),
            direction_to_string(&event.move_toward_food.as_ref().unwrap())
        );
        println!();
    }

    if avoidance_events.is_empty() {
        println!("No obvious food avoidance detected (checked food within distance 3, health < 70)");
    }
}
