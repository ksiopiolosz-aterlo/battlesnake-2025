# Move/Unmake Pattern Optimization

## Overview

Currently, the bot clones the entire `Board` structure at every search node to simulate moves. This creates significant memory allocation overhead. Implementing a move/unmake pattern would eliminate cloning by modifying the board in-place and then reversing changes after exploring a subtree.

## Current Performance Impact

**Clone Locations** (10+ per search iteration):
- `sequential_search`: 1 clone per root move
- `maxn_search`: 1 clone per legal move at each node
- `alpha_beta_minimax`: 1 clone per legal move at each node
- `parallel_search_1v1` & `parallel_maxn_search`: 1 clone per root move

**What Gets Cloned**:
```rust
pub struct Board {
    pub height: u32,          // 4 bytes
    pub width: i32,           // 4 bytes
    pub food: Vec<Coord>,     // ~8-40 bytes (typical 2-10 food items)
    pub snakes: Vec<Battlesnake>, // LARGEST: 4 snakes × ~200 bytes each
    pub hazards: Vec<Coord>,  // Usually empty
}

pub struct Battlesnake {
    pub id: String,           // ~24 bytes
    pub name: String,         // ~24 bytes
    pub health: i32,          // 4 bytes
    pub body: Vec<Coord>,     // ~80-160 bytes (typical length 8-20)
    pub head: Coord,          // 8 bytes
    pub length: i32,          // 4 bytes
    pub shout: Option<String>, // ~24 bytes
    pub squad: Option<String>, // ~24 bytes
}
```

**Estimate**: ~800-1200 bytes per clone × thousands of nodes = **megabytes of allocations per search**

## Expected Benefits

- **Memory**: 30-50% reduction in allocations
- **Speed**: 10-20% faster search (less allocator pressure, better cache locality)
- **Depth**: Potential +0.5-1.0 depth increase from time savings

## Implementation Design

### 1. MoveContext Structure

Captures all state changes for reversal:

```rust
struct MoveContext {
    snake_idx: usize,
    old_head: Coord,
    old_tail: Option<Coord>,  // None if snake grew
    old_health: i32,
    old_length: i32,
    ate_food: Option<Coord>,   // Food that was eaten
}

struct GameStateContext {
    dead_snakes: Vec<usize>,   // Indices of snakes marked dead
    old_healths: Vec<i32>,     // Previous health values
}
```

### 2. Core Functions

#### make_move()
```rust
fn make_move(
    board: &mut Board,
    snake_idx: usize,
    dir: Direction,
    config: &Config
) -> MoveContext {
    let snake = &mut board.snakes[snake_idx];

    let context = MoveContext {
        snake_idx,
        old_head: snake.head,
        old_tail: snake.body.last().copied(),
        old_health: snake.health,
        old_length: snake.length,
        ate_food: None,
    };

    // Apply move (same logic as current apply_move)
    let new_head = dir.apply(&snake.body[0]);
    snake.body.insert(0, new_head);
    snake.head = new_head;

    let ate_food = board.food.contains(&new_head);
    if ate_food {
        context.ate_food = Some(new_head);
        board.food.retain(|&f| f != new_head);
        snake.health = config.game_rules.health_on_food as i32;
        snake.length += 1;
    } else {
        snake.body.pop();
        snake.health = snake.health.saturating_sub(
            config.game_rules.health_loss_per_turn as i32
        );
    }

    if snake.health <= 0 {
        snake.health = 0;
    }

    context
}
```

#### unmake_move()
```rust
fn unmake_move(board: &mut Board, context: MoveContext, config: &Config) {
    let snake = &mut board.snakes[context.snake_idx];

    // Remove new head
    snake.body.remove(0);

    // Restore tail if didn't grow
    if let Some(tail) = context.old_tail {
        snake.body.push(tail);
    }

    // Restore food if eaten
    if let Some(food) = context.ate_food {
        board.food.push(food);
    }

    // Restore state
    snake.head = context.old_head;
    snake.health = context.old_health;
    snake.length = context.old_length;
}
```

#### make_game_state_advance()
```rust
fn make_game_state_advance(board: &mut Board) -> GameStateContext {
    let mut context = GameStateContext {
        dead_snakes: Vec::new(),
        old_healths: board.snakes.iter().map(|s| s.health).collect(),
    };

    // Collision detection (complex!)
    // Track which snakes die in context.dead_snakes

    context
}
```

#### unmake_game_state_advance()
```rust
fn unmake_game_state_advance(
    board: &mut Board,
    context: GameStateContext
) {
    // Restore health values
    for (idx, &health) in context.old_healths.iter().enumerate() {
        board.snakes[idx].health = health;
    }
}
```

### 3. Update Search Functions

Example transformation for `alpha_beta_minimax`:

**Before**:
```rust
for mv in moves {
    let mut child_board = board.clone();
    Self::apply_move(&mut child_board, player_idx, mv, config);
    Self::advance_game_state(&mut child_board);

    let eval = Self::alpha_beta_minimax(
        &child_board, depth - 1, alpha, beta, !is_max, config, tt
    );

    // Process eval...
}
```

