
### 1. **History Heuristic** (Biggest Missing Piece)
This would complement killers nicely:

```rust
pub struct HistoryTable {
    // Track move success rates globally (not depth-specific like killers)
    // Index by [from_x][from_y][direction]
    scores: Vec<Vec<Vec<i32>>>,
    // maybe a nicer way to store this? 
}

impl HistoryTable {
    pub fn update(&mut self, pos: Coord, dir: Direction, depth: u8, caused_cutoff: bool) {
        let bonus = if caused_cutoff { 
            1 << depth  // Exponential bonus by depth
        } else { 
            -(1 << (depth / 2))  // Smaller penalty for non-cutoff moves
        };
        self.scores[pos.x as usize][pos.y as usize][dir as usize] += bonus;
    }
}
```

### 2. **Store Best Move in TT**
Your transposition table should remember which move was best:

```rust
struct TranspositionEntry {
    score: i32,
    depth: u8,
    best_move: Option<Direction>,  // Add this!
    age: u32,
}

// Then use it for move ordering:
if let Some(entry) = tt.probe_with_move(board_hash, depth) {
    pv_move = entry.best_move;  // Use TT move as PV
}
```

### 3. **Late Move Reductions (LMR)**
After trying the first few moves at full depth, search remaining moves at reduced depth:

```rust
for (move_idx, mv) in moves.iter().enumerate() {
    let reduced_depth = if move_idx < 3 || is_killer || is_capture {
        depth - 1  // Full depth for promising moves
    } else {
        depth.saturating_sub(2)  // Reduced depth for likely-bad moves
    };

    let score = search(child_board, reduced_depth, ...);

    // If reduced search finds something good, re-search at full depth
    if reduced_depth < depth - 1 && score > alpha {
        score = search(child_board, depth - 1, ...);
    }
}
```

### 4. **Aspiration Windows**
Instead of starting with [-∞, +∞] window:

```rust
let mut alpha = last_score - 50;
let mut beta = last_score + 50;
let mut score = alpha_beta(board, depth, alpha, beta, ...);

// If we fail high or low, re-search with wider window
while score <= alpha || score >= beta {
    if score <= alpha { alpha -= 200; }
    if score >= beta { beta += 200; }
    score = alpha_beta(board, depth, alpha, beta, ...);
}
```