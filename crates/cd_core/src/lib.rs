//! Core model for cycloid-drive: a single-stage cycloidal reducer (fixed ring pins, Zd = Zp − 1
//! lobes, output through pins in disc holes). Pure Rust, no web dependencies.
//!
//! Units: mm, N, N·mm internally (torque input in N·m), MPa = N/mm², angles in rad unless noted.
//!
//! Geometry (disc frame, disc centre at the origin, parameter φ ∈ [0, 2π)):
//! theoretical curve (path of the ring-pin centre relative to the disc, a curtate epitrochoid)
//!   C(φ) = Rp·(cos φ, sin φ) − e·(cos Zp·φ, sin Zp·φ)
//! disc profile = C offset inward by the pin radius rp along the unit normal.
//! At φ = 0 the profile lies at radius Rp − e − rp (a valley, where a pin seats).
//!
//! Load model (idealised, rigid parts, zero clearance): the pins on one half (sin θ < 0, θ = pin
//! angle measured from the eccentric direction) carry Fi = F0·|sin θi|, acting along the line through
//! the pitch point; F0 is set so Σ Fi·li = T per disc (li = moment arm about the disc centre).
//! Output pins: every pin touches its hole along the eccentric direction; qj = Q0·|sin βj| on the
//! loaded half, Σ qj·Rw·|sin βj| = T. The eccentric bearing carries the resultant of both sets.
pub mod export;
pub mod search;

use std::f64::consts::{PI, TAU};

/// Generic material values, all "typical" — not data for any specific product.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Young's modulus, MPa.
    pub e: f64,
    pub nu: f64,
    /// Allowable contact (Hertz) stress, MPa.
    pub allow: f64,
}

/// 0: PLA/PETG print, 1: Nylon/PA-CF print, 2: hardened steel (all typical).
pub const MATERIALS: [Material; 3] = [
    Material { e: 3_500.0, nu: 0.36, allow: 45.0 },
    Material { e: 7_000.0, nu: 0.35, allow: 70.0 },
    Material { e: 210_000.0, nu: 0.30, allow: 1_500.0 },
];
/// Ring and output pins are assumed to be steel dowels (typical).
pub const PIN_STEEL: Material = MATERIALS[2];
/// Minimum gap between neighbouring ring pins before a warning (mm).
pub const PIN_GAP: f64 = 0.5;
/// Minimum wall between holes / edge / bore before a warning (mm).
pub const MIN_WALL: f64 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Ring pin count (disc lobes Zd = Zp − 1).
    pub zp: u32,
    /// Pin circle radius, mm.
    pub rp: f64,
    /// Eccentricity, mm.
    pub e: f64,
    /// Ring pin diameter, mm.
    pub dp: f64,
    /// Disc thickness, mm.
    pub b: f64,
    /// 1 or 2 discs (180° phased).
    pub discs: u32,
    /// Peak output torque, N·m.
    pub torque: f64,
    /// Input speed, rpm.
    pub rpm: f64,
    /// Output pin count, diameter and pin circle radius (mm).
    pub zw: u32,
    pub dw: f64,
    pub rw: f64,
    /// Central bore for the eccentric bearing (bearing OD), mm.
    pub bore: f64,
    /// Index into `MATERIALS`.
    pub material: usize,
    /// Profile samples.
    pub n: usize,
}

