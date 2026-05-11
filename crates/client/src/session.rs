use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ggrs::{
    Config, GgrsRequest, P2PSession, PlayerType, SessionBuilder, SessionState, SyncTestSession,
};
use macroquad::prelude::{KeyCode, is_key_down};
use matchbox_socket::{PeerId, PeerState, WebRtcSocket};
use sim::{Input, World};

use crate::net_input::NetInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbyPhase {
    Connecting,
    SignalingOpen,
    PeerConnected,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug)]
pub struct LobbyStatus {
    pub remote_peers: usize,
    pub ready: bool,
    pub failed: bool,
    pub signaling_open: bool,
    pub phase: LobbyPhase,
}

/// A driver for the simulation. The main loop calls `advance` once per
/// frame with the wall-clock delta; the session decides how to step `world`.
pub trait Session {
    fn advance(&mut self, frame_dt: f32);
    fn world(&self) -> &World;
}

/// Pluggable input poller. Each tick the session calls it once per local
/// slot to assemble the `Input` bitmask. Decoupling from `is_key_down`
/// makes room for touch/joystick sources without forking the session code.
pub type InputSource = Box<dyn FnMut() -> Input>;

/// Single-machine, fixed-step. Reads inputs from two pluggable sources.
/// Optionally records each input pair via the supplied callback (used for
/// the `--replay` log).
pub struct LocalSession {
    world: World,
    accumulator: f32,
    sources: [InputSource; 2],
    on_tick: Option<Box<dyn FnMut([Input; 2])>>,
}

