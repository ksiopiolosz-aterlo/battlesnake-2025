// Configuration module for reading Snake.toml
// This module provides OOP-style configuration management for the Battlesnake bot

use serde::Deserialize;
use std::fs;
use std::path::Path;

/// Main configuration structure containing all tunable parameters
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub timing: TimingConfig,
    pub time_estimation: TimeEstimationConfig,
    pub strategy: StrategyConfig,
    pub scores: ScoresConfig,
    pub idapos: IdaposConfig,
    pub move_generation: MoveGenerationConfig,
    pub player_indices: PlayerIndicesConfig,
    pub direction_encoding: DirectionEncodingConfig,
    pub game_rules: GameRulesConfig,
}

/// Timing and performance constants
#[derive(Debug, Deserialize, Clone)]
pub struct TimingConfig {
    pub response_time_budget_ms: u64,
    pub network_overhead_ms: u64,
    pub polling_interval_ms: u64,
    pub initial_depth: u8,
    pub min_time_remaining_ms: u64,
    pub max_search_depth: u8,
}

impl TimingConfig {
    /// Computes the effective computation budget
    pub fn effective_budget_ms(&self) -> u64 {
        self.response_time_budget_ms.saturating_sub(self.network_overhead_ms)
    }
}

/// Time estimation constants for iterative deepening
#[derive(Debug, Deserialize, Clone)]
pub struct TimeEstimationConfig {
    pub base_iteration_time_ms: f64,
    pub branching_factor: f64,
}

/// Strategy selection constants
#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    pub min_snakes_for_1v1: usize,
    pub min_cpus_for_parallel: usize,
}

/// All evaluation and scoring constants
#[derive(Debug, Deserialize, Clone)]
pub struct ScoresConfig {
    // Survival scores
    pub score_dead_snake: i32,
    pub score_survival_penalty: i32,
    pub score_survival_weight: f32,

    // Component weights
    pub weight_space: f32,
    pub weight_health: f32,
    pub weight_control: f32,
    pub weight_attack: f32,
    pub weight_length: i32,

    // Health & food constants
    pub score_zero_health: i32,
    pub default_food_distance: i32,
    pub health_max: f32,
    pub score_starvation_base: i32,

    // Space control constants
    pub space_safety_margin: usize,
    pub space_shortage_penalty: i32,

    // Territory control constants
    pub territory_scale_factor: f32,

    // Attack scoring constants
    pub attack_head_to_head_distance: i32,
    pub attack_head_to_head_bonus: i32,
    pub attack_trap_margin: usize,
    pub attack_trap_bonus: i32,
}

/// IDAPOS (Locality Masking) constants
#[derive(Debug, Deserialize, Clone)]
pub struct IdaposConfig {
    pub head_distance_multiplier: i32,
    pub min_snakes_for_alpha_beta: usize,
}

/// Move generation constants
#[derive(Debug, Deserialize, Clone)]
pub struct MoveGenerationConfig {
    pub snake_min_body_length_for_neck: usize,
    pub body_tail_offset: usize,
}

/// Player index constants
#[derive(Debug, Deserialize, Clone)]
pub struct PlayerIndicesConfig {
    pub our_snake_index: usize,
    pub player_max_index: usize,
    pub player_min_index: usize,
}

/// Direction encoding constants
#[derive(Debug, Deserialize, Clone)]
pub struct DirectionEncodingConfig {
    pub direction_up_index: u8,
    pub direction_down_index: u8,
    pub direction_left_index: u8,
    pub direction_right_index: u8,
}

/// Game rules constants
#[derive(Debug, Deserialize, Clone)]
pub struct GameRulesConfig {
    pub health_on_food: u8,
    pub health_loss_per_turn: u8,
    pub terminal_state_threshold: usize,
}

impl Config {
    /// Loads configuration from a TOML file
    ///
    /// # Arguments
    /// * `path` - Path to the Snake.toml configuration file
    ///
    /// # Returns
    /// * `Result<Config, String>` - Parsed configuration or error message
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Failed to read config file: {}", e))?;

        toml::from_str(&contents)
            .map_err(|e| format!("Failed to parse config file: {}", e))
    }

    /// Loads default configuration from Snake.toml in the project root
    pub fn load_default() -> Result<Self, String> {
        Self::from_file("Snake.toml")
    }

    /// Creates a configuration with hardcoded default values as fallback
    /// This should match the constants defined in Snake.toml
    pub fn default_hardcoded() -> Self {
        Config {
            timing: TimingConfig {
                response_time_budget_ms: 400,
                network_overhead_ms: 50,
                polling_interval_ms: 50,
                initial_depth: 2,
                min_time_remaining_ms: 20,
                max_search_depth: 20,
            },
            time_estimation: TimeEstimationConfig {
                base_iteration_time_ms: 0.01,
                branching_factor: 3.5,
            },
            strategy: StrategyConfig {
                min_snakes_for_1v1: 2,
                min_cpus_for_parallel: 2,
            },
            scores: ScoresConfig {
                score_dead_snake: i32::MIN + 1000,
                score_survival_penalty: -1_000_000,
                score_survival_weight: 1000.0,
                weight_space: 10.0,
                weight_health: 5.0,
                weight_control: 3.0,
                weight_attack: 2.0,
                weight_length: 100,
                score_zero_health: -100_000,
                default_food_distance: 999,
                health_max: 100.0,
                score_starvation_base: -50_000,
                space_safety_margin: 5,
                space_shortage_penalty: 100,
                territory_scale_factor: 100.0,
                attack_head_to_head_distance: 3,
                attack_head_to_head_bonus: 50,
                attack_trap_margin: 3,
                attack_trap_bonus: 100,
            },
            idapos: IdaposConfig {
                head_distance_multiplier: 2,
                min_snakes_for_alpha_beta: 2,
            },
            move_generation: MoveGenerationConfig {
                snake_min_body_length_for_neck: 1,
                body_tail_offset: 1,
            },
            player_indices: PlayerIndicesConfig {
                our_snake_index: 0,
                player_max_index: 0,
                player_min_index: 1,
            },
            direction_encoding: DirectionEncodingConfig {
                direction_up_index: 0,
                direction_down_index: 1,
                direction_left_index: 2,
                direction_right_index: 3,
            },
            game_rules: GameRulesConfig {
                health_on_food: 100,
                health_loss_per_turn: 1,
                terminal_state_threshold: 1,
            },
        }
    }

    /// Attempts to load from file, falls back to hardcoded defaults on error
    pub fn load_or_default() -> Self {
        Self::load_default()
            .unwrap_or_else(|e| {
                eprintln!("Warning: Could not load Snake.toml ({}), using hardcoded defaults", e);
                Self::default_hardcoded()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_budget_calculation() {
        let config = Config::default_hardcoded();
        assert_eq!(config.timing.effective_budget_ms(), 350);
    }

    #[test]
    fn test_config_can_be_created() {
        let config = Config::default_hardcoded();
        assert_eq!(config.timing.initial_depth, 2);
        assert_eq!(config.scores.weight_space, 10.0);
    }
}
