pub mod camera;
pub mod menu;
pub mod net;
pub mod net_input;
pub mod session;
pub mod touch;

#[cfg(not(target_arch = "wasm32"))]
mod replay;

use macroquad::prelude::*;
use sim::{BitMask, DEFAULT_SEED, FUEL_MAX, Level, ParticleKind, RectKind, SHIELD_MAX, World};

use std::cell::RefCell;
use std::rc::Rc;

use camera::FollowCamera;
#[cfg(not(target_arch = "wasm32"))]
use session::SyncTestRunner;
use session::{LobbyPhase, LobbyStatus, LocalSession, P2pRunner, Session, no_input};
use touch::TouchInput;

const BASE_VIEW: f32 = 360.0;
const FOLLOW_SMOOTHING: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
pub enum PlayMode {
    Local,
    Net { local_handle: usize },
}

impl PlayMode {
    fn followed_ship(self) -> usize {
        match self {
            PlayMode::Local => 0,
            PlayMode::Net { local_handle } => local_handle,
        }
    }
}

const SIGNALING_BASE: &str = match option_env!("ROKTOFF_SIGNALING_URL") {
    Some(s) => s,
    None => "ws://localhost:3536",
};

const ROOM_CODE_LEN: usize = 5;

fn room_url(room: &str) -> String {
    format!("{SIGNALING_BASE}/{room}?next=2")
}

fn make_room_code() -> String {
    let mut code = String::with_capacity(ROOM_CODE_LEN);
    for _ in 0..ROOM_CODE_LEN {
        let n = rand::gen_range::<u32>(0, 26) as u8;
        code.push(char::from(b'A' + n));
    }
    code
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Host,
    Join,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = "roktoff_get_room")]
    fn js_get_room() -> String;

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = "roktoff_join_show")]
    fn js_join_show();

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = "roktoff_join_hide")]
    fn js_join_hide();

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = "roktoff_join_buffer")]
    fn js_join_buffer() -> String;

    #[wasm_bindgen::prelude::wasm_bindgen(js_name = "roktoff_join_take_submit")]
    fn js_join_take_submit() -> bool;

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(s: &str);
}

#[cfg(not(target_arch = "wasm32"))]
fn js_join_show() {}
#[cfg(not(target_arch = "wasm32"))]
fn js_join_hide() {}
#[cfg(not(target_arch = "wasm32"))]
fn js_join_take_submit() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn url_room_code() -> Option<String> {
    let r = js_get_room();
    (!r.is_empty()).then_some(r)
}

/// On wasm, panics trap without unwinding, leaving miniquad's RefCell
/// guards locked and producing `already_borrowed` spam that hides the
/// real first panic. This hook prints that first panic's payload+location
/// to `console.error` instead of the default `{:?}` debug format.
#[cfg(target_arch = "wasm32")]
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = info
            .payload()
            .downcast_ref::<&'static str>()
            .copied()
            .map(str::to_string)
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let loc = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        console_error(&format!("[panic] {loc}: {msg}"));
    }));
}

const SHIP_SIZE: f32 = 14.0;
const SHIP_COLORS: [Color; 2] = [SKYBLUE, ORANGE];

const PLAY_W: f32 = 1280.0;
const PLAY_H: f32 = 720.0;
const HUD_H: f32 = 96.0;
const TOTAL_H: f32 = PLAY_H + HUD_H;

