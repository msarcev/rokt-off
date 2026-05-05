//! Pure deterministic simulation for head-on-rs.
//!
//! No I/O, no global state, no `Instant::now()`, no unseeded RNG.
//! Anything that breaks rollback determinism does not belong here.

use bitflags::bitflags;
use glam::Vec2;

pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

pub const SHIP_SIZE: f32 = 14.0;
pub const SHIP_RADIUS: f32 = SHIP_SIZE * 0.7;

pub const SHIP_THRUST: f32 = 380.0;
pub const SHIP_ROT_SPEED: f32 = 32.5;
pub const SHIP_ANGULAR_DAMPING: f32 = 0.90;
pub const SHIP_LINEAR_DAMPING: f32 = 0.99;
pub const DEFAULT_GRAVITY: f32 = 90.0;
pub const FUEL_MAX: f32 = 1000.0;
pub const FUEL_BURN_PER_SEC: f32 = 80.0;
pub const SHIELD_MAX: f32 = 100.0;

pub const IMPACT_DAMAGE_SCALE: f32 = 0.25;
pub const SCRAPE_THRESHOLD: f32 = 50.0;
pub const COLLISION_BOUNCE: f32 = 0.3;
pub const FUEL_REFILL_PER_SEC: f32 = 600.0;
pub const SHIELD_RECHARGE_PER_SEC: f32 = 60.0;

pub const UPRIGHT_ANGLE: f32 = -std::f32::consts::FRAC_PI_2;
pub const SETTLED_ANGLE_TOL: f32 = 0.18;
pub const SETTLED_DELAY_TICKS: u32 = 45;
pub const LIFTOFF_VELOCITY: f32 = 30.0;
pub const BOUNCE_RESTITUTION: f32 = 0.4;
pub const BOUNCE_FLOOR: f32 = 10.0;
pub const PAD_LATERAL_FRICTION_FLOOR: f32 = 80.0;
pub const PAD_LATERAL_RESTITUTION: f32 = 0.25;
// Tilt threshold past which the ship tips over instead of settling.
// Tuning knob: bigger = more forgiving. Reference: 0.39 = wing edge
// vertical, 0.79 = CoM over contact wing, 1.57 = wing line vertical.
pub const TIP_OVER_ANGLE: f32 = 0.70;
// Final lying-flat tilt: side edge (nose-to-wing) flush with pad.
// π/2 + atan(0.7 / 1.7) for the current triangle proportions.
pub const TIP_FLAT_ANGLE: f32 = 1.962;
pub const BOUNCE_RECOVERY_FACTOR: f32 = 0.4;
// Per-tick fraction of the angle gap closed while settling on the pad.
pub const SETTLE_RIGHTING_RATE: f32 = 0.05;
pub const TIPPED_SETTLE_RATE: f32 = 0.3;
pub const TIPPED_SETTLE_SNAP_TOL: f32 = 0.3;
// |angular_vel| above this counts as "player actively rotating" and
// suppresses the tipped-settle pivot so A/D recovery isn't fought.
pub const TIPPED_SETTLE_AV_THRESHOLD: f32 = 0.5;
pub const CHIP_DAMAGE_PER_BOUNCE: f32 = 1.5;
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
}

