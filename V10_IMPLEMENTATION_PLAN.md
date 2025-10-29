# V10 Implementation Plan: Health-Aware Risk Assessment & Food Priority

## Overview

V10 combines the food aversion fixes from V9.2 analysis with verification that existing optimizations (AdaptiveTimeEstimator, Vec-based control maps, aggressive IDAPOS) are working correctly.

**Note:** GAPS.md identified several "missing optimizations" but review shows:
- ✅ AdaptiveTimeEstimator IS being used (lines 947, 1049 in bot.rs)
- ✅ Control maps already use Vec, not HashMap (line 1719 in bot.rs)
- ✅ IDAPOS already aggressive (multiplier=1, max_distance=5 in Snake.toml)

The real issues from Game 4 analysis are behavioral, not performance-related.

## Root Cause Summary

**Game 4 Death Analysis:**
1. **Turn 82** (Health=18): Opponent BODY at distance 2 caused food avoidance → Confirmed user's hypothesis ✅
2. **Turn 86** (Health=14): Conservative space control pushed toward wall
3. **Turn 87** (Health=13): CRITICAL - Adjacent food ignored, moved to corner → Replay mismatch indicates depth issue

**Key Insight:** Turn 87 replay shows current code WOULD eat food, but live game didn't. This suggests behavioral thresholds need adjustment, not performance fixes.

## V10 Changes

### Change 1: Opponent Body Threat Filtering
**File:** `src/bot.rs`
**Location:** Add new helper, modify `compute_space_score()` around line 2064

**Goal:** Only consider opponent bodies "threatening" if their head is close enough to actually trap us.

**Implementation:**
```rust
/// Check if opponent is actually threatening (head close enough to trap us)
/// V10: Filter out distant opponents whose bodies look scary but aren't active threats
fn is_opponent_threatening(
    opponent_idx: usize,
    our_head: Coord,
    nearest_food: Option<Coord>,
    board: &Board,
) -> bool {
    if opponent_idx >= board.snakes.len() {
        return false;
    }

    let opponent = &board.snakes[opponent_idx];
    if opponent.health <= 0 || opponent.body.is_empty() {
        return false;
    }

    let opp_head = opponent.body[0];
    let head_distance = manhattan_distance(our_head, opp_head);

    // Opponent is threatening if head is close enough to actively trap us
    let threat_distance = if let Some(food) = nearest_food {
        let food_dist = manhattan_distance(our_head, food);
        // Opponent threatening if they can reach food area before/with us
        (food_dist + 2).min(6)  // Cap at 6 to avoid very distant snakes
    } else {
        4  // Default: heads within 4 moves are threats
    };

    head_distance <= threat_distance
}
```

**Modify `compute_space_score()` around line 2064:**
```rust
// Find nearest food for threat assessment
let nearest_food = if !board.food.is_empty() && !snake.body.is_empty() {
    let head = snake.body[0];
    board.food.iter()
        .min_by_key(|&&food| manhattan_distance(head, food))
        .copied()
} else {
    None
};

// Adversarial entrapment: check if nearby opponents can reduce our space
for &opp_idx in active_snakes {
    if opp_idx == snake_idx {
        continue;
    }

    // V10: Only consider threatening opponents
    if !Self::is_opponent_threatening(opp_idx, snake.body[0], nearest_food, board) {
        continue;
    }

    // ... rest of adversarial entrapment logic unchanged ...
}
```

### Change 2: Health-Aware Corner Danger
**File:** `src/bot.rs`
**Location:** Line 2362-2384 (`compute_corner_danger`)

**Goal:** Scale corner penalty by health - accept corner risk at critical health for food.

**Implementation:**
```rust
/// Computes corner danger penalty with health-aware scaling
/// V10: At critical health, accept corner risk if necessary for food
fn compute_corner_danger(
    pos: Coord,
    width: i32,
    height: i32,
    health: i32,
    config: &Config,
) -> i32 {
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

    // Apply penalty when within threshold
    if min_corner_dist <= config.scores.corner_danger_threshold {
        let base_penalty = config.scores.corner_danger_base / (min_corner_dist + 1);

        // V10: Scale penalty by health urgency
        // At critical health (<20), accept corner risk for food
        // At low health (20-50), reduce penalty to 50%
        // At high health (>50), full penalty
        let health_scale = if health < 20 {
            0.2  // 20% of normal penalty at critical health
        } else if health < 50 {
            0.5  // 50% of normal penalty at low health
        } else {
            1.0  // Full penalty at high health
        };

        -(base_penalty as f32 * health_scale) as i32
    } else {
        0
    }
}
```

