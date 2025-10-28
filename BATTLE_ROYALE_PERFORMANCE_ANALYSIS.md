# Battle Royale Performance Analysis

**Date:** 2025-10-28
**Games Analyzed:** 6 games (2 Florence, 4 All Human)
**Total Turns:** 404 turns across all games

---

## Executive Summary

Analysis of live battle royale games reveals **two critical performance issues**:

1. **🔴 CRITICAL: 61.8% of moves exceed time budget** (147/238 turns timeout)
2. **⚠️ MODERATE: 100% of games end with Rusty getting trapped** (0% win rate)

Despite fixing the illegal move bug, the bot struggles with:
- Excessive computation time causing frequent timeouts (up to 500ms)
- Poor space control leading to getting trapped with no legal moves
- Inefficient search recalculating duplicate board states

---

## Performance Analysis

### Timing Issues

**Florence Games (2 games, 238 turns):**
```
Average latency:     132.2ms (good)
Max latency:         500ms (CRITICAL - exceeds 400ms budget)
Timeout instances:   147 (61.8%)
```

**Timeout Distribution:**
- 4 snakes: 29 timeouts (19.7%)
- 3 snakes: 118 timeouts (80.3%)

**Game Phase Distribution:**
- Early game (<50 turns): 57 timeouts (38.8%)
- Mid game (50-150): 84 timeouts (57.1%)
- Late game (≥150): 6 timeouts (4.1%)

**Critical Finding:** Most timeouts occur with 3 snakes in mid-game. This suggests:
1. IDAPOS locality masking isn't aggressive enough
2. Board state recalculation is expensive (no transposition table)
3. Branching factor estimation may still be too optimistic

---

## Death Pattern Analysis

### All Games End in Trapping

**Florence Games:**
- game_01.jsonl (159 turns): Trapped (health=97, length=5)
- game_02.jsonl (79 turns): Trapped (health=97, length=5)

**All Human Games:**
- game_01.jsonl (137 turns): Trapped (health=unknown)
- game_02.jsonl (67 turns): Trapped (health=95, length=6)
- game_03.jsonl (80 turns): Trapped (health=59, length=4)
- game_04.jsonl (41 turns): Trapped (health=60, length=3)

**Pattern:**
- NO wall collisions (illegal move bug is fixed ✓)
- NO starvation (health management is working)
- 100% trapped with no legal moves

**Root Cause:** Space control evaluation (flood fill) doesn't adequately predict "escape route" viability. The bot can see it has space, but doesn't recognize when that space will become a dead end.

---

## Root Cause Analysis

### 1. Timeout Issues (61.8% of moves)

**Problem:** Search takes too long, frequently exceeding 400ms budget and hitting 500ms cap.

**Contributing Factors:**

#### A. No Transposition Table
- Same board states are recalculated multiple times
- Example: In MaxN search with 3 snakes and 4 moves each, depth 3 generates ~64 states
- Many of these states are transpositions (same board, different move order)
- **Impact:** 2-5x redundant computation

#### B. IDAPOS Distance Too Broad
- Current: `head_distance_multiplier = 2`
- At depth 3: considers snakes within distance `2 * 3 = 6`
- On 11x11 board, this often includes ALL snakes
- **Impact:** Doesn't reduce branching as intended

#### C. Expensive Evaluation Function
- Flood fill (space control): O(W×H) = O(121) per evaluation
- Adversarial flood fill (territory): O(W×H) = O(121) per evaluation
- Called at every leaf node
- **Impact:** Evaluation dominates search time

---

### 2. Trapping Issues (100% of games)

**Problem:** Bot repeatedly gets cornered with no legal moves.

**Contributing Factors:**

#### A. Flood Fill Doesn't Predict Dead Ends
- Current `compute_space_score()` counts reachable cells
- Doesn't consider if those cells form a dead end trap
- Example: 100 reachable cells in a narrow corridor = eventual trap

#### B. Weight Configuration
- `weight_space = 10.0` (moderate priority)
- `weight_control = 3.0` (low priority)
- Space control may not be prioritized enough vs other factors

