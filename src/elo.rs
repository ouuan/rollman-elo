use crate::constants::*;

const K: f32 = 0.05;

pub fn elo(a: f32, b: f32, a_win: f32, opp: f32) -> (f32, f32) {
    let pa = 1.0 / (1.0 + 10.0_f32.powf((b - a) / ELO_STEP));
    let pb = 1.0 / (1.0 + 10.0_f32.powf((a - b) / ELO_STEP));
    let rate = K * ((opp - ELO_BASE) / ELO_STEP).exp();
    (a + rate * (a_win - pa), b + rate * (1.0 - a_win - pb))
}
