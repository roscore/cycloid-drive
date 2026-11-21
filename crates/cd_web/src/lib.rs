//! Web UI for cycloid-drive: wires the HTML controls, HUD, plots, search, export and the 3D view.
pub mod gl;
pub mod i18n;
pub mod math;
pub mod plot2d;
pub mod view3d;

use cd_core::{export, search::{search, Candidate}, CheckId, Design, Params, Status, MATERIALS};
use i18n::tr;
use std::cell::RefCell;
use std::f64::consts::TAU;
use std::rc::Rc;
use view3d::{from3, Hit, Toggles, View3d, S};
use wasm_bindgen::{convert::FromWasmAbi, prelude::*, JsCast};
use web_sys::{Document, Element, Event, HtmlCanvasElement, HtmlElement, HtmlInputElement, HtmlSelectElement, PointerEvent, WheelEvent};

const NUM: [&str; 11] = ["zp", "rp", "e", "dp", "b", "bore", "torque", "rpm", "zw", "dw", "rw"];

#[derive(Clone, Copy)]
enum Drag {
    Orbit,
    Pan,
    Pin,
    Ecc,
    Disc(f64),
}

struct App {
    p: Params,
    d: Design,
    ko: bool,
    dark: bool,
    view: Option<View3d>,
    psi: f64,
    playing: bool,
    speed: f64,
    last: f64,
    tg: Toggles,
    preset: usize,
    ptrs: Vec<(i32, f64, f64)>,
    drag: Option<Drag>,
    dirty: bool,
    redraw: bool,
    results: Vec<Candidate>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

fn doc() -> Document {
    web_sys::window().and_then(|w| w.document()).expect("document")
}
fn el(id: &str) -> Option<Element> {
    doc().get_element_by_id(id)
}
fn input(id: &str) -> Option<HtmlInputElement> {
    el(id)?.dyn_into().ok()
}
fn select(id: &str) -> Option<HtmlSelectElement> {
    el(id)?.dyn_into().ok()
}
fn set_html(id: &str, html: &str) {
    if let Some(e) = el(id) {
        e.set_inner_html(html);
    }
}
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
fn on<E: FromWasmAbi + 'static>(target: &web_sys::EventTarget, ev: &str, f: impl FnMut(E) + 'static) {
    let c = Closure::wrap(Box::new(f) as Box<dyn FnMut(E)>);
    let _ = target.add_event_listener_with_callback(ev, c.as_ref().unchecked_ref());
    c.forget();
}
fn on_id<E: FromWasmAbi + 'static>(id: &str, ev: &str, f: impl FnMut(E) + 'static) {
    if let Some(e) = el(id) {
        on(&e, ev, f);
    }
}
fn now() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}
/// Number without trailing zeros (for input fields).
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
fn cls(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::Warn => "warn",
        Status::Fail => "fail",
    }
}

impl App {
    fn read_params(&self) -> Params {
        let mut p = self.p;
        let v = |id: &str, old: f64, lo: f64, hi: f64| {
            let x = input(id).map(|i| i.value_as_number()).unwrap_or(f64::NAN);
            if x.is_finite() { x.clamp(lo, hi) } else { old }
        };
        p.zp = v("zp", p.zp as f64, 6.0, 80.0).round() as u32;
        p.rp = v("rp", p.rp, 5.0, 1000.0);
        p.e = v("e", p.e, 0.01, 50.0);
        p.dp = v("dp", p.dp, 0.5, 100.0);
        p.b = v("b", p.b, 0.5, 200.0);
        p.bore = v("bore", p.bore, 1.0, 1000.0);
        p.torque = v("torque", p.torque, 0.0, 1e5);
        p.rpm = v("rpm", p.rpm, 0.0, 1e5);
        p.zw = v("zw", p.zw as f64, 3.0, 36.0).round() as u32;
        p.dw = v("dw", p.dw, 0.5, 100.0);
        p.rw = v("rw", p.rw, 1.0, 1000.0);
        p.discs = select("discs").and_then(|s| s.value().parse().ok()).unwrap_or(p.discs).clamp(1, 2);
        p.material = select("mat").and_then(|s| s.value().parse().ok()).unwrap_or(p.material).min(2);
        p
    }