#### C. No "Escape Route" Heuristic
- Doesn't check: "If I go this direction, can I get back out?"
- Doesn't penalize moves toward board edges/corners
- Doesn't favor moves toward center/open areas

---

## Recommended Optimizations

### Priority 1: Transposition Table (HIGH IMPACT)

**Goal:** Reduce computation by 2-5x through state caching

**Implementation:**
```rust
use std::collections::HashMap;
use std::sync::RwLock;

struct TranspositionEntry {
    score: i32,
    depth: u8,
    age: u32,  // For LRU eviction
}

struct TranspositionTable {
    table: RwLock<HashMap<u64, TranspositionEntry>>,
    max_size: usize,
    current_age: AtomicU32,
}

impl TranspositionTable {
    // Hash board state (position of all snakes, food)
    fn hash_board(board: &Board) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut hasher = DefaultHasher::new();

        // Hash snake positions
        for snake in &board.snakes {
            snake.id.hash(&mut hasher);
            snake.health.hash(&mut hasher);
            for coord in &snake.body {
                coord.x.hash(&mut hasher);
                coord.y.hash(&mut hasher);
            }
        }

        // Hash food positions
        for coord in &board.food {
            coord.x.hash(&mut hasher);
            coord.y.hash(&mut hasher);
        }

        hasher.finish()
    }

    // Probe cache
    fn probe(&self, board_hash: u64, depth: u8) -> Option<i32> {
        let table = self.table.read().unwrap();

        if let Some(entry) = table.get(&board_hash) {
            // Only use if cached depth >= required depth
            if entry.depth >= depth {
                return Some(entry.score);
            }
        }

        None
    }

    // Store result
    fn store(&self, board_hash: u64, score: i32, depth: u8) {
        let mut table = self.table.write().unwrap();

        // LRU eviction if table is full
        if table.len() >= self.max_size {
            let age = self.current_age.load(Ordering::Relaxed);
            table.retain(|_, entry| age - entry.age < 100);
        }

        table.insert(board_hash, TranspositionEntry {
            score,
            depth,
            age: self.current_age.load(Ordering::Relaxed),
        });
    }
}
```

**Integration Points:**
- Add `tt: Arc<TranspositionTable>` to search functions
- Probe before evaluation: `if let Some(cached) = tt.probe(hash, depth) { return cached; }`
- Store after evaluation: `tt.store(hash, score, depth);`

**Expected Impact:**
- 2-5x speedup from avoiding redundant computation
- Enable deeper search within same time budget
- Reduce timeout instances from 61.8% to <10%

---

### Priority 2: Tune IDAPOS Parameters (MODERATE IMPACT)

**Goal:** Reduce branching by focusing on immediate threats

**Current Configuration:**
```toml
[idapos]
head_distance_multiplier = 2
```

**Proposed Changes:**
```toml
[idapos]
head_distance_multiplier = 1  # More aggressive locality
max_distance_threshold = 4     # Hard cap on distance
```

**Rationale:**
- At depth 3 with multiplier=2: considers snakes within distance 6
- At depth 3 with multiplier=1: considers snakes within distance 3
- On 11x11 board, distance 3 is sufficient for tactical decisions

**Expected Impact:**
- Reduce branching factor by 30-50% in early game
- Enable deeper search (depth 3-4 instead of 2-3)
- Reduce timeout instances

---

### Priority 3: Improve Space Control Heuristic (MODERATE IMPACT)

**Goal:** Avoid dead-end traps

**Option A: Center-Bias Heuristic**
```rust
fn compute_center_bias(pos: Coord, width: i32, height: i32) -> i32 {
    let center_x = width / 2;
    let center_y = height / 2;
    let dist_from_center = (pos.x - center_x).abs() + (pos.y - center_y).abs();

    // Prefer central positions
    100 - (dist_from_center * 10)
}
```

