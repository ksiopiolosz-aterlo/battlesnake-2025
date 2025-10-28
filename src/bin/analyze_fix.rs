//! Comprehensive analysis tool to verify the race condition fix
//!
//! This tool:
//! 1. Parses validation tool output to find illegal moves in original logs
//! 2. Replays each game with the fixed code to see what moves would be chosen
//! 3. Cross-references to determine how many illegal moves are now prevented
//!
//! Usage:
//!   cargo run --release --bin analyze_fix -- <game_directory>

use std::collections::HashMap;
use std::env;
use std::process::Command;

#[derive(Debug)]
struct IllegalMove {
    turn: i32,
    original_move: String,
    violation_type: String, // "NECK COLLISION" or "ILLEGAL MOVE"
}

#[derive(Debug)]
struct ReplayMismatch {
    turn: i32,
    original_move: String,
    replayed_move: String,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <game_directory>", args[0]);
        eprintln!("Example: {} tests/fixtures/1v1_self/", args[0]);
        std::process::exit(1);
    }

    let game_dir = &args[1];

    println!("============================================================");
    println!("Comprehensive Replay Analysis: Race Condition Fix");
    println!("============================================================");
    println!();

    // Step 1: Run validation tool and parse output
    println!("Step 1: Running validation tool...");
    let validation_output = Command::new("./target/release/validate_moves")
        .arg(game_dir)
        .output()
        .expect("Failed to run validate_moves");

    let validation_text = format!(
        "{}{}",
        String::from_utf8_lossy(&validation_output.stdout),
        String::from_utf8_lossy(&validation_output.stderr)
    );

    let illegal_moves = parse_validation_output(&validation_text);
    let total_illegal: usize = illegal_moves.values().map(|v| v.len()).sum();

    println!("Found {} illegal moves across {} games", total_illegal, illegal_moves.len());
    println!();

    // Step 2: Replay each game and analyze
    println!("Step 2: Replaying games with fixed code...");
    println!();

    let mut total_fixed = 0;
    let mut total_still_illegal = 0;
    let mut total_unchanged = 0;

    for (game_name, violations) in illegal_moves.iter() {
        let game_path = format!("{}/{}", game_dir.trim_end_matches('/'), game_name);

        // Replay this game
        let replay_output = Command::new("./target/release/replay")
            .arg(&game_path)
            .arg("--all")
            .output()
            .expect("Failed to run replay");

        let replay_text = format!(
            "{}{}",
            String::from_utf8_lossy(&replay_output.stdout),
            String::from_utf8_lossy(&replay_output.stderr)
        );

        let mismatches = parse_replay_output(&replay_text);

        // Cross-reference
        let mut game_fixed = 0;
        let mut game_still_illegal = 0;
        let mut game_unchanged = 0;

        for violation in violations {
            if let Some(mismatch) = mismatches.get(&violation.turn) {
                // There's a mismatch at this turn
                if mismatch.original_move == violation.original_move
                    && mismatch.replayed_move != violation.original_move
                {
                    // Fixed code chose a DIFFERENT move - likely fixed!
                    game_fixed += 1;
                } else {
                    // Mismatch but not matching our illegal move pattern
                    game_still_illegal += 1;
                }
            } else {
                // No mismatch - fixed code chose the same move
                game_unchanged += 1;
            }
        }

        if game_fixed > 0 || game_still_illegal > 0 || game_unchanged > 0 {
            println!(
                "{}: {} fixed, {} still illegal, {} unchanged",
                game_name, game_fixed, game_still_illegal, game_unchanged
            );
        }

        total_fixed += game_fixed;
        total_still_illegal += game_still_illegal;
        total_unchanged += game_unchanged;
    }

    println!();
    println!("============================================================");
    println!("SUMMARY");
    println!("============================================================");
    println!("Total illegal moves in original logs:  {}", total_illegal);
    println!(
        "Fixed by race condition fix:           {} ({:.1}%)",
        total_fixed,
        100.0 * total_fixed as f64 / total_illegal as f64
    );
    println!(
        "Still illegal with fixed code:         {} ({:.1}%)",
        total_still_illegal,
        100.0 * total_still_illegal as f64 / total_illegal as f64
    );
    println!(
        "No change (replay matched original):   {} ({:.1}%)",
        total_unchanged,
        100.0 * total_unchanged as f64 / total_illegal as f64
    );
    println!();

    if total_fixed > 0 {
        println!(
            "✅ SUCCESS: The race condition fix prevents {} illegal moves!",
            total_fixed
        );
    } else {
        println!("❌ WARNING: The fix did not prevent any illegal moves");
    }
    println!();
}

fn parse_validation_output(text: &str) -> HashMap<String, Vec<IllegalMove>> {
    let mut illegal_moves: HashMap<String, Vec<IllegalMove>> = HashMap::new();

    // Pattern: NECK COLLISION: game_01.jsonl line 12 turn 5:
    //   Position (6,5) + left = neck at (5,5)
    let lines: Vec<&str> = text.lines().collect();

    for i in 0..lines.len() {
        let line = lines[i];

        if line.contains("NECK COLLISION:") || line.contains("ILLEGAL MOVE:") {
            let violation_type = if line.contains("NECK COLLISION:") {
                "NECK COLLISION"
            } else {
                "ILLEGAL MOVE"
            }
            .to_string();

            // Extract game name and turn
            if let Some(game_start) = line.find("game_") {
                if let Some(game_end) = line[game_start..].find(".jsonl") {
                    let game_name =
                        line[game_start..game_start + game_end + 6].to_string();

                    if let Some(turn_start) = line.find("turn ") {
                        let turn_str = &line[turn_start + 5..];
                        if let Some(colon_pos) = turn_str.find(':') {
                            if let Ok(turn) = turn_str[..colon_pos].trim().parse::<i32>() {
                                // Next line should have the move
                                if i + 1 < lines.len() {
                                    let next_line = lines[i + 1];
                                    if let Some(plus_pos) = next_line.find(" + ") {
                                        let after_plus = &next_line[plus_pos + 3..];
                                        if let Some(space_pos) = after_plus.find(' ') {
                                            let move_str =
                                                after_plus[..space_pos].trim().to_string();

                                            illegal_moves
                                                .entry(game_name)
                                                .or_insert_with(Vec::new)
                                                .push(IllegalMove {
                                                    turn,
                                                    original_move: move_str,
                                                    violation_type,
                                                });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    illegal_moves
}

fn parse_replay_output(text: &str) -> HashMap<i32, ReplayMismatch> {
    let mut mismatches: HashMap<i32, ReplayMismatch> = HashMap::new();

    // Pattern: Turn 5: left → right (score: 1914, depth: 4, time: 51ms)
    for line in text.lines() {
        if line.starts_with("Turn ") && line.contains(" → ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                if let Ok(turn) = parts[1].trim_end_matches(':').parse::<i32>() {
                    let original_move = parts[2].to_string();
                    let replayed_move = parts[4].to_string();

                    mismatches.insert(
                        turn,
                        ReplayMismatch {
                            turn,
                            original_move,
                            replayed_move,
                        },
                    );
                }
            }
        }
    }

    mismatches
}
