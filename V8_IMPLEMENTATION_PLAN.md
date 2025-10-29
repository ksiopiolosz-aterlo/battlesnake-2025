# V8 Implementation Plan

**Goal:** Fix persistent food avoidance issues in V7.2 (18% avoidance rate) by implementing suggestions from V8_SUGGESTIONS.md.

**Current Status:** V7.2 has temporal discounting (0.95^depth) but still avoids food 18% of the time due to:
1. Simplistic food safety checks (only checks opponent distance to food, not post-eating traps)
2. Future penalties overwhelming immediate bonuses through search tree
3. No growth urgency when significantly shorter than opponents
4. Manhattan-based IDAPOS missing actual threats

---

## 🎯 P0 - Critical Fixes (Must Have for V8)

### ✅ Task 1: Implement Smarter Food Safety Check
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md #2
**Problem:** Current `is_food_safe` only checks if opponent is within distance 2 of food. Doesn't predict if opponent can trap us AFTER we eat.

**Example Failure (V7.2 Turn 36):**
- Food at (3, 10) - distance 1 from us
- Snake 3 at distance 2 from us (length 7, we're length 3)
- Food marked "SAFE" but Snake 3 can trap us against top wall after eating
- Bot avoided food → starved later

**Implementation:**
```rust
fn is_food_actually_safe(
    board: &Board,
    food_pos: Coord,
    our_snake: &Battlesnake,
    active_snakes: &[usize],
    config: &Config,
) -> bool {
    let our_dist = manhattan_distance(our_snake.body[0], food_pos);

    for &opp_idx in active_snakes {
        let opp = &board.snakes[opp_idx];
        if opp.health == 0 { continue; }

        let opp_dist = manhattan_distance(opp.body[0], food_pos);

        // Check 1: Can they arrive first or simultaneously?
        if opp_dist <= our_dist {
            // Will they want this food? (hungry or competitive length)
            if opp.health < 60 || opp.length <= our_snake.length {
                return false; // They'll contest it
            }
        }

        // Check 2: Can they cut us off AFTER we eat?
        if opp_dist <= our_dist + 2 && opp.length >= our_snake.length {
            // Simulate: what escape routes do we have after eating?
            let escape_count = count_escape_routes_after_eating(
                board, food_pos, our_snake, opp, config
            );
            if escape_count < 2 {
                return false; // They can trap us post-eating
            }
        }
    }
    true
}
```

**Location:** `src/bot.rs` - replace current `is_food_safe` logic in `compute_health_score`
**Estimated Impact:** Reduce false "safe" classifications by 60-70%

---

### ✅ Task 2: Implement Hierarchical Evaluation System
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md #1
**Problem:** Single 1.875M food urgency score dominates all other components, causing suicidal food grabs. Future penalties (-5000 corner, -1500 escape, -2000 articulation) can still overwhelm through search tree.

**Current Evaluation Structure (V7.2):**
```rust
// All components mixed together equally
scores[idx] = survival + (weight_health * health) + (weight_space * space) + ...
// Problem: 1.875M food dominates, OR penalties overwhelm through tree
```

**New Hierarchical Structure:**
```rust
// Layer 1: Safety vetoes (hard blocks on dangerous moves)
if is_immediately_lethal(board, snake_idx, position, config) {
    return i32::MIN; // Veto this move entirely
}

// Layer 2: Survival layer (critical needs when health < 20)
let survival_score = if snake.health < 20 {
    compute_food_urgency(...) * 1000  // Cap at 1000x, not unlimited
} else {
    0
};

// Layer 3: Tactical layer (normal gameplay components)
let tactical_score =
    (config.scores.weight_health * health as f32) as i32 +
    (config.scores.weight_space * space as f32) as i32 +
    (config.scores.weight_control * control as f32) as i32 +
    (config.scores.weight_attack * attack as f32) as i32 +
    length;

// Layer 4: Strategic layer (long-term positioning)
let strategic_score =
    length_advantage_bonus +
    center_bias_score;

// Final composition
scores[idx] = survival_score + tactical_score + (strategic_score / 2);
```

**New Configuration Parameters:**
```toml
[scores]
# Survival layer
survival_max_multiplier = 1000.0  # Cap urgency at 1000x, not unlimited
survival_health_threshold = 20    # When to activate survival mode

# Tactical weights (rebalanced)
weight_space = 25.0    # Space control
weight_health = 75.0   # Food acquisition
weight_control = 3.0   # Territory
weight_attack = 10.0   # Aggression
```

**Location:** `src/bot.rs` - `evaluate_state()` function
**Estimated Impact:** Prevent ~40% of food avoidance cases where future penalties overwhelm

---

### ✅ Task 3: Implement Growth Urgency Strategy
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md #6
**Problem:** Bot often stays small (length 3-4) while opponents grow to length 7-9. Small size makes us vulnerable to trapping and head-to-head losses.

**Example Failure (V7.2 Turn 36):**
- Our length: 3
- Opponent length: 7
- Length gap of 4 is extremely dangerous (can't contest food, easy to trap)
- Bot had no incentive to close this gap

**Implementation:**
```rust
fn compute_growth_urgency(
    board: &Board,
    snake_idx: usize,
    active_snakes: &[usize],
    config: &Config,
) -> i32 {
    let our_length = board.snakes[snake_idx].length;
    let our_health = board.snakes[snake_idx].health;

    // Find smallest active opponent
    let min_opp_length = active_snakes.iter()
        .filter_map(|&idx| {
            if idx == snake_idx { return None; }
            let s = &board.snakes[idx];
            if s.health > 0 { Some(s.length) } else { None }
        })
        .min()
        .unwrap_or(100);

    // If we're shorter than smallest opponent, GROW URGENTLY
    if our_length < min_opp_length {
        let gap = (min_opp_length - our_length) as i32;
        return gap * config.scores.growth_urgency_per_length; // e.g., 500 per length
    }

    // If we're longest and healthy, grow conservatively
    if our_length > min_opp_length && our_health > 60 {
        return config.scores.growth_bonus_when_ahead; // e.g., 100
    }

    0
}
```

**New Configuration Parameters:**
```toml
[scores]
growth_urgency_per_length = 500   # Bonus per length unit behind smallest opponent
growth_bonus_when_ahead = 100     # Small bonus when already longest
```

**Integration:** Add to tactical score in hierarchical evaluation
**Location:** `src/bot.rs` - new function, called from `evaluate_state`
**Estimated Impact:** Increase average snake length by 2-3 units

---

## 🔧 P1 - High-Impact Improvements (Should Have)

### ✅ Task 4: Improve IDAPOS with Reachable Distance
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md #3
**Problem:** Manhattan distance doesn't represent actual threat. Snake 10 moves away through open space is more relevant than snake 5 moves away behind a wall.

**Current IDAPOS (V7.2):**
```rust
// Uses simple Manhattan distance
let head_dist = manhattan_distance(our_head, snake.body[0]);
if head_dist <= max_locality_distance {
    active.push(idx);
}
```

**New Reachability-Based IDAPOS:**
```rust
fn determine_active_snakes_v2(
    board: &Board,
    our_snake_id: &str,
    remaining_depth: u8,
    config: &Config,
) -> Vec<usize> {
    let our_idx = /* find our snake */;
    let our_head = board.snakes[our_idx].body[0];

    // Use flood-fill to find actually reachable cells within horizon
    let search_radius = remaining_depth * config.idapos.reachability_multiplier;
    let reachable_cells = flood_fill_bfs_limited(
        board,
        our_head,
        search_radius as usize
    );

    let mut active = vec![our_idx];

    for (idx, snake) in board.snakes.iter().enumerate() {
        if idx == our_idx || snake.health == 0 { continue; }

        // Check if any part of snake is reachable within horizon
        let is_reachable = snake.body.iter()
            .any(|seg| reachable_cells.contains(seg));

        if is_reachable {
            active.push(idx);
        }
    }
    active
}
```

**New Configuration Parameters:**
```toml
[idapos]
reachability_multiplier = 4  # Search radius = depth * multiplier
```

**Location:** `src/bot.rs` - `determine_active_snakes` function
**Estimated Impact:** Better opponent relevance detection, reduce wasted computation on irrelevant snakes

---

### ✅ Task 5: Add Time Management with Early Exit
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md #4
**Problem:** Bot continues searching even when outcome is decided (certain win/loss or no score improvement).

**Implementation:**
```rust
fn compute_best_move_internal(...) {
    // Inside iterative deepening loop, after each depth completes:

    let (_, best_score) = shared.get_best();

    // Early exit condition 1: Certain win
    if best_score >= config.timing.certain_win_threshold {
        info!("Certain win detected (score: {}), stopping search at depth {}",
              best_score, current_depth);
        break;
    }

    // Early exit condition 2: Forced loss
    if best_score <= config.timing.certain_loss_threshold {
        info!("Forced loss detected (score: {}), stopping search at depth {}",
              best_score, current_depth);
        break;
    }

    // Early exit condition 3: No improvement in last 2 iterations
    if depth_since_improvement >= 2 && remaining_time < effective_budget / 3 {
        info!("No score improvement, conserving time at depth {}", current_depth);
        break;
    }

    // Track improvement
    if best_score > previous_best_score {
        depth_since_improvement = 0;
    } else {
        depth_since_improvement += 1;
    }
    previous_best_score = best_score;
}
```

**New Configuration Parameters:**
```toml
[timing]
certain_win_threshold = 1000000    # Score threshold for certain win
certain_loss_threshold = -1000000  # Score threshold for certain loss
no_improvement_tolerance = 2       # Iterations without improvement before early exit
```

**Location:** `src/bot.rs` - `compute_best_move_internal` function
**Estimated Impact:** Save 10-20% computation time in decided positions

---

### ✅ Task 6: Implement Persistent Killer/History Tables with Aging
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md #5
**Problem:** Killer and history tables are cleared each iteration, losing valuable move ordering information.

**Current Behavior (V7.2):**
```rust
// Each iteration starts fresh - wastes learning
let mut killers = KillerMoveTable::new(config);
let mut history = HistoryTable::new(board.width, board.height);
```

**New Persistent Learning:**
```rust
// In KillerMoveTable
impl KillerMoveTable {
    pub fn age_killers(&mut self) {
        // Shift all killers down one age level
        // Remove killers older than threshold
        for depth_killers in &mut self.moves {
            // Keep only recent killers
            depth_killers.retain(|killer| killer.age < self.max_age);
            // Age remaining killers
            for killer in depth_killers {
                killer.age += 1;
            }
        }
    }
}

// In HistoryTable
impl HistoryTable {
    pub fn decay_history(&mut self, decay_factor: f32) {
        // Reduce old scores, keep structure
        for row in &mut self.scores {
            for score in row {
                *score = (*score as f32 * decay_factor) as i32;
            }
        }
    }
}

// In iterative deepening loop
// DON'T clear everything - age instead
if current_depth > INITIAL_DEPTH {
    killers.age_killers();
    history.decay_history(0.9); // Keep 90% of previous knowledge
}
```

**New Configuration Parameters:**
```toml
[move_ordering]
killer_max_age = 3           # Max iterations to keep killer move
history_decay_factor = 0.9   # Decay rate per iteration (0.9 = keep 90%)
```

**Location:**
- `src/move_ordering.rs` - Add aging methods
- `src/bot.rs` - Use aging instead of recreation

**Estimated Impact:** Improve move ordering efficiency by 15-20%, reduce nodes searched

---

### ✅ Task 7: Improve Transposition Table Replacement Scheme
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md #7
**Problem:** Crude eviction (removes 50% when full), no replacement strategy for better entries.

**Current Behavior (V7.2):**
```rust
// Evicts half the table when full - loses valuable entries
if table.len() >= self.max_size {
    let to_remove = self.max_size / 2;
    // ... removes half randomly
}
```

**New Replacement Strategy:**
```rust
impl TranspositionTable {
    pub fn store_with_replacement(
        &self,
        board_hash: u64,
        score: i32,
        depth: u8,
        bound_type: BoundType,
        best_move: Option<Direction>,
    ) {
        if let Ok(mut table) = self.table.write() {
            match table.get_mut(&board_hash) {
                Some(entry) => {
                    // Replacement scheme 1: Always replace if deeper search
                    if depth >= entry.depth {
                        *entry = TranspositionEntry {
                            score, depth, bound_type, best_move,
                            age: self.current_age.load(Ordering::Relaxed),
                        };
                    }
                    // Replacement scheme 2: Prefer exact bounds over bounds
                    else if bound_type == BoundType::Exact &&
                            entry.bound_type != BoundType::Exact {
                        *entry = TranspositionEntry { /* ... */ };
                    }
                }
                None => {
                    // Evict oldest 25% when 90% full (not 50% at 100%)
                    if table.len() >= (self.max_size * 9) / 10 {
                        let age_threshold = self.current_age
                            .load(Ordering::Relaxed)
                            .saturating_sub(50);
                        table.retain(|_, e| e.age > age_threshold);
                    }
                    table.insert(board_hash, /* new entry */);
                }
            }
        }
    }
}
```

**Location:** `src/transposition_table.rs` - `store` method
**Estimated Impact:** Improve TT hit rate by 10-15%

---

## 📊 P2 - Configuration Tuning

### ✅ Task 8: Rebalance Configuration Weights
**Status:** PENDING
**Reference:** V8_SUGGESTIONS.md - Configuration Tuning section

**Changes to Snake.toml:**
```toml
[scores]
# Reduce food urgency from unlimited to capped
immediate_food_bonus = 5000        # Keep same base
# But cap total through survival_max_multiplier = 1000.0

# Rebalance component weights for hierarchical system
weight_space = 25.0      # Space is life
weight_health = 75.0     # Food acquisition
weight_control = 3.0     # Territory (strategic, not tactical)
weight_attack = 10.0     # Aggression when ahead

# Growth strategy
growth_urgency_per_length = 500
growth_bonus_when_ahead = 100

# Keep these dangerous
escape_route_min = 2
corner_danger_base = 5000
```

---

## 🧪 Testing & Validation

### ✅ Task 9: Build and Test V8
**Status:** PENDING

**Test Plan:**
1. Build V8: `cargo build --release`
2. Test problematic turns from V7.2:
   - Turn 36 (adjacent food avoided due to trap risk)
   - Turn 48 (repeated cycling near food)
   - Turn 122 (ignored adjacent food at wall)
3. Replay entire V7.2 games with V8 algorithm
4. Compare food avoidance rates:
   - V7.1: 21% avoidance
   - V7.2: 18% avoidance
   - V8 Target: < 10% avoidance

### ✅ Task 10: Update Configuration and Collect Data
**Status:** PENDING

**Steps:**
1. Update `Snake.toml`: `log_file_path = "optimized_v8.jsonl"`
2. Play 5 games to collect data
3. Run analysis:
   ```bash
   ./target/release/split_games optimized_v8.jsonl
   ./target/release/analyze_food_avoidance tests/fixtures/optimized_v8/game_*.jsonl
   ./target/release/analyze_deaths tests/fixtures/optimized_v8/
   ```
4. Compare metrics against V7.2

---

## 📈 Success Criteria

### Quantitative Metrics:
- **Food Avoidance Rate:** < 10% (down from 18%)
- **Average Snake Length:** > 6 (up from 3-4)
- **Search Depth:** Maintain 5-7 with improved move ordering
- **Survival Rate:** > 40% in 4-player games

### Qualitative Improvements:
- ✅ Bot takes safe adjacent food consistently
- ✅ Bot avoids actual traps (not false positives)
- ✅ Bot grows aggressively when behind in length
- ✅ Bot plays conservatively when ahead in length

---

## Implementation Order

**Phase 1 (Critical - Est. 2-3 hours):**
1. Task 1: Smarter food safety
2. Task 2: Hierarchical evaluation
3. Task 3: Growth urgency
4. Task 8: Config rebalancing
5. Task 9: Build & test

**Phase 2 (High-Impact - Est. 1-2 hours):**
6. Task 4: IDAPOS reachability
7. Task 5: Time management
8. Task 6: Persistent learning
9. Task 7: TT replacement

**Phase 3 (Validation - Est. 30 min):**
10. Task 10: Data collection & analysis

---

## Notes & Observations

**From V7.2 Analysis:**
- Turn 36 failure mode: Food at top wall (3,10), opponent length 7 vs our length 3 at distance 2
  - Current code marked food "SAFE" (no opponent within distance 2 of food)
  - But opponent CAN trap us after we eat (limited escape routes at wall)
  - V8 fix: Check escape routes POST-eating, not just pre-eating distance

**Key Insight from V8_SUGGESTIONS.md:**
> "Your V7.2 urgency multiplier (100x for critical food) is dominating all other evaluation components. This makes the bot suicidal for food because 37.5M overwhelms any danger penalty."

V8 addresses this through hierarchical evaluation with hard safety vetoes and capped survival multipliers.