    fn write_params(&self) {
        let p = &self.p;
        let vals = [p.zp as f64, p.rp, p.e, p.dp, p.b, p.bore, p.torque, p.rpm, p.zw as f64, p.dw, p.rw];
        for (id, v) in NUM.iter().zip(vals) {
            if let Some(i) = input(id) {
                i.set_value(&num(v));
            }
        }
        if let Some(s) = select("discs") {
            s.set_value(&p.discs.to_string());
        }
        if let Some(s) = select("mat") {
            s.set_value(&p.material.to_string());
        }
    }

    fn apply(&mut self, p: Params) {
        self.p = p;
        self.d = p.eval();
        if let Some(v) = self.view.as_mut() {
            v.rebuild(&self.d);
        }
        self.dirty = true;
        self.text();
    }

    /// HUD, checks list and captions.
    fn text(&self) {
        let (d, ko) = (&self.d, self.ko);
        let st = |id: CheckId| d.checks.iter().find(|c| c.id == id).map(|c| cls(c.status)).unwrap_or("");
        let allow = MATERIALS[d.p.material].allow;
        let rows: [(&str, String, &str); 9] = [
            ("od", format!("{:.0} mm", d.od), ""),
            ("k1", format!("{:.3}", d.k1), st(CheckId::K1)),
            ("curv", format!("{:.2} / {:.2} mm", d.rho_min, d.p.dp / 2.0), st(CheckId::Undercut)),
            ("pa", format!("{:.1}°", d.alpha_work), st(CheckId::PressureAngle)),
            ("fmax", format!("{:.1} N", d.f_max), ""),
            ("sigma", format!("{:.1} / {:.0} MPa", d.sigma_max, allow), st(CheckId::Stress)),
            ("bearing", format!("{:.1} N", d.bearing_max), ""),
            ("wall", format!("{:.2} mm", d.min_wall), if d.min_wall < 0.0 { "fail" } else { st(CheckId::HoleEdge).max(st(CheckId::HoleSpacing)) }),
            ("out_load", format!("{:.1} N", d.out_max), ""),
        ];
        let mut h = format!(
            "<div class=\"big\"><span>{} {}:1</span><span class=\"{}\">{}</span></div>",
            tr("ratio", ko),
            d.zd,
            cls(d.status),
            tr(cls(d.status), ko)
        );
        for (k, v, c) in rows {
            h += &format!("<span class=\"k\">{}</span><span class=\"v {c}\">{v}</span>", tr(k, ko));
        }
        set_html("hud", &h);

        let mut ck = String::new();
        for (i, c) in d.checks.iter().enumerate() {
            let v = match c.id {
                CheckId::K1 => format!("{:.3} &lt; 1", c.value),
                CheckId::Undercut => format!("{:.2} &gt; {:.2} mm", c.value, c.limit),
                CheckId::PinSpacing | CheckId::HoleSpacing | CheckId::HoleEdge => format!("{:.2} mm", c.value),
                CheckId::Stress => format!("{:.0} / {:.0} MPa", c.value, c.limit),
                CheckId::PressureAngle => format!("{:.1}°", c.value),
            };
            ck += &format!("<li><span>{}</span><span class=\"{}\">{v} · {}</span></li>", tr(&format!("ck{i}"), ko), cls(c.status), tr(cls(c.status), ko));
        }
        set_html("checks", &ck);
        set_html(
            "extra",
            &format!(
                "{} {:.1} mm · {} {:.1} rpm · {} {:.2} Hz",
                tr("perimeter", ko),
                d.perimeter,
                tr("speeds", ko),
                d.out_rpm,
                tr("wobble", ko),
                d.wobble_hz
            ),
        );
        set_html("holecap", &format!("{}: {:.2} mm", tr("hole_d", ko), d.hole_d));
    }

    fn lang(&self) {
        let ko = self.ko;
        let d = doc();
        if let Some(h) = d.document_element() {
            let _ = h.set_attribute("lang", if ko { "ko" } else { "en" });
        }
        for (attr, tip) in [("data-i18n", false), ("data-i18n-tip", true)] {
            if let Ok(list) = d.query_selector_all(&format!("[{attr}]")) {
                for i in 0..list.length() {
                    let Some(e) = list.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else { continue };
                    let key = e.get_attribute(attr).unwrap_or_default();
                    if tip {
                        let _ = e.set_attribute("data-tip", tr(&key, ko));
                        let _ = e.set_attribute("aria-label", tr(&key, ko));
                    } else {
                        e.set_text_content(Some(tr(&key, ko)));
                    }
                }
            }
        }
        if let Some(b) = el("t_lang") {
            b.set_text_content(Some(if ko { "EN" } else { "KO" }));
        }
        self.text();
        self.rows();
    }

