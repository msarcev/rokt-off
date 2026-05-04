//! Pure deterministic simulation for head-on-rs.
//!
//! No I/O, no global state, no `Instant::now()`, no unseeded RNG.
//! Anything that breaks rollback determinism does not belong here.

use bitflags::bitflags;
use glam::Vec2;

pub const TICK_HZ: u32 = 60;
pub const DT: f32 = 1.0 / TICK_HZ as f32;

pub const SHIP_THRUST: f32 = 220.0;
pub const SHIP_ROT_SPEED: f32 = 3.5;
pub const SHIP_ANGULAR_DAMPING: f32 = 0.90;
pub const DEFAULT_GRAVITY: f32 = 90.0;
pub const STARTING_FUEL: f32 = 1000.0;
pub const FUEL_BURN_PER_SEC: f32 = 40.0;

bitflags! {
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Input: u8 {
        const THRUST       = 0b0000_0001;
        const ROTATE_LEFT  = 0b0000_0010;
        const ROTATE_RIGHT = 0b0000_0100;
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
}

impl Ship {
    pub fn new(pos: Vec2, angle: f32) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            angle,
            angular_vel: 0.0,
            fuel: STARTING_FUEL,
        }
    }

    /// Unit vector pointing out the nose of the ship.
    pub fn forward(&self) -> Vec2 {
        Vec2::new(self.angle.cos(), self.angle.sin())
    }
}

#[derive(Clone, Debug)]
pub struct Level {
    pub size: Vec2,
    pub gravity: f32,
    pub spawn_points: [Vec2; 2],
}

impl Default for Level {
    fn default() -> Self {
        Self {
            size: Vec2::new(1280.0, 720.0),
            gravity: DEFAULT_GRAVITY,
            spawn_points: [Vec2::new(240.0, 200.0), Vec2::new(1040.0, 200.0)],
        }
    }
}

#[derive(Clone, Debug)]
pub struct World {
    pub level: Level,
    pub ships: [Ship; 2],
    pub tick: u64,
}

impl World {
    pub fn new(level: Level) -> Self {
        let ships = [
            Ship::new(level.spawn_points[0], -std::f32::consts::FRAC_PI_2),
            Ship::new(level.spawn_points[1], -std::f32::consts::FRAC_PI_2),
        ];
        Self {
            level,
            ships,
            tick: 0,
        }
    }

    /// Advance the world by one fixed-step tick. Pure function of (self, inputs).
    pub fn tick(&mut self, inputs: [Input; 2]) {
        let gravity = Vec2::new(0.0, self.level.gravity);
        for (ship, input) in self.ships.iter_mut().zip(inputs.iter()) {
            step_ship(ship, *input, gravity);
        }
        self.tick += 1;
    }
}

fn step_ship(ship: &mut Ship, input: Input, gravity: Vec2) {
    let mut angular_accel = 0.0;
    if input.contains(Input::ROTATE_LEFT) {
        angular_accel -= SHIP_ROT_SPEED;
    }
    if input.contains(Input::ROTATE_RIGHT) {
        angular_accel += SHIP_ROT_SPEED;
    }
    ship.angular_vel = ship.angular_vel * SHIP_ANGULAR_DAMPING + angular_accel * DT;
    ship.angle += ship.angular_vel * DT;

    let mut accel = gravity;
    if input.contains(Input::THRUST) && ship.fuel > 0.0 {
        accel += ship.forward() * SHIP_THRUST;
        ship.fuel = (ship.fuel - FUEL_BURN_PER_SEC * DT).max(0.0);
    }

    ship.vel += accel * DT;
    ship.pos += ship.vel * DT;
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
}
