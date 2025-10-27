# Battlesnake Rust Starter Project

## Overview
This is a Battlesnake AI server written in Rust using the Rocket web framework. Battlesnake is a competitive programming game where you build an AI to control a snake in a multiplayer arena.

## Current State
- **Status**: Fully configured and running on Replit
- **Port**: 5000 (configured for Replit environment)
- **Language**: Rust (stable)
- **Framework**: Rocket 0.5.0
- **Deployment**: Configured for VM deployment (always running)

## Project Structure
- `src/main.rs` - HTTP endpoint handlers for Battlesnake API
- `src/logic.rs` - Snake AI logic and behavior
- `Cargo.toml` - Rust dependencies
- `Rocket.toml` - Web server configuration

## How It Works
The server implements the Battlesnake API with four main endpoints:
- `GET /` - Returns snake appearance and metadata
- `POST /start` - Called when a game starts
- `POST /move` - Called each turn to decide the snake's next move
- `POST /end` - Called when a game ends

## Recent Changes (Oct 27, 2025)
- Configured Rocket server to use port 5000 for Replit compatibility
- Set up workflow to run `cargo run` on startup
- Configured VM deployment with release build (`cargo run --release`)
- Project is ready to use on play.battlesnake.com

## Development
The snake currently uses basic logic:
- Avoids moving backwards into its own neck
- Chooses random moves from available safe directions
- TODO: Add collision detection, food seeking, and opponent avoidance

## Deployment
The project is configured to deploy as a VM (always running) which is appropriate for a game API that needs to respond to external requests from the Battlesnake platform.

## Playing
To use this Battlesnake:
1. Deploy the project to get a public URL
2. Register the URL at play.battlesnake.com
3. Join games and watch your snake compete!
