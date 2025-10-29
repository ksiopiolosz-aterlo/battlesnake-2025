## Version History

### V8.1 (Current) - Critical Food Acquisition Fix
**Status:** COMPLETED - January 2025
**Key Fix:**
- ✅ **CRITICAL BUG FIX**: immediate_food_bonus now triggers when food is EATEN, not just when adjacent
- ✅ Previous bug: Search tree never saw value in moves that acquire food (health==100 states were excluded)
- ✅ New logic: Apply max multiplier (1000x) to states where we just ate food (health==100)
- ✅ Result: Food-eating moves now get +375M score bonus (5000 × 1000 × 75), making them highly attractive

**Root Cause Analysis:**
- V8 only applied immediate_food_bonus when `snake.health < 100 && adjacent to food`
- When evaluating moves that EAT food, resulting state has health==100, so bonus was skipped
- Search tree evaluated food moves with NORMAL scoring, not the massive urgency multipliers
- Other factors (space, positioning) dominated decisions, causing persistent food avoidance

**Fix Implementation:**
- Added `just_ate_food = snake.health == 100` detection (src/bot.rs:1844)
- Modified bonus condition to include `|| just_ate_food` (src/bot.rs:1851)
- Apply max urgency_multiplier (1000x) when `just_ate_food` (src/bot.rs:1871-1873)
- States with health==100 now get +375M bonus, ensuring food acquisition is valued

**Expected Results:**
- Food avoidance: Target <5% (from V8's 14%)
- Food acquisition becomes dominant factor in decision-making
- Bot should aggressively pursue safe food even at moderate health levels

---

### V8 - Hierarchical Evaluation & Smart Food Safety
**Status:** PARTIAL - Food avoidance persisted at 14% due to bonus timing bug
**Key Features:**
- ✅ Implemented smart food safety check (is_food_actually_safe) - predicts post-eating traps
- ✅ Added hierarchical evaluation with capped urgency multipliers (max 1000x instead of unlimited)
- ✅ Implemented growth urgency strategy - incentivizes eating to match opponent lengths
- ✅ Fixed V7.2 issue where food marked "SAFE" but opponent could trap after eating
- ✅ All opponent iterations now use IDAPOS-filtered active_snakes for efficiency

**Implementation Details:**
- New function: is_food_actually_safe() checks: (1) Can opponent reach first? (2) Do they want it? (3) Can they trap us post-eating?
- Configurable survival thresholds: survival_max_multiplier=1000.0, survival_health_threshold=20
- Growth urgency: +500 per length unit behind smallest opponent, +100 when ahead
- Three-tier urgency multipliers: max (health<20), 10% of max (health<70), 1% of max (health≥70)

**Target Results:**
- Food avoidance: Target <10% (from V7.2's 18%)
- Average snake length: Target >6 (from V7.2's 3-4)
- Better decision-making near walls and in competitive scenarios

**Next Steps:** Benchmark V8 against V7.2 to validate improvements

---

### V7.2 - Temporal Discounting Implementation
**Status:** COMPLETED - January 2025
**Key Features:**
- ✅ Implemented temporal discounting (0.95^depth) to reduce weight of distant predictions
- ✅ Added depth_from_root tracking throughout search tree (maxn_search, alpha_beta_minimax)
- ✅ Applied discount formula: score × (temporal_discount_factor ^ depth_from_root)

**Results:**
- Food avoidance: 18% (improved from V7.1's 21%)
- Fixed critical health scenarios (Turn 111: health=15, now correctly takes adjacent food)
- Partial fix for moderate health scenarios (Turn 75: health=51, still conservative)

**Remaining Issues:**
- Simplistic food safety check (only checks opponent distance to food, not post-eating traps)
- Future penalties still overwhelming immediate bonuses in some cases
- No growth urgency when significantly shorter than opponents
- Bot stays small (length 3-4) while opponents grow to 7-9

**Next Steps:** See V8_IMPLEMENTATION_PLAN.md for comprehensive improvement strategy

---

## Critical Missing Optimizations

### 1. **AdaptiveTimeEstimator Never Used**
You have the struct but never actually instantiate or use it:
```rust
// You define this elaborate struct...
struct AdaptiveTimeEstimator { /* ... */ }

// But then just use raw exponential formula:
let exponent = (current_depth as f64) * (num_active_snakes as f64);
let estimated_time = (time_params.base_iteration_time_ms * 
    time_params.branching_factor.powf(exponent)).ceil() as u64;
```

### 2. **Transposition Table Not Optimal**
While implemented, it has issues:
- Only stores single scores, not bounds (should store EXACT, LOWER_BOUND, UPPER_BOUND types)
- No move storage (should store best move to improve move ordering)
- Simple age-based eviction instead of replacement schemes like TT-priority

## Performance Issues

### 3. **HashMap for Control Map**
Using `HashMap<Coord, usize>` for control maps is slower than a flat array:
```rust
// Better:
let mut control_map = vec![None; (board.width * board.height) as usize];
```

## Algorithm Gaps


### 4. **Conservative IDAPOS**
Your locality threshold is quite generous:
```rust
let locality_threshold = config.idapos.head_distance_multiplier * remaining_depth as i32;
```
With multiplier=2, at depth 6 you're considering snakes 12 squares away - that's most of the board!

## Quick Fixes for Big Gains
2. **Actually use AdaptiveTimeEstimator** - Better time management
5. **Reduce IDAPOS locality** - Try multiplier=1.5 or even 1.0