    fn rows(&self) {
        let mut h = String::new();
        for (i, c) in self.results.iter().enumerate() {
            let p = &c.params;
            h += &format!(
                "<tr data-i=\"{i}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"{}\">{:.0}%</td><td>{:.0}</td></tr>",
                p.zp,
                num(p.rp),
                num(p.e),
                num(p.dp),
                cls(c.status),
                c.margin * 100.0,
                c.od
            );
        }
        set_html("s_rows", &h);
    }

    fn plots(&self) {
        let pal = if self.dark { &plot2d::DARK } else { &plot2d::LIGHT };
        let l = self.p.loads(self.psi, 0);
        let canvas = |id: &str| el(id).and_then(|e| e.dyn_into::<HtmlCanvasElement>().ok());
        if let Some(c) = canvas("p_force") {
            plot2d::force_plot(&c, pal, &l);
        }
        if let Some(c) = canvas("p_pa") {
            plot2d::alpha_plot(&c, pal, &self.d, &l);
        }
        if let Some(c) = canvas("p_prof") {
            plot2d::profile_plot(&c, pal, &self.d, self.psi, self.tg.force);
        }
        if self.view.is_none() {
            if let Some(c) = canvas("flat") {
                plot2d::profile_plot(&c, pal, &self.d, self.psi, self.tg.force);
            }
        }
    }

    fn set_play(&mut self, on: bool) {
        self.playing = on;
        if let Some(b) = el("t_play") {
            b.set_text_content(Some(if on { "❚❚" } else { "▶" }));
            let _ = b.class_list().toggle_with_force("on", on);
        }
    }

    fn tick(&mut self) {
        let t = now();
        let dt = ((t - self.last) / 1000.0).clamp(0.0, 0.1);
        self.last = t;
        if self.playing {
            let rpm = if self.p.rpm > 0.0 { self.p.rpm } else { 60.0 };
            self.psi += TAU * rpm / 60.0 * self.speed * dt;
            self.dirty = true;
        }
        let period = TAU * self.d.zd as f64;
        if self.dirty {
            if let Some(i) = input("scrub") {
                i.set_value(&format!("{:.4}", self.psi.rem_euclid(period) / period));
            }
        }
        // Render only while something changes (animation, camera glide / inertia, edits).
        let mut moving = false;
        if let Some(v) = self.view.as_mut() {
            if self.dirty || self.redraw {
                moving = v.frame(&self.d, self.psi, self.tg, dt as f32);
            }
        }
        if self.dirty {
            self.plots();
        }
        self.redraw = moving;
        self.dirty = false;
    }

    // ----- pointer input -----
    fn pointer_down(&mut self, e: &PointerEvent) {
        let (x, y) = (e.offset_x() as f64, e.offset_y() as f64);
        self.ptrs.retain(|p| p.0 != e.pointer_id());
        self.ptrs.push((e.pointer_id(), x, y));
        let Some(v) = self.view.as_mut() else { return };
        v.cam.held = true;
        if self.ptrs.len() >= 2 {
            self.drag = None;
            return;
        }
        self.drag = Some(if e.button() == 2 || e.shift_key() {
            Drag::Pan
        } else {
            match v.pick(&self.d, self.psi, x, y) {
                Hit::Pin(_) => Drag::Pin,
                Hit::Ecc => Drag::Ecc,
                Hit::Disc => Drag::Disc(self.disc_angle(x, y).unwrap_or(0.0)),
                Hit::None => Drag::Orbit,
            }
        });
        if matches!(self.drag, Some(Drag::Disc(_))) {
            self.set_play(false);
        }
        if matches!(self.drag, Some(Drag::Pin | Drag::Ecc | Drag::Disc(_))) {
            self.cursor("grabbing");
        }
    }

