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

The MaxN algorithm is a multi-player variant of MiniMax that:
1. Evaluates optimal moves for all snakes simultaneously
2. Uses per-snake scoring functions to determine our best move
3. **MUST** be implemented using `rayon` for parallel evaluation
4. **MUST** return within `RESPONSE_TIME_BUDGET_MS` to avoid timeout

## Adaptive Execution Strategy

**Single CPU scenario**: Gracefully degrade to breadth-first evaluation across all snakes sequentially.

**Multiple CPUs + single opponent**: Fan out across all possible directions; update result only if new move weight is superior.

**Timing management**:
- Poll every `POLLING_INTERVAL_MS` to compute current best move
- Track average compute time per iteration
- Return early if next poll would exceed remaining time budget

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