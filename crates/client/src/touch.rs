use std::cell::RefCell;
use std::rc::Rc;

use macroquad::prelude::*;
use sim::Input;

use crate::session::{InputSource, poll_keyboard};

const STICK_DEADZONE_X: f32 = 20.0;
const STICK_THRUST_Y: f32 = -20.0;
const STICK_RING_R: f32 = 60.0;
const STICK_KNOB_R: f32 = 16.0;
const FIRE_R: f32 = 56.0;
const FIRE_MARGIN: f32 = 28.0;

pub struct TouchInput {
    stick_id: Option<u64>,
    stick_origin: Vec2,
    stick_pos: Vec2,
    fire_held: bool,
    was_active: bool,
}

impl TouchInput {
    pub fn new() -> Self {
        Self {
            stick_id: None,
            stick_origin: Vec2::ZERO,
            stick_pos: Vec2::ZERO,
            fire_held: false,
            was_active: false,
        }
    }

    /// Sample current touches and return the resulting input bitmask. Call
    /// once per tick — keyboard composition happens at the call site.
    pub fn poll(&mut self) -> Input {
        let mid_x = screen_width() * 0.5;
        let mut fire = false;
        let mut stick_seen = false;
        for t in touches() {
            self.was_active = true;
            if t.position.x < mid_x {
                if self.stick_id.is_none() {
                    self.stick_id = Some(t.id);
                    self.stick_origin = t.position;
                }
                if self.stick_id == Some(t.id) {
                    self.stick_pos = t.position;
                    stick_seen = true;
                }
            } else {
                fire = true;
            }
        }
        if !stick_seen {
            self.stick_id = None;
        }
        self.fire_held = fire;

        let mut input = Input::empty();
        if self.stick_id.is_some() {
            let d = self.stick_pos - self.stick_origin;
            if d.x < -STICK_DEADZONE_X {
                input |= Input::ROTATE_LEFT;
            }
            if d.x > STICK_DEADZONE_X {
                input |= Input::ROTATE_RIGHT;
            }
            if d.y < STICK_THRUST_Y {
                input |= Input::THRUST;
            }
        }
        if fire {
            input |= Input::FIRE;
        }
        input
    }

    /// User reached for the keyboard — hide the overlay until the next touch.
    pub fn note_keyboard_press(&mut self) {
        self.was_active = false;
    }

    pub fn draw_overlay(&self) {
        if !self.was_active {
            return;
        }
        let sw = screen_width();
        let sh = screen_height();
        let ink = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.7);
        let fill_idle = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.18);
        let fill_held = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.45);

        if self.stick_id.is_some() {
            draw_circle_lines(
                self.stick_origin.x,
                self.stick_origin.y,
                STICK_RING_R,
                2.0,
                ink,
            );
            draw_circle(self.stick_pos.x, self.stick_pos.y, STICK_KNOB_R, ink);
        }

        let fx = sw - FIRE_MARGIN - FIRE_R;
        let fy = sh - FIRE_MARGIN - FIRE_R;
        let fill = if self.fire_held { fill_held } else { fill_idle };
        draw_circle(fx, fy, FIRE_R, fill);
        draw_circle_lines(fx, fy, FIRE_R, 2.0, ink);
        let label = "FIRE";
        let dim = measure_text(label, None, 22, 1.0);
        draw_text(label, fx - dim.width * 0.5, fy + dim.height * 0.5, 22.0, ink);
    }
}

/// OR keyboard with the touch source each tick. Captures a clone of the
/// shared `TouchInput` so the overlay's draw call sees the same state.
pub fn input_source(touch: Rc<RefCell<TouchInput>>) -> InputSource {
    Box::new(move || poll_keyboard() | touch.borrow_mut().poll())
}
