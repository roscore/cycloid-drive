//! 3D reducer scene on top of the generic renderer: housing + ring pins, cycloid disc(s) extruded
//! from the computed profile (with real bore and output holes), eccentric shaft, output pins and
//! flange; force arrows, pressure-angle edge colouring, picking for direct manipulation.
//! Axis = scene y (ground y = 0). Model (x, y) in mm maps to scene (x·S, ·, −y·S).
use crate::gl::{self, Geo, Mesh, Paint, Renderer};
use crate::math::{self, Orbit, OrbitCam, M4, V};
use crate::plot2d::{alpha_col, heat};
use cd_core::{export::hole_centres_of, Design};
use web_sys::HtmlCanvasElement;

/// Scene units per mm (1 unit = 10 mm).
pub const S: f32 = 0.1;

pub fn to3(p: [f64; 2], y: f32) -> V {
    [p[0] as f32 * S, y, -(p[1] as f32) * S]
}
fn xz(p: [f64; 2]) -> [f32; 2] {
    [p[0] as f32 * S, -(p[1] as f32) * S]
}
/// Inverse of `to3` for a scene point (mm).
pub fn from3(v: V) -> [f64; 2] {
    [(v[0] / S) as f64, (-v[2] / S) as f64]
}
/// Rotation about +y by `a` (equals a CCW model-frame rotation) then translation.
fn rot_y(a: f32, t: V) -> M4 {
    let (s, c) = a.sin_cos();
    math::basis([c, 0., -s], [0., 1., 0.], [s, 0., c], [1.; 3], t)
}
/// Unit cylinder placed with centre axis at `c` (x, z), radius `r`, from y0 to y1.
fn cyl_m(c: V, r: f32, y0: f32, y1: f32) -> M4 {
    math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [2. * r, (y1 - y0).max(1e-4), 2. * r], [c[0], (y0 + y1) / 2., c[2]])
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    None,
    Pin(usize),
    Ecc,
    Disc,
}

#[derive(Clone, Copy)]
pub struct Toggles {
    pub force: bool,
    pub pa: bool,
    pub glass: bool,
    pub disc_glass: bool,
    pub explode: bool,
}

/// Heights in scene units for an explode factor x (0..1).
pub struct Layout {
    pub base: f32,
    pub disc: [f32; 2],
    pub b: f32,
    pub stack_top: f32,
    pub flange: f32,
    pub shaft_top: f32,
    pub marker_r: f32,
}

pub fn layout(d: &Design, x: f32) -> Layout {
    let p = &d.p;
    let b = p.b as f32 * S;
    let base = 4.0 * S;
    let step = b + S;
    let lift = x * (b + 14.0 * S);
    let disc = [base + S + lift, base + S + step + 2.0 * lift];
    let stack_top = base + S + step * p.discs as f32 - S;
    let flange = stack_top + S + lift * (p.discs as f32 + 1.0);
    Layout { base, disc, b, stack_top, flange, shaft_top: flange + 5.0 * S + 10.0 * S, marker_r: (p.dp as f32 * 0.35).max(2.0) * S }
}

pub struct View3d {
    pub r: Renderer,
    pub canvas: HtmlCanvasElement,
    pub cam: OrbitCam,
    pub hover: Hit,
    cyl: Mesh,
    sph: Mesh,
    head: Mesh,
    /// One mesh per disc: disc 1's hole pattern is turned by π/Zd.
    disc: [Option<Mesh>; 2],
    rim: Option<Mesh>,
    housing: Option<Mesh>,
    pin_batch: Option<Mesh>,
    explode: f32,
    inv_vp: M4,
}

impl View3d {
    pub fn new(canvas: HtmlCanvasElement, dark: bool) -> Option<Self> {
        let mut r = Renderer::new(&canvas)?;
        r.theme = if dark { gl::DARK } else { gl::LIGHT };
        r.cell = 1.0;
        let head = gl::frustum([0.0, 0.5, 0.0], &[[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]]);
        Some(View3d {
            cyl: r.mesh(&gl::cylinder(28))?,
            sph: r.mesh(&gl::sphere(16))?,
            head: r.mesh(&head)?,
            r,
            canvas,
            cam: OrbitCam::new(Orbit { target: [0.0; 3], yaw: 0.7, pitch: 0.5, dist: 20.0 }),
            hover: Hit::None,
            disc: [None, None],
            rim: None,
            housing: None,
            pin_batch: None,
            explode: 0.0,
            inv_vp: math::IDENTITY,
        })
    }

