// Standalone replay tool for analyzing Battlesnake debug logs
//
// Usage:
//   cargo run --bin replay -- <log_file> [options]
//
// Options:
//   --all                  Replay all turns
//   --turns <turn1,turn2>  Replay specific turns (comma-separated)
//   --validate             Run validation mode with expected moves
//   --verbose              Show detailed output for each turn
//   --config <path>        Path to Snake.toml (default: Snake.toml)

use std::env;
use std::process;

// Import from the main crate
use starter_snake_rust::config::Config;
use starter_snake_rust::replay::ReplayEngine;
use starter_snake_rust::types::Direction;

fn print_usage() {
    eprintln!("Battlesnake Replay Tool");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("  replay <log_file> [OPTIONS]");
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("  --all                   Replay all turns in the log");
    eprintln!("  --turns <T1,T2,...>     Replay specific turns (comma-separated)");
    eprintln!("  --validate <T:M,...>    Validate expected moves (format: turn:move,...)");
    eprintln!("  --verbose               Show detailed output for each turn");
    eprintln!("  --config <path>         Path to Snake.toml (default: Snake.toml)");
    eprintln!("  --help                  Show this help message");
    eprintln!();
    eprintln!("EXAMPLES:");
    eprintln!("  # Replay all turns");
    eprintln!("  replay battlesnake_debug.jsonl --all");
    eprintln!();
    eprintln!("  # Replay specific turns");
    eprintln!("  replay battlesnake_debug.jsonl --turns 5,10,15");
    eprintln!();
    eprintln!("  # Validate expected moves");
    eprintln!("  replay battlesnake_debug.jsonl --validate 5:up,10:right");
    eprintln!();
    eprintln!("  # Verbose replay of all turns");
    eprintln!("  replay battlesnake_debug.jsonl --all --verbose");
}

fn parse_turns(s: &str) -> Result<Vec<i32>, String> {
    s.split(',')
        .map(|t| {
            t.trim()
                .parse::<i32>()
                .map_err(|e| format!("Invalid turn number '{}': {}", t, e))
        })
        .collect()
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

fn parse_expected_moves(s: &str) -> Result<Vec<(i32, Vec<Direction>)>, String> {
    s.split(',')
        .map(|pair| {
            let parts: Vec<&str> = pair.trim().split(':').collect();
            if parts.len() != 2 {
                return Err(format!("Invalid format '{}'. Expected 'turn:move'", pair));
            }

            let turn = parts[0]
                .parse::<i32>()
                .map_err(|e| format!("Invalid turn number '{}': {}", parts[0], e))?;

            // Support multiple acceptable moves separated by '|'
            let moves: Result<Vec<Direction>, String> = parts[1]
                .split('|')
                .map(|m| parse_direction(m.trim()))
                .collect();

            Ok((turn, moves?))
        })
        .collect()
}

fn main() {
    // Initialize logger
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage();
        process::exit(if args.contains(&"--help".to_string()) {
            0
        } else {
            1
        });
    }

    let log_file = &args[1];
    let mut config_path = "Snake.toml".to_string();
    let mut verbose = false;
    let mut mode = None;

    // Parse arguments
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => {
                mode = Some("all");
            }
            "--turns" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --turns requires an argument");
                    process::exit(1);
                }
                mode = Some("turns");
                i += 1;
            }
            "--validate" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --validate requires an argument");
                    process::exit(1);
                }
                mode = Some("validate");
                i += 1;
            }
            "--config" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --config requires an argument");
                    process::exit(1);
                }
                config_path = args[i + 1].clone();
                i += 1;
            }
            "--verbose" => {
                verbose = true;
            }
            _ => {
                eprintln!("Error: Unknown option '{}'", args[i]);
                print_usage();
                process::exit(1);
            }
        }
        i += 1;
    }

    if mode.is_none() {
        eprintln!("Error: Must specify --all, --turns, or --validate");
        print_usage();
        process::exit(1);
    }

    // Load configuration
    let config = Config::from_file(&config_path).unwrap_or_else(|e| {
        eprintln!("Warning: Could not load config from '{}': {}", config_path, e);
        eprintln!("Using default configuration");
        Config::default_hardcoded()
    });

    println!("Loaded configuration from: {}", config_path);
    println!("Replay log file: {}", log_file);
    println!();

    // Create replay engine
    let engine = ReplayEngine::new(config, verbose);

    // Load log file
    let entries = match engine.load_log_file(log_file) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Error loading log file: {}", e);
            process::exit(1);
        }
    };

    if entries.is_empty() {
        eprintln!("Error: Log file is empty");
        process::exit(1);
    }

    println!("Loaded {} log entries\n", entries.len());

    // Execute based on mode
    match mode.as_deref() {
        Some("all") => {
            println!("Replaying all {} turns...\n", entries.len());
            match engine.replay_all(&entries) {
                Ok(results) => {
                    engine.print_report(&results);
                }
                Err(e) => {
                    eprintln!("Error during replay: {}", e);
                    process::exit(1);
                }
            }
        }
        Some("turns") => {
            let turn_arg = &args[args.iter().position(|a| a == "--turns").unwrap() + 1];
            let turns = match parse_turns(turn_arg) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error parsing turns: {}", e);
                    process::exit(1);
                }
            };

            println!("Replaying {} specific turn(s)...\n", turns.len());
            match engine.replay_turns(&entries, &turns) {
                Ok(results) => {
                    engine.print_report(&results);
                }
                Err(e) => {
                    eprintln!("Error during replay: {}", e);
                    process::exit(1);
                }
            }
        }
        Some("validate") => {
            let validate_arg = &args[args.iter().position(|a| a == "--validate").unwrap() + 1];
            let expected_moves = match parse_expected_moves(validate_arg) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error parsing expected moves: {}", e);
                    process::exit(1);
                }
            };

            println!(
                "Validating {} expected move(s)...\n",
                expected_moves.len()
            );
            match engine.validate_expected_moves(&entries, &expected_moves) {
                Ok(()) => {
                    println!("✓ All expected moves validated successfully!");
                }
                Err(e) => {
                    eprintln!("✗ Validation failed: {}", e);
                    process::exit(1);
                }
            }
        }
        _ => unreachable!(),
    }
}
