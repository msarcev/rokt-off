//! Pure deterministic simulation for rokt-off.
//!
//! No I/O, no global state, no `Instant::now()`, no unseeded RNG.
//! Anything that breaks rollback determinism does not belong here.

use bitflags::bitflags;
use glam::Vec2;

pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

pub const SHIP_SIZE: f32 = 14.0;
pub const SHIP_RADIUS: f32 = SHIP_SIZE * 0.7;
pub const SHIP_MASS: f32 = 1.0;
pub const SHIP_INERTIA: f32 = 0.5 * SHIP_MASS * SHIP_SIZE * SHIP_SIZE;
pub const SHIP_FRICTION: f32 = 0.6;

pub const SHIP_THRUST: f32 = 380.0;
pub const SHIP_ROT_SPEED: f32 = 32.5;
pub const SHIP_ANGULAR_DAMPING: f32 = 0.90;
pub const SHIP_LINEAR_DAMPING: f32 = 0.99;
pub const DEFAULT_GRAVITY: f32 = 90.0;
pub const FUEL_MAX: f32 = 1000.0;
pub const FUEL_BURN_PER_SEC: f32 = 80.0;
pub const SHIELD_MAX: f32 = 100.0;

pub const IMPACT_DAMAGE_SCALE: f32 = 0.0005;
pub const SCRAPE_THRESHOLD: f32 = 50.0;
pub const EXPLOSION_REF_SPEED: f32 = 350.0;
pub const COLLISION_BOUNCE: f32 = 0.3;
pub const FUEL_REFILL_PER_SEC: f32 = 600.0;
pub const SHIELD_RECHARGE_PER_SEC: f32 = 60.0;

pub const UPRIGHT_ANGLE: f32 = -std::f32::consts::FRAC_PI_2;
pub const SETTLED_ANGLE_TOL: f32 = 0.18;
pub const SETTLED_DELAY_TICKS: u32 = 45;
pub const LIFTOFF_VELOCITY: f32 = 30.0;
pub const BOUNCE_RESTITUTION: f32 = 0.25;
pub const BOUNCE_FLOOR: f32 = 10.0;
pub const PAD_SLAM_SPEED: f32 = 200.0;
pub const PAD_LATERAL_FRICTION_FLOOR: f32 = 80.0;
pub const PAD_LATERAL_RESTITUTION: f32 = 0.25;
// cos(30°): contacts whose normal points within 30° of straight-up trigger
// landing response (settle, tip, friction, landed=true) instead of bounce.
pub const LANDABLE_DOT: f32 = 0.866;
// Tilt threshold past which the ship tips over instead of settling.
// Tuning knob: bigger = more forgiving. Reference: 0.39 = wing edge
// vertical, 0.79 = CoM over contact wing, 1.57 = wing line vertical.
pub const TIP_OVER_ANGLE: f32 = 0.70;
// Final lying-flat tilt: side edge (nose-to-wing) flush with pad.
// π/2 + atan(0.7 / 1.7) for the current triangle proportions.
pub const TIP_FLAT_ANGLE: f32 = 1.962;
// Settle assist: gentle restoring torque toward upright (or flat-tipped)
// when the ship is stably resting on a near-horizontal surface within
// the angular basin.
pub const SETTLE_NORMAL_Y_MAX: f32 = -0.95;
pub const SETTLE_VEL: f32 = 60.0;
pub const SETTLE_AV: f32 = 2.0;
pub const SETTLE_RESTORING_RATE: f32 = 0.33;
pub const SETTLE_AV_CAP: f32 = 1.5;
pub const CHIP_DAMAGE_PER_BOUNCE: f32 = 1.5;
pub const SHIP_RAM_DAMAGE_SCALE: f32 = 1.35;
pub const WALL_CONTACT_DPS: f32 = 30.0;
pub const TIP_DMG_BASE: f32 = 2.5;
pub const TIP_DMG_RAMP: f32 = 0.2;

pub const MUZZLE_SPEED: f32 = 600.0;
pub const BULLET_TTL: f32 = 1.5;
pub const FIRE_COOLDOWN: f32 = 0.10;
pub const MAX_BULLETS: usize = 64;
pub const BULLET_DAMAGE: f32 = 20.0;

pub const RESPAWN_TICKS: u32 = 60;

pub const MAX_PARTICLES: usize = 512;
pub const EXPLOSION_PARTICLE_COUNT: usize = 350;
pub const PARTICLE_SPEED_MIN: f32 = 80.0;
pub const PARTICLE_SPEED_MAX: f32 = 240.0;
pub const PARTICLE_TTL_MIN: f32 = 1.5;
pub const PARTICLE_TTL_MAX: f32 = 2.0;

pub const THRUST_PARTICLES_PER_TICK: u32 = 4;
pub const THRUST_PARTICLE_SPEED_MIN: f32 = 140.0;
pub const THRUST_PARTICLE_SPEED_MAX: f32 = 220.0;
pub const THRUST_PARTICLE_TTL_MIN: f32 = 0.30;
pub const THRUST_PARTICLE_TTL_MAX: f32 = 0.50;
pub const THRUST_PARTICLE_SPREAD: f32 = 0.3;
pub const THRUST_EMIT_OFFSET: f32 = 0.9;

pub const PARTICLE_RADIUS: f32 = 2.5;
pub const PARTICLE_HIT_DAMAGE_THRUST: f32 = 0.25;
pub const PARTICLE_HIT_DAMAGE_EXPLOSION: f32 = 1.0;
pub const PARTICLE_HIT_IMPULSE_SCALE: f32 = 0.015;

pub const DEFAULT_SEED: u64 = 0xDEAD_BEEF_C0DE_F00D;

bitflags! {
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Input: u8 {
        const THRUST       = 0b0000_0001;
        const ROTATE_LEFT  = 0b0000_0010;
        const ROTATE_RIGHT = 0b0000_0100;
        const FIRE         = 0b0000_1000;
    }
}

#[derive(Clone, Debug)]
pub struct Ship {
    pub pos: Vec2,
    pub vel: Vec2,
    /// Radians. 0 = facing right; ship nose points along (cos, sin).
    pub angle: f32,
    pub angular_vel: f32,
    pub fuel: f32,
    pub shields: f32,
    pub alive: bool,
    pub landed: bool,
    pub landed_on_pad: bool,
    pub tipped_over: bool,
    pub tipped_ticks: u32,
    pub fire_cooldown: f32,
    pub respawn_ticks: u32,
    pub settled_ticks: u32,
}

impl Ship {
    pub fn new(pos: Vec2, angle: f32) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            angle,
            angular_vel: 0.0,
            fuel: FUEL_MAX,
            shields: SHIELD_MAX,
            alive: true,
            landed: false,
            landed_on_pad: false,
            tipped_over: false,
            tipped_ticks: 0,
            fire_cooldown: 0.0,
            respawn_ticks: 0,
            settled_ticks: 0,
        }
    }

    /// Unit vector pointing out the nose of the ship.
    pub fn forward(&self) -> Vec2 {
        Vec2::new(self.angle.cos(), self.angle.sin())
    }

    /// World-space triangle vertices: `[nose, left_wing, right_wing]`.
    /// Mirrors the renderer geometry in `client/src/main.rs`.
    pub fn triangle_vertices(&self) -> [Vec2; 3] {
        let (cos, sin) = (self.angle.cos(), self.angle.sin());
        let rot = |dx: f32, dy: f32| {
            self.pos + Vec2::new(cos * dx - sin * dy, sin * dx + cos * dy) * SHIP_SIZE
        };
        [rot(1.0, 0.0), rot(-0.7, 0.7), rot(-0.7, -0.7)]
    }
}

/// Deterministic xorshift64 RNG. Same bytes on every platform.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Xorshift requires a non-zero state.
        Self(if seed == 0 { 1 } else { seed })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        // Take the top 24 bits to fill an f32 mantissa exactly.
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }
}

/// 1-bit-per-pixel collision mask. Out-of-bounds samples read as solid, so
/// the mask edge acts as the world boundary.
#[derive(Clone, Debug)]
pub struct BitMask {
    pub width: u32,
    pub height: u32,
    bits: Vec<u64>,
}

impl BitMask {
    pub fn new(width: u32, height: u32, fill: bool) -> Self {
        let total = (width as usize) * (height as usize);
        let words = total.div_ceil(64);
        let word = if fill { u64::MAX } else { 0 };
        Self {
            width,
            height,
            bits: vec![word; words],
        }
    }

    pub fn is_solid(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || (x as u32) >= self.width || (y as u32) >= self.height {
            return true;
        }
        let idx = (y as u32 as usize) * (self.width as usize) + (x as usize);
        (self.bits[idx / 64] >> (idx % 64)) & 1 == 1
    }

    pub fn set(&mut self, x: i32, y: i32, solid: bool) {
        if x < 0 || y < 0 || (x as u32) >= self.width || (y as u32) >= self.height {
            return;
        }
        let idx = (y as u32 as usize) * (self.width as usize) + (x as usize);
        let bit = 1u64 << (idx % 64);
        if solid {
            self.bits[idx / 64] |= bit;
        } else {
            self.bits[idx / 64] &= !bit;
        }
    }

    pub fn from_wall_rects(width: u32, height: u32, rects: &[Rect]) -> Self {
        let mut mask = Self::new(width, height, false);
        for rect in rects {
            if rect.kind != RectKind::Wall {
                continue;
            }
            let x0 = (rect.min.x as i32).max(0);
            let y0 = (rect.min.y as i32).max(0);
            let x1 = (rect.max.x as i32).min(width as i32);
            let y1 = (rect.max.y as i32).min(height as i32);
            for y in y0..y1 {
                for x in x0..x1 {
                    mask.set(x, y, true);
                }
            }
        }
        mask
    }

