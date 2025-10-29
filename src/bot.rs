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

use log::{info, warn};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::debug_logger::DebugLogger;
use crate::simple_profiler;
use crate::types::{Battlesnake, Board, Coord, Direction, Game};

/// N-tuple score representation for MaxN algorithm
/// Each component represents the utility score for one player
#[derive(Debug, Clone)]
struct ScoreTuple {
    scores: Vec<i32>,
}

impl ScoreTuple {
    /// Creates a new score tuple with specified size and initial value
    fn new_with_value(num_players: usize, initial_value: i32) -> Self {
        ScoreTuple {
            scores: vec![initial_value; num_players],
        }
    }

    /// Gets the score for a specific player
    fn for_player(&self, player_idx: usize) -> i32 {
        self.scores.get(player_idx).copied().unwrap_or(i32::MIN)
    }
}

/// Entry in the transposition table
#[derive(Debug, Clone)]
struct TranspositionEntry {
    /// Evaluation score for this board state
    score: i32,
    /// Depth at which this state was evaluated
    depth: u8,
    /// Age for LRU eviction (generation number)
    age: u32,
}

/// Transposition table for caching board state evaluations
/// Uses Zobrist-style hashing to detect repeated positions
pub struct TranspositionTable {
    /// Hash map storing board_hash -> evaluation
    table: RwLock<HashMap<u64, TranspositionEntry>>,
    /// Maximum number of entries before eviction
    max_size: usize,
    /// Current generation for LRU eviction
    current_age: AtomicU32,
}

impl TranspositionTable {
    /// Creates a new transposition table with specified maximum size
    pub fn new(max_size: usize) -> Self {
        TranspositionTable {
            table: RwLock::new(HashMap::with_capacity(max_size)),
            max_size,
            current_age: AtomicU32::new(0),
        }
    }