pub fn window_conf() -> Conf {
    Conf {
        window_title: "rokt-off".to_owned(),
        window_width: PLAY_W as i32,
        window_height: TOTAL_H as i32,
        window_resizable: true,
        high_dpi: true,
        ..Default::default()
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_start() {
    macroquad::Window::from_config(window_conf(), run());
}

enum AppState {
    Menu(menu::Menu),
    JoinEntry {
        buffer: String,
    },
    Lobby {
        runner: Option<Box<P2pRunner>>,
        room: String,
        role: Role,
        touch: Rc<RefCell<TouchInput>>,
    },
    Playing {
        mode: PlayMode,
        session: Box<dyn Session>,
        camera: FollowCamera,
        touch: Rc<RefCell<TouchInput>>,
    },
}

pub async fn run() {
    #[cfg(target_arch = "wasm32")]
    install_panic_hook();

    rand::srand(miniquad::date::now().to_bits());

    #[cfg(not(target_arch = "wasm32"))]
    let mut replay: Option<std::rc::Rc<std::cell::RefCell<replay::Replay>>> = None;

    let mut state: AppState = initial_state(
        #[cfg(not(target_arch = "wasm32"))]
        &mut replay,
    );

    let mut fullscreen = false;
    let mut show_mask = false;
    loop {
        if is_key_pressed(KeyCode::F11) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
        }
        if is_key_pressed(KeyCode::F1) {
            show_mask = !show_mask;
        }

        let next: Option<AppState> = match &mut state {
            AppState::Menu(menu) => {
                menu.tick();
                menu.draw();
                menu.take_choice().map(|choice| match choice {
                    menu::MenuChoice::Local => {
                        let world = fresh_world();
                        let mode = PlayMode::Local;
                        let camera = make_camera(&world, mode);
                        let touch = make_touch();
                        AppState::Playing {
                            mode,
                            session: Box::new(LocalSession::new(
                                world,
                                [touch::input_source(touch.clone()), no_input()],
                            )),
                            camera,
                            touch,
                        }
                    }
                    menu::MenuChoice::Host => start_lobby(Role::Host, make_room_code()),
                    menu::MenuChoice::Join => {
                        js_join_show();
                        AppState::JoinEntry {
                            buffer: String::new(),
                        }
                    }
                })
            }
            AppState::JoinEntry { buffer } => {
                tick_join_entry(buffer);
                draw_join_entry(buffer);
                let submitted = js_join_take_submit() || is_key_pressed(KeyCode::Enter);
                if is_key_pressed(KeyCode::Escape) || back_tapped() {
                    js_join_hide();
                    Some(AppState::Menu(menu::Menu::new()))
                } else if submitted && buffer.len() == ROOM_CODE_LEN {
                    js_join_hide();
                    Some(start_lobby(Role::Join, buffer.clone()))
                } else {
                    None
                }
            }
            AppState::Lobby {
                runner,
                room,
                role,
                touch,
            } => {
                let r = runner.as_mut().expect("lobby runner present");
                r.poll();
                let status = r.lobby_status();
                let err = r.last_error();
                draw_lobby(room, *role, status, err.as_deref());

                if is_key_pressed(KeyCode::Escape) || back_tapped() {
                    Some(AppState::Menu(menu::Menu::new()))
                } else if is_key_pressed(KeyCode::R) && !status.ready {
                    let new_room = match role {
                        Role::Host => make_room_code(),
                        Role::Join => room.clone(),
                    };
                    Some(start_lobby(*role, new_room))
                } else if status.ready {
                    let local_handle = runner
                        .as_ref()
                        .and_then(|r| r.local_handle())
                        .expect("local handle set when ready");
                    let taken = runner.take().expect("runner taken once");
                    let mode = PlayMode::Net { local_handle };
                    let camera = make_camera(taken.world(), mode);
                    Some(AppState::Playing {
                        mode,
                        session: taken,
                        camera,
                        touch: touch.clone(),
                    })
                } else {
                    None
                }
            }
            AppState::Playing {
                mode,
                session,
                camera,
                touch,
            } => {
                if is_key_pressed(KeyCode::Escape) || touch.borrow_mut().take_menu_press() {
                    Some(AppState::Menu(menu::Menu::new()))
                } else {
                    note_keyboard_input(touch);

                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(r) = &replay
                        && is_key_pressed(KeyCode::R)
                    {
                        r.borrow_mut().reset();
                        let world = World::with_seed(Level::cave_02(), DEFAULT_SEED);
                        let r2 = r.clone();
                        *session = Box::new(
                            LocalSession::new(
                                world,
                                [touch::input_source(touch.clone()), no_input()],
                            )
                            .with_recorder(Box::new(move |inputs| r2.borrow_mut().record(inputs))),
                        );
                        *camera = make_camera(session.world(), *mode);
                    }

                    let dt = get_frame_time();
                    session.advance(dt);
                    let world = session.world();
                    let sh = screen_height();
                    let play_h = sh - hud_h_px(sh);
                    let aspect = screen_width() / play_h;
                    camera.view_size = view_size_for_aspect(aspect);
                    camera.update(followed_pos(world, *mode), level_size(world), dt);
                    draw_playing(world, camera, &touch.borrow(), show_mask);
                    None
                }
            }
        };

        if let Some(next_state) = next {
            state = next_state;
        }

        next_frame().await
    }
}

fn fresh_world() -> World {
    World::with_seed(Level::cave_02(), DEFAULT_SEED)
}

fn followed_pos(world: &World, mode: PlayMode) -> Vec2 {
    let p = world.ships[mode.followed_ship()].pos;
    vec2(p.x, p.y)
}

fn level_size(world: &World) -> Vec2 {
    vec2(
        world.level.mask.width as f32,
        world.level.mask.height as f32,
    )
}

fn hud_h_px(sh: f32) -> f32 {
    (sh * HUD_H / TOTAL_H).floor()
}

fn view_size_for_aspect(aspect: f32) -> Vec2 {
    if aspect >= 1.0 {
        vec2(BASE_VIEW * aspect, BASE_VIEW)
    } else {
        vec2(BASE_VIEW, BASE_VIEW / aspect)
    }
}

fn make_camera(world: &World, mode: PlayMode) -> FollowCamera {
    let pos = followed_pos(world, mode);
    // Initial view is a placeholder; the run loop overwrites it each frame
    // before `update`. Use a sane 16:9 default so `snap_to`'s clamp produces
    // a sensible first-frame target.
    let initial_view = view_size_for_aspect(16.0 / 9.0);
    let mut cam = FollowCamera::new(pos, initial_view, FOLLOW_SMOOTHING);
    cam.snap_to(pos, level_size(world));
    cam
}

fn make_touch() -> Rc<RefCell<TouchInput>> {
    Rc::new(RefCell::new(TouchInput::new()))
}

fn note_keyboard_input(touch: &Rc<RefCell<TouchInput>>) {
    if [KeyCode::Up, KeyCode::Left, KeyCode::Right, KeyCode::Space]
        .iter()
        .any(|k| is_key_pressed(*k))
    {
        touch.borrow_mut().note_keyboard_press();
    }
}

fn start_lobby(role: Role, room: String) -> AppState {
    let url = room_url(&room);
    println!("[net] {role:?} room={room}");
    let touch = make_touch();
    let source = touch::input_source(touch.clone());
    AppState::Lobby {
        runner: Some(Box::new(P2pRunner::new(fresh_world(), &url, source))),
        room,
        role,
        touch,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn initial_state(replay: &mut Option<std::rc::Rc<std::cell::RefCell<replay::Replay>>>) -> AppState {
    let args: Vec<String> = std::env::args().collect();
    let net_idx = args.iter().position(|a| a == "--net");
    let sync_test = net_idx.is_none() && args.iter().any(|a| a == "--sync-test");
    let replay_flag = net_idx.is_none() && !sync_test && args.iter().any(|a| a == "--replay");

    if let Some(idx) = net_idx {
        let room = args
            .get(idx + 1)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .map(|s| s.to_uppercase())
            .unwrap_or_else(make_room_code);
        return start_lobby(Role::Join, room);
    }
    if sync_test {
        println!("[sync-test] rollback validator engaged; check_distance=4");
        let world = fresh_world();
        let mode = PlayMode::Local;
        let camera = make_camera(&world, mode);
        let touch = make_touch();
        return AppState::Playing {
            mode,
            session: Box::new(SyncTestRunner::new(world)),
            camera,
            touch,
        };
    }
    if replay_flag {
        let r = Rc::new(RefCell::new(replay::Replay::open()));
        let world = World::with_seed(Level::cave_02(), r.borrow().seed);
        let recorded = r.borrow().recorded.clone();
        let touch = make_touch();
        let mut local = LocalSession::new(world, [touch::input_source(touch.clone()), no_input()]);
        local.replay(&recorded);
        println!(
            "[replay] replayed {} ticks from {}",
            recorded.len(),
            r.borrow().path.display()
        );
        let r2 = r.clone();
        local = local.with_recorder(Box::new(move |inputs| r2.borrow_mut().record(inputs)));
        *replay = Some(r);
        let mode = PlayMode::Local;
        let camera = make_camera(local.world(), mode);
        return AppState::Playing {
            mode,
            session: Box::new(local),
            camera,
            touch,
        };
    }
    AppState::Menu(menu::Menu::new())
}

#[cfg(target_arch = "wasm32")]
fn initial_state() -> AppState {
    if let Some(room) = url_room_code() {
        return start_lobby(Role::Join, room);
    }
    AppState::Menu(menu::Menu::new())
}

const BACK_R: f32 = 22.0;
const BACK_MARGIN: f32 = 18.0;

fn back_center() -> Vec2 {
    vec2(BACK_MARGIN + BACK_R, BACK_MARGIN + BACK_R)
}

fn back_tapped() -> bool {
    let bc = back_center();
    let dpr = screen_dpi_scale();
    touches().iter().any(|t| {
        t.phase == TouchPhase::Started && (t.position / dpr - bc).length() <= BACK_R
    })
}

#[cfg(target_arch = "wasm32")]
fn tick_join_entry(buffer: &mut String) {
    *buffer = js_join_buffer();
}

#[cfg(not(target_arch = "wasm32"))]
fn tick_join_entry(buffer: &mut String) {
    while let Some(c) = get_char_pressed() {
        if buffer.len() < ROOM_CODE_LEN && c.is_ascii_alphabetic() {
            buffer.push(c.to_ascii_uppercase());
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        buffer.pop();
    }
}

const SCREEN_BG: Color = Color::new(12.0 / 255.0, 14.0 / 255.0, 20.0 / 255.0, 1.0);
const PAPER: Color = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 1.0);

fn draw_centered(text: &str, sw: f32, y: f32, size: u16, color: Color) {
    let d = measure_text(text, None, size, 1.0);
    draw_text(text, (sw - d.width) * 0.5, y, size as f32, color);
}

fn draw_join_entry(buffer: &str) {
    clear_background(SCREEN_BG);
    let sw = screen_width();
    let sh = screen_height();

    draw_centered("JOIN A ROOM", sw, sh * 0.30, 56, PAPER);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let placeholder = format!("{}{}", buffer, "_".repeat(ROOM_CODE_LEN - buffer.len()));
        draw_centered(&placeholder, sw, sh * 0.55, 96, PAPER);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = buffer;

    let hint = if cfg!(target_arch = "wasm32") {
        "Tap the box to type, then press Done"
    } else {
        "Type a 5-letter code, ENTER to join, ESC to cancel"
    };
    draw_centered(hint, sw, sh * 0.75, 22, PAPER);

    let bc = back_center();
    let ink = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.7);
    let idle = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.18);
    draw_circle(bc.x, bc.y, BACK_R, idle);
    draw_circle_lines(bc.x, bc.y, BACK_R, 2.0, ink);
    let arrow = "<";
    let asize = BACK_R * 1.2;
    let dim = measure_text(arrow, None, asize as u16, 1.0);
    draw_text(
        arrow,
        bc.x - dim.width * 0.5,
        bc.y + dim.height * 0.5,
        asize,
        ink,
    );
}

fn draw_lobby(room: &str, role: Role, status: LobbyStatus, error: Option<&str>) {
    clear_background(SCREEN_BG);
    let sw = screen_width();
    let sh = screen_height();

    let header = match role {
        Role::Host => "HOSTING",
        Role::Join => "JOINING",
    };
    draw_centered(header, sw, sh * 0.25, 48, PAPER);
    draw_centered(room, sw, sh * 0.50, 128, PAPER);

    let status_line = match status.phase {
        LobbyPhase::Connecting => "Connecting to signaling…",
        LobbyPhase::SignalingOpen => "Waiting for opponent…",
        LobbyPhase::PeerConnected => "Peer found — starting match…",
        LobbyPhase::Ready => "Starting…",
        LobbyPhase::Failed => "Connection failed.",
    };
    draw_centered(status_line, sw, sh * 0.64, 28, PAPER);

    let diag = format!(
        "signaling: {} ({})   peers: {}",
        signaling_host(),
        if status.signaling_open { "ok" } else { "…" },
        status.remote_peers,
    );
    draw_centered(&diag, sw, sh * 0.71, 18, PAPER);

    if let Some(e) = error {
        let truncated = if e.len() > 90 { &e[..90] } else { e };
        draw_centered(truncated, sw, sh * 0.77, 16, PAPER);
    }

    let hint = match role {
        Role::Host => "R: new code   ESC / tap < : cancel",
        Role::Join => "R: retry      ESC / tap < : cancel",
    };
    draw_centered(hint, sw, sh * 0.85, 18, PAPER);

    draw_back_arrow();
}

fn signaling_host() -> &'static str {
    let s = SIGNALING_BASE;
    s.strip_prefix("wss://")
        .or_else(|| s.strip_prefix("ws://"))
        .unwrap_or(s)
}