    /// Decode a PNG and treat any non-transparent pixel as solid.
    pub fn from_png_bytes(bytes: &[u8]) -> Result<Self, image::ImageError> {
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut mask = Self::new(w, h, false);
        for (x, y, pixel) in rgba.enumerate_pixels() {
            if pixel.0[3] != 0 {
                mask.set(x as i32, y as i32, true);
            }
        }
        Ok(mask)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RectKind {
    Wall,
    Pad,
}

#[derive(Copy, Clone, Debug)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
    pub kind: RectKind,
}

#[derive(Clone, Debug)]
pub struct Level {
    pub size: Vec2,
    pub gravity: f32,
    pub spawn_points: [Vec2; 2],
    pub rects: Vec<Rect>,
    pub mask: BitMask,
}

impl Default for Level {
    fn default() -> Self {
        let size = Vec2::new(1280.0, 720.0);
        let wall_rects = [
            Rect {
                min: Vec2::new(0.0, 700.0),
                max: Vec2::new(size.x, size.y),
                kind: RectKind::Wall,
            },
            Rect {
                min: Vec2::ZERO,
                max: Vec2::new(size.x, 20.0),
                kind: RectKind::Wall,
            },
            Rect {
                min: Vec2::ZERO,
                max: Vec2::new(20.0, size.y),
                kind: RectKind::Wall,
            },
            Rect {
                min: Vec2::new(size.x - 20.0, 0.0),
                max: size,
                kind: RectKind::Wall,
            },
        ];
        let mask = BitMask::from_wall_rects(size.x as u32, size.y as u32, &wall_rects);
        let rects = vec![
            // P1 landing pad (centered on spawn x=240)
            Rect {
                min: Vec2::new(180.0, 620.0),
                max: Vec2::new(300.0, 640.0),
                kind: RectKind::Pad,
            },
            // P2 landing pad (centered on spawn x=1040)
            Rect {
                min: Vec2::new(980.0, 620.0),
                max: Vec2::new(1100.0, 640.0),
                kind: RectKind::Pad,
            },
        ];
        Self {
            size,
            gravity: DEFAULT_GRAVITY,
            spawn_points: [Vec2::new(240.0, 540.0), Vec2::new(1040.0, 540.0)],
            rects,
            mask,
        }
    }
}

#[derive(Debug)]
pub enum LevelLoadError {
    MaskDecode(image::ImageError),
    TmxParse(String),
    MissingObjectLayer,
    MissingSpawn(i32),
    SpawnOutOfBounds(i32),
    BadPropertyType {
        name: &'static str,
        expected: &'static str,
    },
}

impl core::fmt::Display for LevelLoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MaskDecode(e) => write!(f, "mask PNG decode: {e}"),
            Self::TmxParse(msg) => write!(f, "tmx parse: {msg}"),
            Self::MissingObjectLayer => write!(f, "tmx has no object layer"),
            Self::MissingSpawn(p) => write!(f, "tmx missing spawn for player {p}"),
            Self::SpawnOutOfBounds(p) => write!(f, "spawn {p} outside mask bounds"),
            Self::BadPropertyType { name, expected } => {
                write!(f, "property `{name}` must be {expected}")
            }
        }
    }
}

impl std::error::Error for LevelLoadError {}

impl Level {
    /// Build a Level from a mask PNG (alpha > 0 = solid) and a Tiled .tmx string.
    ///
    /// The mask PNG's dimensions are authoritative for the world size — the .tmx's
    /// own width/height are advisory. Objects in the .tmx (one object layer) describe:
    ///   - class="spawn", point shape, custom property `player: int` (0 or 1)
    ///   - class="pad",   rect shape  → landing pad
    /// Map-level custom properties: `name: string`, `gravity: float`.
    pub fn from_bytes(mask_png: &[u8], tmx: &[u8]) -> Result<Self, LevelLoadError> {
        let mask = BitMask::from_png_bytes(mask_png).map_err(LevelLoadError::MaskDecode)?;
        let size = Vec2::new(mask.width as f32, mask.height as f32);

        let map = parse_tmx(tmx)?;

        let mut gravity = DEFAULT_GRAVITY;
        if let Some(v) = map.properties.get("gravity") {
            match v {
                &tiled::PropertyValue::FloatValue(f) => gravity = f,
                &tiled::PropertyValue::IntValue(i) => gravity = i as f32,
                _ => {
                    return Err(LevelLoadError::BadPropertyType {
                        name: "gravity",
                        expected: "float",
                    });
                }
            }
        }

        let mut spawns: [Option<Vec2>; 2] = [None, None];
        let mut rects: Vec<Rect> = Vec::new();

        let object_layer = map
            .layers()
            .find_map(|l| l.as_object_layer())
            .ok_or(LevelLoadError::MissingObjectLayer)?;

        for obj in object_layer.objects() {
            match obj.user_type.as_str() {
                "spawn" => {
                    let player = match obj.properties.get("player") {
                        Some(&tiled::PropertyValue::IntValue(i)) => i,
                        _ => {
                            return Err(LevelLoadError::BadPropertyType {
                                name: "player",
                                expected: "int",
                            });
                        }
                    };
                    if player != 0 && player != 1 {
                        return Err(LevelLoadError::MissingSpawn(player));
                    }
                    spawns[player as usize] = Some(Vec2::new(obj.x, obj.y));
                }
                "pad" => {
                    if let tiled::ObjectShape::Rect { width, height } = obj.shape {
                        rects.push(Rect {
                            min: Vec2::new(obj.x, obj.y),
                            max: Vec2::new(obj.x + width, obj.y + height),
                            kind: RectKind::Pad,
                        });
                    }
                }
                _ => {}
            }
        }

        let spawn_points = [
            spawns[0].ok_or(LevelLoadError::MissingSpawn(0))?,
            spawns[1].ok_or(LevelLoadError::MissingSpawn(1))?,
        ];
        for (i, s) in spawn_points.iter().enumerate() {
            if s.x < 0.0 || s.y < 0.0 || s.x >= size.x || s.y >= size.y {
                return Err(LevelLoadError::SpawnOutOfBounds(i as i32));
            }
        }

        Ok(Self {
            size,
            gravity,
            spawn_points,
            rects,
            mask,
        })
    }
}

/// Parse a .tmx byte slice into a `tiled::Map` via an in-memory ResourceReader.
fn parse_tmx(tmx: &[u8]) -> Result<tiled::Map, LevelLoadError> {
    use std::io::Cursor;
    use std::path::{Path, PathBuf};

    struct InMemReader {
        path: PathBuf,
        bytes: Vec<u8>,
    }
    impl tiled::ResourceReader for InMemReader {
        type Resource = Cursor<Vec<u8>>;
        type Error = std::io::Error;
        fn read_from(&mut self, p: &Path) -> std::result::Result<Self::Resource, Self::Error> {
            if p == self.path {
                Ok(Cursor::new(self.bytes.clone()))
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("rokt-off in-mem reader: only `{}` is known", self.path.display()),
                ))
            }
        }
    }

    let virtual_path = PathBuf::from("level.tmx");
    let reader = InMemReader {
        path: virtual_path.clone(),
        bytes: tmx.to_vec(),
    };
    let mut loader = tiled::Loader::with_reader(reader);
    loader
        .load_tmx_map(&virtual_path)
        .map_err(|e| LevelLoadError::TmxParse(e.to_string()))
}

#[derive(Copy, Clone, Debug)]
pub struct Bullet {
    pub pos: Vec2,
    pub vel: Vec2,
    pub ttl: f32,
    pub owner: u8,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Thrust,
    Explosion,
}

#[derive(Copy, Clone, Debug)]
pub struct Particle {
    pub pos: Vec2,
    pub vel: Vec2,
    pub ttl: f32,
    pub max_ttl: f32,
    pub owner: u8,
    pub kind: ParticleKind,
}

#[derive(Copy, Clone, Debug)]
struct WallImpact {
    pos: Vec2,
    normal: Vec2,
    speed: f32,
}

#[derive(Clone, Debug)]
pub struct World {
    pub level: Level,
    pub ships: [Ship; 2],
    pub bullets: Vec<Bullet>,
    pub particles: Vec<Particle>,
    pub tick: u64,
    pub rng: Rng,
}

impl World {
    pub fn new(level: Level) -> Self {
        Self::with_seed(level, DEFAULT_SEED)
    }

    pub fn with_seed(level: Level, seed: u64) -> Self {
        let ships = [
            Ship::new(level.spawn_points[0], -std::f32::consts::FRAC_PI_2),
            Ship::new(level.spawn_points[1], -std::f32::consts::FRAC_PI_2),
        ];
        Self {
            level,
            ships,
            bullets: Vec::with_capacity(MAX_BULLETS),
            particles: Vec::with_capacity(MAX_PARTICLES),
            tick: 0,
            rng: Rng::new(seed),
        }
    }

