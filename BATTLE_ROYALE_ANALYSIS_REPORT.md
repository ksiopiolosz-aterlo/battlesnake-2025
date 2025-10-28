# Battle Royale Analysis Report
**Date:** 2025-10-28
**Games Analyzed:** 7 battle royale games vs Hungry Bot
**Average Game Length:** 7.3 turns
**Problem:** Bot consistently runs into walls or collides with other snakes

---

## Executive Summary

The Rusty bot is **completely non-functional** in battle royale (4-player) games, dying within 2-10 turns by running directly into walls. Analysis reveals **three critical bugs**:

1. **🔴 CRITICAL BUG: Illegal move selection** - Bot returns moves that are not in the legal moves list
2. **🔴 CRITICAL: Branching factor misconfiguration** - Multiplayer time estimation prevents any meaningful search
3. **⚠️  MODERATE: Evaluation function doesn't penalize wall proximity**

---

## Detailed Findings

### 1. Illegal Move Selection Bug (CRITICAL)

**Evidence from Game 02, Turn 1:**
```
Position: (1, 10) - at top wall
Legal moves: [Left, Right]
Chosen move: UP  ← ILLEGAL!
Result: Bot hit wall and died
```

The bot is selecting moves that are **not in the legal moves list**. This suggests:
- The move encoding/decoding is broken
- The shared atomic state is not synchronized properly
- There's a fallback to a default move (likely "up") when search fails

**Impact:** Bot will always choose invalid moves when legal moves are constrained, leading to immediate death.

---

### 2. Branching Factor Misconfiguration (CRITICAL)

**Current Configuration (Snake.toml):**
```toml
[time_estimation.multiplayer]
base_iteration_time_ms = 0.01
branching_factor = 4.0
```

**Time Estimation Analysis:**

With 4 snakes in battle royale:
- **Depth 2:** `0.01 * 4.0^(2*4) = 0.01 * 4.0^8 = 655ms` ❌ Exceeds 350ms budget
- **Depth 3:** `0.01 * 4.0^(3*4) = 0.01 * 4.0^12 = 167,772,160ms` ❌ Astronomically high

**Actual Results:**
- Average search depth: **0.8** (barely searching at all!)
- Average computation time: **25.4ms** (using only 7% of 350ms budget)
- 95% of time budget is unused

**Root Cause:** The formula `depth * num_snakes` is **incorrect** for the exponent. It should account for IDAPOS locality masking, which dramatically reduces the effective number of snakes being considered at each depth.

**IDAPOS Reality (from game analysis):**
```
Depth 1: 1 active snake  (just us)
Depth 2: 1-2 active snakes  (us + maybe 1 nearby opponent)
Depth 3: 2-4 active snakes  (gradually expanding locality)
Depth 4+: 4 active snakes  (all snakes now in range)
```

The branching factor should be based on the **number of active snakes at that depth**, not the total number of snakes in the game. At depths 1-2, we're only considering 1-2 snakes, so the branching is much lower than depth 4+.

---

### 3. Evaluation Function Issues (MODERATE)

**Space Control Analysis:**

All moves show nearly identical space control:
```
Turn 7 (at position 5,8):
  Up:    109 reachable cells
  Left:  109 reachable cells
  Right: 109 reachable cells
```

**Problem:** The flood fill counts total reachable cells but doesn't penalize moves that lead toward dead ends or walls. Moving UP toward a wall and moving sideways to open space both score 109 cells.

**What's Missing:**
- No penalty for proximity to walls/boundaries
- No lookahead for "this move leads to a dead end in N turns"
- No consideration of "safe vs dangerous territory"

The evaluation function needs to:
1. Penalize positions near walls (e.g., within 2 cells of boundary)
2. Consider "escape routes" - can we get out if we go this direction?
3. Weight center-of-board positions higher than edges

---

## Move Pattern Analysis

**All 7 games show the exact same pattern:**

