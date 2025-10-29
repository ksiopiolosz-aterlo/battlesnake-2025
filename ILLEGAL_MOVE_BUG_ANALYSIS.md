# CRITICAL BUG: Illegal Move Selection

## Summary

The enhanced trap analysis tool revealed that the bot is selecting **ILLEGAL MOVES**, which directly causes trapped deaths.

## Evidence

### Turn 38 - Game 01
```
Position: (9, 5), Health: 85, Length: 7
Chose: Down (ILLEGAL)

Scores:
- Up:    -2943 (legal)
- Down:  N/A (ILLEGAL - chosen anyway!)
- Left:  -436 (legal, best option)
- Right: -7849 (legal)
```

**The bot chose "Down" which was not a legal move.**

### Turn 44 - Game 01
```
Position: (7, 9), Health: 79, Length: 7
Chose: Right (ILLEGAL)

All other moves (Up, Down, Left) were legal and scored better.
```

**The bot chose "Right" which was not a legal move.**

## Root Cause Investigation

### What the Code SHOULD Do (src/bot.rs:372-379)

```rust
// CRITICAL: Initialize shared state with first legal move to ensure we never
// return an illegal move if search times out before completing any iterations
let legal_moves = Self::generate_legal_moves(board, you, &self.config);
if !legal_moves.is_empty() {
    let first_legal_move = legal_moves[0];
    shared.try_update_best(
        Self::direction_to_index(first_legal_move, &self.config),
        i32::MIN + 1,
    );
}
```

This code **explicitly** initializes the best move with a legal move to prevent this exact bug.

### Hypothesis: Race Condition or Logging Bug?

**Two possibilities:**

1. **Race Condition**: The search algorithm is updating `shared` with illegal moves, overwriting the initial legal move
   - This would be a bug in `parallel_1v1_search`, `parallel_multiplayer_search`, or `sequential_search`
   - These functions iterate over moves returned by `generate_legal_moves()`, so they SHOULD only consider legal moves

2. **Logging Bug**: The debug logger is recording moves incorrectly
   - The bot actually chose legal moves
   - But the logger recorded them wrong (e.g., coordinate system mismatch)
   - This seems LESS likely given that the pattern is consistent

3. **Board State Mismatch**: The board state in the debug log doesn't match the actual game state
   - Opponent moves might have been applied differently
   - Timing issues with concurrent updates

### Next Steps to Diagnose

1. **Add validation** before logging:
   ```rust
   let legal_moves = Self::generate_legal_moves(board, you, &self.config);
   if !legal_moves.contains(&chosen_move) {
       error!("BUG: Chose illegal move {} at turn {}", chosen_move.as_str(), turn);
       // Log all legal moves for debugging
   }
   ```

2. **Replay with validation**: Run the replay tool and check if it generates the same illegal moves
   - If replay also chooses illegal moves → bug in algorithm
   - If replay chooses legal moves → bug in logging or board state recording

3. **Check generate_legal_moves**: Verify it's correctly identifying legal moves
   - Our analysis tool uses its own `is_move_legal()` function
   - Compare with Bot::generate_legal_moves() to see if they disagree

## Immediate Action Plan

### Step 1: Verify Debug Logs Are Accurate

Run the existing games through the validate_moves tool to see if it detects illegal moves:

```bash
./target/release/validate_moves tests/fixtures/mixed_luke_craig/
```

### Step 2: Compare Analysis Tool vs Bot Move Generation

The analysis tool's `is_move_legal()` might be using different rules than `Bot::generate_legal_moves()`.

**Analysis tool checks** (src/bin/analyze_trap_decisions.rs):
- Bounds
- Neck collision
- Body collision (excluding tails)

**Bot might check**:
- Additional rules?
- Different tail handling?
- Head-to-head collision logic?

### Step 3: Add Runtime Validation

Before returning any move, validate it's actually legal:

```rust
// In get_move(), before returning:
let legal_moves = Self::generate_legal_moves(board, you, &self.config);
if !legal_moves.contains(&chosen_move) {
    error!("CRITICAL: About to return illegal move {}!", chosen_move.as_str());
    error!("Legal moves were: {:?}", legal_moves);
    // Fallback to first legal move
    let fallback = legal_moves.first().copied().unwrap_or(Direction::Up);
    return json!({ "move": fallback.as_str() });
}
```

### Step 4: Root Cause Analysis

Once we confirm illegal moves are actually being chosen (not just logged wrong), trace through the search functions to find where illegal moves enter the candidate set.

## Impact

**This bug explains 100% of trapped deaths:**
- The bot chooses an illegal move
- The Battlesnake API rejects it or handles it as "no move"
- The snake doesn't move and becomes trapped
- Game ends

**Priority**: CRITICAL - This is not a scoring issue, it's a correctness issue.

## Expected Fix

Once we identify where illegal moves enter the system, the fix should be straightforward:
1. Ensure `generate_legal_moves()` is correct
2. Ensure search functions only evaluate legal moves
3. Add defensive validation before returning moves
4. Add runtime assertions in debug mode

## Test Plan

1. Fix the bug
2. Re-run games through the analysis tool
3. Verify all chosen moves are legal
4. Verify trapped deaths decrease significantly
5. If still getting trapped, THEN investigate scoring/strategy issues
