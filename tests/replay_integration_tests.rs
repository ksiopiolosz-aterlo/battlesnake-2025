// Integration tests for replay binary
//
// Tests the replay binary CLI behavior:
// - Command-line argument parsing
// - File loading and error handling
// - Different replay modes (--all, --turns, --validate)
// - Output formatting

use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

static INIT: Once = Once::new();

/// Ensures the replay binary is built before running any tests.
/// This is called once before the first test runs.
fn ensure_replay_binary_built() {
    INIT.call_once(|| {
        eprintln!("Building replay binary for integration tests...");

        // Detect build profile
        #[cfg(debug_assertions)]
        let profile_args = vec!["build", "--bin", "replay"];
        #[cfg(not(debug_assertions))]
        let profile_args = vec!["build", "--bin", "replay", "--release"];

        let status = Command::new("cargo")
            .args(&profile_args)
            .status()
            .expect("Failed to execute cargo build");

        assert!(
            status.success(),
            "Failed to build replay binary as test dependency"
        );

        eprintln!("Replay binary built successfully.");
    });
}

/// Helper function to get the path to test fixtures
fn fixture_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(filename)
}

/// Helper function to get the path to the replay binary
fn replay_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");

    // Detect build profile: release if running release tests, otherwise debug
    #[cfg(debug_assertions)]
    let profile = "debug";
    #[cfg(not(debug_assertions))]
    let profile = "release";

    path.push(profile);
    path.push("replay");
    path
}

/// Helper to run replay binary with arguments
fn run_replay(args: &[&str]) -> std::process::Output {
    // Ensure the replay binary is built before running
    ensure_replay_binary_built();

    Command::new(replay_binary_path())
        .args(args)
        .output()
        .expect("Failed to execute replay binary")
}

#[test]
fn test_replay_help() {
    let output = run_replay(&["--help"]);
    assert!(output.status.success(), "Help command should succeed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Battlesnake Replay Tool"), "Should show tool name");
    assert!(stderr.contains("USAGE:"), "Should show usage section");
    assert!(stderr.contains("OPTIONS:"), "Should show options section");
    assert!(stderr.contains("EXAMPLES:"), "Should show examples section");
}

#[test]
fn test_replay_no_arguments() {
    let output = run_replay(&[]);
    assert!(!output.status.success(), "Should fail with no arguments");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("USAGE:"), "Should show usage when arguments missing");
}

#[test]
fn test_replay_all_survival_basic() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay all should succeed for survival_basic.jsonl"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
    assert!(stdout.contains("Total Turns:"), "Should show total turns");
    assert!(stdout.contains("Matches:"), "Should show matches");
    assert!(stdout.contains("Mismatches:"), "Should show mismatches");
}

#[test]
fn test_replay_all_food_acquisition() {
    let fixture = fixture_path("food_acquisition.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay all should succeed for food_acquisition.jsonl"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Loaded 4 log entries"), "Should load 4 entries");
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_all_collision_avoidance() {
    let fixture = fixture_path("collision_avoidance.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay all should succeed for collision_avoidance.jsonl"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Loaded 2 log entries"), "Should load 2 entries");
}

#[test]
fn test_replay_specific_turns() {
    let fixture = fixture_path("food_acquisition.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--turns", "0,2"]);

    assert!(
        output.status.success(),
        "Replay specific turns should succeed"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Replaying 2 specific turn(s)"),
        "Should indicate replaying specific turns"
    );
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_single_turn() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--turns", "0"]);

    assert!(
        output.status.success(),
        "Replay single turn should succeed"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Replaying 1 specific turn(s)"),
        "Should indicate replaying 1 turn"
    );
}

#[test]
fn test_replay_verbose_mode() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all", "--verbose"]);

    assert!(
        output.status.success(),
        "Replay with verbose should succeed"
    );

    // Verbose mode outputs to stderr via logging
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report even in verbose mode");
}

#[test]
fn test_validate_expected_moves_success() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--validate", "0:up"]);

    assert!(
        output.status.success(),
        "Validate should succeed for correct expected move"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("All expected moves validated successfully"),
        "Should show success message"
    );
}

