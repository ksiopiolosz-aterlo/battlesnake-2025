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
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::types::{Battlesnake, Board, Coord, Direction, Game};

/// Execution strategy based on game state and hardware
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionStrategy {
    /// Sequential execution for single-core or simple cases
    Sequential,
    /// Parallel 1v1 using alpha-beta pruning
    Parallel1v1,
    /// Parallel multiplayer using MaxN
    ParallelMultiplayer,
}

/// Adaptive time estimation tracking empirical iteration times
/// Uses exponential moving average to blend observed times with model predictions
#[derive(Debug, Clone)]
struct AdaptiveTimeEstimator {
    /// Observed average time per depth level (index = depth, value = average time in ms)
    depth_timings: Vec<f64>,
    /// Number of observations per depth level for calculating running average
    depth_observations: Vec<u32>,
    /// Blending factor for combining empirical data with model predictions
    /// 0.0 = pure empirical (100% observed data), 1.0 = pure model (100% formula)
    model_weight: f64,
    /// Fallback configuration for exponential model
    base_time_ms: f64,
    branching_factor: f64,
}

impl AdaptiveTimeEstimator {
    /// Creates a new adaptive estimator with configuration parameters
    fn new(base_time_ms: f64, branching_factor: f64, model_weight: f64) -> Self {
        Self {
            depth_timings: Vec::new(),
            depth_observations: Vec::new(),
            model_weight: model_weight.clamp(0.0, 1.0),
            base_time_ms,
            branching_factor,
        }
    }

    /// Records an observed iteration time at a specific depth
    fn record_observation(&mut self, depth: u8, elapsed_ms: f64) {
        let depth_idx = depth as usize;

        // Expand vectors if needed
        while self.depth_timings.len() <= depth_idx {
            self.depth_timings.push(0.0);
            self.depth_observations.push(0);
        }

        // Update running average using incremental mean formula
        let n = self.depth_observations[depth_idx] as f64;
        let old_avg = self.depth_timings[depth_idx];
        let new_avg = (old_avg * n + elapsed_ms) / (n + 1.0);

        self.depth_timings[depth_idx] = new_avg;
        self.depth_observations[depth_idx] += 1;
    }

    /// Estimates time for an iteration at a given depth
    /// Blends empirical observations with exponential model
    fn estimate(&self, depth: u8, num_snakes: usize) -> u64 {
        let depth_idx = depth as usize;

        // Calculate model prediction (exponential branching)
        let exponent = (depth as f64) * (num_snakes as f64);
        let model_estimate = self.base_time_ms * self.branching_factor.powf(exponent);

        // If we have observations for this exact depth, blend with empirical data
        if depth_idx < self.depth_timings.len() && self.depth_observations[depth_idx] > 0 {
            let empirical_estimate = self.depth_timings[depth_idx];
            let blended = self.model_weight * model_estimate
                + (1.0 - self.model_weight) * empirical_estimate;
            return blended.ceil() as u64;
        }

        // If we have observations for earlier depths, extrapolate using ratio
        if let Some(last_observed_depth) = self.find_last_observed_depth(depth) {
            let observed_time = self.depth_timings[last_observed_depth];

            // Calculate expected ratio between depths using model
            let depth_gap = depth - last_observed_depth as u8;
            let exponent_gap = (depth_gap as f64) * (num_snakes as f64);
            let ratio = self.branching_factor.powf(exponent_gap);

            let extrapolated = observed_time * ratio;

            // Blend extrapolation with pure model
            let blended =
                self.model_weight * model_estimate + (1.0 - self.model_weight) * extrapolated;
            return blended.ceil() as u64;
        }

        // No observations yet - fall back to pure model
        model_estimate.ceil() as u64
    }

    /// Finds the highest depth we have observations for, up to the given depth
    fn find_last_observed_depth(&self, max_depth: u8) -> Option<usize> {
        for depth in (0..=max_depth as usize).rev() {
            if depth < self.depth_observations.len() && self.depth_observations[depth] > 0 {
                return Some(depth);
            }
        }
        None
    }
}

