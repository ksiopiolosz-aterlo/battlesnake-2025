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
use crate::types::{Battlesnake, Board, Coord, Direction, Game};

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
            "author": "ksiopiolosz-aterlo",
            "color": "#00DEAD",
            "head": "default",
            "tail": "default",
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
        let safe_moves: Vec<Direction> = self
            .get_safe_moves(board, you)
            .into_iter()
            .filter(|&(_, v)| v)
            .map(|(k, _)| k)
            .collect();

        // Choose a random move from the safe ones
        // If no safe moves exist, pick any move (we're likely going to die anyway)
        let chosen = if safe_moves.is_empty() {
            info!("WARNING: No safe moves available, choosing default move");
            Direction::Up
        } else {
            *safe_moves.choose(&mut rand::rng()).unwrap()
        };

        // TODO: Step 4 - Move towards food instead of random, to regain health and survive longer
        // let food = &board.food;

        info!("MOVE {}: {}", turn, chosen.as_str());
        json!({ "move": chosen.as_str() })
    }

    /// Computes a HashMap of moves marking each as safe or unsafe
    ///
    /// # Arguments
    /// * `board` - Current board state
    /// * `you` - Your snake's current state
    ///
    /// # Returns
    /// * `HashMap<Direction, bool>` - Map of all directions with safety status
    fn get_safe_moves(&self, board: &Board, you: &Battlesnake) -> HashMap<Direction, bool> {
        // Initialize all moves as safe
        let mut is_move_safe: HashMap<Direction, bool> =
            Direction::all().iter().map(|&dir| (dir, true)).collect();

        let my_head = &you.body[0]; // Coordinates of your head
        let my_body = &you.body;
        let body_tail_offset = self.config.move_generation.body_tail_offset;

        // Prevent moving backwards into neck
        Self::avoid_neck_moves(
            &mut is_move_safe,
            my_head,
            my_body,
            self.config.move_generation.snake_min_body_length_for_neck,
        );

        // Step 1: Prevent moving out of bounds
        Self::avoid_boundary_collisions(&mut is_move_safe, my_head, board.width, board.height);

        // Step 2: Prevent colliding with own body
        Self::avoid_self_collisions(&mut is_move_safe, my_head, my_body, body_tail_offset);

        // Step 3: Prevent colliding with other snakes
        let opponents: Vec<Battlesnake> = board
            .snakes
            .iter()
            .filter(|snake| snake.id != you.id)
            .cloned()
            .collect();

        Self::avoid_opponent_collisions(&mut is_move_safe, my_head, &opponents, body_tail_offset);

        is_move_safe
    }

    /// Marks moves that would cause the snake to move backwards into its own neck as unsafe
    fn avoid_neck_moves(
        is_move_safe: &mut HashMap<Direction, bool>,
        my_head: &Coord,
        my_body: &[Coord],
        min_body_length: usize,
    ) {
        // Only check neck if snake has more than minimum body length
        if my_body.len() <= min_body_length {
            return;
        }

        let my_neck = &my_body[1]; // Coordinates of the "neck"

        if my_neck.x < my_head.x {
            // Neck is left of head, don't move left
            is_move_safe.insert(Direction::Left, false);
        } else if my_neck.x > my_head.x {
            // Neck is right of head, don't move right
            is_move_safe.insert(Direction::Right, false);
        } else if my_neck.y < my_head.y {
            // Neck is below head, don't move down
            is_move_safe.insert(Direction::Down, false);
        } else if my_neck.y > my_head.y {
            // Neck is above head, don't move up
            is_move_safe.insert(Direction::Up, false);
        }
    }

    /// Marks moves that would cause the snake to move out of bounds as unsafe
    fn avoid_boundary_collisions(
        is_move_safe: &mut HashMap<Direction, bool>,
        my_head: &Coord,
        board_width: i32,
        board_height: u32,
    ) {
        for direction in Direction::all() {
            let next_pos = direction.apply(my_head);
            if Self::is_out_of_bounds(&next_pos, board_width, board_height) {
                is_move_safe.insert(direction, false);
            }
        }
    }

    /// Marks moves that would cause the snake to collide with itself as unsafe
    fn avoid_self_collisions(
        is_move_safe: &mut HashMap<Direction, bool>,
        my_head: &Coord,
        my_body: &[Coord],
        body_tail_offset: usize,
    ) {
        for direction in Direction::all() {
            let next_pos = direction.apply(my_head);
            if Self::is_self_collision(&next_pos, my_body, body_tail_offset) {
                is_move_safe.insert(direction, false);
            }
        }
    }

    /// Marks moves that would cause the snake to collide with opponents as unsafe
    fn avoid_opponent_collisions(
        is_move_safe: &mut HashMap<Direction, bool>,
        my_head: &Coord,
        opponents: &[Battlesnake],
        body_tail_offset: usize,
    ) {
        for direction in Direction::all() {
            let next_pos = direction.apply(my_head);
            if Self::is_opponent_collision(&next_pos, opponents, body_tail_offset) {
                is_move_safe.insert(direction, false);
            }
        }
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
