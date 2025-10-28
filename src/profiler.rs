//! Performance profiling module for tracking where computation time is spent
//!
//! This module provides fine-grained timing instrumentation to identify performance bottlenecks.
//! Uses thread-local storage to minimize contention in parallel search.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::config::ProfilingConfig;

/// Thread-local profiling data
#[derive(Debug, Default, Clone)]
struct ThreadLocalProfile {
    // Move generation timing
    move_generation_time_ns: u64,
    move_generation_calls: usize,

    // Evaluation timing (breakdown by component)
    eval_total_time_ns: u64,
    eval_calls: usize,
    eval_flood_fill_time_ns: u64,
    eval_flood_fill_calls: usize,
    eval_health_time_ns: u64,
    eval_territory_time_ns: u64,
    eval_attack_time_ns: u64,
    eval_wall_penalty_time_ns: u64,

    // Search timing
    alpha_beta_time_ns: u64,
    alpha_beta_calls: usize,
    alpha_beta_cutoffs: usize,
    maxn_time_ns: u64,
    maxn_calls: usize,
    nodes_evaluated: usize,

    // Transposition table
    tt_lookups: usize,
    tt_hits: usize,
    tt_stores: usize,
}

/// Global profiling aggregator (shared across threads)
#[derive(Debug)]
pub struct ProfilerAggregator {
    config: ProfilingConfig,

    // Move generation
    move_generation_time_ns: AtomicU64,
    move_generation_calls: AtomicUsize,

    // Evaluation
    eval_total_time_ns: AtomicU64,
    eval_calls: AtomicUsize,
    eval_flood_fill_time_ns: AtomicU64,
    eval_flood_fill_calls: AtomicUsize,
    eval_health_time_ns: AtomicU64,
    eval_territory_time_ns: AtomicU64,
    eval_attack_time_ns: AtomicU64,
    eval_wall_penalty_time_ns: AtomicU64,

    // Search
    alpha_beta_time_ns: AtomicU64,
    alpha_beta_calls: AtomicUsize,
    alpha_beta_cutoffs: AtomicUsize,
    maxn_time_ns: AtomicU64,
    maxn_calls: AtomicUsize,
    nodes_evaluated: AtomicUsize,

    // Transposition table
    tt_lookups: AtomicUsize,
    tt_hits: AtomicUsize,
    tt_stores: AtomicUsize,
}

impl ProfilerAggregator {
    pub fn new(config: ProfilingConfig) -> Self {
        ProfilerAggregator {
            config,
            move_generation_time_ns: AtomicU64::new(0),
            move_generation_calls: AtomicUsize::new(0),
            eval_total_time_ns: AtomicU64::new(0),
            eval_calls: AtomicUsize::new(0),
            eval_flood_fill_time_ns: AtomicU64::new(0),
            eval_flood_fill_calls: AtomicUsize::new(0),
            eval_health_time_ns: AtomicU64::new(0),
            eval_territory_time_ns: AtomicU64::new(0),
            eval_attack_time_ns: AtomicU64::new(0),
            eval_wall_penalty_time_ns: AtomicU64::new(0),
            alpha_beta_time_ns: AtomicU64::new(0),
            alpha_beta_calls: AtomicUsize::new(0),
            alpha_beta_cutoffs: AtomicUsize::new(0),
            maxn_time_ns: AtomicU64::new(0),
            maxn_calls: AtomicUsize::new(0),
            nodes_evaluated: AtomicUsize::new(0),
            tt_lookups: AtomicUsize::new(0),
            tt_hits: AtomicUsize::new(0),
            tt_stores: AtomicUsize::new(0),
        }
    }