fn draw_back_arrow() {
    let bc = back_center();
    let ink = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.7);
    let idle = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.18);
    draw_circle(bc.x, bc.y, BACK_R, idle);
    draw_circle_lines(bc.x, bc.y, BACK_R, 2.0, ink);
    let arrow = "<";
    let asize = BACK_R * 1.2;
    let dim = measure_text(arrow, None, asize as u16, 1.0);
    draw_text(
        arrow,
        bc.x - dim.width * 0.5,
        bc.y + dim.height * 0.5,
        asize,
        ink,
    );
}

fn draw_playing(world: &World, camera: &FollowCamera, touch: &TouchInput, show_mask: bool) {
    let sw = screen_width();
    let sh = screen_height();
    let dpi = screen_dpi_scale();

    let hud_px = hud_h_px(sh);
    let play_w_px = sw;
    let play_h_px = sh - hud_px;

    // GL viewport origin is bottom-left; HUD sits at the bottom of the
    // screen in CSS coords, so the play region's GL y-origin is `hud_px`.
    let cam = camera.macroquad_camera((
        0,
        (hud_px * dpi) as i32,
        (play_w_px * dpi) as i32,
        (play_h_px * dpi) as i32,
    ));

    clear_background(BLACK);
    set_camera(&cam);
    clear_background(Color::from_rgba(12, 14, 20, 255));
    draw_level(&world.level);
    draw_mask_walls(&world.level.mask);
    if show_mask {
        draw_mask_overlay(&world.level.mask);
    }
    for (idx, ship) in world.ships.iter().enumerate() {
        if ship.alive {
            draw_ship(ship, SHIP_COLORS[idx]);
        }
    }
    draw_bullets(world);
    draw_particles(world);
    set_default_camera();
    draw_hud(
        world,
        0.0,
        sh - hud_px,
        sw,
        hud_px,
        hud_px / HUD_H,
        touch.is_active(),
        sh > sw,
    );
    touch.draw_overlay();
}