    fn disc_angle(&self, x: f64, y: f64) -> Option<f64> {
        let v = self.view.as_ref()?;
        let l = v.layout(&self.d);
        let h = v.plane_hit(x, y, l.disc[self.p.discs as usize - 1] + l.b)?;
        let q = from3(h);
        Some(q[1].atan2(q[0]))
    }

    fn cursor(&self, c: &str) {
        if let Some(e) = el("view").and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
            let _ = e.style().set_property("cursor", c);
        }
    }

    fn pointer_move(&mut self, e: &PointerEvent) {
        let (x, y) = (e.offset_x() as f64, e.offset_y() as f64);
        let Some(i) = self.ptrs.iter().position(|p| p.0 == e.pointer_id()) else {
            // Hover.
            let Some(v) = self.view.as_mut() else { return };
            let h = v.pick(&self.d, self.psi, x, y);
            if h != v.hover {
                v.hover = h;
                self.dirty = true;
                self.cursor(match h {
                    Hit::Pin(_) | Hit::Disc => "grab",
                    Hit::Ecc => "move",
                    Hit::None => "default",
                });
            }
            return;
        };
        let (_, ox, oy) = self.ptrs[i];
        let (dx, dy) = (x - ox, y - oy);
        if self.ptrs.len() >= 2 {
            let other = self.ptrs[1 - i.min(1)];
            let (d0, d1) = ((ox - other.1).hypot(oy - other.2), (x - other.1).hypot(y - other.2));
            self.ptrs[i] = (e.pointer_id(), x, y);
            if let Some(v) = self.view.as_mut() {
                if d1 > 1.0 {
                    v.cam.zoom((d0 / d1) as f32);
                }
                pan(v, dx * 0.5, dy * 0.5);
            }
            self.dirty = true;
            return;
        }
        self.ptrs[i] = (e.pointer_id(), x, y);
        let Some(drag) = self.drag else { return };
        self.dirty = true;
        match drag {
            Drag::Orbit => {
                if let Some(v) = self.view.as_mut() {
                    v.cam.rotate(-dx as f32 * 0.008, dy as f32 * 0.008);
                }
            }
            Drag::Pan => {
                if let Some(v) = self.view.as_mut() {
                    pan(v, dx, dy);
                }
            }
            Drag::Pin => {
                let Some(v) = self.view.as_ref() else { return };
                let l = v.layout(&self.d);
                if let Some(h) = v.plane_hit(x, y, l.stack_top + S) {
                    let q = from3(h);
                    let rp = ((q[0].hypot(q[1]) * 2.0).round() / 2.0).clamp(5.0, 1000.0);
                    if rp != self.p.rp {
                        let p = Params { rp, ..self.p };
                        self.apply(p);
                        self.write_params();
                    }
                }
            }
            Drag::Ecc => {
                let Some(v) = self.view.as_ref() else { return };
                let m = v.marker_pos(&self.d, self.psi);
                if let Some(h) = v.plane_hit(x, y, m[1]) {
                    let q = from3(h);
                    let lim = (self.p.rp / self.p.zp as f64 * 0.999 * 20.0).floor() / 20.0;
                    let ecc = ((q[0].hypot(q[1]) * 20.0).round() / 20.0).clamp(0.05, lim.max(0.05));
                    if ecc != self.p.e {
                        let p = Params { e: ecc, ..self.p };
                        self.apply(p);
                        self.write_params();
                    }
                }
            }
            Drag::Disc(last) => {
                if let Some(a) = self.disc_angle(x, y) {
                    let da = (a - last + TAU * 1.5).rem_euclid(TAU) - TAU * 0.5;
                    self.psi -= da * self.d.zd as f64;
                    self.drag = Some(Drag::Disc(a));
                }
            }
        }
    }

    fn pointer_up(&mut self, e: &PointerEvent) {
        self.ptrs.retain(|p| p.0 != e.pointer_id());
        if self.ptrs.is_empty() {
            self.drag = None;
            if let Some(v) = self.view.as_mut() {
                v.cam.held = false;
                let h = v.hover;
                self.cursor(if h == Hit::None { "default" } else { "grab" });
            }
        } else {
            self.drag = Some(Drag::Orbit);
        }
    }
}

fn pan(v: &mut View3d, dx: f64, dy: f64) {
    let o = v.cam.cur;
    let f = math::norm(math::sub(o.target, o.eye()));
    let right = math::norm(math::cross(f, [0.0, 1.0, 0.0]));
    let up = math::cross(right, f);
    let k = o.dist * 0.0016;
    v.cam.pan(math::add(math::scale(right, -dx as f32 * k), math::scale(up, dy as f32 * k)));
}