    /// Merges thread-local profile data into global aggregator
    pub fn merge(&self, local: &ThreadLocalProfile) {
        if self.config.track_move_generation {
            self.move_generation_time_ns.fetch_add(local.move_generation_time_ns, Ordering::Relaxed);
            self.move_generation_calls.fetch_add(local.move_generation_calls, Ordering::Relaxed);
        }

        if self.config.track_evaluation {
            self.eval_total_time_ns.fetch_add(local.eval_total_time_ns, Ordering::Relaxed);
            self.eval_calls.fetch_add(local.eval_calls, Ordering::Relaxed);
            self.eval_flood_fill_time_ns.fetch_add(local.eval_flood_fill_time_ns, Ordering::Relaxed);
            self.eval_flood_fill_calls.fetch_add(local.eval_flood_fill_calls, Ordering::Relaxed);
            self.eval_health_time_ns.fetch_add(local.eval_health_time_ns, Ordering::Relaxed);
            self.eval_territory_time_ns.fetch_add(local.eval_territory_time_ns, Ordering::Relaxed);
            self.eval_attack_time_ns.fetch_add(local.eval_attack_time_ns, Ordering::Relaxed);
            self.eval_wall_penalty_time_ns.fetch_add(local.eval_wall_penalty_time_ns, Ordering::Relaxed);
        }

        if self.config.track_search {
            self.alpha_beta_time_ns.fetch_add(local.alpha_beta_time_ns, Ordering::Relaxed);
            self.alpha_beta_calls.fetch_add(local.alpha_beta_calls, Ordering::Relaxed);
            self.alpha_beta_cutoffs.fetch_add(local.alpha_beta_cutoffs, Ordering::Relaxed);
            self.maxn_time_ns.fetch_add(local.maxn_time_ns, Ordering::Relaxed);
            self.maxn_calls.fetch_add(local.maxn_calls, Ordering::Relaxed);
            self.nodes_evaluated.fetch_add(local.nodes_evaluated, Ordering::Relaxed);
        }

        if self.config.track_transposition_table {
            self.tt_lookups.fetch_add(local.tt_lookups, Ordering::Relaxed);
            self.tt_hits.fetch_add(local.tt_hits, Ordering::Relaxed);
            self.tt_stores.fetch_add(local.tt_stores, Ordering::Relaxed);
        }
    }