fn draw_level(level: &Level) {
    for r in &level.rects {
        let color = match r.kind {
            RectKind::Wall => Color::from_rgba(70, 70, 80, 255),
            RectKind::Pad => Color::from_rgba(80, 200, 120, 255),
        };
        let size = r.max - r.min;
        draw_rectangle(r.min.x, r.min.y, size.x, size.y, color);
    }
}

fn draw_mask_walls(mask: &BitMask) {
    draw_mask_runs(mask, Color::from_rgba(70, 70, 80, 255));
}

fn draw_mask_overlay(mask: &BitMask) {
    draw_mask_runs(mask, Color::from_rgba(255, 0, 255, 110));
}

fn draw_mask_runs(mask: &BitMask, color: Color) {
    for y in 0..mask.height as i32 {
        let mut run_start: Option<i32> = None;
        for x in 0..mask.width as i32 {
            let solid = mask.is_solid(x, y);
            match (solid, run_start) {
                (true, None) => run_start = Some(x),
                (false, Some(start)) => {
                    draw_rectangle(start as f32, y as f32, (x - start) as f32, 1.0, color);
                    run_start = None;
                }
                _ => {}
            }
        }
        if let Some(start) = run_start {
            draw_rectangle(
                start as f32,
                y as f32,
                (mask.width as i32 - start) as f32,
                1.0,
                color,
            );
        }
    }
}

