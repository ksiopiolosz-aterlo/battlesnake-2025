/// Analyzes trapped deaths by working backwards to understand what moves could have avoided the trap
/// and why the algorithm didn't choose them. Shows detailed score breakdowns for each alternative.

use starter_snake_rust::bot::{Bot, DetailedScore};
use starter_snake_rust::config::Config;
use starter_snake_rust::types::{Board, Coord, Direction};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <game_file.jsonl> [--lookback N] [--turn T] [--detailed]", args[0]);
        eprintln!("Analyzes trapped deaths by working backwards to find alternative moves");
        eprintln!("Options:");
        eprintln!("  --lookback N : Analyze N turns before death (default: 10)");
        eprintln!("  --turn T     : Analyze only specific turn T");
        eprintln!("  --detailed   : Show detailed score breakdowns");
        std::process::exit(1);
    }

    let game_file = &args[1];
    let mut lookback_turns = 10;
    let mut specific_turn: Option<i32> = None;
    let mut show_detailed = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--lookback" if i + 1 < args.len() => {
                lookback_turns = args[i + 1].parse().unwrap_or(10);
                i += 2;
            }
            "--turn" if i + 1 < args.len() => {
                specific_turn = Some(args[i + 1].parse().expect("Invalid turn number"));
                i += 2;
            }
            "--detailed" => {
                show_detailed = true;
                i += 1;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                i += 1;
            }
        }
    }

    let config = Config::load_or_default();

    println!("============================================================");
    println!("Trap Decision Analysis with Score Breakdowns");
    println!("============================================================\n");
    println!("Analyzing: {}", game_file);
    if let Some(turn) = specific_turn {
        println!("Analyzing only turn: {}", turn);
    } else {
        println!("Looking back: {} turns before death", lookback_turns);
    }
    println!("Detailed scores: {}\n", if show_detailed { "YES" } else { "NO" });

    match analyze_game(game_file, lookback_turns, specific_turn, show_detailed, &config) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Error analyzing game: {}", e);
            std::process::exit(1);
        }
    }
}

fn analyze_game(
    file_path: &str,
    lookback: usize,
    specific_turn: Option<i32>,
    show_detailed: bool,
    config: &Config,
) -> Result<(), String> {
    let file = File::open(file_path).map_err(|e| format!("Failed to open file: {}", e))?;
    let reader = BufReader::new(file);

    let mut entries: Vec<(i32, Direction, Board)> = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;
        let entry: serde_json::Value = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let turn = entry["turn"].as_i64().ok_or("Missing turn")? as i32;
        let chosen_move_str = entry["chosen_move"].as_str().ok_or("Missing chosen_move")?;
        let chosen_move = parse_direction(chosen_move_str)?;

        let board: Board = serde_json::from_value(entry["board"].clone())
            .map_err(|e| format!("Failed to parse board: {}", e))?;

        entries.push((turn, chosen_move, board));
    }

    if entries.is_empty() {
        return Err("No entries found in file".to_string());
    }

    // Find death turn (last turn)
    let death_turn = entries.last().unwrap().0;
    let death_board = &entries.last().unwrap().2;

    // Find our snake ID (first snake with health > 0 at turn 0)
    let our_snake_id = entries[0].2.snakes.iter()
        .find(|s| s.health > 0)
        .ok_or("No alive snake found")?
        .id.clone();

    println!("Death occurred at turn {}", death_turn);
    println!("Our snake ID: {}", our_snake_id);

    // Check death cause
    let our_snake_at_death = death_board.snakes.iter()
        .find(|s| s.id == our_snake_id)
        .ok_or("Our snake not found at death")?;

    if our_snake_at_death.health == 0 {
        println!("Death cause: Starvation");
    } else {
        println!("Death cause: Trapped (no legal moves)");
    }

    println!("\n============================================================");
    println!("DECISION ANALYSIS");
    println!("============================================================\n");

    // Analyze turns leading up to death
    let start_analysis = if let Some(turn) = specific_turn {
        entries.iter().position(|(t, _, _)| *t == turn).ok_or("Turn not found")?
    } else if death_turn as usize > lookback {
        death_turn as usize - lookback
    } else {
        0
    };

    let end_analysis = if specific_turn.is_some() {
        start_analysis + 1
    } else {
        entries.len()
    };

    for i in start_analysis..end_analysis {
        let (turn, chosen_move, board) = &entries[i];

        // Find our snake
        let our_snake = board.snakes.iter()
            .find(|s| s.id == our_snake_id)
            .ok_or("Our snake not found")?;

        if our_snake.health == 0 {
            continue; // Skip if we're already dead
        }

        let our_head = our_snake.body.first().ok_or("Empty body")?;

        println!("─────────────────────────────────────────────────────────");
        println!("Turn {}: Chose {:?}", turn, chosen_move);
        println!("Position: ({}, {}), Health: {}, Length: {}",
            our_head.x, our_head.y, our_snake.health, our_snake.length);

        // Analyze all possible moves with detailed scores
        let all_directions = vec![Direction::Up, Direction::Down, Direction::Left, Direction::Right];
        let mut move_scores: Vec<(Direction, Option<DetailedScore>)> = Vec::new();

        for dir in &all_directions {
            let score = if is_move_legal(board, &our_snake_id, *dir) {
                Some(Bot::evaluate_move_detailed(board, &our_snake_id, *dir, config))
            } else {
                None
            };
            move_scores.push((*dir, score));
        }

        // Find best alternative
        let chosen_score = move_scores.iter()
            .find(|(d, _)| d == chosen_move)
            .and_then(|(_, s)| s.as_ref());

        let best_alternative = move_scores.iter()
            .filter(|(d, s)| d != chosen_move && s.is_some())
            .max_by_key(|(_, s)| s.as_ref().unwrap().total);

        // Display results
        if show_detailed {
            println!("\nDetailed Score Breakdown:");
            println!("{:>8} | {:>8} | {:>6} | {:>6} | {:>6} | {:>6} | {:>6} | {:>6} | {:>8} | {:>6}",
                "Move", "TOTAL", "Health", "Space", "Ctrl", "Attack", "Length", "Wall", "H-Coll", "Center");
            println!("{:-<110}", "");

            for (dir, score_opt) in &move_scores {
                let marker = if dir == chosen_move { ">" } else { " " };
                if let Some(score) = score_opt {
                    println!("{}{:>7} | {:>8} | {:>6} | {:>6} | {:>6} | {:>6} | {:>6} | {:>6} | {:>8} | {:>6}",
                        marker,
                        format!("{:?}", dir),
                        score.total,
                        score.health,
                        score.space,
                        score.control,
                        score.attack,
                        score.length,
                        score.wall_penalty,
                        score.head_collision,
                        score.center_bias);
                } else {
                    println!("{}{:>7} | {:>8} | (illegal move)",
                        marker,
                        format!("{:?}", dir),
                        "N/A");
                }
            }
        } else {
            // Simple display
            for (dir, score_opt) in &move_scores {
                if let Some(score) = score_opt {
                    if dir == chosen_move {
                        println!("  ✓ Chosen: {:?} → score = {}", dir, score.total);
                    } else {
                        let diff = score.total - chosen_score.map(|s| s.total).unwrap_or(i32::MIN);
                        if diff > 0 {
                            println!("  🔴 BETTER: {:?} → score = {} (+{} vs chosen)", dir, score.total, diff);
                        } else {
                            println!("  Alternative: {:?} → score = {}", dir, score.total);
                        }
                    }
                } else {
                    println!("  ✗ {:?} - Illegal", dir);
                }
            }
        }

        // Summary: Was this a bad decision?
        if let (Some(best_alt), Some(chosen)) = (best_alternative, chosen_score) {
            let diff = best_alt.1.as_ref().unwrap().total - chosen.total;
            if diff > 1000 {
                println!("\n  ⚠️  BAD DECISION: Best alternative {:?} scores {} points higher!",
                    best_alt.0, diff);
                if show_detailed {
                    analyze_score_difference(chosen, best_alt.1.as_ref().unwrap());
                }
            }
        }

        println!();
    }

    Ok(())
}

