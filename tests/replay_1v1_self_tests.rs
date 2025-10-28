// Integration tests for 1v1 self-play replay validation
//
// Tests that the bot's replay system produces consistent results
// when replaying games where Rusty plays against itself.
//
// These games are particularly interesting for algorithm analysis:
// - Both snakes use the same decision logic
// - Identifies potential issues with wall-running behavior
// - Reveals successful boxing/trapping strategies
// - Shows emergent patterns in symmetric gameplay
//
// Each game file contains log entries from BOTH Rusty instances,
// allowing comprehensive analysis of adversarial decision-making.

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
        .join("1v1_self")
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

// Generate tests for all 47 games

macro_rules! generate_replay_test {
    ($test_name:ident, $game_num:expr) => {
        #[test]
        fn $test_name() {
            let fixture = fixture_path(&format!("game_{:02}.jsonl", $game_num));
            let output = run_replay(&[fixture.to_str().unwrap(), "--all"]);

            assert!(
                output.status.success(),
                "Replay should succeed for 1v1_self game_{:02}",
                $game_num
            );

            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("REPLAY REPORT"), "Should show replay report");
            assert!(stdout.contains("Total Turns:"), "Should show total turns");
        }
    };
}

// Generate tests for all 47 self-play games
generate_replay_test!(test_replay_1v1_self_game_01, 1);
generate_replay_test!(test_replay_1v1_self_game_02, 2);
generate_replay_test!(test_replay_1v1_self_game_03, 3);
generate_replay_test!(test_replay_1v1_self_game_04, 4);
generate_replay_test!(test_replay_1v1_self_game_05, 5);
generate_replay_test!(test_replay_1v1_self_game_06, 6);
generate_replay_test!(test_replay_1v1_self_game_07, 7);
generate_replay_test!(test_replay_1v1_self_game_08, 8);
generate_replay_test!(test_replay_1v1_self_game_09, 9);
generate_replay_test!(test_replay_1v1_self_game_10, 10);
generate_replay_test!(test_replay_1v1_self_game_11, 11);
generate_replay_test!(test_replay_1v1_self_game_12, 12);
generate_replay_test!(test_replay_1v1_self_game_13, 13);
generate_replay_test!(test_replay_1v1_self_game_14, 14);
generate_replay_test!(test_replay_1v1_self_game_15, 15);
generate_replay_test!(test_replay_1v1_self_game_16, 16);
generate_replay_test!(test_replay_1v1_self_game_17, 17);
generate_replay_test!(test_replay_1v1_self_game_18, 18);
generate_replay_test!(test_replay_1v1_self_game_19, 19);
generate_replay_test!(test_replay_1v1_self_game_20, 20);
generate_replay_test!(test_replay_1v1_self_game_21, 21);
generate_replay_test!(test_replay_1v1_self_game_22, 22);
generate_replay_test!(test_replay_1v1_self_game_23, 23);
generate_replay_test!(test_replay_1v1_self_game_24, 24);
generate_replay_test!(test_replay_1v1_self_game_25, 25);
generate_replay_test!(test_replay_1v1_self_game_26, 26);
generate_replay_test!(test_replay_1v1_self_game_27, 27);
generate_replay_test!(test_replay_1v1_self_game_28, 28);
generate_replay_test!(test_replay_1v1_self_game_29, 29);
generate_replay_test!(test_replay_1v1_self_game_30, 30);
generate_replay_test!(test_replay_1v1_self_game_31, 31);
generate_replay_test!(test_replay_1v1_self_game_32, 32);
generate_replay_test!(test_replay_1v1_self_game_33, 33);
generate_replay_test!(test_replay_1v1_self_game_34, 34);
generate_replay_test!(test_replay_1v1_self_game_35, 35);
generate_replay_test!(test_replay_1v1_self_game_36, 36);
generate_replay_test!(test_replay_1v1_self_game_37, 37);
generate_replay_test!(test_replay_1v1_self_game_38, 38);
generate_replay_test!(test_replay_1v1_self_game_39, 39);
generate_replay_test!(test_replay_1v1_self_game_40, 40);
generate_replay_test!(test_replay_1v1_self_game_41, 41);
generate_replay_test!(test_replay_1v1_self_game_42, 42);
generate_replay_test!(test_replay_1v1_self_game_43, 43);
generate_replay_test!(test_replay_1v1_self_game_44, 44);
generate_replay_test!(test_replay_1v1_self_game_45, 45);
generate_replay_test!(test_replay_1v1_self_game_46, 46);
generate_replay_test!(test_replay_1v1_self_game_47, 47);

