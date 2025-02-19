mod constants;
mod create_match;
mod elo;
mod fetch;
mod stats;

use chrono::{Local, Timelike};
use color_eyre::eyre::Result;
use create_match::create_matches;
use stats::Stats;
use std::thread::sleep;

fn main() -> Result<()> {
    color_eyre::install()?;

    let now = Local::now();
    if matches!(now.hour(), 3 | 4) {
        let next_five = now.with_hour(5).unwrap().with_minute(0).unwrap();
        let duration = (next_five - now).to_std().unwrap();
        sleep(duration);
    }

    let mut stats = Stats::load()?;

    let mut page = 0;
    let awaiting = stats
        .awaiting
        .min(stats.matches.last_key_value().map(|(k, _)| *k).unwrap_or(0));
    stats.awaiting = u32::MAX;
    while fetch::fetch(&mut stats, awaiting, page)? {
        page += 1;
        println!("Collected {} matches", stats.matches.len());
    }
    println!("Collected {} matches", stats.matches.len());
    stats.save()?;
    println!("Creating matches...");
    create_matches(&stats);

    Ok(())
}
