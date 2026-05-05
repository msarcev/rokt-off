use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

use macroquad::prelude::*;
use sim::{Input, Level, ParticleKind, RectKind, World, DEFAULT_SEED, FUEL_MAX, SHIELD_MAX};

const SHIP_SIZE: f32 = 14.0;
const SHIP_COLORS: [Color; 2] = [SKYBLUE, ORANGE];

const PLAY_W: f32 = 1280.0;
const PLAY_H: f32 = 720.0;
const HUD_H: f32 = 96.0;
const TOTAL_H: f32 = PLAY_H + HUD_H;

fn window_conf() -> Conf {
    Conf {
        window_title: "head-on-rs".to_owned(),
        window_width: PLAY_W as i32,
        window_height: TOTAL_H as i32,
        window_resizable: true,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let replay_enabled = std::env::args().any(|a| a == "--replay");
    let mut replay = if replay_enabled { Some(Replay::open()) } else { None };

    let seed = replay.as_ref().map(|w| w.seed).unwrap_or(DEFAULT_SEED);
    let mut world = World::with_seed(Level::default(), seed);

    if let Some(w) = &replay {
        for inputs in &w.recorded {
            world.tick(*inputs);
        }
        println!("[replay] replayed {} ticks from {}", w.recorded.len(), w.path.display());
    }

    let mut accumulator = 0.0_f32;
    let mut fullscreen = false;
    loop {
        if is_key_pressed(KeyCode::F11) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
        }

        if replay.is_some() && is_key_pressed(KeyCode::R) {
            replay.as_mut().unwrap().reset();
            world = World::with_seed(Level::default(), DEFAULT_SEED);
            accumulator = 0.0;
        }

        accumulator += get_frame_time();
        while accumulator >= sim::DT {
            let inputs = [poll_input_p1(), poll_input_p2()];
            world.tick(inputs);
            if let Some(w) = &mut replay {
                w.record(inputs);
            }
            accumulator -= sim::DT;
        }

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
        for (idx, ship) in world.ships.iter().enumerate() {
            if ship.alive {
                draw_ship(ship, SHIP_COLORS[idx]);
            }
        }
        draw_bullets(&world);
        draw_particles(&world);
        set_default_camera();
        draw_hud(&world, 0.0, sh - hud_h_px, sw, hud_h_px, hud_h_px / HUD_H);

        next_frame().await
    }
}

struct Replay {
    path: PathBuf,
    file: File,
    seed: u64,
    recorded: Vec<[Input; 2]>,
}

impl Replay {
    fn open() -> Self {
        let path = replay_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let (seed, recorded) = match File::open(&path) {
            Ok(mut f) => {
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).expect("read dev.bin");
                if buf.len() >= 8 {
                    let seed = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                    let recorded = buf[8..]
                        .chunks_exact(2)
                        .map(|c| [Input::from_bits_truncate(c[0]), Input::from_bits_truncate(c[1])])
                        .collect();
                    (seed, recorded)
                } else {
                    (DEFAULT_SEED, Vec::new())
                }
            }
            Err(_) => (DEFAULT_SEED, Vec::new()),
        };

        if recorded.is_empty() {
            let mut f = File::create(&path).expect("create dev.bin");
            f.write_all(&seed.to_le_bytes()).expect("write seed");
        }

        let file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open dev.bin for append");

        Self { path, file, seed, recorded }
    }

    fn record(&mut self, inputs: [Input; 2]) {
        let _ = self.file.write_all(&[inputs[0].bits(), inputs[1].bits()]);
    }

    fn reset(&mut self) {
        let mut f = File::create(&self.path).expect("truncate dev.bin");
        f.write_all(&DEFAULT_SEED.to_le_bytes()).expect("write seed");
        self.file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .expect("reopen dev.bin");
        self.seed = DEFAULT_SEED;
        self.recorded.clear();
        println!("[replay] reset");
    }
}

fn replay_path() -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    exe.parent()
        .and_then(|p| p.parent())
        .map(|target| target.join("dev.bin"))
        .unwrap_or_else(|| PathBuf::from("target/dev.bin"))
}

fn poll_input_p1() -> Input {
    let mut input = Input::empty();
    if is_key_down(KeyCode::W) {
        input |= Input::THRUST;
    }
    if is_key_down(KeyCode::A) {
        input |= Input::ROTATE_LEFT;
    }
    if is_key_down(KeyCode::D) {
        input |= Input::ROTATE_RIGHT;
    }
    if is_key_down(KeyCode::F) {
        input |= Input::FIRE;
    }
    input
}

fn poll_input_p2() -> Input {
    let mut input = Input::empty();
    if is_key_down(KeyCode::Up) {
        input |= Input::THRUST;
    }
    if is_key_down(KeyCode::Left) {
        input |= Input::ROTATE_LEFT;
    }
    if is_key_down(KeyCode::Right) {
        input |= Input::ROTATE_RIGHT;
    }
    if is_key_down(KeyCode::RightControl) {
        input |= Input::FIRE;
    }
    input
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

fn draw_hud(world: &World, x: f32, y: f32, w: f32, h: f32, s: f32) {
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
}

fn draw_pencil_bar(x: f32, y: f32, w: f32, h: f32, frac: f32, fill: Color, ink: Color, ink_soft: Color, s: f32) {
    let frac = frac.clamp(0.0, 1.0);
    draw_rectangle(x, y, w * frac, h, fill);
    draw_rectangle_lines(x, y, w, h, 1.5 * s, ink);
    draw_rectangle_lines(x + 1.5 * s, y - 1.0 * s, w, h, 0.8 * s, ink_soft);
}