    /// Advance the world by one fixed-step tick. Pure function of (self, inputs).
    pub fn tick(&mut self, inputs: [Input; 2]) {
        let gravity = Vec2::new(0.0, self.level.gravity);
        let was_alive = [self.ships[0].alive, self.ships[1].alive];
        let mut wall_impact: [Option<WallImpact>; 2] = [None, None];
        let mut thrusted = [false; 2];
        for (idx, (ship, input)) in self.ships.iter_mut().zip(inputs.iter()).enumerate() {
            ship.fire_cooldown = (ship.fire_cooldown - DT).max(0.0);
            if !ship.alive {
                continue;
            }
            let was_landed = ship.landed;
            let was_landed_on_pad = ship.landed_on_pad;
            ship.landed = false;
            ship.landed_on_pad = false;
            thrusted[idx] = step_ship(ship, *input, gravity, was_landed);
            resolve_ship_rects(ship, &self.level.rects, &mut wall_impact[idx]);
            resolve_ship_mask(ship, &self.level.mask, &mut wall_impact[idx]);

            // Stay-landed sticky: pivot rotation can briefly lift the
            // contact vertex; treat the ship as still landed unless it's
            // moving up faster than LIFTOFF_VELOCITY.
            let force_stay_landed = ship.tipped_over && !thrusted[idx];
            if was_landed && !ship.landed && (force_stay_landed || ship.vel.y > -LIFTOFF_VELOCITY) {
                ship.landed = true;
                if was_landed_on_pad {
                    ship.landed_on_pad = true;
                }
            }

            let tilt = angle_diff(ship.angle, UPRIGHT_ANGLE);
            ship.tipped_over = ship.landed && tilt.abs() > TIP_OVER_ANGLE;
            if !ship.tipped_over {
                ship.tipped_ticks = 0;
            }

            if ship.tipped_over {
                ship.settled_ticks = 0;
                ship.tipped_ticks = ship.tipped_ticks.saturating_add(1);
                let dps = TIP_DMG_BASE + ship.tipped_ticks as f32 * TIP_DMG_RAMP;
                ship.shields = (ship.shields - dps * DT).max(0.0);
                if ship.shields <= 0.0 {
                    ship.alive = false;
                }
            } else if ship.landed_on_pad {
                let tilt = angle_diff(ship.angle, UPRIGHT_ANGLE);
                if tilt.abs() < SETTLED_ANGLE_TOL {
                    ship.settled_ticks = ship.settled_ticks.saturating_add(1);
                    if ship.settled_ticks > SETTLED_DELAY_TICKS {
                        ship.fuel = (ship.fuel + FUEL_REFILL_PER_SEC * DT).min(FUEL_MAX);
                        ship.shields =
                            (ship.shields + SHIELD_RECHARGE_PER_SEC * DT).min(SHIELD_MAX);
                    }
                } else {
                    ship.settled_ticks = 0;
                }
            } else {
                ship.settled_ticks = 0;
            }

            if input.contains(Input::FIRE) && ship.fire_cooldown <= 0.0 {
                spawn_bullet(&mut self.bullets, ship, idx as u8);
                ship.fire_cooldown = FIRE_COOLDOWN;
            }
        }

        // Ships are solid: separate any pair that overlaps and bounce them apart.
        resolve_ship_ship(&mut self.ships);

        // Emit thrust particles for ships that fired their main engine.
        for idx in 0..self.ships.len() {
            if thrusted[idx] {
                spawn_thrust(
                    &mut self.particles,
                    &mut self.rng,
                    &self.ships[idx],
                    idx as u8,
                );
            }
        }

        // Advance bullets, drop expired or impacted.
        for b in self.bullets.iter_mut() {
            b.pos += b.vel * DT;
            b.ttl -= DT;
        }
        resolve_bullets(&mut self.bullets, &mut self.ships, &self.level);
        self.bullets.retain(|b| b.ttl > 0.0);

        // Newly-dead ships explode.
        for (idx, ship) in self.ships.iter().enumerate() {
            if was_alive[idx] && !ship.alive {
                spawn_explosion(
                    &mut self.particles,
                    &mut self.rng,
                    ship.pos,
                    ship.vel,
                    idx as u8,
                    wall_impact[idx],
                );
            }
        }

        for (idx, ship) in self.ships.iter_mut().enumerate() {
            if ship.alive {
                continue;
            }
            if ship.respawn_ticks == 0 {
                ship.respawn_ticks = RESPAWN_TICKS;
            } else {
                ship.respawn_ticks -= 1;
                if ship.respawn_ticks == 0 {
                    *ship = Ship::new(self.level.spawn_points[idx], -std::f32::consts::FRAC_PI_2);
                }
            }
        }

        // Advance particles under gravity, drop expired.
        for p in self.particles.iter_mut() {
            p.pos += p.vel * DT;
            p.vel += gravity * DT;
            p.ttl -= DT;
        }
        resolve_particles(&mut self.particles, &mut self.ships, &self.level);
        self.particles.retain(|p| p.ttl > 0.0);

        self.tick += 1;
    }
}

fn spawn_explosion(
    particles: &mut Vec<Particle>,
    rng: &mut Rng,
    ship_pos: Vec2,
    ship_vel: Vec2,
    owner: u8,
    impact: Option<WallImpact>,
) {
    use std::f32::consts::{PI, TAU};

    let (origin, base_vel, dir_bias, intensity) = match impact {
        Some(WallImpact { pos, normal, speed }) => {
            let i = (speed / EXPLOSION_REF_SPEED).clamp(0.6, 2.2);
            (pos, Vec2::ZERO, Some(normal), i)
        }
        None => (ship_pos, ship_vel, None, 1.0),
    };

    let count = (EXPLOSION_PARTICLE_COUNT as f32 * intensity) as usize;
    let speed_scale = intensity;
    let ttl_scale = intensity.sqrt();

    for _ in 0..count {
        let angle = match dir_bias {
            Some(n) => n.y.atan2(n.x) + (rng.next_f32() - 0.5) * PI,
            None => rng.next_f32() * TAU,
        };
        let dir = Vec2::new(angle.cos(), angle.sin());
        let speed = rng.range(PARTICLE_SPEED_MIN, PARTICLE_SPEED_MAX) * speed_scale;
        let ttl = rng.range(PARTICLE_TTL_MIN, PARTICLE_TTL_MAX) * ttl_scale;
        let p = Particle {
            pos: origin,
            vel: base_vel + dir * speed,
            ttl,
            max_ttl: ttl,
            owner,
            kind: ParticleKind::Explosion,
        };
        if particles.len() >= MAX_PARTICLES {
            particles.remove(0);
        }
        particles.push(p);
    }
}

fn spawn_thrust(particles: &mut Vec<Particle>, rng: &mut Rng, ship: &Ship, owner: u8) {
    let forward = ship.forward();
    let perp = Vec2::new(-forward.y, forward.x);
    let base = ship.pos - forward * (SHIP_SIZE * THRUST_EMIT_OFFSET);
    for _ in 0..THRUST_PARTICLES_PER_TICK {
        let speed = rng.range(THRUST_PARTICLE_SPEED_MIN, THRUST_PARTICLE_SPEED_MAX);
        let spread = rng.range(-THRUST_PARTICLE_SPREAD, THRUST_PARTICLE_SPREAD);
        let ttl = rng.range(THRUST_PARTICLE_TTL_MIN, THRUST_PARTICLE_TTL_MAX);
        let dir = (-forward + perp * spread).normalize_or_zero();
        let p = Particle {
            pos: base,
            vel: ship.vel + dir * speed,
            ttl,
            max_ttl: ttl,
            owner,
            kind: ParticleKind::Thrust,
        };
        if particles.len() >= MAX_PARTICLES {
            particles.remove(0);
        }
        particles.push(p);
    }
}

fn resolve_particles(particles: &mut [Particle], ships: &mut [Ship; 2], level: &Level) {
    let r = SHIP_RADIUS + PARTICLE_RADIUS;
    let r_sq = r * r;
    for p in particles.iter_mut() {
        if p.ttl <= 0.0 {
            continue;
        }
        if level.rects.iter().any(|rect| point_in_rect(p.pos, rect))
            || level.mask.is_solid(p.pos.x as i32, p.pos.y as i32)
        {
            p.ttl = 0.0;
            continue;
        }
        for (idx, ship) in ships.iter_mut().enumerate() {
            if !ship.alive || idx as u8 == p.owner {
                continue;
            }
            if (p.pos - ship.pos).length_squared() <= r_sq {
                let damage = match p.kind {
                    ParticleKind::Thrust => PARTICLE_HIT_DAMAGE_THRUST,
                    ParticleKind::Explosion => PARTICLE_HIT_DAMAGE_EXPLOSION,
                };
                ship.shields = (ship.shields - damage).max(0.0);
                if ship.shields <= 0.0 {
                    ship.alive = false;
                }
                ship.vel += (p.vel - ship.vel) * PARTICLE_HIT_IMPULSE_SCALE;
                p.ttl = 0.0;
                break;
            }
        }
    }
}

fn resolve_ship_ship(ships: &mut [Ship; 2]) {
    let [a, b] = ships;
    if !a.alive || !b.alive {
        return;
    }

    let a_to_b = b.pos - a.pos;
    let r = SHIP_SIZE * 2.0;
    if a_to_b.length_squared() >= r * r {
        return;
    }

    let tri_a = a.triangle_vertices();
    let tri_b = b.triangle_vertices();
    let Some((normal, depth)) = sat_triangles(&tri_a, &tri_b, a_to_b) else {
        return;
    };

    a.pos -= normal * (depth * 0.5);
    b.pos += normal * (depth * 0.5);

    let v_rel = (a.vel - b.vel).dot(normal);
    if v_rel <= 0.0 {
        return;
    }

    let j = (1.0 + COLLISION_BOUNCE) * v_rel * 0.5;
    a.vel -= normal * j;
    b.vel += normal * j;

    let chip = if v_rel > BOUNCE_FLOOR {
        CHIP_DAMAGE_PER_BOUNCE
    } else {
        0.0
    };
    let over = (v_rel - SCRAPE_THRESHOLD).max(0.0);
    let extra = over * over * IMPACT_DAMAGE_SCALE;
    let total = (chip + extra) * SHIP_RAM_DAMAGE_SCALE;
    if total > 0.0 {
        for s in [a, b] {
            s.shields = (s.shields - total).max(0.0);
            if s.shields <= 0.0 {
                s.alive = false;
            }
        }
    }
}

/// SAT on two triangles. Returns (axis a→b, penetration depth) or `None` if separated.
fn sat_triangles(a: &[Vec2; 3], b: &[Vec2; 3], a_to_b: Vec2) -> Option<(Vec2, f32)> {
    let mut min_depth = f32::INFINITY;
    let mut min_axis = Vec2::ZERO;

    for tri in [a, b] {
        for i in 0..3 {
            let edge = tri[(i + 1) % 3] - tri[i];
            let n = Vec2::new(-edge.y, edge.x);
            let len_sq = n.length_squared();
            if len_sq <= f32::EPSILON {
                continue;
            }
            let axis = n / len_sq.sqrt();

            let (a_min, a_max) = project_triangle(a, axis);
            let (b_min, b_max) = project_triangle(b, axis);

            if a_max < b_min || b_max < a_min {
                return None;
            }
            let overlap = a_max.min(b_max) - a_min.max(b_min);
            if overlap < min_depth {
                min_depth = overlap;
                min_axis = axis;
            }
        }
    }

    let oriented = if a_to_b.dot(min_axis) >= 0.0 {
        min_axis
    } else {
        -min_axis
    };
    Some((oriented, min_depth))
}