impl Default for Level {
    fn default() -> Self {
        let size = Vec2::new(1280.0, 720.0);
        let rects = vec![
            // floor
            Rect { min: Vec2::new(0.0, 700.0), max: Vec2::new(size.x, size.y), kind: RectKind::Wall },
            // ceiling
            Rect { min: Vec2::ZERO, max: Vec2::new(size.x, 20.0), kind: RectKind::Wall },
            // left wall
            Rect { min: Vec2::ZERO, max: Vec2::new(20.0, size.y), kind: RectKind::Wall },
            // right wall
            Rect { min: Vec2::new(size.x - 20.0, 0.0), max: size, kind: RectKind::Wall },
            // central refuel pad
            Rect { min: Vec2::new(560.0, 620.0), max: Vec2::new(720.0, 640.0), kind: RectKind::Pad },
        ];
        Self {
            size,
            gravity: DEFAULT_GRAVITY,
            spawn_points: [Vec2::new(240.0, 200.0), Vec2::new(1040.0, 200.0)],
            rects,
        }
    }
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
        let mut thrusted = [false; 2];
        for (idx, (ship, input)) in self.ships.iter_mut().zip(inputs.iter()).enumerate() {
            ship.fire_cooldown = (ship.fire_cooldown - DT).max(0.0);
            if !ship.alive {
                continue;
            }
            let was_landed = ship.landed;
            ship.landed = false;
            thrusted[idx] = step_ship(ship, *input, gravity, was_landed);
            resolve_ship_rects(ship, &self.level.rects);

            // Stay-landed sticky: pivot rotation can briefly lift the
            // contact vertex; treat the ship as still landed unless it's
            // moving up faster than LIFTOFF_VELOCITY.
            if was_landed
                && !ship.landed
                && (ship.tipped_over || ship.vel.y > -LIFTOFF_VELOCITY)
            {
                ship.landed = true;
            }

            if ship.tipped_over {
                ship.settled_ticks = 0;
                let tilt = angle_diff(ship.angle, UPRIGHT_ANGLE);
                if tilt.abs() < TIP_OVER_ANGLE {
                    // Player rotated back into the basin: clear and reset.
                    ship.tipped_over = false;
                    ship.tipped_ticks = 0;
                } else {
                    ship.tipped_ticks = ship.tipped_ticks.saturating_add(1);
                    let dps = TIP_DMG_BASE + ship.tipped_ticks as f32 * TIP_DMG_RAMP;
                    ship.shields = (ship.shields - dps * DT).max(0.0);
                    if ship.shields <= 0.0 {
                        ship.alive = false;
                    }
                }
            } else if ship.landed {
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

            // Hold-fire = autofire. Tipped ships can't fire.
            if !ship.tipped_over
                && input.contains(Input::FIRE)
                && ship.fire_cooldown <= 0.0
            {
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
        resolve_bullets(&mut self.bullets, &mut self.ships, &self.level.rects);
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
                    *ship = Ship::new(
                        self.level.spawn_points[idx],
                        -std::f32::consts::FRAC_PI_2,
                    );
                }
            }
        }

        // Advance particles under gravity, drop expired.
        for p in self.particles.iter_mut() {
            p.pos += p.vel * DT;
            p.vel += gravity * DT;
            p.ttl -= DT;
        }
        resolve_particles(&mut self.particles, &mut self.ships, &self.level.rects);
        self.particles.retain(|p| p.ttl > 0.0);

        self.tick += 1;
    }
}

