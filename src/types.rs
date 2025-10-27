// Battlesnake API Types
// See https://docs.battlesnake.com/api

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Game metadata including ID, ruleset, and timeout
#[derive(Deserialize, Serialize, Debug)]
pub struct Game {
    pub id: String,
    pub ruleset: HashMap<String, Value>,
    pub timeout: u32,
}

/// Board state including dimensions, food, snakes, and hazards
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Board {
    pub height: u32,
    pub width: i32,
    pub food: Vec<Coord>,
    pub snakes: Vec<Battlesnake>,
    pub hazards: Vec<Coord>,
}

/// Snake representation with all state information
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Battlesnake {
    pub id: String,
    pub name: String,
    pub health: i32,
    pub body: Vec<Coord>,
    pub head: Coord,
    pub length: i32,
    pub latency: String,
    pub shout: Option<String>,
}

/// 2D coordinate on the board
#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
}

/// Represents the four possible movement directions for a Battlesnake
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// Returns all possible directions
    pub fn all() -> [Direction; 4] {
        [Direction::Up, Direction::Down, Direction::Left, Direction::Right]
    }

    /// Converts direction to string representation for API response
    pub fn as_str(&self) -> &'static str {
        match self {
            Direction::Up => "up",
            Direction::Down => "down",
            Direction::Left => "left",
            Direction::Right => "right",
        }
    }

    /// Calculates the next coordinate when moving in this direction
    pub fn apply(&self, coord: &Coord) -> Coord {
        match self {
            Direction::Up => Coord { x: coord.x, y: coord.y + 1 },
            Direction::Down => Coord { x: coord.x, y: coord.y - 1 },
            Direction::Left => Coord { x: coord.x - 1, y: coord.y },
            Direction::Right => Coord { x: coord.x + 1, y: coord.y },
        }
    }
}

/// Complete game state received from the API
#[derive(Deserialize, Serialize, Debug)]
pub struct GameState {
    pub game: Game,
    pub turn: i32,
    pub board: Board,
    pub you: Battlesnake,
}