```
Game 01: up, up, up, up, up, up, up, up, up  → Hit top wall
Game 02: up, up  → Hit top wall
Game 03: up, up, up, up, up, up, up, up, up, up  → Hit top wall
Game 04: up, up, up, up, up, up, up  → Hit top wall
Game 05: up, up, up, up, up, up, up, up, up, up  → Hit top wall
Game 06: up, up, up, up, up, up, up  → Hit top wall
Game 07: up, up, up, up, up, up  → Hit top wall
```

**This is not a coincidence.** The bot is:
1. Not searching deep enough to see the wall coming (due to branching factor bug)
2. Always defaulting to "up" when search fails or returns invalid moves (due to illegal move bug)
3. Not differentiating between safe and dangerous moves (due to evaluation bug)

---

## IDAPOS Analysis

**IDAPOS is working correctly!** The locality masking successfully reduces the number of snakes considered at shallow depths:

```
Turn 0 (all snakes at distance 8):
  Depth 1: 1 snake  (just us)
  Depth 2: 1 snake  (no opponents within distance 2*2=4)
  Depth 3: 1 snake  (no opponents within distance 2*3=6)
  Depth 4: 4 snakes (all opponents within distance 2*4=8)

Turn 5 (opponents closer, distance 2-4):
  Depth 1: 2 snakes (us + 1 nearby)
  Depth 2: 4 snakes (all opponents within range)
```

IDAPOS correctly identifies that at depths 1-3, we don't need to model distant snakes. The problem is that **we never reach depth 4** due to the branching factor misconfiguration.

---

## Timing Analysis

**Time Budget Utilization:**
- Effective budget: 350ms
- Average usage: 25.4ms (7.3%)
- Max usage: 398ms (1 outlier)
- Unused budget: ~325ms (93%)

**All 7 games have 100% unused time budget.** We could be searching 10-15x deeper but the branching factor estimation prevents it.

---

## Root Cause Summary

### Primary Issue: Illegal Move Selection
**Symptom:** Bot chooses "up" even when it's not a legal move
**Cause:** Unknown - needs investigation of move encoding and atomic state management
**Fix Priority:** 🔴 **CRITICAL** - Must fix before any other improvements

### Secondary Issue: Time Estimation Formula
**Symptom:** Search depth of 0.8 instead of 4-6
**Cause:** Branching factor calculation uses `depth * num_snakes` instead of accounting for IDAPOS
**Current Formula:** `0.01 * 4.0^(depth * num_snakes)`
**Fix Priority:** 🔴 **CRITICAL** - Prevents bot from searching deep enough

### Tertiary Issue: Evaluation Function
**Symptom:** All moves score identically, no wall avoidance
**Cause:** Flood fill doesn't penalize wall proximity or dead ends
**Fix Priority:** ⚠️  **MODERATE** - Can be addressed after above bugs are fixed

---

## Recommendations

### Immediate Actions (Critical Fixes)

#### 1. Fix Illegal Move Selection Bug
**Investigation needed:**
- Check move encoding/decoding in `src/bot.rs`
- Verify atomic state synchronization between threads
- Add assertion: chosen move MUST be in legal moves list
- Add fallback: if chosen move is illegal, select first legal move

**Code to add:**
```rust
// Before returning move
let legal_moves = generate_legal_moves(&game_state, our_idx);
assert!(legal_moves.contains(&chosen_move),
    "BUG: Chosen move {:?} not in legal moves {:?}", chosen_move, legal_moves);
```

#### 2. Fix Branching Factor Calculation
**Current (wrong):**
```rust
let exponent = (depth as f64) * (num_snakes as f64);
let estimate = base * branching_factor.powf(exponent);
```

**Proposed fix:**
```rust
// Account for IDAPOS: at low depths, only a few snakes are active
let active_snakes = determine_active_snakes_count(state, depth);
let exponent = depth as f64;  // NOT depth * num_snakes
let branching = 4.0_f64.powi(active_snakes as i32 - 1);  // 4 moves per active snake
let estimate = base * branching.powf(exponent);
```

