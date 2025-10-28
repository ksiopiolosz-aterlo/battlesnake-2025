# Self-Play Game Analysis

## Overview

Analyzed 47 Rusty vs Rusty self-play games to identify algorithmic weaknesses and opportunities for improvement.

## Key Findings

### 1. Wall-Running Behavior Analysis

**Initial Investigation - Game 01**:
- Turn 42: Snake at (0,8) chose "up" → (0,9) - **VALID MOVE**
- Game ends at turn 42 with one snake at (0,8) against left wall
- Death was NOT from wall collision, likely from head-to-head or body collision

**Game 02 - Bottom Wall Pattern**:
- Turn 50: Snake at (3,1) chose "down" → (3,0) - **VALID MOVE** (y=0 is legal)
- Turn 51: Snake at (3,0) chose "up" → (3,1) - **VALID MOVE** (correctly avoids wall)
- Game ends at turn 51
- No wall collision detected - snake correctly stayed in bounds

**Conclusion**: Initial hypothesis of wall-running bug NOT confirmed
- All observed moves stayed within legal bounds (0 to width-1, 0 to height-1)
- Move generation correctly filters out-of-bounds moves
- Deaths appear to be from:
  - Head-to-head collisions (both snakes die if equal length)
  - Body collisions (running into opponent or self)
  - Starvation (health reaches 0)

**Status**: ❌ REAL BUG FOUND - See detailed analysis below

### Validation Results (Updated)

**Initial validation** (flawed - checked all snakes with one move):
- 3,311 illegal moves detected
- Validation tool was incorrectly applying chosen_move to ALL snakes

**Corrected validation** (checks only snake at index 0):
- **1,708 confirmed illegal moves** across 47 games
- Mix of wall collisions and neck collisions
- These are REAL bugs in move selection logic

### Root Cause Analysis

**Bug Location**: `src/bot.rs` lines 450-457, 1572-1579 (and similar in other search functions)

**The Problem**: When `generate_legal_moves()` returns an empty vector (no legal moves available), the code defaults to "up" direction **without checking if "up" is actually legal**.

```rust
if legal_moves.is_empty() {
    info!("No legal moves available");
    shared.best_move.store(
        config.direction_encoding.direction_up_index,  // ← Bug: "up" might be illegal!
        Ordering::Release,
    );
    shared.best_score.store(i32::MIN, Ordering::Release);
    return;
}
```

**Why This Happens**:
- Snake gets trapped (no legal moves due to walls/bodies)
- Code defaults to "up" as a fallback
- If snake is at top wall (y=10) or "up" goes into neck, the move is illegal
- Game server receives illegal move
- This shows up in logs as illegal wall collision or neck collision

**Evidence**:
- Game 04, Turn 2: Snake at (8,0) chose "down" → (8,-1) [out of bounds]
  - Snake was at bottom wall, had legal moves "up" and "left" available
  - This suggests the bug might be more complex than just the no-moves fallback
- Game 47: 93 illegal moves (most of any game)
  - Many wall collisions at x=10 or x=11 (right wall)
  - Many neck collisions

**Status**: ✅ Wall collision **DETECTION** working correctly in validation
**Status**: ❌ Wall collision **PREVENTION** has bugs in move selection

### Fix Applied

**Change**: Modified all three search functions (`sequential_search`, `parallel_multiplayer_search`, `parallel_1v1_search`) to use intelligent fallback when no legal moves exist.

**Before**:
```rust
if legal_moves.is_empty() {
    shared.best_move.store(direction_up_index, Ordering::Release);
    // Always defaults to "up", even if "up" is illegal!
}
```

**After**:
```rust
if legal_moves.is_empty() {
    // Try to find ANY in-bounds move, even if it hits body/neck
    let fallback_move = Direction::all()
        .iter()
        .find(|&&dir| {
            let next = dir.apply(&you.body[0]);
            !Self::is_out_of_bounds(&next, board.width, board.height)
        })
        .copied()
        .unwrap_or(Direction::Up); // Only default to Up if ALL moves are out-of-bounds

    shared.best_move.store(Self::direction_to_index(fallback_move, config), Ordering::Release);
}
```