    /// Camera preset: 0 perspective, 1 front (axial, top-down), 2 side, 3 exploded.
    pub fn preset(d: &Design, i: usize) -> Orbit {
        let l = layout(d, if i == 3 { 1.0 } else { 0.0 });
        let dist = d.od as f32 * S * if i == 3 { 2.4 } else { 2.0 };
        let target = [0.0, l.flange * 0.5, 0.0];
        let (yaw, pitch) = [(0.7, 0.5), (0.0, 1.55), (0.0, 0.02), (0.7, 0.35)][i.min(3)];
        Orbit { target, yaw, pitch, dist }
    }

    fn swap(&self, slot: &mut Option<Mesh>, g: &Geo) {
        if let Some(m) = slot.take() {
            self.r.free(m);
        }
        *slot = self.r.mesh(g);
    }

    /// Re-extrude the disc and rebuild static meshes after a parameter change.
    pub fn rebuild(&mut self, d: &Design) {
        let p = &d.p;
        let l = layout(d, 0.0);
        let rh = d.hole_d / 2.0;
        for k in 0..2 {
            let mut slot = self.disc[k].take();
            if k < p.discs as usize {
                self.swap(&mut slot, &disc_geo(d, &hole_centres_of(d, k), rh, l.b));
            } else if let Some(m) = slot.take() {
                self.r.free(m);
            }
            self.disc[k] = slot;
        }


        // Pressure-angle ribbon just outside the disc wall.
        let mut rg = Geo::default();
        let n = d.prof.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let off = |q: [f64; 2]| {
                let r = q[0].hypot(q[1]);
                [q[0] * (r + 0.15) / r, q[1] * (r + 0.15) / r]
            };
            let (a, b) = (to3(off(d.prof[i]), -0.01), to3(off(d.prof[j]), -0.01));
            let (a2, b2) = ([a[0], l.b + 0.01, a[2]], [b[0], l.b + 0.01, b[2]]);
            let nrm = math::norm(math::cross(math::sub(b, a), [0.0, 1.0, 0.0]));
            let (ca, cb) = (alpha_col(d.alpha[i]), alpha_col(d.alpha[j]));
            for (v, c) in [(a, ca), (b, cb), (b2, cb), (a, ca), (b2, cb), (a2, ca)] {
                rg.pos.extend(v);
                rg.nrm.extend(nrm);
                rg.col.extend(c);
            }
        }
        let mut rim = None;
        self.swap(&mut rim, &rg);
        self.rim = rim;

        let ring = |r: f64| -> Vec<[f32; 2]> { (0..120).map(|i| xz([r * (i as f64 * 0.05236).cos(), r * (i as f64 * 0.05236).sin()])).collect() };
        let mut hg = gl::prism(&ring(p.rp + p.dp * 0.2), &ring(d.od / 2.0), l.stack_top + S);
        for v in hg.pos.chunks_mut(3) {
            v[1] = v[1].max(l.base * 0.5);
        }
        let mut housing = None;
        self.swap(&mut housing, &hg);
        self.housing = housing;

