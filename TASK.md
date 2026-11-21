# Task: cycloidal reducer designer for robot joints (Rust → WebAssembly, static web app)

Original open-source tool. Write all code yourself; do not copy anyone's code, tables or assets.
Use textbook methods only (epitrochoid / cycloid disc profile from the parametric equations with pin
radius offset, shortening coefficient K1 = e·Zp/Rp, curvature / undercut condition, pressure angle
along the profile, simple Hertz line-contact stress between pin and disc, eccentric-bearing load from
torque, ratio = Zd/(Zp − Zd) with Zp = Zd + 1). All material values are generic and labelled "typical".

## Product (SMALL scope — core + UI about 800-1200 lines of new Rust; renderer/math are reused)
A single-page web app (GitHub Pages, sub-path `/cycloid-drive/`): "I want a single-stage cycloidal
reducer for my robot joint (3D-printed or machined) — which pin count, pin circle, eccentricity and
pin diameter give a valid, non-undercut profile, what are the contact stresses and bearing loads at my
peak torque, and can I export the disc profile to CAD?"

1. **Inputs**: pin count Zp (e.g. 8-60; disc lobes Zd = Zp − 1); pin circle radius Rp (mm);
   eccentricity e (mm); ring pin diameter dp (mm); disc thickness b (mm); number of discs (1 or 2,
   180° phased); output torque peak (N·m); input speed (rpm); output-pin count and pin circle
   (for the output-hole geometry: hole Ø = output pin Ø + 2e); material preset "PLA/PETG print
   (typical)", "Nylon/PA-CF print (typical)", "Steel hardened (typical)" with E, ν and allowable
   contact stress.
2. **Model (core)**: profile points (x,y) for N samples from the cycloid parametric equations with
   pin-radius offset; ratio; K1 and checks: K1 < 1, minimum radius of curvature of the theoretical
   curve > pin radius (no undercut / no cusps), pins don't overlap each other (pin pitch > dp + gap),
   output holes don't intersect each other or the disc edge; pressure angle distribution; idealised
   load sharing: about half the pins in contact, force per pin Fi ∝ sin(φi) normalised so ΣFi·li = T
   (simple, documented model); max pin force; Hertz line-contact stress with the local curvature of
   the disc profile; eccentric-bearing radial load; output-pin load; disc speed and wobble frequency.
   Status per check (OK / warning / fail) and an overall status. Profile perimeter and min wall.
3. **Extras (small)**: (a) "find designs" search: for a target ratio and a max outer diameter, search
   Zp, e (0.5..3 mm step 0.25), dp (from a small list) that pass every check, list top 12 sorted by
   (passes, min stress margin, OD); click a row to load. (b) Export the disc profile as an SVG and a
   DXF (LWPOLYLINE, mm) generated in the browser via a Blob download (no network).
4. **3D view (WebGL2, game-grade — see below)**: real 3D reducer: ring of housing pins (cylinders),
   the cycloid disc(s) extruded with `prism()` from the computed profile (translucent option), the
   eccentric input shaft, output pins through the disc holes, output flange, simple housing. Animated:
   input shaft spins, disc orbits with e and counter-rotates at the correct ratio, output rotates;
   play/pause, slow motion (default ~0.25×), time slider. Toggles: contact force colouring (pins coloured
   by load, force arrows on loaded pins), pressure-angle colouring along the disc edge, translucent
   housing, exploded view.
5. **2D plots** (canvas 2D, own small module): per-pin force vs pin index at the current instant, and
   pressure angle vs profile parameter, each with the current point marked; plus a 2D top-view profile
   preview (theoretical vs offset curve) used as the WebGL2 fallback.
6. Korean + English toggle (browser default). Mobile friendly. CSS prefers-color-scheme only.
Out of scope (do NOT build): tolerance/backlash modification (offset/equidistant modification), fatigue
life, lubrication, thermal, multi-stage, URL share, JSON import, STL export.

