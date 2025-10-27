// HTTP handler bindings for Battlesnake API endpoints
//
// This module provides thin wrapper functions that bind Rocket HTTP routes
// to the Bot's core logic methods. Handlers are responsible for:
// - Deserializing incoming JSON requests
// - Extracting Bot instance from Rocket's managed state
// - Delegating to Bot methods
// - Serializing responses

use rocket::http::Status;
use rocket::serde::json::Json;
use serde_json::Value;

use crate::bot::Bot;
use crate::types::GameState;

/// GET / endpoint
/// Returns bot metadata and appearance configuration
#[get("/")]
pub fn index(bot: &rocket::State<Bot>) -> Json<Value> {
    Json(bot.info())
}

/// POST /start endpoint
/// Called when a game starts - allows initialization logic
#[post("/start", format = "json", data = "<start_req>")]
pub fn start(bot: &rocket::State<Bot>, start_req: Json<GameState>) -> Status {
    bot.start(
        &start_req.game,
        &start_req.turn,
        &start_req.board,
        &start_req.you,
    );

    Status::Ok
}

/// POST /move endpoint
/// Called each turn to compute and return the next move
#[post("/move", format = "json", data = "<move_req>")]
pub async fn get_move(bot: &rocket::State<Bot>, move_req: Json<GameState>) -> Json<Value> {
    let response = bot.get_move(
        &move_req.game,
        &move_req.turn,
        &move_req.board,
        &move_req.you,
    ).await;

    Json(response)
}

/// POST /end endpoint
/// Called when a game ends - allows cleanup and logging
#[post("/end", format = "json", data = "<end_req>")]
pub fn end(bot: &rocket::State<Bot>, end_req: Json<GameState>) -> Status {
    bot.end(&end_req.game, &end_req.turn, &end_req.board, &end_req.you);

    Status::Ok
}
