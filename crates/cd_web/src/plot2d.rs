//! Small canvas-2D plots: per-pin force bars, pressure angle vs profile parameter, and a top-view
//! profile preview (also the full-screen fallback when WebGL2 is unavailable).
use cd_core::{export::hole_centres, Design, Loads};
use std::f64::consts::TAU;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d as Ctx, HtmlCanvasElement};

/// Blue → green → yellow → red for t in 0..1.
pub fn heat(t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let k = [[0.25, 0.45, 0.95], [0.20, 0.80, 0.45], [0.98, 0.82, 0.20], [0.95, 0.25, 0.20]];
    let x = t * 3.0;
    let i = (x as usize).min(2);
    let f = x - i as f32;
    std::array::from_fn(|c| k[i][c] + (k[i + 1][c] - k[i][c]) * f)
}
/// Pressure angle colour: 60° and below green, 90° red.
pub fn alpha_col(a: f64) -> [f32; 3] {
    heat(((a - 60.0) / 30.0) as f32 * 0.67 + 0.33)
}
fn css(c: [f32; 3]) -> String {
    format!("rgb({},{},{})", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)
}

pub struct Pal {
    pub bg: &'static str,
    pub grid: &'static str,
    pub text: &'static str,
    pub ink: &'static str,
    pub accent: &'static str,
    pub pin: &'static str,
}
pub const DARK: Pal = Pal { bg: "#0d1117", grid: "#2d333b", text: "#8b949e", ink: "#e6edf3", accent: "#f0883e", pin: "#9aa4b2" };
pub const LIGHT: Pal = Pal { bg: "#ffffff", grid: "#d0d7de", text: "#57606a", ink: "#1f2328", accent: "#bc4c00", pin: "#6e7781" };

/// Size the backing store to the element (device pixels) and return a context scaled to CSS px.
fn prep(c: &HtmlCanvasElement, pal: &Pal) -> Option<(Ctx, f64, f64)> {
    let dpr = web_sys::window()?.device_pixel_ratio();
    let (w, h) = (c.client_width() as f64, c.client_height() as f64);
    if w < 4.0 || h < 4.0 {
        return None;
    }
    let (pw, ph) = ((w * dpr) as u32, (h * dpr) as u32);
    if c.width() != pw || c.height() != ph {
        c.set_width(pw);
        c.set_height(ph);
    }
    let x: Ctx = c.get_context("2d").ok()??.dyn_into().ok()?;
    let _ = x.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    x.set_fill_style_str(pal.bg);
    x.fill_rect(0.0, 0.0, w, h);
    x.set_font("11px system-ui, sans-serif");
    Some((x, w, h))
}

fn axes(x: &Ctx, pal: &Pal, l: f64, t: f64, r: f64, b: f64, ymax: f64, unit: &str) {
    x.set_stroke_style_str(pal.grid);
    x.set_line_width(1.0);
    x.set_fill_style_str(pal.text);
    for i in 0..=2 {
        let y = b - (b - t) * i as f64 / 2.0;
        x.begin_path();
        x.move_to(l, y);
        x.line_to(r, y);
        x.stroke();
        let _ = x.fill_text(&format!("{:.0}{unit}", ymax * i as f64 / 2.0), 2.0, y + 4.0);
    }
}

pub fn force_plot(c: &HtmlCanvasElement, pal: &Pal, l: &Loads) {
    let Some((x, w, h)) = prep(c, pal) else { return };
    let n = l.pin.len().max(1);
    let fmax = l.max_pin();
    let ymax = nice(fmax);
    let (l0, t0, r0, b0) = (34.0, 8.0, w - 6.0, h - 18.0);
    axes(&x, pal, l0, t0, r0, b0, ymax, "");
    let bw = (r0 - l0) / n as f64;
    let imax = l.pin.iter().enumerate().fold(0, |m, (i, f)| if *f > l.pin[m] { i } else { m });
    for (i, f) in l.pin.iter().enumerate() {
        let hh = (b0 - t0) * f / ymax;
        x.set_fill_style_str(&css(if *f > 0.0 { heat((*f / fmax.max(1e-9)) as f32) } else { [0.4, 0.42, 0.46] }));
        x.fill_rect(l0 + i as f64 * bw + bw * 0.15, b0 - hh, bw * 0.7, hh.max(1.0));
    }
    x.set_fill_style_str(pal.text);
    let step = (n / 12).max(1);
    for i in (0..n).step_by(step) {
        let _ = x.fill_text(&i.to_string(), l0 + (i as f64 + 0.3) * bw, h - 5.0);
    }
    if fmax > 0.0 {
        let cx = l0 + (imax as f64 + 0.5) * bw;
        let cy = b0 - (b0 - t0) * fmax / ymax;
        dot(&x, pal.ink, cx, cy, 3.5);
        x.set_fill_style_str(pal.ink);
        let _ = x.fill_text(&format!("{fmax:.1} N"), (cx + 6.0).min(w - 50.0), cy + 10.0);
    }
}