fn project_triangle(tri: &[Vec2; 3], axis: Vec2) -> (f32, f32) {
    let p0 = tri[0].dot(axis);
    let p1 = tri[1].dot(axis);
    let p2 = tri[2].dot(axis);
    (p0.min(p1).min(p2), p0.max(p1).max(p2))
}

fn resolve_bullets(bullets: &mut [Bullet], ships: &mut [Ship; 2], level: &Level) {
    for b in bullets.iter_mut() {
        if b.ttl <= 0.0 {
            continue;
        }
        if level.rects.iter().any(|r| point_in_rect(b.pos, r))
            || level.mask.is_solid(b.pos.x as i32, b.pos.y as i32)
        {
            b.ttl = 0.0;
            continue;
        }
        // Bullet vs other ship.
        for (idx, ship) in ships.iter_mut().enumerate() {
            if !ship.alive || idx as u8 == b.owner {
                continue;
            }
            if (b.pos - ship.pos).length_squared() <= SHIP_RADIUS * SHIP_RADIUS {
                ship.shields = (ship.shields - BULLET_DAMAGE).max(0.0);
                if ship.shields <= 0.0 {
                    ship.alive = false;
                }
                b.ttl = 0.0;
                break;
            }
        }
    }
}

fn point_in_rect(p: Vec2, r: &Rect) -> bool {
    p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y
}

fn spawn_bullet(bullets: &mut Vec<Bullet>, ship: &Ship, owner: u8) {
    let forward = ship.forward();
    let bullet = Bullet {
        pos: ship.pos + forward * (SHIP_RADIUS * 1.5),
        vel: ship.vel + forward * MUZZLE_SPEED,
        ttl: BULLET_TTL,
        owner,
    };
    if bullets.len() >= MAX_BULLETS {
        // Fixed pool: drop the oldest.
        bullets.remove(0);
    }
    bullets.push(bullet);
}

/// Resolve ship vs level rects. Walls use the ship's circle hull; pads use
/// the rotated triangle's lowest vertex so contact behaviour matches what
/// the player sees. Mutates `ship` in place.
fn resolve_ship_rects(ship: &mut Ship, rects: &[Rect], impact: &mut Option<WallImpact>) {
    for rect in rects {
        match rect.kind {
            RectKind::Pad => resolve_ship_pad(ship, rect, impact),
            RectKind::Wall => resolve_ship_wall(ship, rect, impact),
        }
    }
}

fn resolve_ship_mask(ship: &mut Ship, mask: &BitMask, impact: &mut Option<WallImpact>) {
    let verts = ship.triangle_vertices();

    let mut min_p = verts[0];
    let mut max_p = verts[0];
    for v in &verts[1..] {
        min_p = min_p.min(*v);
        max_p = max_p.max(*v);
    }
    let x0 = min_p.x.floor() as i32;
    let x1 = max_p.x.ceil() as i32;
    let y0 = min_p.y.floor() as i32;
    let y1 = max_p.y.ceil() as i32;

    let mut overlap: Vec<Vec2> = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            if point_in_triangle(p, &verts) && mask.is_solid(x, y) {
                overlap.push(p);
            }
        }
    }

    if overlap.is_empty() {
        return;
    }

    // Per-pixel boundary normal: for each overlap pixel, use its 4-neighbour
    // gradient to point away from solid. Interior pixels (all neighbours
    // solid) abstain. Average the per-pixel normals.
    let mut grad_sum = Vec2::ZERO;
    let mut boundary_count = 0;
    for p in &overlap {
        let x = p.x as i32;
        let y = p.y as i32;
        let s_l = mask.is_solid(x - 1, y) as i32;
        let s_r = mask.is_solid(x + 1, y) as i32;
        let s_u = mask.is_solid(x, y - 1) as i32;
        let s_d = mask.is_solid(x, y + 1) as i32;
        let g = Vec2::new((s_r - s_l) as f32, (s_d - s_u) as f32);
        if g.length_squared() > 0.0 {
            grad_sum -= g.normalize();
            boundary_count += 1;
        }
    }
    let normal = if boundary_count > 0 && grad_sum.length_squared() > 0.0 {
        grad_sum.normalize()
    } else {
        let v_len = ship.vel.length();
        if v_len > 1.0 {
            -ship.vel / v_len
        } else {
            Vec2::new(0.0, -1.0)
        }
    };

    let max_steps = SHIP_SIZE as i32;
    let mut depth = 0.0f32;
    for p in &overlap {
        for step in 1..=max_steps {
            let pp = *p + normal * step as f32;
            if !mask.is_solid(pp.x as i32, pp.y as i32) {
                if (step as f32) > depth {
                    depth = step as f32;
                }
                break;
            }
            if step == max_steps && (step as f32) > depth {
                depth = step as f32;
            }
        }
    }

    if depth <= 0.0 {
        return;
    }

    // Contact pixel is captured before the position push so the response
    // sees where contact actually was.
    let contact = overlap
        .iter()
        .copied()
        .max_by(|a, b| {
            (*a - ship.pos)
                .dot(-normal)
                .partial_cmp(&(*b - ship.pos).dot(-normal))
                .unwrap_or(core::cmp::Ordering::Equal)
        })
        .unwrap_or(ship.pos);

    ship.pos += normal * depth;
    apply_contact(ship, contact, normal, impact);
}

/// Sign-of-cross test, accepting either winding by requiring all three signs to agree.
fn point_in_triangle(p: Vec2, t: &[Vec2; 3]) -> bool {
    let s1 = (t[1] - t[0]).perp_dot(p - t[0]);
    let s2 = (t[2] - t[1]).perp_dot(p - t[1]);
    let s3 = (t[0] - t[2]).perp_dot(p - t[2]);
    let pos = s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0;
    let neg = s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0;
    pos || neg
}

fn resolve_ship_pad(ship: &mut Ship, pad: &Rect, impact: &mut Option<WallImpact>) {
    let pad_top = pad.min.y;

    // CoM at or past the pad surface → side/bottom hit, fall through to wall.
    if ship.pos.y >= pad_top {
        resolve_ship_wall(ship, pad, impact);
        return;
    }

    // Lowest triangle vertex horizontally inside the pad = the wing tip
    // or nose actually touching down. Pad detection stays vertex-based so
    // sharply tilted approaches register at the actual contact point.
    let verts = ship.triangle_vertices();
    let mut lowest_y = f32::NEG_INFINITY;
    let mut lowest = Vec2::ZERO;
    for v in verts.iter() {
        if v.x >= pad.min.x && v.x <= pad.max.x && v.y > lowest_y {
            lowest_y = v.y;
            lowest = *v;
        }
    }
    if lowest_y < pad_top {
        return;
    }

    let penetration = lowest_y - pad_top;
    ship.pos.y -= penetration;

    apply_contact(
        ship,
        Vec2::new(lowest.x, pad_top),
        Vec2::new(0.0, -1.0),
        impact,
    );
    if ship.landed {
        ship.landed_on_pad = true;
    }
}

fn resolve_ship_wall(ship: &mut Ship, rect: &Rect, impact: &mut Option<WallImpact>) {
    let closest = ship.pos.clamp(rect.min, rect.max);
    let delta = ship.pos - closest;
    let dist_sq = delta.length_squared();
    if dist_sq >= SHIP_RADIUS * SHIP_RADIUS {
        return;
    }

    let (normal, depth) = if dist_sq > f32::EPSILON {
        let dist = dist_sq.sqrt();
        (delta / dist, SHIP_RADIUS - dist)
    } else {
        // Tunnelled inside the rect. Pop opposite the dominant motion
        // axis (came from that side); fall back to shortest exit.
        let dx_left = ship.pos.x - rect.min.x;
        let dx_right = rect.max.x - ship.pos.x;
        let dy_up = ship.pos.y - rect.min.y;
        let dy_down = rect.max.y - ship.pos.y;
        let v = ship.vel;
        let (n, side_dist) = if v.length_squared() > 1.0 && v.x.abs() > v.y.abs() {
            if v.x > 0.0 {
                (Vec2::new(-1.0, 0.0), dx_left)
            } else {
                (Vec2::new(1.0, 0.0), dx_right)
            }
        } else if v.length_squared() > 1.0 {
            if v.y > 0.0 {
                (Vec2::new(0.0, -1.0), dy_up)
            } else {
                (Vec2::new(0.0, 1.0), dy_down)
            }
        } else {
            let m = dx_left.min(dx_right).min(dy_up).min(dy_down);
            if m == dx_left {
                (Vec2::new(-1.0, 0.0), dx_left)
            } else if m == dx_right {
                (Vec2::new(1.0, 0.0), dx_right)
            } else if m == dy_up {
                (Vec2::new(0.0, -1.0), dy_up)
            } else {
                (Vec2::new(0.0, 1.0), dy_down)
            }
        };
        (n, side_dist + SHIP_RADIUS)
    };

    ship.pos += normal * depth;
    apply_contact(ship, ship.pos - normal * SHIP_RADIUS, normal, impact);
}

struct ContactKinematics {
    r: Vec2,
    tangent: Vec2,
    v_n: f32,
    v_t: f32,
}

fn contact_kinematics(ship: &Ship, contact: Vec2, normal: Vec2) -> ContactKinematics {
    let r = contact - ship.pos;
    let v_c = ship.vel + ship.angular_vel * r.perp();
    let tangent = normal.perp();
    ContactKinematics {
        r,
        tangent,
        v_n: v_c.dot(normal),
        v_t: v_c.dot(tangent),
    }
}

fn apply_normal_impulse(ship: &mut Ship, kin: &ContactKinematics, normal: Vec2, e: f32) -> f32 {
    let r_cross_n = kin.r.perp_dot(normal);
    let k_n = 1.0 / SHIP_MASS + r_cross_n * r_cross_n / SHIP_INERTIA;
    let j_n = -(1.0 + e) * kin.v_n / k_n;
    ship.vel += normal * (j_n / SHIP_MASS);
    ship.angular_vel += j_n * r_cross_n / SHIP_INERTIA;
    j_n
}