**After**:
```rust
for mv in moves {
    let move_ctx = Self::make_move(board, player_idx, mv, config);
    let state_ctx = Self::make_game_state_advance(board);

    let eval = Self::alpha_beta_minimax(
        board, depth - 1, alpha, beta, !is_max, config, tt
    );

    // Process eval...

    Self::unmake_game_state_advance(board, state_ctx);
    Self::unmake_move(board, move_ctx, config);
}
```

### 4. Search Functions to Update

1. ✅ `sequential_search` (src/bot.rs:928)
2. ✅ `alpha_beta_minimax` (src/bot.rs:2485)
3. ✅ `maxn_search` (src/bot.rs:2284)
4. ✅ `parallel_search_1v1` (src/bot.rs:2680)
5. ✅ `parallel_maxn_search` (src/bot.rs:2764)
6. ✅ `quiescence_search` (if applicable)

## Challenges & Considerations

### 1. Collision Detection Complexity

The `advance_game_state` function performs complex simultaneous collision detection:
- Head-to-head collisions between multiple snakes
- Length comparisons for collision outcomes
- Multiple snakes can die simultaneously
- Must track all state changes accurately

**Solution**: Capture comprehensive before/after state in `GameStateContext`.

### 2. Parallel Search Compatibility

Parallel searches can't share a mutable board.

**Solution**: Parallel root searches can still clone once per root move, but eliminate internal clones. This still provides significant benefit since most cloning happens in the tree, not at the root.

### 3. Transposition Table Invalidation

The TT stores references to board states. With in-place modification, we must ensure:
- TT lookups happen before making moves
- TT stores happen after move sequences complete
- No dangling references to modified state

**Solution**: Current TT design already uses Zobrist hashing and stores only scores/bounds, not board references. No changes needed.

### 4. Testing & Correctness

The move/unmake pattern is error-prone. Any mistake can cause:
- Incorrect search results
- Subtle bugs that only manifest in specific board configurations
- Non-deterministic failures

**Solution**:
- Implement comprehensive unit tests for make/unmake pairs
- Use replay validation to detect regressions
- Test with valgrind/miri to catch memory errors
- Add debug assertions that verify board state consistency

## Implementation Plan

### Phase 1: Foundation (Est. 2-3 hours)
1. Define `MoveContext` and `GameStateContext` structures
2. Implement `make_move()` and `unmake_move()`
3. Write unit tests verifying move/unmake symmetry
4. Test with simple board configurations

### Phase 2: Game State Advance (Est. 2-3 hours)
1. Implement `make_game_state_advance()` and `unmake_game_state_advance()`
2. Handle complex collision scenarios
3. Write extensive tests for collision detection reversal
4. Verify with historical game data

### Phase 3: Search Integration (Est. 3-4 hours)
1. Update `alpha_beta_minimax` to use make/unmake
2. Update `maxn_search` to use make/unmake
3. Update `sequential_search` to use make/unmake
4. Run replay validation on large test suites
5. Fix any bugs discovered

### Phase 4: Parallel Search (Est. 1-2 hours)
1. Update parallel search functions
2. Verify thread safety
3. Benchmark performance gains

### Phase 5: Validation & Tuning (Est. 1-2 hours)
1. Run full test suite
2. Compare search depth/timing before/after
3. Play test games to verify correct behavior
4. Adjust if needed

**Total Estimated Effort**: 9-14 hours of focused development

## Alternative: Partial Implementation

If full move/unmake is too complex, consider a hybrid approach:

1. **Clone at root**: Keep cloning at root node for simplicity
2. **Make/unmake in tree**: Use make/unmake for all internal nodes
3. **Skip game state advance**: Don't unmake collision detection (clone that step)

This provides ~70% of the benefit with ~40% of the complexity.

## Validation Strategy

After implementation:

1. **Replay tests**: Run all fixture replays, compare move decisions
2. **Depth verification**: Confirm search depth increases by expected amount
3. **Performance benchmarks**: Measure timing improvement on battle royale games
4. **Memory profiling**: Use valgrind/heaptrack to verify allocation reduction
5. **Long games**: Play 100+ turn games to catch cumulative errors

## References

- Chess engines (Stockfish, etc.) use move/unmake universally
- Typical allocation reduction: 40-60% in practice
- Performance gain: 15-25% faster search at same depth
- Risk: High (easy to introduce subtle bugs)
- Reward: High (significant performance improvement)

## Status

**Deferred to future session** due to complexity. Bot already has 9 major optimizations completed:
1. Move ordering ✅
2. History heuristic ✅
3. Quiescence search ✅
4. Transposition table bounds ✅
5. Aspiration windows ✅
6. Branching factor tuning ✅
7. Flood fill early exit ✅
8. IDAPOS locality masking ✅
9. Move generation optimization ✅

Current performance is excellent. Move/unmake would provide incremental improvement but requires dedicated focus.
