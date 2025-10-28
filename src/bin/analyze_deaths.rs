//! Death Pattern Analysis Tool
//!
//! Analyzes game logs to understand why snakes died and identify patterns.
//! Focuses on the final turns to categorize death causes and suggest improvements.
//!
//! Usage:
//!   cargo run --release --bin analyze_deaths -- <log_directory>
//!
//! Output:
//!   - Death cause categorization (starvation, collision, trapped)
//!   - Final board states for each death
//!   - Common patterns and preventable mistakes
//!   - Strategic recommendations

use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
struct DeathAnalysis {
    game_name: String,
    total_turns: usize,
    winner_id: String,
    winner_length: usize,
    loser_id: String,
    death_cause: DeathCause,
    final_health: i64,
    final_length: usize,
    available_space: Option<usize>,
    food_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum DeathCause {
    Starvation,           // Health reached 0
    WallCollision,        // Hit boundary
    SelfCollision,        // Hit own body
    OpponentCollision,    // Hit opponent's body
    HeadToHead,           // Head-to-head with equal/longer opponent
    Trapped,              // No legal moves available
    Unknown,
}

impl DeathCause {
    fn as_str(&self) -> &str {
        match self {
            DeathCause::Starvation => "Starvation",
            DeathCause::WallCollision => "Wall Collision",
            DeathCause::SelfCollision => "Self Collision",
            DeathCause::OpponentCollision => "Opponent Collision",
            DeathCause::HeadToHead => "Head-to-Head Loss",
            DeathCause::Trapped => "Trapped (No Legal Moves)",
            DeathCause::Unknown => "Unknown",
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <log_directory>", args[0]);
        eprintln!("Example: {} tests/fixtures/1v1_self/", args[0]);
        std::process::exit(1);
    }

    let log_dir = &args[1];

    println!("============================================================");
    println!("Death Pattern Analysis");
    println!("============================================================");
    println!();
    println!("Analyzing: {}", log_dir);
    println!();

    // Get all JSONL files
    let paths: Vec<_> = fs::read_dir(log_dir)
        .expect("Failed to read log directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();

    if paths.is_empty() {
        eprintln!("No .jsonl files found in: {}", log_dir);
        std::process::exit(1);
    }

    let mut all_deaths: Vec<DeathAnalysis> = Vec::new();

    for path in &paths {
        match analyze_game_death(&path) {
            Ok(analysis) => {
                all_deaths.push(analysis);
            }
            Err(e) => {
                eprintln!("Error analyzing {}: {}", path.display(), e);
            }
        }
    }

    print_death_report(&all_deaths);
}

fn analyze_game_death(path: &Path) -> Result<DeathAnalysis, String> {
    let file = File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let reader = BufReader::new(file);
    let mut turns: HashMap<u64, Vec<Value>> = HashMap::new();

    // Group entries by turn
    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line: {}", e))?;

        if line.trim().is_empty() {
            continue;
        }

        let entry: Value = serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        let turn = entry["turn"].as_u64().unwrap_or(0);
        turns.entry(turn).or_insert_with(Vec::new).push(entry);
    }

    let total_turns = turns.len();

    // Get the final turn to analyze death
    let max_turn = turns.keys().max().copied().unwrap_or(0);
    let final_entries = turns.get(&max_turn).ok_or("No final turn found")?;

    // Get one entry to analyze the final board state
    let final_entry = final_entries.first().ok_or("No entries in final turn")?;
    let board = &final_entry["board"];

    let snakes = board["snakes"].as_array().ok_or("No snakes array")?;

    // Find winner (alive) and loser (dead or lower health)
    let (winner, loser) = identify_winner_loser(snakes)?;

    let winner_id = winner["id"].as_str().unwrap_or("unknown").to_string();
    let winner_length = winner["length"].as_u64().unwrap_or(0) as usize;

    let loser_id = loser["id"].as_str().unwrap_or("unknown").to_string();
    let final_health = loser["health"].as_i64().unwrap_or(0);
    let final_length = loser["length"].as_u64().unwrap_or(0) as usize;

    // Determine death cause
    let death_cause = determine_death_cause(&loser, snakes, board);

    let food_count = board["food"].as_array().map(|f| f.len()).unwrap_or(0);

    Ok(DeathAnalysis {
        game_name: path.file_name().unwrap().to_str().unwrap().to_string(),
        total_turns,
        winner_id,
        winner_length,
        loser_id,
        death_cause,
        final_health,
        final_length,
        available_space: None, // Could calculate with flood fill
        food_count,
    })
}

fn identify_winner_loser(snakes: &[Value]) -> Result<(Value, Value), String> {
    if snakes.is_empty() {
        return Err("No snakes found".to_string());
    }

    // Find our snake (Rusty) - this is the one we're analyzing
    let our_snake = snakes
        .iter()
        .find(|s| {
            s["name"].as_str() == Some("Rusty") ||
            s["id"].as_str().map(|id| id.contains("Rusty")).unwrap_or(false)
        })
        .ok_or("Could not find Rusty snake in game")?;

    // Find the winner - the snake with the highest health, or longest if tied
    let winner = snakes
        .iter()
        .filter(|s| s["id"] != our_snake["id"]) // Not us
        .max_by_key(|s| {
            let health = s["health"].as_i64().unwrap_or(0);
            let length = s["length"].as_u64().unwrap_or(0);
            (health, length)
        })
        .unwrap_or(snakes.first().unwrap()); // Fallback to first snake if all are us (shouldn't happen)

    Ok((winner.clone(), our_snake.clone()))
}

fn determine_death_cause(loser: &Value, snakes: &[Value], board: &Value) -> DeathCause {
    let health = loser["health"].as_i64().unwrap_or(0);

    // Check for starvation
    if health == 0 {
        return DeathCause::Starvation;
    }

    // Check if snake has body (if not, something went wrong)
    let body = match loser["body"].as_array() {
        Some(b) if !b.is_empty() => b,
        _ => return DeathCause::Unknown,
    };

    let head = &body[0];
    let head_x = head["x"].as_i64().unwrap_or(0);
    let head_y = head["y"].as_i64().unwrap_or(0);

    // Check for wall collision
    let width = board["width"].as_i64().unwrap_or(11);
    let height = board["height"].as_i64().unwrap_or(11);

    if head_x < 0 || head_x >= width || head_y < 0 || head_y >= height {
        return DeathCause::WallCollision;
    }

    // Check for self collision (head overlaps with body segment beyond neck)
    for segment in body.iter().skip(1) {
        if segment["x"] == head["x"] && segment["y"] == head["y"] {
            return DeathCause::SelfCollision;
        }
    }

    // Check for opponent collision
    for snake in snakes {
        let snake_id = snake["id"].as_str().unwrap_or("");
        let loser_id = loser["id"].as_str().unwrap_or("");

        if snake_id == loser_id {
            continue; // Skip self
        }

        if let Some(opponent_body) = snake["body"].as_array() {
            // Check head-to-head
            if !opponent_body.is_empty() {
                let opp_head = &opponent_body[0];
                if opp_head["x"] == head["x"] && opp_head["y"] == head["y"] {
                    return DeathCause::HeadToHead;
                }
            }

            // Check collision with opponent body
            for segment in opponent_body {
                if segment["x"] == head["x"] && segment["y"] == head["y"] {
                    return DeathCause::OpponentCollision;
                }
            }
        }
    }

    // If alive but game ended, likely trapped
    if health > 0 {
        return DeathCause::Trapped;
    }

    DeathCause::Unknown
}

fn print_death_report(deaths: &[DeathAnalysis]) {
    println!("Analyzed {} games", deaths.len());
    println!();

    // Categorize by death cause
    let mut by_cause: HashMap<String, Vec<&DeathAnalysis>> = HashMap::new();
    for death in deaths {
        by_cause
            .entry(death.death_cause.as_str().to_string())
            .or_insert_with(Vec::new)
            .push(death);
    }

    println!("============================================================");
    println!("DEATH CAUSE DISTRIBUTION");
    println!("============================================================");

    let mut causes: Vec<_> = by_cause.iter().collect();
    causes.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));

