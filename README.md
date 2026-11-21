# cycloid-drive

로봇 관절용 1단 사이클로이드 감속기 설계 도구 — Rust → WebAssembly 정적 웹 앱 (WebGL2 3D 뷰).

*A single-stage cycloidal reducer designer for robot joints — Rust → WebAssembly static web app with a WebGL2 3D view. English below.*

> **교육용 추정치입니다.** 로봇에 맡기기 전에 실제 부품과 시험으로 검증하세요.

## 한국어

### 무엇을 하나요
"로봇 관절에 쓸 1단 사이클로이드 감속기(3D 프린팅 또는 가공)를 만들고 싶다 — 핀 수, 핀 원 반경, 편심량, 핀 지름을
어떻게 잡아야 언더컷 없는 유효한 프로파일이 나오고, 최대 토크에서 접촉 응력과 베어링 하중은 얼마이며, 디스크
프로파일을 CAD로 내보낼 수 있을까?"에 답합니다.

- **입력**: 링 핀 수 Zp (로브 Zd = Zp − 1), 핀 원 반경 Rp, 편심량 e, 링 핀 지름 dp, 디스크 두께 b, 디스크 수(1 또는 2장, 180° 위상),
  최대 출력 토크, 입력 회전수, 출력 핀 수·지름·핀 원 반경, 편심 베어링 구멍 지름, 재료 프리셋(일반값).
- **결과**: 감속비, K1, 최소 곡률반경, 압력각 분포, 핀별 하중, 헤르츠 접촉 응력, 편심 베어링 하중, 출력 핀 하중,
  출력 구멍 여유, 프로파일 둘레, 회전수·요동 주파수 — 각 검토 항목별 통과/주의/불가와 종합 상태.
- **설계 탐색**: 목표 감속비와 최대 외경으로 e(0.5–3 mm, 0.25 간격), dp(목록), Rp(1 mm 간격)를 훑어 모든 검토를
  통과하는 상위 12개를 보여줍니다. 행을 누르면 불러옵니다.
- **내보내기**: 디스크 프로파일 SVG / DXF(LWPOLYLINE, mm, 보어·출력 구멍 원 포함). 브라우저 안에서 Blob으로 생성 — 네트워크 없음.
- **3D 뷰**: 하우징 핀, 압출된 사이클로이드 디스크(실제 보어·출력 구멍), 편심 입력축, 출력 핀과 플랜지, 하우징.
  애니메이션(재생/일시정지, 슬로모션 기본 0.25×, 시간 슬라이더), 하중 색상·화살표, 압력각 색상, 반투명, 분해도.
- **2D 그래프**: 현재 순간의 핀별 하중, 프로파일 파라미터별 압력각, 상면도(이론 곡선 vs 오프셋 곡선).
  WebGL2가 없으면 상면도가 3D 뷰를 대신합니다.

### 모델과 가정
- 이론 곡선(디스크 좌표계): C(φ) = Rp(cos φ, sin φ) − e(cos Zp·φ, sin Zp·φ). 디스크 프로파일은 이를 핀 반경 rp만큼
  안쪽 법선 방향으로 오프셋한 곡선이며, φ = 0에서 반경 Rp − e − rp (핀이 앉는 골).
- 감속비 = Zd / (Zp − Zd), Zp = Zd + 1 (출력은 입력과 반대 방향). 단축계수 K1 = e·Zp / Rp < 1.
- 언더컷/첨점: 이론 곡선의 볼록부 최소 곡률반경 ρmin > rp.
- 압력각: 접촉 법선과, 디스크가 자기 중심으로 돌 때 접촉점의 운동 방향 사이 각 (모멘트 암 = |C|·cos α).
  HUD의 최대값은 최대 하중의 50 % 이상을 받는 핀(작동 영역)에서 측정합니다.
- 하중 분담(이상화): 편심 방향 기준 각 θ에서 sin θ < 0인 절반의 핀이 Fi = F0·|sin θi|를 받고, 하중선은 피치점을
  지납니다. F0는 Σ Fi·li = T(디스크당)가 되도록 정합니다. 출력 핀은 편심 방향으로 구멍 벽에 닿고 qj = Q0·|sin βj|,
  Σ qj·Rw·|sin βj| = T. 편심 베어링은 두 하중의 합력을 받습니다. 2장일 때 토크는 반씩 나눕니다.
- 헤르츠 선접촉: σ = √(F·E* / (π·b·R*)), 1/R* = 1/rp + 1/ρ디스크 (오목부는 음수 곡률). 핀은 강철(일반값)로 가정.
- 하우징 외경 추정 = 2·(Rp + dp). 출력 구멍 지름 = 출력 핀 지름 + 2e.
- 강체, 이상화된 하중 분담, 프로파일 수정·백래시 없음, 재료값은 모두 "일반값(typical)"입니다.
  피로 수명, 윤활, 열, 다단, 공차 해석은 다루지 않습니다.

### 조작
- 왼쪽 드래그: 회전 · 오른쪽 드래그/Shift+드래그/두 손가락: 이동 · 휠/핀치: 확대 · 더블클릭: 시점 초기화
- 링 핀 드래그 → Rp 변경, 주황 마커(편심축) 드래그 → e 변경(0.05 mm 단위), 디스크 가장자리 드래그 → 시간 되감기
- 툴바: 재생/일시정지, 하중 표시(F), 압력각 색상(α), 반투명 하우징, 반투명 디스크, 분해도, 시점 프리셋, 언어