**Update call site around line 2964:**
```rust
let (wall_penalty, center_bias, corner_danger) = if !snake.body.is_empty() {
    let head = snake.body[0];
    (
        Self::compute_wall_penalty(head, board.width as i32, board.height as i32, snake.health, config),
        Self::compute_center_bias(head, board.width as i32, board.height as i32, config),
        Self::compute_corner_danger(head, board.width as i32, board.height as i32, snake.health, config),  // ADD health
    )
} else {
    (0, 0, 0)
};
```

### Change 3: More Aggressive Critical Health Food Multipliers
**File:** `src/bot.rs`
**Location:** Lines 1917-1976 (distance-2 food multiplier logic)

**Goal:** Use max multiplier at health < 30 for distance-2 food (not just < 50).

**Implementation:**
```rust
} else if nearest_food_dist == 2 {
    // Distance 2: Check if we have clear advantage
    let nearest_opponent_dist = active_snakes.iter()
        .filter_map(|&opp_idx| {
            if opp_idx == snake_idx || opp_idx >= board.snakes.len() {
                return None;
            }
            let opp = &board.snakes[opp_idx];
            if opp.health <= 0 || opp.body.is_empty() {
                return None;
            }
            nearest_food.map(|f| manhattan_distance(opp.body[0], f))
        })
        .min()
        .unwrap_or(999);

    // V10: More aggressive at critical health
    if snake.health < 30 {
        // Critical health (<30): ALWAYS use max multiplier for distance-2 food
        config.scores.survival_max_multiplier
    } else if snake.health < 50 {
        // Low health (30-50): Use max multiplier only if clear advantage
        if nearest_opponent_dist >= nearest_food_dist + 3 {
            config.scores.survival_max_multiplier
        } else {
            // Moderate multiplier when contested
            config.scores.survival_max_multiplier * 0.6
        }
    } else if nearest_opponent_dist >= nearest_food_dist + 3 {
        // High health but clear advantage: use high multiplier
        config.scores.survival_max_multiplier * 0.8
    } else {
        // Contested: moderate multiplier
        config.scores.survival_max_multiplier * 0.2
    }
}
```

### Change 4: Configuration Updates
**File:** `Snake.toml`

**Add new parameters:**
```toml
# Adversarial Entrapment Constants (UPDATED)
# Distance threshold to consider opponent head as "nearby threat"
# V10: Used in threat filtering - only opponents within this distance are considered
adversarial_entrapment_distance = 4

# V10: NEW - Opponent Body Threat Buffer
# Only consider opponent bodies threatening if head is within (food_distance + this value)
adversarial_body_threat_buffer = 2
```

**Note:** Most other parameters remain unchanged. The changes are behavioral thresholds, not performance tuning.

### Change 5: Update Debug Log Path
**File:** `Snake.toml`
**Line:** 358

```toml
[debug]
# Enable debug mode to log game states, moves, and turns to disk
enabled = true
# Path to debug log file (relative to working directory)
log_file_path = "optimized_v10.jsonl"
```

### Change 6: Update Configuration Structs
**File:** `src/config.rs`

Add new field to Scores config:
```rust
pub struct ScoresConfig {
    // ... existing fields ...

    // V10: NEW
    pub adversarial_body_threat_buffer: i32,
}
```

Update `default_hardcoded()` method:
```rust
adversarial_body_threat_buffer: 2,
```

## Expected Behavioral Changes

### Turn 82 (Health=18, Food distance=2)
**Before:** Opponent at distance 4 with body at distance 2 → Avoid food (UP)
**After:** Opponent at distance 4 not considered threatening (4 > 2+2) → Consider food (may choose DOWN)
**Impact:** May still choose UP due to search tree depth, but food will be more attractive

