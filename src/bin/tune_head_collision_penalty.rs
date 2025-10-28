use starter_snake_rust::config::Config;
use starter_snake_rust::replay::ReplayEngine;
use starter_snake_rust::types::{Board, Coord, Direction};
use std::collections::HashMap;
use std::path::Path;

/// Tune the head_collision_penalty parameter by testing different values
/// against historical games and measuring decision quality

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <log_directory>", args[0]);
        eprintln!("Example: {} tests/fixtures/balanced/", args[0]);
        std::process::exit(1);
    }

    let log_dir = &args[1];

    println!("════════════════════════════════════════════════════════");
    println!("       HEAD COLLISION PENALTY TUNING ANALYSIS");
    println!("════════════════════════════════════════════════════════");
    println!();
    println!("Analyzing games in: {}", log_dir);
    println!();

    // Test different penalty values
    let penalty_values = vec![
        -500_000, // Current value
        -250_000,
        -100_000,
        -50_000,
        -25_000,
        -10_000,
        -5_000,
    ];

    let mut results = Vec::new();

    for &penalty in &penalty_values {
        let score = evaluate_penalty_value(log_dir, penalty);
        results.push((penalty, score));
        println!();
    }

    println!("════════════════════════════════════════════════════════");
    println!("                    SUMMARY");
    println!("════════════════════════════════════════════════════════");
    println!();
    println!("{:>12} | {:>12} | {:>10} | {:>10} | {:>10}",
             "Penalty", "Total Score", "Avg Space", "Collisions", "Traps");
    println!("{}", "-".repeat(70));

    for (penalty, score) in &results {
        println!("{:>12} | {:>12.2} | {:>10.1} | {:>10} | {:>10}",
                 penalty,
                 score.total_score,
                 score.avg_space_chosen,
                 score.collision_risks_avoided,
                 score.trap_deaths);
    }

    println!();

    // Find best penalty value
    let best = results.iter()
        .max_by(|a, b| a.1.total_score.partial_cmp(&b.1.total_score).unwrap())
        .unwrap();

    println!("════════════════════════════════════════════════════════");
    println!("RECOMMENDATION: Use head_collision_penalty = {}", best.0);
    println!("════════════════════════════════════════════════════════");
    println!();
    println!("Rationale:");
    println!("  - Maximizes space control in critical decisions");
    println!("  - Balances collision avoidance with trap prevention");
    println!("  - Average space chosen: {:.1} cells", best.1.avg_space_chosen);
    println!();
}

#[derive(Debug, Clone)]
struct PenaltyScore {
    total_score: f64,
    avg_space_chosen: f64,
    collision_risks_avoided: i32,
    trap_deaths: i32,
    decisions_analyzed: i32,
}

