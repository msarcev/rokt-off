use macroquad::prelude::{is_key_down, KeyCode};
use sim::{Input, World};

/// A driver for the simulation. The main loop calls `advance` once per
/// frame with the wall-clock delta; the session decides how to step `world`.
pub trait Session {
    fn advance(&mut self, frame_dt: f32);
    fn world(&self) -> &World;
}

/// Single-machine, fixed-step. Polls the keyboard for both players directly.
/// Optionally records each input pair via the supplied callback (used for
/// the `--replay` log).
pub struct LocalSession {
    world: World,
    accumulator: f32,
    on_tick: Option<Box<dyn FnMut([Input; 2])>>,
}

impl LocalSession {
    pub fn new(world: World) -> Self {
        Self { world, accumulator: 0.0, on_tick: None }
    }

    pub fn with_recorder(mut self, f: Box<dyn FnMut([Input; 2])>) -> Self {
        self.on_tick = Some(f);
        self
    }

    /// Replay a recorded input stream against the world before live play.
    pub fn replay(&mut self, recorded: &[[Input; 2]]) {
        for inputs in recorded {
            self.world.tick(*inputs);
        }
    }
}

impl Session for LocalSession {
    fn advance(&mut self, frame_dt: f32) {
        self.accumulator += frame_dt;
        while self.accumulator >= sim::DT {
            let inputs = [poll_input_p1(), poll_input_p2()];
            self.world.tick(inputs);
            if let Some(cb) = self.on_tick.as_mut() {
                cb(inputs);
            }
            self.accumulator -= sim::DT;
        }
    }

    fn world(&self) -> &World {
        &self.world
    }
}

pub fn poll_input_p1() -> Input {
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

pub fn poll_input_p2() -> Input {
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
