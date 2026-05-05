use macroquad::prelude::*;
use sim::{Input, Level, RectKind, World, FUEL_MAX, SHIELD_MAX};

const SHIP_SIZE: f32 = 14.0;
const SHIP_COLORS: [Color; 2] = [SKYBLUE, ORANGE];

fn window_conf() -> Conf {
    Conf {
        window_title: "head-on-rs".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let level = Level::default();
    let mut world = World::new(level);
    let mut accumulator = 0.0_f32;

    loop {
        accumulator += get_frame_time();
        while accumulator >= sim::DT {
            let inputs = [poll_input_p1(), poll_input_p2()];
            world.tick(inputs);
            accumulator -= sim::DT;
        }

        clear_background(Color::from_rgba(12, 14, 20, 255));
        draw_level(&world.level);
        for (idx, ship) in world.ships.iter().enumerate() {
            if ship.alive {
                draw_ship(ship, SHIP_COLORS[idx]);
            }
        }
        draw_bullets(&world);
        draw_particles(&world);
        draw_hud(&world);

        next_frame().await
    }
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
    if is_key_down(KeyCode::Space) {
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
    if is_key_down(KeyCode::M) {
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
        let frac = (p.ttl / p.max_ttl).clamp(0.0, 1.0);
        let color = Color::new(1.0, 0.7, 0.3, frac);
        draw_circle(p.pos.x, p.pos.y, 2.0, color);
    }
}

fn draw_hud(world: &World) {
    const BAR_W: f32 = 180.0;
    const BAR_H: f32 = 10.0;
    const PAD: f32 = 12.0;
    let screen_w = screen_width();
    for (idx, ship) in world.ships.iter().enumerate() {
        let x = if idx == 0 { PAD } else { screen_w - PAD - BAR_W };
        let y0 = PAD;

        draw_text(
            &format!("P{}", idx + 1),
            x,
            y0 + 14.0,
            18.0,
            SHIP_COLORS[idx],
        );

        let bar_x = x + 28.0;
        // Shield bar (cyan).
        draw_bar(bar_x, y0, BAR_W - 28.0, BAR_H, ship.shields / SHIELD_MAX, SKYBLUE);
        // Fuel bar (yellow).
        draw_bar(bar_x, y0 + BAR_H + 4.0, BAR_W - 28.0, BAR_H, ship.fuel / FUEL_MAX, YELLOW);
    }
}

fn draw_bar(x: f32, y: f32, w: f32, h: f32, frac: f32, fill: Color) {
    let frac = frac.clamp(0.0, 1.0);
    draw_rectangle(x, y, w, h, Color::from_rgba(40, 40, 50, 200));
    draw_rectangle(x, y, w * frac, h, fill);
    draw_rectangle_lines(x, y, w, h, 1.0, Color::from_rgba(180, 180, 200, 200));
}