fn draw_ship(ship: &sim::Ship, color: Color) {
    let cos = ship.angle.cos();
    let sin = ship.angle.sin();
    let nose = vec2(ship.pos.x + cos * SHIP_SIZE, ship.pos.y + sin * SHIP_SIZE);
    let left = vec2(
        ship.pos.x + (cos * -0.7 - sin * 0.7) * SHIP_SIZE,
        ship.pos.y + (sin * -0.7 + cos * 0.7) * SHIP_SIZE,
    );
    let right = vec2(
        ship.pos.x + (cos * -0.7 + sin * 0.7) * SHIP_SIZE,
        ship.pos.y + (sin * -0.7 - cos * 0.7) * SHIP_SIZE,
    );
    draw_triangle(nose, left, right, color);
    draw_triangle_lines(nose, left, right, 1.5, WHITE);
}

fn draw_bullets(world: &World) {
    for b in &world.bullets {
        let color = SHIP_COLORS[b.owner as usize];
        draw_circle(b.pos.x, b.pos.y, 2.5, color);
    }
}

fn draw_particles(world: &World) {
    for p in &world.particles {
        let frac_alpha = (p.ttl / p.max_ttl).clamp(0.5, 1.0);
        let frac_size = (p.ttl / p.max_ttl).clamp(0.75, 1.0);
        let (color, radius) = match p.kind {
            ParticleKind::Thrust => (Color::new(0.5, 0.85, 1.0, frac_alpha), 1.6),
            ParticleKind::Explosion => (Color::new(1.0, 0.7, 0.3, frac_alpha), 2.0),
        };
        draw_circle(p.pos.x, p.pos.y, radius * frac_size, color);
    }
}