    /// Prints profiling report to stderr
    pub fn print_report(&self, total_time_ms: u64) {
        if !self.config.enabled || !self.config.log_to_stderr {
            return;
        }

        let total_time_ns = total_time_ms * 1_000_000;

        eprintln!("\n═══════════════════════════════════════════════════════════");
        eprintln!("                 PERFORMANCE PROFILE");
        eprintln!("═══════════════════════════════════════════════════════════");
        eprintln!("Total Time: {}ms\n", total_time_ms);

        if self.config.track_move_generation {
            let mg_time_ns = self.move_generation_time_ns.load(Ordering::Relaxed);
            let mg_calls = self.move_generation_calls.load(Ordering::Relaxed);
            let mg_time_ms = mg_time_ns as f64 / 1_000_000.0;
            let mg_pct = if total_time_ns > 0 {
                100.0 * mg_time_ns as f64 / total_time_ns as f64
            } else {
                0.0
            };
            let avg_us = if mg_calls > 0 {
                mg_time_ns as f64 / (mg_calls * 1000) as f64
            } else {
                0.0
            };

            eprintln!("Move Generation:");
            eprintln!("  Time:     {:.2}ms ({:.1}%)", mg_time_ms, mg_pct);
            eprintln!("  Calls:    {}", mg_calls);
            eprintln!("  Avg:      {:.2}µs/call", avg_us);
            eprintln!();
        }

        if self.config.track_evaluation {
            let eval_time_ns = self.eval_total_time_ns.load(Ordering::Relaxed);
            let eval_calls = self.eval_calls.load(Ordering::Relaxed);
            let eval_time_ms = eval_time_ns as f64 / 1_000_000.0;
            let eval_pct = if total_time_ns > 0 {
                100.0 * eval_time_ns as f64 / total_time_ns as f64
            } else {
                0.0
            };
            let avg_us = if eval_calls > 0 {
                eval_time_ns as f64 / (eval_calls * 1000) as f64
            } else {
                0.0
            };

            eprintln!("Evaluation:");
            eprintln!("  Total Time: {:.2}ms ({:.1}%)", eval_time_ms, eval_pct);
            eprintln!("  Calls:      {}", eval_calls);
            eprintln!("  Avg:        {:.2}µs/call", avg_us);

            // Breakdown
            let ff_time_ns = self.eval_flood_fill_time_ns.load(Ordering::Relaxed);
            let ff_calls = self.eval_flood_fill_calls.load(Ordering::Relaxed);
            let ff_time_ms = ff_time_ns as f64 / 1_000_000.0;
            let ff_pct = if eval_time_ns > 0 {
                100.0 * ff_time_ns as f64 / eval_time_ns as f64
            } else {
                0.0
            };
            let ff_avg_us = if ff_calls > 0 {
                ff_time_ns as f64 / (ff_calls * 1000) as f64
            } else {
                0.0
            };

            eprintln!("  Breakdown:");
            eprintln!("    Flood Fill:    {:.2}ms ({:.1}%) - {} calls, {:.2}µs avg",
                ff_time_ms, ff_pct, ff_calls, ff_avg_us);

            let health_time_ms = self.eval_health_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            let health_pct = if eval_time_ns > 0 {
                100.0 * self.eval_health_time_ns.load(Ordering::Relaxed) as f64 / eval_time_ns as f64
            } else {
                0.0
            };
            eprintln!("    Health:        {:.2}ms ({:.1}%)", health_time_ms, health_pct);

            let territory_time_ms = self.eval_territory_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            let territory_pct = if eval_time_ns > 0 {
                100.0 * self.eval_territory_time_ns.load(Ordering::Relaxed) as f64 / eval_time_ns as f64
            } else {
                0.0
            };
            eprintln!("    Territory:     {:.2}ms ({:.1}%)", territory_time_ms, territory_pct);

            let attack_time_ms = self.eval_attack_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            let attack_pct = if eval_time_ns > 0 {
                100.0 * self.eval_attack_time_ns.load(Ordering::Relaxed) as f64 / eval_time_ns as f64
            } else {
                0.0
            };
            eprintln!("    Attack:        {:.2}ms ({:.1}%)", attack_time_ms, attack_pct);

            let wall_time_ms = self.eval_wall_penalty_time_ns.load(Ordering::Relaxed) as f64 / 1_000_000.0;
            let wall_pct = if eval_time_ns > 0 {
                100.0 * self.eval_wall_penalty_time_ns.load(Ordering::Relaxed) as f64 / eval_time_ns as f64
            } else {
                0.0
            };
            eprintln!("    Wall Penalty:  {:.2}ms ({:.1}%)", wall_time_ms, wall_pct);
            eprintln!();
        }

        if self.config.track_search {
            let nodes = self.nodes_evaluated.load(Ordering::Relaxed);
            let ab_time_ns = self.alpha_beta_time_ns.load(Ordering::Relaxed);
            let ab_calls = self.alpha_beta_calls.load(Ordering::Relaxed);
            let ab_cutoffs = self.alpha_beta_cutoffs.load(Ordering::Relaxed);
            let mn_time_ns = self.maxn_time_ns.load(Ordering::Relaxed);
            let mn_calls = self.maxn_calls.load(Ordering::Relaxed);

            let ab_time_ms = ab_time_ns as f64 / 1_000_000.0;
            let ab_pct = if total_time_ns > 0 {
                100.0 * ab_time_ns as f64 / total_time_ns as f64
            } else {
                0.0
            };
            let mn_time_ms = mn_time_ns as f64 / 1_000_000.0;
            let mn_pct = if total_time_ns > 0 {
                100.0 * mn_time_ns as f64 / total_time_ns as f64
            } else {
                0.0
            };
            let cutoff_rate = if ab_calls > 0 {
                100.0 * ab_cutoffs as f64 / ab_calls as f64
            } else {
                0.0
            };

            eprintln!("Search:");
            eprintln!("  Total Nodes:    {}", nodes);
            eprintln!("  Alpha-Beta:");
            eprintln!("    Time:         {:.2}ms ({:.1}%)", ab_time_ms, ab_pct);
            eprintln!("    Calls:        {}", ab_calls);
            eprintln!("    Cutoffs:      {} ({:.1}%)", ab_cutoffs, cutoff_rate);
            eprintln!("  MaxN:");
            eprintln!("    Time:         {:.2}ms ({:.1}%)", mn_time_ms, mn_pct);
            eprintln!("    Calls:        {}", mn_calls);
            eprintln!();
        }

        if self.config.track_transposition_table {
            let lookups = self.tt_lookups.load(Ordering::Relaxed);
            let hits = self.tt_hits.load(Ordering::Relaxed);
            let stores = self.tt_stores.load(Ordering::Relaxed);
            let hit_rate = if lookups > 0 {
                100.0 * hits as f64 / lookups as f64
            } else {
                0.0
            };

            eprintln!("Transposition Table:");
            eprintln!("  Lookups:    {}", lookups);
            eprintln!("  Hits:       {} ({:.1}%)", hits, hit_rate);
            eprintln!("  Stores:     {}", stores);
            eprintln!();
        }

        eprintln!("═══════════════════════════════════════════════════════════\n");
    }
}

