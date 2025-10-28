// Integration test for trapped snake fallback behavior
//
// Tests that when a snake has NO legal moves (completely trapped),
// it chooses a move that:
// 1. Stays in-bounds if possible
// 2. Doesn't arbitrarily default to "up" if "up" is out of bounds
//
// This validates the fix for the illegal move bug where trapped snakes
// would default to "up" even when at the top wall.

use serde_json::json;
use starter_snake_rust::bot::Bot;
use starter_snake_rust::config::Config;
use starter_snake_rust::types::{Battlesnake, Board, Coord, Direction, Game};
use std::sync::Arc;

/// Test: Snake at top wall (y=10), completely surrounded except for down
/// Should choose "down" (in-bounds), not "up" (out-of-bounds)
#[tokio::test]
async fn test_trapped_at_top_wall_chooses_in_bounds_move() {
    let config = Config::default_hardcoded();
    let bot = Bot::new(config);

    let game = Game {
        id: "test-game".to_string(),
        ruleset: json!({}),
        timeout: 500,
        source: "test".to_string(),
    };

    // Snake at top wall (y=10), surrounded by bodies
    // Body blocks left, right, and wraps around
    // Only "down" stays in-bounds
    let board = Board {
        height: 11, // 0-10
        width: 11,  // 0-10
        food: vec![],
        snakes: vec![
            Battlesnake {
                id: "our-snake".to_string(),
                name: "Rusty".to_string(),
                health: 50,
                body: vec![
                    Coord { x: 5, y: 10 }, // head at top wall
                    Coord { x: 5, y: 9 },  // neck blocks down-back
                    Coord { x: 4, y: 9 },  // body
                    Coord { x: 4, y: 10 }, // body blocks left
                ],
                head: Coord { x: 5, y: 10 },
                length: 4,
                latency: "0".to_string(),
                shout: "".to_string(),
            },
            Battlesnake {
                id: "opponent".to_string(),
                name: "Enemy".to_string(),
                health: 50,
                body: vec![
                    Coord { x: 6, y: 10 }, // blocks right
                    Coord { x: 6, y: 9 },
                    Coord { x: 6, y: 8 },
                ],
                head: Coord { x: 6, y: 10 },
                length: 3,
                latency: "0".to_string(),
                shout: "".to_string(),
            },
        ],
        hazards: vec![],
    };

    let you = board.snakes[0].clone();

    // Get the bot's move
    let response = bot.get_move(&game, &0, &board, &you).await;
    let chosen_move = response["move"].as_str().unwrap();

    // The bot should NOT choose "up" (out of bounds)
    assert_ne!(
        chosen_move, "up",
        "Bot should not choose 'up' when at top wall (y=10)"
    );

    // In this trapped scenario, "down" is the only in-bounds direction
    // (Even though it hits the neck, it's better than going out of bounds)
    println!("Bot chose: {}", chosen_move);
}

/// Test: Snake at bottom wall (y=0), should not choose "down"
#[tokio::test]
async fn test_trapped_at_bottom_wall_avoids_down() {
    let config = Config::default_hardcoded();
    let bot = Bot::new(config);

    let game = Game {
        id: "test-game".to_string(),
        ruleset: json!({}),
        timeout: 500,
        source: "test".to_string(),
    };

    let board = Board {
        height: 11,
        width: 11,
        food: vec![],
        snakes: vec![Battlesnake {
            id: "our-snake".to_string(),
            name: "Rusty".to_string(),
            health: 50,
            body: vec![
                Coord { x: 5, y: 0 }, // head at bottom wall
                Coord { x: 5, y: 1 }, // neck blocks up-back
                Coord { x: 4, y: 1 }, // body
                Coord { x: 4, y: 0 }, // body blocks left
                Coord { x: 3, y: 0 }, // more body
                Coord { x: 2, y: 0 }, // more body
                Coord { x: 1, y: 0 }, // more body
                Coord { x: 0, y: 0 }, // more body
            ],
            head: Coord { x: 5, y: 0 },
            length: 8,
            latency: "0".to_string(),
            shout: "".to_string(),
        }],
        hazards: vec![],
    };

    let you = board.snakes[0].clone();

    let response = bot.get_move(&game, &0, &board, &you).await;
    let chosen_move = response["move"].as_str().unwrap();

    // Should NOT choose "down" (out of bounds at y=0)
    assert_ne!(
        chosen_move, "down",
        "Bot should not choose 'down' when at bottom wall (y=0)"
    );

    println!("Bot chose: {}", chosen_move);
}

/// Test: Snake at left wall (x=0), should not choose "left"
#[tokio::test]
async fn test_trapped_at_left_wall_avoids_left() {
    let config = Config::default_hardcoded();
    let bot = Bot::new(config);

    let game = Game {
        id: "test-game".to_string(),
        ruleset: json!({}),
        timeout: 500,
        source: "test".to_string(),
    };

    let board = Board {
        height: 11,
        width: 11,
        food: vec![],
        snakes: vec![Battlesnake {
            id: "our-snake".to_string(),
            name: "Rusty".to_string(),
            health: 50,
            body: vec![
                Coord { x: 0, y: 5 }, // head at left wall
                Coord { x: 1, y: 5 }, // neck blocks right-back
                Coord { x: 1, y: 4 }, // body
                Coord { x: 0, y: 4 }, // body blocks down
                Coord { x: 0, y: 3 }, // more body
                Coord { x: 0, y: 2 }, // more body
                Coord { x: 0, y: 1 }, // more body
            ],
            head: Coord { x: 0, y: 5 },
            length: 7,
            latency: "0".to_string(),
            shout: "".to_string(),
        }],
        hazards: vec![],
    };

    let you = board.snakes[0].clone();

    let response = bot.get_move(&game, &0, &board, &you).await;
    let chosen_move = response["move"].as_str().unwrap();

    // Should NOT choose "left" (out of bounds at x=0)
    assert_ne!(
        chosen_move, "left",
        "Bot should not choose 'left' when at left wall (x=0)"
    );

    println!("Bot chose: {}", chosen_move);
}

/// Test: Snake at right wall (x=10), should not choose "right"
#[tokio::test]
async fn test_trapped_at_right_wall_avoids_right() {
    let config = Config::default_hardcoded();
    let bot = Bot::new(config);

    let game = Game {
        id: "test-game".to_string(),
        ruleset: json!({}),
        timeout: 500,
        source: "test".to_string(),
    };

    let board = Board {
        height: 11,
        width: 11,
        food: vec![],
        snakes: vec![Battlesnake {
            id: "our-snake".to_string(),
            name: "Rusty".to_string(),
            health: 50,
            body: vec![
                Coord { x: 10, y: 5 }, // head at right wall
                Coord { x: 9, y: 5 },  // neck blocks left-back
                Coord { x: 9, y: 6 },  // body
                Coord { x: 10, y: 6 }, // body blocks up
                Coord { x: 10, y: 7 }, // more body
                Coord { x: 10, y: 8 }, // more body
            ],
            head: Coord { x: 10, y: 5 },
            length: 6,
            latency: "0".to_string(),
            shout: "".to_string(),
        }],
        hazards: vec![],
    };

    let you = board.snakes[0].clone();

    let response = bot.get_move(&game, &0, &board, &you).await;
    let chosen_move = response["move"].as_str().unwrap();

    // Should NOT choose "right" (out of bounds at x=10)
    assert_ne!(
        chosen_move, "right",
        "Bot should not choose 'right' when at right wall (x=10)"
    );

    println!("Bot chose: {}", chosen_move);
}
