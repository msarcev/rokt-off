use ggrs::{
    Config, GgrsRequest, PlayerType, PredictRepeatLast, SessionBuilder, SyncTestSession,
};
use macroquad::prelude::{is_key_down, KeyCode};
use sim::{Input, World};

use crate::net_input::NetInput;

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

/// GGRS type bundle. `State = World` (already `Clone + Send + Sync`).
/// `Input = NetInput` (a serde-able wrapper around `sim::Input`).
/// `Address = String` is unused in SyncTest; gets re-used for matchbox PeerId
/// in commit 3.
pub struct GgrsConfig;

impl Config for GgrsConfig {
    type Input = NetInput;
    type InputPredictor = PredictRepeatLast;
    type State = World;
    type Address = String;
}

/// Offline rollback validator. Re-simulates `check_distance` frames every
/// step and panics on a checksum mismatch — the cheapest way to flush out
/// non-determinism in the sim before going to the network.
pub struct SyncTestRunner {
    session: SyncTestSession<GgrsConfig>,
    world: World,
    accumulator: f32,
    frame: i64,
}

impl SyncTestRunner {
    pub fn new(world: World) -> Self {
        let session = SessionBuilder::<GgrsConfig>::new()
            .with_num_players(2)
            .expect("num_players")
            .with_check_distance(4)
            .with_input_delay(0)
            .add_player(PlayerType::Local, 0)
            .expect("add p0")
            .add_player(PlayerType::Local, 1)
            .expect("add p1")
            .start_synctest_session()
            .expect("start synctest");
        Self { session, world, accumulator: 0.0, frame: 0 }
    }

    fn step_one(&mut self) {
        // Both players are local in synctest; we feed both inputs ourselves.
        self.session
            .add_local_input(0, NetInput::from(poll_input_p1()))
            .expect("add input p0");
        self.session
            .add_local_input(1, NetInput::from(poll_input_p2()))
            .expect("add input p1");
        let requests = match self.session.advance_frame() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[synctest] advance_frame error: {e:?}");
                return;
            }
        };
        for req in requests {
            self.handle(req);
        }
    }

    fn handle(&mut self, req: GgrsRequest<GgrsConfig>) {
        match req {
            GgrsRequest::SaveGameState { cell, frame } => {
                let checksum = checksum_world(&self.world);
                cell.save(frame, Some(self.world.clone()), Some(checksum));
            }
            GgrsRequest::LoadGameState { cell, .. } => {
                self.world = cell.load().expect("loaded state present");
            }
            GgrsRequest::AdvanceFrame { inputs } => {
                let p0: Input = inputs[0].0.into();
                let p1: Input = inputs[1].0.into();
                self.world.tick([p0, p1]);
                self.frame += 1;
            }
        }
    }
}

impl Session for SyncTestRunner {
    fn advance(&mut self, frame_dt: f32) {
        self.accumulator += frame_dt;
        while self.accumulator >= sim::DT {
            self.step_one();
            self.accumulator -= sim::DT;
        }
    }

    fn world(&self) -> &World {
        &self.world
    }
}

/// Cheap deterministic hash of the world for SyncTest checksums. Enough to
/// detect divergence; not cryptographic. Hashes the bitwise representation
/// of every f32 we care about so NaN/-0.0 differences would also show up.
fn checksum_world(w: &World) -> u128 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    w.tick.hash(&mut h);
    for s in &w.ships {
        s.pos.x.to_bits().hash(&mut h);
        s.pos.y.to_bits().hash(&mut h);
        s.vel.x.to_bits().hash(&mut h);
        s.vel.y.to_bits().hash(&mut h);
        s.angle.to_bits().hash(&mut h);
        s.angular_vel.to_bits().hash(&mut h);
        s.fuel.to_bits().hash(&mut h);
        s.shields.to_bits().hash(&mut h);
        s.alive.hash(&mut h);
        s.landed.hash(&mut h);
        s.tipped_over.hash(&mut h);
        s.fire_cooldown.to_bits().hash(&mut h);
        s.respawn_ticks.hash(&mut h);
    }
    w.bullets.len().hash(&mut h);
    for b in &w.bullets {
        b.pos.x.to_bits().hash(&mut h);
        b.pos.y.to_bits().hash(&mut h);
        b.vel.x.to_bits().hash(&mut h);
        b.vel.y.to_bits().hash(&mut h);
        b.ttl.to_bits().hash(&mut h);
        b.owner.hash(&mut h);
    }
    w.particles.len().hash(&mut h);
    for p in &w.particles {
        p.pos.x.to_bits().hash(&mut h);
        p.pos.y.to_bits().hash(&mut h);
        p.vel.x.to_bits().hash(&mut h);
        p.vel.y.to_bits().hash(&mut h);
        p.ttl.to_bits().hash(&mut h);
        p.owner.hash(&mut h);
    }
    h.finish() as u128
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
