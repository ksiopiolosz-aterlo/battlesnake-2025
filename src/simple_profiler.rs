//! Simple profiling macros using thread-local storage and conditional compilation
//!
//! This module provides lightweight profiling without changing function signatures.
//! Enable with environment variable: BATTLESNAKE_PROFILE=1

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

thread_local! {
    static MOVE_GEN_TIME: RefCell<u64> = RefCell::new(0);
    static MOVE_GEN_COUNT: RefCell<usize> = RefCell::new(0);

    static EVAL_TIME: RefCell<u64> = RefCell::new(0);
    static EVAL_COUNT: RefCell<usize> = RefCell::new(0);

    static FLOOD_FILL_TIME: RefCell<u64> = RefCell::new(0);
    static FLOOD_FILL_COUNT: RefCell<usize> = RefCell::new(0);

    static ADVERSARIAL_FLOOD_FILL_TIME: RefCell<u64> = RefCell::new(0);
    static ADVERSARIAL_FLOOD_FILL_COUNT: RefCell<usize> = RefCell::new(0);

    static APPLY_MOVE_TIME: RefCell<u64> = RefCell::new(0);
    static APPLY_MOVE_COUNT: RefCell<usize> = RefCell::new(0);

    static ALPHA_BETA_TIME: RefCell<u64> = RefCell::new(0);
    static ALPHA_BETA_COUNT: RefCell<usize> = RefCell::new(0);
    static ALPHA_BETA_CUTOFFS: RefCell<usize> = RefCell::new(0);

    static MAXN_TIME: RefCell<u64> = RefCell::new(0);
    static MAXN_COUNT: RefCell<usize> = RefCell::new(0);

    static TT_LOOKUPS: RefCell<usize> = RefCell::new(0);
    static TT_HITS: RefCell<usize> = RefCell::new(0);
}

// Global aggregators
static GLOBAL_MOVE_GEN_TIME: AtomicU64 = AtomicU64::new(0);
static GLOBAL_MOVE_GEN_COUNT: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_EVAL_TIME: AtomicU64 = AtomicU64::new(0);
static GLOBAL_EVAL_COUNT: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_FLOOD_FILL_TIME: AtomicU64 = AtomicU64::new(0);
static GLOBAL_FLOOD_FILL_COUNT: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_ADVERSARIAL_FLOOD_FILL_TIME: AtomicU64 = AtomicU64::new(0);
static GLOBAL_ADVERSARIAL_FLOOD_FILL_COUNT: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_APPLY_MOVE_TIME: AtomicU64 = AtomicU64::new(0);
static GLOBAL_APPLY_MOVE_COUNT: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_ALPHA_BETA_TIME: AtomicU64 = AtomicU64::new(0);
static GLOBAL_ALPHA_BETA_COUNT: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_ALPHA_BETA_CUTOFFS: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_MAXN_TIME: AtomicU64 = AtomicU64::new(0);
static GLOBAL_MAXN_COUNT: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_TT_LOOKUPS: AtomicUsize = AtomicUsize::new(0);
static GLOBAL_TT_HITS: AtomicUsize = AtomicUsize::new(0);

#[inline]
pub fn is_profiling_enabled() -> bool {
    std::env::var("BATTLESNAKE_PROFILE").is_ok()
}

pub struct ProfileGuard {
    start: Instant,
    category: &'static str,
}

impl ProfileGuard {
    pub fn new(category: &'static str) -> Option<Self> {
        if is_profiling_enabled() {
            Some(ProfileGuard {
                start: Instant::now(),
                category,
            })
        } else {
            None
        }
    }
}

