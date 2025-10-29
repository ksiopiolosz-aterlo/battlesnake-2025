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
    pub move_ordering: MoveOrderingConfig,
    pub aspiration_windows: AspirationWindowsConfig,
    pub move_generation: MoveGenerationConfig,
    pub player_indices: PlayerIndicesConfig,
    pub direction_encoding: DirectionEncodingConfig,
    pub game_rules: GameRulesConfig,
    pub debug: DebugConfig,
    pub profiling: ProfilingConfig,
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
    pub certain_win_threshold: i32,
    pub certain_loss_threshold: i32,
    pub no_improvement_tolerance: u8,
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
    pub model_weight: f64,
    pub one_vs_one: GameModeTimeEstimation,
    pub multiplayer: GameModeTimeEstimation,
}

/// Time estimation parameters for a specific game mode
#[derive(Debug, Deserialize, Clone)]
pub struct GameModeTimeEstimation {
    pub base_iteration_time_ms: f64,
    pub branching_factor: f64,
}

impl TimeEstimationConfig {
    /// Gets the appropriate time estimation parameters based on number of alive snakes
    ///
    /// # Arguments
    /// * `num_alive_snakes` - Number of snakes still alive in the game
    ///
    /// # Returns
    /// Reference to the appropriate GameModeTimeEstimation
    pub fn for_snake_count(&self, num_alive_snakes: usize) -> &GameModeTimeEstimation {
        if num_alive_snakes == 2 {
            &self.one_vs_one
        } else {
            &self.multiplayer
        }
    }
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
    // Temporal discounting
    pub temporal_discount_factor: f32,

    // V8: Hierarchical evaluation
    pub survival_max_multiplier: f32,
    pub survival_health_threshold: u8,

    // V8: Growth strategy
    pub growth_urgency_per_length: i32,
    pub growth_bonus_when_ahead: i32,

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
    pub health_threat_distance: i32,
    pub immediate_food_bonus: i32,
    pub immediate_food_distance: i32,

    // Space control constants
    pub space_safety_margin: usize,
    pub space_shortage_penalty: i32,

    // Length-aware health/food constants
    pub health_urgency_base_length: f32,
    pub health_urgency_length_multiplier: f32,
    pub health_urgency_max_multiplier: f32,
    pub health_urgency_min_multiplier: f32,
    pub starvation_buffer_divisor: i32,

    // Entrapment detection constants
    pub entrapment_nearby_threshold: i32,
    pub entrapment_severe_threshold: f32,
    pub entrapment_severe_penalty_multiplier: f32,
    pub entrapment_moderate_threshold: f32,
    pub entrapment_moderate_penalty_multiplier: f32,

    // Adversarial entrapment constants
    pub adversarial_entrapment_distance: i32,
    pub adversarial_body_threat_buffer: i32,  // V10: NEW
    pub adversarial_space_reduction_penalty: i32,
    pub adversarial_space_reduction_threshold: f32,

    // Territory control constants
    pub territory_scale_factor: f32,

    // Attack scoring constants
    pub attack_head_to_head_distance: i32,
    pub attack_head_to_head_bonus: i32,
    pub attack_trap_margin: usize,
    pub attack_trap_bonus: i32,

    // Head-to-head collision avoidance
    pub head_collision_penalty: i32,

    // Wall proximity penalty (mathematical formula)
    pub wall_penalty_base: i32,
    pub safe_distance_from_wall: i32,

    // Center bias
    pub center_bias_multiplier: i32,

    // Corner danger penalty
    pub corner_danger_base: i32,
    pub corner_danger_threshold: i32,

    // Escape route evaluation
    pub escape_route_penalty_base: i32,
    pub escape_route_penalty_health_scale: bool,
    pub escape_route_min: i32,

    // Safe food bonus
    pub safe_food_bonus: i32,
    pub safe_food_center_threshold: i32,

    // Length advantage bonus
    pub length_advantage_bonus: i32,

    // Tail-chasing detection
    pub tail_chasing_detection_distance: i32,
    pub tail_chasing_penalty_per_segment: i32,
    pub tail_chasing_penalty_exponent: f32,
    pub tail_chasing_opponent_distance: i32,

    // Articulation point detection
    pub articulation_point_penalty: i32,
    pub articulation_point_enabled: bool,
}

/// IDAPOS (Locality Masking) constants
#[derive(Debug, Deserialize, Clone)]
pub struct IdaposConfig {
    pub head_distance_multiplier: i32,
    pub max_locality_distance: i32,
    pub min_snakes_for_alpha_beta: usize,
}

/// Move ordering constants
#[derive(Debug, Deserialize, Clone)]
pub struct MoveOrderingConfig {
    pub killer_moves_per_depth: usize,
    pub enable_pv_ordering: bool,
    pub enable_killer_heuristic: bool,
}