## 3D view requirements (game-like, reuse the existing renderer)
- `crates/cd_web/src/gl.rs` and `crates/cd_web/src/math.rs` already exist: a tool-independent WebGL2
  renderer (lit meshes, checker/grid ground with fog, sky gradient, stencil planar shadows, translucent
  volumes, lines, `prism()` extrusion) and pure math (mat4, damped orbit camera with inertia,
  ray-plane/ray-sphere picking) with unit tests. Reuse them; extend only if needed and keep them
  tool-independent and tested.
- Scene code in `view3d.rs`. Orbit (left drag) / pan (right drag, two fingers) / zoom (wheel, pinch)
  with inertia, double-click reset, `touch-action:none`. View presets: Perspective / Front (axial) /
  Side / Exploded.
- Direct manipulation: dragging a housing pin radially changes Rp; dragging the eccentric shaft
  marker changes e (snaps to 0.05 mm, updates inputs live and re-extrudes the disc); dragging the disc
  rim scrubs the animation angle. Hover highlight + cursor change.
- Toggle buttons over the view: force colouring/arrows, pressure-angle colouring, translucent housing,
  play/pause.
- HUD over the canvas: ratio, OD, K1, min curvature radius vs pin radius, max pressure angle,
  max pin force N, max contact stress MPa vs allowable, bearing load N, output-hole clearance,
  overall status.
- If WebGL2 is unavailable, show a message and keep the numeric readouts and 2D plots working.
- Release wasm ≲ 350 KB. No crates besides wasm-bindgen, web-sys, js-sys.

## Core crate (pure Rust, no web deps, unit tested) — `crates/cd_core`
- Tests (hand-checkable, 6-10): ratio for Zp=12 is 11:1; profile point at parameter 0 lies at
  radius Rp − e − rp (or the documented equivalent); profile is closed and has Zd lobes (count local
  maxima of radius); K1 ≥ 1 flagged as fail; small curvature radius < pin radius flagged as undercut;
  sum of pin moments equals the input torque within 1e-6; Hertz stress scales with sqrt(F/b);
  DXF output starts with the section header and contains N vertices; search results all pass and are
  sorted.
- `crates/cd_web`: UI, plain web-sys. The existing math.rs/gl.rs tests must keep passing.

## Repo hygiene
- README (Korean first, then English): what it is, model and assumptions (rigid parts, idealised load
  sharing, no profile modification/backlash, generic typical values), controls, local build
  (`cargo test`, `trunk build --release`), MIT license, "educational estimates — validate with real
  parts and tests before trusting a robot with it".
- LICENSE: MIT, copyright "cycloid-drive contributors" (exists). `.gitignore` for target/ dist/ (exists).
- `index.html` with a CSP meta that blocks external requests:
  `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; base-uri 'self'`.
  `Trunk.toml` exists (`filehash = false`, `public_url = "/cycloid-drive/"`, external `web/boot.js`
  copied via `<link data-trunk rel="copy-file" href="web/boot.js">`, which imports `./cd_web.js`).
- Root `Cargo.toml` workspace with members `crates/cd_core`, `crates/cd_web` (exists).
- `.github/workflows/pages.yml` exists (trunk 0.22.1, privacy check, cargo test, deploy-pages). Keep it.
- `tools/privacy_check.sh` already exists; keep it and keep it clean.
- Polished app UI: dark design tokens (CSS variables, system fonts), full-viewport 3D canvas as hero,
  slim floating toolbar with tooltips, collapsible right-side settings cards (bottom sheet on mobile),
  example loaded on first run with a dismissible hint.

## Done when
- `cargo test` green (workspace); `trunk build --release` succeeds with no warnings;
  `sh tools/privacy_check.sh` clean.
- Commit author `cycloid-drive contributors <59640857+roscore@users.noreply.github.com>`.
- Commit everything with clear commits; open a PR. Do NOT create new GitHub repos or change repo settings.
