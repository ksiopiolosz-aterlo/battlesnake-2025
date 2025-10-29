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

/// Bound type for transposition table entries
/// Used for alpha-beta pruning optimization
#[derive(Debug, Clone, Copy, PartialEq)]
enum BoundType {
    /// Exact score (PV node)
    Exact,
    /// Lower bound (beta cutoff, actual score >= stored score)
    Lower,
    /// Upper bound (alpha cutoff, actual score <= stored score)
    Upper,
}

/// Entry in the transposition table
#[derive(Debug, Clone)]
struct TranspositionEntry {
    /// Evaluation score for this board state
    score: i32,
    /// Depth at which this state was evaluated
    depth: u8,
    /// Type of bound stored (exact, lower, or upper)
    bound_type: BoundType,
    /// Best move found at this position (for move ordering)
    best_move: Option<Direction>,
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

    /// Probes the transposition table and returns both score and best move
    pub fn probe_with_move(&self, board_hash: u64, required_depth: u8) -> Option<(i32, Option<Direction>)> {
        let table = self.table.read().ok()?;

        if let Some(entry) = table.get(&board_hash) {
            // Only use cached value if it was searched to at least the required depth
            if entry.depth >= required_depth {
                return Some((entry.score, entry.best_move));
            }
        }

        None
    }