/// Thread-local profiler instance
pub struct Profiler {
    config: ProfilingConfig,
    local: RefCell<ThreadLocalProfile>,
}

impl Profiler {
    pub fn new(config: ProfilingConfig) -> Self {
        Profiler {
            config,
            local: RefCell::new(ThreadLocalProfile::default()),
        }
    }

    /// Tracks time spent in move generation
    pub fn track_move_generation<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_move_generation {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.move_generation_time_ns += elapsed;
        local.move_generation_calls += 1;

        result
    }

    /// Tracks time spent in evaluation function
    pub fn track_evaluation<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_evaluation {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.eval_total_time_ns += elapsed;
        local.eval_calls += 1;

        result
    }

    /// Tracks time spent in flood fill (space control)
    pub fn track_flood_fill<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_evaluation {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.eval_flood_fill_time_ns += elapsed;
        local.eval_flood_fill_calls += 1;

        result
    }

    /// Tracks time spent in health evaluation
    pub fn track_health<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_evaluation {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.eval_health_time_ns += elapsed;

        result
    }

    /// Tracks time spent in territory control
    pub fn track_territory<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_evaluation {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.eval_territory_time_ns += elapsed;

        result
    }

    /// Tracks time spent in attack evaluation
    pub fn track_attack<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_evaluation {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.eval_attack_time_ns += elapsed;

        result
    }

    /// Tracks time spent in wall penalty calculation
    pub fn track_wall_penalty<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_evaluation {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.eval_wall_penalty_time_ns += elapsed;

        result
    }

    /// Tracks time spent in alpha-beta search
    pub fn track_alpha_beta<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_search {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.alpha_beta_time_ns += elapsed;
        local.alpha_beta_calls += 1;
        local.nodes_evaluated += 1;

        result
    }

    /// Records an alpha-beta cutoff
    pub fn record_alpha_beta_cutoff(&self) {
        if !self.config.enabled || !self.config.track_search {
            return;
        }

        let mut local = self.local.borrow_mut();
        local.alpha_beta_cutoffs += 1;
    }

    /// Tracks time spent in MaxN search
    pub fn track_maxn<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        if !self.config.enabled || !self.config.track_search {
            return f();
        }

        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;

        let mut local = self.local.borrow_mut();
        local.maxn_time_ns += elapsed;
        local.maxn_calls += 1;
        local.nodes_evaluated += 1;

        result
    }

    /// Records a transposition table lookup
    pub fn record_tt_lookup(&self, hit: bool) {
        if !self.config.enabled || !self.config.track_transposition_table {
            return;
        }

        let mut local = self.local.borrow_mut();
        local.tt_lookups += 1;
        if hit {
            local.tt_hits += 1;
        }
    }

    /// Records a transposition table store
    pub fn record_tt_store(&self) {
        if !self.config.enabled || !self.config.track_transposition_table {
            return;
        }

        let mut local = self.local.borrow_mut();
        local.tt_stores += 1;
    }

    /// Merges this thread's profile data into global aggregator
    pub fn merge_into(&self, aggregator: &ProfilerAggregator) {
        let local = self.local.borrow().clone();
        aggregator.merge(&local);
    }
}
