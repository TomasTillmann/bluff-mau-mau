//! Fixed, reproducible mixed baseline workload. Run with --release.
use bluff_mau_mau::engines::{
    baseline::Baseline,
    matches::{MatchOptions, run_match},
};
use std::time::Instant;
fn main() {
    let games: usize = std::env::args()
        .nth(1)
        .unwrap_or("1000".into())
        .parse()
        .expect("positive game count");
    assert!(games > 0);
    let bots = [
        Baseline::RandomLegal,
        Baseline::HonestFirst,
        Baseline::mixed(10, 10, 20).unwrap(),
        Baseline::mixed(0, 80, 30).unwrap(),
        Baseline::mixed(100, 100, 100).unwrap(),
        Baseline::mixed(40, 70, 60).unwrap(),
    ];
    let mut decisions = 0;
    let mut checksum = 0;
    let started = Instant::now();
    for i in 0..games {
        let a = i % bots.len();
        let b = (i / bots.len() + a + 1) % bots.len();
        let result = run_match(
            [&bots[a], &bots[b]],
            MatchOptions {
                seed: i as i64,
                bot_seeds: [10000 + i as i64, 20000 + i as i64],
                ..Default::default()
            },
        )
        .unwrap();
        decisions += result.decisions;
        checksum += (i + 1) * (result.winner().map_or(3, |p| p + 1)) + result.decisions;
    }
    let seconds = started.elapsed().as_secs_f64();
    println!(
        "{}",
        serde_json::json!({"games":games,"decisions":decisions,"checksum":checksum,"seconds":seconds,"games_per_second":games as f64/seconds})
    );
}