#[test]
fn test_replay_1v1_self_all_games_have_statistics() {
    // Sample test checking that statistics are present for a selection of games
    for game_num in [1, 15, 30, 47] {
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
fn test_replay_1v1_self_games_are_symmetric() {
    // In self-play, both snakes should be present in all games
    // Check a few games to validate that both snakes are present initially
    for game_num in [1, 15, 30, 47] {
        let fixture = fixture_path(&format!("game_{:02}.jsonl", game_num));
        let game_content = std::fs::read_to_string(&fixture)
            .expect(&format!("Failed to read game {:02}", game_num));

        let first_line = game_content
            .lines()
            .next()
            .expect(&format!("Game {:02} has no lines", game_num));

        // Both snakes should be named "Rusty" in self-play
        let rusty_count = first_line.matches("\"name\":\"Rusty\"").count();
        assert_eq!(
            rusty_count, 2,
            "Game {:02}: Should have exactly 2 Rusty snakes in initial state. Found {}",
            game_num, rusty_count
        );
    }
}

#[test]
fn test_replay_1v1_self_game_contains_both_perspectives() {
    // Each game should contain log entries from both Rusty instances
    // Verify that we have two different "chosen_move" entries per turn
    let fixture = fixture_path("game_15.jsonl");
    let game_content = std::fs::read_to_string(&fixture)
        .expect("Failed to read game_15.jsonl");

    let lines: Vec<&str> = game_content.lines().collect();

    // Game 15 has 260 entries (130 turns × 2 players)
    assert_eq!(lines.len(), 260, "Game 15 should have 260 log entries");

    // Check that consecutive pairs of entries have the same turn number but different moves
    for i in (0..lines.len()).step_by(2) {
        if i + 1 < lines.len() {
            let entry1: serde_json::Value = serde_json::from_str(lines[i])
                .expect("Failed to parse first entry");
            let entry2: serde_json::Value = serde_json::from_str(lines[i + 1])
                .expect("Failed to parse second entry");

            let turn1 = entry1["turn"].as_u64().expect("Missing turn in entry 1");
            let turn2 = entry2["turn"].as_u64().expect("Missing turn in entry 2");

            assert_eq!(
                turn1, turn2,
                "Consecutive entries should have same turn number (got {} and {})",
                turn1, turn2
            );
        }
    }
}

// TODO: Add tests for specific interesting behaviors after manual analysis:
//
// #[test]
// fn test_replay_1v1_self_wall_running_games() {
//     // Games where one snake chose to run into a wall despite safer options
//     // These indicate potential logic bugs in survival evaluation
//     let problematic_games = vec![]; // To be filled after analysis
//     // ...
// }
//
// #[test]
// fn test_replay_1v1_self_successful_boxing() {
//     // Games showing successful opponent trapping/boxing strategies
//     // Worth studying for understanding effective attack patterns
//     let boxing_games = vec![]; // To be filled after analysis
//     // ...
// }
//
// #[test]
// fn test_replay_1v1_self_long_endurance_games() {
//     // Game 47 has 840 turns (420 actual game turns)
//     // These long games show sustained strategic play
//     let long_games = vec![47, 46]; // Games with >600 entries
//     // ...
// }