### Turn 87 (Health=13, Food distance=1)
**Before:** Corner penalty -2500 overwhelms food decisions → Choose LEFT (corner)
**After:** Corner penalty -500 (20% of base) → Food bonus 37.5B dominates → Choose RIGHT (eat)
**Impact:** Should consistently eat adjacent food at critical health

### General
- More willing to pursue distance-2 food at health 15-25 (critical range)
- Less spooked by distant opponent bodies when their heads aren't threatening
- Corners acceptable as last resort at very low health

## Testing Strategy

### 1. Replay Verification
```bash
cargo build --release
./target/release/replay tests/fixtures/optimized_v9.1.2/game_04.jsonl --turns 82,86,87 --verbose
```

**Expected:**
- Turn 82: Score differential between UP and DOWN should narrow
- Turn 86: May show different choice avoiding corner approach
- Turn 87: Should choose RIGHT (eat food) with high confidence

### 2. Full Game Testing
```bash
# Play 4+ games with V10
# Analyze food pursuit patterns
./target/release/split_games optimized_v10.jsonl
./target/release/analyze_food_pursuit tests/fixtures/optimized_v10/

# Target: <5% food aversion rate
# V9.1.2 showed ~11% aversion (27 cases in Game 4)
```

### 3. Death Pattern Analysis
```bash
./target/release/analyze_deaths tests/fixtures/optimized_v10/
```

**Target:**
- Fewer starvation deaths
- Similar or lower trapped deaths
- No increase in head-to-head collision deaths

### 4. Comparative Replay
```bash
# Compare V9.1.2 vs V10 decisions on same board states
./target/release/replay tests/fixtures/optimized_v9.1.2/game_04.jsonl --all > v9.1.2_replay.txt
# After implementing V10:
./target/release/replay tests/fixtures/optimized_v9.1.2/game_04.jsonl --all > v10_replay.txt
diff v9.1.2_replay.txt v10_replay.txt
```

## Risk Assessment

### Low Risk ✅
- **Health-aware corner danger**: Clear improvement, no downside at high health
- **Critical health threshold (50→30)**: More conservative, actually reduces risk

### Medium Risk ⚠️
- **Opponent body threat filtering**: May miss some legitimate traps
  - Mitigation: Generous threshold (food_dist + 2), cap at 6
  - Monitoring: Watch for "obvious trap" deaths in testing

### Performance Impact 📊
- **Negligible**: All changes are simple arithmetic/comparisons
- No new flood fills, no additional search depth
- One extra distance calculation per opponent in space scoring (already O(n))

## Success Metrics

1. **Food Aversion Rate:** < 5% (from 11% in V9.1.2 Game 4)
2. **Starvation Deaths:** Reduce by 30%+
3. **Replay Match Rate:** > 90% on critical turns (82, 87)
4. **No Regression:** Head-to-head collision deaths should not increase

## GAPS.md Status Update

After implementation, update GAPS.md to remove stale items:

```markdown
## Verified Optimizations (Already Implemented)
- ✅ AdaptiveTimeEstimator actively used (bot.rs:947, 1049)
- ✅ Control maps use Vec, not HashMap (bot.rs:1719)
- ✅ IDAPOS aggressive (multiplier=1, max_distance=5)

## Remaining Optimization Opportunities
### Transposition Table Enhancement
- Store bounds (EXACT, LOWER_BOUND, UPPER_BOUND) not just scores
- Store best move for improved move ordering
- Implement TT-priority replacement scheme

### Move Ordering Improvements
- Killer moves already implemented ✅
- PV ordering already implemented ✅
- Consider history heuristic for quiet moves
```

## Implementation Order

1. ✅ Create V10_IMPLEMENTATION_PLAN.md (this file)
2. → Add `is_opponent_threatening()` helper function
3. → Modify `compute_space_score()` to filter opponents
4. → Update `compute_corner_danger()` signature and logic
5. → Update `compute_corner_danger()` call site
6. → Adjust distance-2 food multiplier thresholds
7. → Add config parameters to Snake.toml
8. → Add config fields to src/config.rs
9. → Update debug log path in Snake.toml
10. → Build and verify compilation
11. → Run replay tests on Game 4 Turn 82, 87
12. → Update GAPS.md with V10 summary
13. → Play full games and analyze results