        let cyl = gl::cylinder(16);
        let parts: Vec<(&Geo, M4, V)> = (0..p.zp as usize)
            .map(|k| {
                let a = p.pin_angle(k);
                (&cyl, cyl_m(to3([p.rp * a.cos(), p.rp * a.sin()], 0.0), p.dp as f32 * 0.5 * S, l.base, l.stack_top + S), [1.0; 3])
            })
            .collect();
        let mut pb = None;
        self.swap(&mut pb, &gl::batch(&parts));
        self.pin_batch = pb;
    }

    fn size(&self) -> (f64, f64) {
        (self.canvas.client_width().max(1) as f64, self.canvas.client_height().max(1) as f64)
    }

    /// World ray through a CSS-pixel position on the canvas.
    pub fn ray(&self, px: f64, py: f64) -> (V, V) {
        let (w, h) = self.size();
        math::ndc_ray(&self.inv_vp, (2.0 * px / w - 1.0) as f32, (1.0 - 2.0 * py / h) as f32)
    }
    pub fn plane_hit(&self, px: f64, py: f64, y: f32) -> Option<V> {
        let (o, d) = self.ray(px, py);
        math::ray_plane(o, d, [0.0, y, 0.0], [0.0, 1.0, 0.0])
    }
    pub fn layout(&self, d: &Design) -> Layout {
        layout(d, self.explode)
    }
    pub fn marker_pos(&self, d: &Design, psi: f64) -> V {
        let l = self.layout(d);
        let (c, _) = d.p.disc_pose(psi, 0);
        let mut m = to3(c, l.shaft_top + l.marker_r);
        m[1] = l.shaft_top + l.marker_r;
        m
    }

    pub fn pick(&self, d: &Design, psi: f64, px: f64, py: f64) -> Hit {
        let (o, dir) = self.ray(px, py);
        let l = self.layout(d);
        if math::ray_sphere(o, dir, self.marker_pos(d, psi), l.marker_r * 1.8).is_some() {
            return Hit::Ecc;
        }
        let p = &d.p;
        let mut best = (f32::INFINITY, Hit::None);
        for k in 0..p.zp as usize {
            let a = p.pin_angle(k);
            let c = to3([p.rp * a.cos(), p.rp * a.sin()], 0.0);
            if let Some(t) = ray_vcyl(o, dir, c, (p.dp as f32 * 0.5 + 1.0) * S, l.base, l.stack_top + S) {
                if t < best.0 {
                    best = (t, Hit::Pin(k));
                }
            }
        }
        let top = l.disc[p.discs as usize - 1] + l.b;
        if let Some(h) = math::ray_plane(o, dir, [0.0, top, 0.0], [0.0, 1.0, 0.0]) {
            let (c, _) = p.disc_pose(psi, p.discs as usize - 1);
            let q = from3(h);
            let r = (q[0] - c[0]).hypot(q[1] - c[1]);
            let t = math::dot(math::sub(h, o), dir);
            if r > d.r_min * 0.55 && r < d.r_max + 1.0 && t < best.0 {
                best = (t, Hit::Disc);
            }
        }
        best.1
    }

    /// Advance the camera / explode animation and draw. Returns true while something still moves.
    pub fn frame(&mut self, d: &Design, psi: f64, tg: Toggles, dt: f32) -> bool {
        let dpr = web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0);
        let (cw, ch) = self.size();
        let (pw, ph) = ((cw * dpr) as u32, (ch * dpr) as u32);
        if self.canvas.width() != pw || self.canvas.height() != ph {
            self.canvas.set_width(pw);
            self.canvas.set_height(ph);
        }
        let mut moving = self.cam.step(dt);
        let goal = if tg.explode { 1.0 } else { 0.0 };
        if (self.explode - goal).abs() > 1e-3 {
            self.explode += (goal - self.explode) * (1.0 - (-dt * 8.0).exp());
            moving = true;
        } else {
            self.explode = goal;
        }
        let o = self.cam.cur;
        let eye = o.eye();
        let view = math::look_at(eye, o.target, [0.0, 1.0, 0.0]);
        let proj = math::perspective(0.8, (cw / ch) as f32, 0.02 * o.dist.max(1.0), 60.0 * o.dist.max(5.0));
        self.inv_vp = math::invert(&math::mul(&proj, &view)).unwrap_or(math::IDENTITY);
        let sz = d.od as f32 * S;
        self.r.fog = [sz * 6.0 + o.dist, sz * 20.0 + o.dist * 3.0];
        self.r.begin(&view, &proj, eye, [0, 0, pw as i32, ph as i32]);
        self.draw_scene(d, psi, tg);
        moving
    }

    fn draw_scene(&self, d: &Design, psi: f64, tg: Toggles) {
        let (r, p, l) = (&self.r, &d.p, self.layout(d));
        let lit = |c: V, a: f32| Paint::lit([c[0], c[1], c[2], a]);
        let glow = |mut q: Paint, on: bool| {
            if on {
                q.glow = 0.35;
            }
            q
        };
        let id = math::IDENTITY;
        let od2 = d.od as f32 * 0.5 * S;
        let loads = p.loads(psi, 0);
        let fmax = loads.max_pin().max(1e-9);
        let out_rot = p.output_angle(psi) as f32;
        let fl_r = ((p.rw + p.dw) as f32 * S).min(d.r_min as f32 * S);
        let flange_m = cyl_m([0.0; 3], fl_r, l.flange, l.flange + 5.0 * S);
        let disc_m = |k: usize| {
            let (c, rot) = p.disc_pose(psi, k);
            rot_y(rot as f32, to3(c, l.disc[k]))
        };

        // Shadows first (they lie on the ground under everything).
        if let Some(m) = &self.pin_batch {
            r.shadow(m, &id);
        }
        for k in 0..p.discs as usize {
            if let Some(m) = &self.disc[k] {
                r.shadow(m, &disc_m(k));
            }
        }
        r.shadow(&self.cyl, &flange_m);

        r.draw(&self.cyl, &cyl_m([0.0; 3], od2, 0.0, l.base), lit([0.30, 0.32, 0.36], 1.0));
        for k in 0..p.zp as usize {
            let a = p.pin_angle(k);
            let c = to3([p.rp * a.cos(), p.rp * a.sin()], 0.0);
            let f = loads.pin[k];
            let col = if tg.force && f > 0.0 { heat((f / fmax) as f32) } else { [0.74, 0.76, 0.80] };
            // Dragging any pin changes Rp, so all pins light up a little on hover.
            let g = match self.hover {
                Hit::Pin(h) if h == k => 0.4,
                Hit::Pin(_) => 0.12,
                _ => 0.0,
            };
            let paint = Paint { glow: g, ..lit(col, 1.0) };
            r.draw(&self.cyl, &cyl_m(c, p.dp as f32 * 0.5 * S, l.base, l.stack_top + S), paint);
        }
        // Input shaft, eccentric cams (bearings), marker.
        let shaft_r = (p.bore as f32 * 0.22).max(1.0) * S;
        r.draw(&self.cyl, &cyl_m([0.0; 3], shaft_r, 0.0, l.shaft_top), lit([0.82, 0.83, 0.86], 1.0));
        for k in 0..p.discs as usize {
            let (c, _) = p.disc_pose(psi, k);
            r.draw(&self.cyl, &cyl_m(to3(c, 0.0), p.bore as f32 * 0.5 * S * 0.98, l.disc[k] + 0.05 * S, l.disc[k] + l.b - 0.05 * S), lit([0.86, 0.70, 0.34], 1.0));
        }
        let mk = self.marker_pos(d, psi);
        let ms = l.marker_r;
        r.draw(&self.sph, &math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [ms; 3], mk), glow(lit([0.98, 0.55, 0.20], 1.0), self.hover == Hit::Ecc));
        r.draw(&self.cyl, &cyl_m([mk[0] * 0.5, 0.0, mk[2] * 0.5], ms * 0.35, l.shaft_top - S, l.shaft_top + ms), lit([0.98, 0.55, 0.20], 1.0));

        // Output pins ride with the flange; holes in the discs are real cut-outs.
        let l0 = layout(d, 0.0);
        let pin_len = l0.flange - l0.disc[0] + 0.5 * S;
        for j in 0..p.zw as usize {
            let a = out_rot + std::f32::consts::TAU * j as f32 / p.zw.max(1) as f32;
            let c = to3([p.rw * a.cos() as f64, p.rw * a.sin() as f64], 0.0);
            r.draw(&self.cyl, &cyl_m(c, p.dw as f32 * 0.5 * S, l.flange - pin_len, l.flange + 0.1 * S), lit([0.90, 0.90, 0.92], 1.0));
        }
        if !tg.glass {
            r.draw(&self.cyl, &flange_m, lit([0.60, 0.63, 0.68], 1.0));
        }
        // A key notch on the flange so its rotation is visible.
        let kc = to3([fl_r as f64 / S as f64 * 0.8 * (out_rot as f64).cos(), fl_r as f64 / S as f64 * 0.8 * (out_rot as f64).sin()], 0.0);
        r.draw(&self.cyl, &cyl_m(kc, 1.6 * S, l.flange + 5.0 * S, l.flange + 6.0 * S), lit([0.98, 0.55, 0.20], 1.0));

        let disc_cols = [[0.36, 0.56, 0.88], [0.86, 0.56, 0.32]];
        let disc_a = if tg.disc_glass { 0.35 } else { 1.0 };
        for (k, col) in disc_cols.iter().enumerate().take(p.discs as usize) {
            if let (Some(m), false) = (&self.disc[k], tg.disc_glass) {
                r.draw(m, &disc_m(k), glow(lit(*col, 1.0), self.hover == Hit::Disc));
            }
        }
        if tg.pa {
            if let Some(m) = &self.rim {
                for k in 0..p.discs as usize {
                    r.draw(m, &disc_m(k), Paint::lit([1.0; 4]));
                }
            }
        }
        if tg.force {
            let ay = l.disc[0] + l.b * 0.5;
            let w = (p.dp as f32 * 0.12).max(0.5) * S;
            for k in 0..p.zp as usize {
                let f = loads.pin[k];
                if f <= 0.0 {
                    continue;
                }
                let a = p.pin_angle(k);
                let start = to3([p.rp * a.cos(), p.rp * a.sin()], ay);
                let dv = loads.dir[k];
                let dir = math::norm([dv[0] as f32, 0.0, -(dv[1] as f32)]);
                let len = (f / fmax) as f32 * (p.rp as f32 * 0.45) * S;
                let side = [dir[2], 0.0, -dir[0]];
                let col = heat((f / fmax) as f32);
                let shaft = math::basis(side, dir, [0., 1., 0.], [w, len, w], math::add(start, math::scale(dir, len * 0.5)));
                r.draw(&self.cyl, &shaft, Paint::lit([col[0], col[1], col[2], 1.0]));
                let head = math::basis(side, dir, [0., 1., 0.], [w * 3.0, w * 4.0, w * 3.0], math::add(start, math::scale(dir, len + w * 2.0)));
                r.draw(&self.head, &head, Paint::lit([col[0], col[1], col[2], 1.0]));
            }
        }
        // Translucent last: discs, then housing ring.
        for (k, col) in disc_cols.iter().enumerate().take(p.discs as usize) {
            if let (Some(m), true) = (&self.disc[k], tg.disc_glass) {
                r.draw(m, &disc_m(k), glow(lit(*col, disc_a), self.hover == Hit::Disc));
            }
        }
        if tg.glass {
            r.draw(&self.cyl, &flange_m, lit([0.60, 0.63, 0.68], 0.25));
        }
        if let Some(m) = &self.housing {
            r.draw(m, &id, lit([0.48, 0.52, 0.60], if tg.glass { 0.16 } else { 1.0 }));
        }
    }
}