    /// Stores an evaluation in the transposition table
    /// Performs LRU eviction if table is full
    pub fn store(&self, board_hash: u64, score: i32, depth: u8, bound_type: BoundType, best_move: Option<Direction>) {
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
                    entry.bound_type = bound_type;
                    entry.best_move = best_move;
                    entry.age = current_age;
                }
                None => {
                    // Insert new entry
                    table.insert(board_hash, TranspositionEntry {
                        score,
                        depth,
                        bound_type,
                        best_move,
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

/// Calculates Manhattan distance between two coordinates
fn manhattan_distance(a: Coord, b: Coord) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
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

/// Detects if a position is "unstable" and needs quiescence search extension
/// Unstable positions in Battlesnake:
/// 1. Head is adjacent to food (eating will change evaluation dramatically)
/// 2. Opponent heads are nearby (head-to-head collision risk)
/// 3. Health is critically low (starvation imminent)
/// 4. **NEW: Trap detection - reachable space is critically low (entrapment risk)**
fn is_position_unstable(board: &Board, our_snake_id: &str, config: &Config) -> bool {
    let our_snake = match board.snakes.iter().find(|s| &s.id == our_snake_id) {
        Some(s) if s.health > 0 => s,
        _ => return false,
    };

    let our_head = our_snake.body[0];

    // Check 1: Is head adjacent to food?
    for &food in &board.food {
        if manhattan_distance(our_head, food) == 1 {
            return true; // About to eat food
        }
    }

    // Check 2: Are opponent heads nearby? (head-to-head collision risk)
    for opponent in &board.snakes {
        if opponent.id == our_snake_id || opponent.health == 0 {
            continue;
        }

        let opp_head = opponent.body[0];
        let head_dist = manhattan_distance(our_head, opp_head);

        // If heads are 1-2 moves apart, this is tactically critical
        if head_dist <= 2 {
            return true;
        }
    }

    // Check 3: Critical health and food is nearby?
    if our_snake.health <= 15 {
        for &food in &board.food {
            if manhattan_distance(our_head, food) <= 3 {
                return true; // Starvation risk with nearby food
            }
        }
    }

    // Check 4: Trap detection - critically low reachable space
    // If we have very limited space, this is tactically critical (entrapment risk)
    // Use a quick flood fill to check available space
    let our_idx = board.snakes.iter().position(|s| &s.id == our_snake_id).unwrap_or(0);
    let required_space = our_snake.length as usize + config.scores.space_safety_margin;
    let critical_space_threshold = required_space + (required_space / 2);

    // Use early exit optimization - stop counting once we know we have enough space
    let reachable = Bot::flood_fill_bfs(board, our_head, our_idx, Some(critical_space_threshold + 1));

    // If we're within 50% of minimum required space, consider it unstable (trap forming)
    if reachable <= critical_space_threshold {
        return true; // Trap risk - extend search to find escape route
    }

    false
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

        // Create adaptive time estimator for this search
        // Starts with configured model parameters, adapts based on observed iteration times
        let mut time_estimator = AdaptiveTimeEstimator::new(
            time_params.base_iteration_time_ms,
            time_params.branching_factor,
            config.time_estimation.model_weight,
        );

        let init_elapsed = init_start.elapsed().as_micros();
        if simple_profiler::is_profiling_enabled() {
            eprintln!("[PROFILE] Initialization: {}µs", init_elapsed);
        }

        // Iterative deepening loop
        let mut current_depth = config.timing.initial_depth;
        let effective_budget = config.timing.effective_budget_ms();
        let mut previous_score: Option<i32> = None;  // Track previous iteration score for aspiration windows

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

            // Estimate time for next iteration using ADAPTIVE estimation
            // Blends observed iteration times with exponential model for accurate predictions
            // Adapts dynamically as code changes (move ordering, trap detection, etc.)
            let estimated_time = time_estimator.estimate(current_depth, num_active_snakes);

            if simple_profiler::is_profiling_enabled() {
                eprintln!("[PROFILE] Time estimation: depth={}, snakes_total={}, snakes_active={} (IDAPOS), estimated={}ms (adaptive)",
                         current_depth, num_alive_snakes, num_active_snakes, estimated_time);
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

            // Determine if we should use aspiration windows
            let use_aspiration_windows = config.aspiration_windows.enabled
                && strategy == ExecutionStrategy::Sequential
                && num_alive_snakes == config.strategy.min_snakes_for_1v1
                && previous_score.is_some();

            // Execute search with strategy-specific parameters
            match strategy {
                ExecutionStrategy::Sequential => {
                    if use_aspiration_windows {
                        let prev_score = previous_score.unwrap();
                        let window_size = config.aspiration_windows.initial_window_size;
                        let mut alpha = prev_score.saturating_sub(window_size);
                        let mut beta = prev_score.saturating_add(window_size);

                        info!("Using aspiration window: [{}, {}] (previous score: {})", alpha, beta, prev_score);

                        // First search with narrow window
                        Self::sequential_search(board, you, current_depth, &shared, config, &tt, &mut killers, &mut history, pv_move, alpha, beta);

                        // Check if we failed outside the window
                        let (_, result_score) = shared.get_best();

                        if result_score <= alpha {
                            // Fail-low: re-search with lower bound at -∞
                            info!("Aspiration window fail-low ({} <= {}), re-searching with wider window", result_score, alpha);
                            alpha = i32::MIN;
                            Self::sequential_search(board, you, current_depth, &shared, config, &tt, &mut killers, &mut history, pv_move, alpha, beta);

                            let (_, retry_score) = shared.get_best();
                            if retry_score >= beta {
                                // Also failed high on retry, do full window search
                                info!("Retry also failed high ({} >= {}), searching with full window", retry_score, beta);
                                Self::sequential_search(board, you, current_depth, &shared, config, &tt, &mut killers, &mut history, pv_move, i32::MIN, i32::MAX);
                            }
                        } else if result_score >= beta {
                            // Fail-high: re-search with upper bound at +∞
                            info!("Aspiration window fail-high ({} >= {}), re-searching with wider window", result_score, beta);
                            beta = i32::MAX;
                            Self::sequential_search(board, you, current_depth, &shared, config, &tt, &mut killers, &mut history, pv_move, alpha, beta);

                            let (_, retry_score) = shared.get_best();
                            if retry_score <= alpha {
                                // Also failed low on retry, do full window search
                                info!("Retry also failed low ({} <= {}), searching with full window", retry_score, alpha);
                                Self::sequential_search(board, you, current_depth, &shared, config, &tt, &mut killers, &mut history, pv_move, i32::MIN, i32::MAX);
                            }
                        }
                    } else {
                        // No aspiration windows, use full window
                        Self::sequential_search(board, you, current_depth, &shared, config, &tt, &mut killers, &mut history, pv_move, i32::MIN, i32::MAX);
                    }
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

            // Record observation for adaptive time estimation
            // This teaches the estimator about actual iteration times, making future estimates more accurate
            time_estimator.record_observation(current_depth, iteration_elapsed as f64);

            // Extract best move and score from this iteration
            let (best_move_idx, best_score) = shared.get_best();
            pv_move = Some(Self::index_to_direction(best_move_idx, config));
            previous_score = Some(best_score);  // Store for next iteration's aspiration window

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
        alpha: i32,
        beta: i32,
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
                // Use alpha-beta for 1v1 with aspiration window
                Self::alpha_beta_minimax(
                    &child_board,
                    our_snake_id,
                    depth.saturating_sub(1),
                    1,  // One ply down from root after applying move
                    alpha,
                    beta,
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
                    1, // One ply down from root
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
    ///
    /// # Performance Optimization
    /// If `early_exit_threshold` is provided, the search terminates early once
    /// that many cells are found. This is useful when we only need to know if
    /// "enough" space exists (e.g., checking if opponent is trapped).
    fn flood_fill_bfs(
        board: &Board,
        start: Coord,
        _snake_idx: usize,
        early_exit_threshold: Option<usize>,
    ) -> usize {
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
            // Early exit optimization: if we've found enough space, stop searching
            if let Some(threshold) = early_exit_threshold {
                if visited.len() >= threshold {
                    return visited.len();
                }
            }

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
    fn adversarial_flood_fill(board: &Board, active_snakes: &[usize]) -> Vec<Option<usize>> {
        let _prof = simple_profiler::ProfileGuard::new("adversarial_flood_fill");

        let size = (board.width * board.height as i32) as usize;
        let mut control_map: Vec<Option<usize>> = vec![None; size];
        let mut distance_map: Vec<Option<usize>> = vec![None; size];

        // Determine which snakes to process
        let process_all = active_snakes.is_empty();

        // Helper to convert Coord to flat array index
        let coord_to_idx = |c: &Coord| (c.y * board.width + c.x) as usize;

        // Mark snake bodies as obstacles controlled by their owner
        if process_all {
            for (idx, snake) in board.snakes.iter().enumerate() {
                if snake.health <= 0 {
                    continue;
                }
                for &seg in &snake.body {
                    control_map[coord_to_idx(&seg)] = Some(idx);
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
                    control_map[coord_to_idx(&seg)] = Some(idx);
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
                let head_idx = coord_to_idx(&snake.body[0]);
                queue.push_back((snake.body[0], *idx, 0));
                distance_map[head_idx] = Some(0);
            }
        }

        while let Some((pos, owner, dist)) = queue.pop_front() {
            let pos_idx = coord_to_idx(&pos);

            // Skip if already claimed by another snake at same or closer distance
            if let Some(existing_dist) = distance_map[pos_idx] {
                if existing_dist < dist {
                    continue;
                }
            }

            // Claim cell if not already controlled
            if control_map[pos_idx].is_none() {
                control_map[pos_idx] = Some(owner);
            }

            for dir in Direction::all().iter() {
                let next = dir.apply(&pos);

                if next.x < 0
                    || next.x >= board.width
                    || next.y < 0
                    || next.y >= board.height as i32
                {
                    continue;
                }

                let next_idx = coord_to_idx(&next);
                let next_dist = dist + 1;

                // Only explore if we can reach it faster (not equal distance - prevents re-exploration)
                let should_explore = match distance_map[next_idx] {
                    Some(existing_dist) => next_dist < existing_dist,
                    None => true,
                };

                if should_explore && control_map[next_idx].is_none() {
                    distance_map[next_idx] = Some(next_dist);
                    queue.push_back((next, owner, next_dist));
                }
            }
        }

        control_map
    }

    /// Helper to compute control score from pre-computed map
    fn compute_control_score_from_map(
        control_map: &[Option<usize>],
        snake_idx: usize,
        config: &Config,
    ) -> i32 {
        let our_cells = control_map
            .iter()
            .filter(|cell| cell.map_or(false, |owner| owner == snake_idx))
            .count();
        let total_free = control_map.iter().filter(|cell| cell.is_some()).count();

        if total_free == 0 {
            return 0;
        }

        ((our_cells as f32 / total_free as f32) * config.scores.territory_scale_factor) as i32
    }

    /// Computes health and food score for a snake
    /// Returns higher score for closer food when health is low
    /// Adds extra urgency when in health disadvantage vs opponents
    fn compute_health_score(
        board: &Board,
        snake_idx: usize,
        active_snakes: &[usize],
        config: &Config,
    ) -> i32 {
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
            .map(|&food| manhattan_distance(head, food))
            .min()
            .unwrap_or(config.scores.default_food_distance);

        // V8.1 CRITICAL FIX: Reward states where we JUST ATE food (health==100)
        // Previous bug: Only rewarded being ADJACENT to food, not EATING it
        // Result: Search tree never saw value in moves that acquire food
        let just_ate_food = snake.health == 100;

        // Immediate food bonus: strongly incentivize grabbing adjacent food
        // This overrides normal distance penalty to ensure we eat when safe
        // V6 fix: Check escape routes before eating food to avoid "grab and die" pattern
        // V7.1 fix: Add urgency multiplier for distance-1 food when health is low
        // V8.1 fix: Also apply bonus when we just ate food (health==100)
        if (nearest_food_dist <= config.scores.immediate_food_distance && snake.health < 100) || just_ate_food {
            // Find the nearest food position
            let nearest_food = board
                .food
                .iter()
                .min_by_key(|&food| manhattan_distance(head, *food))
                .copied();

            // V8: Use smarter food safety check that predicts post-eating traps
            let is_food_safe = if let Some(food_pos) = nearest_food {
                Self::is_food_actually_safe(board, food_pos, snake_idx, active_snakes, config)
            } else {
                false
            };

            // V8: Hierarchical urgency multiplier with configurable cap
            // Prevents unlimited multipliers from dominating ALL other factors
            // survival_max_multiplier (default 1000.0) ensures survival layer dominates tactical,
            // but safety vetoes can still block dangerous moves
            // V8.1: Apply max multiplier when we just ate food (health==100) to reward acquisition
            let urgency_multiplier = if just_ate_food {
                // Just ate food - strongly reward this state!
                config.scores.survival_max_multiplier
            } else if nearest_food_dist == 1 && is_food_safe {
                // Adjacent safe food - use urgency based on health threshold
                if snake.health < config.scores.survival_health_threshold as i32 {
                    // CRITICAL survival mode: max multiplier
                    config.scores.survival_max_multiplier
                } else if snake.health < 70 {
                    // Moderate urgency: 10% of max
                    config.scores.survival_max_multiplier * 0.1
                } else {
                    // Low urgency: 1% of max
                    config.scores.survival_max_multiplier * 0.01
                }
            } else {
                1.0  // Normal: Food at distance 2 or food is guarded
            };

            // Check escape routes after eating this food
            if let Some(food_pos) = nearest_food {
                let escape_routes = Self::count_escape_routes_after_eating(board, snake_idx, food_pos);

                // If we'd have insufficient escape routes after eating, penalize
                // V7: Scale penalty by health urgency (lower health = more willing to risk)
                if escape_routes < config.scores.escape_route_min {
                    let penalty = if config.scores.escape_route_penalty_health_scale {
                        let health_urgency = (100.0 - snake.health as f32) / 100.0;
                        // At low health (0-30): penalty *= 0.5 (more aggressive)
                        // At high health (70-100): penalty *= 1.0 (more conservative)
                        (config.scores.escape_route_penalty_base as f32 * (0.5 + health_urgency * 0.5)) as i32
                    } else {
                        config.scores.escape_route_penalty_base
                    };

                    // V7: Add safe food bonus for central food
                    let center_x = (board.width / 2) as i32;
                    let center_y = (board.height / 2) as i32;
                    let center = Coord { x: center_x, y: center_y };
                    let dist_from_center = manhattan_distance(food_pos, center);

                    let safe_food_bonus = if dist_from_center <= config.scores.safe_food_center_threshold {
                        config.scores.safe_food_bonus
                    } else {
                        0
                    };

                    // Apply urgency multiplier to the total bonus
                    let base_bonus = config.scores.immediate_food_bonus + penalty + safe_food_bonus;
                    return (base_bonus as f32 * urgency_multiplier) as i32;
                }
            }

            // No escape route penalty - just apply urgency multiplier to base bonus
            return (config.scores.immediate_food_bonus as f32 * urgency_multiplier) as i32;
        }

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

        // Check if we're in a health disadvantage against nearby opponents (IDAPOS-filtered)
        // This helps break out of "death dance" scenarios where both snakes circle endlessly
        // Only consider ACTIVE opponents that are close enough to be an immediate threat
        let max_nearby_opponent_health = active_snakes
            .iter()
            .filter_map(|&idx| {
                if idx == snake_idx || idx >= board.snakes.len() {
                    return None;
                }
                let s = &board.snakes[idx];
                if s.health <= 0 || s.body.is_empty() {
                    return None;
                }
                // Only consider opponents within threat range
                let dist = manhattan_distance(head, s.body[0]);
                if dist <= config.scores.health_threat_distance {
                    Some(s.health)
                } else {
                    None
                }
            })
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
            let distance = manhattan_distance(our_head, opponent.body[0]);
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
        Self::compute_control_score_from_map(&control_map, snake_idx, config)
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
                let dist = manhattan_distance(our_head, opponent.body[0]);
                if dist <= config.scores.attack_head_to_head_distance {
                    attack += config.scores.attack_head_to_head_bonus;
                }
            }

            // Trap potential - opponent has limited space (use cache if available)
            // Early exit threshold: if opponent has enough space, we don't need exact count
            let trap_threshold = opponent.length as usize + config.scores.attack_trap_margin;
            let opp_space = space_cache
                .get(&idx)
                .copied()
                .unwrap_or_else(|| {
                    Self::flood_fill_bfs(board, opponent.body[0], idx, Some(trap_threshold + 1))
                });
            if opp_space < trap_threshold {
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
    /// Health-aware: scales penalty down when health is low to allow edge food acquisition
    /// Examples (at full health): distance=0 → -500, distance=1 → -250, distance=2 → -167
    /// Caps at distance >= 3 (safe distance)
    fn compute_wall_penalty(pos: Coord, width: i32, height: i32, health: i32, config: &Config) -> i32 {
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

        // Health-aware scaling: reduce penalty when hungry to allow edge food acquisition
        let health_factor = if health < 30 {
            0.5  // 50% penalty when health < 30
        } else if health < 60 {
            0.75  // 75% penalty when health < 60
        } else {
            1.0  // Full penalty when healthy
        };

        let base_penalty = config.scores.wall_penalty_base as f32 * health_factor;
        -(base_penalty / (dist_to_wall + 1) as f32) as i32
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

    /// Computes corner danger penalty - exponential penalty as snake approaches corners
    /// V5 fix: Game 03 died at (10,10) after eating corner food - need to avoid corners
    fn compute_corner_danger(pos: Coord, width: i32, height: i32, config: &Config) -> i32 {
        // Distance to nearest corner
        let corners = [
            (0, 0),
            (0, height - 1),
            (width - 1, 0),
            (width - 1, height - 1),
        ];

        let min_corner_dist = corners
            .iter()
            .map(|&(cx, cy)| (pos.x - cx).abs() + (pos.y - cy).abs())
            .min()
            .unwrap_or(999);

        // Apply exponential penalty when within threshold
        if min_corner_dist <= config.scores.corner_danger_threshold {
            // Exponential: at corner (0) = -5000, at distance 1 = -2500, at distance 2 = -1667, at distance 3 = -1250
            -(config.scores.corner_danger_base / (min_corner_dist + 1))
        } else {
            0
        }
    }

    /// Counts escape routes (legal moves) after eating food at a position
    /// V6 fix: Prevents "grab food and die" pattern from V5 Game 03
    fn count_escape_routes_after_eating(board: &Board, snake_idx: usize, food_pos: Coord) -> i32 {
        if snake_idx >= board.snakes.len() {
            return 0;
        }

        let snake = &board.snakes[snake_idx];
        if snake.body.is_empty() {
            return 0;
        }

        // Simulate eating the food: head moves to food_pos, body grows
        let new_head = food_pos;
        let mut new_body = vec![new_head];
        new_body.extend_from_slice(&snake.body);
        // Body grows when eating food (don't remove tail)

        // Count legal moves from the new position
        let directions = [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ];

        let mut legal_moves = 0;
        for dir in &directions {
            let next_pos = match dir {
                Direction::Up => Coord {
                    x: new_head.x,
                    y: new_head.y + 1,
                },
                Direction::Down => Coord {
                    x: new_head.x,
                    y: new_head.y - 1,
                },
                Direction::Left => Coord {
                    x: new_head.x - 1,
                    y: new_head.y,
                },
                Direction::Right => Coord {
                    x: new_head.x + 1,
                    y: new_head.y,
                },
            };

            // Check bounds
            if next_pos.x < 0
                || next_pos.x >= board.width as i32
                || next_pos.y < 0
                || next_pos.y >= board.height as i32
            {
                continue;
            }

            // Check if we'd hit our new body (excluding tail which will move)
            let body_collision = new_body
                .iter()
                .take(new_body.len().saturating_sub(1)) // Exclude tail
                .any(|&segment| segment == next_pos);

            if body_collision {
                continue;
            }

            // Check if we'd hit other snakes (excluding their tails)
            let mut other_collision = false;
            for (idx, other_snake) in board.snakes.iter().enumerate() {
                if idx == snake_idx || other_snake.health == 0 {
                    continue;
                }

                let other_body_check = &other_snake.body[..other_snake.body.len().saturating_sub(1)];
                if other_body_check.contains(&next_pos) {
                    other_collision = true;
                    break;
                }
            }

            if other_collision {
                continue;
            }

            // This move is legal
            legal_moves += 1;
        }

        legal_moves
    }

    /// V8: Smarter food safety check - predicts opponent behavior and post-eating traps
    /// Fixes V7.2 issue where food marked "SAFE" but opponent could trap us after eating
    ///
    /// Checks:
    /// 1. Can opponent reach food before/simultaneously?
    /// 2. Does opponent WANT this food? (hungry or competitive length)
    /// 3. Can opponent cut us off AFTER we eat? (escape route analysis)
    ///
    /// Example failure (V7.2 Turn 36):
    /// - Food at (3,10), our head at (3,9) - distance 1
    /// - Opponent at distance 2, length 7 vs our length 3
    /// - Old code: marked "SAFE" (opponent not within distance 2 of food)
    /// - Reality: Opponent traps us against top wall after we eat
    /// - V8 fix: Checks escape routes POST-eating with opponent in pursuit
    fn is_food_actually_safe(
        board: &Board,
        food_pos: Coord,
        snake_idx: usize,
        active_snakes: &[usize],
        config: &Config,
    ) -> bool {
        if snake_idx >= board.snakes.len() {
            return false;
        }

        let our_snake = &board.snakes[snake_idx];
        if our_snake.body.is_empty() {
            return false;
        }

        let our_head = our_snake.body[0];
        let our_dist = manhattan_distance(our_head, food_pos);

        // Check each ACTIVE opponent (IDAPOS-filtered)
        for &opp_idx in active_snakes {
            if opp_idx == snake_idx || opp_idx >= board.snakes.len() {
                continue;
            }

            let opp = &board.snakes[opp_idx];
            if opp.health <= 0 || opp.body.is_empty() {
                continue;
            }

            let opp_head = opp.body[0];
            let opp_dist = manhattan_distance(opp_head, food_pos);

            // Check 1: Can they arrive first or simultaneously?
            if opp_dist <= our_dist {
                // Will they want this food?
                // - Hungry snakes (health < 60) will contest any food
                // - Competitive snakes (similar/smaller length) will contest for growth
                let is_hungry = opp.health < 60;
                let is_competitive = opp.length <= our_snake.length + 2; // Within 2 length

                if is_hungry || is_competitive {
                    return false; // They'll contest it
                }
            }

            // Check 2: Can they cut us off AFTER we eat?
            // If opponent is close and has length advantage, they can pressure us
            if opp_dist <= our_dist + 2 && opp.length >= our_snake.length {
                // Count escape routes after eating, assuming opponent moves toward us
                let escape_count = Self::count_escape_routes_after_eating(board, snake_idx, food_pos);

                // If we'd have insufficient escape routes, opponent can trap us
                // Note: config.scores.escape_route_min is typically 2
                if escape_count < config.scores.escape_route_min {
                    return false; // They can trap us post-eating
                }

                // Additional check: Is food near a wall/corner? (more dangerous)
                let dist_to_wall = food_pos.x.min(food_pos.y)
                    .min(board.width as i32 - 1 - food_pos.x)
                    .min(board.height as i32 - 1 - food_pos.y);

                // Food within 1 cell of wall + nearby opponent with advantage = DANGER
                if dist_to_wall <= 1 && opp.length > our_snake.length {
                    return false; // Wall trap risk too high
                }
            }
        }

        true // Safe from all active opponents
    }

    /// Computes length advantage bonus to encourage growth
    /// V5 fix: Bot stayed small (length 6) while opponents grew (length 19)
    fn compute_length_advantage(board: &Board, snake_idx: usize, config: &Config) -> i32 {
        let our_length = board.snakes[snake_idx].length;

        // Get opponent lengths (alive snakes only, excluding ourselves)
        let opponent_lengths: Vec<i32> = board
            .snakes
            .iter()
            .enumerate()
            .filter(|(idx, s)| *idx != snake_idx && s.health > 0)
            .map(|(_, s)| s.length)
            .collect();

        if opponent_lengths.is_empty() {
            return 0; // No opponents, no bonus
        }

        // Calculate median opponent length
        let mut sorted_lengths = opponent_lengths.clone();
        sorted_lengths.sort_unstable();
        let median_length = if sorted_lengths.len() % 2 == 0 {
            (sorted_lengths[sorted_lengths.len() / 2 - 1] + sorted_lengths[sorted_lengths.len() / 2]) / 2
        } else {
            sorted_lengths[sorted_lengths.len() / 2]
        };

        // Bonus for being longer than median, penalty for being shorter
        let length_diff = our_length - median_length;
        length_diff * config.scores.length_advantage_bonus
    }

    /// V8: Computes growth urgency based on opponent lengths (IDAPOS-filtered)
    /// Strongly incentivizes growth when significantly shorter than opponents
    /// Example failure (V7.2 Turn 36): Our length 3 vs opponent length 7 = 4 unit gap
    /// With growth_urgency_per_length=500, this generates +2000 bonus to close gap
    fn compute_growth_urgency(
        board: &Board,
        snake_idx: usize,
        active_snakes: &[usize],
        config: &Config,
    ) -> i32 {
        if snake_idx >= board.snakes.len() {
            return 0;
        }

        let our_snake = &board.snakes[snake_idx];
        let our_length = our_snake.length;
        let our_health = our_snake.health;

        // Find shortest ACTIVE opponent (IDAPOS-filtered)
        let min_opp_length = active_snakes
            .iter()
            .filter_map(|&idx| {
                if idx == snake_idx || idx >= board.snakes.len() {
                    return None;
                }
                let s = &board.snakes[idx];
                if s.health > 0 {
                    Some(s.length as i32)
                } else {
                    None
                }
            })
            .min()
            .unwrap_or(100); // If no opponents, assume we're fine

        // If we're shorter than smallest opponent, GROW URGENTLY
        if (our_length as i32) < min_opp_length {
            let gap = min_opp_length - (our_length as i32);
            return gap * config.scores.growth_urgency_per_length;
        }

        // If we're longest and healthy, grow conservatively
        if (our_length as i32) > min_opp_length && our_health > 60 {
            return config.scores.growth_bonus_when_ahead;
        }

        0
    }

    /// V7: Detects tail-chasing pattern (body segments clustering near head)
    /// NUANCED: Only applies penalty when opponents are nearby (indicating active trap risk)
    /// Prevents self-trapping but allows tail-chasing as valid survival tactic when isolated
    /// Uses IDAPOS-filtered active_snakes to check for nearby opponents
    fn compute_tail_chasing_penalty(
        board: &Board,
        snake_idx: usize,
        active_snakes: &[usize],
        config: &Config,
    ) -> i32 {
        if snake_idx >= board.snakes.len() {
            return 0;
        }

        let snake = &board.snakes[snake_idx];
        if snake.body.len() < 4 {
            return 0; // Need minimum length to form a loop
        }

        let head = snake.body[0];

        // NUANCE: Check if any active opponent is nearby (IDAPOS-filtered)
        // If no opponents nearby, tail-chasing is a valid survival tactic (doesn't risk trap)
        let has_nearby_opponent = active_snakes
            .iter()
            .filter(|&&idx| idx != snake_idx && idx < board.snakes.len())
            .any(|&idx| {
                let opponent = &board.snakes[idx];
                if opponent.health <= 0 || opponent.body.is_empty() {
                    return false;
                }
                let opp_head = opponent.body[0];
                manhattan_distance(head, opp_head) <= config.scores.tail_chasing_opponent_distance
            });

        // If no opponents nearby, tail-chasing is safe (no penalty)
        if !has_nearby_opponent {
            return 0;
        }

        // Count body segments within detection distance of head (excluding neck)
        let nearby_segments = snake.body[2..]
            .iter()
            .filter(|&&seg| {
                manhattan_distance(head, seg) <= config.scores.tail_chasing_detection_distance
            })
            .count();

        if nearby_segments == 0 {
            return 0;
        }

        // Exponential penalty: more nearby segments = tighter loop = higher risk
        let penalty_base = nearby_segments as f32;
        let penalty = penalty_base.powf(config.scores.tail_chasing_penalty_exponent)
            * config.scores.tail_chasing_penalty_per_segment as f32;

        -(penalty as i32)
    }

    /// Helper: Flood fill that returns HashSet of reachable positions
    /// Uses IDAPOS-filtered active snakes for collision detection (consistent with space control)
    fn flood_fill_for_articulation(
        board: &Board,
        start: Coord,
        snake_idx: usize,
        active_snakes: &[usize],
    ) -> HashSet<Coord> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(start);
        visited.insert(start);

        while let Some(pos) = queue.pop_front() {
            for &dir in &[Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
                let next = match dir {
                    Direction::Up => Coord { x: pos.x, y: pos.y + 1 },
                    Direction::Down => Coord { x: pos.x, y: pos.y - 1 },
                    Direction::Left => Coord { x: pos.x - 1, y: pos.y },
                    Direction::Right => Coord { x: pos.x + 1, y: pos.y },
                };

                // Check bounds
                if next.x < 0 || next.x >= board.width as i32 ||
                   next.y < 0 || next.y >= board.height as i32 {
                    continue;
                }

                if visited.contains(&next) {
                    continue;
                }

                // IDAPOS: Only check collision with active (nearby) snakes
                let blocked = active_snakes.iter().any(|&idx| {
                    if idx >= board.snakes.len() {
                        return false;
                    }
                    let snake = &board.snakes[idx];
                    snake.health > 0 && snake.body.contains(&next)
                });

                if !blocked {
                    visited.insert(next);
                    queue.push_back(next);
                }
            }
        }

        visited
    }

    /// V7: Detects articulation points in reachable space
    /// Articulation points are positions whose removal would disconnect the space
    /// These are narrow passages that create high trap risk
    /// Uses IDAPOS-filtered active_snakes for efficient collision detection
    fn compute_articulation_point_penalty(
        board: &Board,
        snake_idx: usize,
        active_snakes: &[usize],
        config: &Config,
    ) -> i32 {
        if !config.scores.articulation_point_enabled {
            return 0;
        }

        if snake_idx >= board.snakes.len() {
            return 0;
        }

        let snake = &board.snakes[snake_idx];
        if snake.body.is_empty() {
            return 0;
        }

        let head = snake.body[0];

        // Flood fill to get reachable space (returns HashSet)
        // Uses IDAPOS-filtered snakes for collision checks
        let reachable = Self::flood_fill_for_articulation(board, head, snake_idx, active_snakes);

        if reachable.len() < 4 {
            return 0; // Too small to have meaningful articulation points
        }

        // Check if current head position is an articulation point
        // Method: Remove head from reachable set and check connectivity
        let is_articulation = Self::is_articulation_point(head, &reachable);

        if is_articulation {
            config.scores.articulation_point_penalty
        } else {
            0
        }
    }

    /// Helper: Check if a position is an articulation point
    fn is_articulation_point(
        pos: Coord,
        reachable: &HashSet<Coord>,
    ) -> bool {
        // Get neighbors that are in reachable set
        let neighbors: Vec<Coord> = [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ]
        .iter()
        .filter_map(|&dir| {
            let next = match dir {
                Direction::Up => Coord { x: pos.x, y: pos.y + 1 },
                Direction::Down => Coord { x: pos.x, y: pos.y - 1 },
                Direction::Left => Coord { x: pos.x - 1, y: pos.y },
                Direction::Right => Coord { x: pos.x + 1, y: pos.y },
            };
            if reachable.contains(&next) && next != pos {
                Some(next)
            } else {
                None
            }
        })
        .collect();

        if neighbors.len() < 2 {
            return false; // Not enough neighbors to be articulation point
        }

        // Check if removing this position disconnects the neighbors
        // Do BFS from first neighbor without going through pos
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(neighbors[0]);
        visited.insert(neighbors[0]);
        visited.insert(pos); // Block the articulation point candidate

        while let Some(current) = queue.pop_front() {
            for &dir in &[
                Direction::Up,
                Direction::Down,
                Direction::Left,
                Direction::Right,
            ] {
                let next = match dir {
                    Direction::Up => Coord { x: current.x, y: current.y + 1 },
                    Direction::Down => Coord { x: current.x, y: current.y - 1 },
                    Direction::Left => Coord { x: current.x - 1, y: current.y },
                    Direction::Right => Coord { x: current.x + 1, y: current.y },
                };
                if reachable.contains(&next) && !visited.contains(&next) {
                    visited.insert(next);
                    queue.push_back(next);
                }
            }
        }

        // If not all neighbors are reachable, pos is an articulation point
        neighbors.iter().any(|n| !visited.contains(n))
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
        depth_from_root: u8,
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
                    // No early exit threshold - cache needs exact count for multiple components
                    space_cache.insert(idx, Self::flood_fill_bfs(board, snake.body[0], idx, None));
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
            let active_list = active_snakes.unwrap_or(&[]);
            let health = Self::compute_health_score(board, idx, active_list, config);

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

            // Wall proximity penalty, center bias, and corner danger
            let (wall_penalty, center_bias, corner_danger) = if !snake.body.is_empty() {
                let head = snake.body[0];
                (
                    Self::compute_wall_penalty(head, board.width as i32, board.height as i32, snake.health, config),
                    Self::compute_center_bias(head, board.width as i32, board.height as i32, config),
                    Self::compute_corner_danger(head, board.width as i32, board.height as i32, config),
                )
            } else {
                (0, 0, 0)
            };

            // Length advantage bonus
            let length_advantage = Self::compute_length_advantage(board, idx, config);

            // V8: Growth urgency - incentivize growth when shorter than opponents
            // Uses IDAPOS-filtered active snakes to compare lengths efficiently
            let growth_urgency = if is_active {
                let active_list = active_snakes.unwrap_or(&[]);
                Self::compute_growth_urgency(board, idx, active_list, config)
            } else {
                0  // Skip for non-active snakes
            };

            // V7: Tail-chasing detection (nuanced - only when opponents nearby)
            // Uses IDAPOS-filtered active snakes to check for nearby opponents
            let tail_chasing_penalty = if is_active {
                let active_list = active_snakes.unwrap_or(&[]);
                Self::compute_tail_chasing_penalty(board, idx, active_list, config)
            } else {
                0  // Skip tail-chasing check for non-active snakes
            };

            // V7: Articulation point detection (narrow passage risk)
            // Uses IDAPOS-filtered active snakes for efficient collision detection
            let articulation_penalty = if is_active {
                let active_list = active_snakes.unwrap_or(&[]);
                Self::compute_articulation_point_penalty(board, idx, active_list, config)
            } else {
                0  // Skip expensive articulation check for non-active snakes
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
                + center_bias
                + corner_danger
                + length_advantage + growth_urgency
                + tail_chasing_penalty
                + articulation_penalty;
        }

        // Apply survival penalty if our snake is dead
        if let Some(our_idx) = board.snakes.iter().position(|s| s.id == our_snake_id) {
            if board.snakes[our_idx].health <= 0 {
                scores[our_idx] = config.scores.score_survival_penalty;
            }
        }

        // V7.2: Apply temporal discounting - future scores less confident, weighted lower
        // discount = (0.95 ^ depth): depth 0 = 1.0, depth 5 = 0.77, depth 10 = 0.60
        if depth_from_root > 0 {
            let discount = config.scores.temporal_discount_factor.powi(depth_from_root as i32);
            for score in &mut scores {
                *score = (*score as f32 * discount) as i32;
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

        // Calculate locality threshold with maximum cap
        // Base threshold grows with depth, but cap prevents over-inclusion at high depths
        let base_threshold = config.idapos.head_distance_multiplier * remaining_depth as i32;
        let locality_threshold = std::cmp::min(base_threshold, config.idapos.max_locality_distance);

        for (idx, snake) in board.snakes.iter().enumerate() {
            if idx == our_idx || snake.health <= 0 {
                continue;
            }

            // Check head distance
            let head_dist = manhattan_distance(our_head, snake.body[0]);
            if head_dist <= locality_threshold {
                active.push(idx);
                continue;
            }

            // Check any body segment distance (using same capped threshold)
            for &segment in &snake.body {
                if manhattan_distance(our_head, segment) <= locality_threshold {
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
        depth_from_root: u8,
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
            depth_from_root,
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
        depth_from_root: u8,
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

        // Check for terminal state first
        if Self::is_terminal(board, our_snake_id, config) {
            let eval = Self::evaluate_state(board, our_snake_id, config, Some(&active_snakes), depth_from_root);
            tt.store(board_hash, eval.for_player(our_idx), depth, BoundType::Exact, None);
            return eval;
        }

        // At depth 0, check if position is unstable (quiescence extension)
        if depth == 0 {
            if is_position_unstable(board, our_snake_id, config) {
                // Extend search by 1 ply for tactically critical positions
                // Recompute active snakes for extended depth
                return Self::maxn_search(
                    board,
                    our_snake_id,
                    1, // Extended depth
                    depth_from_root + 1, // Going one ply deeper
                    current_player_idx,
                    config,
                    tt,
                    killers,
                    history,
                );
            }

            // Stable position at depth 0, evaluate normally
            let eval = Self::evaluate_state(board, our_snake_id, config, Some(&active_snakes), depth_from_root);
            tt.store(board_hash, eval.for_player(our_idx), depth, BoundType::Exact, None);
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
                depth_from_root,
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
                return Self::maxn_search(&advanced_board, our_snake_id, depth - 1, depth_from_root + 1, our_idx, config, tt, killers, history);
            } else {
                // Continue with next player at same depth
                return Self::maxn_search(board, our_snake_id, depth, depth_from_root, next, config, tt, killers, history);
            }
        }

        // Generate legal moves for current player
        let mut moves = Self::generate_legal_moves(board, &board.snakes[current_player_idx], config);

        if moves.is_empty() {
            // No legal moves - mark snake as dead and continue
            let mut dead_board = board.clone();
            dead_board.snakes[current_player_idx].health = 0;
            let next = (current_player_idx + 1) % board.snakes.len();
            return Self::maxn_search(&dead_board, our_snake_id, depth, depth_from_root, next, config, tt, killers, history);
        }

        // Try to get best move from transposition table for move ordering
        let tt_best_move = tt.probe_with_move(board_hash, depth).and_then(|(_, mv)| mv);

        // Order moves using TT move > killers > history heuristic
        let current_pos = &board.snakes[current_player_idx].body[0];
        moves = order_moves(moves, tt_best_move, killers, Some((history, current_pos)), depth, config);

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
                Self::maxn_search(&child_board, our_snake_id, depth - 1, depth_from_root + 1, our_idx, config, tt, killers, history)
            } else {
                // Continue with next player at same depth
                Self::maxn_search(&child_board, our_snake_id, depth, depth_from_root, next, config, tt, killers, history)
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
        tt.store(board_hash, best_tuple.for_player(our_idx), depth, BoundType::Exact, None);
        best_tuple
    }

    /// Alpha-beta minimax for 2-player zero-sum games (1v1)
    /// More efficient than MaxN when only two snakes remain
    fn alpha_beta_minimax(
        board: &Board,
        our_snake_id: &str,
        depth: u8,
        depth_from_root: u8,
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

        // Check for terminal state first
        if Self::is_terminal(board, our_snake_id, config) {
            let scores = Self::evaluate_state(board, our_snake_id, config, None, depth_from_root);
            let our_idx = board
                .snakes
                .iter()
                .position(|s| &s.id == our_snake_id)
                .unwrap_or(0);
            let score = scores.for_player(our_idx);
            tt.store(board_hash, score, depth, BoundType::Exact, None);
            return score;
        }

        // At depth 0, check if position is unstable (quiescence extension)
        if depth == 0 {
            if is_position_unstable(board, our_snake_id, config) {
                // Extend search by 1 ply for tactically critical positions
                // This helps avoid horizon effect on food eating and collisions
                return Self::alpha_beta_minimax(
                    board,
                    our_snake_id,
                    1, // Extended depth
                    depth_from_root + 1,  // Extending search, increment depth from root
                    alpha,
                    beta,
                    is_max,
                    config,
                    tt,
                    killers,
                    history,
                );
            }

            // Stable position at depth 0, evaluate normally
            let scores = Self::evaluate_state(board, our_snake_id, config, None, depth_from_root);
            let our_idx = board
                .snakes
                .iter()
                .position(|s| &s.id == our_snake_id)
                .unwrap_or(0);
            let score = scores.for_player(our_idx);
            tt.store(board_hash, score, depth, BoundType::Exact, None);
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
            let scores = Self::evaluate_state(board, our_snake_id, config, None, depth_from_root);
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
                depth_from_root,  // Same depth, no state change
                alpha,
                beta,
                !is_max,
                config,
                tt,
                killers,
                history,
            );
        }

        // Try to get best move from transposition table for move ordering
        let tt_best_move = tt.probe_with_move(board_hash, depth).and_then(|(_, mv)| mv);

        // Order moves using TT move > killers > history heuristic
        let current_pos = &board.snakes[player_idx].body[0];
        moves = order_moves(moves, tt_best_move, killers, Some((history, current_pos)), depth, config);

        if is_max {
            let mut max_eval = i32::MIN;
            let mut best_move: Option<Direction> = None;
            let mut had_cutoff = false;

            for mv in moves {
                let mut child_board = board.clone();
                Self::apply_move(&mut child_board, player_idx, mv, config);
                Self::advance_game_state(&mut child_board);

                let eval = Self::alpha_beta_minimax(
                    &child_board,
                    our_snake_id,
                    depth - 1,
                    depth_from_root + 1,  // One ply deeper
                    alpha,
                    beta,
                    false,
                    config,
                    tt,
                    killers,
                    history,
                );

                if eval > max_eval {
                    max_eval = eval;
                    best_move = Some(mv);
                }

                alpha = alpha.max(eval);
                if beta <= alpha {
                    // Beta cutoff: record this move as a killer and update history
                    killers.record_killer(depth, mv, config);
                    history.update(current_pos, mv, depth, true);
                    simple_profiler::record_alpha_beta_cutoff();
                    had_cutoff = true;
                    break;
                }
            }

            // Store with appropriate bound type
            let bound_type = if had_cutoff {
                BoundType::Lower  // Beta cutoff: actual score >= max_eval
            } else {
                BoundType::Exact  // All moves explored: exact score
            };
            tt.store(board_hash, max_eval, depth, bound_type, best_move);
            max_eval
        } else {
            let mut min_eval = i32::MAX;
            let mut best_move: Option<Direction> = None;
            let mut had_cutoff = false;

            for mv in moves {
                let mut child_board = board.clone();
                Self::apply_move(&mut child_board, player_idx, mv, config);
                Self::advance_game_state(&mut child_board);

                let eval = Self::alpha_beta_minimax(
                    &child_board,
                    our_snake_id,
                    depth - 1,
                    depth_from_root + 1,  // One ply deeper
                    alpha,
                    beta,
                    true,
                    config,
                    tt,
                    killers,
                    history,
                );

                if eval < min_eval {
                    min_eval = eval;
                    best_move = Some(mv);
                }

                beta = beta.min(eval);
                if beta <= alpha {
                    // Alpha cutoff: record this move as a killer and update history
                    killers.record_killer(depth, mv, config);
                    history.update(current_pos, mv, depth, true);
                    simple_profiler::record_alpha_beta_cutoff();
                    had_cutoff = true;
                    break;
                }
            }

            // Store with appropriate bound type
            let bound_type = if had_cutoff {
                BoundType::Upper  // Alpha cutoff: actual score <= min_eval
            } else {
                BoundType::Exact  // All moves explored: exact score
            };
            tt.store(board_hash, min_eval, depth, bound_type, best_move);
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
                1, // One ply down from root
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
                1,  // One ply down from root after applying move
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
        let health = Self::compute_health_score(&test_board, our_idx, &[], config);
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
                Self::compute_wall_penalty(h, test_board.width as i32, test_board.height as i32, test_board.snakes[our_idx].health, config),
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
