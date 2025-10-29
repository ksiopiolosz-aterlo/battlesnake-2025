# Optimization Opportunities Identified During V8 Implementation

This document tracks potential optimizations and algorithm improvements noticed during V8 development.

## Date: 2025-10-29

### Data Structure Improvements

1. **Control Map Performance** (GAPS.md #3 - confirmed during V7.2 review)
   - Current: `HashMap<Coord, usize>` for adversarial_flood_fill
   - Better: `Vec<Option<usize>>` with flat array indexed by `y * width + x`
   - Benefit: Cache-friendly access, ~2-3x faster lookups
   - Impact: Medium (used once per evaluation)

2. **Coordinate Hashing**
   - Current: Coord struct used as HashMap key
   - Consider: Pack (x, y) into single u16 (if board < 256x256)
   - Benefit: Smaller keys, faster hashing
   - Impact: Low (TT already uses board hash, not coord hash)

### Algorithm Improvements

3. **IDAPOS Active Snake Determination** (V8 Task 4)
   - Current: Manhattan distance check
   - V8: Flood-fill reachability check
   - Further: Cache reachable cells per depth level (avoid recomputation)
   - Benefit: ~30-40% faster active snake filtering in complex positions

4. **Flood Fill Early Termination**
   - Current: Floods entire reachable space
   - Better: Early exit when target count reached (e.g., "do we have 10+ cells?")
   - Use case: Space safety checks only need threshold, not exact count
   - Benefit: 2x faster in large open areas

5. **Move Ordering Caching**
   - Current: History table cleared each iteration
   - V8: Persistent with decay
   - Further: Cache PV (principal variation) moves across game turns
   - Benefit: Better move ordering on similar positions

### Evaluation Function

6. **Escape Route Counting**
   - Current: Simulates eating food, generates moves, counts
   - Optimization: Pre-compute legal moves, cache per position
   - Benefit: ~1.5x faster (called frequently)

7. **Lazy Evaluation Components**
   - Current: All components computed every evaluation
   - Better: Hierarchical early-exit (safety veto → survival → tactical)
   - If safety veto triggers (i32::MIN), skip expensive computations
   - Benefit: 10-15% faster in obvious bad positions

### Search Improvements

8. **Transposition Table Entry Size**
   - Current: Stores score + depth + bound_type + best_move + age
   - Consider: Separate tables for frequent vs rare positions
   - Hot table: Recent/high-depth entries (fast eviction)
   - Cold table: Deep historical entries (slow eviction)
   - Benefit: Better hit rates, less thrashing

9. **Aspiration Window Tuning**
   - Current: Fixed initial window (±50)
   - Better: Adaptive based on score stability
   - If last 2 iterations stable: narrow window (±25)
   - If volatile: wider window (±100)
   - Benefit: Fewer re-searches

10. **Parallel Search Load Balancing**
    - Current: Each root move gets one thread
    - Issue: Uneven work distribution (some moves deeper than others)
    - Better: Work-stealing queue or dynamic allocation
    - Benefit: Better CPU utilization

### Memory Optimizations

11. **Board Clone Reduction**
    - Current: Clone board for every child node
    - Better: Move-make/unmake pattern with single board
    - Challenge: Rust ownership (need unsafe or complex lifetimes)
    - Benefit: 3-5x faster (no allocations in hot path)
    - Risk: High complexity, error-prone

12. **Snake Body Representation**
    - Current: Vec<Coord> for each snake
    - Consider: Circular buffer with head/tail pointers
    - Benefit: O(1) move application (just update head pointer)
    - Challenge: More complex but potentially worth it

### Time Management

13. **Adaptive Branching Factor** (GAPS.md #1 - confirmed never used)
    - Current: AdaptiveTimeEstimator exists but unused
    - Implementation: Track actual vs predicted times, adjust factor
    - Benefit: Reach optimal depth more consistently
    - Note: This was the original plan but never wired up

14. **Depth-Selective Search**
    - Current: Uniform depth across all moves
    - Better: Deep search promising moves, shallow search obviously bad ones
    - Criteria: TT hit with good bound, killer move, captures food
    - Benefit: Effective depth increase with same time budget

### Code Quality

15. **Active Snakes Usage Consistency** (User reminder during V8)
    - Issue: Some functions enumerate all snakes, some use active_snakes
    - Fix: Always pass active_snakes and use filtered iteration
    - Benefit: Consistent performance, avoid redundant work
    - Status: Being addressed in V8 implementation

16. **Function Complexity**
    - compute_health_score: ~150 lines, cyclomatic complexity ~15
    - Consider: Break into sub-functions (check_food_safety, compute_urgency, etc.)
    - Benefit: Maintainability, easier testing

## Implementation Priority

**High Impact, Low Effort:**
- [#4] Flood fill early termination
- [#6] Escape route caching
- [#7] Lazy evaluation
- [#15] Active snakes consistency (in progress)

**High Impact, Medium Effort:**
- [#3] IDAPOS reachability caching
- [#5] PV move caching
- [#13] Adaptive branching factor (resurrect existing code)

**High Impact, High Effort:**
- [#11] Move make/unmake pattern (big refactor)
- [#14] Depth-selective search
- [#10] Parallel load balancing

**Research Needed:**
- [#12] Snake body circular buffer (benchmark first)
- [#8] Two-tier TT (complex eviction logic)

---

## Notes

- Many optimizations trade code complexity for performance
- Focus on hot path optimizations (search, evaluation, move gen)
- Profile before optimizing (use BATTLESNAKE_PROFILE=1)
- Maintain type safety (avoid unsafe unless absolutely necessary)