/// Disc solid with real bore and output holes from two `prism()` calls split at the output-pin
/// circle: bore → near hole edge, far hole edge → profile (both Rw where a ray misses the holes).
fn disc_geo(d: &Design, holes: &[[f64; 2]], rh: f64, h: f32) -> Geo {
    let p = &d.p;
    let (mut bore, mut near, mut far, mut outer) = (vec![], vec![], vec![], vec![]);
    for q in &d.prof {
        let rq = q[0].hypot(q[1]);
        let u = [q[0] / rq, q[1] / rq];
        let (mut a, mut b) = (p.rw, p.rw);
        for c in holes {
            let t = c[0] * u[0] + c[1] * u[1];
            let d2 = c[0] * c[0] + c[1] * c[1] - t * t;
            if t > 0.0 && d2 < rh * rh {
                let w = (rh * rh - d2).sqrt();
                (a, b) = (t - w, t + w);
            }
        }
        let lo = p.bore / 2.0 + 0.05;
        let hi = rq - 0.05;
        let at = |r: f64| xz([u[0] * r, u[1] * r]);
        bore.push(at(p.bore / 2.0));
        near.push(at(a.clamp(lo, hi)));
        far.push(at(b.clamp(lo, hi)));
        outer.push(xz(*q));
    }
    let mut g = gl::prism(&bore, &near, h);
    let g2 = gl::prism(&far, &outer, h);
    g.pos.extend(g2.pos);
    g.nrm.extend(g2.nrm);
    g
}

