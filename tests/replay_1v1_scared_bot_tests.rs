// Integration tests for 1v1 scared bot replay validation
//
// Tests that the bot's replay system produces consistent results
// when replaying games against the scared bot opponent.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

static INIT: Once = Once::new();

/// Ensures the replay binary is built before running any tests.
fn ensure_replay_binary_built() {
    INIT.call_once(|| {
        eprintln!("Building replay binary for integration tests...");

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
        .join("1v1_scared_bot")
        .join(filename)
}

/// Helper function to get the path to the replay binary
fn replay_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");

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
    ensure_replay_binary_built();

    Command::new(replay_binary_path())
        .args(args)
        .output()
        .expect("Failed to execute replay binary")
}

#[test]
fn test_replay_1v1_scared_bot_game_01() {
    let fixture = fixture_path("game_01.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay should succeed for 1v1_scared_bot game_01"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
    assert!(stdout.contains("Total Turns:"), "Should show total turns");
}

#[test]
fn test_replay_1v1_scared_bot_game_02() {
    let fixture = fixture_path("game_02.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay should succeed for 1v1_scared_bot game_02"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_1v1_scared_bot_game_03() {
    let fixture = fixture_path("game_03.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay should succeed for 1v1_scared_bot game_03"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_1v1_scared_bot_game_04() {
    let fixture = fixture_path("game_04.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay should succeed for 1v1_scared_bot game_04"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_1v1_scared_bot_game_05() {
    let fixture = fixture_path("game_05.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay should succeed for 1v1_scared_bot game_05"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_1v1_scared_bot_game_06() {
    let fixture = fixture_path("game_06.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay should succeed for 1v1_scared_bot game_06"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_1v1_scared_bot_game_07() {
    let fixture = fixture_path("game_07.jsonl");
    let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(
        output.status.success(),
        "Replay should succeed for 1v1_scared_bot game_07"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
}

#[test]
fn test_replay_1v1_scared_bot_all_games_have_statistics() {
    // Test that all games show proper statistics
    for game_num in 1..=7 {
        let fixture = fixture_path(&format!("game_{:02}.jsonl", game_num));
        let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

        assert!(
            output.status.success(),
            "Replay should succeed for game {:02}",
            game_num
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Average Search Depth:"),
            "Game {:02} should show average search depth",
            game_num
        );
        assert!(
            stdout.contains("Average Computation Time:"),
            "Game {:02} should show average computation time",
            game_num
        );
    }
}

#[test]
fn test_replay_1v1_scared_bot_determinism() {
    // Test that replaying the same game twice produces similar results
    // Note: Due to iterative deepening and time-based cutoffs, perfect
    // determinism is not guaranteed, but results should be very similar
    let fixture = fixture_path("game_01.jsonl");

    let output1 = run_replay(&[fixture.to_str().unwrap(), "--all"]);
    let output2 = run_replay(&[fixture.to_str().unwrap(), "--all"]);

    assert!(output1.status.success(), "First replay should succeed");
    assert!(output2.status.success(), "Second replay should succeed");

    let stdout1 = String::from_utf8_lossy(&output1.stdout);
    let stdout2 = String::from_utf8_lossy(&output2.stdout);

    // Extract match count and total turns from both outputs
    let extract_matches = |s: &str| -> Option<usize> {
        s.lines()
            .find(|line| line.contains("Matches:"))
            .and_then(|line| {
                line.split_whitespace()
                    .nth(1)
                    .and_then(|num| num.parse::<usize>().ok())
            })
    };

    let extract_total = |s: &str| -> Option<usize> {
        s.lines()
            .find(|line| line.contains("Total Turns:"))
            .and_then(|line| {
                line.split_whitespace()
                    .nth(2)
                    .and_then(|num| num.parse::<usize>().ok())
            })
    };

    let matches1 = extract_matches(&stdout1);
    let matches2 = extract_matches(&stdout2);
    let total1 = extract_total(&stdout1);
    let total2 = extract_total(&stdout2);

    assert_eq!(
        total1, total2,
        "Total turns should be identical"
    );

    // Allow for slight variation in matches (within 10% due to timing)
    if let (Some(m1), Some(m2), Some(total)) = (matches1, matches2, total1) {
        let diff = (m1 as i32 - m2 as i32).abs();
        let max_diff = (total as f64 * 0.1).ceil() as usize;
        assert!(
            diff <= max_diff as i32,
            "Match counts should be very similar (within 10%): {} vs {} (diff: {}, max allowed: {})",
            m1, m2, diff, max_diff
        );
    }
}

#[test]
fn test_replay_1v1_scared_bot_all_games_are_wins() {
    // Validate that Rusty won all games against scared bot
    // A win means Rusty is the only snake remaining in the final turn
    for game_num in 1..=7 {
        let fixture = fixture_path(&format!("game_{:02}.jsonl", game_num));
        let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

        assert!(
            output.status.success(),
            "Replay should succeed for game {:02}",
            game_num
        );

        // Read the last line of the game file to check final state
        let game_content = std::fs::read_to_string(&fixture)
            .expect(&format!("Failed to read game {:02}", game_num));
        let last_line = game_content
            .lines()
            .last()
            .expect(&format!("Game {:02} has no lines", game_num));

        // Check that Rusty is present in the final state
        assert!(
            last_line.contains("\"name\":\"Rusty\""),
            "Game {:02}: Rusty should be present in final turn",
            game_num
        );

        // Count how many snakes are in the final state (should be 1 for a win)
        let snake_name_count = last_line.matches("\"name\":\"").count();
        assert_eq!(
            snake_name_count, 1,
            "Game {:02}: Only Rusty should remain (WIN). Found {} snakes",
            game_num, snake_name_count
        );
    }
}