#[test]
fn test_validate_expected_moves_failure() {
    let fixture = fixture_path("survival_basic.jsonl");
    // Turn 0 is "up", so expecting "down" should fail
    let output = run_replay(&[fixture.to_str().unwrap(), "--validate", "0:down"]);

    assert!(
        !output.status.success(),
        "Validate should fail for incorrect expected move"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Validation failed"),
        "Should show validation failure message"
    );
}

#[test]
fn test_validate_multiple_moves() {
    let fixture = fixture_path("food_acquisition.jsonl");
    let output = run_replay(&[
        fixture.to_str().unwrap(),
        "--validate",
        "0:right,1:right,2:right",
    ]);

    assert!(
        output.status.success(),
        "Validate should succeed for multiple correct moves"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Validating 3 expected move(s)"),
        "Should indicate validating 3 moves"
    );
}

#[test]
fn test_validate_with_alternatives() {
    let fixture = fixture_path("survival_basic.jsonl");
    // Allow either "up" or "right" for turn 0 (actual is "up")
    let output = run_replay(&[fixture.to_str().unwrap(), "--validate", "0:up|right"]);

    assert!(
        output.status.success(),
        "Validate should succeed when move matches one of alternatives"
    );
}

#[test]
fn test_nonexistent_log_file() {
    let output = run_replay(&["nonexistent.jsonl", "--all"]);

    assert!(
        !output.status.success(),
        "Should fail for nonexistent log file"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error loading log file"),
        "Should show error message for missing file"
    );
}

#[test]
fn test_missing_mode_argument() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap()]);

    assert!(
        !output.status.success(),
        "Should fail when no mode (--all, --turns, --validate) specified"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Must specify"),
        "Should show error about missing mode"
    );
}

#[test]
fn test_invalid_turn_number() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--turns", "invalid"]);

    assert!(
        !output.status.success(),
        "Should fail for invalid turn number"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error parsing turns") || stderr.contains("Invalid turn number"),
        "Should show error about invalid turn number"
    );
}

#[test]
fn test_invalid_direction() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--validate", "0:invalid"]);

    assert!(
        !output.status.success(),
        "Should fail for invalid direction"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid direction") || stderr.contains("Error parsing"),
        "Should show error about invalid direction"
    );
}

#[test]
fn test_custom_config_path() {
    let fixture = fixture_path("survival_basic.jsonl");
    // Test with default Snake.toml (should exist or fall back to defaults)
    let output = run_replay(&[
        fixture.to_str().unwrap(),
        "--all",
        "--config",
        "Snake.toml",
    ]);

    // Should succeed even if file doesn't exist (falls back to defaults)
    assert!(
        output.status.success(),
        "Should handle custom config path gracefully"
    );
}

#[test]
fn test_turn_not_found() {
    let fixture = fixture_path("survival_basic.jsonl");
    // survival_basic.jsonl only has turns 0 and 1
    let output = run_replay(&[fixture.to_str().unwrap(), "--turns", "999"]);

    assert!(
        !output.status.success(),
        "Should fail when turn not found in log"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Turn") && stderr.contains("not found"),
        "Should show error about turn not found"
    );
}

#[test]
fn test_output_contains_statistics() {
    let fixture = fixture_path("food_acquisition.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(output.status.success(), "Replay should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Average Search Depth:"),
        "Should show average search depth"
    );
    assert!(
        stdout.contains("Average Computation Time:"),
        "Should show average computation time"
    );
}

#[test]
fn test_output_shows_mismatches() {
    let fixture = fixture_path("survival_basic.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(output.status.success(), "Replay should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // If there are mismatches, should show detailed section
    if stdout.contains("Mismatches:     0") {
        assert!(
            !stdout.contains("DETAILED MISMATCHES"),
            "Should not show mismatch details when there are none"
        );
    } else {
        // If mismatches exist, detailed section should appear
        assert!(
            stdout.contains("DETAILED MISMATCHES") || stdout.contains("Mismatches:     0"),
            "Should either show no mismatches or detailed mismatch section"
        );
    }
}