### 로컬 빌드
```sh
rustup target add wasm32-unknown-unknown
cargo test            # 코어 모델 + 렌더러 수학 단위 테스트
trunk build --release # dist/ 생성 (public_url = /cycloid-drive/)
sh tools/privacy_check.sh
```
`trunk serve --release` 후 `http://localhost:8080/cycloid-drive/`에서 확인할 수 있습니다.

### 개인정보
100 % 클라이언트 측에서 동작합니다. 분석·추적·외부 CDN·폰트·런타임 네트워크 요청이 없고(CSP로 차단),
언어 선택과 안내 닫힘 여부만 브라우저 localStorage에 저장합니다.

## English

### What it is
A single-page tool answering: "I want a single-stage cycloidal reducer for my robot joint (3D-printed or machined) —
which pin count, pin circle, eccentricity and pin diameter give a valid, non-undercut profile, what are the contact
stresses and bearing loads at my peak torque, and can I export the disc profile to CAD?"

- **Inputs**: ring pin count Zp (lobes Zd = Zp − 1), pin circle radius Rp, eccentricity e, ring pin Ø dp, disc thickness b,
  1 or 2 discs (180° phased), peak output torque, input speed, output pin count / Ø / circle radius, eccentric-bearing bore Ø,
  material preset (typical values).
- **Outputs**: ratio, K1, minimum radius of curvature, pressure-angle distribution, per-pin forces, Hertz contact stress,
  eccentric-bearing load, output-pin load, output-hole clearance, profile perimeter, speeds and wobble frequency — each check
  OK / warning / fail plus an overall status.
- **Find designs**: for a target ratio and max OD, sweeps e (0.5–3 mm, step 0.25), dp (from a list) and Rp (1 mm steps),
  lists the top 12 that pass every check, sorted by (OK checks, stress margin, OD). Click a row to load it.
- **Export**: disc profile as SVG or DXF (LWPOLYLINE, mm, plus bore and output-hole circles), generated in the browser
  via a Blob download — no network.
- **3D view**: housing pins, extruded cycloid disc(s) with real bore and output holes, eccentric input shaft, output pins and
  flange, simple housing; animated (play/pause, slow motion default 0.25×, time slider); force colouring and arrows,
  pressure-angle edge colouring, translucent housing / discs, exploded view.
- **2D plots**: force per pin at the current instant, pressure angle vs profile parameter, and a top-view profile preview
  (theoretical vs offset curve), which also replaces the 3D view when WebGL2 is unavailable.

### Model and assumptions
- Theoretical curve (disc frame): C(φ) = Rp(cos φ, sin φ) − e(cos Zp·φ, sin Zp·φ); the disc profile is C offset inward by
  the pin radius rp along the normal. At φ = 0 it lies at radius Rp − e − rp (a valley where a pin seats).
- Ratio = Zd / (Zp − Zd) with Zp = Zd + 1 (output turns opposite to the input). Shortening coefficient K1 = e·Zp / Rp < 1.
- Undercut / cusp: the minimum convex radius of curvature of the theoretical curve must exceed rp.
- Pressure angle: angle between the contact normal and the direction of motion of the contact point when the disc turns
  about its own centre (moment arm = |C|·cos α). The HUD maximum is taken over the working zone (pins carrying ≥ 50 % of
  the peak force).
- Idealised load sharing: with θ the pin angle from the eccentric direction, the half with sin θ < 0 carries
  Fi = F0·|sin θi| along the line through the pitch point, F0 chosen so Σ Fi·li = T per disc. Output pins touch their holes
  along the eccentric direction with qj = Q0·|sin βj|, Σ qj·Rw·|sin βj| = T. The eccentric bearing carries the resultant.
  With two discs each carries half the torque.
- Hertz line contact: σ = √(F·E* / (π·b·R*)), 1/R* = 1/rp + 1/ρdisc (negative curvature on concave flanks); pins are
  assumed steel (typical).
- Housing OD estimate = 2·(Rp + dp). Output hole Ø = output pin Ø + 2e.
- Rigid parts, idealised load sharing, no profile modification / backlash, generic "typical" material values.
  No fatigue life, lubrication, thermal, multi-stage or tolerance analysis.

### Controls
- Left drag: orbit · right drag / Shift+drag / two fingers: pan · wheel / pinch: zoom · double-click: reset view
- Drag a ring pin → Rp; drag the orange eccentric marker → e (0.05 mm steps); drag the disc rim → scrub the motion
- Toolbar: play/pause, forces (F), pressure-angle colouring (α), translucent housing, translucent discs, exploded view,
  view presets (Perspective / Front (axial) / Side / Exploded), language

### Local build
```sh
rustup target add wasm32-unknown-unknown
cargo test            # core model + renderer math unit tests
trunk build --release # writes dist/ (public_url = /cycloid-drive/)
sh tools/privacy_check.sh
```
Run `trunk serve --release` and open `http://localhost:8080/cycloid-drive/`.

### Privacy
Runs 100 % client-side: no analytics, trackers, external CDNs, fonts or runtime network requests (blocked by the CSP).
Only the language choice and the dismissed hint are kept in the browser's localStorage.

### Layout
- `crates/cd_core` — pure-Rust model (profile, checks, loads, search, SVG/DXF export) with unit tests.
- `crates/cd_web` — web UI: `gl.rs` (generic WebGL2 renderer), `math.rs` (camera / picking math), `view3d.rs` (scene),
  `plot2d.rs` (canvas plots), `i18n.rs`, `lib.rs` (wiring).

## License
MIT — see [LICENSE](LICENSE). Educational estimates — validate with real parts and tests before trusting a robot with it.
