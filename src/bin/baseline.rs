use bluff_mau_mau::engines::{
    Bot,
    baseline::Baseline,
    matches::{evaluate_grid, round_robin},
};

fn run() -> Result<(), String> {
    let (mut deals, mut seed, mut bot_seed, mut max_decisions, mut grid) =
        (20usize, 0i64, 10000i64, 1000usize, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "baseline [--deals 20] [--seed 0] [--bot-seed 10000] [--max-decisions 1000] [--grid]"
            );
            return Ok(());
        }
        if arg == "--grid" {
            grid = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {arg}"))?;
        match arg.as_str() {
            "--deals" => deals = value.parse().map_err(|_| "Invalid deals")?,
            "--seed" => seed = value.parse().map_err(|_| "Invalid seed")?,
            "--bot-seed" => bot_seed = value.parse().map_err(|_| "Invalid bot seed")?,
            "--max-decisions" => {
                max_decisions = value.parse().map_err(|_| "Invalid decision limit")?
            }
            _ => return Err(format!("Unknown option {arg}")),
        }
    }
    if deals == 0 || max_decisions == 0 {
        return Err("deals and max-decisions must be positive".into());
    }
    let seeds: Vec<i64> = (0..deals)
        .map(|offset| {
            seed.checked_add(i64::try_from(offset).map_err(|_| "Too many deals")?)
                .ok_or("Game seed overflow")
        })
        .collect::<Result<_, _>>()?;
    let bot_seeds = [
        bot_seed,
        bot_seed.checked_add(1).ok_or("Bot seed overflow")?,
    ];
    let stats = if grid {
        evaluate_grid(&seeds, bot_seeds, max_decisions)?
    } else {
        let bots = [
            Baseline::RandomLegal,
            Baseline::HonestFirst,
            Baseline::default(),
        ];
        let named: Vec<_> = bots
            .iter()
            .map(|bot| (bot.name(), bot as &dyn Bot))
            .collect();
        round_robin(&named, &seeds, bot_seeds, max_decisions)?
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&stats).map_err(|error| error.to_string())?
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("baseline: error: {error}");
        std::process::exit(1);
    }
}
