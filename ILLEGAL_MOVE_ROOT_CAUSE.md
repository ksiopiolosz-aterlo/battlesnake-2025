# ROOT CAUSE: Illegal Move Bug

## Summary

**30.2% of moves are illegal** - the bot returns moves that violate game rules (neck collisions, out-of-bounds).

## Root Cause

**Race condition in `get_move()` initialization** (src/bot.rs:353-429)

### Current (Buggy) Code Flow

```rust
// Line 368: Create shared state with DEFAULT move=0 (Up), score=i32::MIN
let shared = Arc::new(SharedSearchState::new());

// Lines 372-379: TRY to initialize with first legal move
let legal_moves = Self::generate_legal_moves(board, you, &self.config);
if !legal_moves.is_empty() {
    let first_legal_move = legal_moves[0];
    shared.try_update_best(
        Self::direction_to_index(first_legal_move, &self.config),
        i32::MIN + 1,  // ← Score slightly better than default
    );
}

// Lines 390-392: Spawn compute task (STARTS IMMEDIATELY!)
tokio::task::spawn_blocking(move || {
    Bot::compute_best_move_internal(&board_clone, &you, shared_clone, start_time, &config)
});
```

### The Race Condition

1. **T=0ms**: `SharedSearchState::new()` initializes with **move=0 (Up), score=i32::MIN**
2. **T=1ms**: Compute task spawns and **immediately** starts evaluating moves on rayon threads
3. **T=2ms**: Compute task finds a move with score > i32::MIN and updates shared state
4. **T=3ms**: Main thread **FINALLY** tries to run lines 372-379 initialization
5. **T=4ms**: Initialization **FAILS** because `try_update_best()` only updates if new score > current score
6. **Result**: Default move (Up) remains even though it's illegal!

### Why try_update_best Fails

```rust
pub fn try_update_best(&self, move_idx: u8, score: i32) -> bool {
    ...
    // Only update if new score is strictly better
    if score <= current_score {  // ← Initialization fails if search already updated!
        return false;
    }
    ...
}
```

If the compute task updates the score to anything ≥ i32::MIN + 2, the initialization with score=i32::MIN + 1 will be rejected!

## Evidence

### Diagnostic Tool Output

```
❌ Turn 2: Chose Up but legal moves were [Left, Right]
   Head: (6, 10)
   Neck: (6, 9)

❌ Turn 5: Chose Up but legal moves were [Down, Left, Right]
   Head: (7, 8)
   Neck: (7, 9)
   ⚠️  NECK COLLISION: Move would hit neck!

Total turns: 63
Illegal moves: 19 (30.2%)

🚨 CRITICAL BUG: Bot returned moves that generate_legal_moves says are illegal!
```

### Why Default Move (Up) Appears So Often

- `SharedSearchState::new()` always initializes with move=0
- move=0 maps to Direction::Up (src/bot.rs:828)
- When the race condition occurs, Up remains as the chosen move
- Up is frequently illegal (out of bounds at top edge, or neck collision when moving upward)

## Fix

**Move the initialization BEFORE spawning the compute task AND use atomic initialization:**

```rust
// Create shared state with UNINITIALIZED sentinel
let shared = Arc::new(SharedSearchState::new_uninitialized());

// CRITICAL: Initialize BEFORE spawning compute task
let legal_moves = Self::generate_legal_moves(board, you, &self.config);
if !legal_moves.is_empty() {
    let first_legal_move = legal_moves[0];
    shared.force_initialize(  // ← New method that bypasses score check
        Self::direction_to_index(first_legal_move, &self.config),
        i32::MIN + 1,
    );
} else {
    // No legal moves - we're trapped, pick least-bad fallback
    shared.force_initialize(0, i32::MIN);
}

// NOW spawn compute task (shared state is guaranteed valid)
tokio::task::spawn_blocking(move || {
    Bot::compute_best_move_internal(&board_clone, &you, shared_clone, start_time, &config)
});
```

### Required Changes

1. **Add `force_initialize()` method** to `SharedSearchState`:
   ```rust
   /// Force-set the initial move without score comparison
   /// ONLY use this during initialization before search starts
   pub fn force_initialize(&self, move_idx: u8, score: i32) {
       let packed = Self::pack_move_score(move_idx, score);
       self.best_move_and_score.store(packed, Ordering::Release);
   }
   ```

2. **Call `force_initialize()` before spawning compute task**

This ensures the initialization cannot be overwritten by search threads because it completes atomically before any search code runs.

## Alternative Fix (Simpler)

Just move lines 372-379 to happen AFTER line 382 (after cloning) but BEFORE line 390 (spawn):

```rust
let shared_clone = shared.clone();

// INITIALIZE BEFORE SPAWNING (no new method needed)
let legal_moves = Self::generate_legal_moves(board, you, &self.config);
if !legal_moves.is_empty() {
    let first_legal_move = legal_moves[0];
    // Force store without compare-exchange
    let packed = SharedSearchState::pack_move_score(
        Self::direction_to_index(first_legal_move, &self.config),
        i32::MIN + 1,
    );
    shared.best_move_and_score.store(packed, Ordering::Release);
}

// Now spawn (guaranteed safe initialization)
tokio::task::spawn_blocking(move || {
    Bot::compute_best_move_internal(&board_clone, &you, shared_clone, start_time, &config)
});
```

Wait, that won't work because `best_move_and_score` is private. We need the `force_initialize()` method.

## Impact

Once fixed:
- ✅ 0% illegal moves (down from 30.2%)
- ✅ Trapped deaths should decrease significantly
- ✅ Bot will always return a legal move (or best illegal move if truly trapped)

## Related Files

- src/bot.rs:353-429 (`get_move()` function)
- src/bot.rs:200-273 (`SharedSearchState` implementation)
- src/bin/diagnose_illegal_moves.rs (diagnostic tool that found this bug)