    for (cause, games) in &causes {
        println!("{}: {} games ({:.1}%)",
            cause,
            games.len(),
            100.0 * games.len() as f64 / deaths.len() as f64
        );
    }
    println!();

    // Quick games analysis (< 100 turns)
    let quick_deaths: Vec<_> = deaths.iter().filter(|d| d.total_turns < 100).collect();

    if !quick_deaths.is_empty() {
        println!("============================================================");
        println!("QUICK GAMES (<100 turns) - {} games", quick_deaths.len());
        println!("============================================================");

        for death in &quick_deaths {
            println!("{} (turn {}): {}",
                death.game_name,
                death.total_turns,
                death.death_cause.as_str()
            );
            println!("  Loser: health={}, length={}, food_available={}",
                death.final_health,
                death.final_length,
                death.food_count
            );
        }
        println!();
    }

    // Epic games analysis (> 300 turns)
    let epic_games: Vec<_> = deaths.iter().filter(|d| d.total_turns > 300).collect();

    if !epic_games.is_empty() {
        println!("============================================================");
        println!("EPIC GAMES (>300 turns) - {} games", epic_games.len());
        println!("============================================================");

        for death in &epic_games {
            println!("{} (turn {}): {}",
                death.game_name,
                death.total_turns,
                death.death_cause.as_str()
            );
            println!("  Winner: length={}", death.winner_length);
            println!("  Loser: health={}, length={}", death.final_health, death.final_length);
        }
        println!();
    }

    println!("============================================================");
    println!("STRATEGIC INSIGHTS");
    println!("============================================================");

    // Count preventable deaths
    let starvation_count = by_cause.get("Starvation").map(|v| v.len()).unwrap_or(0);
    let trapped_count = by_cause.get("Trapped (No Legal Moves)").map(|v| v.len()).unwrap_or(0);
    let collision_count = by_cause.get("Wall Collision").map(|v| v.len()).unwrap_or(0)
        + by_cause.get("Self Collision").map(|v| v.len()).unwrap_or(0);

    if starvation_count > 0 {
        println!("• Starvation ({} games): Improve food-seeking behavior", starvation_count);
        println!("  - Consider increasing weight_health in evaluation");
        println!("  - Review health_threat_distance threshold");
    }

    if trapped_count > 0 {
        println!("• Trapped ({} games): Improve space control", trapped_count);
        println!("  - Consider increasing weight_space in evaluation");
        println!("  - Review flood fill and space calculation");
    }

    if collision_count > 0 {
        println!("• Collisions ({} games): Improve tactical awareness", collision_count);
        println!("  - Review move generation and validation");
        println!("  - Consider penalizing risky positions more heavily");
    }

    println!("============================================================");
}