    /// Hashes a board state for use as transposition table key
    /// Includes all snake positions, healths, and food positions
    pub fn hash_board(board: &Board) -> u64 {
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();

        // Hash snakes (position and health matter, not ID)
        // Sort by position to ensure consistent hashing regardless of snake order
        let mut snake_positions: Vec<_> = board.snakes.iter()
            .filter(|s| s.health > 0)
            .flat_map(|s| s.body.iter().map(move |coord| (coord.x, coord.y, s.health)))
            .collect();
        snake_positions.sort_unstable();

        for (x, y, health) in snake_positions {
            x.hash(&mut hasher);
            y.hash(&mut hasher);
            health.hash(&mut hasher);
        }

        // Hash food positions
        let mut food_positions: Vec<_> = board.food.iter().map(|c| (c.x, c.y)).collect();
        food_positions.sort_unstable();

        for (x, y) in food_positions {
            x.hash(&mut hasher);
            y.hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Probes the transposition table for a cached evaluation
    /// Returns Some(score) if found and depth is sufficient, None otherwise
    pub fn probe(&self, board_hash: u64, required_depth: u8) -> Option<i32> {
        let table = self.table.read().ok()?;

        if let Some(entry) = table.get(&board_hash) {
            // Only use cached value if it was searched to at least the required depth
            if entry.depth >= required_depth {
                return Some(entry.score);
            }
        }

        None
    }

    /// Stores an evaluation in the transposition table
    /// Performs LRU eviction if table is full
    pub fn store(&self, board_hash: u64, score: i32, depth: u8) {
        let current_age = self.current_age.load(Ordering::Relaxed);

        if let Ok(mut table) = self.table.write() {
            // Evict old entries if table is full
            if table.len() >= self.max_size {
                let age_threshold = current_age.saturating_sub(100);
                table.retain(|_, entry| entry.age > age_threshold);

                // If still too full after age-based eviction, clear half the table
                if table.len() >= self.max_size {
                    let keys_to_remove: Vec<_> = table.keys()
                        .take(self.max_size / 2)
                        .copied()
                        .collect();
                    for key in keys_to_remove {
                        table.remove(&key);
                    }
                }
            }

            // Store or update entry
            match table.get_mut(&board_hash) {
                Some(entry) if entry.depth < depth => {
                    // Update if new depth is deeper
                    entry.score = score;
                    entry.depth = depth;
                    entry.age = current_age;
                }
                None => {
                    // Insert new entry
                    table.insert(board_hash, TranspositionEntry {
                        score,
                        depth,
                        age: current_age,
                    });
                }
                _ => {
                    // Existing entry is deeper, don't update
                }
            }
        }
    }

    /// Increments the age counter (call at start of each search)
    pub fn increment_age(&self) {
        self.current_age.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns statistics about the transposition table
    pub fn stats(&self) -> (usize, usize) {
        if let Ok(table) = self.table.read() {
            (table.len(), self.max_size)
        } else {
            (0, self.max_size)
        }
    }
}

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

/// Lock-free shared state for communication between async poller and computation engine
#[derive(Debug)]
pub struct SharedSearchState {
    /// Packed best move and score (u64: high 32 bits = score as i32, low 8 bits = move, rest unused)
    /// This ensures atomic updates of both values together, preventing race conditions
    pub best_move_and_score: Arc<AtomicU64>,
    /// Flag indicating search completion
    pub search_complete: Arc<AtomicBool>,
    /// Current search depth being explored
    pub current_depth: Arc<AtomicU8>,
}

impl SharedSearchState {
    /// Creates a new shared state with default initial values
    pub fn new() -> Self {
        // Pack initial values: move=0 (Up), score=i32::MIN
        let packed = Self::pack_move_score(0, i32::MIN);
        SharedSearchState {
            best_move_and_score: Arc::new(AtomicU64::new(packed)),
            search_complete: Arc::new(AtomicBool::new(false)),
            current_depth: Arc::new(AtomicU8::new(0)),
        }
    }

    /// Packs move (u8) and score (i32) into a u64
    /// Format: [score: i32 as u32 (bits 32-63)][unused: u24 (bits 8-31)][move: u8 (bits 0-7)]
    #[inline]
    fn pack_move_score(move_idx: u8, score: i32) -> u64 {
        let score_bits = (score as i32) as u32 as u64;
        let move_bits = move_idx as u64;
        (score_bits << 32) | move_bits
    }

    /// Unpacks u64 into (move_idx, score)
    #[inline]
    fn unpack_move_score(packed: u64) -> (u8, i32) {
        let move_idx = (packed & 0xFF) as u8;
        let score = ((packed >> 32) as u32) as i32;
        (move_idx, score)
    }

    /// Atomically updates best move and score if the new score is better
    /// Returns true if update succeeded, false if another thread had a better score
    pub fn try_update_best(&self, move_idx: u8, score: i32) -> bool {
        let new_packed = Self::pack_move_score(move_idx, score);

        loop {
            let current_packed = self.best_move_and_score.load(Ordering::Acquire);
            let (_current_move, current_score) = Self::unpack_move_score(current_packed);

            // Only update if new score is strictly better
            if score <= current_score {
                return false;
            }

            // Try to atomically swap
            match self.best_move_and_score.compare_exchange(
                current_packed,
                new_packed,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(_) => continue, // Another thread updated, retry
            }
        }
    }

    /// Force-set the initial move and score without comparison
    /// ONLY use this during initialization BEFORE search threads start
    /// This prevents race conditions where search updates before initialization completes
    pub fn force_initialize(&self, move_idx: u8, score: i32) {
        let packed = Self::pack_move_score(move_idx, score);
        self.best_move_and_score.store(packed, Ordering::Release);
    }

    /// Gets the current best move and score atomically
    /// Returns (move_idx, score) as a tuple
    pub fn get_best(&self) -> (u8, i32) {
        let packed = self.best_move_and_score.load(Ordering::Acquire);
        Self::unpack_move_score(packed)
    }

}

/// Killer Move Table for move ordering heuristic
/// Tracks moves that caused alpha-beta cutoffs at each depth
/// Used to improve move ordering and increase cutoff rate
pub struct KillerMoveTable {
    /// Two killer moves per depth (standard killer heuristic)
    /// Array size is max_search_depth + 1 (config value: 20 + 1 = 21)
    killers: Vec<Vec<Option<Direction>>>,
}

impl KillerMoveTable {
    /// Creates a new killer move table
    /// Size is determined by config.timing.max_search_depth
    pub fn new(config: &Config) -> Self {
        let max_depth = (config.timing.max_search_depth + 1) as usize;
        let killer_count = config.move_ordering.killer_moves_per_depth;

        KillerMoveTable {
            killers: vec![vec![None; killer_count]; max_depth],
        }
    }

    /// Records a killer move at a specific depth
    /// Shifts existing killer moves down (most recent first)
    pub fn record_killer(&mut self, depth: u8, mv: Direction, config: &Config) {
        if !config.move_ordering.enable_killer_heuristic {
            return;
        }

        let depth_idx = depth as usize;
        if depth_idx >= self.killers.len() {
            return;
        }

        // Check if this move is already a killer at this depth
        if self.killers[depth_idx].iter().any(|k| k == &Some(mv)) {
            return;
        }

        // Shift killers: [0] -> [1], [1] -> [2], etc.
        // Insert new killer at position 0
        self.killers[depth_idx].rotate_right(1);
        self.killers[depth_idx][0] = Some(mv);
    }

    /// Checks if a move is a killer move at a specific depth
    pub fn is_killer(&self, depth: u8, mv: Direction) -> bool {
        let depth_idx = depth as usize;
        if depth_idx >= self.killers.len() {
            return false;
        }
        self.killers[depth_idx].contains(&Some(mv))
    }

    /// Clears all killer moves (called at start of new search iteration)
    pub fn clear(&mut self) {
        for depth_killers in &mut self.killers {
            depth_killers.fill(None);
        }
    }
}

/// History Heuristic Table for move ordering
/// Tracks globally successful moves (not depth-specific like killers)
/// Complements killer heuristic by learning which moves work well across all positions
pub struct HistoryTable {
    /// Scores indexed by [position][direction]
    /// Position is flattened: index = y * width + x
    /// Higher scores = more likely to cause cutoffs
    scores: Vec<[i32; 4]>,  // 4 directions: Up, Down, Left, Right
    width: usize,
    height: usize,
}

impl HistoryTable {
    /// Creates a new history table for the given board dimensions
    pub fn new(width: u32, height: u32) -> Self {
        let width = width as usize;
        let height = height as usize;
        let size = width * height;

        HistoryTable {
            scores: vec![[0; 4]; size],
            width,
            height,
        }
    }

    /// Updates history score for a move
    /// Exponential bonus for cutoffs (2^depth), smaller penalty for non-cutoffs
    pub fn update(&mut self, coord: &Coord, dir: Direction, depth: u8, caused_cutoff: bool) {
        let x = coord.x as usize;
        let y = coord.y as usize;

        if x >= self.width || y >= self.height {
            return;  // Out of bounds
        }

        let pos_idx = y * self.width + x;
        let dir_idx = direction_to_index(dir);

        let bonus = if caused_cutoff {
            // Exponential bonus by depth (deeper cutoffs are more valuable)
            1 << depth.min(10)  // Cap at 2^10 to prevent overflow
        } else {
            // Small penalty for moves that didn't cause cutoffs
            -(1 << (depth / 2).min(5))  // Smaller penalty, also capped
        };

        // Saturating add to prevent overflow
        self.scores[pos_idx][dir_idx] = self.scores[pos_idx][dir_idx].saturating_add(bonus);
    }

    /// Gets the history score for a move
    /// Higher scores indicate moves that historically cause more cutoffs
    pub fn get_score(&self, coord: &Coord, dir: Direction) -> i32 {
        let x = coord.x as usize;
        let y = coord.y as usize;

        if x >= self.width || y >= self.height {
            return 0;  // Out of bounds
        }

        let pos_idx = y * self.width + x;
        let dir_idx = direction_to_index(dir);

        self.scores[pos_idx][dir_idx]
    }

    /// Clears all history scores (called at start of new game or search tree)
    /// Note: Unlike killers, history often persists across iterations
    /// but we clear it per root position for freshness
    pub fn clear(&mut self) {
        for scores in &mut self.scores {
            scores.fill(0);
        }
    }
}

/// Helper function to convert Direction to array index
fn direction_to_index(dir: Direction) -> usize {
    match dir {
        Direction::Up => 0,
        Direction::Down => 1,
        Direction::Left => 2,
        Direction::Right => 3,
    }
}

/// Orders moves for better alpha-beta pruning
/// Priority: PV move > killer moves > history scores > remaining moves
/// This can improve alpha-beta efficiency by 50-80%
fn order_moves(
    moves: Vec<Direction>,
    pv_move: Option<Direction>,
    killers: &KillerMoveTable,
    history: Option<(&HistoryTable, &Coord)>,  // (history_table, current_position)
    depth: u8,
    config: &Config,
) -> Vec<Direction> {
    let mut ordered = Vec::with_capacity(moves.len());

    // Priority 1: PV (Principal Variation) move from previous iteration
    if config.move_ordering.enable_pv_ordering {
        if let Some(pv) = pv_move {
            if moves.contains(&pv) {
                ordered.push(pv);
            }
        }
    }

    // Priority 2: Killer moves
    if config.move_ordering.enable_killer_heuristic {
        for &mv in &moves {
            if !ordered.contains(&mv) && killers.is_killer(depth, mv) {
                ordered.push(mv);
            }
        }
    }

    // Priority 3: History heuristic - sort remaining moves by history score
    if let Some((hist, pos)) = history {
        let mut remaining: Vec<_> = moves.iter()
            .filter(|&&mv| !ordered.contains(&mv))
            .map(|&mv| (mv, hist.get_score(pos, mv)))
            .collect();

        // Sort by history score (descending - higher scores first)
        remaining.sort_by(|a, b| b.1.cmp(&a.1));

        for (mv, _score) in remaining {
            ordered.push(mv);
        }
    } else {
        // Priority 4: Remaining moves (if no history available)
        for &mv in &moves {
            if !ordered.contains(&mv) {
                ordered.push(mv);
            }
        }
    }

    ordered
}

/// Battlesnake Bot with OOP-style API
/// Takes static configuration dependencies and exposes methods corresponding to API endpoints
pub struct Bot {
    config: Config,
    debug_logger: Arc<tokio::sync::Mutex<Option<DebugLogger>>>,
}

impl Bot {
    /// Creates a new Bot instance with the given configuration
    ///
    /// # Arguments
    /// * `config` - Static configuration that does not change during the bot's lifetime
    pub fn new(config: Config) -> Self {
        Bot {
            config,
            debug_logger: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Ensures the debug logger is initialized (lazy initialization)
    /// This is called on the first move to avoid blocking during startup
    async fn ensure_debug_logger_initialized(&self) {
        let mut logger_guard = self.debug_logger.lock().await;
        if logger_guard.is_none() {
            if self.config.debug.enabled {
                *logger_guard = Some(
                    DebugLogger::new(true, &self.config.debug.log_file_path).await
                );
            } else {
                *logger_guard = Some(DebugLogger::disabled());
            }
        }
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

        // Ensure debug logger is initialized (lazy initialization on first call)
        self.ensure_debug_logger_initialized().await;

        // Create shared state for lock-free communication between poller and search
        let shared = Arc::new(SharedSearchState::new());

        // CRITICAL: Initialize shared state with first legal move BEFORE spawning search
        // Use force_initialize() to prevent race condition where search updates before init completes
        // ALSO: Keep legal_moves for later validation (must do this before cloning `you`)
        let legal_moves = Self::generate_legal_moves(board, you, &self.config);
        if !legal_moves.is_empty() {
            let first_legal_move = legal_moves[0];
            shared.force_initialize(
                Self::direction_to_index(first_legal_move, &self.config),
                i32::MIN + 1, // Slightly better than initial i32::MIN
            );
        } else {
            // No legal moves - we're trapped, keep default
            // (will be handled by fallback logic in compute_best_move_internal)
            warn!("No legal moves available at turn {}", turn);
        }

        let shared_clone = shared.clone();

        // Clone data needed for the blocking task
        let board_clone = board.clone();
        let you = you.clone();
        let config = self.config.clone();

        // Spawn CPU-bound computation on rayon thread pool
        tokio::task::spawn_blocking(move || {
            Bot::compute_best_move_internal(&board_clone, &you, shared_clone, start_time, &config)
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
        let (best_move_idx, final_score) = shared.get_best();
        let chosen_move = Self::index_to_direction(best_move_idx, &self.config);
        let final_depth = shared.current_depth.load(Ordering::Acquire);

        // DEFENSIVE: Validate chosen move is actually legal (catches any remaining edge cases)
        let final_move = if legal_moves.contains(&chosen_move) {
            chosen_move
        } else {
            warn!(
                "Turn {}: ILLEGAL MOVE DETECTED! Chose {} but legal moves are {:?}. Falling back to first legal move.",
                turn, chosen_move.as_str(), legal_moves
            );
            legal_moves.first().copied().unwrap_or(Direction::Up)
        };

        info!(
            "Turn {}: Chose {} (score: {}, depth: {}, time: {}ms)",
            turn,
            final_move.as_str(),
            final_score,
            final_depth,
            start_time.elapsed().as_millis()
        );

        // Fire-and-forget debug logging (non-blocking)
        if let Some(logger) = self.debug_logger.lock().await.as_ref() {
            logger.log_move(*turn, board.clone(), final_move);
        }

        json!({ "move": final_move.as_str() })
    }

    /// Internal computation engine - runs on rayon thread pool
    /// Performs iterative deepening MaxN search with time management
    pub fn compute_best_move_internal(
        board: &Board,
        you: &Battlesnake,
        shared: Arc<SharedSearchState>,
        start_time: Instant,
        config: &Config,
    ) {
        info!("Starting MaxN search computation");
        let init_start = Instant::now();

        // Create transposition table for this search
        // Size: 100k entries = ~1.6MB memory (16 bytes per entry)
        let tt = Arc::new(TranspositionTable::new(100_000));
        tt.increment_age();

        // Create killer move table for move ordering
        // Tracks moves that caused cutoffs for better alpha-beta pruning
        let mut killers = KillerMoveTable::new(config);
        let mut pv_move: Option<Direction> = None;

        // Create history table for move ordering
        // Tracks globally successful moves across all positions
        let mut history = HistoryTable::new(board.width as u32, board.height as u32);

        // Determine execution strategy
        let num_alive_snakes = board.snakes.iter().filter(|s| s.health > 0).count();
        let num_cpus = rayon::current_num_threads();

        let strategy = Self::determine_strategy(num_alive_snakes, num_cpus, config);
        info!(
            "Selected strategy: {:?} (snakes={}, cpus={})",
            strategy, num_alive_snakes, num_cpus
        );

        // Get appropriate time estimation parameters based on number of alive snakes
        let time_params = config.time_estimation.for_snake_count(num_alive_snakes);

        let init_elapsed = init_start.elapsed().as_micros();
        if simple_profiler::is_profiling_enabled() {
            eprintln!("[PROFILE] Initialization: {}µs", init_elapsed);
        }

        // Iterative deepening loop
        let mut current_depth = config.timing.initial_depth;
        let effective_budget = config.timing.effective_budget_ms();

        loop {
            let elapsed = start_time.elapsed().as_millis() as u64;
            let remaining = effective_budget.saturating_sub(elapsed);

            if simple_profiler::is_profiling_enabled() {
                eprintln!("[PROFILE] Loop iteration: depth={}, elapsed={}ms, remaining={}ms",
                         current_depth, elapsed, remaining);
            }

            // Check if we have enough time for another iteration
            if remaining < config.timing.min_time_remaining_ms {
                info!(
                    "Stopping search: insufficient time remaining ({}ms)",
                    remaining
                );
                if simple_profiler::is_profiling_enabled() {
                    eprintln!("[PROFILE] STOP REASON: Insufficient time ({}ms < {}ms min)",
                             remaining, config.timing.min_time_remaining_ms);
                }
                break;
            }

            // CRITICAL FIX: Use IDAPOS-filtered snake count for time estimation
            // Previously used num_alive_snakes (all snakes), causing massive overestimation
            let active_snakes = Self::determine_active_snakes(board, &you.id, current_depth, config);
            let num_active_snakes = active_snakes.len();

            // Estimate time for next iteration using exponential model
            // time = base_time * (branching_factor ^ (depth * num_snakes))
            // Use IDAPOS-filtered count for accurate estimation
            let exponent = (current_depth as f64) * (num_active_snakes as f64);
            let estimated_time = (time_params.base_iteration_time_ms * time_params.branching_factor.powf(exponent)).ceil() as u64;

            if simple_profiler::is_profiling_enabled() {
                eprintln!("[PROFILE] Time estimation: depth={}, snakes_total={}, snakes_active={} (IDAPOS), exponent={:.2}, base={:.3}ms, factor={:.2}, estimated={}ms",
                         current_depth, num_alive_snakes, num_active_snakes, exponent,
                         time_params.base_iteration_time_ms, time_params.branching_factor, estimated_time);
            }

            if estimated_time > remaining {
                info!("Stopping search: next iteration would exceed budget (estimated {}ms, remaining {}ms)",
                      estimated_time, remaining);
                if simple_profiler::is_profiling_enabled() {
                    eprintln!("[PROFILE] STOP REASON: Time estimate too high ({}ms > {}ms remaining)",
                             estimated_time, remaining);
                }
                break;
            }

            // Safety cap on depth
            if current_depth > config.timing.max_search_depth {
                info!("Stopping search: reached max depth ({})", current_depth);
                break;
            }

            info!(
                "Starting iteration at depth {} (estimated time: {}ms, mode: {} snakes)",
                current_depth, estimated_time, num_alive_snakes
            );
            shared.current_depth.store(current_depth, Ordering::Release);

            // Clear killers and history from previous iteration
            killers.clear();
            history.clear();

            // Record iteration start time
            let iteration_start = Instant::now();

            // Execute search with strategy-specific parameters
            match strategy {
                ExecutionStrategy::Sequential => {
                    Self::sequential_search(board, you, current_depth, &shared, config, &tt, &mut killers, &mut history, pv_move);
                }
                ExecutionStrategy::Parallel1v1 => {
                    Self::parallel_1v1_search(board, you, current_depth, &shared, config, &tt, &mut history, pv_move);
                }
                ExecutionStrategy::ParallelMultiplayer => {
                    Self::parallel_multiplayer_search(board, you, current_depth, &shared, config, &tt, &mut history, pv_move);
                }
            }

            // Record actual iteration time
            let iteration_elapsed = iteration_start.elapsed().as_millis() as u64;

            // Extract best move from this iteration to use as PV move for next iteration
            let (best_move_idx, _) = shared.get_best();
            pv_move = Some(Self::index_to_direction(best_move_idx, config));

            info!(
                "Completed depth {} in {}ms (estimated: {}ms, diff: {}ms)",
                current_depth, iteration_elapsed, estimated_time, iteration_elapsed as i64 - estimated_time as i64
            );

            current_depth += 1;
        }

        shared.search_complete.store(true, Ordering::Release);

        // Merge profiling data from all threads
        if simple_profiler::is_profiling_enabled() {
            simple_profiler::merge_thread_local();
        }

        let (best_move_idx, best_score) = shared.get_best();
        let (tt_entries, tt_capacity) = tt.stats();
        info!(
            "Search complete. Best move: {:?}, Score: {}, TT: {}/{} entries ({:.1}% full)",
            Self::index_to_direction(best_move_idx, config).as_str(),
            best_score,
            tt_entries,
            tt_capacity,
            100.0 * tt_entries as f64 / tt_capacity as f64
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
        depth: u8,
        shared: &Arc<SharedSearchState>,
        config: &Config,
        tt: &Arc<TranspositionTable>,
        killers: &mut KillerMoveTable,
        history: &mut HistoryTable,
        pv_move: Option<Direction>,
    ) {
        // Generate legal moves for our snake
        let mut legal_moves = Self::generate_legal_moves(board, you, config);

        if legal_moves.is_empty() {
            info!("No legal moves available - choosing least-bad fallback");
            // When trapped, try to pick a move that's at least in-bounds
            // Priority: any in-bounds move > out-of-bounds move
            let fallback_move = Direction::all()
                .iter()
                .find(|&&dir| {
                    let next = dir.apply(&you.body[0]);
                    !Self::is_out_of_bounds(&next, board.width, board.height)
                })
                .copied()
                .unwrap_or(Direction::Up); // If all moves are out of bounds, default to Up

            shared.try_update_best(
                Self::direction_to_index(fallback_move, config),
                i32::MIN,
            );
            return;
        }

        // Order moves for better alpha-beta pruning
        // Priority: PV move > killer moves > history heuristic > remaining moves
        legal_moves = order_moves(legal_moves, pv_move, killers, Some((history, &you.body[0])), depth, config);

        info!("Evaluating {} legal moves sequentially (ordered by PV + killers)", legal_moves.len());

        // Determine if we should use 1v1 alpha-beta or multiplayer MaxN
        let num_alive = board.snakes.iter().filter(|s| s.health > 0).count();
        let use_alpha_beta = num_alive == config.strategy.min_snakes_for_1v1;

        let our_snake_id = &you.id;
        let our_idx = board
            .snakes
            .iter()
            .position(|s| &s.id == our_snake_id)
            .unwrap_or(0);

        let mut best_score = i32::MIN;

        for &mv in legal_moves.iter() {
            let mut child_board = board.clone();
            Self::apply_move(&mut child_board, our_idx, mv, config);

            let score = if use_alpha_beta {
                // Use alpha-beta for 1v1
                Self::alpha_beta_minimax(
                    &child_board,
                    our_snake_id,
                    depth.saturating_sub(1),
                    i32::MIN,
                    i32::MAX,
                    false,
                    config,
                    tt,
                    killers,
                    history,
                )
            } else {
                // Use MaxN for multiplayer
                let tuple = Self::maxn_search(
                    &child_board,
                    our_snake_id,
                    depth.saturating_sub(1),
                    our_idx,
                    config,
                    tt,
                    killers,
                    history,
                );
                tuple.for_player(our_idx)
            };

            if score > best_score {
                best_score = score;

                // Immediate update (anytime property)
                shared.try_update_best(Self::direction_to_index(mv, config), best_score);
            }
        }

        info!("Sequential search complete: best score = {}", best_score);
    }

    /// Generates all legal moves for a snake
    /// A move is legal if it:
    /// - Doesn't go out of bounds
    /// - Doesn't collide with snake bodies (excluding tails which will move)
    /// - Doesn't reverse into the neck
    /// - Avoids head-to-head collisions with equal or longer snakes (unless no other option)
    pub fn generate_legal_moves(board: &Board, snake: &Battlesnake, config: &Config) -> Vec<Direction> {
        let _prof = simple_profiler::ProfileGuard::new("move_gen");

        if snake.health <= 0 || snake.body.is_empty() {
            return vec![];
        }

        let head = snake.body[0];
        let neck = if snake.body.len() > config.move_generation.snake_min_body_length_for_neck {
            Some(snake.body[1])
        } else {
            None
        };

        // First, generate all moves that pass basic collision checks
        let basic_legal_moves: Vec<Direction> = Direction::all()
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
            .collect();

        // Now filter out dangerous head-to-head positions
        let safe_moves: Vec<Direction> = basic_legal_moves
            .iter()
            .filter(|&&dir| {
                let next = dir.apply(&head);
                !Self::is_dangerous_head_to_head(&next, snake, board)
            })
            .copied()
            .collect();

        // If we have safe moves, use them. Otherwise, fall back to basic legal moves
        // (better to risk a head-to-head than to definitely die)
        if !safe_moves.is_empty() {
            safe_moves
        } else {
            basic_legal_moves
        }
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

    /// Checks if moving to a position could result in a dangerous head-to-head collision
    /// Returns true if any opponent snake could also move to the same position,
    /// AND that opponent is equal or longer length (meaning we would lose or tie)
    ///
    /// This handles two scenarios:
    /// 1. Direct collision: both snakes move to the exact same cell (e.g., converging on food)
    /// 2. Adjacent threat: opponent head is adjacent to our target position and could move there
    fn is_dangerous_head_to_head(position: &Coord, our_snake: &Battlesnake, board: &Board) -> bool {
        for opponent in &board.snakes {
            // Skip ourselves and dead snakes
            if opponent.id == our_snake.id || opponent.health <= 0 || opponent.body.is_empty() {
                continue;
            }

            let opp_head = opponent.body[0];

            // Get opponent's neck to avoid considering reverse moves
            let opp_neck = if opponent.body.len() > 1 {
                Some(opponent.body[1])
            } else {
                None
            };

            // Check if opponent could also move to the exact same target position
            // This is the key check for converging collisions (e.g., both going for food)
            for dir in Direction::all() {
                let opp_next = dir.apply(&opp_head);

                // Skip if opponent would be reversing onto their neck
                if let Some(neck) = opp_neck {
                    if opp_next == neck {
                        continue;
                    }
                }

                // If opponent could move to the same position as us
                if opp_next == *position {
                    // This is dangerous if they're equal or longer length
                    // Equal length: both die (bad for us)
                    // Longer: we die (bad for us)
                    // Only safe if we're strictly longer
                    if opponent.length >= our_snake.length {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Calculates Manhattan distance between two coordinates
    fn manhattan_distance(a: Coord, b: Coord) -> i32 {
        (a.x - b.x).abs() + (a.y - b.y).abs()
    }

    /// Converts a direction to its encoded index
    pub fn direction_to_index(dir: Direction, config: &Config) -> u8 {
        match dir {
            Direction::Up => config.direction_encoding.direction_up_index,
            Direction::Down => config.direction_encoding.direction_down_index,
            Direction::Left => config.direction_encoding.direction_left_index,
            Direction::Right => config.direction_encoding.direction_right_index,
        }
    }

    /// Converts an encoded index to a direction
    pub fn index_to_direction(idx: u8, config: &Config) -> Direction {
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

    /// Applies a move to a specific snake in the game state
    /// Updates snake position, handles food consumption, and decreases health
    fn apply_move(board: &mut Board, snake_idx: usize, dir: Direction, config: &Config) {
        let _prof = simple_profiler::ProfileGuard::new("apply_move");

        if snake_idx >= board.snakes.len() {
            return;
        }

        let snake = &mut board.snakes[snake_idx];
        if snake.health <= 0 || snake.body.is_empty() {
            return;
        }

        // Calculate new head position
        let new_head = dir.apply(&snake.body[0]);

        // Move head to new position
        snake.body.insert(0, new_head);
        snake.head = new_head;

        // Check if food was eaten
        let ate_food = board.food.contains(&new_head);
        if ate_food {
            // Remove food from board
            board.food.retain(|&f| f != new_head);
            // Restore health
            snake.health = config.game_rules.health_on_food as i32;
            // Grow snake (don't remove tail)
            snake.length += 1;
        } else {
            // Remove tail (snake doesn't grow)
            snake.body.pop();
            // Decrease health
            snake.health = snake.health.saturating_sub(config.game_rules.health_loss_per_turn as i32);
        }

        // Mark snake as dead if health reaches zero
        if snake.health <= 0 {
            snake.health = 0;
        }
    }

    /// Advances the game state by one turn after all snakes have moved
    /// Handles head-to-head collisions and body collisions
    fn advance_game_state(board: &mut Board) {
        // Detect head-to-head collisions
        let mut head_positions: HashMap<Coord, Vec<usize>> = HashMap::new();

        for (idx, snake) in board.snakes.iter().enumerate() {
            if snake.health > 0 && !snake.body.is_empty() {
                head_positions
                    .entry(snake.body[0])
                    .or_insert_with(Vec::new)
                    .push(idx);
            }
        }

        // Process head-to-head collisions
        for (_, indices) in head_positions.iter() {
            if indices.len() > 1 {
                // Multiple snakes at same position
                let max_length = indices
                    .iter()
                    .map(|&i| board.snakes[i].length)
                    .max()
                    .unwrap_or(0);

                // Count how many snakes have max length
                let max_count = indices
                    .iter()
                    .filter(|&&i| board.snakes[i].length == max_length)
                    .count();

                // Kill snakes based on length comparison
                for &idx in indices {
                    if board.snakes[idx].length < max_length {
                        // Shorter snake dies
                        board.snakes[idx].health = 0;
                    } else if max_count > 1 {
                        // Equal length: all die
                        board.snakes[idx].health = 0;
                    }
                }
            }
        }

        // Check for body collisions (snake head hitting any body segment)
        let mut collision_snakes = Vec::new();
        for (idx, snake) in board.snakes.iter().enumerate() {
            if snake.health <= 0 {
                continue;
            }

            let head = snake.body[0];

            // Check collision with all snake bodies (including own)
            for (other_idx, other_snake) in board.snakes.iter().enumerate() {
                if other_snake.health <= 0 {
                    continue;
                }

                // Check against body segments (excluding the tail which just moved)
                let check_len = if idx == other_idx {
                    // Own body: check all except head and tail
                    if other_snake.body.len() > 2 {
                        other_snake.body.len() - 1
                    } else {
                        0
                    }
                } else {
                    // Other snake: check all except tail
                    other_snake.body.len().saturating_sub(1)
                };

                if other_snake.body[1..check_len.min(other_snake.body.len())]
                    .contains(&head)
                {
                    collision_snakes.push(idx);
                    break;
                }
            }
        }

        // Mark collided snakes as dead
        for idx in collision_snakes {
            board.snakes[idx].health = 0;
        }
    }

    /// Checks if the game state is terminal (game over)
    fn is_terminal(board: &Board, our_snake_id: &str, config: &Config) -> bool {
        let alive_count = board.snakes.iter().filter(|s| s.health > 0).count();

        // Terminal if only one or zero snakes alive
        if alive_count <= config.game_rules.terminal_state_threshold {
            return true;
        }

        // Terminal if our snake is dead
        if let Some(our_snake) = board.snakes.iter().find(|s| s.id == our_snake_id) {
            if our_snake.health <= 0 {
                return true;
            }
        }

        false
    }

    /// Flood fill BFS to count reachable cells from a starting position
    /// Accounts for snake bodies that will move over time
    /// Returns the number of cells reachable
    fn flood_fill_bfs(board: &Board, start: Coord, _snake_idx: usize) -> usize {
        let _prof = simple_profiler::ProfileGuard::new("flood_fill");

        // Pre-build obstacle map for O(1) lookups (huge performance improvement)
        // Maps each occupied cell to the number of turns until it becomes free
        let mut obstacles: HashMap<Coord, usize> = HashMap::new();
        for snake in &board.snakes {
            if snake.health <= 0 {
                continue;
            }
            for (seg_idx, &segment) in snake.body.iter().enumerate() {
                let segments_from_tail = snake.body.len() - seg_idx;
                obstacles.insert(segment, segments_from_tail);
            }
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back((start, 0)); // (position, turns_elapsed)
        visited.insert(start);

        while let Some((pos, turns)) = queue.pop_front() {
            for dir in Direction::all().iter() {
                let next = dir.apply(&pos);

                // Check bounds
                if next.x < 0
                    || next.x >= board.width
                    || next.y < 0
                    || next.y >= board.height as i32
                {
                    continue;
                }

                if visited.contains(&next) {
                    continue;
                }

                // Check if blocked using pre-built obstacle map (O(1) instead of O(snakes × length))
                if let Some(&segments_from_tail) = obstacles.get(&next) {
                    if segments_from_tail > turns {
                        continue; // Still blocked
                    }
                }

                visited.insert(next);
                queue.push_back((next, turns + 1));
            }
        }

        visited.len()
    }

    /// Enhanced flood fill that returns distance information for entrapment detection
    /// Returns (total_cells, distance_map) where distance_map tracks turns to reach each cell
    fn flood_fill_with_distances(
        board: &Board,
        start: Coord,
        _snake_idx: usize,
    ) -> (usize, HashMap<Coord, usize>) {
        let _prof = simple_profiler::ProfileGuard::new("flood_fill_with_distances");

        // Pre-build obstacle map for O(1) lookups
        let mut obstacles: HashMap<Coord, usize> = HashMap::new();
        for snake in &board.snakes {
            if snake.health <= 0 {
                continue;
            }
            for (seg_idx, &segment) in snake.body.iter().enumerate() {
                let segments_from_tail = snake.body.len() - seg_idx;
                obstacles.insert(segment, segments_from_tail);
            }
        }

        let mut distance_map = HashMap::new();
        let mut queue = VecDeque::new();

        queue.push_back((start, 0)); // (position, turns_elapsed)
        distance_map.insert(start, 0);

        while let Some((pos, turns)) = queue.pop_front() {
            for dir in Direction::all().iter() {
                let next = dir.apply(&pos);

                // Check bounds
                if next.x < 0
                    || next.x >= board.width
                    || next.y < 0
                    || next.y >= board.height as i32
                {
                    continue;
                }

                if distance_map.contains_key(&next) {
                    continue;
                }

                // Check if blocked using pre-built obstacle map
                if let Some(&segments_from_tail) = obstacles.get(&next) {
                    if segments_from_tail > turns {
                        continue; // Still blocked
                    }
                }

                distance_map.insert(next, turns + 1);
                queue.push_back((next, turns + 1));
            }
        }

        let total = distance_map.len();
        (total, distance_map)
    }

    /// Checks if a position will be blocked at a future turn
    /// Accounts for snake body segments moving away over time
    fn is_position_blocked_at_time(
        board: &Board,
        pos: Coord,
        turns_future: usize,
        _checking_snake: usize,
    ) -> bool {
        for snake in &board.snakes {
            if snake.health <= 0 {
                continue;
            }

            for (seg_idx, &segment) in snake.body.iter().enumerate() {
                if segment == pos {
                    // Will this segment have moved away?
                    let segments_from_tail = snake.body.len() - seg_idx;
                    if segments_from_tail > turns_future {
                        return true; // Still occupied
                    }
                }
            }
        }
        false
    }

    /// Adversarial flood fill - simultaneous BFS from all snake heads
    /// Returns map of which snake controls each cell (longer snakes win ties)
    ///
    /// If active_snakes is empty, processes all snakes.
    /// Otherwise, only processes snakes in the provided list (IDAPOS optimization).
    fn adversarial_flood_fill(board: &Board, active_snakes: &[usize]) -> HashMap<Coord, usize> {
        let _prof = simple_profiler::ProfileGuard::new("adversarial_flood_fill");

        let mut control_map: HashMap<Coord, usize> = HashMap::new();
        let mut distance_map: HashMap<Coord, usize> = HashMap::new();

        // Determine which snakes to process
        let process_all = active_snakes.is_empty();

        // Mark snake bodies as obstacles controlled by their owner
        if process_all {
            for (idx, snake) in board.snakes.iter().enumerate() {
                if snake.health <= 0 {
                    continue;
                }
                for &seg in &snake.body {
                    control_map.insert(seg, idx);
                }
            }
        } else {
            for &idx in active_snakes {
                if idx >= board.snakes.len() {
                    continue;
                }
                let snake = &board.snakes[idx];
                if snake.health <= 0 {
                    continue;
                }
                for &seg in &snake.body {
                    control_map.insert(seg, idx);
                }
            }
        }

        // Sort snakes by length (longer snakes win ties)
        let mut snakes_sorted: Vec<(usize, &Battlesnake)> = if process_all {
            board.snakes.iter().enumerate().collect()
        } else {
            active_snakes
                .iter()
                .filter_map(|&idx| {
                    if idx < board.snakes.len() {
                        Some((idx, &board.snakes[idx]))
                    } else {
                        None
                    }
                })
                .collect()
        };
        snakes_sorted.sort_by_key(|(_, s)| std::cmp::Reverse(s.length));

        // Simultaneous BFS from all heads
        let mut queue = VecDeque::new();
        for (idx, snake) in snakes_sorted.iter() {
            if snake.health > 0 && !snake.body.is_empty() {
                queue.push_back((snake.body[0], *idx, 0));
                distance_map.insert(snake.body[0], 0);
            }
        }

        while let Some((pos, owner, dist)) = queue.pop_front() {
            // Skip if already claimed by another snake at same or closer distance
            if let Some(&existing_dist) = distance_map.get(&pos) {
                if existing_dist < dist {
                    continue;
                }
            }

            // Claim cell if not already controlled
            control_map.entry(pos).or_insert(owner);

            for dir in Direction::all().iter() {
                let next = dir.apply(&pos);

                if next.x < 0
                    || next.x >= board.width
                    || next.y < 0
                    || next.y >= board.height as i32
                {
                    continue;
                }

                let next_dist = dist + 1;

                // Only explore if we can reach it faster (not equal distance - prevents re-exploration)
                let should_explore = match distance_map.get(&next) {
                    Some(&existing_dist) => next_dist < existing_dist,
                    None => true,
                };

                if should_explore && !control_map.contains_key(&next) {
                    distance_map.insert(next, next_dist);
                    queue.push_back((next, owner, next_dist));
                }
            }
        }

        control_map
    }

    /// Helper to compute control score from pre-computed map
    fn compute_control_score_from_map(
        control_map: &HashMap<Coord, usize>,
        snake_idx: usize,
        config: &Config,
    ) -> i32 {
        let our_cells = control_map
            .values()
            .filter(|&&owner| owner == snake_idx)
            .count();
        let total_free = control_map.len();

        if total_free == 0 {
            return 0;
        }

        ((our_cells as f32 / total_free as f32) * config.scores.territory_scale_factor) as i32
    }

    /// Computes health and food score for a snake
    /// Returns higher score for closer food when health is low
    /// Adds extra urgency when in health disadvantage vs opponents
    fn compute_health_score(board: &Board, snake_idx: usize, config: &Config) -> i32 {
        if snake_idx >= board.snakes.len() {
            return config.scores.score_zero_health;
        }

        let snake = &board.snakes[snake_idx];
        if snake.health <= 0 {
            return config.scores.score_zero_health;
        }

        if board.food.is_empty() {
            // No food available - penalty based on remaining health
            let health_ratio = snake.health as f32 / config.scores.health_max;
            return (health_ratio * config.scores.score_zero_health as f32) as i32;
        }

        let head = snake.body[0];

        // Find nearest food
        let nearest_food_dist = board
            .food
            .iter()
            .map(|&food| Self::manhattan_distance(head, food))
            .min()
            .unwrap_or(config.scores.default_food_distance);

        // Urgency increases as health decreases
        // Length-aware: longer snakes need to plan further ahead (more body to navigate)
        let base_urgency = (config.scores.health_max - snake.health as f32) / config.scores.health_max;
        let length_multiplier = (config.scores.health_urgency_min_multiplier +
            ((snake.length as f32 - config.scores.health_urgency_base_length) *
             config.scores.health_urgency_length_multiplier))
            .min(config.scores.health_urgency_max_multiplier)
            .max(config.scores.health_urgency_min_multiplier);
        let urgency = base_urgency * length_multiplier;
        let distance_penalty = -(nearest_food_dist as f32 * urgency) as i32;

        // Critical: will starve before reaching food
        // Add buffer for longer snakes - they need more turns to maneuver around their body
        let starvation_buffer = (snake.length as i32 / config.scores.starvation_buffer_divisor).max(0);
        if snake.health as i32 <= nearest_food_dist + starvation_buffer {
            return config.scores.score_starvation_base + distance_penalty;
        }

        // Check if we're in a health disadvantage against nearby opponents
        // This helps break out of "death dance" scenarios where both snakes circle endlessly
        // Only consider opponents that are close enough to be an immediate threat
        let max_nearby_opponent_health = board
            .snakes
            .iter()
            .enumerate()
            .filter(|(idx, s)| {
                if *idx == snake_idx || s.health <= 0 || s.body.is_empty() {
                    return false;
                }
                // Only consider opponents within threat range
                let dist = Self::manhattan_distance(head, s.body[0]);
                dist <= config.scores.health_threat_distance
            })
            .map(|(_, s)| s.health)
            .max()
            .unwrap_or(0);

        // If any nearby opponent has more health than us, add extra food urgency
        // This multiplier increases the further behind we are in health
        let health_disadvantage = if max_nearby_opponent_health > snake.health {
            let health_gap = max_nearby_opponent_health as f32 - snake.health as f32;
            // Scale the disadvantage: larger gaps = more urgency
            // Multiply distance penalty by (1 + gap/50), capping at 3x
            let multiplier = (1.0 + (health_gap / 50.0)).min(3.0);
            (distance_penalty as f32 * multiplier) as i32
        } else {
            distance_penalty
        };

        health_disadvantage
    }

    /// Computes space control score - how many cells are reachable
    /// Penalizes cramped positions that could lead to being trapped
    /// Uses IDAPOS-filtered active_snakes list for adversarial entrapment detection
    fn compute_space_score(
        board: &Board,
        snake_idx: usize,
        active_snakes: &[usize],
        config: &Config,
    ) -> i32 {
        if snake_idx >= board.snakes.len() {
            return -(config.scores.space_safety_margin as i32)
                * config.scores.space_shortage_penalty;
        }

        let snake = &board.snakes[snake_idx];
        if snake.health <= 0 || snake.body.is_empty() {
            return -(config.scores.space_safety_margin as i32)
                * config.scores.space_shortage_penalty;
        }

        // Get reachable cells with distance information
        let (reachable, distance_map) = Self::flood_fill_with_distances(board, snake.body[0], snake_idx);
        let required = snake.length as usize + config.scores.space_safety_margin;

        if reachable < required {
            return -((required as i32 - reachable as i32) * config.scores.space_shortage_penalty);
        }

        // Detect tight spaces / narrow corridors (entrapment risk)
        // If most cells are far away, we're in a narrow corridor that could trap us
        let nearby_threshold = (snake.length.min(config.scores.entrapment_nearby_threshold as i32)) as usize;
        let nearby_cells = distance_map.iter().filter(|(_, &dist)| dist <= nearby_threshold).count();
        let compactness_ratio = nearby_cells as f32 / reachable as f32;

        // Penalty for narrow spaces based on compactness ratio thresholds
        let entrapment_penalty = if compactness_ratio < config.scores.entrapment_severe_threshold {
            // Severe penalty: likely in a narrow corridor
            -((reachable as f32 * config.scores.entrapment_severe_penalty_multiplier) as i32)
        } else if compactness_ratio < config.scores.entrapment_moderate_threshold {
            // Moderate penalty: somewhat confined
            -((reachable as f32 * config.scores.entrapment_moderate_penalty_multiplier) as i32)
        } else {
            0
        };

        // Adversarial Entrapment: Detect if opponents are actively trapping us
        // Use pre-computed IDAPOS active_snakes list for efficiency
        let adversarial_penalty = Self::compute_adversarial_entrapment_penalty(
            board,
            snake_idx,
            reachable,
            active_snakes,
            config
        );

        reachable as i32 + entrapment_penalty + adversarial_penalty
    }

    /// Detects if nearby opponents are actively reducing our space (adversarial entrapment)
    /// Returns penalty if opponent movements would significantly cut our accessible area
    /// Uses pre-computed IDAPOS active_snakes list for maximum efficiency
    fn compute_adversarial_entrapment_penalty(
        board: &Board,
        our_idx: usize,
        _our_current_space: usize,
        active_snakes: &[usize],
        config: &Config,
    ) -> i32 {
        if our_idx >= board.snakes.len() {
            return 0;
        }

        let our_snake = &board.snakes[our_idx];
        if our_snake.health <= 0 || our_snake.body.is_empty() {
            return 0;
        }

        let our_head = our_snake.body[0];
        let locality_threshold = config.scores.adversarial_entrapment_distance;
        let mut max_penalty = 0;

        // Use IDAPOS-filtered active_snakes list - only these snakes are relevant
        // This avoids redundant locality checks since active_snakes is already filtered
        for &opp_idx in active_snakes {
            if opp_idx == our_idx {
                continue;
            }

            let opponent = &board.snakes[opp_idx];
            if opponent.health <= 0 || opponent.body.is_empty() {
                continue;
            }

            // Check if opponent is within entrapment distance
            let distance = Self::manhattan_distance(our_head, opponent.body[0]);
            if distance > locality_threshold {
                continue; // Snake too far away to pose entrapment threat
            }

            // Opponent is nearby and active - check if they're cutting off our space
            // If opponent is longer or equal, they're more dangerous
            if opponent.length >= our_snake.length {
                // Estimate: if opponent moves toward us, how much space do we lose?
                // Simple heuristic: opponents closer to us reduce our effective space
                let space_threat_ratio = (locality_threshold - distance) as f32 /
                    locality_threshold as f32;

                if space_threat_ratio > config.scores.adversarial_space_reduction_threshold {
                    let penalty = (config.scores.adversarial_space_reduction_penalty as f32 *
                        space_threat_ratio) as i32;
                    max_penalty = max_penalty.min(-penalty); // Accumulate worst case
                }
            }
        }

        max_penalty
    }

    /// Computes territory control score - percentage of free cells controlled
    /// Uses adversarial flood fill to determine territory ownership
    fn compute_control_score(board: &Board, snake_idx: usize, config: &Config) -> i32 {
        if snake_idx >= board.snakes.len() {
            return 0;
        }

        let control_map = Self::adversarial_flood_fill(board, &[]);

        let our_cells = control_map
            .values()
            .filter(|&&owner| owner == snake_idx)
            .count();
        let total_free = control_map.len();

        if total_free == 0 {
            return 0;
        }

        ((our_cells as f32 / total_free as f32) * config.scores.territory_scale_factor) as i32
    }

    /// Computes attack potential score
    /// Awards points for length advantage near opponents and trapping opponents
    /// Uses cached flood fill results if available (P2: caching optimization)
    fn compute_attack_score(
        board: &Board,
        snake_idx: usize,
        config: &Config,
        space_cache: &HashMap<usize, usize>,
    ) -> i32 {
        if snake_idx >= board.snakes.len() {
            return 0;
        }

        let our_snake = &board.snakes[snake_idx];
        if our_snake.health <= 0 || our_snake.body.is_empty() {
            return 0;
        }

        let our_head = our_snake.body[0];
        let mut attack = 0i32;

        for (idx, opponent) in board.snakes.iter().enumerate() {
            if idx == snake_idx || opponent.health <= 0 || opponent.body.is_empty() {
                continue;
            }

            // Head-to-head advantage if longer
            if our_snake.length > opponent.length {
                let dist = Self::manhattan_distance(our_head, opponent.body[0]);
                if dist <= config.scores.attack_head_to_head_distance {
                    attack += config.scores.attack_head_to_head_bonus;
                }
            }

            // Trap potential - opponent has limited space (use cache if available)
            let opp_space = space_cache
                .get(&idx)
                .copied()
                .unwrap_or_else(|| Self::flood_fill_bfs(board, opponent.body[0], idx));
            if opp_space < opponent.length as usize + config.scores.attack_trap_margin {
                attack += config.scores.attack_trap_bonus;
            }
        }

        attack
    }

    /// Checks if a position could result in a head-to-head collision with equal/longer opponents
    /// Returns a penalty if the position is dangerous (could lose head-to-head)
    fn check_head_collision_danger(
        board: &Board,
        snake_idx: usize,
        position: Coord,
        config: &Config,
    ) -> i32 {
        if snake_idx >= board.snakes.len() {
            return 0;
        }

        let our_snake = &board.snakes[snake_idx];
        if our_snake.health <= 0 || our_snake.body.is_empty() {
            return 0;
        }

        // Check each opponent
        for (idx, opponent) in board.snakes.iter().enumerate() {
            if idx == snake_idx || opponent.health <= 0 || opponent.body.is_empty() {
                continue;
            }

            let opp_head = opponent.body[0];

            // Get opponent's neck to avoid considering reverse moves
            let opp_neck = if opponent.body.len() > 1 {
                Some(opponent.body[1])
            } else {
                None
            };

            // For each possible opponent move, check if they could reach our position
            for dir in Direction::all() {
                let opp_next_pos = dir.apply(&opp_head);

                // Skip if opponent would be reversing onto their neck
                if let Some(neck) = opp_neck {
                    if opp_next_pos == neck {
                        continue;
                    }
                }

                // If opponent could move to the same position as us
                if opp_next_pos == position {
                    // Check if we would lose (equal or shorter length)
                    if our_snake.length <= opponent.length {
                        // This is a dangerous position - we would lose or tie
                        return config.scores.head_collision_penalty;
                    }
                }
            }
        }

        0
    }

    /// Computes wall proximity penalty to discourage moves toward boundaries
    /// Uses formula: penalty = -wall_penalty_base / (distance + 1)
    /// Examples: distance=0 → -10000, distance=1 → -5000, distance=2 → -3333
    /// Caps at distance >= 3 (safe distance)
    fn compute_wall_penalty(pos: Coord, width: i32, height: i32, config: &Config) -> i32 {
        let dist_to_wall = [
            pos.x,                  // distance to left wall
            width - 1 - pos.x,      // distance to right wall
            pos.y,                  // distance to bottom wall
            height - 1 - pos.y,     // distance to top wall
        ]
        .iter()
        .min()
        .copied()
        .unwrap_or(0);

        // Cap at safe distance from wall
        if dist_to_wall >= config.scores.safe_distance_from_wall {
            return 0;
        }

        // Apply mathematical formula for smooth gradient
        // Penalty decreases as distance increases
        -(config.scores.wall_penalty_base / (dist_to_wall + 1))
    }

    /// Computes center bias to encourage staying in central board positions
    /// Central positions provide more escape routes and avoid dead ends
    fn compute_center_bias(pos: Coord, width: i32, height: i32, config: &Config) -> i32 {
        let center_x = width / 2;
        let center_y = height / 2;
        let dist_from_center = (pos.x - center_x).abs() + (pos.y - center_y).abs();

        // Prefer central positions
        // Center = +100, edges = 0 or negative
        100 - (dist_from_center * config.scores.center_bias_multiplier)
    }

    /// Evaluates the current game state for all snakes
    /// Returns an N-tuple of scores (one per snake)
    ///
    /// # Parameters
    /// * `active_snakes` - Optional set of snake indices to evaluate in detail (from IDAPOS)
    ///                     If None, evaluates all snakes fully
    fn evaluate_state(
        board: &Board,
        our_snake_id: &str,
        config: &Config,
        active_snakes: Option<&[usize]>,
    ) -> ScoreTuple {
        let _prof = simple_profiler::ProfileGuard::new("eval");

        let num_snakes = board.snakes.len();
        let mut scores = vec![0i32; num_snakes];

        // Pre-compute ALL flood fills once per evaluation (P2: caching optimization)
        // This eliminates redundant computation in space + attack scores
        let mut space_cache: HashMap<usize, usize> = HashMap::new();
        for (idx, snake) in board.snakes.iter().enumerate() {
            if snake.health > 0 && !snake.body.is_empty() {
                // Only compute for active snakes (IDAPOS optimization)
                let is_active = active_snakes.map_or(true, |active| active.contains(&idx));
                if is_active {
                    space_cache.insert(idx, Self::flood_fill_bfs(board, snake.body[0], idx));
                }
            }
        }

        // Compute territory control ONCE for active snakes only (major optimization!)
        // If active_snakes is empty, processes all snakes. Otherwise, only processes filtered snakes.
        let control_map = if let Some(active) = active_snakes {
            if active.is_empty() {
                None
            } else {
                Some(Self::adversarial_flood_fill(board, active))
            }
        } else {
            Some(Self::adversarial_flood_fill(board, &[]))
        };

        for (idx, snake) in board.snakes.iter().enumerate() {
            if snake.health <= 0 {
                scores[idx] = config.scores.score_dead_snake;
                continue;
            }

            // Check if this snake is active (needs full evaluation)
            let is_active = active_snakes.map_or(true, |active| active.contains(&idx));

            // Multi-component evaluation
            let survival = 0; // Alive = 0 penalty
            let health = Self::compute_health_score(board, idx, config);

            // Compute space score with entrapment detection
            // Uses IDAPOS-filtered active snakes for adversarial entrapment detection
            let space = if is_active {
                let active_list = active_snakes.unwrap_or(&[]);
                Self::compute_space_score(board, idx, active_list, config)
            } else {
                0
            };

            // Only compute expensive control and attack for active snakes
            let control = if is_active {
                if let Some(ref map) = control_map {
                    Self::compute_control_score_from_map(map, idx, config)
                } else {
                    0
                }
            } else {
                0  // Skip expensive territory control for non-active snakes
            };

            let length = snake.length * config.scores.weight_length;

            let attack = if is_active {
                Self::compute_attack_score(board, idx, config, &space_cache)
            } else {
                0  // Skip expensive attack calculation for non-active snakes
            };

            // Check for head-to-head collision danger
            let head_collision_danger = if !snake.body.is_empty() {
                Self::check_head_collision_danger(board, idx, snake.body[0], config)
            } else {
                0
            };

            // Wall proximity penalty and center bias
            let (wall_penalty, center_bias) = if !snake.body.is_empty() {
                let head = snake.body[0];
                (
                    Self::compute_wall_penalty(head, board.width as i32, board.height as i32, config),
                    Self::compute_center_bias(head, board.width as i32, board.height as i32, config),
                )
            } else {
                (0, 0)
            };

            // Weighted combination
            scores[idx] = survival
                + (config.scores.score_survival_weight * survival as f32) as i32
                + (config.scores.weight_space * space as f32) as i32
                + (config.scores.weight_health * health as f32) as i32
                + (config.scores.weight_control * control as f32) as i32
                + (config.scores.weight_attack * attack as f32) as i32
                + length
                + head_collision_danger
                + wall_penalty
                + center_bias;
        }

        // Apply survival penalty if our snake is dead
        if let Some(our_idx) = board.snakes.iter().position(|s| s.id == our_snake_id) {
            if board.snakes[our_idx].health <= 0 {
                scores[our_idx] = config.scores.score_survival_penalty;
            }
        }

        ScoreTuple { scores }
    }

    /// Determines which snakes are active (local) for IDAPOS optimization
    /// Returns indices of snakes within locality distance
    fn determine_active_snakes(
        board: &Board,
        our_snake_id: &str,
        remaining_depth: u8,
        config: &Config,
    ) -> Vec<usize> {
        let our_idx = match board.snakes.iter().position(|s| s.id == our_snake_id) {
            Some(idx) => idx,
            None => return vec![],
        };

        let mut active = vec![our_idx];

        if board.snakes[our_idx].health <= 0 || board.snakes[our_idx].body.is_empty() {
            return active;
        }

        let our_head = board.snakes[our_idx].body[0];
        let locality_threshold =
            config.idapos.head_distance_multiplier * remaining_depth as i32;

        for (idx, snake) in board.snakes.iter().enumerate() {
            if idx == our_idx || snake.health <= 0 {
                continue;
            }

            // Check head distance
            let head_dist = Self::manhattan_distance(our_head, snake.body[0]);
            if head_dist <= locality_threshold {
                active.push(idx);
                continue;
            }

            // Check any body segment distance
            for &segment in &snake.body {
                if Self::manhattan_distance(our_head, segment) <= remaining_depth as i32 {
                    active.push(idx);
                    break;
                }
            }
        }

        active
    }

    /// Pessimistic tie-breaking for MaxN: assume opponents minimize our score
    /// Returns the tuple with lower sum of opponent scores
    fn pessimistic_tie_break(
        a: &ScoreTuple,
        b: &ScoreTuple,
        our_idx: usize,
    ) -> ScoreTuple {
        let opponent_sum = |t: &ScoreTuple| {
            t.scores
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != our_idx)
                .map(|(_, &s)| s)
                .sum::<i32>()
        };

        if opponent_sum(a) < opponent_sum(b) {
            a.clone()
        } else {
            b.clone()
        }
    }

    /// Converts a 2-snake scenario to alpha-beta search and returns ScoreTuple
    /// Used by IDAPOS when locality masking reduces game to 2 active snakes
    fn alpha_beta_for_two_snakes(
        board: &Board,
        our_snake_id: &str,
        depth: u8,
        our_idx: usize,
        opponent_idx: usize,
        config: &Config,
        tt: &Arc<TranspositionTable>,
    ) -> ScoreTuple {
        // Create a simplified 2-player board with only the active snakes
        let mut simplified_board = board.clone();

        // Mark all non-active snakes as dead
        for (idx, snake) in simplified_board.snakes.iter_mut().enumerate() {
            if idx != our_idx && idx != opponent_idx {
                snake.health = 0;
            }
        }

        // Create local killer table and history table for this search
        let mut killers = KillerMoveTable::new(config);
        let mut history = HistoryTable::new(board.width as u32, board.height as u32);

        // Use alpha-beta to get our score
        let our_score = Self::alpha_beta_minimax(
            &simplified_board,
            our_snake_id,
            depth,
            i32::MIN,
            i32::MAX,
            true,
            config,
            tt,
            &mut killers,
            &mut history,
        );

        // Create score tuple with our score and opponent's inverse
        // In zero-sum approximation, opponent's score is -our_score
        let mut scores = vec![i32::MIN; board.snakes.len()];
        scores[our_idx] = our_score;
        scores[opponent_idx] = -our_score;

        ScoreTuple { scores }
    }

    /// MaxN recursive search for multiplayer games
    /// Each player maximizes their own score component
    fn maxn_search(
        board: &Board,
        our_snake_id: &str,
        depth: u8,
        current_player_idx: usize,
        config: &Config,
        tt: &Arc<TranspositionTable>,
        killers: &mut KillerMoveTable,
        history: &mut HistoryTable,
    ) -> ScoreTuple {
        let _prof = simple_profiler::ProfileGuard::new("maxn");

        // Probe transposition table
        let board_hash = TranspositionTable::hash_board(board);
        if let Some(cached_score) = tt.probe(board_hash, depth) {
            simple_profiler::record_tt_lookup(true);
            return ScoreTuple::new_with_value(board.snakes.len(), cached_score);
        }
        simple_profiler::record_tt_lookup(false);

        let our_idx = board
            .snakes
            .iter()
            .position(|s| &s.id == our_snake_id)
            .unwrap_or(0);

        // IDAPOS: Determine active (local) snakes to reduce branching
        // Do this BEFORE terminal evaluation so we can optimize evaluation too
        let active_snakes = Self::determine_active_snakes(board, our_snake_id, depth, config);

        // Terminal conditions
        if depth == 0 || Self::is_terminal(board, our_snake_id, config) {
            let eval = Self::evaluate_state(board, our_snake_id, config, Some(&active_snakes));
            tt.store(board_hash, eval.for_player(our_idx), depth);
            return eval;
        }

        // If only 2 snakes are active and we're one of them, use alpha-beta
        if active_snakes.len() == config.idapos.min_snakes_for_alpha_beta
            && active_snakes.contains(&our_idx)
        {
            // Switch to alpha-beta for efficiency
            let opponent_idx = active_snakes
                .iter()
                .find(|&&idx| idx != our_idx)
                .copied()
                .unwrap_or(0);

            return Self::alpha_beta_for_two_snakes(
                board,
                our_snake_id,
                depth,
                our_idx,
                opponent_idx,
                config,
                tt,
            );
        }

        // Check if current player is alive and active
        if current_player_idx >= board.snakes.len()
            || board.snakes[current_player_idx].health <= 0
            || !active_snakes.contains(&current_player_idx)
        {
            // Skip to next player (inactive snake passes their turn)
            let next = (current_player_idx + 1) % board.snakes.len();

            // Check if we've completed a full round (cycled back to our snake)
            if next == our_idx {
                // All active snakes have moved, inactive snakes passed
                // Advance game state and reduce depth
                let mut advanced_board = board.clone();
                Self::advance_game_state(&mut advanced_board);
                return Self::maxn_search(&advanced_board, our_snake_id, depth - 1, our_idx, config, tt, killers, history);
            } else {
                // Continue with next player at same depth
                return Self::maxn_search(board, our_snake_id, depth, next, config, tt, killers, history);
            }
        }

        // Generate legal moves for current player
        let mut moves = Self::generate_legal_moves(board, &board.snakes[current_player_idx], config);

        if moves.is_empty() {
            // No legal moves - mark snake as dead and continue
            let mut dead_board = board.clone();
            dead_board.snakes[current_player_idx].health = 0;
            let next = (current_player_idx + 1) % board.snakes.len();
            return Self::maxn_search(&dead_board, our_snake_id, depth, next, config, tt, killers, history);
        }

        // Order moves using history heuristic for better move ordering
        let current_pos = &board.snakes[current_player_idx].body[0];
        moves = order_moves(moves, None, killers, Some((history, current_pos)), depth, config);

        let mut best_tuple =
            ScoreTuple::new_with_value(board.snakes.len(), i32::MIN);

        for mv in moves {
            let mut child_board = board.clone();
            Self::apply_move(&mut child_board, current_player_idx, mv, config);

            let next = (current_player_idx + 1) % board.snakes.len();
            let all_moved = next == our_idx;

            let child_tuple = if all_moved {
                // All snakes have moved - advance game state and reduce depth
                Self::advance_game_state(&mut child_board);
                Self::maxn_search(&child_board, our_snake_id, depth - 1, our_idx, config, tt, killers, history)
            } else {
                // Continue with next player at same depth
                Self::maxn_search(&child_board, our_snake_id, depth, next, config, tt, killers, history)
            };

            // Update if current player improves their score
            if child_tuple.for_player(current_player_idx)
                > best_tuple.for_player(current_player_idx)
            {
                // Update history for this good move
                history.update(current_pos, mv, depth, false);
                best_tuple = child_tuple;
            } else if child_tuple.for_player(current_player_idx)
                == best_tuple.for_player(current_player_idx)
            {
                // Pessimistic tie-breaking
                best_tuple = Self::pessimistic_tie_break(&best_tuple, &child_tuple, our_idx);
            }
        }

        // Store result in transposition table before returning
        tt.store(board_hash, best_tuple.for_player(our_idx), depth);
        best_tuple
    }

    /// Alpha-beta minimax for 2-player zero-sum games (1v1)
    /// More efficient than MaxN when only two snakes remain
    fn alpha_beta_minimax(
        board: &Board,
        our_snake_id: &str,
        depth: u8,
        mut alpha: i32,
        mut beta: i32,
        is_max: bool,
        config: &Config,
        tt: &Arc<TranspositionTable>,
        killers: &mut KillerMoveTable,
        history: &mut HistoryTable,
    ) -> i32 {
        let _prof = simple_profiler::ProfileGuard::new("alpha_beta");

        // Probe transposition table
        let board_hash = TranspositionTable::hash_board(board);
        if let Some(cached_score) = tt.probe(board_hash, depth) {
            simple_profiler::record_tt_lookup(true);
            return cached_score;
        }
        simple_profiler::record_tt_lookup(false);

        if depth == 0 || Self::is_terminal(board, our_snake_id, config) {
            // In alpha-beta (1v1), no need to filter - only 2 snakes
            let scores = Self::evaluate_state(board, our_snake_id, config, None);
            let our_idx = board
                .snakes
                .iter()
                .position(|s| &s.id == our_snake_id)
                .unwrap_or(0);
            let score = scores.for_player(our_idx);
            tt.store(board_hash, score, depth);
            return score;
        }

        let our_idx = board
            .snakes
            .iter()
            .position(|s| &s.id == our_snake_id)
            .unwrap_or(0);

        // Determine which player moves
        let player_idx = if is_max {
            our_idx
        } else {
            // Find opponent (first alive snake that isn't us)
            board
                .snakes
                .iter()
                .enumerate()
                .find(|(i, s)| *i != our_idx && s.health > 0)
                .map(|(i, _)| i)
                .unwrap_or(our_idx)
        };

        if player_idx >= board.snakes.len() || board.snakes[player_idx].health <= 0 {
            // Player is dead, return evaluation
            let scores = Self::evaluate_state(board, our_snake_id, config, None);
            return scores.for_player(our_idx);
        }

        let mut moves = Self::generate_legal_moves(board, &board.snakes[player_idx], config);

        if moves.is_empty() {
            let mut dead_board = board.clone();
            dead_board.snakes[player_idx].health = 0;
            return Self::alpha_beta_minimax(
                &dead_board,
                our_snake_id,
                depth,
                alpha,
                beta,
                !is_max,
                config,
                tt,
                killers,
                history,
            );
        }

        // Order moves for better alpha-beta pruning
        // Use history heuristic for move ordering (no PV at interior nodes)
        let current_pos = &board.snakes[player_idx].body[0];
        moves = order_moves(moves, None, killers, Some((history, current_pos)), depth, config);

        if is_max {
            let mut max_eval = i32::MIN;
            for mv in moves {
                let mut child_board = board.clone();
                Self::apply_move(&mut child_board, player_idx, mv, config);
                Self::advance_game_state(&mut child_board);

                let eval = Self::alpha_beta_minimax(
                    &child_board,
                    our_snake_id,
                    depth - 1,
                    alpha,
                    beta,
                    false,
                    config,
                    tt,
                    killers,
                    history,
                );
                max_eval = max_eval.max(eval);
                alpha = alpha.max(eval);
                if beta <= alpha {
                    // Beta cutoff: record this move as a killer and update history
                    killers.record_killer(depth, mv, config);
                    history.update(current_pos, mv, depth, true);
                    simple_profiler::record_alpha_beta_cutoff();
                    break;
                }
            }
            tt.store(board_hash, max_eval, depth);
            max_eval
        } else {
            let mut min_eval = i32::MAX;
            for mv in moves {
                let mut child_board = board.clone();
                Self::apply_move(&mut child_board, player_idx, mv, config);
                Self::advance_game_state(&mut child_board);

                let eval = Self::alpha_beta_minimax(
                    &child_board,
                    our_snake_id,
                    depth - 1,
                    alpha,
                    beta,
                    true,
                    config,
                    tt,
                    killers,
                    history,
                );
                min_eval = min_eval.min(eval);
                beta = beta.min(eval);
                if beta <= alpha {
                    // Alpha cutoff: record this move as a killer and update history
                    killers.record_killer(depth, mv, config);
                    history.update(current_pos, mv, depth, true);
                    simple_profiler::record_alpha_beta_cutoff();
                    break;
                }
            }
            tt.store(board_hash, min_eval, depth);
            min_eval
        }
    }

    /// Parallel multiplayer MaxN search using rayon
    /// Evaluates root moves in parallel, then uses sequential MaxN for subtrees
    fn parallel_multiplayer_search(
        board: &Board,
        you: &Battlesnake,
        depth: u8,
        shared: &Arc<SharedSearchState>,
        config: &Config,
        tt: &Arc<TranspositionTable>,
        _history: &mut HistoryTable,  // Unused in parallel search (each thread has its own)
        pv_move: Option<Direction>,
    ) {
        // Order moves using PV move from previous iteration
        let mut legal_moves = Self::generate_legal_moves(board, you, config);

        if !legal_moves.is_empty() {
            // Order root moves by PV only (no killers/history at root for parallel search)
            legal_moves = order_moves(legal_moves, pv_move, &KillerMoveTable::new(config), None, depth, config);
        }

        if legal_moves.is_empty() {
            info!("No legal moves available - choosing least-bad fallback");
            // When trapped, try to pick a move that's at least in-bounds
            // Priority: any in-bounds move > out-of-bounds move
            let fallback_move = Direction::all()
                .iter()
                .find(|&&dir| {
                    let next = dir.apply(&you.body[0]);
                    !Self::is_out_of_bounds(&next, board.width, board.height)
                })
                .copied()
                .unwrap_or(Direction::Up); // If all moves are out of bounds, default to Up

            shared.try_update_best(
                Self::direction_to_index(fallback_move, config),
                i32::MIN,
            );
            return;
        }

        info!(
            "Evaluating {} legal moves in parallel (multiplayer MaxN)",
            legal_moves.len()
        );

        let our_snake_id = &you.id;
        let our_idx = board
            .snakes
            .iter()
            .position(|s| &s.id == our_snake_id)
            .unwrap_or(0);

        // Parallel evaluation of root moves
        legal_moves.par_iter().enumerate().for_each(|(_idx, &mv)| {
            // Each thread needs its own killers and history tables (can't share mutable refs across threads)
            let mut local_killers = KillerMoveTable::new(config);
            let mut local_history = HistoryTable::new(board.width as u32, board.height as u32);

            let mut child_board = board.clone();
            Self::apply_move(&mut child_board, our_idx, mv, config);

            let tuple = Self::maxn_search(
                &child_board,
                our_snake_id,
                depth.saturating_sub(1),
                our_idx,
                config,
                tt,
                &mut local_killers,
                &mut local_history,
            );
            let our_score = tuple.for_player(our_idx);

            // Atomic update of best move and score together (prevents race conditions)
            shared.try_update_best(Self::direction_to_index(mv, config), our_score);
        });

        let (_, final_score) = shared.get_best();
        info!(
            "Parallel multiplayer search complete: best score = {}",
            final_score
        );
    }

    /// Parallel 1v1 alpha-beta search using rayon
    /// Evaluates root moves in parallel, then uses sequential alpha-beta for subtrees
    fn parallel_1v1_search(
        board: &Board,
        you: &Battlesnake,
        depth: u8,
        shared: &Arc<SharedSearchState>,
        config: &Config,
        tt: &Arc<TranspositionTable>,
        _history: &mut HistoryTable,  // Unused in parallel search (each thread has its own)
        pv_move: Option<Direction>,
    ) {
        // Order moves using PV move from previous iteration
        let mut legal_moves = Self::generate_legal_moves(board, you, config);

        if !legal_moves.is_empty() {
            // Order root moves by PV only (no killers/history at root for parallel search)
            legal_moves = order_moves(legal_moves, pv_move, &KillerMoveTable::new(config), None, depth, config);
        }

        if legal_moves.is_empty() {
            info!("No legal moves available - choosing least-bad fallback");
            // When trapped, try to pick a move that's at least in-bounds
            // Priority: any in-bounds move > out-of-bounds move
            let fallback_move = Direction::all()
                .iter()
                .find(|&&dir| {
                    let next = dir.apply(&you.body[0]);
                    !Self::is_out_of_bounds(&next, board.width, board.height)
                })
                .copied()
                .unwrap_or(Direction::Up); // If all moves are out of bounds, default to Up

            shared.try_update_best(
                Self::direction_to_index(fallback_move, config),
                i32::MIN,
            );
            return;
        }

        info!(
            "Evaluating {} legal moves in parallel (1v1 alpha-beta)",
            legal_moves.len()
        );

        let our_snake_id = &you.id;
        let our_idx = board
            .snakes
            .iter()
            .position(|s| &s.id == our_snake_id)
            .unwrap_or(0);

        // Parallel evaluation of root moves
        legal_moves.par_iter().enumerate().for_each(|(_idx, &mv)| {
            // Create local killer table and history table for this subtree (each thread gets its own)
            let mut local_killers = KillerMoveTable::new(config);
            let mut local_history = HistoryTable::new(board.width as u32, board.height as u32);

            let mut child_board = board.clone();
            Self::apply_move(&mut child_board, our_idx, mv, config);

            let score = Self::alpha_beta_minimax(
                &child_board,
                our_snake_id,
                depth.saturating_sub(1),
                i32::MIN,
                i32::MAX,
                false,
                config,
                tt,
                &mut local_killers,
                &mut local_history,
            );

            // Atomic update of best move and score together (prevents race conditions)
            shared.try_update_best(Self::direction_to_index(mv, config), score);
        });

        let (_, final_score) = shared.get_best();
        info!("Parallel 1v1 search complete: best score = {}", final_score);
    }

    /// Public evaluation for analysis tools - provides detailed score breakdown
    pub fn evaluate_move_detailed(
        board: &Board,
        our_snake_id: &str,
        test_move: Direction,
        config: &Config,
    ) -> DetailedScore {
        // Apply the move to get resulting board state
        let mut test_board = board.clone();
        let our_idx = test_board.snakes.iter().position(|s| s.id == our_snake_id)
            .expect("Our snake not found");

        let snake = &test_board.snakes[our_idx];
        let head = snake.body[0];
        let new_head = match test_move {
            Direction::Up => Coord { x: head.x, y: head.y + 1 },
            Direction::Down => Coord { x: head.x, y: head.y - 1 },
            Direction::Left => Coord { x: head.x - 1, y: head.y },
            Direction::Right => Coord { x: head.x + 1, y: head.y },
        };

        // Apply move
        test_board.snakes[our_idx].body.insert(0, new_head);
        if test_board.food.contains(&new_head) {
            test_board.food.retain(|f| *f != new_head);
            test_board.snakes[our_idx].health = config.game_rules.health_on_food as i32;
            test_board.snakes[our_idx].length += 1;
        } else {
            test_board.snakes[our_idx].body.pop();
            test_board.snakes[our_idx].health = test_board.snakes[our_idx].health.saturating_sub(config.game_rules.health_loss_per_turn as i32);
        }

        // Compute individual score components
        let health = Self::compute_health_score(&test_board, our_idx, config);
        let space = Self::compute_space_score(&test_board, our_idx, &[], config);
        let control = Self::compute_control_score(&test_board, our_idx, config);
        let length = test_board.snakes[our_idx].length * config.scores.weight_length;

        let space_cache: HashMap<usize, usize> = HashMap::new();
        let attack = Self::compute_attack_score(&test_board, our_idx, config, &space_cache);

        let head_collision = if !test_board.snakes[our_idx].body.is_empty() {
            Self::check_head_collision_danger(&test_board, our_idx, test_board.snakes[our_idx].body[0], config)
        } else {
            0
        };

        let (wall_penalty, center_bias) = if !test_board.snakes[our_idx].body.is_empty() {
            let h = test_board.snakes[our_idx].body[0];
            (
                Self::compute_wall_penalty(h, test_board.width as i32, test_board.height as i32, config),
                Self::compute_center_bias(h, test_board.width as i32, test_board.height as i32, config),
            )
        } else {
            (0, 0)
        };

        let survival = if test_board.snakes[our_idx].health > 0 { 0 } else { config.scores.score_survival_penalty };

        // Weighted total
        let total = survival
            + (config.scores.score_survival_weight * survival as f32) as i32
            + (config.scores.weight_space * space as f32) as i32
            + (config.scores.weight_health * health as f32) as i32
            + (config.scores.weight_control * control as f32) as i32
            + (config.scores.weight_attack * attack as f32) as i32
            + length
            + head_collision
            + wall_penalty
            + center_bias;

        DetailedScore {
            total,
            survival,
            health,
            space,
            control,
            attack,
            length,
            head_collision,
            wall_penalty,
            center_bias,
        }
    }
}

/// Detailed score breakdown for analysis
#[derive(Debug, Clone)]
pub struct DetailedScore {
    pub total: i32,
    pub survival: i32,
    pub health: i32,
    pub space: i32,
    pub control: i32,
    pub attack: i32,
    pub length: i32,
    pub head_collision: i32,
    pub wall_penalty: i32,
    pub center_bias: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_unpack_positive_score() {
        let move_idx = 2u8; // Left
        let score = 12345i32;
        
        let packed = SharedSearchState::pack_move_score(move_idx, score);
        let (unpacked_move, unpacked_score) = SharedSearchState::unpack_move_score(packed);
        
        assert_eq!(unpacked_move, move_idx, "Move should be preserved");
        assert_eq!(unpacked_score, score, "Score should be preserved");
    }

    #[test]
    fn test_pack_unpack_negative_score() {
        let move_idx = 3u8; // Right
        let score = -54321i32;
        
        let packed = SharedSearchState::pack_move_score(move_idx, score);
        let (unpacked_move, unpacked_score) = SharedSearchState::unpack_move_score(packed);
        
        assert_eq!(unpacked_move, move_idx, "Move should be preserved");
        assert_eq!(unpacked_score, score, "Negative score should be preserved");
    }

    #[test]
    fn test_pack_unpack_min_score() {
        let move_idx = 0u8; // Up
        let score = i32::MIN;
        
        let packed = SharedSearchState::pack_move_score(move_idx, score);
        let (unpacked_move, unpacked_score) = SharedSearchState::unpack_move_score(packed);
        
        assert_eq!(unpacked_move, move_idx, "Move should be preserved");
        assert_eq!(unpacked_score, score, "i32::MIN should be preserved");
    }

    #[test]
    fn test_pack_unpack_max_score() {
        let move_idx = 1u8; // Down
        let score = i32::MAX;
        
        let packed = SharedSearchState::pack_move_score(move_idx, score);
        let (unpacked_move, unpacked_score) = SharedSearchState::unpack_move_score(packed);
        
        assert_eq!(unpacked_move, move_idx, "Move should be preserved");
        assert_eq!(unpacked_score, score, "i32::MAX should be preserved");
    }

    #[test]
    fn test_pack_unpack_all_moves() {
        // Test all possible move values (0-3)
        for move_idx in 0u8..=3 {
            let score = (move_idx as i32) * 1000 - 5000;
            
            let packed = SharedSearchState::pack_move_score(move_idx, score);
            let (unpacked_move, unpacked_score) = SharedSearchState::unpack_move_score(packed);
            
            assert_eq!(unpacked_move, move_idx, "Move {} should be preserved", move_idx);
            assert_eq!(unpacked_score, score, "Score for move {} should be preserved", move_idx);
        }
    }

    #[test]
    fn test_try_update_best_improves() {
        let state = SharedSearchState::new();

        // Initial state: move=0 (Up), score=i32::MIN
        let (move_idx, score) = state.get_best();
        assert_eq!(move_idx, 0);
        assert_eq!(score, i32::MIN);

        // Update with better score should succeed
        let result = state.try_update_best(2, 1000);
        assert!(result, "Update with better score should succeed");
        let (move_idx, score) = state.get_best();
        assert_eq!(move_idx, 2);
        assert_eq!(score, 1000);
    }

    #[test]
    fn test_try_update_best_rejects_worse() {
        let state = SharedSearchState::new();
        state.try_update_best(1, 5000);

        // Update with worse score should fail
        let result = state.try_update_best(2, 3000);
        assert!(!result, "Update with worse score should fail");
        let (move_idx, score) = state.get_best();
        assert_eq!(move_idx, 1, "Move should not change");
        assert_eq!(score, 5000, "Score should not change");
    }

    #[test]
    fn test_try_update_best_rejects_equal() {
        let state = SharedSearchState::new();
        state.try_update_best(1, 5000);

        // Update with equal score should fail
        let result = state.try_update_best(2, 5000);
        assert!(!result, "Update with equal score should fail");
        let (move_idx, score) = state.get_best();
        assert_eq!(move_idx, 1, "Move should not change");
        assert_eq!(score, 5000, "Score should not change");
    }

    #[test]
    fn test_concurrent_updates_no_mismatch() {
        use std::sync::Arc;
        use std::thread;
        
        let state = Arc::new(SharedSearchState::new());
        let mut handles = vec![];
        
        // Spawn 10 threads, each trying to update with different scores
        for i in 0..10 {
            let state_clone = Arc::clone(&state);
            let handle = thread::spawn(move || {
                let move_idx = (i % 4) as u8;
                let score = i * 1000;
                state_clone.try_update_best(move_idx, score);
            });
            handles.push(handle);
        }
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Verify final state is consistent (move and score match)
        let (final_move, final_score) = state.get_best();

        // The score should be 9000 (highest), and move should match
        assert_eq!(final_score, 9000, "Best score should be from highest update");
        assert_eq!(final_move, 1, "Best move should match the highest score (9 % 4 = 1)");
    }
}
