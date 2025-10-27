// Welcome to
// __________         __    __  .__                               __
// \______   \_____ _/  |__/  |_|  |   ____   ______ ____ _____  |  | __ ____
//  |    |  _/\__  \\   __\   __\  | _/ __ \ /  ___//    \\__  \ |  |/ // __ \
//  |    |   \ / __ \|  |  |  | |  |_\  ___/ \___ \|   |  \/ __ \|    <\  ___/
//  |________/(______/__|  |__| |____/\_____>______>___|__(______/__|__\\_____>
//
// This file can be a nice home for your Battlesnake logic and helper functions.
//
// To get you started we've included code to prevent your Battlesnake from moving backwards.
// For more info see docs.battlesnake.com

use log::info;
use rand::seq::IndexedRandom;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::config::Config;
use crate::types::{Battlesnake, Board, Coord, Game};

/// Battlesnake Bot with OOP-style API
/// Takes static configuration dependencies and exposes methods corresponding to API endpoints
pub struct Bot {
    config: Config,
}

impl Bot {
    /// Creates a new Bot instance with the given configuration
    ///
    /// # Arguments
    /// * `config` - Static configuration that does not change during the bot's lifetime
    pub fn new(config: Config) -> Self {
        Bot { config }
    }

    /// Returns bot metadata and appearance
    /// Corresponds to GET / endpoint
    pub fn info(&self) -> Value {
        info!("INFO");

        json!({
            "apiversion": "1",
            "author": "", // TODO: Your Battlesnake Username
            "color": "#888888", // TODO: Choose color
            "head": "default", // TODO: Choose head
            "tail": "default", // TODO: Choose tail
        })
    }

    /// Called when a game starts
    /// Corresponds to POST /start endpoint
    pub fn start(&self, _game: &Game, _turn: &i32, _board: &Board, _you: &Battlesnake) {
        info!("GAME START");
    }

    /// Called when a game ends
    /// Corresponds to POST /end endpoint
    pub fn end(&self, _game: &Game, _turn: &i32, _board: &Board, _you: &Battlesnake) {
        info!("GAME OVER");
    }