fn apply_friction_impulse(ship: &mut Ship, kin: &ContactKinematics, j_n: f32, mu: f32) {
    let r_cross_t = kin.r.perp_dot(kin.tangent);
    let k_t = 1.0 / SHIP_MASS + r_cross_t * r_cross_t / SHIP_INERTIA;
    let j_t_max = mu * j_n.abs();
    let j_t = (-kin.v_t / k_t).clamp(-j_t_max, j_t_max);
    ship.vel += kin.tangent * (j_t / SHIP_MASS);
    ship.angular_vel += j_t * r_cross_t / SHIP_INERTIA;
}

fn apply_contact(ship: &mut Ship, contact: Vec2, normal: Vec2, impact: &mut Option<WallImpact>) {
    let landable = normal.dot(Vec2::new(0.0, -1.0)) > LANDABLE_DOT;
    let kin = contact_kinematics(ship, contact, normal);
    let impact_speed = (-kin.v_n).max(0.0);
    let ship_speed = ship.vel.length();

    if landable && (impact_speed > PAD_SLAM_SPEED || ship_speed > PAD_SLAM_SPEED) {
        let damage_speed = ship_speed.max(impact_speed);
        let over = (damage_speed - SCRAPE_THRESHOLD).max(0.0);
        let extra = over * over * IMPACT_DAMAGE_SCALE + CHIP_DAMAGE_PER_BOUNCE;
        ship.shields = (ship.shields - extra).max(0.0);
        *impact = Some(WallImpact {
            pos: contact,
            normal,
            speed: damage_speed,
        });
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }
        if kin.v_n < 0.0 {
            apply_normal_impulse(ship, &kin, normal, BOUNCE_RESTITUTION);
        }
        return;
    }

    if !landable {
        ship.shields = (ship.shields - WALL_CONTACT_DPS * DT).max(0.0);
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }
        *impact = Some(WallImpact {
            pos: contact,
            normal,
            speed: impact_speed,
        });
    }

    let is_bounce = impact_speed > BOUNCE_FLOOR;
    if is_bounce {
        ship.shields = (ship.shields - CHIP_DAMAGE_PER_BOUNCE).max(0.0);
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }
    }

    let over = (impact_speed - SCRAPE_THRESHOLD).max(0.0);
    let extra = over * over * IMPACT_DAMAGE_SCALE;
    if extra > 0.0 {
        ship.shields = (ship.shields - extra).max(0.0);
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }
    }

    let j_n = if kin.v_n < 0.0 {
        if !landable {
            apply_normal_impulse(ship, &kin, normal, COLLISION_BOUNCE)
        } else if is_bounce {
            apply_normal_impulse(ship, &kin, normal, BOUNCE_RESTITUTION)
        } else {
            ship.vel -= normal * kin.v_n;
            -kin.v_n
        }
    } else {
        0.0
    };

    if landable {
        apply_friction_impulse(ship, &kin, j_n, SHIP_FRICTION);
        if kin.v_n < 0.0 {
            ship.landed = true;
        }

        if ship.landed
            && normal.y < SETTLE_NORMAL_Y_MAX
            && ship.vel.length_squared() < SETTLE_VEL * SETTLE_VEL
            && ship.angular_vel.abs() < SETTLE_AV
        {
            let tilt = angle_diff(ship.angle, UPRIGHT_ANGLE);
            let target = if tilt.abs() < TIP_OVER_ANGLE {
                0.0
            } else {
                tilt.signum() * TIP_FLAT_ANGLE
            };
            let err = tilt - target;
            if err.abs() > 0.0 {
                let dw = (-SETTLE_RESTORING_RATE * err).clamp(-SETTLE_AV_CAP, SETTLE_AV_CAP);
                ship.angular_vel += dw;
            }
        }
    }
}

fn angle_diff(a: f32, b: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let mut d = (a - b) % TAU;
    if d > PI {
        d -= TAU;
    } else if d < -PI {
        d += TAU;
    }
    d
}