/// Aspiration windows constants for 1v1 alpha-beta search
#[derive(Debug, Deserialize, Clone)]
pub struct AspirationWindowsConfig {
    pub enabled: bool,
    pub initial_window_size: i32,
    pub window_expansion_multiplier: i32,
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

/// Debug configuration
#[derive(Debug, Deserialize, Clone)]
pub struct DebugConfig {
    pub enabled: bool,
    pub log_file_path: String,
}

/// Performance profiling configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ProfilingConfig {
    pub enabled: bool,
    pub log_to_stderr: bool,
    pub track_move_generation: bool,
    pub track_evaluation: bool,
    pub track_search: bool,
    pub track_transposition_table: bool,
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
                certain_win_threshold: 1000000,
                certain_loss_threshold: -1000000,
                no_improvement_tolerance: 2,
            },
            time_estimation: TimeEstimationConfig {
                model_weight: 0.1,  // Reduced from 0.4 - favor empirical observations
                one_vs_one: GameModeTimeEstimation {
                    base_iteration_time_ms: 0.01,
                    branching_factor: 2.2,  // Initial model (will adapt via AdaptiveTimeEstimator)
                },
                multiplayer: GameModeTimeEstimation {
                    base_iteration_time_ms: 0.01,
                    branching_factor: 1.2,  // Reduced from 1.6 for deeper search
                },
            },
            strategy: StrategyConfig {
                min_snakes_for_1v1: 2,
                min_cpus_for_parallel: 2,
            },
            scores: ScoresConfig {
                temporal_discount_factor: 0.95,
                survival_max_multiplier: 200.0,  // V11: Reduced from 1000.0
                survival_health_threshold: 20,
                growth_urgency_per_length: 500,
                growth_bonus_when_ahead: 100,
                score_dead_snake: i32::MIN + 1000,
                score_survival_penalty: -1_000_000,
                score_survival_weight: 1000.0,
                weight_space: 15.0,  // V11: Reduced from 25.0 for balanced play
                weight_health: 40.0,  // V11: Reduced from 75.0 to match lower food bonuses
                weight_control: 5.0,  // V11: Increased from 3.0 for strategic positioning
                weight_attack: 8.0,  // V11: Reduced from 10.0 for selective aggression
                weight_length: 100,
                score_zero_health: -100_000,
                default_food_distance: 999,
                health_max: 100.0,
                score_starvation_base: -50_000,
                health_threat_distance: 3,
                immediate_food_bonus: 100000,  // V11.2: Increased from 75000 (eliminate last cycle)
                immediate_food_distance: 2,
                space_safety_margin: 5,
                space_shortage_penalty: 100,
                // Length-aware health constants
                health_urgency_base_length: 3.0,
                health_urgency_length_multiplier: 0.1,
                health_urgency_max_multiplier: 2.0,
                health_urgency_min_multiplier: 1.0,
                starvation_buffer_divisor: 3,
                // Entrapment detection constants
                entrapment_nearby_threshold: 5,
                entrapment_severe_threshold: 0.3,
                entrapment_severe_penalty_multiplier: 0.5,
                entrapment_moderate_threshold: 0.5,
                entrapment_moderate_penalty_multiplier: 0.2,
                // Adversarial entrapment constants
                adversarial_entrapment_distance: 4,  // V10: Increased from 3
                adversarial_body_threat_buffer: 2,  // V10: NEW
                adversarial_space_reduction_penalty: 10000,
                adversarial_space_reduction_threshold: 0.2,
                territory_scale_factor: 100.0,
                attack_head_to_head_distance: 3,
                attack_head_to_head_bonus: 200,  // Increased from 50 for aggressive kills
                attack_trap_margin: 3,
                attack_trap_bonus: 300,  // Increased from 100 to reward trapping
                head_collision_penalty: -50_000,
                wall_penalty_base: 500,  // Reduced from 1000 to allow edge food acquisition
                safe_distance_from_wall: 3,
                center_bias_multiplier: 50,  // Increased from 10 to prevent wall-hugging
                corner_danger_base: 5000,
                corner_danger_threshold: 3,
                escape_route_penalty_base: -1500,  // V6: Reduced from -3000 to allow safe food acquisition
                escape_route_penalty_health_scale: true,
                escape_route_min: 2,
                safe_food_bonus: 2000,  // V6: Bonus for food in safe central area
                safe_food_center_threshold: 3,
                length_advantage_bonus: 200,
                tail_chasing_detection_distance: 2,
                tail_chasing_penalty_per_segment: 300,
                tail_chasing_penalty_exponent: 2.0,
                tail_chasing_opponent_distance: 6,
                articulation_point_penalty: -2000,
                articulation_point_enabled: true,
            },
            idapos: IdaposConfig {
                head_distance_multiplier: 1,  // Aggressive locality masking: only snakes within depth distance
                max_locality_distance: 5,     // Cap to prevent over-inclusion at high depths
                min_snakes_for_alpha_beta: 2,
            },
            move_ordering: MoveOrderingConfig {
                killer_moves_per_depth: 2,
                enable_pv_ordering: true,
                enable_killer_heuristic: true,
            },
            aspiration_windows: AspirationWindowsConfig {
                enabled: true,
                initial_window_size: 50,
                window_expansion_multiplier: 3,
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
            debug: DebugConfig {
                enabled: false,
                log_file_path: "battlesnake_debug.jsonl".to_string(),
            },
            profiling: ProfilingConfig {
                enabled: false,
                log_to_stderr: true,
                track_move_generation: true,
                track_evaluation: true,
                track_search: true,
                track_transposition_table: true,
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
        assert_eq!(config.scores.weight_space, 20.0);  // Updated to match Snake.toml
    }

    #[test]
    fn test_snake_toml_can_be_parsed() {
        // This test ensures Snake.toml is valid and can be parsed
        let result = Config::from_file("Snake.toml");
        assert!(
            result.is_ok(),
            "Failed to parse Snake.toml: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_snake_toml_contains_all_required_fields() {
        let config = Config::from_file("Snake.toml")
            .expect("Snake.toml should be parseable");

        // Test timing config
        assert!(config.timing.response_time_budget_ms > 0);
        assert!(config.timing.network_overhead_ms > 0);
        assert!(config.timing.polling_interval_ms > 0);
        assert!(config.timing.initial_depth > 0);
        assert!(config.timing.min_time_remaining_ms > 0);
        assert!(config.timing.max_search_depth > 0);

        // Test time estimation config
        assert!(config.time_estimation.one_vs_one.base_iteration_time_ms > 0.0);
        assert!(config.time_estimation.one_vs_one.branching_factor > 0.0);
        assert!(config.time_estimation.multiplayer.base_iteration_time_ms > 0.0);
        assert!(config.time_estimation.multiplayer.branching_factor > 0.0);

        // Test strategy config
        assert!(config.strategy.min_snakes_for_1v1 > 0);
        assert!(config.strategy.min_cpus_for_parallel > 0);

        // Test scores config (including health_threat_distance)
        assert!(config.scores.health_threat_distance > 0);
        assert!(config.scores.score_dead_snake < 0);
        assert!(config.scores.score_survival_penalty < 0);
        assert!(config.scores.score_survival_weight > 0.0);
        assert!(config.scores.weight_space > 0.0);
        assert!(config.scores.weight_health > 0.0);
        assert!(config.scores.weight_control > 0.0);
        assert!(config.scores.weight_attack > 0.0);
        assert!(config.scores.weight_length > 0);

        // Test debug config
        assert!(!config.debug.log_file_path.is_empty());
    }

    #[test]
    fn test_health_threat_distance_matches_hardcoded_default() {
        let file_config = Config::from_file("Snake.toml")
            .expect("Snake.toml should be parseable");
        let hardcoded_config = Config::default_hardcoded();

        assert_eq!(
            file_config.scores.health_threat_distance,
            hardcoded_config.scores.health_threat_distance,
            "health_threat_distance in Snake.toml should match hardcoded default"
        );
    }

    #[test]
    fn test_all_config_values_match_hardcoded_defaults() {
        let file_config = Config::from_file("Snake.toml")
            .expect("Snake.toml should be parseable");
        let hardcoded_config = Config::default_hardcoded();

        // Timing
        assert_eq!(
            file_config.timing.response_time_budget_ms,
            hardcoded_config.timing.response_time_budget_ms
        );
        assert_eq!(
            file_config.timing.network_overhead_ms,
            hardcoded_config.timing.network_overhead_ms
        );
        assert_eq!(
            file_config.timing.initial_depth,
            hardcoded_config.timing.initial_depth
        );

        // Scores
        assert_eq!(
            file_config.scores.weight_space,
            hardcoded_config.scores.weight_space
        );
        assert_eq!(
            file_config.scores.weight_health,
            hardcoded_config.scores.weight_health
        );
        assert_eq!(
            file_config.scores.health_threat_distance,
            hardcoded_config.scores.health_threat_distance
        );
        assert_eq!(
            file_config.scores.head_collision_penalty,
            hardcoded_config.scores.head_collision_penalty
        );

        // Strategy
        assert_eq!(
            file_config.strategy.min_snakes_for_1v1,
            hardcoded_config.strategy.min_snakes_for_1v1
        );
        assert_eq!(
            file_config.strategy.min_cpus_for_parallel,
            hardcoded_config.strategy.min_cpus_for_parallel
        );

        // IDAPOS
        assert_eq!(
            file_config.idapos.head_distance_multiplier,
            hardcoded_config.idapos.head_distance_multiplier
        );
        assert_eq!(
            file_config.idapos.min_snakes_for_alpha_beta,
            hardcoded_config.idapos.min_snakes_for_alpha_beta
        );

        // Game Rules
        assert_eq!(
            file_config.game_rules.health_on_food,
            hardcoded_config.game_rules.health_on_food
        );
        assert_eq!(
            file_config.game_rules.health_loss_per_turn,
            hardcoded_config.game_rules.health_loss_per_turn
        );
        assert_eq!(
            file_config.game_rules.terminal_state_threshold,
            hardcoded_config.game_rules.terminal_state_threshold
        );
    }

    #[test]
    fn test_load_or_default_works() {
        // This should succeed with the actual file
        let config = Config::load_or_default();
        assert_eq!(config.scores.health_threat_distance, 3);
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        // Test with a non-existent file
        let result = Config::from_file("nonexistent.toml");
        assert!(result.is_err());
    }
}