**Rationale**: When the snake is truly trapped (no legal moves), it's going to die anyway. But we should at least try to pick a move that stays in-bounds rather than defaulting to an arbitrary direction that might be out-of-bounds.

**Expected Impact**: Should reduce wall collision illegal moves in trapped scenarios. However, this fix only addresses the "no legal moves" case. The mystery remains: why do illegal moves occur when legal moves ARE available (e.g., Game 04 Turn 2)?

**Testing**: Fix needs validation with behavioral tests and re-running validation tool on self-play games.

### 2. Match Rate Statistics

**Game 01 Replay Results**:
- Total Turns: 86 log entries (43 actual game turns)
- Matches: 55 (64.0%)
- Mismatches: 31 (36.0%)

**Interpretation**:
- High mismatch rate suggests non-determinism in decision-making
- Could be due to:
  - Time-based cutoffs in iterative deepening
  - Tie-breaking randomness
  - Parallel execution ordering differences

### 3. Game Duration Distribution

| Game Range | Entry Count | Actual Turns | Category |
|-----------|-------------|--------------|----------|
| 1-5 | 86-154 | 43-77 | Quick games |
| 15 | 260 | 130 | Medium |
| 30 | 414 | 207 | Long |
| 47 | 840 | 420 | Epic |

**Quick games (43-77 turns)** are most interesting for bug analysis as they represent early deaths.

### 4. Boxing/Trapping Patterns

The longer games (400+ turns) suggest successful evasion and counter-play. These should be analyzed for:
- Successful space control
- Opponent trapping strategies
- Endgame scenarios

## Recommended Algorithm Improvements

### Priority 1: ~~Fix Wall Collision Logic~~ ✅ Working Correctly

**Status**: Analysis confirmed wall collision prevention is working
- Move generation properly filters out-of-bounds positions
- No instances of wall collision deaths found in 47 games
- Deaths are from legitimate game-ending scenarios

### Priority 2: Food Competition Logic (Nice-to-Have)

**Current Behavior**: Snake always targets nearest food regardless of opponent positioning

**Proposed Enhancement**:
```rust
fn evaluate_food_target(state: &GameState, snake_idx: usize) -> Option<Coord> {
    let our_snake = &state.snakes[snake_idx];
    let our_head = our_snake.body[0];

    // Find all food sorted by distance
    let mut food_options: Vec<(Coord, i32)> = state.food.iter()
        .map(|&food| (food, manhattan_distance(our_head, food)))
        .collect();
    food_options.sort_by_key(|(_, dist)| *dist);

    for (food, our_dist) in food_options {
        let mut contested = false;

        // Check if any opponent is closer AND has more health
        for (idx, opponent) in state.snakes.iter().enumerate() {
            if idx == snake_idx || !opponent.is_alive { continue; }

            let opp_dist = manhattan_distance(opponent.body[0], food);
            if opp_dist < our_dist && opponent.health >= our_snake.health {
                contested = true;
                break;
            }
        }

        if !contested {
            return Some(food);  // Target this food
        }
    }

    // Fallback to nearest if all contested
    food_options.first().map(|(coord, _)| *coord)
}
```

**Rationale**:
- Avoid races we're likely to lose (opponent closer + more health)
- Seek alternative food sources when primary is contested
- Reduces risky confrontations that lead to preventable deaths

**Implementation Location**: `src/bot.rs` in `compute_health_score()`

### Priority 3: Improve Survival Scoring

**Current weights** (from CLAUDE.md):
```rust
SCORE_SURVIVAL_WEIGHT: 1000.0
WEIGHT_SPACE: 10.0
WEIGHT_HEALTH: 5.0
WEIGHT_CONTROL: 3.0
WEIGHT_ATTACK: 2.0
WEIGHT_LENGTH: 100
```

**Issue**: Space control (10.0) might be too low relative to other factors

**Proposed**:
- Increase WEIGHT_SPACE to 50.0 or higher
- Add explicit "moves until trapped" calculation
- Penalize positions where flood fill shows < 2x snake length space

### Priority 3: Reduce Non-Determinism

**Goal**: Achieve >90% match rate on replay

