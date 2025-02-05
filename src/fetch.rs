use crate::constants::*;
use crate::stats::*;
use color_eyre::eyre::Result;
use serde::Deserialize;
use std::io::{BufRead, BufReader};

const PAGE_SIZE: usize = 20;

#[derive(Deserialize)]
struct Response {
    results: Vec<MatchInfo>,
}

#[derive(Deserialize)]
struct MatchInfo {
    id: u32,
    logic_version: Option<u16>,
    state: String,
    info: (AgentInfo, AgentInfo),
}

#[derive(Deserialize)]
struct AgentInfo {
    code: Option<CodeInfo>,
    user: UserInfo,
    score: i16,
    end_state: Option<String>,
}

#[derive(Deserialize)]
struct CodeInfo {
    id: String,
    entity: String,
    version: u32,
}

#[derive(Deserialize)]
struct UserInfo {
    username: String,
}

#[derive(Deserialize)]
struct ReplayLine {
    score: (i16, i16),
}

pub fn fetch(stats: &mut Stats, awaiting: u32, page: usize) -> Result<bool> {
    let req = ureq::get(format!("{BASE_URL}/matches/"))
        .query("limit", PAGE_SIZE.to_string())
        .query("offset", (page * PAGE_SIZE).to_string())
        .query("game", GAME_ID.to_string())
        .header(TOKEN_HEADER, &*TOKEN);
    let res: Response = req.call()?.into_body().read_json()?;

    for result in res.results {
        if result.state == "评测中" || result.state == "准备中" {
            stats.awaiting = result.id;
            continue;
        }
        if result.state == "评测失败" {
            continue;
        }
        let logic_version = if let Some(logic_version) = result.logic_version {
            logic_version
        } else {
            continue;
        };
        if logic_version < stats.logic_version {
            return Ok(false);
        }
        if stats.matches.contains_key(&result.id) {
            if result.id < awaiting {
                return Ok(false);
            }
            continue;
        }
        let (code0, code1) = match (result.info.0.code, result.info.1.code) {
            (Some(code0), Some(code1)) => (code0, code1),
            _ => continue,
        };
        if result.info.0.score == result.info.1.score
            || result.info.0.end_state != Some("OK".to_string())
            || result.info.1.end_state != Some("OK".to_string())
        {
            if result.info.0.end_state != Some("OK".to_string())
                && result.info.1.end_state == Some("OK".to_string())
            {
                if let Some(agent) = stats.agents.get_mut(&code0.id) {
                    agent.failure.insert(result.id);
                }
            }
            if result.info.1.end_state != Some("OK".to_string())
                && result.info.0.end_state == Some("OK".to_string())
            {
                if let Some(agent) = stats.agents.get_mut(&code1.id) {
                    agent.failure.insert(result.id);
                }
            }
            continue;
        }
        if logic_version > stats.logic_version {
            stats.clear();
            stats.logic_version = logic_version;
        }

        let replay_reader = match ureq::get(&format!("{BASE_URL}/matches/{}/download/", result.id))
            .header(TOKEN_HEADER, &*TOKEN)
            .call()
        {
            Ok(res) => res.into_body().into_reader(),
            _ => continue,
        };

        let last_line = if let Some(line) = BufReader::new(replay_reader).lines().last() {
            line?
        } else {
            continue;
        };
        let replay_line: ReplayLine = serde_json::from_str(&last_line)?;
        let (rollman_score, ghost_score) = replay_line.score;

        let (rollman, ghost) =
            if result.info.0.score == rollman_score && result.info.1.score == ghost_score {
                (code0.id.clone(), code1.id.clone())
            } else if result.info.1.score == rollman_score && result.info.0.score == ghost_score {
                (code1.id.clone(), code0.id.clone())
            } else {
                eprintln!("Invalid scores: {}", result.id);
                continue;
            };

        stats.agents.entry(code0.id).or_insert(Agent::new(
            result.info.0.user.username,
            code0.entity,
            code0.version,
        ));
        stats.agents.entry(code1.id).or_insert(Agent::new(
            result.info.1.user.username,
            code1.entity,
            code1.version,
        ));

        let m = Match {
            rollman,
            ghost,
            rollman_score,
            ghost_score,
        };
        stats.add_match(result.id, m);
    }

    Ok(true)
}