/// Lock-free shared state for communication between async poller and computation engine
#[derive(Debug)]
struct SharedSearchState {
    /// Best move found so far (encoded as direction index)
    best_move: Arc<AtomicU8>,
    /// Best score for our snake
    best_score: Arc<AtomicI32>,
    /// Flag indicating search completion
    search_complete: Arc<AtomicBool>,
    /// Current search depth being explored
    current_depth: Arc<AtomicU8>,
}

impl SharedSearchState {
    /// Creates a new shared state with default initial values
    fn new() -> Self {
        SharedSearchState {
            best_move: Arc::new(AtomicU8::new(0)), // Default to Up
            best_score: Arc::new(AtomicI32::new(i32::MIN)),
            search_complete: Arc::new(AtomicBool::new(false)),
            current_depth: Arc::new(AtomicU8::new(0)),
        }
    }
}

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

    /// Computes and returns the next move using MaxN search with iterative deepening
    /// Corresponds to POST /move endpoint
    ///
    /// This method orchestrates the async polling and CPU-bound search computation:
    /// 1. Spawns background search on rayon thread pool
    /// 2. Polls for results with timeout management
    /// 3. Returns best move found within time budget (anytime property)
    ///
    /// # Arguments
    /// * `_game` - Current game metadata
    /// * `turn` - Current turn number
    /// * `board` - Current board state
    /// * `you` - Your snake's current state
    ///
    /// # Returns
    /// * `Value` - JSON response containing the chosen move direction
    pub async fn get_move(
        &self,
        _game: &Game,
        turn: &i32,
        board: &Board,
        you: &Battlesnake,
    ) -> Value {
        let start_time = Instant::now();

        info!("Turn {}: Computing move", turn);

        // Create shared state for lock-free communication between poller and search
        let shared = Arc::new(SharedSearchState::new());
        let shared_clone = shared.clone();

        // Clone data needed for the blocking task
        let board = board.clone();
        let you = you.clone();
        let config = self.config.clone();

        // Spawn CPU-bound computation on rayon thread pool
        tokio::task::spawn_blocking(move || {
            Bot::compute_best_move_internal(&board, &you, shared_clone, start_time, &config)
        });

        // Polling loop: check for results or timeout
        let effective_budget = self.config.timing.effective_budget_ms();
        let polling_interval = Duration::from_millis(self.config.timing.polling_interval_ms);

        loop {
            tokio::time::sleep(polling_interval).await;

            let elapsed = start_time.elapsed().as_millis() as u64;

            // Check if we've exceeded our time budget or search is complete
            if elapsed >= effective_budget || shared.search_complete.load(Ordering::Acquire) {
                break;
            }
        }

        // Extract results from shared state
        let chosen_move =
            Self::index_to_direction(shared.best_move.load(Ordering::Acquire), &self.config);
        let final_score = shared.best_score.load(Ordering::Acquire);
        let final_depth = shared.current_depth.load(Ordering::Acquire);

        info!(
            "Turn {}: Chose {} (score: {}, depth: {}, time: {}ms)",
            turn,
            chosen_move.as_str(),
            final_score,
            final_depth,
            start_time.elapsed().as_millis()
        );

        json!({ "move": chosen_move.as_str() })
    }

    /// Internal computation engine - runs on rayon thread pool
    /// Performs iterative deepening MaxN search with time management
    fn compute_best_move_internal(
        board: &Board,
        you: &Battlesnake,
        shared: Arc<SharedSearchState>,
        start_time: Instant,
        config: &Config,
    ) {
        info!("Starting MaxN search computation");

        // Determine execution strategy
        let num_alive_snakes = board.snakes.iter().filter(|s| s.health > 0).count();
        let num_cpus = rayon::current_num_threads();

        let strategy = Self::determine_strategy(num_alive_snakes, num_cpus, config);
        info!(
            "Selected strategy: {:?} (snakes={}, cpus={})",
            strategy, num_alive_snakes, num_cpus
        );

        // Initialize adaptive time estimator
        let mut time_estimator = AdaptiveTimeEstimator::new(
            config.time_estimation.base_iteration_time_ms,
            config.time_estimation.branching_factor,
            config.time_estimation.model_weight,
        );

        // Iterative deepening loop
        let mut current_depth = config.timing.initial_depth;
        let effective_budget = config.timing.effective_budget_ms();

        loop {
            let elapsed = start_time.elapsed().as_millis() as u64;
            let remaining = effective_budget.saturating_sub(elapsed);

            // Check if we have enough time for another iteration
            if remaining < config.timing.min_time_remaining_ms {
                info!(
                    "Stopping search: insufficient time remaining ({}ms)",
                    remaining
                );
                break;
            }

            // Estimate time for next iteration using adaptive estimator
            let estimated_time = time_estimator.estimate(current_depth, num_alive_snakes);
            if estimated_time > remaining {
                info!("Stopping search: next iteration would exceed budget (estimated {}ms, remaining {}ms)",
                      estimated_time, remaining);
                break;
            }

            // Safety cap on depth
            if current_depth > config.timing.max_search_depth {
                info!("Stopping search: reached max depth ({})", current_depth);
                break;
            }

            info!(
                "Starting iteration at depth {} (estimated time: {}ms)",
                current_depth, estimated_time
            );
            shared.current_depth.store(current_depth, Ordering::Release);

            // Record iteration start time
            let iteration_start = Instant::now();

            // Execute search based on strategy
            match strategy {
                ExecutionStrategy::Parallel1v1 => {
                    info!(
                        "TODO: parallel_1v1_search not yet implemented, using sequential fallback"
                    );
                    Self::sequential_search(board, you, current_depth, &shared, config);
                }
                ExecutionStrategy::ParallelMultiplayer => {
                    info!("TODO: parallel_multiplayer_search not yet implemented, using sequential fallback");
                    Self::sequential_search(board, you, current_depth, &shared, config);
                }
                ExecutionStrategy::Sequential => {
                    Self::sequential_search(board, you, current_depth, &shared, config);
                }
            }

            // Record actual iteration time
            let iteration_elapsed = iteration_start.elapsed().as_millis() as f64;
            time_estimator.record_observation(current_depth, iteration_elapsed);

            info!(
                "Completed depth {} in {:.2}ms (estimated: {}ms)",
                current_depth, iteration_elapsed, estimated_time
            );

            current_depth += 1;
        }

        shared.search_complete.store(true, Ordering::Release);
        info!(
            "Search complete. Best move: {:?}, Score: {}",
            Self::index_to_direction(shared.best_move.load(Ordering::Acquire), config).as_str(),
            shared.best_score.load(Ordering::Acquire)
        );
    }

    /// Determines the execution strategy based on game state and hardware
    fn determine_strategy(
        num_snakes: usize,
        num_cpus: usize,
        config: &Config,
    ) -> ExecutionStrategy {
        match (num_snakes, num_cpus) {
            (n, cpus)
                if n == config.strategy.min_snakes_for_1v1
                    && cpus >= config.strategy.min_cpus_for_parallel =>
            {
                ExecutionStrategy::Parallel1v1
            }
            (_, cpus) if cpus >= config.strategy.min_cpus_for_parallel => {
                ExecutionStrategy::ParallelMultiplayer
            }
            _ => ExecutionStrategy::Sequential,
        }
    }

    /// Sequential search implementation (works on any hardware)
    fn sequential_search(
        board: &Board,
        you: &Battlesnake,
        _depth: u8,
        shared: &Arc<SharedSearchState>,
        config: &Config,
    ) {
        // Generate legal moves for our snake
        let legal_moves = Self::generate_legal_moves(board, you, config);

        if legal_moves.is_empty() {
            info!("No legal moves available");
            shared.best_move.store(
                config.direction_encoding.direction_up_index,
                Ordering::Release,
            );
            shared.best_score.store(i32::MIN, Ordering::Release);
            return;
        }

        info!("Evaluating {} legal moves", legal_moves.len());

        // For now, use simple heuristic: move towards closest food
        // TODO: Replace with actual MaxN search
        let chosen_move = Self::choose_move_towards_food(board, you, &legal_moves);

        let move_idx = Self::direction_to_index(chosen_move, config);
        let score = 1000; // Placeholder score

        shared.best_move.store(move_idx, Ordering::Release);
        shared.best_score.store(score, Ordering::Release);
    }

    /// Generates all legal moves for a snake
    /// A move is legal if it:
    /// - Doesn't go out of bounds
    /// - Doesn't collide with snake bodies (excluding tails which will move)
    /// - Doesn't reverse into the neck
    fn generate_legal_moves(board: &Board, snake: &Battlesnake, config: &Config) -> Vec<Direction> {
        if snake.health <= 0 || snake.body.is_empty() {
            return vec![];
        }

        let head = snake.body[0];
        let neck = if snake.body.len() > config.move_generation.snake_min_body_length_for_neck {
            Some(snake.body[1])
        } else {
            None
        };

        Direction::all()
            .iter()
            .filter(|&&dir| {
                let next = dir.apply(&head);

                // Can't reverse onto neck
                if let Some(n) = neck {
                    if next == n {
                        return false;
                    }
                }

                // Must stay in bounds
                if Self::is_out_of_bounds(&next, board.width, board.height) {
                    return false;
                }

                // Can't collide with bodies (excluding tails which will move)
                if Self::is_collision(&next, board, config.move_generation.body_tail_offset) {
                    return false;
                }

                true
            })
            .copied()
            .collect()
    }

    /// Checks if a coordinate is out of bounds
    fn is_out_of_bounds(coord: &Coord, board_width: i32, board_height: u32) -> bool {
        coord.x < 0 || coord.x >= board_width || coord.y < 0 || coord.y >= board_height as i32
    }

    /// Checks if a coordinate collides with any snake body
    fn is_collision(coord: &Coord, board: &Board, body_tail_offset: usize) -> bool {
        for snake in &board.snakes {
            if snake.health <= 0 {
                continue;
            }

            let body_check_len = snake.body.len().saturating_sub(body_tail_offset);
            if snake.body[..body_check_len].contains(coord) {
                return true;
            }
        }
        false
    }

    /// Simple heuristic: choose move that gets us closer to nearest food
    /// This is a placeholder until full MaxN search is implemented
    fn choose_move_towards_food(
        board: &Board,
        you: &Battlesnake,
        legal_moves: &[Direction],
    ) -> Direction {
        if board.food.is_empty() || legal_moves.is_empty() {
            return legal_moves[0];
        }

        let head = you.body[0];

        // Find closest food
        let closest_food = board
            .food
            .iter()
            .min_by_key(|&&food| Self::manhattan_distance(head, food))
            .copied()
            .unwrap();

        // Choose move that minimizes distance to closest food
        legal_moves
            .iter()
            .min_by_key(|&&dir| {
                let next = dir.apply(&head);
                Self::manhattan_distance(next, closest_food)
            })
            .copied()
            .unwrap_or(legal_moves[0])
    }

    /// Calculates Manhattan distance between two coordinates
    fn manhattan_distance(a: Coord, b: Coord) -> i32 {
        (a.x - b.x).abs() + (a.y - b.y).abs()
    }

    /// Converts a direction to its encoded index
    fn direction_to_index(dir: Direction, config: &Config) -> u8 {
        match dir {
            Direction::Up => config.direction_encoding.direction_up_index,
            Direction::Down => config.direction_encoding.direction_down_index,
            Direction::Left => config.direction_encoding.direction_left_index,
            Direction::Right => config.direction_encoding.direction_right_index,
        }
    }

    /// Converts an encoded index to a direction
    fn index_to_direction(idx: u8, config: &Config) -> Direction {
        if idx == config.direction_encoding.direction_up_index {
            Direction::Up
        } else if idx == config.direction_encoding.direction_down_index {
            Direction::Down
        } else if idx == config.direction_encoding.direction_left_index {
            Direction::Left
        } else if idx == config.direction_encoding.direction_right_index {
            Direction::Right
        } else {
            Direction::Up // Default fallback
        }
    }
}