fn is_move_legal(board: &Board, our_snake_id: &str, test_move: Direction) -> bool {
    let our_snake = match board.snakes.iter().find(|s| s.id == our_snake_id) {
        Some(s) => s,
        None => return false,
    };
    let head = match our_snake.body.first() {
        Some(h) => *h,
        None => return false,
    };

    let new_head = match test_move {
        Direction::Up => Coord { x: head.x, y: head.y + 1 },
        Direction::Down => Coord { x: head.x, y: head.y - 1 },
        Direction::Left => Coord { x: head.x - 1, y: head.y },
        Direction::Right => Coord { x: head.x + 1, y: head.y },
    };

    // Check bounds
    if new_head.x < 0 || new_head.x >= board.width || new_head.y < 0 || new_head.y >= board.height as i32 {
        return false;
    }

    // Check collision with neck
    if our_snake.body.len() > 1 && new_head == our_snake.body[1] {
        return false;
    }

    // Check collision with any snake body (except tails)
    for snake in &board.snakes {
        if snake.health == 0 {
            continue;
        }
        let body_to_check = if snake.body.len() > 1 {
            &snake.body[..snake.body.len() - 1]
        } else {
            &snake.body[..]
        };

        if body_to_check.contains(&new_head) {
            return false;
        }
    }

    true
}

fn analyze_score_difference(chosen: &DetailedScore, better: &DetailedScore) {
    println!("    Why the alternative is better:");

    let components = [
        ("Health", chosen.health, better.health),
        ("Space", chosen.space, better.space),
        ("Control", chosen.control, better.control),
        ("Attack", chosen.attack, better.attack),
        ("Wall penalty", chosen.wall_penalty, better.wall_penalty),
        ("Head collision", chosen.head_collision, better.head_collision),
        ("Center bias", chosen.center_bias, better.center_bias),
    ];

    for (name, chosen_val, better_val) in components {
        let diff = better_val - chosen_val;
        if diff.abs() > 100 {
            println!("      - {} difference: {:+} ({} → {})",
                name, diff, chosen_val, better_val);
        }
    }
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