impl Default for Params {
    /// A small 3D-printable example: 12 pins (11:1), Ø 90 mm class.
    fn default() -> Self {
        Params { zp: 12, rp: 40.0, e: 1.5, dp: 6.0, b: 10.0, discs: 2, torque: 3.0, rpm: 300.0, zw: 6, dw: 6.0, rw: 20.0, bore: 22.0, material: 1, n: 720 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

/// Which check (the UI maps the id to a label).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckId {
    K1,
    Undercut,
    PinSpacing,
    HoleSpacing,
    HoleEdge,
    Stress,
    PressureAngle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Check {
    pub id: CheckId,
    pub status: Status,
    /// Measured value and the limit it is compared with (units depend on the check).
    pub value: f64,
    pub limit: f64,
}

/// Forces at one instant (one disc, housing frame).
#[derive(Clone, Debug, Default)]
pub struct Loads {
    /// Force magnitude per ring pin, N (index = pin number).
    pub pin: Vec<f64>,
    /// Unit direction of the force on the disc per pin (housing frame).
    pub dir: Vec<[f64; 2]>,
    /// Moment arm about the disc centre per pin, mm.
    pub arm: Vec<f64>,
    /// Hertz contact stress per pin, MPa.
    pub sigma: Vec<f64>,
    /// Profile parameter φ of each pin's contact point (disc frame).
    pub phi: Vec<f64>,
    /// Force per output pin, N.
    pub out: Vec<f64>,
    /// Eccentric-bearing radial load vector (force on the disc from the bearing), N.
    pub bearing: [f64; 2],
}

impl Loads {
    pub fn max_pin(&self) -> f64 {
        self.pin.iter().copied().fold(0.0, f64::max)
    }
    pub fn max_sigma(&self) -> f64 {
        self.sigma.iter().copied().fold(0.0, f64::max)
    }
    pub fn max_out(&self) -> f64 {
        self.out.iter().copied().fold(0.0, f64::max)
    }
    pub fn bearing_load(&self) -> f64 {
        self.bearing[0].hypot(self.bearing[1])
    }
}

/// Evaluated design.
#[derive(Clone, Debug)]
pub struct Design {
    pub p: Params,
    pub zd: u32,
    /// Reduction ratio Zd / (Zp − Zd); the output turns opposite to the input.
    pub ratio: f64,
    /// Shortening coefficient K1 = e·Zp / Rp.
    pub k1: f64,
    /// Sampled theoretical curve and disc profile (disc frame), parameter φi = 2π·i/n.
    pub theo: Vec<[f64; 2]>,
    pub prof: Vec<[f64; 2]>,
    /// Pressure angle per sample, degrees.
    pub alpha: Vec<f64>,
    /// Minimum convex radius of curvature of the theoretical curve, mm.
    pub rho_min: f64,
    pub perimeter: f64,
    pub r_min: f64,
    pub r_max: f64,
    /// Output hole diameter = output pin Ø + 2e.
    pub hole_d: f64,
    /// Smallest wall between output holes, disc edge and bore, mm.
    pub min_wall: f64,
    pub pin_gap: f64,
    /// Housing OD estimate 2·(Rp + dp), mm.
    pub od: f64,
    /// Worst values over one pin-pitch of input rotation.
    pub f_max: f64,
    pub sigma_max: f64,
    pub bearing_max: f64,
    pub out_max: f64,
    /// Max pressure angle over the working zone (pins carrying ≥ 50 % of the peak force), degrees.
    pub alpha_work: f64,
    pub disc_rpm: f64,
    pub out_rpm: f64,
    pub wobble_hz: f64,
    pub checks: Vec<Check>,
    pub status: Status,
}

/// Threshold for the "working zone" used by the pressure-angle check.
pub const WORK_ZONE: f64 = 0.5;
/// Pressure angle above which the working zone is flagged (degrees).
pub const ALPHA_WARN: f64 = 85.0;

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn len(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}
fn polar(r: f64, a: f64) -> [f64; 2] {
    [r * a.cos(), r * a.sin()]
}

impl Params {
    pub fn zd(&self) -> u32 {
        self.zp.saturating_sub(1).max(1)
    }
    pub fn k1(&self) -> f64 {
        self.e * self.zp as f64 / self.rp
    }
    /// Theoretical curve point and its first / second derivatives at φ.
    pub fn curve(&self, phi: f64) -> ([f64; 2], [f64; 2], [f64; 2]) {
        let (zp, rp, e) = (self.zp as f64, self.rp, self.e);
        let (s, c) = phi.sin_cos();
        let (sz, cz) = (zp * phi).sin_cos();
        (
            [rp * c - e * cz, rp * s - e * sz],
            [-rp * s + e * zp * sz, rp * c - e * zp * cz],
            [-rp * c + e * zp * zp * cz, -rp * s + e * zp * zp * sz],
        )
    }
    /// Signed curvature of the theoretical curve (positive = convex), 1/mm.
    pub fn kappa(&self, phi: f64) -> f64 {
        let (_, d1, d2) = self.curve(phi);
        cross(d1, d2) / len(d1).powi(3).max(1e-300)
    }
    /// Disc profile point: theoretical point offset inward by the pin radius.
    pub fn profile(&self, phi: f64) -> [f64; 2] {
        let (c, d1, _) = self.curve(phi);
        let l = len(d1).max(1e-300);
        let r = self.dp / 2.0;
        [c[0] - r * d1[1] / l, c[1] + r * d1[0] / l]
    }
    /// Pitch point in the disc frame for parameter φ (on the line disc centre → ring centre).
    fn pitch(&self, phi: f64) -> [f64; 2] {
        polar(self.e * self.zd() as f64, self.zp as f64 * phi)
    }
    /// Pressure angle (deg): angle between the contact normal and the direction of motion of that
    /// point when the disc turns about its own centre (so the moment arm is |C|·cos α).
    pub fn pressure_angle(&self, phi: f64) -> f64 {
        let (c, _, _) = self.curve(phi);
        let n = sub(c, self.pitch(phi));
        let t = [-c[1], c[0]];
        let ca = (n[0] * t[0] + n[1] * t[1]).abs() / (len(n) * len(t)).max(1e-300);
        ca.clamp(0.0, 1.0).acos().to_degrees()
    }
    /// Effective modulus E* for steel pin against the disc material, MPa.
    pub fn e_star(&self) -> f64 {
        let d = MATERIALS[self.material.min(2)];
        1.0 / ((1.0 - PIN_STEEL.nu.powi(2)) / PIN_STEEL.e + (1.0 - d.nu.powi(2)) / d.e)
    }
    /// Disc pose in the housing frame for input angle ψ: (centre, rotation). Disc 1 runs 180°
    /// out of phase and is turned by −π/Zd so both discs drive the same output pins.
    pub fn disc_pose(&self, psi: f64, disc: usize) -> ([f64; 2], f64) {
        let zd = self.zd() as f64;
        let ph = if disc == 0 { 0.0 } else { PI };
        (polar(self.e, psi + ph), -psi / zd - ph / zd)
    }
    /// Output (and output flange) rotation for input angle ψ.
    pub fn output_angle(&self, psi: f64) -> f64 {
        -psi / self.zd() as f64
    }
    /// Angle of output hole j in the frame of disc `disc`. Disc 1 is turned by −π/Zd (see
    /// `disc_pose`), so its hole pattern is cut at +π/Zd to stay on the output pins.
    pub fn hole_angle(&self, j: usize, disc: usize) -> f64 {
        let ph = if disc == 0 { 0.0 } else { PI / self.zd() as f64 };
        TAU * j as f64 / self.zw.max(1) as f64 + ph
    }
    /// Housing angle of ring pin k.
    pub fn pin_angle(&self, k: usize) -> f64 {
        TAU * k as f64 / self.zp as f64
    }

    /// Forces on disc `disc` at input angle ψ (idealised load sharing, see crate docs).
    pub fn loads(&self, psi: f64, disc: usize) -> Loads {
        let zp = self.zp as usize;
        let t = self.torque * 1000.0 / self.discs.max(1) as f64;
        let (od, rot) = self.disc_pose(psi, disc);
        let ecc = psi + if disc == 0 { 0.0 } else { PI };
        let u = [ecc.cos(), ecc.sin()];
        let pitch = polar(self.e * self.zp as f64, ecc);
        let mut l = Loads::default();
        let mut sum = 0.0;
        for k in 0..zp {
            let a = self.pin_angle(k);
            let q = polar(self.rp, a);
            let d = sub(pitch, q);
            let dl = len(d).max(1e-300);
            let dir = [d[0] / dl, d[1] / dl];
            let arm = cross(sub(q, od), dir);
            let w = (-(a - ecc).sin()).max(0.0);
            sum += w * arm.max(0.0);
            l.pin.push(w);
            l.dir.push(dir);
            l.arm.push(arm.abs());
            l.phi.push((a - rot).rem_euclid(TAU));
        }
        let f0 = if sum > 0.0 { t / sum } else { 0.0 };
        let (es, b, r) = (self.e_star(), self.b, self.dp / 2.0);
        let mut res = [0.0, 0.0];
        for k in 0..zp {
            let f = l.pin[k] * f0;
            l.pin[k] = f;
            res = [res[0] + f * l.dir[k][0], res[1] + f * l.dir[k][1]];
            let kap = self.kappa(l.phi[k]);
            let den = 1.0 - r * kap;
            let sigma = if f <= 0.0 {
                0.0
            } else if den <= 0.0 {
                f64::INFINITY
            } else {
                hertz(f, b, 1.0 / (1.0 / r + kap / den), es)
            };
            l.sigma.push(sigma);
        }
        // Output pins (rotating with the output) push on the hole walls along −u.
        let mut s2 = 0.0;
        let out_rot = self.output_angle(psi);
        for j in 0..self.zw as usize {
            let beta = out_rot + TAU * j as f64 / self.zw.max(1) as f64;
            let w = (-(beta - ecc).sin()).max(0.0);
            s2 += w * w;
            l.out.push(w);
        }
        let q0 = if s2 > 0.0 && self.rw > 0.0 { t / (self.rw * s2) } else { 0.0 };
        for q in l.out.iter_mut() {
            *q *= q0;
            res = [res[0] - *q * u[0], res[1] - *q * u[1]];
        }
        l.bearing = [-res[0], -res[1]];
        l
    }

    pub fn eval(&self) -> Design {
        Design::new(*self)
    }
}

/// Hertz line contact: peak pressure p0 = sqrt(F·E* / (π·b·R*)), MPa (F in N, mm).
pub fn hertz(f: f64, b: f64, r_eff: f64, e_star: f64) -> f64 {
    (f * e_star / (PI * b * r_eff)).sqrt()
}

impl Design {
    pub fn new(p: Params) -> Design {
        let p = Params { zp: p.zp.max(3), n: p.n.max(16), discs: p.discs.clamp(1, 2), ..p };
        let n = p.n;
        let zd = p.zd();
        let rpin = p.dp / 2.0;
        let phis: Vec<f64> = (0..n).map(|i| TAU * i as f64 / n as f64).collect();
        let theo: Vec<[f64; 2]> = phis.iter().map(|&f| p.curve(f).0).collect();
        let prof: Vec<[f64; 2]> = phis.iter().map(|&f| p.profile(f)).collect();
        let alpha: Vec<f64> = phis.iter().map(|&f| p.pressure_angle(f)).collect();
        let rho_min = phis.iter().map(|&f| p.kappa(f)).filter(|k| *k > 0.0).map(|k| 1.0 / k).fold(f64::INFINITY, f64::min);
        let perimeter = (0..n).map(|i| len(sub(prof[(i + 1) % n], prof[i]))).sum();
        let radii = prof.iter().map(|q| len(*q));
        let r_min = radii.clone().fold(f64::INFINITY, f64::min);
        let r_max = radii.fold(0.0, f64::max);
        let hole_d = p.dw + 2.0 * p.e;
        let zw = p.zw.max(1) as f64;
        let hole_gap = if p.zw >= 2 { 2.0 * p.rw * (PI / zw).sin() - hole_d } else { f64::INFINITY };
        let edge = r_min - (p.rw + hole_d / 2.0);
        let bore = p.rw - hole_d / 2.0 - p.bore / 2.0;
        let min_wall = hole_gap.min(edge).min(bore);
        let pin_gap = 2.0 * p.rp * (PI / p.zp as f64).sin() - p.dp;
        let k1 = p.k1();

        // Worst case over one pin pitch of input rotation (the load pattern repeats after that).
        let (mut f_max, mut sigma_max, mut bearing_max, mut out_max, mut alpha_work) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for s in 0..8 {
            let l = p.loads(TAU / p.zp as f64 * s as f64 / 8.0, 0);
            let fm = l.max_pin();
            f_max = f_max.max(fm);
            sigma_max = sigma_max.max(l.max_sigma());
            bearing_max = bearing_max.max(l.bearing_load());
            out_max = out_max.max(l.max_out());
            for k in 0..l.pin.len() {
                if fm > 0.0 && l.pin[k] >= WORK_ZONE * fm {
                    alpha_work = alpha_work.max(p.pressure_angle(l.phi[k]));
                }
            }
        }
        let allow = MATERIALS[p.material.min(2)].allow;
        let grade = |fail: bool, warn: bool| if fail { Status::Fail } else if warn { Status::Warn } else { Status::Ok };
        let checks = vec![
            Check { id: CheckId::K1, status: grade(k1 >= 1.0, k1 > 0.9), value: k1, limit: 1.0 },
            Check { id: CheckId::Undercut, status: grade(rho_min <= rpin, rho_min < 1.2 * rpin), value: rho_min, limit: rpin },
            Check { id: CheckId::PinSpacing, status: grade(pin_gap <= 0.0, pin_gap < PIN_GAP), value: pin_gap, limit: PIN_GAP },
            Check { id: CheckId::HoleSpacing, status: grade(hole_gap <= 0.0, hole_gap < MIN_WALL), value: hole_gap, limit: MIN_WALL },
            Check { id: CheckId::HoleEdge, status: grade(edge.min(bore) <= 0.0, edge.min(bore) < MIN_WALL), value: edge.min(bore), limit: MIN_WALL },
            Check { id: CheckId::Stress, status: grade(!(sigma_max <= allow), sigma_max > 0.85 * allow), value: sigma_max, limit: allow },
            Check { id: CheckId::PressureAngle, status: grade(false, alpha_work > ALPHA_WARN), value: alpha_work, limit: ALPHA_WARN },
        ];
        let status = checks.iter().map(|c| c.status).max().unwrap_or(Status::Ok);
        let ratio = zd as f64 / (p.zp - zd) as f64;
        Design {
            p,
            zd,
            ratio,
            k1,
            theo,
            prof,
            alpha,
            rho_min,
            perimeter,
            r_min,
            r_max,
            hole_d,
            min_wall,
            pin_gap,
            od: 2.0 * (p.rp + p.dp),
            f_max,
            sigma_max,
            bearing_max,
            out_max,
            alpha_work,
            disc_rpm: -p.rpm / ratio,
            out_rpm: -p.rpm / ratio,
            wobble_hz: p.rpm / 60.0,
            checks,
            status,
        }
    }

    /// Stress margin 1 − σ/σallow (negative = over the limit).
    pub fn margin(&self) -> f64 {
        1.0 - self.sigma_max / MATERIALS[self.p.material.min(2)].allow
    }

    /// Number of local radius maxima of the sampled profile (= disc lobes).
    pub fn lobes(&self) -> usize {
        let r: Vec<f64> = self.prof.iter().map(|q| len(*q)).collect();
        let n = r.len();
        (0..n).filter(|&i| r[i] > r[(i + n - 1) % n] && r[i] >= r[(i + 1) % n]).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> Params {
        Params::default()
    }

    #[test]
    fn ratio_zp12_is_11() {
        let d = Params { zp: 12, ..p() }.eval();
        assert_eq!(d.zd, 11);
        assert!((d.ratio - 11.0).abs() < 1e-12);
    }

    #[test]
    fn profile_start_point_radius() {
        let q = p();
        let pt = q.profile(0.0);
        assert!((pt[0] - (q.rp - q.e - q.dp / 2.0)).abs() < 1e-9);
        assert!(pt[1].abs() < 1e-9);
    }

    #[test]
    fn profile_closed_with_zd_lobes() {
        for zp in [8, 12, 25, 40] {
            let q = Params { zp, e: 0.8, rp: 45.0, dp: 4.0, ..p() };
            let a = q.profile(0.0);
            let b = q.profile(TAU);
            assert!(len(sub(a, b)) < 1e-9, "closed");
            assert_eq!(q.eval().lobes(), zp as usize - 1, "lobes for Zp={zp}");
        }
    }

    #[test]
    fn k1_at_least_one_fails() {
        let d = Params { e: 40.0 / 12.0, ..p() }.eval();
        assert!(d.k1 >= 1.0 - 1e-12);
        let c = d.checks.iter().find(|c| c.id == CheckId::K1).unwrap();
        assert_eq!(c.status, Status::Fail);
        assert_eq!(d.status, Status::Fail);
    }

    #[test]
    fn small_curvature_flags_undercut() {
        // K1 = 0.3 (valid), but a huge pin: the convex lobe tips are sharper than the pin.
        let q = Params { zp: 12, rp: 40.0, e: 1.0, dp: 32.0, ..p() };
        let d = q.eval();
        assert!(d.k1 < 1.0);
        assert!(d.rho_min < q.dp / 2.0);
        let c = d.checks.iter().find(|c| c.id == CheckId::Undercut).unwrap();
        assert_eq!(c.status, Status::Fail);
        // The default example is fine.
        let d = p().eval();
        assert_eq!(d.checks.iter().find(|c| c.id == CheckId::Undercut).unwrap().status, Status::Ok);
    }

    #[test]
    fn pin_moments_sum_to_torque() {
        for discs in [1, 2] {
            let q = Params { discs, ..p() };
            for psi in [0.0, 0.37, 2.0, 5.1] {
                for disc in 0..discs as usize {
                    let l = q.loads(psi, disc);
                    let (od, _) = q.disc_pose(psi, disc);
                    let m: f64 = (0..q.zp as usize)
                        .map(|k| {
                            let pin = polar(q.rp, q.pin_angle(k));
                            cross(sub(pin, od), [l.dir[k][0] * l.pin[k], l.dir[k][1] * l.pin[k]])
                        })
                        .sum();
                    let t = q.torque * 1000.0 / discs as f64;
                    assert!((m - t).abs() < 1e-6, "{m} vs {t}");
                    // About half the pins are loaded.
                    let n = l.pin.iter().filter(|f| **f > 0.0).count();
                    assert!(n >= 5 && n <= 6, "{n} loaded");
                }
            }
        }
    }

    #[test]
    fn hertz_scales_with_sqrt_f_over_b() {
        let s = hertz(100.0, 10.0, 3.0, 1000.0);
        assert!((hertz(400.0, 10.0, 3.0, 1000.0) / s - 2.0).abs() < 1e-12);
        assert!((hertz(100.0, 40.0, 3.0, 1000.0) / s - 0.5).abs() < 1e-12);
        let a = Params { torque: 2.0, ..p() }.eval().sigma_max;
        let b = Params { torque: 8.0, ..p() }.eval().sigma_max;
        assert!((b / a - 2.0).abs() < 1e-9);
    }

    #[test]
    fn ring_pins_lie_on_theoretical_curve_in_disc_frame() {
        let q = Params { zp: 11, rp: 30.0, e: 1.2, ..p() };
        let dense: Vec<[f64; 2]> = (0..40_000).map(|i| q.curve(TAU * i as f64 / 40_000.0).0).collect();
        for disc in 0..2 {
            for psi in [0.0, 0.9, 4.4] {
                let (c, rot) = q.disc_pose(psi, disc);
                for k in 0..q.zp as usize {
                    let w = sub(polar(q.rp, q.pin_angle(k)), c);
                    let (s, co) = (-rot).sin_cos();
                    let local = [co * w[0] - s * w[1], s * w[0] + co * w[1]];
                    let d = dense.iter().map(|t| len(sub(*t, local))).fold(f64::INFINITY, f64::min);
                    assert!(d < 0.01, "pin {k} disc {disc} psi {psi}: {d}");
                }
            }
        }
    }

    #[test]
    fn output_holes_sit_e_from_output_pins_on_both_discs() {
        let q = p();
        for psi in [0.0, 1.3, 4.0] {
            for disc in 0..2 {
                let (c, rot) = q.disc_pose(psi, disc);
                for j in 0..q.zw as usize {
                    let hole = polar(q.rw, q.hole_angle(j, disc) + rot);
                    let hole = [hole[0] + c[0], hole[1] + c[1]];
                    let pin = polar(q.rw, q.output_angle(psi) + TAU * j as f64 / q.zw as f64);
                    assert!((len(sub(hole, pin)) - q.e).abs() < 1e-9, "disc {disc} hole {j}");
                }
            }
        }
    }

    #[test]
    fn default_example_passes() {
        let d = p().eval();
        assert!(d.status != Status::Fail, "{:?}", d.checks);
        assert!(d.f_max > 0.0 && d.bearing_max > 0.0 && d.out_max > 0.0);
        assert!((d.hole_d - (p().dw + 2.0 * p().e)).abs() < 1e-12);
    }
}