**Actions**:
1. Use deterministic tie-breaking (e.g., lexicographic move order)
2. Set consistent random seed for testing
3. Disable parallel search in replay mode
4. Log search depth achieved per turn

### Priority 4: Head-to-Head Collision Logic

**Current approach** (from CLAUDE.md):
- Equal length snakes: both die
- Longer snake wins

**Potential issue**: May not account for:
- Multiple snakes converging on same cell
- Longer snake deliberately avoiding collision

**Verify**: Check if head-to-head logic correctly handles all cases in `advance_game_state()`

## Testing Strategy

### Phase 1: Reproduce Wall-Running Bug

Create minimal test case:
```rust
#[test]
fn test_wall_collision_prevention() {
    // Snake at (0, 5) on 11x11 board
    // Left move should NEVER be chosen
    // Even if it scores highest before wall check
}
```

### Phase 2: Validate Fix

1. Re-run all 47 self-play games after fix
2. Count wall collision deaths (should be 0)
3. Check if match rates improve
4. Verify longer snakes survive more often

### Phase 3: Behavioral Tests

Add tests for:
- Trapped scenarios (limited escape routes)
- Food vs survival trade-offs
- Aggressive vs defensive play

### Phase 4: Sanity Checking Code Checks
Looking through the code carefully, see if the following potentially identified flaws hold any water:

1. **Food Consumption Race Condition**
In `apply_move`, food is removed immediately when a snake eats it:
```rust
if ate_food {
    board.food.retain(|&f| f != new_head);
    //...
}
```
But in the search tree, when evaluating moves sequentially for different snakes, the first snake evaluated "gets" the food. In reality, if multiple snakes reach food simultaneously, they all eat it. This could lead to incorrect evaluation of contested food situations.

2. **Head Collision Detection Incomplete**
The `is_dangerous_head_to_head` function doesn't account for snakes that might grow before collision. If both snakes eat food on their way to a collision point, the length comparison could change. This is especially important near food clusters.

3. **Advance Game State Order of Operations**
The `advance_game_state` function handles collisions AFTER all moves are applied, but it checks for body collisions using the NEW positions. This means a snake could appear to collide with a body segment that actually just moved away. The body collision check should use the positions from BEFORE the moves.
4. **MaxN Dead Snake Handling**
When a snake has no legal moves in `maxn_search`, it's marked dead but the search continues to the next player at the same depth. However, this death might create cascading effects (like freeing up space) that aren't properly propagated until the next full round.

5. **Missing Bounds Check in Direction::apply**
While the code checks bounds after applying moves, the `Direction::apply` method itself could produce coordinates that overflow/underflow i32. Not shown in this file, but if `Coord` uses i32, moves near i32::MAX or i32::MIN could cause issues.

6. **Evaluation Asymmetry**
The evaluation function computes scores for all snakes but only applies the survival penalty to our snake when dead. Opponent snakes get `score_dead_snake` but not `score_survival_penalty`. This asymmetry could lead to suboptimal decisions in endgame scenarios.


## Next Steps

1. **Investigate Game 01, Turn 42-43** - Confirm wall collision
2. **Search all 47 games** - Find other wall collision instances
3. **Implement wall-death prevention** - Add lookahead validation
4. **Run regression tests** - Ensure fix doesn't break other scenarios
5. **Analyze long games** - Study successful boxing strategies
6. **Tune evaluation weights** - Increase survival priority

## Tools for Further Analysis

```bash
# Replay with verbose output
cargo run --release --bin replay -- tests/fixtures/1v1_self/game_01.jsonl --all --verbose

# Search for specific turn
cargo run --release --bin replay -- tests/fixtures/1v1_self/game_01.jsonl --turns 42,43 --verbose

# Check all games for low match rates
for i in {01..47}; do
  echo "Game $i:"
  cargo run --release --bin replay -- tests/fixtures/1v1_self/game_$i.jsonl --all 2>&1 | grep "Matches:"
done
```


## References

- Game files: `tests/fixtures/1v1_self/game_*.jsonl`
- Test suite: `tests/replay_1v1_self_tests.rs`
- Bot logic: `src/bot.rs`
- Config: `Snake.toml` and `src/config.rs`
- Algorithm spec: `CLAUDE.md`