impl Drop for ProfileGuard {
    fn drop(&mut self) {
        let elapsed_ns = self.start.elapsed().as_nanos() as u64;

        match self.category {
            "move_gen" => {
                MOVE_GEN_TIME.with(|t| *t.borrow_mut() += elapsed_ns);
                MOVE_GEN_COUNT.with(|c| *c.borrow_mut() += 1);
            }
            "eval" => {
                EVAL_TIME.with(|t| *t.borrow_mut() += elapsed_ns);
                EVAL_COUNT.with(|c| *c.borrow_mut() += 1);
            }
            "flood_fill" => {
                FLOOD_FILL_TIME.with(|t| *t.borrow_mut() += elapsed_ns);
                FLOOD_FILL_COUNT.with(|c| *c.borrow_mut() += 1);
            }
            "adversarial_flood_fill" => {
                ADVERSARIAL_FLOOD_FILL_TIME.with(|t| *t.borrow_mut() += elapsed_ns);
                ADVERSARIAL_FLOOD_FILL_COUNT.with(|c| *c.borrow_mut() += 1);
            }
            "apply_move" => {
                APPLY_MOVE_TIME.with(|t| *t.borrow_mut() += elapsed_ns);
                APPLY_MOVE_COUNT.with(|c| *c.borrow_mut() += 1);
            }
            "alpha_beta" => {
                ALPHA_BETA_TIME.with(|t| *t.borrow_mut() += elapsed_ns);
                ALPHA_BETA_COUNT.with(|c| *c.borrow_mut() += 1);
            }
            "maxn" => {
                MAXN_TIME.with(|t| *t.borrow_mut() += elapsed_ns);
                MAXN_COUNT.with(|c| *c.borrow_mut() += 1);
            }
            _ => {}
        }
    }
}

#[inline]
pub fn record_alpha_beta_cutoff() {
    if is_profiling_enabled() {
        ALPHA_BETA_CUTOFFS.with(|c| *c.borrow_mut() += 1);
    }
}

#[inline]
pub fn record_tt_lookup(hit: bool) {
    if is_profiling_enabled() {
        TT_LOOKUPS.with(|c| *c.borrow_mut() += 1);
        if hit {
            TT_HITS.with(|c| *c.borrow_mut() += 1);
        }
    }
}

pub fn merge_thread_local() {
    if !is_profiling_enabled() {
        return;
    }

    MOVE_GEN_TIME.with(|t| {
        GLOBAL_MOVE_GEN_TIME.fetch_add(*t.borrow(), Ordering::Relaxed);
        *t.borrow_mut() = 0;
    });
    MOVE_GEN_COUNT.with(|c| {
        GLOBAL_MOVE_GEN_COUNT.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });

    EVAL_TIME.with(|t| {
        GLOBAL_EVAL_TIME.fetch_add(*t.borrow(), Ordering::Relaxed);
        *t.borrow_mut() = 0;
    });
    EVAL_COUNT.with(|c| {
        GLOBAL_EVAL_COUNT.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });

    FLOOD_FILL_TIME.with(|t| {
        GLOBAL_FLOOD_FILL_TIME.fetch_add(*t.borrow(), Ordering::Relaxed);
        *t.borrow_mut() = 0;
    });
    FLOOD_FILL_COUNT.with(|c| {
        GLOBAL_FLOOD_FILL_COUNT.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });

    ADVERSARIAL_FLOOD_FILL_TIME.with(|t| {
        GLOBAL_ADVERSARIAL_FLOOD_FILL_TIME.fetch_add(*t.borrow(), Ordering::Relaxed);
        *t.borrow_mut() = 0;
    });
    ADVERSARIAL_FLOOD_FILL_COUNT.with(|c| {
        GLOBAL_ADVERSARIAL_FLOOD_FILL_COUNT.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });

    APPLY_MOVE_TIME.with(|t| {
        GLOBAL_APPLY_MOVE_TIME.fetch_add(*t.borrow(), Ordering::Relaxed);
        *t.borrow_mut() = 0;
    });
    APPLY_MOVE_COUNT.with(|c| {
        GLOBAL_APPLY_MOVE_COUNT.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });

    ALPHA_BETA_TIME.with(|t| {
        GLOBAL_ALPHA_BETA_TIME.fetch_add(*t.borrow(), Ordering::Relaxed);
        *t.borrow_mut() = 0;
    });
    ALPHA_BETA_COUNT.with(|c| {
        GLOBAL_ALPHA_BETA_COUNT.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });
    ALPHA_BETA_CUTOFFS.with(|c| {
        GLOBAL_ALPHA_BETA_CUTOFFS.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });

    MAXN_TIME.with(|t| {
        GLOBAL_MAXN_TIME.fetch_add(*t.borrow(), Ordering::Relaxed);
        *t.borrow_mut() = 0;
    });
    MAXN_COUNT.with(|c| {
        GLOBAL_MAXN_COUNT.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });

    TT_LOOKUPS.with(|c| {
        GLOBAL_TT_LOOKUPS.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });
    TT_HITS.with(|c| {
        GLOBAL_TT_HITS.fetch_add(*c.borrow(), Ordering::Relaxed);
        *c.borrow_mut() = 0;
    });
}

