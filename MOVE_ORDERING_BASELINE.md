# Move Ordering Performance Baseline

## Purpose
Track performance improvements from move ordering (PV + killer heuristic) implementation.

## Baseline Metrics (Before Move Ordering)
Collected: 2025-10-28
Dataset: tests/fixtures/balanced/ (5 games, 641 moves)

### Timing Statistics
- **Average latency:** 74.1ms (21% of 350ms budget)
- **Unused budget:** ~276ms (79% unused!)
- **Max latency:** 412ms (1 timeout in game_05)
- **Min latency:** 21ms
- **Median latency:** ~57ms

Time budget utilization:
- Avg < 200ms: 5 games (100.0%)
- Avg < 300ms: 5 games (100.0%)
- Avg < 350ms: 5 games (100.0%)
- Avg >= 350ms: 0 games (0.0%)

### Performance Statistics
- **Total games:** 5
- **Total moves:** 641
- **Average game length:** 128.2 turns
- **Shortest game:** 32 turns (game_02)
- **Longest game:** 284 turns (game_05)

### Game Distribution
- Quick games (<100 turns): 3 games
- Medium games (100-200 turns): 1 game
- Long games (200-300 turns): 1 game

### Configuration (Before Move Ordering)
```toml
[timing]
initial_depth = 2
min_time_remaining_ms = 20
max_search_depth = 20

[time_estimation.one_vs_one]
base_iteration_time_ms = 0.01
branching_factor = 3.25

[time_estimation.multiplayer]
base_iteration_time_ms = 0.01
branching_factor = 2.25

[move_ordering]
# NOT YET INTEGRATED - structures exist but not used
killer_moves_per_depth = 2
enable_pv_ordering = true
enable_killer_heuristic = true
```

## Expected Improvements After Integration

Based on literature and GAPS.md analysis:
- **Alpha-beta efficiency:** 50-80% improvement in pruning
- **Search depth:** +2 to +4 levels deeper
- **Time utilization:** Better use of the 79% unused budget
- **Move quality:** Improved by searching further ahead
- **Horizon effect:** Reduced by seeing 2-4 turns further

## Metrics to Collect After Integration

Run same analysis on same dataset:
```bash
cargo run --release --bin analyze_timing tests/fixtures/balanced/
cargo run --release --bin analyze_performance tests/fixtures/balanced/
```

### Expected Changes
1. **Average latency should INCREASE** (using more of the budget)
   - Target: 150-250ms (40-70% of budget)
   - Indicates deeper search

2. **Search depth should INCREASE**
   - Baseline: Unknown (not yet instrumented)
   - Target: +2 to +4 levels on average

3. **Timeout risk should remain stable**
   - Max latency should stay < 400ms
   - Better move ordering → better cutoffs → controlled time

4. **Game outcomes might improve**
   - Fewer trap deaths
   - Better strategic decisions
   - Need new game logs to measure

## Validation Criteria

Move ordering is successful if:
- ✓ Average latency increases 50-150ms (deeper search)
- ✓ Max latency remains < 400ms (no new timeouts)
- ✓ No regressions in existing games (replay tests pass)
- ✓ Search depth increases by 2+ levels (once instrumented)

## Notes

- Baseline logs do NOT contain depth metadata
- Need to add depth logging to measure actual improvement
- Current 79% unused budget suggests huge opportunity
- One timeout (412ms) suggests branching factor is already slightly aggressive