**Alternative (simpler):**
- Reduce multiplayer branching_factor from 4.0 to **2.0**
- This would allow depth 3-4 search: `0.01 * 2.0^(3*4) = 40ms` ✅
- Collect real-world timing data and adjust

#### 3. Add Wall Proximity Penalty
**In evaluation function:**
```rust
fn compute_wall_penalty(pos: Coord, width: i32, height: i32) -> i32 {
    let dist_to_wall = [
        pos.x,           // distance to left wall
        width - 1 - pos.x,  // distance to right wall
        pos.y,           // distance to bottom wall
        height - 1 - pos.y, // distance to top wall
    ].iter().min().unwrap();

    match dist_to_wall {
        0 => -10000,  // At wall (should be impossible due to legal move check)
        1 => -5000,   // One square from wall
        2 => -1000,   // Two squares from wall
        _ => 0,       // Safe distance
    }
}
```

### Testing Strategy

1. **Verify illegal move fix:**
   - Replay all 7 games
   - Assert no illegal moves chosen
   - Verify bot survives past turn 10

2. **Verify branching factor fix:**
   - Run game_01 with new branching factor
   - Check search depth reaches 3-5
   - Verify computation time stays under 350ms

3. **Verify wall penalty:**
   - Create test fixture with snake near wall
   - Verify bot chooses moves away from wall
   - Check evaluation scores penalize wall proximity

### Long-Term Improvements

1. **Adaptive time estimation:** Measure actual iteration time and adjust branching factor dynamically
2. **Territory analysis:** Prefer moves toward center/open space
3. **Collision prediction:** Explicitly model head-to-head collisions with other snakes
4. **Replay-based testing:** Create regression test suite from problematic games

---

## Appendix: Sample Game Analysis

### Game 02 (Shortest Game - 2 Turns)

**Turn 0:**
- Position: (1, 9) - near top wall
- All opponents at distance 8-16 (very far)
- IDAPOS: Only 1 active snake at depths 1-3
- Legal moves: All 4 directions
- **Chosen:** UP → moved to (1, 10) [top edge]

**Turn 1:**
- Position: (1, 10) - **AT TOP WALL**
- IDAPOS: Only 1 active snake at depths 1-3
- Legal moves: **[Left, Right]** only - UP and DOWN are illegal
- **Chosen:** UP ← **ILLEGAL MOVE**
- **Result:** Bot tried to move up into wall and died

**What should have happened:**
With even depth-1 search:
1. Try UP: Hits wall immediately → DEAD → score = -∞
2. Try LEFT: Survives → score = positive
3. Try RIGHT: Survives → score = positive
4. Choose LEFT or RIGHT (both valid)

The fact that the bot chose UP (illegal) suggests the search never ran, or the move encoding is broken, or there's a race condition in the atomic state.

---

## Conclusion

The battle royale mode is **completely broken** due to a combination of three bugs:

1. **Move selection bug** (returns illegal moves)
2. **Time estimation bug** (prevents meaningful search)
3. **Evaluation bug** (doesn't avoid walls)

**Priority order for fixes:**
1. Fix illegal move selection (enables bot to survive basic scenarios)
2. Fix branching factor (enables deeper search for strategic decisions)
3. Add wall penalty (improves move quality)

**Expected outcome after fixes:**
- Bot should survive 50+ turns in battle royale
- Search depth should reach 3-5 consistently
- Bot should avoid walls and seek center/open space
- Win rate against Hungry Bot should improve significantly

---

## Next Steps

1. Investigate move selection bug in `src/bot.rs`
2. Implement branching factor fix with IDAPOS awareness
3. Add wall proximity penalty to evaluation
4. Create regression tests using game_01.jsonl and game_02.jsonl
5. Re-run battle royale matches to validate fixes
6. Collect timing data to tune branching factor empirically