pub fn print_report(total_time_ms: u64) {
    if !is_profiling_enabled() {
        return;
    }

    let total_ns = total_time_ms * 1_000_000;

    eprintln!("\n═══════════════════════════════════════════════════════════");
    eprintln!("                 PERFORMANCE PROFILE");
    eprintln!("═══════════════════════════════════════════════════════════");
    eprintln!("Total Time: {}ms\n", total_time_ms);

    let mg_time = GLOBAL_MOVE_GEN_TIME.load(Ordering::Relaxed);
    let mg_count = GLOBAL_MOVE_GEN_COUNT.load(Ordering::Relaxed);
    let mg_ms = mg_time as f64 / 1_000_000.0;
    let mg_pct = if total_ns > 0 { 100.0 * mg_time as f64 / total_ns as f64 } else { 0.0 };
    let mg_avg_us = if mg_count > 0 { mg_time as f64 / (mg_count * 1000) as f64 } else { 0.0 };

    eprintln!("Move Generation:");
    eprintln!("  Time:     {:.2}ms ({:.1}%)", mg_ms, mg_pct);
    eprintln!("  Calls:    {}", mg_count);
    eprintln!("  Avg:      {:.2}µs/call\n", mg_avg_us);

    let eval_time = GLOBAL_EVAL_TIME.load(Ordering::Relaxed);
    let eval_count = GLOBAL_EVAL_COUNT.load(Ordering::Relaxed);
    let eval_ms = eval_time as f64 / 1_000_000.0;
    let eval_pct = if total_ns > 0 { 100.0 * eval_time as f64 / total_ns as f64 } else { 0.0 };
    let eval_avg_us = if eval_count > 0 { eval_time as f64 / (eval_count * 1000) as f64 } else { 0.0 };

    let ff_time = GLOBAL_FLOOD_FILL_TIME.load(Ordering::Relaxed);
    let ff_count = GLOBAL_FLOOD_FILL_COUNT.load(Ordering::Relaxed);
    let ff_ms = ff_time as f64 / 1_000_000.0;
    let ff_pct = if eval_time > 0 { 100.0 * ff_time as f64 / eval_time as f64 } else { 0.0 };
    let ff_avg_us = if ff_count > 0 { ff_time as f64 / (ff_count * 1000) as f64 } else { 0.0 };

    let aff_time = GLOBAL_ADVERSARIAL_FLOOD_FILL_TIME.load(Ordering::Relaxed);
    let aff_count = GLOBAL_ADVERSARIAL_FLOOD_FILL_COUNT.load(Ordering::Relaxed);
    let aff_ms = aff_time as f64 / 1_000_000.0;
    let aff_pct = if eval_time > 0 { 100.0 * aff_time as f64 / eval_time as f64 } else { 0.0 };
    let aff_avg_us = if aff_count > 0 { aff_time as f64 / (aff_count * 1000) as f64 } else { 0.0 };

    eprintln!("Evaluation:");
    eprintln!("  Total Time:            {:.2}ms ({:.1}%)", eval_ms, eval_pct);
    eprintln!("  Calls:                 {}", eval_count);
    eprintln!("  Avg:                   {:.2}µs/call", eval_avg_us);
    eprintln!("  Flood Fill (Space):    {:.2}ms ({:.1}%) - {} calls, {:.2}µs avg",
        ff_ms, ff_pct, ff_count, ff_avg_us);
    eprintln!("  Territory Control:     {:.2}ms ({:.1}%) - {} calls, {:.2}µs avg\n",
        aff_ms, aff_pct, aff_count, aff_avg_us);

    let ab_time = GLOBAL_ALPHA_BETA_TIME.load(Ordering::Relaxed);
    let ab_count = GLOBAL_ALPHA_BETA_COUNT.load(Ordering::Relaxed);
    let ab_cutoffs = GLOBAL_ALPHA_BETA_CUTOFFS.load(Ordering::Relaxed);
    let ab_ms = ab_time as f64 / 1_000_000.0;
    let ab_pct = if total_ns > 0 { 100.0 * ab_time as f64 / total_ns as f64 } else { 0.0 };
    let cutoff_rate = if ab_count > 0 { 100.0 * ab_cutoffs as f64 / ab_count as f64 } else { 0.0 };

    let mn_time = GLOBAL_MAXN_TIME.load(Ordering::Relaxed);
    let mn_count = GLOBAL_MAXN_COUNT.load(Ordering::Relaxed);
    let mn_ms = mn_time as f64 / 1_000_000.0;
    let mn_pct = if total_ns > 0 { 100.0 * mn_time as f64 / total_ns as f64 } else { 0.0 };

    let am_time = GLOBAL_APPLY_MOVE_TIME.load(Ordering::Relaxed);
    let am_count = GLOBAL_APPLY_MOVE_COUNT.load(Ordering::Relaxed);
    let am_ms = am_time as f64 / 1_000_000.0;
    let am_pct = if total_ns > 0 { 100.0 * am_time as f64 / total_ns as f64 } else { 0.0 };
    let am_avg_us = if am_count > 0 { am_time as f64 / (am_count * 1000) as f64 } else { 0.0 };

    eprintln!("Search:");
    eprintln!("  Alpha-Beta: {:.2}ms ({:.1}%) - {} calls, {:.1}% cutoff rate",
        ab_ms, ab_pct, ab_count, cutoff_rate);
    eprintln!("  MaxN:       {:.2}ms ({:.1}%) - {} calls",
        mn_ms, mn_pct, mn_count);
    eprintln!("  Apply Move: {:.2}ms ({:.1}%) - {} calls, {:.2}µs avg\n",
        am_ms, am_pct, am_count, am_avg_us);

    let tt_lookups = GLOBAL_TT_LOOKUPS.load(Ordering::Relaxed);
    let tt_hits = GLOBAL_TT_HITS.load(Ordering::Relaxed);
    let hit_rate = if tt_lookups > 0 { 100.0 * tt_hits as f64 / tt_lookups as f64 } else { 0.0 };

    eprintln!("Transposition Table:");
    eprintln!("  Lookups:    {}", tt_lookups);
    eprintln!("  Hits:       {} ({:.1}%)\n", tt_hits, hit_rate);

    eprintln!("═══════════════════════════════════════════════════════════\n");
}