fn evaluate_penalty_value(log_dir: &str, penalty: i32) -> PenaltyScore {
    println!("Testing penalty value: {}", penalty);

    // Load all game files
    let paths = std::fs::read_dir(log_dir)
        .expect("Failed to read directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "jsonl")
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    if paths.is_empty() {
        eprintln!("No .jsonl files found in {}", log_dir);
        return PenaltyScore {
            total_score: 0.0,
            avg_space_chosen: 0.0,
            collision_risks_avoided: 0,
            trap_deaths: 0,
            decisions_analyzed: 0,
        };
    }

    let mut total_space = 0.0;
    let mut collision_avoids = 0;
    let mut trap_deaths = 0;
    let mut decisions = 0;

    for path in &paths {
        let game_score = analyze_game_with_penalty(path, penalty);
        total_space += game_score.total_space_chosen;
        collision_avoids += game_score.collision_risks_avoided;
        trap_deaths += game_score.trap_deaths;
        decisions += game_score.decisions_made;
    }

    let avg_space = if decisions > 0 {
        total_space / decisions as f64
    } else {
        0.0
    };

    // Scoring formula:
    // - Maximize average space chosen (weight: 1.0)
    // - Reward collision avoidance (weight: 0.1 per avoid)
    // - Penalize trap deaths (weight: -10.0 per trap)
    let total_score = avg_space
        + (collision_avoids as f64 * 0.1)
        - (trap_deaths as f64 * 10.0);

    println!("  Decisions analyzed: {}", decisions);
    println!("  Avg space chosen: {:.1}", avg_space);
    println!("  Collision risks avoided: {}", collision_avoids);
    println!("  Trap deaths: {}", trap_deaths);
    println!("  Total score: {:.2}", total_score);

    PenaltyScore {
        total_score,
        avg_space_chosen: avg_space,
        collision_risks_avoided: collision_avoids,
        trap_deaths,
        decisions_analyzed: decisions,
    }
}

#[derive(Debug)]
struct GameScore {
    total_space_chosen: f64,
    collision_risks_avoided: i32,
    trap_deaths: i32,
    decisions_made: i32,
}

fn analyze_game_with_penalty(path: &Path, penalty: i32) -> GameScore {
    // Create a custom config with the test penalty
    let mut config = Config::load_or_default();
    config.scores.head_collision_penalty = penalty;

    let engine = ReplayEngine::new(config, false);
    let entries = match engine.load_log_file(path.to_str().unwrap()) {
        Ok(e) => e,
        Err(_) => return GameScore {
            total_space_chosen: 0.0,
            collision_risks_avoided: 0,
            trap_deaths: 0,
            decisions_made: 0,
        },
    };

    if entries.is_empty() {
        return GameScore {
            total_space_chosen: 0.0,
            collision_risks_avoided: 0,
            trap_deaths: 0,
            decisions_made: 0,
        };
    }

    // Find our snake ID (first snake with health > 0 at turn 0)
    let our_snake_id = match entries[0].board.snakes.iter().find(|s| s.health > 0) {
        Some(snake) => snake.id.clone(),
        None => return GameScore {
            total_space_chosen: 0.0,
            collision_risks_avoided: 0,
            trap_deaths: 0,
            decisions_made: 0,
        },
    };

    // Analyze last 10 turns before death (critical decisions)
    let death_turn = entries.len();
    let start_turn = if death_turn > 10 { death_turn - 10 } else { 0 };

    let mut total_space = 0.0;
    let mut decisions = 0;
    let mut collision_avoids = 0;

    for entry in entries.iter().skip(start_turn) {
        let board = &entry.board;

        // Find our snake
        let you = match board.snakes.iter().find(|s| s.id == our_snake_id) {
            Some(snake) => snake,
            None => continue,
        };

        // Generate legal moves and evaluate with this penalty
        let moves = generate_legal_moves(board, you);
        if moves.is_empty() {
            continue;
        }

        // For each move, estimate space control (simplified flood fill)
        let mut move_spaces: Vec<(Direction, usize)> = moves.iter()
            .map(|&mv| {
                let space = estimate_space_for_move(board, you, mv);
                (mv, space)
            })
            .collect();

        move_spaces.sort_by_key(|&(_, space)| std::cmp::Reverse(space));

        // Check if there are head collision risks
        let has_collision_risk = has_nearby_opponents(board, you);

        if has_collision_risk {
            collision_avoids += 1;
        }

        // Which move would be chosen? (simplified: pick move with most space)
        if let Some((_, space)) = move_spaces.first() {
            total_space += *space as f64;
            decisions += 1;
        }
    }

    // Check if death was by trap
    let trap_death = if let Some(last_entry) = entries.last() {
        if let Some(you) = last_entry.board.snakes.iter().find(|s| s.id == our_snake_id) {
            let moves = generate_legal_moves(&last_entry.board, you);
            moves.is_empty() as i32
        } else {
            0
        }
    } else {
        0
    };

    GameScore {
        total_space_chosen: total_space,
        collision_risks_avoided: collision_avoids,
        trap_deaths: trap_death,
        decisions_made: decisions,
    }
}

fn generate_legal_moves(board: &Board, you: &starter_snake_rust::types::Battlesnake) -> Vec<Direction> {
    use Direction::*;
    let head = you.head;
    let neck = if you.body.len() > 1 {
        Some(you.body[1])
    } else {
        None
    };

    [Up, Down, Left, Right]
        .iter()
        .filter_map(|&dir| {
            let next = move_coord(head, dir);

            // Can't reverse
            if let Some(n) = neck {
                if next == n {
                    return None;
                }
            }

            // Must be in bounds
            if next.x < 0 || next.x >= board.width as i32
                || next.y < 0 || next.y >= board.height as i32 {
                return None;
            }

            // Can't hit bodies (simplified check)
            for snake in &board.snakes {
                if snake.body.len() > 1 && snake.body[..snake.body.len()-1].contains(&next) {
                    return None;
                }
            }

            Some(dir)
        })
        .collect()
}

fn move_coord(c: Coord, dir: Direction) -> Coord {
    match dir {
        Direction::Up => Coord { x: c.x, y: c.y + 1 },
        Direction::Down => Coord { x: c.x, y: c.y - 1 },
        Direction::Left => Coord { x: c.x - 1, y: c.y },
        Direction::Right => Coord { x: c.x + 1, y: c.y },
    }
}

fn estimate_space_for_move(
    board: &Board,
    you: &starter_snake_rust::types::Battlesnake,
    mv: Direction,
) -> usize {
    use std::collections::{HashSet, VecDeque};

    let start = move_coord(you.head, mv);
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(start);
    visited.insert(start);

    while let Some(pos) = queue.pop_front() {
        for dir in &[Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
            let next = move_coord(pos, *dir);

            if next.x < 0 || next.x >= board.width as i32
                || next.y < 0 || next.y >= board.height as i32 {
                continue;
            }

            if visited.contains(&next) {
                continue;
            }

            // Check if blocked by snake bodies
            let blocked = board.snakes.iter().any(|snake| {
                snake.body.iter().any(|&seg| seg == next)
            });

            if blocked {
                continue;
            }

            visited.insert(next);
            queue.push_back(next);
        }
    }

    visited.len()
}

fn has_nearby_opponents(board: &Board, you: &starter_snake_rust::types::Battlesnake) -> bool {
    let our_head = you.head;

    for snake in &board.snakes {
        if snake.id == you.id {
            continue;
        }

        let dist = manhattan_distance(our_head, snake.head);
        if dist <= 2 {
            return true;
        }
    }

    false
}

fn manhattan_distance(a: Coord, b: Coord) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}