fn draw_hud(
    world: &World,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    touch_active: bool,
    portrait: bool,
) {
    let paper = Color::from_rgba(238, 232, 213, 255);
    let ink = Color::from_rgba(50, 45, 60, 230);
    let ink_soft = Color::from_rgba(50, 45, 60, 110);
    let shield_fill = Color::from_rgba(110, 160, 190, 180);
    let fuel_fill = Color::from_rgba(190, 160, 80, 180);

    draw_rectangle(x, y, w, h, paper);
    draw_line(x, y, x + w, y, 1.5 * s, ink);
    draw_line(
        x + 24.0 * s,
        y + 3.0 * s,
        x + w - 24.0 * s,
        y + 3.0 * s,
        0.7 * s,
        ink_soft,
    );

    let pad_x = if portrait { 10.0 * s } else { 22.0 * s };
    let bar_h = 11.0 * s;
    let bar_gap = 7.0 * s;
    let label_size = 22.0 * s;
    let label_w = 28.0 * s;
    let label_gap = if portrait { 6.0 * s } else { 12.0 * s };

    let half = w * 0.5;
    let max_bar_w = (half - pad_x - label_w - label_gap - 8.0 * s).max(40.0 * s);
    let bar_w = (200.0 * s).min(max_bar_w);

    let bar_y_shield = y + 20.0 * s;
    let bar_y_fuel = bar_y_shield + bar_h + bar_gap;
    let label_y = bar_y_shield + bar_h + bar_gap * 0.5 + label_size * 0.35;

    for (idx, ship) in world.ships.iter().enumerate() {
        let (label_x, bar_x) = if idx == 0 {
            (x + pad_x, x + pad_x + label_w + label_gap)
        } else {
            let bx = x + w - pad_x - bar_w;
            (bx - label_gap - label_w, bx)
        };

        draw_text(
            &format!("P{}", idx + 1),
            label_x,
            label_y,
            label_size,
            SHIP_COLORS[idx],
        );
        draw_pencil_bar(
            bar_x,
            bar_y_shield,
            bar_w,
            bar_h,
            ship.shields / SHIELD_MAX,
            shield_fill,
            ink,
            ink_soft,
            s,
        );
        draw_pencil_bar(
            bar_x,
            bar_y_fuel,
            bar_w,
            bar_h,
            ship.fuel / FUEL_MAX,
            fuel_fill,
            ink,
            ink_soft,
            s,
        );
    }

    if !touch_active && !portrait {
        let legend = "Move: Arrows    Fire: Space";
        let legend_size = 16.0 * s;
        let dim = measure_text(legend, None, legend_size as u16, 1.0);
        let legend_x = x + (w - dim.width) * 0.5;
        let legend_y = y + h - 10.0 * s;
        draw_text(legend, legend_x, legend_y, legend_size, ink);
    }
}

fn draw_pencil_bar(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    frac: f32,
    fill: Color,
    ink: Color,
    ink_soft: Color,
    s: f32,
) {
    let frac = frac.clamp(0.0, 1.0);
    draw_rectangle(x, y, w * frac, h, fill);
    draw_rectangle_lines(x, y, w, h, 1.5 * s, ink);
    draw_rectangle_lines(x + 1.5 * s, y - 1.0 * s, w, h, 0.8 * s, ink_soft);
}
