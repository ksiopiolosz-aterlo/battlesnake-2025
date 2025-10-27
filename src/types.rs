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
#[derive(Deserialize, Serialize, Debug)]
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

/// Complete game state received from the API
#[derive(Deserialize, Serialize, Debug)]
pub struct GameState {
    pub game: Game,
    pub turn: i32,
    pub board: Board,
    pub you: Battlesnake,
}
