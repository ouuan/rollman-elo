use std::sync::LazyLock;

pub const BASE_URL: &str = "https://api.saiblo.net/api";
pub const GAME_ID: u32 = 42;

pub const TOKEN_HEADER: &str = "Authorization";
pub static TOKEN: LazyLock<String> =
    LazyLock::new(|| std::env::var("SAIBLO_TOKEN").expect("SAIBLO_TOKEN not set"));

pub const MAX_MATCHES: usize = 210;
pub const RECENT_THRESHOLD: u32 = 5000;
pub const ELO_BASE: f32 = 1500.0;
pub const ELO_STEP: f32 = 400.0;