pub fn reset() {
    GLOBAL_MOVE_GEN_TIME.store(0, Ordering::Relaxed);
    GLOBAL_MOVE_GEN_COUNT.store(0, Ordering::Relaxed);
    GLOBAL_EVAL_TIME.store(0, Ordering::Relaxed);
    GLOBAL_EVAL_COUNT.store(0, Ordering::Relaxed);
    GLOBAL_FLOOD_FILL_TIME.store(0, Ordering::Relaxed);
    GLOBAL_FLOOD_FILL_COUNT.store(0, Ordering::Relaxed);
    GLOBAL_ADVERSARIAL_FLOOD_FILL_TIME.store(0, Ordering::Relaxed);
    GLOBAL_ADVERSARIAL_FLOOD_FILL_COUNT.store(0, Ordering::Relaxed);
    GLOBAL_APPLY_MOVE_TIME.store(0, Ordering::Relaxed);
    GLOBAL_APPLY_MOVE_COUNT.store(0, Ordering::Relaxed);
    GLOBAL_ALPHA_BETA_TIME.store(0, Ordering::Relaxed);
    GLOBAL_ALPHA_BETA_COUNT.store(0, Ordering::Relaxed);
    GLOBAL_ALPHA_BETA_CUTOFFS.store(0, Ordering::Relaxed);
    GLOBAL_MAXN_TIME.store(0, Ordering::Relaxed);
    GLOBAL_MAXN_COUNT.store(0, Ordering::Relaxed);
    GLOBAL_TT_LOOKUPS.store(0, Ordering::Relaxed);
    GLOBAL_TT_HITS.store(0, Ordering::Relaxed);
}

#[macro_export]
macro_rules! profile {
    ($category:expr, $code:block) => {{
        let _guard = $crate::simple_profiler::ProfileGuard::new($category);
        $code
    }};
}