fn download(name: &str, mime: &str, text: &str) -> Option<()> {
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts).ok()?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).ok()?;
    let a: web_sys::HtmlAnchorElement = doc().create_element("a").ok()?.dyn_into().ok()?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Some(())
}

fn toggle(id: &'static str, f: fn(&mut Toggles) -> &mut bool) {
    on_id(id, "click", move |_: Event| {
        with(|a| {
            let b = f(&mut a.tg);
            *b = !*b;
            let on = *b;
            if let Some(e) = el(id) {
                let _ = e.class_list().toggle_with_force("on", on);
            }
            a.dirty = true;
        });
    });
}

#[wasm_bindgen(start)]
pub fn start() {
    let win = web_sys::window().expect("window");
    let dark = win.match_media("(prefers-color-scheme: light)").ok().flatten().is_none_or(|m| !m.matches());
    let ko = storage()
        .and_then(|s| s.get_item("cd.lang").ok().flatten())
        .map(|l| l == "ko")
        .unwrap_or_else(|| win.navigator().language().unwrap_or_default().to_lowercase().starts_with("ko"));
    let canvas: HtmlCanvasElement = el("view").and_then(|e| e.dyn_into().ok()).expect("canvas");
    let p = Params::default();
    let d = p.eval();
    let mut view = View3d::new(canvas.clone(), dark);
    if let Some(v) = view.as_mut() {
        v.rebuild(&d);
        let o = View3d::preset(&d, 0);
        v.cam = math::OrbitCam::new(math::Orbit { dist: o.dist * 1.6, yaw: o.yaw - 0.8, ..o });
        v.cam.set_goal(o);
    } else {
        canvas.set_hidden(true);
        for id in ["nogl", "flat"] {
            if let Some(e) = el(id).and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
                e.set_hidden(false);
            }
        }
    }
    let app = App {
        p,
        d,
        ko,
        dark,
        view,
        psi: 0.0,
        playing: true,
        speed: 0.25,
        last: now(),
        tg: Toggles { force: true, pa: false, glass: true, disc_glass: false, explode: false },
        preset: 0,
        ptrs: vec![],
        drag: None,
        dirty: true,
        redraw: true,
        results: vec![],
    };
    app.write_params();
    app.lang();
    APP.with(|a| *a.borrow_mut() = Some(app));

    on_id("hud", "click", |_: Event| {
        if let Some(h) = el("hud") {
            let _ = h.class_list().toggle("open");
        }
    });
    // The hint explains 3D manipulation, so it only shows with a working 3D view.
    let has_view = with(|a| a.view.is_some()).unwrap_or(false);
    if has_view && storage().and_then(|s| s.get_item("cd.hint").ok().flatten()).is_none() {
        if let Some(h) = el("hint").and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
            h.set_hidden(false);
        }
    }
    on_id("hint_ok", "click", |_: Event| {
        if let Some(h) = el("hint").and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
            h.set_hidden(true);
        }
        if let Some(s) = storage() {
            let _ = s.set_item("cd.hint", "1");
        }
    });

    for id in NUM.iter().chain(&["discs", "mat"]) {
        on_id(id, "input", |_: Event| {
            with(|a| {
                let p = a.read_params();
                a.apply(p);
            });
        });
    }
    on_id("t_play", "click", |_: Event| {
        with(|a| a.set_play(!a.playing));
    });
    toggle("t_force", |t| &mut t.force);
    toggle("t_pa", |t| &mut t.pa);
    toggle("t_glass", |t| &mut t.glass);
    toggle("t_disc", |t| &mut t.disc_glass);
    toggle("t_explode", |t| &mut t.explode);
    on_id("t_lang", "click", |_: Event| {
        with(|a| {
            a.ko = !a.ko;
            if let Some(s) = storage() {
                let _ = s.set_item("cd.lang", if a.ko { "ko" } else { "en" });
            }
            a.lang();
        });
    });
    on_id("viewsel", "change", |_: Event| {
        with(|a| {
            let i: usize = select("viewsel").and_then(|s| s.value().parse().ok()).unwrap_or(0);
            a.preset = i;
            a.tg.explode = i == 3;
            if let Some(e) = el("t_explode") {
                let _ = e.class_list().toggle_with_force("on", i == 3);
            }
            if let Some(v) = a.view.as_mut() {
                v.cam.set_goal(View3d::preset(&a.d, i));
            }
            a.dirty = true;
        });
    });
    on_id("speed", "change", |_: Event| {
        with(|a| a.speed = select("speed").and_then(|s| s.value().parse().ok()).unwrap_or(0.25));
    });
    on_id("scrub", "input", |_: Event| {
        with(|a| {
            a.set_play(false);
            let v = input("scrub").map(|i| i.value_as_number()).unwrap_or(0.0);
            a.psi = v * TAU * a.d.zd as f64;
            a.dirty = true;
        });
    });
    on_id("s_go", "click", |_: Event| {
        with(|a| {
            let ratio = input("s_ratio").map(|i| i.value_as_number()).filter(|v| v.is_finite()).unwrap_or(11.0).clamp(5.0, 79.0) as u32;
            let od = input("s_od").map(|i| i.value_as_number()).filter(|v| v.is_finite()).unwrap_or(100.0).clamp(20.0, 2000.0);
            a.results = search(&a.p, ratio, od, 12);
            let msg = if a.results.is_empty() { "s_none" } else { "s_hint" };
            if let Some(e) = el("s_msg") {
                let _ = e.set_attribute("data-i18n", msg);
                e.set_text_content(Some(tr(msg, a.ko)));
            }
            a.rows();
        });
    });
    on_id("s_rows", "click", |e: Event| {
        let Some(row) = e.target().and_then(|t| t.dyn_into::<Element>().ok()).and_then(|t| t.closest("tr").ok().flatten()) else { return };
        let Some(i) = row.get_attribute("data-i").and_then(|s| s.parse::<usize>().ok()) else { return };
        with(|a| {
            if let Some(c) = a.results.get(i) {
                let p = c.params;
                a.apply(p);
                a.write_params();
                if let Some(v) = a.view.as_mut() {
                    v.cam.set_goal(View3d::preset(&a.d, a.preset));
                }
            }
        });
    });
    on_id("x_svg", "click", |_: Event| {
        with(|a| download(&format!("cycloid-disc-zp{}.svg", a.p.zp), "image/svg+xml", &export::svg(&a.d)));
    });
    on_id("x_dxf", "click", |_: Event| {
        with(|a| download(&format!("cycloid-disc-zp{}.dxf", a.p.zp), "application/dxf", &export::dxf(&a.d)));
    });

    on(&canvas, "pointerdown", |e: PointerEvent| {
        if let Some(t) = e.target().and_then(|t| t.dyn_into::<Element>().ok()) {
            let _ = t.set_pointer_capture(e.pointer_id());
        }
        with(|a| a.pointer_down(&e));
    });
    on(&canvas, "pointermove", |e: PointerEvent| {
        with(|a| a.pointer_move(&e));
    });
    for ev in ["pointerup", "pointercancel"] {
        on(&canvas, ev, |e: PointerEvent| {
            with(|a| a.pointer_up(&e));
        });
    }
    on(&canvas, "wheel", |e: WheelEvent| {
        e.prevent_default();
        with(|a| {
            if let Some(v) = a.view.as_mut() {
                v.cam.zoom((e.delta_y() * 0.0012).exp() as f32);
            }
            a.redraw = true;
        });
    });
    on(&canvas, "dblclick", |_: Event| {
        with(|a| {
            if let Some(v) = a.view.as_mut() {
                v.cam.set_goal(View3d::preset(&a.d, a.preset));
            }
            a.redraw = true;
        });
    });
    on(&canvas, "contextmenu", |e: Event| e.prevent_default());
    on(&win, "resize", |_: Event| {
        with(|a| a.dirty = true);
    });

    // Animation loop.
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        with(|a| a.tick());
        if let (Some(w), Some(c)) = (web_sys::window(), f.borrow().as_ref()) {
            let _ = w.request_animation_frame(c.as_ref().unchecked_ref());
        }
    }) as Box<dyn FnMut()>));
    if let Some(c) = g.borrow().as_ref() {
        let _ = win.request_animation_frame(c.as_ref().unchecked_ref());
    };
}