pub fn alpha_plot(c: &HtmlCanvasElement, pal: &Pal, d: &Design, l: &Loads) {
    let Some((x, w, h)) = prep(c, pal) else { return };
    let (l0, t0, r0, b0) = (34.0, 8.0, w - 6.0, h - 18.0);
    axes(&x, pal, l0, t0, r0, b0, 90.0, "°");
    let n = d.alpha.len();
    let px = |phi: f64| l0 + (r0 - l0) * phi / TAU;
    let py = |a: f64| b0 - (b0 - t0) * a / 90.0;
    x.set_stroke_style_str(pal.accent);
    x.set_line_width(1.5);
    x.begin_path();
    for (i, a) in d.alpha.iter().enumerate() {
        let (xx, yy) = (px(TAU * i as f64 / n as f64), py(*a));
        if i == 0 { x.move_to(xx, yy) } else { x.line_to(xx, yy) }
    }
    x.stroke();
    let fmax = l.max_pin();
    for (k, f) in l.pin.iter().enumerate() {
        if *f > 0.0 {
            let phi = l.phi[k];
            dot(&x, &css(heat((*f / fmax) as f32)), px(phi), py(d.p.pressure_angle(phi)), 3.5);
        }
    }
    x.set_fill_style_str(pal.text);
    for q in 0..=4 {
        let _ = x.fill_text(&format!("{}°", q * 90), px(TAU * q as f64 / 4.0) - if q == 4 { 26.0 } else { 0.0 }, h - 5.0);
    }
}

/// Top view in the housing frame at input angle ψ (y up).
pub fn profile_plot(c: &HtmlCanvasElement, pal: &Pal, d: &Design, psi: f64, force: bool) {
    let Some((x, w, h)) = prep(c, pal) else { return };
    let p = &d.p;
    let s = (w.min(h) * 0.5 - 6.0) / (p.rp + p.dp);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let to = |q: [f64; 2]| (cx + q[0] * s, cy - q[1] * s);
    let tf = |q: [f64; 2], c: [f64; 2], r: f64| {
        let (sn, cs) = r.sin_cos();
        to([c[0] + cs * q[0] - sn * q[1], c[1] + sn * q[0] + cs * q[1]])
    };
    let path = |pts: &[[f64; 2]], c: [f64; 2], r: f64| {
        x.begin_path();
        for (i, q) in pts.iter().enumerate() {
            let (a, b) = tf(*q, c, r);
            if i == 0 { x.move_to(a, b) } else { x.line_to(a, b) }
        }
        x.close_path();
    };
    let circle = |q: (f64, f64), r: f64| {
        x.begin_path();
        let _ = x.arc(q.0, q.1, r.max(0.5), 0.0, TAU);
    };
    for disc in (0..p.discs as usize).rev() {
        let (c, r) = p.disc_pose(psi, disc);
        path(&d.prof, c, r);
        x.set_fill_style_str(if disc == 0 { "rgba(88,166,255,0.30)" } else { "rgba(240,136,62,0.18)" });
        x.fill();
        x.set_stroke_style_str(if disc == 0 { "#58a6ff" } else { pal.accent });
        x.set_line_width(1.3);
        x.stroke();
        if disc == 0 {
            let _ = x.set_line_dash(&js_sys::Array::of2(&4.0.into(), &3.0.into()));
            path(&d.theo, c, r);
            x.set_stroke_style_str(pal.text);
            x.set_line_width(1.0);
            x.stroke();
            let _ = x.set_line_dash(&js_sys::Array::new());
            for hc in hole_centres(d) {
                circle(tf(hc, c, r), d.hole_d / 2.0 * s);
                x.set_stroke_style_str(pal.ink);
                x.stroke();
            }
            circle(tf([0.0, 0.0], c, r), p.bore / 2.0 * s);
            x.stroke();
        }
    }
    let l = p.loads(psi, 0);
    let fmax = l.max_pin();
    for k in 0..p.zp as usize {
        let a = p.pin_angle(k);
        circle(to([p.rp * a.cos(), p.rp * a.sin()]), p.dp / 2.0 * s);
        x.set_fill_style_str(&if !force || fmax <= 0.0 || l.pin[k] <= 0.0 { pal.pin.to_string() } else { css(heat((l.pin[k] / fmax) as f32)) });
        x.fill();
    }
    let out = p.output_angle(psi);
    for j in 0..p.zw as usize {
        let a = out + TAU * j as f64 / p.zw.max(1) as f64;
        circle(to([p.rw * a.cos(), p.rw * a.sin()]), p.dw / 2.0 * s);
        x.set_fill_style_str(pal.ink);
        x.fill();
    }
    dot(&x, pal.accent, cx + p.e * psi.cos() * s, cy - p.e * psi.sin() * s, 3.0);
}

fn dot(x: &Ctx, col: &str, cx: f64, cy: f64, r: f64) {
    x.set_fill_style_str(col);
    x.begin_path();
    let _ = x.arc(cx, cy, r, 0.0, TAU);
    x.fill();
}

/// Round up to 1 / 2 / 5 × 10ⁿ.
fn nice(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let m = 10f64.powf(v.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|k| k * m).find(|c| *c >= v).unwrap_or(10.0 * m)
}