**Option B: Escape Route Validation**
```rust
fn has_escape_route(board: &Board, pos: Coord, snake_idx: usize) -> bool {
    let reachable = flood_fill_bfs(board, pos, snake_idx);

    // Check if reachable cells connect to "safe" areas (center, multiple exits)
    let has_multiple_exits = count_edge_connections(&reachable, board) > 1;
    let has_center_access = reachable_contains_center(&reachable, board);

    has_multiple_exits || has_center_access
}
```

**Option C: Increase Space Weight**
```toml
[scores]
weight_space = 20.0  # Increase from 10.0
```

**Expected Impact:**
- Reduce trapping from 100% to <30%
- Improve win rate against human players
- Better long-term survival

---

## Implementation Plan

### Phase 1: Transposition Table (Critical - addresses 61.8% timeout rate)

1. Add `TranspositionTable` struct to `src/bot.rs`
2. Integrate into `maxn_search()` and `alpha_beta_minimax()`
3. Add configuration: `tt_max_size = 100000` (adjust based on memory)
4. Test on Florence games, measure speedup

**Success Criteria:**
- Timeout rate < 10% (currently 61.8%)
- Average computation time < 200ms (currently 132ms but many 400ms+ outliers)
- Search depth increases to 3-4 (currently 2)

### Phase 2: IDAPOS Tuning (Moderate - further reduces computation)

1. Change `head_distance_multiplier` from 2 to 1
2. Add `max_distance_threshold = 4` hard cap
3. Test on Florence games, measure branching reduction

**Success Criteria:**
- Branching factor reduced by 30-50%
- Deeper search (depth 4-5)
- Timeout rate < 5%

### Phase 3: Space Control Improvement (Moderate - addresses 100% trapping)

1. Implement center-bias heuristic (simplest)
2. If insufficient, add escape route validation
3. Test on Florence games, measure trap rate

**Success Criteria:**
- Trapping rate < 30% (currently 100%)
- Win at least 1/6 games (currently 0/6)
- Average survival > 100 turns (currently variable)

---

## Testing Strategy

### Regression Tests
```bash
# Validate no illegal moves
cargo run --release --bin validate_moves -- tests/fixtures/battle_royale_florence/
cargo run --release --bin validate_moves -- tests/fixtures/battle_royale_all_human/

# Analyze timing improvement
cargo run --release --bin analyze_timing -- tests/fixtures/battle_royale_florence/

# Analyze death patterns
cargo run --release --bin analyze_deaths -- tests/fixtures/battle_royale_florence/

# Replay consistency
cargo run --release --bin replay -- tests/fixtures/battle_royale_florence/game_01.jsonl --all
```

### Performance Benchmarks

**Before Optimizations (Baseline):**
- Average latency: 132ms
- Max latency: 500ms
- Timeout rate: 61.8% (147/238)
- Trap rate: 100% (6/6)
- Win rate: 0% (0/6)

**After Transposition Table (Target):**
- Average latency: <100ms
- Max latency: <350ms
- Timeout rate: <10%
- Trap rate: 100% (unchanged)
- Win rate: 0% (unchanged)

**After All Optimizations (Target):**
- Average latency: <100ms
- Max latency: <350ms
- Timeout rate: <5%
- Trap rate: <30%
- Win rate: >15% (≥1/6)

---

## Conclusion

The battle royale bot faces two distinct issues:

1. **Performance:** 61.8% timeout rate due to redundant computation
   - **Solution:** Transposition table (HIGH priority)
   - **Impact:** 2-5x speedup, deeper search, <10% timeout rate

2. **Strategy:** 100% trap rate due to poor space control
   - **Solution:** Center bias + IDAPOS tuning (MODERATE priority)
   - **Impact:** <30% trap rate, >15% win rate

Implementing the transposition table is the HIGHEST priority optimization, as it:
- Addresses the most severe issue (timeouts)
- Enables all other optimizations (deeper search improves strategy)
- Has the clearest implementation path
- Provides measurable, immediate impact

The IDAPOS tuning and space control improvements should follow once the transposition table is validated and deployed.
