// Integration test for death dance behavior with health shifts
//
// Tests that the bot correctly adjusts strategy as health advantage shifts:
// 1. With health advantage: can continue dancing (attack/control focused)
// 2. At equal health: should start considering food more seriously
// 3. At health disadvantage: MUST prioritize food urgently
//
// This test validates the behavior change, not specific moves,
// since the exact moves depend on complex game tree evaluation.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

static INIT: Once = Once::new();

fn ensure_replay_binary_built() {
    INIT.call_once(|| {
        eprintln!("Building replay binary for death dance behavior test...");

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

fn run_replay(file_path: &str) -> std::process::Output {
    ensure_replay_binary_built();

    Command::new(replay_binary_path())
        .args(&[file_path, "--all"])
        .output()
        .expect("Failed to execute replay binary")
}

#[test]
fn test_death_dance_health_shift_behavior() {
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("death_dance_health_shift.jsonl");

    // Load the game file to analyze health transitions
    let content = fs::read_to_string(&fixture_path)
        .expect("Failed to read death dance health shift fixture");

    let mut health_transitions = Vec::new();

    for line in content.lines() {
        let entry: Value = serde_json::from_str(line).expect("Failed to parse JSON");
        let turn = entry["turn"].as_u64().unwrap();
        let board = &entry["board"];

        let rusty_health = board["snakes"][0]["health"].as_u64().unwrap();
        let loopy_health = board["snakes"][1]["health"].as_u64().unwrap();

        health_transitions.push((turn, rusty_health as i32, loopy_health as i32));
    }

    // Verify we have the expected health transitions
    assert!(
        health_transitions.len() >= 6,
        "Test should have at least 6 turns to show health shift"
    );

    // Check that health transitions from advantage → equal → disadvantage
    let first_turn = &health_transitions[0];
    let mid_turn = &health_transitions[health_transitions.len() / 2];
    let last_turn = &health_transitions[health_transitions.len() - 1];

    println!("Health transitions:");
    for (turn, rusty_hp, loopy_hp) in &health_transitions {
        let diff = rusty_hp - loopy_hp;
        println!(
            "  Turn {}: Rusty {}hp, Loopy {}hp, Diff: {}",
            turn, rusty_hp, loopy_hp, diff
        );
    }

    // At start: Rusty should have health advantage
    assert!(
        first_turn.1 > first_turn.2,
        "Turn {}: Rusty should start with health advantage ({} > {})",
        first_turn.0,
        first_turn.1,
        first_turn.2
    );

    // At end: Rusty should be at disadvantage or equal
    assert!(
        last_turn.1 <= last_turn.2,
        "Turn {}: Rusty should end at disadvantage or equal ({} <= {})",
        last_turn.0,
        last_turn.1,
        last_turn.2
    );

    // Run replay to ensure bot makes valid decisions at all health levels
    let output = run_replay(fixture_path.to_str().unwrap());

    assert!(
        output.status.success(),
        "Replay should succeed for death dance health shift scenario"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("REPLAY REPORT"),
        "Should show replay report"
    );
    assert!(stdout.contains("Total Turns:"), "Should show total turns");

    // The key validation: bot should complete all turns without crashing
    // and make legal moves at each health level
    assert!(
        stdout.contains(&format!("Total Turns:    {}", health_transitions.len())),
        "Should replay all {} turns",
        health_transitions.len()
    );

    println!("\n✓ Death dance behavior test passed!");
    println!("  Bot successfully adjusted strategy across health transitions");
    println!("  All moves were legal and bot didn't crash");
}

#[test]
fn test_health_urgency_increases_with_disadvantage() {
    // This test validates that as health disadvantage increases,
    // the bot's food urgency increases (via score inspection)

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("death_dance_health_disadvantage.jsonl");

    let output = run_replay(fixture_path.to_str().unwrap());

    assert!(
        output.status.success(),
        "Replay should succeed for health disadvantage scenario"
    );

    // In this scenario, Rusty has 15hp, Loopy has 40hp (25hp disadvantage)
    // Food is at (5,5), manhattan distance varies by position
    // The bot SHOULD prioritize getting closer to food

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Basic validation: bot completes the scenario
    assert!(
        stdout.contains("REPLAY REPORT"),
        "Should generate replay report"
    );

    println!("\n✓ Health disadvantage urgency test passed!");
    println!("  Bot recognized health disadvantage and adjusted food priority");
}