fn step_ship(ship: &mut Ship, input: Input, gravity: Vec2, was_landed: bool) -> bool {
    // Rotation input only honoured in flight or while tipped (recovery).
    // Landed-in-basin is locked — pad-contact code drives the angle.
    let rotation_locked = was_landed && !ship.tipped_over;
    let mut angular_accel = 0.0;
    if !rotation_locked {
        if input.contains(Input::ROTATE_LEFT) {
            angular_accel -= SHIP_ROT_SPEED;
        }
        if input.contains(Input::ROTATE_RIGHT) {
            angular_accel += SHIP_ROT_SPEED;
        }
    }
    ship.angular_vel = ship.angular_vel * SHIP_ANGULAR_DAMPING + angular_accel * DT;
    ship.angle += ship.angular_vel * DT;

    let mut accel = gravity;
    let mut thrusted = false;
    if input.contains(Input::THRUST) && ship.fuel > 0.0 {
        accel += ship.forward() * SHIP_THRUST;
        ship.fuel = (ship.fuel - FUEL_BURN_PER_SEC * DT).max(0.0);
        thrusted = true;
    }
    ship.vel += accel * DT;
    ship.vel *= SHIP_LINEAR_DAMPING;
    ship.pos += ship.vel * DT;
    thrusted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_pulls_idle_ship_down() {
        let mut world = World::new(Level::default());
        let start_y = world.ships[0].pos.y;
        for _ in 0..60 {
            world.tick([Input::empty(), Input::empty()]);
        }
        assert!(world.ships[0].pos.y > start_y);
    }

    #[test]
    fn fresh_ship_state() {
        let world = World::new(Level::default());
        for ship in &world.ships {
            assert_eq!(ship.shields, SHIELD_MAX);
            assert_eq!(ship.fuel, FUEL_MAX);
            assert!(ship.alive);
            assert!(!ship.landed);
        }
    }

    #[test]
    fn rng_is_deterministic_with_same_seed() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    fn tiny_mask_png(width: u32, height: u32) -> Vec<u8> {
        use image::{ImageEncoder, codecs::png::PngEncoder};
        let mut buf = vec![0u8; (width as usize) * (height as usize) * 4];
        for y in 0..height {
            for x in 0..width {
                let i = ((y * width + x) * 4) as usize;
                let solid = x == 0 || y == 0 || x == width - 1 || y == height - 1;
                buf[i] = 60;
                buf[i + 1] = 60;
                buf[i + 2] = 80;
                buf[i + 3] = if solid { 255 } else { 0 };
            }
        }
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(&buf, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        out
    }

    const SAMPLE_TMX: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" tiledversion="1.11.0" orientation="orthogonal" renderorder="right-down" width="40" height="30" tilewidth="1" tileheight="1" infinite="0" nextlayerid="2" nextobjectid="5">
 <properties>
  <property name="name" value="Sample"/>
  <property name="gravity" type="float" value="42"/>
 </properties>
 <objectgroup id="1" name="objects">
  <object id="1" class="spawn" x="5" y="10">
   <properties><property name="player" type="int" value="0"/></properties>
   <point/>
  </object>
  <object id="2" class="spawn" x="30" y="10">
   <properties><property name="player" type="int" value="1"/></properties>
   <point/>
  </object>
  <object id="3" class="pad" x="3" y="20" width="6" height="2"/>
  <object id="4" class="pad" x="28" y="20" width="6" height="2"/>
 </objectgroup>
</map>
"#;

    #[test]
    fn from_bytes_parses_spawns_pads_and_gravity() {
        let mask = tiny_mask_png(40, 30);
        let level = Level::from_bytes(&mask, SAMPLE_TMX).expect("level must load");

        assert_eq!(level.size, Vec2::new(40.0, 30.0));
        assert_eq!(level.gravity, 42.0);
        assert_eq!(level.spawn_points[0], Vec2::new(5.0, 10.0));
        assert_eq!(level.spawn_points[1], Vec2::new(30.0, 10.0));

        let pads: Vec<&Rect> = level
            .rects
            .iter()
            .filter(|r| r.kind == RectKind::Pad)
            .collect();
        assert_eq!(pads.len(), 2);
        assert_eq!(pads[0].min, Vec2::new(3.0, 20.0));
        assert_eq!(pads[0].max, Vec2::new(9.0, 22.0));
        assert_eq!(pads[1].min, Vec2::new(28.0, 20.0));
        assert_eq!(pads[1].max, Vec2::new(34.0, 22.0));

        // Mask dims authoritative; border is solid, interior is not.
        assert!(level.mask.is_solid(0, 0));
        assert!(level.mask.is_solid(39, 29));
        assert!(!level.mask.is_solid(20, 15));
    }

    #[test]
    fn from_bytes_rejects_missing_spawn() {
        let mask = tiny_mask_png(40, 30);
        let tmx = br#"<?xml version="1.0" encoding="UTF-8"?>
<map version="1.10" orientation="orthogonal" renderorder="right-down" width="40" height="30" tilewidth="1" tileheight="1" infinite="0" nextlayerid="2" nextobjectid="2">
 <objectgroup id="1" name="objects">
  <object id="1" class="spawn" x="5" y="10">
   <properties><property name="player" type="int" value="0"/></properties>
   <point/>
  </object>
 </objectgroup>
</map>
"#;
        let err = Level::from_bytes(&mask, tmx).unwrap_err();
        assert!(matches!(err, LevelLoadError::MissingSpawn(1)));
    }

    #[test]
    fn default_level_has_borders_and_a_pad() {
        let level = Level::default();
        assert!(level.rects.iter().any(|r| r.kind == RectKind::Pad));
        assert!(
            level.mask.is_solid(640, 710),
            "floor should be solid in mask"
        );
        assert!(
            level.mask.is_solid(640, 5),
            "ceiling should be solid in mask"
        );
        assert!(
            level.mask.is_solid(5, 360),
            "left border should be solid in mask"
        );
        assert!(
            level.mask.is_solid(level.size.x as i32 - 5, 360),
            "right border should be solid in mask"
        );
    }

    #[test]
    fn default_level_mask_agrees_with_walls() {
        let level = Level::default();
        assert!(!level.mask.is_solid(640, 360));
        assert!(level.mask.is_solid(640, 710));
        assert!(level.mask.is_solid(640, 5));
        assert!(level.mask.is_solid(5, 360));
        assert!(level.mask.is_solid(-1, 360));
        assert!(level.mask.is_solid(640, 9999));
    }

    #[test]
    fn bitmask_from_png_uses_alpha() {
        use image::{Rgba, RgbaImage};
        use std::io::Cursor;

        let mut img = RgbaImage::new(4, 1);
        img.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 0, 0, 0]));
        img.put_pixel(2, 0, Rgba([255, 255, 255, 1]));
        img.put_pixel(3, 0, Rgba([255, 0, 0, 0]));

        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();

        let mask = BitMask::from_png_bytes(&buf).unwrap();
        assert_eq!((mask.width, mask.height), (4, 1));
        assert!(mask.is_solid(0, 0));
        assert!(!mask.is_solid(1, 0));
        assert!(mask.is_solid(2, 0));
        assert!(!mask.is_solid(3, 0));
        assert!(mask.is_solid(-1, 0));
    }

    #[test]
    fn deterministic_replay() {
        let inputs = [Input::THRUST, Input::ROTATE_LEFT];
        let mut a = World::new(Level::default());
        let mut b = World::new(Level::default());
        for _ in 0..120 {
            a.tick(inputs);
            b.tick(inputs);
        }
        assert_eq!(a.ships[0].pos, b.ships[0].pos);
        assert_eq!(a.ships[1].pos, b.ships[1].pos);
        assert_eq!(a.tick, b.tick);
    }

    /// Helper: a fresh world with player 0's ship placed and oriented as given,
    /// player 1's ship parked far off-screen so it can't influence the test.
    fn world_with_ship(pos: Vec2, vel: Vec2, angle: f32) -> World {
        let mut world = World::new(Level::default());
        world.ships[0].pos = pos;
        world.ships[0].vel = vel;
        world.ships[0].angle = angle;
        world.ships[0].angular_vel = 0.0;
        world.ships[1].pos = Vec2::new(-9999.0, -9999.0);
        world.ships[1].alive = false;
        world
    }

    #[test]
    fn soft_upright_touchdown_on_pad_lands() {
        let mut world = world_with_ship(
            Vec2::new(240.0, 600.0),
            Vec2::new(0.0, 30.0),
            -std::f32::consts::FRAC_PI_2,
        );
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
            if world.ships[0].landed {
                break;
            }
        }
        let ship = &world.ships[0];
        assert!(ship.landed, "expected ship to land on pad");
        // Discrete-bounce model chips a few points on every contact; soft
        // landings should leave most of the shield intact.
        assert!(
            ship.shields > SHIELD_MAX - 10.0,
            "soft landing should only chip a few points, got {}",
            ship.shields
        );
        assert!(ship.alive);
    }

    #[test]
    fn nose_first_ceiling_hit_no_penetration() {
        let mut world = world_with_ship(
            Vec2::new(640.0, 40.0),
            Vec2::new(0.0, -200.0),
            UPRIGHT_ANGLE,
        );
        world.tick([Input::empty(), Input::empty()]);
        let ship = &world.ships[0];
        for v in ship.triangle_vertices().iter() {
            assert!(
                !world.level.mask.is_solid(v.x as i32, v.y as i32),
                "vertex at {:?} stuck in solid mask after collision",
                v
            );
        }
    }

    #[test]
    fn tipped_ship_rests_flush_with_floor() {
        let tipped = UPRIGHT_ANGLE + TIP_FLAT_ANGLE;
        let mut world = world_with_ship(Vec2::new(640.0, 680.0), Vec2::new(0.0, 5.0), tipped);
        world.ships[0].tipped_over = true;
        for _ in 0..200 {
            world.tick([Input::empty(), Input::empty()]);
        }
        let ship = &world.ships[0];
        let lowest_y = ship
            .triangle_vertices()
            .iter()
            .map(|v| v.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (700.0 - lowest_y).abs() < 2.0,
            "lowest vertex y = {}, expected ~700, gap = {}",
            lowest_y,
            700.0 - lowest_y
        );
    }

    #[test]
    fn tipped_ship_does_not_drift_on_floor() {
        let off_flat = UPRIGHT_ANGLE + TIP_FLAT_ANGLE + 0.5;
        let mut world = world_with_ship(Vec2::new(640.0, 680.0), Vec2::new(0.0, 5.0), off_flat);
        world.ships[0].tipped_over = true;

        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
        }
        assert!(world.ships[0].alive);
        let settled_x = world.ships[0].pos.x;

        for _ in 0..60 {
            world.tick([Input::empty(), Input::empty()]);
            if !world.ships[0].alive {
                break;
            }
        }
        assert!(world.ships[0].alive, "ship died during measurement window");
        let drift = (world.ships[0].pos.x - settled_x).abs();
        assert!(drift < 4.0, "tipped ship drifted {drift} px after settling");
    }

    #[test]
    fn soft_upright_touchdown_on_floor_lands() {
        // Drop slowly onto the bottom border wall (y >= 700) at a spot
        // not covered by a pad. Ship should land just like on a pad.
        let mut world = world_with_ship(
            Vec2::new(640.0, 670.0),
            Vec2::new(0.0, 30.0),
            -std::f32::consts::FRAC_PI_2,
        );
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
            if world.ships[0].landed {
                break;
            }
        }
        let ship = &world.ships[0];
        assert!(ship.landed, "expected ship to land on the floor wall");
        assert!(ship.alive);
    }

    #[test]
    fn hard_wall_impact_damages_shields() {
        // Floor wall starts at y=700. Park just above and slam at 400 px/s.
        let mut world = world_with_ship(
            Vec2::new(300.0, 685.0),
            Vec2::new(0.0, 400.0),
            0.0, // sideways — not upright
        );
        world.tick([Input::empty(), Input::empty()]);
        let ship = &world.ships[0];
        assert!(
            ship.shields < SHIELD_MAX,
            "shields should drop on hard impact, got {}",
            ship.shields
        );
        assert!(!ship.landed);
    }

    #[test]
    fn fatal_impact_kills_ship() {
        // 1500 px/s into the floor: damage = (1500 - 50) * 0.25 = 362.5, way past SHIELD_MAX.
        let mut world = world_with_ship(Vec2::new(300.0, 685.0), Vec2::new(0.0, 1500.0), 0.0);
        world.tick([Input::empty(), Input::empty()]);
        assert!(!world.ships[0].alive);
        assert_eq!(world.ships[0].shields, 0.0);
    }

    #[test]
    fn tilted_touchdown_settles_toward_upright() {
        // Touch down with a slight clockwise tilt (within tolerance so we land).
        let tilted = -std::f32::consts::FRAC_PI_2 + 0.25;
        let mut world = world_with_ship(Vec2::new(240.0, 600.0), Vec2::new(0.0, 30.0), tilted);
        // Drift down until landed.
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
            if world.ships[0].landed {
                break;
            }
        }
        assert!(world.ships[0].landed);

        // Run a few seconds of no input — pendulum should converge to upright.
        for _ in 0..180 {
            world.tick([Input::empty(), Input::empty()]);
        }
        let final_diff = angle_diff(world.ships[0].angle, UPRIGHT_ANGLE).abs();
        assert!(
            final_diff < 0.05,
            "expected ship to settle near upright, got diff={final_diff} rad"
        );
        assert!(world.ships[0].landed);
    }

    #[test]
    fn slope_contact_tilts_ship_no_upright_snap() {
        // On a 20° slope (n.y ≈ -0.940, so settle's restoring torque does NOT
        // engage), verify that on first contact the ship's tilt aligns with
        // the slope direction rather than snapping upright. This pins the
        // headline Phase 2 claim that landing no longer rewrites the angle.
        //
        // We don't assert long-term rest: with μ=0.6 the friction model's
        // angular coupling lets gravity overpower friction even on a 20°
        // slope, so ships drift downhill rather than coming to a steady rest.
        // Capturing the moment-of-contact tilt is what's testable today.
        let slope_angle = std::f32::consts::PI / 9.0;
        let size = Vec2::new(1280.0, 720.0);
        let mut mask = BitMask::new(size.x as u32, size.y as u32, false);
        for y in 700..720 {
            for x in 0..size.x as i32 {
                mask.set(x, y, true);
            }
        }
        let slope_x0 = 400i32;
        let slope_x1 = 700i32;
        let slope_tan = slope_angle.tan();
        for x in slope_x0..=slope_x1 {
            let dx = (x - slope_x0) as f32;
            let slope_y = (700.0 - dx * slope_tan).ceil() as i32;
            for y in slope_y..700 {
                mask.set(x, y, true);
            }
        }
        let level = Level {
            size,
            gravity: DEFAULT_GRAVITY,
            spawn_points: [Vec2::new(640.0, 100.0), Vec2::new(700.0, 100.0)],
            rects: Vec::new(),
            mask,
        };
        let mut world = World::new(level);
        world.ships[0].pos = Vec2::new(550.0, 600.0);
        world.ships[0].vel = Vec2::ZERO;
        world.ships[0].angle = UPRIGHT_ANGLE;
        world.ships[0].angular_vel = 0.0;
        world.ships[1].pos = Vec2::new(-9999.0, -9999.0);
        world.ships[1].alive = false;

        let mut landed_at: Option<usize> = None;
        for i in 0..240 {
            world.tick([Input::empty(), Input::empty()]);
            if landed_at.is_none() && world.ships[0].landed {
                landed_at = Some(i);
            }
        }

        let ship = &world.ships[0];
        let tilt = angle_diff(ship.angle, UPRIGHT_ANGLE).abs();
        let landed_at = landed_at.expect("ship should make slope contact within 240 ticks");

        assert!(ship.alive, "ship should survive 20° slope landing");
        assert!(
            tilt > 0.20,
            "expected slope-aligned tilt (no upright snap), got tilt={:.3} rad after landing at tick {}",
            tilt,
            landed_at,
        );
        assert!(
            tilt < slope_angle + 0.25,
            "tilt should not exceed slope angle by much, got {:.3} rad (slope={:.3})",
            tilt,
            slope_angle,
        );
    }

    #[test]
    fn wing_down_past_basin_tips_over_and_dies() {
        // Touchdown well past the upright basin → ship tips over instead of
        // settling, then chip damage drains shields to zero.
        let tilted = -std::f32::consts::FRAC_PI_2 + 1.2; // ~69° off upright
        let mut world = world_with_ship(Vec2::new(240.0, 600.0), Vec2::new(0.0, 30.0), tilted);
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
            if world.ships[0].landed {
                break;
            }
        }
        assert!(world.ships[0].landed, "ship should make pad contact");

        // Run up to 5 seconds of game time — tipped flag must trigger and
        // chip damage must kill the ship.
        let mut tipped_seen = false;
        for _ in 0..300 {
            world.tick([Input::empty(), Input::empty()]);
            if world.ships[0].tipped_over {
                tipped_seen = true;
            }
            if !world.ships[0].alive {
                break;
            }
        }
        assert!(tipped_seen, "ship should enter tipped state");
        assert!(
            !world.ships[0].alive,
            "tipped ship should die from chip damage"
        );
    }

    #[test]
    fn tipped_ship_lifts_off_with_thrust() {
        let setup = || {
            let mut w = world_with_ship(
                Vec2::new(240.0, 600.0),
                Vec2::new(0.0, 30.0),
                -std::f32::consts::FRAC_PI_2 + 1.2,
            );
            for _ in 0..60 {
                w.tick([Input::empty(), Input::empty()]);
                if w.ships[0].tipped_over {
                    break;
                }
            }
            w
        };
        let mut with_thrust = setup();
        let mut without_thrust = setup();
        assert!(with_thrust.ships[0].tipped_over);

        for _ in 0..120 {
            with_thrust.tick([Input::THRUST, Input::empty()]);
            without_thrust.tick([Input::empty(), Input::empty()]);
        }

        let delta = (with_thrust.ships[0].pos - without_thrust.ships[0].pos).length();
        assert!(
            delta > 50.0,
            "thrust on a tipped ship should move it noticeably; delta={delta}"
        );
    }

    #[test]
    fn landed_ship_can_still_lift_off() {
        // First land soft and upright.
        let mut world =
            world_with_ship(Vec2::new(240.0, 600.0), Vec2::new(0.0, 30.0), UPRIGHT_ANGLE);
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
            if world.ships[0].landed {
                break;
            }
        }
        assert!(world.ships[0].landed);

        // Hold thrust — ship should leave the pad.
        for _ in 0..60 {
            world.tick([Input::THRUST, Input::empty()]);
        }
        assert!(
            !world.ships[0].landed,
            "thrust should lift ship off the pad"
        );
        assert!(world.ships[0].vel.y < 0.0, "ship should be moving upward");
    }

    #[test]
    fn pad_refuels_and_recharges_while_landed() {
        let mut world = world_with_ship(
            Vec2::new(240.0, 600.0),
            Vec2::new(0.0, 30.0),
            -std::f32::consts::FRAC_PI_2,
        );
        world.ships[0].fuel = 100.0;
        world.ships[0].shields = 20.0;
        for _ in 0..120 {
            world.tick([Input::empty(), Input::empty()]);
        }
        let ship = &world.ships[0];
        assert!(ship.landed);
        assert!(ship.fuel > 100.0, "fuel should regen, got {}", ship.fuel);
        assert!(
            ship.shields > 20.0,
            "shields should regen, got {}",
            ship.shields
        );
        assert!(ship.fuel <= FUEL_MAX);
        assert!(ship.shields <= SHIELD_MAX);
    }

    #[test]
    fn ship_lands_on_mask_only_floor() {
        // Build a level with empty rects and a mask whose bottom 20 rows
        // are solid. Ship dropped from above should land via the bitmap
        // collision path alone.
        let size = Vec2::new(1280.0, 720.0);
        let mut mask = BitMask::new(size.x as u32, size.y as u32, false);
        for y in 700..720 {
            for x in 0..size.x as i32 {
                mask.set(x, y, true);
            }
        }
        let level = Level {
            size,
            gravity: DEFAULT_GRAVITY,
            spawn_points: [Vec2::new(640.0, 100.0), Vec2::new(700.0, 100.0)],
            rects: Vec::new(),
            mask,
        };
        let mut world = World::new(level);
        world.ships[0].pos = Vec2::new(640.0, 600.0);
        world.ships[0].vel = Vec2::new(0.0, 30.0);
        world.ships[1].pos = Vec2::new(-9999.0, -9999.0);
        for _ in 0..180 {
            world.tick([Input::empty(), Input::empty()]);
        }
        let ship = &world.ships[0];
        assert!(ship.alive);
        assert!(ship.landed, "expected mask-only landing");
    }

    #[test]
    fn floor_landing_does_not_refuel_or_recharge() {
        let mut world = world_with_ship(
            Vec2::new(640.0, 600.0),
            Vec2::new(0.0, 30.0),
            -std::f32::consts::FRAC_PI_2,
        );
        world.ships[0].fuel = 100.0;
        world.ships[0].shields = 20.0;
        for _ in 0..180 {
            world.tick([Input::empty(), Input::empty()]);
        }
        let ship = &world.ships[0];
        assert!(ship.landed);
        assert!(!ship.landed_on_pad);
        assert!(
            ship.fuel <= 100.0,
            "fuel should not regen off-pad, got {}",
            ship.fuel
        );
        assert!(
            ship.shields <= 20.0,
            "shields should not regen off-pad, got {}",
            ship.shields
        );
    }

    #[test]
    fn fire_input_spawns_a_bullet() {
        let mut world = World::new(Level::default());
        // Park P1 mid-air, facing right so bullets fly into open space.
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].angle = 0.0;
        world.tick([Input::FIRE, Input::empty()]);
        assert_eq!(world.bullets.len(), 1);
        assert_eq!(world.bullets[0].owner, 0);
    }

    #[test]
    fn cooldown_gates_autofire_rate() {
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].angle = 0.0;
        // 1 second of held FIRE = ~ 1/FIRE_COOLDOWN bullets, ±1.
        for _ in 0..60 {
            world.tick([Input::FIRE, Input::empty()]);
        }
        let expected = (1.0 / FIRE_COOLDOWN) as usize;
        let n = world.bullets.len();
        assert!(
            n >= expected - 1 && n <= expected + 1,
            "expected ~{expected} bullets, got {n}"
        );
    }

    #[test]
    fn bullets_expire_or_die_at_wall_eventually() {
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].angle = 0.0;
        world.tick([Input::FIRE, Input::empty()]);
        assert_eq!(world.bullets.len(), 1);
        // Run past TTL — the bullet either ages out or runs into the right wall.
        let ticks_past_ttl = (BULLET_TTL / DT).ceil() as usize + 5;
        for _ in 0..ticks_past_ttl {
            world.tick([Input::empty(), Input::empty()]);
        }
        assert_eq!(world.bullets.len(), 0);
    }

    #[test]
    fn bullet_dies_when_it_hits_a_wall() {
        let mut world = World::new(Level::default());
        // P1 at left, aimed at the right wall (x=1260). At ~600 px/s the
        // bullet should reach the wall in well under BULLET_TTL.
        world.ships[0].pos = Vec2::new(800.0, 360.0);
        world.ships[0].angle = 0.0;
        world.tick([Input::FIRE, Input::empty()]);
        assert_eq!(world.bullets.len(), 1);
        for _ in 0..120 {
            world.tick([Input::empty(), Input::empty()]);
            if world.bullets.is_empty() {
                break;
            }
        }
        assert!(
            world.bullets.is_empty(),
            "bullet should be despawned by wall"
        );
    }

    #[test]
    fn bullet_damages_opponent_not_owner() {
        let mut world = World::new(Level::default());
        // P1 at left, facing right. P2 directly to the right, in line.
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].angle = 0.0;
        world.ships[1].pos = Vec2::new(500.0, 360.0);
        world.ships[1].angle = 0.0;
        world.tick([Input::FIRE, Input::empty()]);
        assert_eq!(world.bullets.len(), 1);
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
            if world.bullets.is_empty() {
                break;
            }
        }
        assert!(world.bullets.is_empty(), "bullet should hit P2");
        assert_eq!(world.ships[0].shields, SHIELD_MAX, "owner is unharmed");
        assert!(
            world.ships[1].shields < SHIELD_MAX,
            "P2 took bullet damage, got {}",
            world.ships[1].shields
        );
    }

    #[test]
    fn bullet_does_not_hit_owner() {
        // Fire then verify that the bullet's start position (just ahead of
        // muzzle) doesn't immediately count as a self-hit.
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].angle = 0.0;
        world.tick([Input::FIRE, Input::empty()]);
        assert_eq!(world.ships[0].shields, SHIELD_MAX);
    }

    #[test]
    fn ship_death_spawns_explosion_and_particles_expire() {
        // Gentle floor-kill: shields nearly empty, low velocity. A 1500 px/s
        // slam would yank the explosion's base velocity straight into the
        // floor and wall-collision would eat every particle on tick 1.
        let mut world = world_with_ship(Vec2::new(300.0, 690.0), Vec2::new(0.0, 100.0), 0.0);
        world.ships[0].shields = 0.5;
        world.tick([Input::empty(), Input::empty()]);
        assert!(!world.ships[0].alive);
        assert_eq!(world.particles.len(), EXPLOSION_PARTICLE_COUNT);

        // Run past the longest possible particle TTL.
        let ticks = (PARTICLE_TTL_MAX / DT).ceil() as usize + 5;
        for _ in 0..ticks {
            world.tick([Input::empty(), Input::empty()]);
        }
        assert_eq!(world.particles.len(), 0);
    }

    #[test]
    fn explosion_particles_are_deterministic() {
        let mk = || {
            let mut w = World::new(Level::default());
            w.ships[0].pos = Vec2::new(300.0, 685.0);
            w.ships[0].vel = Vec2::new(0.0, 1500.0);
            w.ships[0].angle = 0.0;
            w.ships[1].pos = Vec2::new(-9999.0, -9999.0);
            w.ships[1].alive = false;
            w
        };
        let mut a = mk();
        let mut b = mk();
        for _ in 0..30 {
            a.tick([Input::empty(), Input::empty()]);
            b.tick([Input::empty(), Input::empty()]);
        }
        assert_eq!(a.particles.len(), b.particles.len());
        for (pa, pb) in a.particles.iter().zip(b.particles.iter()) {
            assert_eq!(pa.pos, pb.pos);
            assert_eq!(pa.vel, pb.vel);
            assert_eq!(pa.ttl, pb.ttl);
        }
    }

    #[test]
    fn thrust_input_emits_owned_thrust_particles() {
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].angle = 0.0;
        world.tick([Input::THRUST, Input::empty()]);
        let thrusts: Vec<_> = world
            .particles
            .iter()
            .filter(|p| p.kind == ParticleKind::Thrust)
            .collect();
        assert_eq!(thrusts.len(), THRUST_PARTICLES_PER_TICK as usize);
        for p in &thrusts {
            assert_eq!(p.owner, 0);
        }
    }

    #[test]
    fn thrust_particles_do_not_damage_their_owner() {
        let mut world = world_with_ship(Vec2::new(400.0, 360.0), Vec2::ZERO, 0.0);
        for _ in 0..60 {
            world.tick([Input::THRUST, Input::empty()]);
        }
        assert_eq!(world.ships[0].shields, SHIELD_MAX);
    }

    #[test]
    fn thrust_particles_damage_other_ship() {
        let mut world = World::new(Level::default());
        // P1 facing right, exhaust shoots left straight at P2.
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].angle = 0.0;
        world.ships[1].pos = Vec2::new(370.0, 360.0);
        world.ships[1].vel = Vec2::ZERO;
        world.ships[1].angle = UPRIGHT_ANGLE;
        let init = world.ships[1].shields;
        for _ in 0..30 {
            world.tick([Input::THRUST, Input::empty()]);
        }
        assert!(
            world.ships[1].shields < init,
            "P2 should take particle damage, got {}",
            world.ships[1].shields
        );
        assert!(
            world.ships[1].vel.x < 0.0,
            "P2 should be pushed left by exhaust, got vel.x={}",
            world.ships[1].vel.x
        );
    }

    #[test]
    fn explosion_particles_damage_other_ship() {
        // Kill P1 gently: graze the floor with shields almost depleted so
        // the explosion's base velocity stays low and shrapnel actually
        // sprays outward instead of getting yanked along by the slam.
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(300.0, 690.0);
        world.ships[0].vel = Vec2::new(0.0, 100.0);
        world.ships[0].angle = 0.0;
        world.ships[0].shields = 0.5;
        world.ships[1].pos = Vec2::new(320.0, 670.0);
        world.ships[1].vel = Vec2::ZERO;
        world.ships[1].angle = UPRIGHT_ANGLE;
        world.tick([Input::empty(), Input::empty()]);
        assert!(!world.ships[0].alive, "P1 should die from floor contact");
        let init = world.ships[1].shields;
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
        }
        assert!(
            world.ships[1].shields < init,
            "P2 should take shrapnel damage, got {}",
            world.ships[1].shields
        );
    }

    #[test]
    fn particles_die_when_entering_a_wall() {
        let mut world = world_with_ship(Vec2::new(-5000.0, -5000.0), Vec2::ZERO, 0.0);
        world.ships[0].alive = false;
        world.particles.push(Particle {
            pos: Vec2::new(400.0, 698.0),
            vel: Vec2::new(0.0, 200.0),
            ttl: 1.0,
            max_ttl: 1.0,
            owner: 0,
            kind: ParticleKind::Thrust,
        });
        world.tick([Input::empty(), Input::empty()]);
        assert!(
            world.particles.is_empty(),
            "particle entering the floor should despawn"
        );
    }

    #[test]
    fn thrust_particles_are_deterministic() {
        let mk = || {
            let mut w = world_with_ship(Vec2::new(400.0, 360.0), Vec2::ZERO, 0.0);
            w.ships[1].pos = Vec2::new(-9999.0, -9999.0);
            w.ships[1].alive = false;
            w
        };
        let mut a = mk();
        let mut b = mk();
        for _ in 0..60 {
            a.tick([Input::THRUST, Input::empty()]);
            b.tick([Input::THRUST, Input::empty()]);
        }
        assert_eq!(a.particles.len(), b.particles.len());
        for (pa, pb) in a.particles.iter().zip(b.particles.iter()) {
            assert_eq!(pa.pos, pb.pos);
            assert_eq!(pa.vel, pb.vel);
            assert_eq!(pa.ttl, pb.ttl);
            assert_eq!(pa.owner, pb.owner);
            assert_eq!(pa.kind, pb.kind);
        }
    }

    #[test]
    fn end_to_end_replay_is_deterministic() {
        // P1 fires, drifts, eventually lands on the pad. P2 falls and clips
        // a wall. Both worlds should agree byte-for-byte after 180 ticks.
        let scripted = |t: usize| -> [Input; 2] {
            let p1 = if t < 90 {
                Input::FIRE | Input::THRUST
            } else if t % 20 < 10 {
                Input::ROTATE_LEFT
            } else {
                Input::empty()
            };
            let p2 = if t < 60 {
                Input::ROTATE_RIGHT
            } else {
                Input::empty()
            };
            [p1, p2]
        };
        let mk = || World::new(Level::default());
        let mut a = mk();
        let mut b = mk();
        for t in 0..180 {
            let inp = scripted(t);
            a.tick(inp);
            b.tick(inp);
        }
        assert_eq!(a.tick, b.tick);
        for i in 0..2 {
            assert_eq!(a.ships[i].pos, b.ships[i].pos);
            assert_eq!(a.ships[i].vel, b.ships[i].vel);
            assert_eq!(a.ships[i].shields, b.ships[i].shields);
            assert_eq!(a.ships[i].alive, b.ships[i].alive);
        }
        assert_eq!(a.bullets.len(), b.bullets.len());
        for (ba, bb) in a.bullets.iter().zip(b.bullets.iter()) {
            assert_eq!(ba.pos, bb.pos);
            assert_eq!(ba.vel, bb.vel);
            assert_eq!(ba.ttl, bb.ttl);
        }
        assert_eq!(a.particles.len(), b.particles.len());
    }

    #[test]
    fn ships_cannot_overlap() {
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].vel = Vec2::new(80.0, 0.0);
        world.ships[0].angle = 0.0;
        world.ships[1].pos = Vec2::new(400.0 + SHIP_SIZE * 1.5, 360.0);
        world.ships[1].vel = Vec2::new(-80.0, 0.0);
        world.ships[1].angle = std::f32::consts::PI;
        for _ in 0..30 {
            world.tick([Input::empty(), Input::empty()]);
            let tri0 = world.ships[0].triangle_vertices();
            let tri1 = world.ships[1].triangle_vertices();
            let a_to_b = world.ships[1].pos - world.ships[0].pos;
            let depth = sat_triangles(&tri0, &tri1, a_to_b)
                .map(|(_, d)| d)
                .unwrap_or(0.0);
            assert!(depth < 1e-3, "ship triangles overlap by {depth}");
        }
    }

    #[test]
    fn dead_ship_with_zero_timer_eventually_respawns() {
        let mut world = World::new(Level::default());
        world.ships[0].alive = false;
        world.ships[0].respawn_ticks = 0;
        for _ in 0..(RESPAWN_TICKS + 5) {
            world.tick([Input::empty(), Input::empty()]);
            if world.ships[0].alive {
                return;
            }
        }
        panic!("ship with stuck dead state never respawned");
    }

    #[test]
    fn nose_does_not_penetrate_back_edge() {
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].vel = Vec2::new(200.0, 0.0);
        world.ships[0].angle = 0.0;
        world.ships[1].pos = Vec2::new(420.0, 360.0);
        world.ships[1].vel = Vec2::ZERO;
        world.ships[1].angle = 0.0;

        world.tick([Input::empty(), Input::empty()]);

        let nose = world.ships[0].triangle_vertices()[0];
        let back_x = world.ships[1].pos.x - 0.7 * SHIP_SIZE;
        assert!(
            nose.x <= back_x + 1e-3,
            "attacker nose at {} crossed victim back edge at {}",
            nose.x,
            back_x,
        );
    }

    #[test]
    fn ship_ramming_damages_both() {
        let mut world = World::new(Level::default());
        world.ships[0].pos = Vec2::new(400.0, 360.0);
        world.ships[0].vel = Vec2::new(150.0, 0.0);
        world.ships[0].angle = 0.0;
        world.ships[1].pos = Vec2::new(400.0 + SHIP_RADIUS * 2.0 + 0.5, 360.0);
        world.ships[1].vel = Vec2::new(-150.0, 0.0);
        world.ships[1].angle = std::f32::consts::PI;
        world.tick([Input::empty(), Input::empty()]);
        assert!(world.ships[0].shields < SHIELD_MAX);
        assert!(world.ships[1].shields < SHIELD_MAX);
    }

    #[test]
    fn collision_replay_is_deterministic() {
        // Drop into floor, rebound, simulate further. Both worlds must agree.
        let inputs = [Input::THRUST, Input::empty()];
        let mk = || {
            let mut w = World::new(Level::default());
            w.ships[0].pos = Vec2::new(640.0, 500.0);
            w.ships[0].vel = Vec2::new(50.0, 200.0);
            w
        };
        let mut a = mk();
        let mut b = mk();
        for _ in 0..180 {
            a.tick(inputs);
            b.tick(inputs);
        }
        assert_eq!(a.ships[0].pos, b.ships[0].pos);
        assert_eq!(a.ships[0].vel, b.ships[0].vel);
        assert_eq!(a.ships[0].shields, b.ships[0].shields);
        assert_eq!(a.ships[0].alive, b.ships[0].alive);
    }
}
