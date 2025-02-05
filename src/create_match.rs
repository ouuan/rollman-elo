use crate::constants::*;
use crate::stats::Stats;
use color_eyre::eyre::Result;
use ordered_float::NotNan;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use ureq::Body;

#[derive(Serialize)]
struct GameInfo {
    game_id: u32,
    player_number: u32,
}

const GAME_INFO: GameInfo = GameInfo {
    game_id: GAME_ID,
    player_number: 2,
};

#[derive(Deserialize)]
struct Room {
    id: u32,
}

#[derive(Serialize)]
struct Join<'a> {
    enter: bool,
    entity: &'a str,
    is_remote: bool,
    is_user: bool,
    order: u8,
}

// `send_json` uses Transfer-Encoding: chunked, which is somehow not recognized by Saiblo
fn post(url: &str, json: impl Serialize) -> Result<Body> {
    let data = serde_json::to_string(&json)?;
    let res = ureq::post(url)
        .header(TOKEN_HEADER, &*TOKEN)
        .header("Content-Type", "application/json")
        .send(data)?
        .into_body();
    Ok(res)
}

pub fn create_match(rollman: &str, ghost: &str) -> Result<()> {
    let room: Room = post(&format!("{BASE_URL}/rooms/"), GAME_INFO)?.read_json()?;
    let url = format!("{BASE_URL}/rooms/{}", room.id);

    let rollman_join = Join {
        enter: true,
        entity: rollman,
        is_remote: false,
        is_user: false,
        order: 0,
    };
    post(&format!("{url}/join/"), rollman_join)?;

    let ghost_join = Join {
        enter: true,
        entity: ghost,
        is_remote: false,
        is_user: false,
        order: 1,
    };
    post(&format!("{url}/join/"), ghost_join)?;

    ureq::post(format!("{url}/begin_match/"))
        .header(TOKEN_HEADER, &*TOKEN)
        .send_empty()?;

    Ok(())
}

#[derive(Deserialize)]
struct Count {
    count: usize,
}

pub fn create_matches(stats: &Stats) {
    const THRESHOLD: f32 = ELO_BASE - ELO_STEP;

    let mut pairs = Vec::new();

    for (rollman, rollman_elo) in stats.agents.iter().filter_map(|(id, agent)| {
        (agent.can_rollman() && agent.rollman_elo > THRESHOLD).then_some((id, agent.rollman_elo))
    }) {
        for (ghost, ghost_elo) in stats.agents.iter().filter_map(|(id, agent)| {
            (agent.can_ghost() && agent.ghost_elo > THRESHOLD).then_some((id, agent.ghost_elo))
        }) {
            let count = stats
                .count_rollman_ghost
                .get(rollman)
                .and_then(|m| m.get(ghost).copied())
                .unwrap_or_default();
            let weight = ((rollman_elo - ghost_elo).abs() / ELO_STEP).exp();
            pairs.push((
                rollman,
                ghost,
                NotNan::new(count as f32 * weight).expect("nan"),
            ));
        }
    }

    pairs.shuffle(&mut rand::rng());
    pairs.sort_unstable_by_key(|(_, _, count)| *count);

    let res: Count = ureq::get(format!("{BASE_URL}/matches/"))
        .query("limit", "1")
        .query("state", "评测中")
        .query("game", GAME_ID.to_string())
        .header(TOKEN_HEADER, &*TOKEN)
        .call()
        .expect("failed to get judging matches")
        .into_body()
        .read_json()
        .expect("invalid matches JSON");
    let create_count = MAX_MATCHES.saturating_sub(res.count);

    for (rollman, ghost, _) in pairs.into_iter().take(create_count) {
        if let Err(e) = create_match(rollman, ghost) {
            eprintln!("Failed to create match:\n{e:?}");
        }
    }
}