/// Ray vs vertical (y) cylinder of radius `r` around (c.x, c.z), limited to y0..y1: nearest t.
fn ray_vcyl(o: V, d: V, c: V, r: f32, y0: f32, y1: f32) -> Option<f32> {
    let (ox, oz) = (o[0] - c[0], o[2] - c[2]);
    let a = d[0] * d[0] + d[2] * d[2];
    let b = ox * d[0] + oz * d[2];
    let cc = ox * ox + oz * oz - r * r;
    let ok = |t: f32| t >= 0.0 && (y0..=y1).contains(&(o[1] + d[1] * t));
    let mut best = None;
    if a > 1e-9 {
        let disc = b * b - a * cc;
        if disc >= 0.0 {
            for t in [(-b - disc.sqrt()) / a, (-b + disc.sqrt()) / a] {
                if ok(t) {
                    best = Some(best.map_or(t, |x: f32| x.min(t)));
                }
            }
        }
    }
    // Top cap.
    if d[1].abs() > 1e-6 {
        let t = (y1 - o[1]) / d[1];
        let (x, z) = (ox + d[0] * t, oz + d[2] * t);
        if t >= 0.0 && x * x + z * z <= r * r {
            best = Some(best.map_or(t, |b: f32| b.min(t)));
        }
    }
    best
}
