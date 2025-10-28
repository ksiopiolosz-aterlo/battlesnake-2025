// Unit tests for replay module
//
// Tests the core functionality of the replay engine including:
// - Loading JSONL log files
// - Replaying individual turns
// - Validating expected moves
// - Generating statistics

use starter_snake_rust::config::Config;
use starter_snake_rust::replay::ReplayEngine;
use starter_snake_rust::types::Direction;
use std::path::PathBuf;

/// Helper function to get the path to test fixtures
fn fixture_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(filename)
}

#[test]
fn test_load_log_file_survival_basic() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("survival_basic.jsonl"))
        .expect("Failed to load survival_basic.jsonl");

    assert_eq!(entries.len(), 2, "Expected 2 log entries");
    assert_eq!(entries[0].turn, 0, "First entry should be turn 0");
    assert_eq!(entries[0].chosen_move, "up", "First move should be up");
    assert_eq!(entries[1].turn, 1, "Second entry should be turn 1");
    assert_eq!(entries[1].chosen_move, "left", "Second move should be left");
}

#[test]
fn test_load_log_file_food_acquisition() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("food_acquisition.jsonl"))
        .expect("Failed to load food_acquisition.jsonl");

    assert_eq!(entries.len(), 4, "Expected 4 log entries");

    // Verify all moves are "right" (moving toward food)
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.turn, i as i32, "Turn number should match index");
        if i < 3 {
            assert_eq!(
                entry.chosen_move, "right",
                "Moves 0-2 should be right (toward food)"
            );
        } else {
            assert_eq!(
                entry.chosen_move, "right",
                "Move 3 should be right (after eating food)"
            );
        }
    }
}

#[test]
fn test_load_log_file_collision_avoidance() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("collision_avoidance.jsonl"))
        .expect("Failed to load collision_avoidance.jsonl");

    assert_eq!(entries.len(), 2, "Expected 2 log entries");
    assert_eq!(
        entries[0].board.snakes.len(),
        2,
        "Should have 2 snakes in collision scenario"
    );
}

#[test]
fn test_replay_all_basic() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("survival_basic.jsonl"))
        .expect("Failed to load survival_basic.jsonl");

    let results = engine
        .replay_all(&entries)
        .expect("Failed to replay all turns");

    assert_eq!(
        results.len(),
        2,
        "Should have replayed all 2 turns successfully"
    );

    // Verify each result has valid data
    for result in &results {
        assert!(
            result.computation_time_ms > 0,
            "Computation time should be positive"
        );
        assert!(
            result.search_depth > 0,
            "Search depth should be positive"
        );
    }
}

#[test]
fn test_replay_specific_turns() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("food_acquisition.jsonl"))
        .expect("Failed to load food_acquisition.jsonl");

    // Replay only turns 0 and 2
    let turn_numbers = vec![0, 2];
    let results = engine
        .replay_turns(&entries, &turn_numbers)
        .expect("Failed to replay specific turns");

    assert_eq!(results.len(), 2, "Should have replayed 2 specific turns");
    assert_eq!(results[0].turn, 0, "First result should be turn 0");
    assert_eq!(results[1].turn, 2, "Second result should be turn 2");
}

#[test]
fn test_generate_stats() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("survival_basic.jsonl"))
        .expect("Failed to load survival_basic.jsonl");

    let results = engine
        .replay_all(&entries)
        .expect("Failed to replay all turns");

    let stats = engine.generate_stats(&results);

    assert_eq!(stats.total_turns, 2, "Should have 2 total turns");
    assert_eq!(
        stats.matches + stats.mismatches,
        stats.total_turns,
        "Matches + mismatches should equal total turns"
    );
    assert!(
        stats.match_rate >= 0.0 && stats.match_rate <= 100.0,
        "Match rate should be between 0 and 100"
    );
}

#[test]
fn test_validate_expected_moves_success() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("survival_basic.jsonl"))
        .expect("Failed to load survival_basic.jsonl");

    // Validate that turn 0 has move "up"
    let expected_moves = vec![(0, vec![Direction::Up])];

    let result = engine.validate_expected_moves(&entries, &expected_moves);
    assert!(
        result.is_ok(),
        "Validation should succeed for correct expected move"
    );
}

#[test]
fn test_validate_expected_moves_multiple_acceptable() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("survival_basic.jsonl"))
        .expect("Failed to load survival_basic.jsonl");

    // Allow multiple acceptable moves for turn 1
    let expected_moves = vec![(1, vec![Direction::Left, Direction::Right])];

    let result = engine.validate_expected_moves(&entries, &expected_moves);
    assert!(
        result.is_ok(),
        "Validation should succeed when move matches one of multiple acceptable moves"
    );
}

#[test]
fn test_validate_expected_moves_failure() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("survival_basic.jsonl"))
        .expect("Failed to load survival_basic.jsonl");

    // Expect wrong move for turn 0
    let expected_moves = vec![(0, vec![Direction::Down])];

    let result = engine.validate_expected_moves(&entries, &expected_moves);
    assert!(
        result.is_err(),
        "Validation should fail for incorrect expected move"
    );
}

#[test]
fn test_board_state_consistency() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("food_acquisition.jsonl"))
        .expect("Failed to load food_acquisition.jsonl");

    // Verify board dimensions are consistent
    for entry in &entries {
        assert_eq!(entry.board.height, 7, "Board height should be 7");
        assert_eq!(entry.board.width, 7, "Board width should be 7");
    }

    // Verify snake health decreases over time (unless food is eaten)
    for i in 0..entries.len() - 1 {
        let current_health = entries[i].board.snakes[0].health;
        let next_health = entries[i + 1].board.snakes[0].health;

        // Health should decrease by 1 or reset to 100 (food eaten)
        assert!(
            next_health == current_health - 1 || next_health == 100,
            "Health should decrease by 1 or reset to 100 after eating food"
        );
    }
}

#[test]
fn test_snake_length_increases_after_food() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let entries = engine
        .load_log_file(fixture_path("food_acquisition.jsonl"))
        .expect("Failed to load food_acquisition.jsonl");

    // Snake should start at length 3
    assert_eq!(
        entries[0].board.snakes[0].body.len(),
        3,
        "Snake should start at length 3"
    );

    // After eating food (turn 3), snake should be length 4
    assert_eq!(
        entries[3].board.snakes[0].body.len(),
        4,
        "Snake should be length 4 after eating food"
    );

    // Health should be restored to 100
    assert_eq!(
        entries[3].board.snakes[0].health, 100,
        "Health should be 100 after eating food"
    );
}

#[test]
fn test_load_nonexistent_file() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let result = engine.load_log_file(fixture_path("nonexistent.jsonl"));
    assert!(
        result.is_err(),
        "Loading nonexistent file should return error"
    );
}

#[test]
fn test_replay_empty_entries() {
    let config = Config::default_hardcoded();
    let engine = ReplayEngine::new(config, false);

    let results = engine.replay_all(&[]).expect("Should handle empty entries");
    assert_eq!(results.len(), 0, "Replaying empty entries should return empty results");
}
