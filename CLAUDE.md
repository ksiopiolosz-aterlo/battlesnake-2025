# Project Overview

You are collaborating on a competitive **Battlesnakes** bot (https://docs.battlesnake.com/) written in Rust. The goal is to build a high-performance snake AI that uses the MaxN algorithm to evaluate moves and outmaneuver opponents within strict time constraints.

## Configuration Parameters

These are tunable parameters (consider externalizing to a config file):
- `RESPONSE_TIME_BUDGET_MS`: Maximum response time for move endpoint (default: 400ms)
- `POLLING_INTERVAL_MS`: How often to recompute optimal move (default: 50ms)
- Weights for scoring function (see Priorities section)

---

# Code Style

## Core Principles
- **IMPORTANT**: Use OOP-style patterns in Rust for clear object representation and maintainability
- **MUST NEVER** use `unsafe` code blocks
- Prefer simple, straightforward representations over complex abstractions
- Keep functions small and focused (cognitive complexity < 15)

## Concurrency & Performance
- **I/O-bound tasks** (async API endpoints): Use `tokio`
- **CPU-bound tasks** (move computation): Use `rayon`
- Prefer atomics over locks; if locks required, use `parking_lot` over `std`
- Avoid cloning wherever possible:
  - Pass read-only data by immutable reference
  - Use `Arc` only when crossing thread boundaries
- Minimize memory contention hotspots:
  - Example: Use per-thread atomics rather than a single shared atomic

## Constants & Configuration
- Use constants for all magic numbers
- Consider externalizing to configuration file (e.g., `Snake.toml`)

---

# Workflow

## Development Process
1. Make code changes following the style guide
2. **IMPORTANT**: Validate compilation via `cargo build` and resolve all compiler errors
3. Run static analysis tools to catch issues
4. **Default behavior**: Do NOT generate tests unless explicitly requested

## Testing
- Test format: JSON input matching existing data contracts
- **MUST** clarify expectations if requirements are vague or unclear
- Run targeted tests (single test or small subset) rather than full suite for performance

---

# Algorithm Implementation

## MaxN Algorithm Specification

**Definition**: MaxN is a multi-agent adversarial search algorithm extending minimax to N players in non-zero-sum games. Each game state evaluates to an N-tuple `[s₀, s₁, ..., sₙ₋₁]` where `sᵢ` represents player i's utility score. At interior nodes, the active player selects the child maximizing their own score component.

**Key Property**: Unlike minimax (zero-sum where Σsᵢ = 0), MaxN handles games where Σsᵢ ≠ constant, enabling independent player objectives.

### Architecture Components

The implementation uses three concurrent components communicating via lock-free atomics:

1. **Main Loop** (async, tokio): Handles `/move` HTTP endpoint with polling
2. **Computation Engine** (sync, rayon): Performs parallel game tree search
3. **Shared Atomic State**: Lock-free communication between components

```
[HTTP Request] → [Tokio: Start Timer] → [Rayon: Iterative Deepening]
    ↓                                           ↓
[Polling Loop] ← [Atomic Cache: Best Move] ← [Strategy Selection]
    ↓
[Return Move Before Timeout]
```

### Core Data Structures

```rust
// Configuration constants
const RESPONSE_TIME_BUDGET_MS: u64 = 400;  // Total allowed time
const NETWORK_OVERHEAD_MS: u64 = 50;        // Network latency buffer
const EFFECTIVE_BUDGET_MS: u64 = 350;       // Actual computation time
const POLLING_INTERVAL_MS: u64 = 50;        // Update frequency
const INITIAL_DEPTH: u8 = 2;                // Starting search depth

// Lock-free shared state
struct SharedSearchState {
    best_move: Arc<AtomicU8>,           // 0=Up, 1=Down, 2=Left, 3=Right
    best_score: Arc<AtomicI32>,         // Current best utility
    search_complete: Arc<AtomicBool>,   // Completion signal
    current_depth: Arc<AtomicU8>,       // Active search depth
}

// Game state representation
struct GameState {
    board_width: u8,
    board_height: u8,
    snakes: Vec<Snake>,    // Index 0 = our snake
    food: Vec<Coord>,
    turn: u32,
}

struct Snake {
    id: String,
    health: u8,            // 0-100
    body: Vec<Coord>,      // [head, segment1, ..., tail]
    length: usize,
    is_alive: bool,
}

// Evaluation result (N-tuple)
struct ScoreTuple {
    scores: Vec<i32>,      // scores[i] = utility for snake i
}
```

### Algorithm Flow

**Step 1: Request Handler (Tokio Async)**
```rust
async fn handle_move_request(game_state: GameState) -> MoveResponse {
    let start_time = Instant::now();
    let shared = Arc::new(SharedSearchState::new());

    // Spawn CPU-bound computation on rayon pool
    let shared_clone = shared.clone();
    tokio::task::spawn_blocking(move || {
        compute_best_move(game_state, shared_clone, start_time)
    });

    // Poll until timeout or completion
    loop {
        tokio::time::sleep(Duration::from_millis(POLLING_INTERVAL_MS)).await;
        let elapsed = start_time.elapsed().as_millis() as u64;

        if elapsed >= EFFECTIVE_BUDGET_MS ||
           shared.search_complete.load(Ordering::Acquire) {
            break;
        }
    }

    // Return best move (guaranteed valid by anytime property)
    let move_idx = shared.best_move.load(Ordering::Acquire);
    MoveResponse { direction: index_to_direction(move_idx), shout: None }
}
```

**Step 2: Iterative Deepening Engine**
```rust
fn compute_best_move(
    initial_state: GameState,
    shared: Arc<SharedSearchState>,
    start_time: Instant,
) {
    // Determine execution strategy
    let num_snakes = initial_state.snakes.iter().filter(|s| s.is_alive).count();
    let num_cpus = rayon::current_num_threads();

    let strategy = match (num_snakes, num_cpus) {
        (2, n) if n > 1 => ExecutionStrategy::Parallel1v1,      // Alpha-beta
        (_, n) if n > 1 => ExecutionStrategy::ParallelMultiplayer, // MaxN
        _ => ExecutionStrategy::Sequential,                     // No parallelism
    };

    // Iterative deepening loop
    let mut current_depth = INITIAL_DEPTH;
    loop {
        let remaining = EFFECTIVE_BUDGET_MS.saturating_sub(
            start_time.elapsed().as_millis() as u64
        );

        if remaining < 20 ||
           estimate_iteration_time(current_depth, num_snakes) > remaining {
            break;
        }

        // Execute search (strategy-dependent)
        match strategy {
            ExecutionStrategy::Parallel1v1 =>
                parallel_1v1_search(&initial_state, current_depth, &shared),
            ExecutionStrategy::ParallelMultiplayer =>
                parallel_multiplayer_search(&initial_state, current_depth, &shared),
            ExecutionStrategy::Sequential =>
                sequential_search(&initial_state, current_depth, &shared),
        }

        current_depth += 1;
        if current_depth > 20 { break; }  // Safety cap
    }

    shared.search_complete.store(true, Ordering::Release);
}

// Time estimation: exponential branching ~4^(depth×snakes)
fn estimate_iteration_time(depth: u8, num_snakes: usize) -> u64 {
    let exponent = (depth as f64) * (num_snakes as f64);
    (0.01 * 3.5_f64.powf(exponent)).ceil() as u64
}
```

**Step 3: Parallel 1v1 Search (Alpha-Beta Optimization)**
```rust
fn parallel_1v1_search(state: &GameState, depth: u8, shared: &Arc<SharedSearchState>) {
    let our_moves = generate_legal_moves(state, 0);
    if our_moves.is_empty() {
        shared.best_move.store(0, Ordering::Release);
        shared.best_score.store(i32::MIN, Ordering::Release);
        return;
    }

    // Parallel evaluation: one thread per root move
    our_moves.par_iter().enumerate().for_each(|(idx, &mv)| {
        let mut child = state.clone();
        apply_move(&mut child, 0, mv);

        // Use alpha-beta for opponent (2-player zero-sum optimization)
        let score = alpha_beta_minimax(&child, depth - 1, i32::MIN, i32::MAX, false);

        // Lock-free atomic update (compare-and-swap)
        loop {
            let current_best = shared.best_score.load(Ordering::Acquire);
            if score <= current_best { break; }

            if shared.best_score.compare_exchange(
                current_best, score, Ordering::Release, Ordering::Acquire
            ).is_ok() {
                shared.best_move.store(idx as u8, Ordering::Release);
                break;
            }
        }
    });
}

fn alpha_beta_minimax(
    state: &GameState, depth: u8, mut alpha: i32, mut beta: i32, is_max: bool
) -> i32 {
    if depth == 0 || is_terminal(state) {
        return evaluate_state(state, 0).for_player(0);
    }

    let player = if is_max { 0 } else { 1 };
    let moves = generate_legal_moves(state, player);

    if is_max {
        let mut max_eval = i32::MIN;
        for mv in moves {
            let mut child = state.clone();
            apply_move(&mut child, player, mv);
            let eval = alpha_beta_minimax(&child, depth - 1, alpha, beta, false);
            max_eval = max_eval.max(eval);
            alpha = alpha.max(eval);
            if beta <= alpha { break; }  // Beta cutoff
        }
        max_eval
    } else {
        let mut min_eval = i32::MAX;
        for mv in moves {
            let mut child = state.clone();
            apply_move(&mut child, player, mv);
            let eval = alpha_beta_minimax(&child, depth - 1, alpha, beta, true);
            min_eval = min_eval.min(eval);
            beta = beta.min(eval);
            if beta <= alpha { break; }  // Alpha cutoff
        }
        min_eval
    }
}
```

**Step 4: Parallel Multiplayer MaxN Search**
```rust
fn parallel_multiplayer_search(state: &GameState, depth: u8, shared: &Arc<SharedSearchState>) {
    let our_moves = generate_legal_moves(state, 0);
    if our_moves.is_empty() {
        shared.best_move.store(0, Ordering::Release);
        shared.best_score.store(i32::MIN, Ordering::Release);
        return;
    }

    our_moves.par_iter().enumerate().for_each(|(idx, &mv)| {
        let mut child = state.clone();
        apply_move(&mut child, 0, mv);

        let score_tuple = maxn_search(&child, depth - 1, 0);
        let our_score = score_tuple.for_player(0);

        // Atomic update
        loop {
            let current_best = shared.best_score.load(Ordering::Acquire);
            if our_score <= current_best { break; }

            if shared.best_score.compare_exchange(
                current_best, our_score, Ordering::Release, Ordering::Acquire
            ).is_ok() {
                shared.best_move.store(idx as u8, Ordering::Release);
                break;
            }
        }
    });
}

fn maxn_search(state: &GameState, depth: u8, current_player: usize) -> ScoreTuple {
    if depth == 0 || is_terminal(state) {
        return evaluate_state(state, 0);
    }

    // IDAPOS: Locality masking optimization
    let active_snakes = determine_active_snakes(state, depth);

    // Switch to alpha-beta if only 2 snakes in locality
    if active_snakes.len() == 2 && active_snakes.contains(&0) {
        let other = *active_snakes.iter().find(|&&i| i != 0).unwrap();
        return alpha_beta_for_two_snakes(state, depth, 0, other);
    }

    // Standard MaxN recursion
    let moves = generate_legal_moves(state, current_player);
    if moves.is_empty() {
        let mut dead_state = state.clone();
        dead_state.snakes[current_player].is_alive = false;
        let next = (current_player + 1) % state.snakes.len();
        return maxn_search(&dead_state, depth, next);
    }

    let mut best_tuple = ScoreTuple::new_with_value(state.snakes.len(), i32::MIN);

    for mv in moves {
        let mut child = state.clone();
        apply_move(&mut child, current_player, mv);

        let next = (current_player + 1) % state.snakes.len();
        let all_moved = next == 0;

        let child_tuple = if all_moved {
            advance_game_state(&mut child);
            maxn_search(&child, depth - 1, 0)
        } else {
            maxn_search(&child, depth, next)
        };

        // Update if current player improves
        if child_tuple.for_player(current_player) > best_tuple.for_player(current_player) {
            best_tuple = child_tuple;
        } else if child_tuple.for_player(current_player) == best_tuple.for_player(current_player) {
            // Pessimistic tie-breaking
            best_tuple = pessimistic_tie_break(&best_tuple, &child_tuple, 0);
        }
    }

    best_tuple
}

// IDAPOS: Mask non-local snakes to reduce branching
fn determine_active_snakes(state: &GameState, remaining_depth: u8) -> Vec<usize> {
    let mut active = vec![0];
    let our_head = state.snakes[0].body[0];

    for (idx, snake) in state.snakes.iter().enumerate().skip(1) {
        if !snake.is_alive { continue; }

        let head_dist = manhattan_distance(our_head, snake.body[0]);
        if head_dist <= 2 * remaining_depth as i32 {
            active.push(idx);
            continue;
        }

        for &segment in &snake.body {
            if manhattan_distance(our_head, segment) <= remaining_depth as i32 {
                active.push(idx);
                break;
            }
        }
    }
    active
}

// Pessimistic tie-breaking: assume opponents minimize our score
fn pessimistic_tie_break(a: &ScoreTuple, b: &ScoreTuple, our_idx: usize) -> ScoreTuple {
    let opponent_sum = |t: &ScoreTuple| {
        t.scores.iter().enumerate()
            .filter(|(i, _)| *i != our_idx)
            .map(|(_, &s)| s)
            .sum::<i32>()
    };

    if opponent_sum(a) < opponent_sum(b) { a.clone() } else { b.clone() }
}
```

**Step 5: Sequential Search (Graceful Uniprocessor Degradation)**
```rust
fn sequential_search(state: &GameState, depth: u8, shared: &Arc<SharedSearchState>) {
    let num_snakes = state.snakes.iter().filter(|s| s.is_alive).count();

    if num_snakes == 2 {
        sequential_1v1_search(state, depth, shared);
    } else {
        sequential_multiplayer_search(state, depth, shared);
    }
}

fn sequential_1v1_search(state: &GameState, depth: u8, shared: &Arc<SharedSearchState>) {
    let moves = generate_legal_moves(state, 0);
    if moves.is_empty() {
        shared.best_move.store(0, Ordering::Release);
        shared.best_score.store(i32::MIN, Ordering::Release);
        return;
    }

    let mut best_score = i32::MIN;
    let mut best_idx = 0;

    for (idx, &mv) in moves.iter().enumerate() {
        let mut child = state.clone();
        apply_move(&mut child, 0, mv);

        let score = alpha_beta_minimax(&child, depth - 1, i32::MIN, i32::MAX, false);

        if score > best_score {
            best_score = score;
            best_idx = idx;

            // Immediate update (anytime property)
            shared.best_move.store(best_idx as u8, Ordering::Release);
            shared.best_score.store(best_score, Ordering::Release);
        }
    }
}

fn sequential_multiplayer_search(state: &GameState, depth: u8, shared: &Arc<SharedSearchState>) {
    let moves = generate_legal_moves(state, 0);
    if moves.is_empty() {
        shared.best_move.store(0, Ordering::Release);
        shared.best_score.store(i32::MIN, Ordering::Release);
        return;
    }

    let mut best_score = i32::MIN;
    let mut best_idx = 0;

    for (idx, &mv) in moves.iter().enumerate() {
        let mut child = state.clone();
        apply_move(&mut child, 0, mv);

        let tuple = maxn_search(&child, depth - 1, 0);
        let score = tuple.for_player(0);

        if score > best_score {
            best_score = score;
            best_idx = idx;
            shared.best_move.store(best_idx as u8, Ordering::Release);
            shared.best_score.store(best_score, Ordering::Release);
        }
    }
}
```

## State Evaluation Function

**Step 6: Multi-Component Scoring**
```rust
fn evaluate_state(state: &GameState, our_idx: usize) -> ScoreTuple {
    let mut scores = vec![0i32; state.snakes.len()];

    for (idx, snake) in state.snakes.iter().enumerate() {
        if !snake.is_alive {
            scores[idx] = i32::MIN + 1000;
            continue;
        }

        // Component values
        let survival = if snake.is_alive { 0 } else { -1_000_000 };
        let health = compute_health_score(state, idx);
        let space = compute_space_score(state, idx);
        let control = compute_control_score(state, idx);
        let length = (snake.length as i32) * 100;
        let attack = compute_attack_score(state, idx);

        // Weighted combination (tune these coefficients)
        scores[idx] = survival
            + (1000.0 * survival as f32) as i32
            + (10.0 * space as f32) as i32
            + (5.0 * health as f32) as i32
            + (3.0 * control as f32) as i32
            + (2.0 * attack as f32) as i32
            + length;
    }

    ScoreTuple { scores }
}
```

**Weight Components** (tune experimentally):

| Component | Weight | Description |
|-----------|--------|-------------|
| Survival | 1000.0 | Dead = -∞; alive = 0; head-to-head disadvantage = -500 |
| Space | 10.0 | Flood fill reachable cells; penalize cramped positions |
| Health | 5.0 | Distance to food scaled by urgency `(100-health)/100` |
| Control | 3.0 | Voronoi territory size / total free space |
| Attack | 2.0 | Trap potential (opponent space < threshold) |

### Health & Food Scoring

```rust
fn compute_health_score(state: &GameState, snake_idx: usize) -> i32 {
    let snake = &state.snakes[snake_idx];
    if snake.health == 0 { return -100_000; }

    let head = snake.body[0];
    let nearest_food = state.food.iter()
        .map(|&food| manhattan_distance(head, food))
        .min()
        .unwrap_or(999);

    let urgency = (100.0 - snake.health as f32) / 100.0;
    let distance_penalty = -(nearest_food as f32 * urgency) as i32;

    // Starvation check
    if snake.health <= nearest_food as u8 {
        return -50_000 + distance_penalty;
    }

    distance_penalty
}
```

### Space Control (Flood Fill)

```rust
fn compute_space_score(state: &GameState, snake_idx: usize) -> i32 {
    let snake = &state.snakes[snake_idx];
    let reachable = flood_fill_bfs(state, snake.body[0], snake_idx);

    let required = snake.length + 5;  // Safety margin
    if reachable < required {
        return -(required as i32 - reachable as i32) * 100;
    }

    reachable as i32
}

fn flood_fill_bfs(state: &GameState, start: Coord, snake_idx: usize) -> usize {
    use std::collections::{HashSet, VecDeque};

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back((start, 0));  // (position, turns_elapsed)
    visited.insert(start);

    while let Some((pos, turns)) = queue.pop_front() {
        for dir in &[Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
            let next = move_coord(pos, *dir);

            // Check bounds
            if next.x < 0 || next.x >= state.board_width as i32 ||
               next.y < 0 || next.y >= state.board_height as i32 {
                continue;
            }

            if visited.contains(&next) { continue; }

            // Check if blocked (accounting for bodies that will move)
            if is_position_blocked(state, next, turns, snake_idx) {
                continue;
            }

            visited.insert(next);
            queue.push_back((next, turns + 1));
        }
    }

    visited.len()
}

fn is_position_blocked(
    state: &GameState, pos: Coord, turns_future: usize, checking_snake: usize
) -> bool {
    for (idx, snake) in state.snakes.iter().enumerate() {
        if !snake.is_alive { continue; }

        for (seg_idx, &segment) in snake.body.iter().enumerate() {
            if segment == pos {
                // Will this segment have moved away?
                let segments_from_tail = snake.body.len() - seg_idx;
                if segments_from_tail > turns_future {
                    return true;  // Still occupied
                }
            }
        }
    }
    false
}
```

### Territory Control (Adversarial Flood Fill)

```rust
fn compute_control_score(state: &GameState, snake_idx: usize) -> i32 {
    let control_map = adversarial_flood_fill(state);

    let our_cells = control_map.iter().filter(|&&owner| owner == Some(snake_idx)).count();
    let total_free = control_map.iter().filter(|&&owner| owner.is_some()).count();

    if total_free == 0 { return 0; }

    ((our_cells as f32 / total_free as f32) * 100.0) as i32
}

fn adversarial_flood_fill(state: &GameState) -> Vec<Option<usize>> {
    use std::collections::VecDeque;

    let size = (state.board_width as usize) * (state.board_height as usize);
    let mut control_map = vec![None; size];

    // Mark snake bodies as obstacles
    for (idx, snake) in state.snakes.iter().enumerate() {
        if !snake.is_alive { continue; }
        for &seg in &snake.body {
            let map_idx = (seg.y as usize) * (state.board_width as usize) + (seg.x as usize);
            control_map[map_idx] = Some(idx);
        }
    }

    // Simultaneous BFS from all heads (sorted by length for tie-breaking)
    let mut snakes_sorted: Vec<_> = state.snakes.iter().enumerate().collect();
    snakes_sorted.sort_by_key(|(_, s)| std::cmp::Reverse(s.length));

    let mut queue = VecDeque::new();
    for (idx, snake) in snakes_sorted.iter() {
        if snake.is_alive {
            queue.push_back((snake.body[0], *idx, 0));
        }
    }

    while let Some((pos, owner, dist)) = queue.pop_front() {
        let map_idx = (pos.y as usize) * (state.board_width as usize) + (pos.x as usize);

        if control_map[map_idx].is_some() { continue; }
        control_map[map_idx] = Some(owner);

        for dir in &[Direction::Up, Direction::Down, Direction::Left, Direction::Right] {
            let next = move_coord(pos, *dir);
            if next.x < 0 || next.x >= state.board_width as i32 ||
               next.y < 0 || next.y >= state.board_height as i32 {
                continue;
            }
            queue.push_back((next, owner, dist + 1));
        }
    }

    control_map
}
```

### Attack Potential

```rust
fn compute_attack_score(state: &GameState, snake_idx: usize) -> i32 {
    let our_snake = &state.snakes[snake_idx];
    let our_head = our_snake.body[0];
    let mut attack = 0i32;

    for (idx, opponent) in state.snakes.iter().enumerate() {
        if idx == snake_idx || !opponent.is_alive { continue; }

        // Advantage if longer (can win head-to-head)
        if our_snake.length > opponent.length {
            let dist = manhattan_distance(our_head, opponent.body[0]);
            if dist <= 3 { attack += 50; }
        }

        // Trap potential
        let opp_space = flood_fill_bfs(state, opponent.body[0], idx);
        if opp_space < opponent.length + 3 {
            attack += 100;  // Opponent trapped
        }
    }

    attack
}
```

## Move Generation & Game State Updates

```rust
#[derive(Copy, Clone, Debug)]
enum Direction { Up, Down, Left, Right }

fn generate_legal_moves(state: &GameState, snake_idx: usize) -> Vec<Direction> {
    let snake = &state.snakes[snake_idx];
    if !snake.is_alive || snake.body.is_empty() { return vec![]; }

    let head = snake.body[0];
    let neck = if snake.body.len() > 1 { Some(snake.body[1]) } else { None };

    [Direction::Up, Direction::Down, Direction::Left, Direction::Right]
        .iter()
        .filter_map(|&dir| {
            let next = move_coord(head, dir);

            // Can't reverse onto neck
            if let Some(n) = neck {
                if next == n { return None; }
            }

            // Must stay in bounds
            if next.x < 0 || next.x >= state.board_width as i32 ||
               next.y < 0 || next.y >= state.board_height as i32 {
                return None;
            }

            // Can't collide with bodies (except tails which will move)
            for other in &state.snakes {
                if !other.is_alive { continue; }
                let body_check = if other.body.len() > 1 {
                    &other.body[..other.body.len() - 1]
                } else {
                    &other.body[..]
                };
                if body_check.contains(&next) { return None; }
            }

            Some(dir)
        })
        .collect()
}

fn apply_move(state: &mut GameState, snake_idx: usize, dir: Direction) {
    let snake = &mut state.snakes[snake_idx];
    if !snake.is_alive { return; }

    let new_head = move_coord(snake.body[0], dir);
    snake.body.insert(0, new_head);

    if state.food.contains(&new_head) {
        state.food.retain(|&f| f != new_head);
        snake.health = 100;
        snake.length += 1;
    } else {
        snake.body.pop();
        snake.health = snake.health.saturating_sub(1);
    }

    if snake.health == 0 { snake.is_alive = false; }
}

fn advance_game_state(state: &mut GameState) {
    use std::collections::HashMap;

    // Head-to-head collision detection
    let mut collisions: HashMap<Coord, Vec<usize>> = HashMap::new();
    for (idx, snake) in state.snakes.iter().enumerate() {
        if snake.is_alive {
            collisions.entry(snake.body[0]).or_insert_with(Vec::new).push(idx);
        }
    }

    // Process collisions
    for (_, indices) in collisions {
        if indices.len() > 1 {
            let max_len = indices.iter().map(|&i| state.snakes[i].length).max().unwrap();
            for &idx in &indices {
                if state.snakes[idx].length < max_len {
                    state.snakes[idx].is_alive = false;
                } else if indices.iter().filter(|&&i| state.snakes[i].length == max_len).count() > 1 {
                    state.snakes[idx].is_alive = false;  // Equal length: all die
                }
            }
        }
    }

    state.turn += 1;
}

fn move_coord(c: Coord, dir: Direction) -> Coord {
    match dir {
        Direction::Up => Coord { x: c.x, y: c.y + 1 },
        Direction::Down => Coord { x: c.x, y: c.y - 1 },
        Direction::Left => Coord { x: c.x - 1, y: c.y },
        Direction::Right => Coord { x: c.x + 1, y: c.y },
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
struct Coord { x: i32, y: i32 }

fn manhattan_distance(a: Coord, b: Coord) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn is_terminal(state: &GameState) -> bool {
    state.snakes.iter().filter(|s| s.is_alive).count() <= 1
}

fn index_to_direction(idx: u8) -> String {
    match idx {
        0 => "up".to_string(),
        1 => "down".to_string(),
        2 => "left".to_string(),
        3 => "right".to_string(),
        _ => "up".to_string(),
    }
}
```

## Performance Characteristics

| Configuration | Algorithm | Parallelism | Expected Depth |
|--------------|-----------|-------------|----------------|
| 1v1, Multi-CPU | Alpha-Beta | 4-way root parallel | 8-12 |
| 1v1, Single CPU | Alpha-Beta | Sequential | 6-8 |
| Multiplayer, Multi-CPU | MaxN + IDAPOS | Root parallel | 4-6 |
| Multiplayer, Single CPU | MaxN + IDAPOS | Sequential | 3-5 |

**Complexity**:
- Alpha-Beta: O(b^(d/2)) with good move ordering
- MaxN: O(b^d) for N players (no asymptotic pruning)
- IDAPOS: Reduces branching via locality masking
- Flood Fill: O(W×H) per evaluation

---

# Constraints

- **CRITICAL**: The `/move` endpoint MUST respond in < `RESPONSE_TIME_BUDGET_MS`

---

# Decision Priorities

## Our Snake (in order of importance)

1. **Survival** (highest priority)
   - MUST avoid walls and snake collisions
   - Defensively maneuver to evade threats

2. **Food acquisition**
   - Obtain food when trap/collision weight is acceptably low
   - Balance risk vs. health needs

3. **Offensive trapping**
   - Calculate trap weight: probability of successfully enclosing an opponent
   - Maintain trap once established (unless health critical)
   - MUST NOT collect food inside our own trap perimeter

## Opponent Modeling (for MaxN evaluation)

Assume opponents prioritize:
1. Survival (avoid walls and our snake)
2. Attacking us (trap or collision attempts)
3. Food acquisition