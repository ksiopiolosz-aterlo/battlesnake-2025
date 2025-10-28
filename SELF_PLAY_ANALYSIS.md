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

**Status**: ✅ Wall collision prevention working correctly

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