fn spawn_explosion(
    particles: &mut Vec<Particle>,
    rng: &mut Rng,
    pos: Vec2,
    base_vel: Vec2,
    owner: u8,
) {
    use std::f32::consts::TAU;
    for _ in 0..EXPLOSION_PARTICLE_COUNT {
        let angle = rng.next_f32() * TAU;
        let speed = rng.range(PARTICLE_SPEED_MIN, PARTICLE_SPEED_MAX);
        let ttl = rng.range(PARTICLE_TTL_MIN, PARTICLE_TTL_MAX);
        let dir = Vec2::new(angle.cos(), angle.sin());
        let p = Particle {
            pos,
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

fn resolve_particles(particles: &mut [Particle], ships: &mut [Ship; 2], rects: &[Rect]) {
    let r = SHIP_RADIUS + PARTICLE_RADIUS;
    let r_sq = r * r;
    for p in particles.iter_mut() {
        if p.ttl <= 0.0 {
            continue;
        }
        if rects.iter().any(|rect| point_in_rect(p.pos, rect)) {
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

    let chip = if v_rel > BOUNCE_FLOOR { CHIP_DAMAGE_PER_BOUNCE } else { 0.0 };
    let extra = (v_rel - SCRAPE_THRESHOLD).max(0.0) * IMPACT_DAMAGE_SCALE;
    let total = chip + extra;
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

    let oriented = if a_to_b.dot(min_axis) >= 0.0 { min_axis } else { -min_axis };
    Some((oriented, min_depth))
}

fn project_triangle(tri: &[Vec2; 3], axis: Vec2) -> (f32, f32) {
    let p0 = tri[0].dot(axis);
    let p1 = tri[1].dot(axis);
    let p2 = tri[2].dot(axis);
    (p0.min(p1).min(p2), p0.max(p1).max(p2))
}

fn resolve_bullets(bullets: &mut [Bullet], ships: &mut [Ship; 2], rects: &[Rect]) {
    for b in bullets.iter_mut() {
        if b.ttl <= 0.0 {
            continue;
        }
        // Bullet vs rects: any kind kills the bullet on contact.
        if rects.iter().any(|r| point_in_rect(b.pos, r)) {
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
fn resolve_ship_rects(ship: &mut Ship, rects: &[Rect]) {
    for rect in rects {
        match rect.kind {
            RectKind::Pad => resolve_ship_pad(ship, rect),
            RectKind::Wall => resolve_ship_wall(ship, rect),
        }
    }
}

fn resolve_ship_pad(ship: &mut Ship, pad: &Rect) {
    let pad_top = pad.min.y;

    // CoM at or past the pad surface → treat as solid rect (side/bottom
    // collision). Top-landing only applies when approaching from above.
    if ship.pos.y >= pad_top {
        resolve_ship_wall(ship, pad);
        return;
    }

    // Lowest triangle vertex horizontally inside the pad = the wing tip
    // or nose actually touching down.
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

    // Rigid-body vertex velocity: v_at(p).y = v_cm.y + ω * (p.x - cm.x).
    let r_x = lowest.x - ship.pos.x;
    let v_at_vertex_y = ship.vel.y + ship.angular_vel * r_x;
    let impact_speed = v_at_vertex_y.max(0.0);

    let is_bounce = v_at_vertex_y > BOUNCE_FLOOR;
    if is_bounce {
        // Discrete-bounce model: every contact chips shields and snaps
        // the angle toward its target attitude (upright in basin, flat
        // outside). Suppressed for already-tipped ships under active
        // player rotation so A/D recovery isn't fought.
        ship.shields = (ship.shields - CHIP_DAMAGE_PER_BOUNCE).max(0.0);
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }

        let snap_allowed =
            !ship.tipped_over || ship.angular_vel.abs() < TIPPED_SETTLE_AV_THRESHOLD;
        if snap_allowed {
            let tilt = angle_diff(ship.angle, UPRIGHT_ANGLE);
            let target_tilt = if tilt.abs() < TIP_OVER_ANGLE {
                0.0
            } else {
                tilt.signum() * TIP_FLAT_ANGLE
            };
            let new_tilt = target_tilt + (tilt - target_tilt) * BOUNCE_RECOVERY_FACTOR;
            ship.angle = UPRIGHT_ANGLE + new_tilt;
            ship.angular_vel = 0.0;

            if tilt.abs() > TIP_OVER_ANGLE {
                ship.tipped_over = true;
            }
        }
    }

    let extra = (impact_speed - SCRAPE_THRESHOLD).max(0.0) * IMPACT_DAMAGE_SCALE;
    if extra > 0.0 {
        ship.shields = (ship.shields - extra).max(0.0);
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }
    }

    if is_bounce {
        ship.vel.y = -BOUNCE_RESTITUTION * v_at_vertex_y;
    } else if ship.vel.y > 0.0 {
        ship.vel.y = 0.0;
        let tilt = angle_diff(ship.angle, UPRIGHT_ANGLE);
        if !ship.tipped_over {
            // Smooth pivot toward upright; snap once close enough.
            ship.angle = if tilt.abs() < SETTLED_ANGLE_TOL {
                UPRIGHT_ANGLE
            } else {
                UPRIGHT_ANGLE + tilt * (1.0 - SETTLE_RIGHTING_RATE)
            };
            ship.angular_vel = 0.0;
        } else if ship.angular_vel.abs() < TIPPED_SETTLE_AV_THRESHOLD {
            // Tipped and idle: pivot toward lying-flat on the side wing.
            // Don't reset angular_vel — rotation input ramps through
            // 0.9-damping and would never escape if we did.
            let target = tilt.signum() * TIP_FLAT_ANGLE;
            ship.angle = if (tilt - target).abs() < TIPPED_SETTLE_SNAP_TOL {
                UPRIGHT_ANGLE + target
            } else {
                UPRIGHT_ANGLE + target + (tilt - target) * (1.0 - TIPPED_SETTLE_RATE)
            };
        }
    }

    // Sticky pad: gentle sideways motion locks immediately, hard sideways
    // impact gets one weak bounce before locking.
    if ship.vel.x.abs() > PAD_LATERAL_FRICTION_FLOOR {
        ship.vel.x = -PAD_LATERAL_RESTITUTION * ship.vel.x;
    } else {
        ship.vel.x = 0.0;
    }

    ship.landed = true;
}

fn resolve_ship_wall(ship: &mut Ship, rect: &Rect) {
    let closest = ship.pos.clamp(rect.min, rect.max);
    let delta = ship.pos - closest;
    let dist_sq = delta.length_squared();
    if dist_sq >= SHIP_RADIUS * SHIP_RADIUS {
        return;
    }

    ship.shields = (ship.shields - WALL_CONTACT_DPS * DT).max(0.0);
    if ship.shields <= 0.0 {
        ship.alive = false;
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
            if m == dx_left { (Vec2::new(-1.0, 0.0), dx_left) }
            else if m == dx_right { (Vec2::new(1.0, 0.0), dx_right) }
            else if m == dy_up { (Vec2::new(0.0, -1.0), dy_up) }
            else { (Vec2::new(0.0, 1.0), dy_down) }
        };
        (n, side_dist + SHIP_RADIUS)
    };

    ship.pos += normal * depth;

    let v_along_normal = ship.vel.dot(normal);
    let impact_speed = (-v_along_normal).max(0.0);

    // Chip on rebound only — sliding/resting contacts don't chip.
    if impact_speed > BOUNCE_FLOOR {
        ship.shields = (ship.shields - CHIP_DAMAGE_PER_BOUNCE).max(0.0);
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }
    }
    let extra = (impact_speed - SCRAPE_THRESHOLD).max(0.0) * IMPACT_DAMAGE_SCALE;
    if extra > 0.0 {
        ship.shields = (ship.shields - extra).max(0.0);
        if ship.shields <= 0.0 {
            ship.alive = false;
            return;
        }
    }
    if v_along_normal < 0.0 {
        ship.vel -= normal * (1.0 + COLLISION_BOUNCE) * v_along_normal;
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

    // Tipped ships can rotate (A/D recovery) but can't thrust.
    let mut accel = gravity;
    let mut thrusted = false;
    if !ship.tipped_over && input.contains(Input::THRUST) && ship.fuel > 0.0 {
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

    #[test]
    fn default_level_has_walls_and_a_pad() {
        let level = Level::default();
        assert!(level.rects.iter().any(|r| r.kind == RectKind::Pad));
        assert!(level.rects.iter().filter(|r| r.kind == RectKind::Wall).count() >= 4);
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
        // Pad in default level is at (560..720, 620..640). Park just above and drift down slowly.
        let mut world = world_with_ship(
            Vec2::new(640.0, 600.0),
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
        let mut world = world_with_ship(
            Vec2::new(300.0, 685.0),
            Vec2::new(0.0, 1500.0),
            0.0,
        );
        world.tick([Input::empty(), Input::empty()]);
        assert!(!world.ships[0].alive);
        assert_eq!(world.ships[0].shields, 0.0);
    }

    #[test]
    fn tilted_touchdown_settles_toward_upright() {
        // Touch down with a slight clockwise tilt (within tolerance so we land).
        let tilted = -std::f32::consts::FRAC_PI_2 + 0.25;
        let mut world = world_with_ship(
            Vec2::new(640.0, 600.0),
            Vec2::new(0.0, 30.0),
            tilted,
        );
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
    fn wing_down_past_basin_tips_over_and_dies() {
        // Touchdown well past the upright basin → ship tips over instead of
        // settling, then chip damage drains shields to zero.
        let tilted = -std::f32::consts::FRAC_PI_2 + 1.2; // ~69° off upright
        let mut world = world_with_ship(
            Vec2::new(640.0, 600.0),
            Vec2::new(0.0, 30.0),
            tilted,
        );
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
        assert!(!world.ships[0].alive, "tipped ship should die from chip damage");
    }

    #[test]
    fn tipped_ship_cannot_lift_off() {
        // Two parallel worlds, identical setup. One mashes thrust, the
        // other holds nothing. If thrust is fully suppressed for tipped
        // ships, both worlds must produce identical state up to death —
        // including the rotation arc to lying-flat and the chip-damage
        // timeline.
        let setup = || {
            let mut w = world_with_ship(
                Vec2::new(640.0, 600.0),
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

        while with_thrust.ships[0].alive {
            with_thrust.tick([Input::THRUST, Input::empty()]);
            without_thrust.tick([Input::empty(), Input::empty()]);
            assert_eq!(
                with_thrust.ships[0].pos, without_thrust.ships[0].pos,
                "thrust must have no effect on a tipped ship"
            );
        }
        assert!(!with_thrust.ships[0].alive);
    }

    #[test]
    fn landed_ship_can_still_lift_off() {
        // First land soft and upright.
        let mut world = world_with_ship(
            Vec2::new(640.0, 600.0),
            Vec2::new(0.0, 30.0),
            UPRIGHT_ANGLE,
        );
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
        assert!(!world.ships[0].landed, "thrust should lift ship off the pad");
        assert!(world.ships[0].vel.y < 0.0, "ship should be moving upward");
    }

    #[test]
    fn pad_refuels_and_recharges_while_landed() {
        let mut world = world_with_ship(
            Vec2::new(640.0, 600.0),
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
        assert!(ship.shields > 20.0, "shields should regen, got {}", ship.shields);
        assert!(ship.fuel <= FUEL_MAX);
        assert!(ship.shields <= SHIELD_MAX);
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
        assert!(world.bullets.is_empty(), "bullet should be despawned by wall");
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
        let mut world = world_with_ship(
            Vec2::new(300.0, 690.0),
            Vec2::new(0.0, 100.0),
            0.0,
        );
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
            let p2 = if t < 60 { Input::ROTATE_RIGHT } else { Input::empty() };
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
