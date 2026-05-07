pub mod menu;
pub mod net;
pub mod net_input;
pub mod session;

#[cfg(not(target_arch = "wasm32"))]
mod replay;

use macroquad::prelude::*;
use sim::{BitMask, DEFAULT_SEED, FUEL_MAX, Level, ParticleKind, RectKind, SHIELD_MAX, World};

use session::{LobbyStatus, LocalSession, P2pRunner, Session};
#[cfg(not(target_arch = "wasm32"))]
use session::SyncTestRunner;

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

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(s: &str);
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
    JoinEntry { buffer: String },
    Lobby { runner: Option<Box<P2pRunner>>, room: String, role: Role },
    Playing { is_net: bool, session: Box<dyn Session> },
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
                    menu::MenuChoice::Local => AppState::Playing {
                        is_net: false,
                        session: Box::new(LocalSession::new(fresh_world())),
                    },
                    menu::MenuChoice::Host => start_lobby(Role::Host, make_room_code()),
                    menu::MenuChoice::Join => AppState::JoinEntry { buffer: String::new() },
                })
            }
            AppState::JoinEntry { buffer } => {
                tick_join_entry(buffer);
                draw_join_entry(buffer);
                if is_key_pressed(KeyCode::Escape) {
                    Some(AppState::Menu(menu::Menu::new()))
                } else if is_key_pressed(KeyCode::Enter) && buffer.len() == ROOM_CODE_LEN {
                    Some(start_lobby(Role::Join, buffer.clone()))
                } else {
                    None
                }
            }
            AppState::Lobby { runner, room, role } => {
                let r = runner.as_mut().expect("lobby runner present");
                r.poll();
                let status = r.lobby_status();
                draw_lobby(room, *role, status);

                if is_key_pressed(KeyCode::Escape) {
                    Some(AppState::Menu(menu::Menu::new()))
                } else if is_key_pressed(KeyCode::R) && !status.ready {
                    let new_room = match role {
                        Role::Host => make_room_code(),
                        Role::Join => room.clone(),
                    };
                    Some(start_lobby(*role, new_room))
                } else if status.ready {
                    let taken = runner.take().expect("runner taken once");
                    Some(AppState::Playing { is_net: true, session: taken })
                } else {
                    None
                }
            }
            AppState::Playing { is_net, session } => {
                if is_key_pressed(KeyCode::Escape) {
                    Some(AppState::Menu(menu::Menu::new()))
                } else {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(r) = &replay
                        && is_key_pressed(KeyCode::R)
                    {
                        r.borrow_mut().reset();
                        let world = World::with_seed(Level::default(), DEFAULT_SEED);
                        let r2 = r.clone();
                        *session = Box::new(
                            LocalSession::new(world).with_recorder(Box::new(move |inputs| {
                                r2.borrow_mut().record(inputs)
                            })),
                        );
                    }

                    session.advance(get_frame_time());
                    draw_playing(session.world(), *is_net, show_mask);
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
    World::with_seed(Level::default(), DEFAULT_SEED)
}

fn start_lobby(role: Role, room: String) -> AppState {
    let url = room_url(&room);
    println!("[net] {role:?} room={room}");
    AppState::Lobby {
        runner: Some(Box::new(P2pRunner::new(fresh_world(), &url))),
        room,
        role,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn initial_state(
    replay: &mut Option<std::rc::Rc<std::cell::RefCell<replay::Replay>>>,
) -> AppState {
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
        return AppState::Playing {
            is_net: false,
            session: Box::new(SyncTestRunner::new(fresh_world())),
        };
    }
    if replay_flag {
        use std::cell::RefCell;
        use std::rc::Rc;
        let r = Rc::new(RefCell::new(replay::Replay::open()));
        let world = World::with_seed(Level::default(), r.borrow().seed);
        let recorded = r.borrow().recorded.clone();
        let mut local = LocalSession::new(world);
        local.replay(&recorded);
        println!(
            "[replay] replayed {} ticks from {}",
            recorded.len(),
            r.borrow().path.display()
        );
        let r2 = r.clone();
        local = local.with_recorder(Box::new(move |inputs| r2.borrow_mut().record(inputs)));
        *replay = Some(r);
        return AppState::Playing { is_net: false, session: Box::new(local) };
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
    let placeholder = format!("{}{}", buffer, "_".repeat(ROOM_CODE_LEN - buffer.len()));
    draw_centered(&placeholder, sw, sh * 0.55, 96, PAPER);
    draw_centered("Type a 5-letter code, ENTER to join, ESC to cancel", sw, sh * 0.75, 22, PAPER);
}

fn draw_lobby(room: &str, role: Role, status: LobbyStatus) {
    clear_background(SCREEN_BG);
    let sw = screen_width();
    let sh = screen_height();

    let header = match role {
        Role::Host => "HOSTING",
        Role::Join => "JOINING",
    };
    draw_centered(header, sw, sh * 0.25, 48, PAPER);
    draw_centered(room, sw, sh * 0.50, 128, PAPER);

    let status_line = if status.failed {
        "Connection failed."
    } else if status.ready {
        "Starting…"
    } else if status.remote_peers >= 1 {
        "Peer found — handshaking…"
    } else {
        "Waiting for opponent…"
    };
    let hint = match role {
        Role::Host => "R: new code   ESC: cancel",
        Role::Join => "R: retry   ESC: cancel",
    };
    draw_centered(status_line, sw, sh * 0.68, 28, PAPER);
    draw_centered(hint, sw, sh * 0.78, 20, PAPER);
}

fn draw_playing(world: &World, is_net: bool, show_mask: bool) {
    let sw = screen_width();
    let sh = screen_height();
    let dpi = screen_dpi_scale();

    let hud_h_px = (sh * HUD_H / TOTAL_H).floor();
    let avail_h = sh - hud_h_px;
    let play_scale = (sw / PLAY_W).min(avail_h / PLAY_H);
    let play_w_px = PLAY_W * play_scale;
    let play_h_px = PLAY_H * play_scale;
    let play_off_x = ((sw - play_w_px) * 0.5).floor();
    let play_off_y = ((avail_h - play_h_px) * 0.5).floor();

    let vp_y = sh - play_off_y - play_h_px;
    let cam = Camera2D {
        target: vec2(PLAY_W / 2.0, PLAY_H / 2.0),
        zoom: vec2(2.0 / PLAY_W, 2.0 / PLAY_H),
        viewport: Some((
            (play_off_x * dpi) as i32,
            (vp_y * dpi) as i32,
            (play_w_px * dpi) as i32,
            (play_h_px * dpi) as i32,
        )),
        ..Default::default()
    };

    clear_background(BLACK);
    set_camera(&cam);
    clear_background(Color::from_rgba(12, 14, 20, 255));
    draw_level(&world.level);
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
    draw_hud(world, 0.0, sh - hud_h_px, sw, hud_h_px, hud_h_px / HUD_H, is_net);
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

fn draw_mask_overlay(mask: &BitMask) {
    let color = Color::from_rgba(255, 0, 255, 110);
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

fn draw_hud(world: &World, x: f32, y: f32, w: f32, h: f32, s: f32, is_net: bool) {
    let paper = Color::from_rgba(238, 232, 213, 255);
    let ink = Color::from_rgba(50, 45, 60, 230);
    let ink_soft = Color::from_rgba(50, 45, 60, 110);
    let shield_fill = Color::from_rgba(110, 160, 190, 180);
    let fuel_fill = Color::from_rgba(190, 160, 80, 180);

    draw_rectangle(x, y, w, h, paper);
    draw_line(x, y, x + w, y, 1.5 * s, ink);
    draw_line(x + 24.0 * s, y + 3.0 * s, x + w - 24.0 * s, y + 3.0 * s, 0.7 * s, ink_soft);

    let pad_x = 22.0 * s;
    let bar_h = 11.0 * s;
    let bar_gap = 7.0 * s;
    let label_size = 22.0 * s;
    let label_w = 28.0 * s;
    let label_gap = 12.0 * s;

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

        draw_text(&format!("P{}", idx + 1), label_x, label_y, label_size, SHIP_COLORS[idx]);
        draw_pencil_bar(bar_x, bar_y_shield, bar_w, bar_h, ship.shields / SHIELD_MAX, shield_fill, ink, ink_soft, s);
        draw_pencil_bar(bar_x, bar_y_fuel, bar_w, bar_h, ship.fuel / FUEL_MAX, fuel_fill, ink, ink_soft, s);
    }

    let legend = if is_net {
        "Move: WASD or Arrows    Fire: F or Space"
    } else {
        "P1: WASD + F          P2: Arrows + Space"
    };
    let legend_size = 16.0 * s;
    let dim = measure_text(legend, None, legend_size as u16, 1.0);
    let legend_x = x + (w - dim.width) * 0.5;
    let legend_y = y + h - 10.0 * s;
    draw_text(legend, legend_x, legend_y, legend_size, ink);
}

fn draw_pencil_bar(x: f32, y: f32, w: f32, h: f32, frac: f32, fill: Color, ink: Color, ink_soft: Color, s: f32) {
    let frac = frac.clamp(0.0, 1.0);
    draw_rectangle(x, y, w * frac, h, fill);
    draw_rectangle_lines(x, y, w, h, 1.5 * s, ink);
    draw_rectangle_lines(x + 1.5 * s, y - 1.0 * s, w, h, 0.8 * s, ink_soft);
}
