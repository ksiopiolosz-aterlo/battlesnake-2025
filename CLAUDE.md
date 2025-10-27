# Project Overview

You are collaborating on a competitive **Battlesnakes** bot (https://docs.battlesnake.com/) written in Rust. The goal is to build a high-performance snake AI that uses the MaxN algorithm to evaluate moves and outmaneuver opponents within strict time constraints.

## Configuration Parameters

These are tunable parameters (consider externalizing to a config file):
- `RESPONSE_TIME_BUDGET_MS`: Maximum response time for move endpoint (default: 400ms)
- `POLLING_INTERVAL_MS`: How often to recompute optimal move (default: 50ms)
- Weights for scoring function (see Priorities section)

---

# Code Style

## Core Principles
- **IMPORTANT**: Use OOP-style patterns in Rust for clear object representation and maintainability
- **MUST NEVER** use `unsafe` code blocks
- Prefer simple, straightforward representations over complex abstractions
- Keep functions small and focused (cognitive complexity < 15)

## Concurrency & Performance
- **I/O-bound tasks** (async API endpoints): Use `tokio`
- **CPU-bound tasks** (move computation): Use `rayon`
- Prefer atomics over locks; if locks required, use `parking_lot` over `std`
- Avoid cloning wherever possible:
  - Pass read-only data by immutable reference
  - Use `Arc` only when crossing thread boundaries
- Minimize memory contention hotspots:
  - Example: Use per-thread atomics rather than a single shared atomic

## Constants & Configuration
- Use constants for all magic numbers
- Consider externalizing to configuration file (e.g., `Snake.toml`)

---

# Workflow

## Development Process
1. Make code changes following the style guide
2. **IMPORTANT**: Validate compilation via `cargo build` and resolve all compiler errors
3. Run static analysis tools to catch issues
4. **Default behavior**: Do NOT generate tests unless explicitly requested

## Testing
- Test format: JSON input matching existing data contracts
- **MUST** clarify expectations if requirements are vague or unclear
- Run targeted tests (single test or small subset) rather than full suite for performance

---

# Algorithm Implementation

## MaxN Algorithm Overview

MaxN is a multi-player adversarial search algorithm that generalizes MiniMax to N players. Unlike zero-sum two-player games, MaxN handles scenarios where each player independently maximizes their own utility.

**Core Mechanics**:
1. **Tree Construction**: Build game tree to depth D, where each node represents a board state
   - Each level represents sequential or simultaneous moves by all snakes
   - Branch factor: 4 directions (up, down, left, right) per snake
   - Prune illegal moves immediately (collisions, out-of-bounds)

2. **Evaluation Function**: At leaf nodes (depth D) or terminal states, compute score tuple `[score_0, score_1, ..., score_n]`
   - Each element represents utility for corresponding snake
   - Scores are non-zero-sum: players have independent objectives

3. **Backpropagation**: At each internal node, each snake selects the child node maximizing **its own** score component
   - In multiplayer: each player's move choice affects the resulting tuple
   - In 1v1: simplifies to evaluating all move pair combinations

## Required Algorithms & Heuristics

### Move Validation & Safety
- **Collision Detection**: Check wall boundaries, self-collision, and body collisions (O(1) with spatial hashing)
- **Head-to-Head Analysis**: When snakes can collide head-on, shorter snake loses; equal length = both eliminated

### Space Analysis
- **Flood Fill**: Calculate reachable cells from each potential position (BFS, O(W×H))
  - Accounts for snake body positions that will vacate over time
  - Critical for avoiding enclosed spaces (traps)
  - Heuristic: avoid moves where `reachable_space < snake_length + margin`

### Food Acquisition
- **A\* Pathfinding**: Compute shortest path distance to nearest food (O(W×H log(W×H)))
  - Use Manhattan distance heuristic for admissibility
  - Factor in dynamic obstacles (snake bodies that will move)

### Territory Control
- **Voronoi Regions**: Partition board into zones each snake controls (approximated via flood fill from each head)
  - Measure: `controlled_cells / total_free_cells`
  - Higher control correlates with strategic advantage

## Evaluation Function Weights

Compute weighted sum for each snake's score component:

```
score = w_survival * survival_score
      + w_space * space_score
      + w_food * food_score
      + w_control * control_score
      + w_attack * attack_score
```

**Weight Components** (suggested starting values, tune experimentally):

| Component | Weight | Description |
|-----------|--------|-------------|
| `w_survival` | 1000.0 | Immediate death = -∞; valid moves = 0; head-to-head disadvantage = -500 |
| `w_space` | 10.0 | Flood fill reachable cells; penalize cramped positions |
| `w_food` | 5.0 | Inverse distance to food, scaled by health urgency `(100 - health)/100` |
| `w_control` | 3.0 | Voronoi region size / total free space |
| `w_attack` | 2.0 | Trap potential (opponent reachable space < threshold) |

**Critical Rules**:
- Any move resulting in immediate death: `score = -∞`
- Survival always dominates other factors
- Food urgency: scale `w_food` proportionally as health depletes

## Tree Exploration Strategy

**Depth Selection**:
- **1v1**: Target depth 8-12 (4^2 = 16 to 4^2 = 16,777,216 nodes at leaves, use iterative deepening)
- **Multiplayer (N snakes)**: Target depth 4-6 (4^N branches per level; exponential growth limits practical depth)

**Iterative Deepening**:
- Start at depth 2, incrementally increase depth as time permits
- Maintain best move found so far (anytime algorithm property)
- Ensures valid move even if interrupted

**Move Ordering** (for better pruning/early cutoffs):
1. Moves toward food (if health < 30)
2. Moves maximizing flood fill space
3. Moves maintaining current strategy (center control, trap persistence)
4. Moves toward enemy (offensive pressure)

**Pruning Techniques**:
- **Immediate Pruning**: Eliminate illegal moves at root (collisions, walls)
- **Shallow Pruning**: At depth 1-2, eliminate moves with flood fill < snake length
- **Dominated Move Elimination**: If move A is strictly worse than move B across all scenarios, prune A
- Note: Alpha-beta pruning doesn't apply (non-zero-sum), but can use paranoid or best-reply search variants for multiplayer

## Adaptive Execution Strategy

**1v1 Scenario** (2 snakes):
- **Parallelization**: Use `rayon` to fan out across our 4 possible first moves
- Each thread evaluates full subtree for one root move
- Atomic shared state: `Arc<AtomicU8>` tracking best move index, `Arc<AtomicI32>` for best score
- Thread updates best move **only if** its score > current best (lock-free compare-and-swap)
- Result: 4-way parallelism on multi-core systems

**Multiplayer Scenario** (3+ snakes):
- **Parallel Game State Evaluation**: Distribute leaf node evaluations across threads
- Use work-stealing scheduler (rayon default) for load balancing
- Intermediate nodes aggregate results serially (minimal overhead)
- Gracefully degrades to sequential BFS on single-core systems

**Timing Management** (Anytime Algorithm):
1. Reserve buffer: `EFFECTIVE_BUDGET = RESPONSE_TIME_BUDGET_MS - 50ms` (network/overhead)
2. Iterative deepening loop:
   ```
   depth = 2
   while elapsed < EFFECTIVE_BUDGET:
       best_move = maxn_search(depth, remaining_time)
       depth += 1
       if estimated_next_iteration_time > remaining_time:
           break
   return best_move
   ```
3. **Time Estimation**: Track average nodes/ms during search; estimate next depth cost = `current_nodes × branching_factor × avg_time_per_node`
4. **Polling**: Every `POLLING_INTERVAL_MS`, check elapsed time and update best move cache

**Graceful Degradation**:
- **Single CPU**: Sequential move evaluation; no parallel overhead
- **Timeout Risk**: Return best move from deepest completed iteration
- **No Valid Move Found**: Fallback to simple heuristic (maximize immediate flood fill space)

---

# Constraints

- **CRITICAL**: The `/move` endpoint MUST respond in < `RESPONSE_TIME_BUDGET_MS`

---

# Decision Priorities

## Our Snake (in order of importance)

1. **Survival** (highest priority)
   - MUST avoid walls and snake collisions
   - Defensively maneuver to evade threats

2. **Food acquisition**
   - Obtain food when trap/collision weight is acceptably low
   - Balance risk vs. health needs

3. **Offensive trapping**
   - Calculate trap weight: probability of successfully enclosing an opponent
   - Maintain trap once established (unless health critical)
   - MUST NOT collect food inside our own trap perimeter

## Opponent Modeling (for MaxN evaluation)

Assume opponents prioritize:
1. Survival (avoid walls and our snake)
2. Attacking us (trap or collision attempts)
3. Food acquisition