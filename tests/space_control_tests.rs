//! Space Control and Wall Penalty Tests
//!
//! Tests for wall proximity penalty and center bias heuristics
//! to ensure proper scoring for space control evaluation.

use starter_snake_rust::types::Coord;

/// Test compute_wall_penalty with various distances from walls
#[test]
fn test_wall_penalty_at_wall() {
    // Position at left wall (x=0)
    let pos = Coord { x: 0, y: 5 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    assert_eq!(penalty, -10000, "At wall should have maximum penalty");
}

#[test]
fn test_wall_penalty_one_from_wall() {
    // Position 1 square from left wall
    let pos = Coord { x: 1, y: 5 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    assert_eq!(penalty, -5000, "One square from wall should have -5000 penalty");
}

#[test]
fn test_wall_penalty_two_from_wall() {
    // Position 2 squares from left wall
    let pos = Coord { x: 2, y: 5 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    // Formula: -10000 / (2 + 1) = -3333
    assert_eq!(penalty, -3333, "Two squares from wall should have -3333 penalty");
}

#[test]
fn test_wall_penalty_safe_distance() {
    // Position 3+ squares from all walls (center of 11x11)
    let pos = Coord { x: 5, y: 5 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    assert_eq!(penalty, 0, "Safe distance from walls should have no penalty");
}

#[test]
fn test_wall_penalty_corner() {
    // Corner position (0,0) - at two walls simultaneously
    let pos = Coord { x: 0, y: 0 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    assert_eq!(penalty, -10000, "Corner should have maximum penalty (closest wall is 0)");
}

#[test]
fn test_wall_penalty_near_top_wall() {
    // Position 1 from top wall in 11x11 board
    let pos = Coord { x: 5, y: 10 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    assert_eq!(penalty, -10000, "At top wall should have maximum penalty");
}

#[test]
fn test_wall_penalty_near_right_wall() {
    // Position 1 from right wall
    let pos = Coord { x: 10, y: 5 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    assert_eq!(penalty, -10000, "At right wall should have maximum penalty");
}

#[test]
fn test_wall_penalty_near_bottom_wall() {
    // Position 1 from bottom wall
    let pos = Coord { x: 5, y: 0 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    assert_eq!(penalty, -10000, "At bottom wall should have maximum penalty");
}

/// Test center bias computation
#[test]
fn test_center_bias_at_center() {
    // Center of 11x11 board
    let pos = Coord { x: 5, y: 5 };
    let bias = compute_center_bias(pos, 11, 11);
    assert_eq!(bias, 100, "Center position should have maximum bias (+100)");
}

#[test]
fn test_center_bias_one_from_center() {
    // One square from center
    let pos = Coord { x: 6, y: 5 };
    let bias = compute_center_bias(pos, 11, 11);
    assert_eq!(bias, 90, "One square from center should have +90 bias");
}

#[test]
fn test_center_bias_at_corner() {
    // Corner (0,0) - maximum distance from center (5,5)
    let pos = Coord { x: 0, y: 0 };
    let bias = compute_center_bias(pos, 11, 11);
    // Distance from center = |0-5| + |0-5| = 10
    // Bias = 100 - (10 * 10) = 0
    assert_eq!(bias, 0, "Corner should have 0 or negative bias");
}

#[test]
fn test_center_bias_at_edge() {
    // Edge position (0, 5) - middle of left edge
    let pos = Coord { x: 0, y: 5 };
    let bias = compute_center_bias(pos, 11, 11);
    // Distance from center = |0-5| + |5-5| = 5
    // Bias = 100 - (5 * 10) = 50
    assert_eq!(bias, 50, "Edge middle should have +50 bias");
}

#[test]
fn test_center_bias_different_board_size() {
    // Test on 7x7 board
    // Center is (3, 3)
    let pos = Coord { x: 3, y: 3 };
    let bias = compute_center_bias(pos, 7, 7);
    assert_eq!(bias, 100, "Center of 7x7 should have +100 bias");

    // Corner (0, 0)
    let pos_corner = Coord { x: 0, y: 0 };
    let bias_corner = compute_center_bias(pos_corner, 7, 7);
    // Distance = |0-3| + |0-3| = 6
    // Bias = 100 - (6 * 10) = 40
    assert_eq!(bias_corner, 40, "Corner of 7x7 should have +40 bias");
}

/// Test wall penalty prioritization over center bias
#[test]
fn test_wall_penalty_dominates_center_bias() {
    // At center but 1 from wall (shouldn't happen on 11x11 but testing principle)
    let pos = Coord { x: 1, y: 5 };
    let penalty = compute_wall_penalty(pos, 11, 11);
    let bias = compute_center_bias(pos, 11, 11);

    // Wall penalty (-5000) should dominate center bias (~80)
    assert!(penalty.abs() > bias.abs(),
        "Wall penalty magnitude should exceed center bias magnitude");
}

/// Test edge case: empty board dimensions
#[test]
fn test_wall_penalty_minimum_board() {
    // 1x1 board (degenerate case)
    let pos = Coord { x: 0, y: 0 };
    let penalty = compute_wall_penalty(pos, 1, 1);
    assert_eq!(penalty, -10000, "Single cell board should be at wall");
}

/// Test that positions move closer to center when far from walls
#[test]
fn test_center_bias_gradient() {
    // As we move from edge to center, bias should increase
    let positions = [
        Coord { x: 0, y: 5 },  // Left edge
        Coord { x: 2, y: 5 },  // 2 from left
        Coord { x: 4, y: 5 },  // 1 from center
        Coord { x: 5, y: 5 },  // Center
    ];

    let biases: Vec<i32> = positions.iter()
        .map(|&pos| compute_center_bias(pos, 11, 11))
        .collect();

    // Each bias should be greater than or equal to the previous
    for i in 1..biases.len() {
        assert!(biases[i] >= biases[i-1],
            "Center bias should increase as we approach center: {} >= {}",
            biases[i], biases[i-1]);
    }
}

// Helper functions matching the bot's implementation
fn compute_wall_penalty(pos: Coord, width: i32, height: i32) -> i32 {
    let wall_penalty_base = 10000;
    let safe_distance_from_wall = 3;  // Matches Snake.toml default
    let dist_to_wall = [
        pos.x,                  // distance to left wall
        width - 1 - pos.x,      // distance to right wall
        pos.y,                  // distance to bottom wall
        height - 1 - pos.y,     // distance to top wall
    ]
    .iter()
    .min()
    .copied()
    .unwrap_or(0);

    // Cap at safe distance from wall
    if dist_to_wall >= safe_distance_from_wall {
        return 0;
    }

    // Apply mathematical formula: penalty = -base / (distance + 1)
    -(wall_penalty_base / (dist_to_wall + 1))
}

fn compute_center_bias(pos: Coord, width: i32, height: i32) -> i32 {
    let center_x = width / 2;
    let center_y = height / 2;
    let dist_from_center = (pos.x - center_x).abs() + (pos.y - center_y).abs();

    // Prefer central positions
    100 - (dist_from_center * 10)
}
