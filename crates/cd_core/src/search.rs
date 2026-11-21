//! "Find designs": for a target ratio (Zp = ratio + 1) and a max housing OD, sweep
//! e = 0.5…3 mm (step 0.25), dp from `DP_LIST` and Rp in 1 mm steps up to the OD limit
//! (OD = 2·(Rp + dp)). Output pins keep the user's count / Ø and sit midway between the bore and
//! the profile valleys. Keeps the best Rp per (e, dp), drops anything with a failing check and
//! sorts by (number of OK checks ↓, stress margin ↓, OD ↑), so designs with warnings rank lower.
use crate::{Params, Status};

pub const DP_LIST: [f64; 7] = [2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub params: Params,
    pub ok: usize,
    pub margin: f64,
    pub od: f64,
    pub status: Status,
}

fn better(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    b.ok.cmp(&a.ok).then(b.margin.total_cmp(&a.margin)).then(a.od.total_cmp(&b.od))
}

pub fn search(base: &Params, ratio: u32, max_od: f64, limit: usize) -> Vec<Candidate> {
    let zp = ratio.max(2) + 1;
    let mut out: Vec<Candidate> = vec![];
    for &dp in &DP_LIST {
        for ei in 0..=10 {
            let e = 0.5 + 0.25 * ei as f64;
            let mut best: Option<Candidate> = None;
            let rp_max = (max_od / 2.0 - dp).floor() as i32;
            for rp in 8..=rp_max {
                let rp = rp as f64;
                let r_min = rp - e - dp / 2.0;
                let p = Params { zp, rp, e, dp, n: 180, rw: (base.bore / 2.0 + r_min) / 2.0, ..*base };
                let d = p.eval();
                if d.status == Status::Fail {
                    continue;
                }
                let c = Candidate {
                    params: Params { n: base.n, ..p },
                    ok: d.checks.iter().filter(|c| c.status == Status::Ok).count(),
                    margin: d.margin(),
                    od: d.od,
                    status: d.status,
                };
                if best.as_ref().is_none_or(|b| better(&c, b).is_lt()) {
                    best = Some(c);
                }
            }
            out.extend(best);
        }
    }
    out.sort_by(better);
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_pass_and_are_sorted() {
        let base = Params::default();
        let r = search(&base, 11, 100.0, 12);
        assert!(!r.is_empty() && r.len() <= 12);
        for c in &r {
            assert_eq!(c.params.zp, 12);
            assert!(c.od <= 100.0 + 1e-9);
            let d = c.params.eval();
            assert_ne!(d.status, Status::Fail, "{:?}", d.checks);
        }
        assert!(r.windows(2).all(|w| better(&w[0], &w[1]).is_le()));
    }
}
