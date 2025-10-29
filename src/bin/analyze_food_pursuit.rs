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

fn apply_move(pos: Coord, direction: &str) -> Coord {
    match direction {
        "up" => Coord { x: pos.x, y: pos.y + 1 },
        "down" => Coord { x: pos.x, y: pos.y - 1 },
        "left" => Coord { x: pos.x - 1, y: pos.y },
        "right" => Coord { x: pos.x + 1, y: pos.y },
        _ => pos,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: analyze_food_pursuit <game_file.jsonl>");
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

    println!("═══════════════════════════════════════════════════════════");
    println!("           FOOD PURSUIT ANALYSIS");
    println!("═══════════════════════════════════════════════════════════\n");
    println!("Analyzing {} turns for potential food aversion...\n", entries.len());

    let mut aversion_cases = Vec::new();

    for entry in &entries {
        let our_snake = &entry.board.snakes[0];
        let our_head = our_snake.body[0];

        // Find nearest food
        let nearest_food = entry.board.food.iter()
            .min_by_key(|&&food| manhattan_distance(our_head, food));

        if let Some(&food_pos) = nearest_food {
            let food_dist = manhattan_distance(our_head, food_pos);

            // Only analyze food at distance 2-5 (distance 1 is handled by immediate bonus)
            if food_dist >= 2 && food_dist <= 5 {
                // Calculate distance after our move
                let new_head = apply_move(our_head, &entry.chosen_move);
                let new_food_dist = manhattan_distance(new_head, food_pos);

                // Check if we're moving away from food or not closing distance
                let moving_away = new_food_dist > food_dist;
                let not_closing = new_food_dist >= food_dist;

                if not_closing {
                    // Check opponent distances
                    let mut nearest_opponent_dist = 999;
                    let mut nearest_opponent_name = String::from("None");

                    for (i, opponent) in entry.board.snakes.iter().enumerate() {
                        if i == 0 || opponent.health <= 0 || opponent.body.is_empty() {
                            continue;
                        }

                        let opp_dist = manhattan_distance(opponent.body[0], food_pos);
                        if opp_dist < nearest_opponent_dist {
                            nearest_opponent_dist = opp_dist;
                            nearest_opponent_name = opponent.name.clone();
                        }
                    }

                    // Calculate move advantage: how many moves ahead we are
                    let move_advantage = nearest_opponent_dist - food_dist;

                    // Flag if we have significant move advantage (3+ moves)
                    if move_advantage >= 3 {
                        aversion_cases.push((
                            entry.turn,
                            our_head,
                            food_pos,
                            food_dist,
                            new_food_dist,
                            our_snake.health,
                            nearest_opponent_name.clone(),
                            nearest_opponent_dist,
                            move_advantage,
                            moving_away,
                            entry.chosen_move.clone(),
                        ));
                    }
                }
            }
        }
    }

    if aversion_cases.is_empty() {
        println!("✅ No significant food aversion detected!");
        println!("   All food avoidance appears justified by opponent positioning.\n");
    } else {
        println!("⚠️  POTENTIAL FOOD AVERSION CASES:\n");
        println!("   (Cases where we had 3+ move advantage but didn't pursue food)\n");

        for (turn, our_pos, food_pos, food_dist, new_dist, health, opp_name, opp_dist, advantage, moving_away, chosen_move) in &aversion_cases {
            let direction_indicator = if *moving_away { "↗️ AWAY" } else { "→ SAME" };

            println!("Turn {}: {} {}", turn, direction_indicator, chosen_move);
            println!("  Our position: {:?}, Health: {}", our_pos, health);
            println!("  Food position: {:?}", food_pos);
            println!("  Distance: {} → {} ({})",
                food_dist,
                new_dist,
                if *moving_away { "increased" } else { "unchanged" }
            );
            println!("  Nearest opponent: {} at distance {} ({} move advantage)",
                opp_name, opp_dist, advantage);
            println!();
        }

        println!("═══════════════════════════════════════════════════════════");
        println!("Total potential aversion cases: {}", aversion_cases.len());
        println!("═══════════════════════════════════════════════════════════\n");
    }
}
