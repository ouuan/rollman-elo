use crate::constants::*;
use crate::elo::elo;
use chrono::Local;
use color_eyre::eyre::Result;
use ordered_float::OrderedFloat;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};

#[derive(Default, Serialize, Deserialize)]
pub struct Stats {
    pub agents: HashMap<String, Agent>,
    pub matches: BTreeMap<u32, Match>,
    pub logic_version: u16,
    pub awaiting: u32,
    #[serde(skip)]
    pub matches_with_rollman: HashMap<String, Vec<(u32, Match)>>,
    #[serde(skip)]
    pub matches_with_ghost: HashMap<String, Vec<(u32, Match)>>,
    #[serde(skip)]
    pub count_rollman_ghost: HashMap<String, HashMap<String, u32>>,
}

impl Stats {
    pub fn add_match(&mut self, id: u32, m: Match) {
        let rollman = self.agents.get_mut(&m.rollman).unwrap();
        rollman.rollman_count += 1;
        rollman.rollman_time = rollman.rollman_time.min(id);
        let rollman_elo = rollman.rollman_elo;

        let ghost = self.agents.get_mut(&m.ghost).unwrap();
        ghost.ghost_count += 1;
        ghost.ghost_time = ghost.ghost_time.min(id);
        let ghost_elo = ghost.ghost_elo;

        let rollman_matches = self
            .matches_with_rollman
            .get(&m.rollman)
            .map_or::<&[_], _>(&[], Vec::as_slice);

        for (_, n) in rollman_matches {
            if n.ghost == m.ghost {
                continue;
            }
            let win = match m.ghost_score.cmp(&n.ghost_score) {
                Ordering::Greater => 1.0,
                Ordering::Equal => 0.5,
                Ordering::Less => 0.0,
            };
            let a = self.agents.get(&m.ghost).unwrap().ghost_elo;
            let b = self.agents.get(&n.ghost).unwrap().ghost_elo;
            let (new_a, new_b) = elo(a, b, win, rollman_elo, rollman_matches.len());
            self.agents.get_mut(&m.ghost).unwrap().ghost_elo = new_a;
            self.agents.get_mut(&n.ghost).unwrap().ghost_elo = new_b;
        }

        let ghost_matches = self
            .matches_with_ghost
            .get(&m.ghost)
            .map_or::<&[_], _>(&[], Vec::as_slice);

        for (_, n) in ghost_matches {
            if n.rollman == m.rollman {
                continue;
            }
            let win = match m.rollman_score.cmp(&n.rollman_score) {
                Ordering::Greater => 1.0,
                Ordering::Equal => 0.5,
                Ordering::Less => 0.0,
            };
            let a = self.agents.get(&m.rollman).unwrap().rollman_elo;
            let b = self.agents.get(&n.rollman).unwrap().rollman_elo;
            let (new_a, new_b) = elo(a, b, win, ghost_elo, ghost_matches.len());
            self.agents.get_mut(&m.rollman).unwrap().rollman_elo = new_a;
            self.agents.get_mut(&n.rollman).unwrap().rollman_elo = new_b;
        }

        self.matches_with_rollman
            .entry(m.rollman.clone())
            .or_default()
            .push((id, m.clone()));
        self.matches_with_ghost
            .entry(m.ghost.clone())
            .or_default()
            .push((id, m.clone()));
        self.count_rollman_ghost
            .entry(m.rollman.clone())
            .or_default()
            .entry(m.ghost.clone())
            .and_modify(|e| *e += 1)
            .or_insert(1);
        self.matches.insert(id, m);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn load() -> Result<Self> {
        let storage = match File::open("storage.json") {
            Ok(f) => f,
            Err(_) => return Ok(Self::default()),
        };
        let buf = BufReader::new(storage);
        let mut stats: Self = serde_json::from_reader(buf)?;

        for a in stats.agents.values_mut() {
            a.rollman_elo = ELO_BASE;
            a.ghost_elo = ELO_BASE;
            a.rollman_count = 0;
            a.ghost_count = 0;
            a.rollman_time = u32::MAX;
            a.ghost_time = u32::MAX;
        }

        let mut matches = BTreeMap::new();
        std::mem::swap(&mut stats.matches, &mut matches);
        let mut matches = matches.into_iter().collect::<Vec<_>>();

        let rng = &mut rand::rng();
        matches.shuffle(rng);

        for (id, m) in matches.clone() {
            stats.add_match(id, m);
        }

        stats.matches.clear();
        stats.matches_with_rollman.clear();
        stats.matches_with_ghost.clear();
        stats.count_rollman_ghost.clear();

        for a in stats.agents.values_mut() {
            a.rollman_count = 0;
            a.ghost_count = 0;
        }

        matches.shuffle(rng);

        for (id, m) in matches {
            stats.add_match(id, m);
        }

        Ok(stats)
    }

    pub fn save(&self) -> Result<()> {
        let storage = File::create("storage.json")?;
        let buf = BufWriter::new(storage);
        serde_json::to_writer(buf, self)?;

        let elo = File::create("elo.csv")?;
        let mut buf = BufWriter::new(elo);
        writeln!(&mut buf, "user,name,version,rollman_elo,ghost_elo")?;
        for agent in self.agents.values() {
            writeln!(
                &mut buf,
                "{},{},{},{},{}",
                agent.user, agent.name, agent.version, agent.rollman_elo, agent.ghost_elo
            )?;
        }

        let html = File::create("ranking.html")?;
        let mut buf = BufWriter::new(html);

        let mut rollmen: Vec<_> = self
            .agents
            .iter()
            .filter(|(_, a)| a.can_rollman())
            .collect();
        let mut ghosts: Vec<_> = self.agents.iter().filter(|(_, a)| a.can_ghost()).collect();

        let last_match = self.matches.last_key_value().map(|(k, _)| *k).unwrap_or(0);
        rollmen.sort_by_key(|(_, a)| Reverse(OrderedFloat(a.rollman_elo)));
        ghosts.sort_by_key(|(_, a)| Reverse(OrderedFloat(a.ghost_elo)));

        const RECENT_MATCH_COUNT: usize = 100;

        write!(
            buf,
            r#"<!DOCTYPE html>
<html>

<head>
  <title>RollMan Ranking</title>
  <style>
    table {{ border-collapse: collapse; margin-top: 1rem; }}
    th, td {{ padding: 10px; border: 1px solid #ddd; }}
    th {{ background-color: #f5f5f5; }}
    .flex {{ display: flex; flex-wrap: wrap; justify-content: space-around; }}
    .best {{ font-weight: bold; }}
    .unreliable {{ opacity: 0.5; }}
    .best-only:checked ~ table tr:not(.best):not(:first-child) {{ display: none; }}
    tr:not(:first-child) {{ counter-increment: row-num; }}
    tr:not(:first-child) td:first-child::before {{ content: counter(row-num); }}
  </style>
  <script>
    function copy(token) {{
      navigator.clipboard.writeText(token)
        .catch(() => alert(`Failed to copy token: ${{token}}`));
    }}
  </script>
  <script defer data-domain="misc.ouuan.moe" src="https://plausible.ouuan.moe/js/script.js"></script>
</head>

<body>
  <div class="flex">
    <div>
      <div>
        <a href="https://www.saiblo.net/game/42">RollMan (Saiblo)</a>
        <a href="https://www.saiblo.net/game/42?id=2">对局列表</a>
        <a href="https://github.com/ouuan/rollman-elo">Repo</a>
        最后更新于 {}
      </div>
      <div>
        <span {}>rating 颜色</span>基于 <a href="https://uoj.ac">UOJ</a>；
        <span style="background-color: rgb(240, 136, 62, 0.4);">最新 bot</span>；
        <span class="best">用户的最强 bot</span>；
        <span class="unreliable">对局数不足时 rating 不准确</span>
      </div>
    </div>
  </div>
  <div class="flex">"#,
            Local::now().format("%F %T"),
            rating_color(1500.0),
        )?;

        let mut rollman_users = HashSet::new();

        write!(
            buf,
            r#"
    <section>
      <h2>Rollman Ranking</h2>
      <input type="checkbox" class="best-only" id="best-only-rollman">
      <label for="best-only-rollman">只显示每个用户的最强 rollman</label>
      <table>
        <tr>
            <th>#</th>
            <th>User</th>
            <th>Bot</th>
            <th>Ver.</th>
            <th>Elo</th>
            <th title="对局数">#M</th>
            <th title="近{RECENT_MATCH_COUNT}局中的fail数">F%</th>
            <th>Token</th>
        </tr>"#
        )?;

        for (token, agent) in &rollmen {
            let mut matches = self
                .matches_with_rollman
                .get(*token)
                .unwrap()
                .iter()
                .map(|(id, _)| (Reverse(*id), false))
                .chain(agent.failure.iter().map(|id| (Reverse(*id), true)))
                .collect::<Vec<_>>();
            let fail_count = if matches.len() > RECENT_MATCH_COUNT {
                matches
                    .select_nth_unstable(RECENT_MATCH_COUNT)
                    .0
                    .iter()
                    .filter(|(_, f)| *f)
                    .count()
            } else {
                agent.failure.len()
            };
            write!(
                buf,
                r#"
        <tr{}>
          <td></td>
          <td><a href="https://www.saiblo.net/user/{}">{}</a></td>
          <td>{}</td><td>{}</td><td {}>{:.0}</td><td>{}</td><td>{}</td>
          <td><button onclick="copy('{}')">token</button>
        </tr>"#,
                row_style(
                    agent.rollman_count < ghosts.len(),
                    agent.rollman_time,
                    last_match,
                    rollman_users.insert(agent.user.clone())
                ),
                escape_html(&agent.user),
                escape_html(&agent.user),
                escape_html(&agent.name),
                agent.version,
                rating_color(agent.rollman_elo),
                agent.rollman_elo,
                agent.rollman_count,
                fail_count,
                token,
            )?;
        }
        write!(
            buf,
            "
      </table></section>"
        )?;

        write!(
            buf,
            r#"
    <section>
      <h2>Ghost Ranking</h2>
      <input type="checkbox" class="best-only" id="best-only-ghost">
      <label for="best-only-ghost">只显示每个用户的最强 ghost</label>
      <table>
        <tr>
            <th>#</th>
            <th>User</th>
            <th>Bot</th>
            <th>Ver.</th>
            <th>Elo</th>
            <th title="对局数">#M</th>
            <th title="近{RECENT_MATCH_COUNT}局中的fail数">F%</th>
            <th>Token</th>
        </tr>"#
        )?;

        let mut ghost_users = HashSet::new();

        for (token, agent) in &ghosts {
            let mut matches = self
                .matches_with_ghost
                .get(*token)
                .unwrap()
                .iter()
                .map(|(id, _)| (Reverse(*id), false))
                .chain(agent.failure.iter().map(|id| (Reverse(*id), true)))
                .collect::<Vec<_>>();
            let fail_count = if matches.len() > RECENT_MATCH_COUNT {
                matches
                    .select_nth_unstable(RECENT_MATCH_COUNT)
                    .0
                    .iter()
                    .filter(|(_, f)| *f)
                    .count()
            } else {
                agent.failure.len()
            };
            write!(
                buf,
                r#"
        <tr{}>
          <td></td>
          <td><a href="https://www.saiblo.net/user/{}">{}</a></td>
          <td>{}</td><td>{}</td><td {}>{:.0}</td><td>{}</td><td>{}</td>
          <td><button onclick="copy('{}')">token</button>
        </tr>"#,
                row_style(
                    agent.ghost_count < rollmen.len(),
                    agent.ghost_time,
                    last_match,
                    ghost_users.insert(agent.user.clone())
                ),
                escape_html(&agent.user),
                escape_html(&agent.user),
                escape_html(&agent.name),
                agent.version,
                rating_color(agent.ghost_elo),
                agent.ghost_elo,
                agent.ghost_count,
                fail_count,
                token,
            )?;
        }
        write!(
            buf,
            "
      </table>
    </section>"
        )?;

        writeln!(
            buf,
            r#"
  </div>
</body>

</html>"#
        )?;

        Ok(())
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn row_style(unreliable: bool, time: u32, last: u32, new_user: bool) -> String {
    let mut style = Vec::new();
    if last - time < RECENT_THRESHOLD {
        style.push(format!(
            "background-color: rgb(240, 136, 62, {});",
            0.5 * (RECENT_THRESHOLD + time - last) as f32 / RECENT_THRESHOLD as f32
        ));
    }

    let mut class = Vec::new();
    if unreliable {
        class.push("unreliable");
    }
    if new_user {
        class.push("best");
    }

    let mut result = String::new();
    if !class.is_empty() {
        result += &format!(r#" class="{}""#, class.join(" "));
    }
    if !style.is_empty() {
        result += &format!(r#" style="{}""#, style.join(" "));
    }
    result
}

// https://github.com/vfleaking/uoj/blob/04061436e53ac7390b34aac6760e03fc6ad6b39f/web/public/js/uoj.js#L146
fn rating_color(rating: f32) -> String {
    let rating = rating.clamp(300.0, 2500.0);
    let (h, s, v) = if rating < 1500.0 {
        const H: f32 = 300.0 - (1500.0 - 850.0) * 300.0 / 1650.0;
        const S: f32 = 30.0 + (1500.0 - 850.0) * 70.0 / 1650.0;
        const V: f32 = 50.0 + (1500.0 - 850.0) * 50.0 / 1650.0;
        let k = (rating - 300.0) / 1200.0;
        (
            H + (300.0 - H) * (1.0 - k),
            30.0 + (S - 30.0) * k,
            50.0 + (V - 50.0) * k,
        )
    } else {
        let k = (rating - 850.0) / 1650.0;
        (300.0 - 300.0 * k, 30.0 + 70.0 * k, 50.0 + 50.0 * k)
    };
    let l = v - v * s / 200.0;
    let m = l.min(100.0 - l);
    let s = if m < 0.1 { 0.0 } else { 100.0 * (v - l) / m };
    format!(
        r#"style="font-weight: bold; color: hsl({} {} {});""#,
        h, s, l
    )
}

#[derive(Serialize, Deserialize)]
pub struct Agent {
    pub user: String,
    pub name: String,
    pub version: u32,
    #[serde(skip)]
    pub rollman_elo: f32,
    #[serde(skip)]
    pub ghost_elo: f32,
    #[serde(skip)]
    pub rollman_count: usize,
    #[serde(skip)]
    pub ghost_count: usize,
    #[serde(skip)]
    pub rollman_time: u32,
    #[serde(skip)]
    pub ghost_time: u32,
    #[serde(default)]
    pub failure: BTreeSet<u32>,
}

impl Agent {
    pub fn new(user: String, name: String, version: u32) -> Self {
        Self {
            user,
            name,
            version,
            rollman_elo: ELO_BASE,
            ghost_elo: ELO_BASE,
            rollman_count: 0,
            ghost_count: 0,
            rollman_time: u32::MAX,
            ghost_time: u32::MAX,
            failure: BTreeSet::new(),
        }
    }

    pub fn can_rollman(&self) -> bool {
        self.rollman_count > self.failure.len().saturating_sub(50) * 10
    }

    pub fn can_ghost(&self) -> bool {
        self.ghost_count > self.failure.len().saturating_sub(50) * 10
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Match {
    pub rollman: String,
    pub ghost: String,
    pub rollman_score: i16,
    pub ghost_score: i16,
}
