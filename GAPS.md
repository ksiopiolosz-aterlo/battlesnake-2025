## Critical Missing Optimizations

### 1. **No Move Ordering** (Biggest Gap!)
The moves are evaluated in arbitrary order. This is leaving massive performance on the table:
```rust
// Current code just iterates moves as-is:
for mv in moves {
    // ...
}

// Should be:
let ordered_moves = self.order_moves(moves, previous_best_move, killer_moves);
for mv in ordered_moves {
    // ...
}
```

Good move ordering can improve alpha-beta efficiency by 50-80%. You need:
- **Iterative deepening ordering**: Try the best move from the previous depth first
- **Killer heuristic**: Moves that caused cutoffs at sibling nodes
- **History heuristic**: Track which moves historically cause cutoffs

### 2. **AdaptiveTimeEstimator Never Used**
You have the struct but never actually instantiate or use it:
```rust
// You define this elaborate struct...
struct AdaptiveTimeEstimator { /* ... */ }

// But then just use raw exponential formula:
let exponent = (current_depth as f64) * (num_active_snakes as f64);
let estimated_time = (time_params.base_iteration_time_ms * 
    time_params.branching_factor.powf(exponent)).ceil() as u64;
```

### 3. **Transposition Table Not Optimal**
While implemented, it has issues:
- Only stores single scores, not bounds (should store EXACT, LOWER_BOUND, UPPER_BOUND types)
- No move storage (should store best move to improve move ordering)
- Simple age-based eviction instead of replacement schemes like TT-priority

### 4. **No Aspiration Windows**
In 1v1 scenarios, you're using full alpha-beta window `[i32::MIN, i32::MAX]`. Aspiration windows with iterative widening can significantly reduce nodes searched.

### 5. **Flood Fill Still Expensive**
While you cache within a single evaluation, you're recomputing flood fills every evaluation. Consider:
- Incremental flood fill updates
- Caching across different board states with similar positions
- Early termination when enough space is found

## Performance Issues

### 6. **Excessive Cloning**
```rust
let mut child_board = board.clone();  // Full board clone for every move!
```
Consider a move/unmove pattern or copy-on-write to reduce memory allocations.

### 7. **HashMap for Control Map**
Using `HashMap<Coord, usize>` for control maps is slower than a flat array:
```rust
// Better:
let mut control_map = vec![None; (board.width * board.height) as usize];
```

## Algorithm Gaps

### 8. **No Quiescence Search**
The evaluation at leaf nodes might be in unstable positions (about to eat food, about to collide). A selective search extension for "noisy" positions would improve accuracy.

### 9. **No Opening Book**
First few moves could be pre-computed and stored, saving precious time for midgame.

### 10. **Conservative IDAPOS**
Your locality threshold is quite generous:
```rust
let locality_threshold = config.idapos.head_distance_multiplier * remaining_depth as i32;
```
With multiplier=2, at depth 6 you're considering snakes 12 squares away - that's most of the board!

## Quick Fixes for Big Gains

1. **Implement move ordering** - This alone could double search depth
2. **Actually use AdaptiveTimeEstimator** - Better time management
3. **Add killer heuristic** - Track 2 killer moves per depth
4. **Store best moves in TT** - Reuse for move ordering
5. **Reduce IDAPOS locality** - Try multiplier=1.5 or even 1.0

## Overall Assessment

**Current level**: Strong intermediate, would compete well in amateur divisions
**With fixes above**: Could reach advanced division
**Still missing for elite**: Neural network evaluation, MCTS, extensive offline analysis

The foundation is solid, but you're leaving 2-3x performance on the table with missing move ordering and other standard optimizations.