#[macro_use]
extern crate rocket;

use log::info;
use rocket::fairing::AdHoc;
use std::env;

mod bot;
mod config;
mod debug_logger;
mod handler;
mod replay;
mod simple_profiler;
mod types;

#[launch]
fn rocket() -> _ {
    // Lots of web hosting services expect you to bind to the port specified by the `PORT`
    // environment variable. However, Rocket looks at the `ROCKET_PORT` environment variable.
    // If we find a value for `PORT`, we set `ROCKET_PORT` to that value.
    if let Ok(port) = env::var("PORT") {
        env::set_var("ROCKET_PORT", &port);
    }

    // We default to 'info' level logging. But if the `RUST_LOG` environment variable is set,
    // we keep that value instead.
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    env_logger::init();

    info!("Starting Battlesnake Server...");

    // Load configuration once at startup
    let config = config::Config::load_or_default();
    let bot = bot::Bot::new(config);

    rocket::build()
        .manage(bot)
        .attach(AdHoc::on_response("Server ID Middleware", |_, res| {
            Box::pin(async move {
                res.set_raw_header("Server", "battlesnake/github/starter-snake-rust");
            })
        }))
        .mount(
            "/",
            routes![handler::index, handler::start, handler::get_move, handler::end],
        )
}