impl LocalSession {
    pub fn new(world: World, sources: [InputSource; 2]) -> Self {
        Self {
            world,
            accumulator: 0.0,
            sources,
            on_tick: None,
        }
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
            let inputs = [(self.sources[0])(), (self.sources[1])()];
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
/// `Address = PeerId` so the same config is reused for matchbox P2P.
#[derive(Debug)]
pub struct GgrsConfig;

impl Config for GgrsConfig {
    type Input = NetInput;
    type State = World;
    type Address = PeerId;
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
            .with_check_distance(4)
            .with_input_delay(0)
            .add_player(PlayerType::Local, 0)
            .expect("add p0")
            .add_player(PlayerType::Local, 1)
            .expect("add p1")
            .start_synctest_session()
            .expect("start synctest");
        Self {
            session,
            world,
            accumulator: 0.0,
            frame: 0,
        }
    }

    fn step_one(&mut self) {
        // Both players are local in synctest; only the followed ship (0)
        // takes keyboard, the other rides on empty inputs.
        self.session
            .add_local_input(0, NetInput::from(poll_keyboard()))
            .expect("add input p0");
        self.session
            .add_local_input(1, NetInput::from(Input::empty()))
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

/// Live P2P session driven by matchbox WebRTC + GGRS rollback. The socket
/// lives across both phases (waiting for peer / running); the GGRS session
/// is only built once both peers are known.
pub struct P2pRunner {
    socket: WebRtcSocket,
    failed: Arc<AtomicBool>,
    signaling_open: bool,
    session: Option<P2PSession<GgrsConfig>>,
    world: World,
    accumulator: f32,
    local_handles: Vec<usize>,
    local_source: InputSource,
}

impl P2pRunner {
    pub fn new(world: World, room_url: &str, local_source: InputSource) -> Self {
        let (socket, failed) = crate::net::open(room_url);
        Self {
            socket,
            failed,
            signaling_open: false,
            session: None,
            world,
            accumulator: 0.0,
            local_handles: Vec::new(),
            local_source,
        }
    }

    /// First local handle once the session is built — caller picks the ship
    /// to follow with the camera. None until both peers connect.
    pub fn local_handle(&self) -> Option<usize> {
        self.local_handles.first().copied()
    }

    pub fn lobby_status(&self) -> LobbyStatus {
        let remote_peers = self.socket.connected_peers().count();
        let ready = self.session.is_some();
        let failed = self.failed.load(Ordering::Relaxed);
        let signaling_open = self.signaling_open;
        let phase = if failed {
            LobbyPhase::Failed
        } else if ready {
            LobbyPhase::Ready
        } else if remote_peers >= 1 {
            LobbyPhase::PeerConnected
        } else if signaling_open {
            LobbyPhase::SignalingOpen
        } else {
            LobbyPhase::Connecting
        };
        LobbyStatus {
            remote_peers,
            ready,
            failed,
            signaling_open,
            phase,
        }
    }

    pub fn poll(&mut self) {
        if self.session.is_none() {
            self.poll_lobby();
        }
    }

    fn poll_lobby(&mut self) {
        // `try_update_peers` (vs `update_peers`, which panics) lets us surface
        // a closed message-loop as a `failed` flag instead of a crash.
        let updates = match self.socket.try_update_peers() {
            Ok(u) => {
                if !self.signaling_open {
                    println!("[net] signaling open");
                    self.signaling_open = true;
                }
                u
            }
            Err(e) => {
                eprintln!("[net] socket closed during lobby: {e:?}");
                self.failed.store(true, Ordering::Relaxed);
                return;
            }
        };
        for (peer, state) in updates {
            match state {
                PeerState::Connected => println!("[net] peer joined: {peer}"),
                PeerState::Disconnected => println!("[net] peer left: {peer}"),
            }
        }

        let players = self.socket.players();
        if players.len() < 2 {
            return;
        }

        let mut builder = SessionBuilder::<GgrsConfig>::new()
            .with_num_players(2)
            .with_input_delay(2)
            .with_max_prediction_window(8);

        let mut local_handles = Vec::new();
        for (handle, player) in players.iter().enumerate() {
            if matches!(player, PlayerType::Local) {
                local_handles.push(handle);
            }
            builder = builder.add_player(*player, handle).expect("add player");
        }

        let channel = self.socket.take_channel(0).expect("take channel 0");
        let session = builder
            .start_p2p_session(channel)
            .expect("start p2p session");
        println!("[net] starting match; local_handles={local_handles:?}, frame=0");
        self.local_handles = local_handles;
        self.session = Some(session);
    }

    fn step_one(
        world: &mut World,
        session: &mut P2PSession<GgrsConfig>,
        local_handles: &[usize],
        local_source: &mut InputSource,
    ) {
        session.poll_remote_clients();
        for ev in session.events() {
            println!("[net] event: {ev:?}");
        }
        if session.current_state() != SessionState::Running {
            return;
        }

        let i = local_source();
        for &h in local_handles {
            if let Err(e) = session.add_local_input(h, NetInput::from(i)) {
                eprintln!("[net] add_local_input handle={h}: {e:?}");
            }
        }

        match session.advance_frame() {
            Ok(requests) => {
                for req in requests {
                    Self::handle_request(world, req);
                }
            }
            Err(ggrs::GgrsError::PredictionThreshold) => {
                // Remote inputs aren't here yet; let them catch up.
            }
            Err(e) => eprintln!("[net] advance_frame: {e:?}"),
        }
    }

    fn handle_request(world: &mut World, req: GgrsRequest<GgrsConfig>) {
        match req {
            GgrsRequest::SaveGameState { cell, frame } => {
                let checksum = checksum_world(world);
                cell.save(frame, Some(world.clone()), Some(checksum));
            }
            GgrsRequest::LoadGameState { cell, .. } => {
                *world = cell.load().expect("loaded state present");
            }
            GgrsRequest::AdvanceFrame { inputs } => {
                let p0: Input = inputs[0].0.into();
                let p1: Input = inputs[1].0.into();
                world.tick([p0, p1]);
            }
        }
    }
}

impl Session for P2pRunner {
    fn advance(&mut self, frame_dt: f32) {
        if self.session.is_none() {
            self.poll_lobby();
            self.accumulator = 0.0;
            return;
        }

        self.accumulator += frame_dt;
        let session = self.session.as_mut().expect("session present");
        while self.accumulator >= sim::DT {
            Self::step_one(
                &mut self.world,
                session,
                &self.local_handles,
                &mut self.local_source,
            );
            self.accumulator -= sim::DT;
        }
    }

    fn world(&self) -> &World {
        &self.world
    }
}

/// Cheap deterministic hash of the world for GGRS save-state checksums.
/// Used by both SyncTest (in-process) and P2P (cross-peer): a mismatch
/// between the two peers' checksums on the same frame surfaces as a
/// `GgrsEvent::DesyncDetected`. Hashes the bitwise representation of every
/// f32 we care about so NaN / -0.0 differences would also show up.
/// Portable across two machines running the same Rust toolchain because
/// `DefaultHasher` (SipHasher13 with fixed seed) is itself deterministic.
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

/// Arrows + Space — the only keyset. Feeds the followed ship in every
/// mode; the other slot in Local uses `no_input`.
pub fn keyboard() -> InputSource {
    Box::new(poll_keyboard)
}

/// Empty input every tick. Used for slots no human is driving (ship 1 in
/// Local single-follow, ship 1 in SyncTest).
pub fn no_input() -> InputSource {
    Box::new(|| Input::empty())
}

pub fn poll_keyboard() -> Input {
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
    if is_key_down(KeyCode::Space) {
        input |= Input::FIRE;
    }
    input
}
