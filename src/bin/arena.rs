use bluff_mau_mau::engines::arena::{
    RunOptions, config, create_run, print_standings, run_arena, standings,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

fn run() -> Result<(), String> {
    let (mut resume, mut status, mut runs_dir): (
        Option<PathBuf>,
        Option<PathBuf>,
        Option<PathBuf>,
    ) = (None, None, None);
    let (mut roster, mut seed, mut rounds, mut max_decisions): (
        Option<String>,
        Option<i64>,
        Option<usize>,
        Option<usize>,
    ) = (None, None, None, None);
    let mut options = RunOptions::default();
    let mut workers_set = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "arena [--resume DIR | --status DIR] [--runs-dir runs] [--roster all|basic] [--seed 0] [--rounds 50] [--max-decisions 1000] [--workers 8] [--top 20]\nBoth games per pair commit atomically. Ctrl-C/SIGTERM stops cleanly; --resume continues.\nResume requires the same compiled engine and compiler fingerprint."
            );
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {arg}"))?;
        match arg.as_str() {
            "--resume" => resume = Some(value.into()),
            "--status" => status = Some(value.into()),
            "--runs-dir" => runs_dir = Some(value.into()),
            "--roster" => roster = Some(value),
            "--seed" => seed = Some(value.parse().map_err(|_| "Invalid seed")?),
            "--rounds" => rounds = Some(value.parse().map_err(|_| "Invalid rounds")?),
            "--max-decisions" => {
                max_decisions = Some(value.parse().map_err(|_| "Invalid max-decisions")?)
            }
            "--workers" => {
                options.workers = value.parse().map_err(|_| "Invalid workers")?;
                workers_set = true;
            }
            "--top" => options.top = value.parse().map_err(|_| "Invalid top")?,
            _ => return Err(format!("Unknown option {arg}")),
        }
    }
    if options.top == 0
        || options.workers == 0
        || options.workers.checked_mul(8).is_none()
        || rounds == Some(0)
        || max_decisions == Some(0)
    {
        return Err("workers, top, rounds, and max-decisions must be positive".into());
    }
    if resume.is_some() && status.is_some() {
        return Err("--resume and --status are mutually exclusive".into());
    }
    if (resume.is_some() || status.is_some())
        && (roster.is_some() || seed.is_some() || max_decisions.is_some() || runs_dir.is_some())
    {
        return Err("Stored settings cannot be overridden; only workers, top, and rounds may change on resume".into());
    }
    if let Some(path) = status {
        if workers_set || rounds.is_some() {
            return Err("--status accepts --top, not --workers or --rounds".into());
        }
        print_standings(&path, &config(&path)?, &standings(&path)?, options.top);
        return Ok(());
    }
    let stop = options.stop.clone();
    ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed))
        .map_err(|error| error.to_string())?;
    let path = match resume {
        Some(path) => path,
        None => create_run(
            &runs_dir.unwrap_or_else(|| "runs".into()),
            roster.as_deref().unwrap_or("all"),
            seed.unwrap_or(0),
            rounds.unwrap_or(50),
            max_decisions.unwrap_or(1000),
        )?,
    };
    options.rounds = rounds;
    run_arena(&path, options)?;
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("arena: error: {error}");
        std::process::exit(1);
    }
}