    /// Computes and returns the next move
    /// Corresponds to POST /move endpoint
    ///
    /// # Arguments
    /// * `game` - Current game metadata
    /// * `turn` - Current turn number
    /// * `board` - Current board state
    /// * `you` - Your snake's current state
    ///
    /// # Returns
    /// * `Value` - JSON response containing the chosen move direction
    pub fn get_move(&self, _game: &Game, turn: &i32, board: &Board, you: &Battlesnake) -> Value {
        let mut is_move_safe: HashMap<_, _> = vec![
            ("up", true),
            ("down", true),
            ("left", true),
            ("right", true),
        ]
        .into_iter()
        .collect();

        // We've included code to prevent your Battlesnake from moving backwards
        let my_head = &you.body[0]; // Coordinates of your head

        // Only check neck if snake has more than minimum body length
        if you.body.len() > self.config.move_generation.snake_min_body_length_for_neck {
            let my_neck = &you.body[1]; // Coordinates of your "neck"

            if my_neck.x < my_head.x {
                // Neck is left of head, don't move left
                is_move_safe.insert("left", false);
            } else if my_neck.x > my_head.x {
                // Neck is right of head, don't move right
                is_move_safe.insert("right", false);
            } else if my_neck.y < my_head.y {
                // Neck is below head, don't move down
                is_move_safe.insert("down", false);
            } else if my_neck.y > my_head.y {
                // Neck is above head, don't move up
                is_move_safe.insert("up", false);
            }
        }

        // Step 1: Prevent your Battlesnake from moving out of bounds
        let board_width = board.width;
        let board_height = board.height;

        // Calculate potential next positions for each direction
        let move_up = Coord {
            x: my_head.x,
            y: my_head.y + 1,
        };
        let move_down = Coord {
            x: my_head.x,
            y: my_head.y - 1,
        };
        let move_left = Coord {
            x: my_head.x - 1,
            y: my_head.y,
        };
        let move_right = Coord {
            x: my_head.x + 1,
            y: my_head.y,
        };

        // Check boundary collisions
        if Self::is_out_of_bounds(&move_up, board_width, board_height) {
            is_move_safe.insert("up", false);
        }
        if Self::is_out_of_bounds(&move_down, board_width, board_height) {
            is_move_safe.insert("down", false);
        }
        if Self::is_out_of_bounds(&move_left, board_width, board_height) {
            is_move_safe.insert("left", false);
        }
        if Self::is_out_of_bounds(&move_right, board_width, board_height) {
            is_move_safe.insert("right", false);
        }

        // Step 2: Prevent your Battlesnake from colliding with itself
        let my_body = &you.body;
        let body_tail_offset = self.config.move_generation.body_tail_offset;

        if Self::is_self_collision(&move_up, my_body, body_tail_offset) {
            is_move_safe.insert("up", false);
        }
        if Self::is_self_collision(&move_down, my_body, body_tail_offset) {
            is_move_safe.insert("down", false);
        }
        if Self::is_self_collision(&move_left, my_body, body_tail_offset) {
            is_move_safe.insert("left", false);
        }
        if Self::is_self_collision(&move_right, my_body, body_tail_offset) {
            is_move_safe.insert("right", false);
        }

        // Step 3: Prevent your Battlesnake from colliding with other Battlesnakes
        // Filter to get only opponents (not our own snake)
        let opponents: Vec<Battlesnake> = board
            .snakes
            .iter()
            .filter(|snake| snake.id != you.id)
            .cloned()
            .collect();

        if Self::is_opponent_collision(&move_up, &opponents, body_tail_offset) {
            is_move_safe.insert("up", false);
        }
        if Self::is_opponent_collision(&move_down, &opponents, body_tail_offset) {
            is_move_safe.insert("down", false);
        }
        if Self::is_opponent_collision(&move_left, &opponents, body_tail_offset) {
            is_move_safe.insert("left", false);
        }
        if Self::is_opponent_collision(&move_right, &opponents, body_tail_offset) {
            is_move_safe.insert("right", false);
        }

        // Are there any safe moves left?
        let safe_moves = is_move_safe
            .into_iter()
            .filter(|&(_, v)| v)
            .map(|(k, _)| k)
            .collect::<Vec<_>>();

        // Choose a random move from the safe ones
        let chosen = safe_moves.choose(&mut rand::rng()).unwrap();

        // TODO: Step 4 - Move towards food instead of random, to regain health and survive longer
        // let food = &board.food;

        info!("MOVE {}: {}", turn, chosen);
        json!({ "move": chosen })
    }

    /// Checks if a coordinate is out of bounds
    fn is_out_of_bounds(coord: &Coord, board_width: i32, board_height: u32) -> bool {
        coord.x < 0 || coord.x >= board_width || coord.y < 0 || coord.y >= board_height as i32
    }

    /// Checks if a coordinate collides with the snake's own body
    /// Uses body_tail_offset to exclude the tail (which will move away)
    fn is_self_collision(coord: &Coord, snake_body: &[Coord], body_tail_offset: usize) -> bool {
        // Check against body segments, excluding the tail which will move
        let body_check_len = snake_body.len().saturating_sub(body_tail_offset);
        snake_body[..body_check_len].contains(coord)
    }

    /// Checks if a coordinate collides with any opponent snake's body
    /// Uses body_tail_offset to exclude opponent tails (which will move away)
    fn is_opponent_collision(
        coord: &Coord,
        opponents: &[Battlesnake],
        body_tail_offset: usize,
    ) -> bool {
        for opponent in opponents {
            let body_check_len = opponent.body.len().saturating_sub(body_tail_offset);
            if opponent.body[..body_check_len].contains(coord) {
                return true;
            }
        }
        false
    }
}
