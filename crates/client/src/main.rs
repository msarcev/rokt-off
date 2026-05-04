use macroquad::prelude::*;
use sim::{Input, Level, World};

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
        for (idx, ship) in world.ships.iter().enumerate() {
            draw_ship(ship, SHIP_COLORS[idx]);
        }
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
    input
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

fn draw_hud(world: &World) {
    for (idx, ship) in world.ships.iter().enumerate() {
        let label = format!(
            "P{}  fuel {:>4.0}  v ({:>5.0},{:>5.0})",
            idx + 1,
            ship.fuel,
            ship.vel.x,
            ship.vel.y
        );
        draw_text(
            &label,
            12.0,
            22.0 + idx as f32 * 20.0,
            20.0,
            SHIP_COLORS[idx],
        );
    }
}
