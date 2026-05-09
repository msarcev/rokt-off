use std::cell::RefCell;
use std::rc::Rc;

use macroquad::prelude::*;
use sim::Input;

use crate::session::{InputSource, poll_keyboard};

const STICK_DEADZONE_X: f32 = 20.0;
const STICK_THRUST_Y: f32 = -20.0;
const HYSTERESIS: f32 = 0.7;
const STICK_RING_R: f32 = 60.0;
const STICK_KNOB_R: f32 = 16.0;
const FIRE_R: f32 = 56.0;
const FIRE_MARGIN: f32 = 28.0;
const PAUSE_R: f32 = 22.0;
const PAUSE_MARGIN: f32 = 18.0;

pub struct TouchInput {
    stick_id: Option<u64>,
    stick_origin: Vec2,
    stick_pos: Vec2,
    fire_held: bool,
    was_active: bool,
    pause_id: Option<u64>,
    menu_pressed: bool,
    fire_ids: Vec<u64>,
    last_input: Input,
}

fn pause_center() -> Vec2 {
    vec2(screen_width() - PAUSE_MARGIN - PAUSE_R, PAUSE_MARGIN + PAUSE_R)
}

impl TouchInput {
    pub fn new() -> Self {
        Self {
            stick_id: None,
            stick_origin: Vec2::ZERO,
            stick_pos: Vec2::ZERO,
            fire_held: false,
            was_active: false,
            pause_id: None,
            menu_pressed: false,
            fire_ids: Vec::new(),
            last_input: Input::empty(),
        }
    }

    /// Edge-triggered: returns true once per pause-button tap, then clears.
    pub fn take_menu_press(&mut self) -> bool {
        let pressed = self.menu_pressed;
        self.menu_pressed = false;
        pressed
    }

    /// Sample current touches and return the resulting input bitmask. Call
    /// once per tick — keyboard composition happens at the call site.
    pub fn poll(&mut self) -> Input {
        let dpr = screen_dpi_scale();
        let mid_x = screen_width() * 0.5;
        let pause_c = pause_center();
        let dz = STICK_DEADZONE_X * dpr;
        let thrust = STICK_THRUST_Y * dpr;

        let mut stick_seen = false;
        let mut still_firing: Vec<u64> = Vec::with_capacity(self.fire_ids.len() + 1);

        for t in touches() {
            self.was_active = true;

            // Pause finger: own its lifecycle, never feed stick/fire.
            if self.pause_id == Some(t.id) {
                let inside = (t.position - pause_c).length() <= PAUSE_R;
                match t.phase {
                    TouchPhase::Ended => {
                        if inside {
                            self.menu_pressed = true;
                        }
                        self.pause_id = None;
                    }
                    TouchPhase::Cancelled => self.pause_id = None,
                    _ => {}
                }
                continue;
            }

            // New touch: claim a zone based on where it *started*.
            if t.phase == TouchPhase::Started {
                if (t.position - pause_c).length() <= PAUSE_R {
                    self.pause_id = Some(t.id);
                } else if t.position.x < mid_x {
                    if self.stick_id.is_none() {
                        self.stick_id = Some(t.id);
                        self.stick_origin = t.position;
                        self.stick_pos = t.position;
                        stick_seen = true;
                    }
                } else {
                    still_firing.push(t.id);
                }
                continue;
            }

            // Established stick finger.
            if self.stick_id == Some(t.id) {
                match t.phase {
                    TouchPhase::Ended | TouchPhase::Cancelled => self.stick_id = None,
                    _ => {
                        self.stick_pos = t.position;
                        stick_seen = true;
                    }
                }
                continue;
            }

            // Established fire finger.
            if self.fire_ids.contains(&t.id) {
                if !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                    still_firing.push(t.id);
                }
                continue;
            }
        }

        if !stick_seen {
            self.stick_id = None;
        }
        self.fire_ids = still_firing;
        self.fire_held = !self.fire_ids.is_empty();

        let mut input = Input::empty();
        if self.stick_id.is_some() {
            let d = self.stick_pos - self.stick_origin;
            // Hysteresis: lower the threshold once the input is already on, so
            // jitter near the edge doesn't chatter the bitflag on/off.
            let dz_left = if self.last_input.contains(Input::ROTATE_LEFT) { dz * HYSTERESIS } else { dz };
            let dz_right = if self.last_input.contains(Input::ROTATE_RIGHT) { dz * HYSTERESIS } else { dz };
            let thrust_off = if self.last_input.contains(Input::THRUST) { thrust * HYSTERESIS } else { thrust };
            if d.x < -dz_left {
                input |= Input::ROTATE_LEFT;
            }
            if d.x > dz_right {
                input |= Input::ROTATE_RIGHT;
            }
            if d.y < thrust_off {
                input |= Input::THRUST;
            }
        }
        if self.fire_held {
            input |= Input::FIRE;
        }
        self.last_input = input;
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

        let pc = pause_center();
        let pause_fill = if self.pause_id.is_some() { fill_held } else { fill_idle };
        draw_circle(pc.x, pc.y, PAUSE_R, pause_fill);
        draw_circle_lines(pc.x, pc.y, PAUSE_R, 2.0, ink);
        let bar_w = 4.0;
        let bar_h = PAUSE_R * 0.8;
        let gap = 4.0;
        draw_rectangle(pc.x - bar_w - gap * 0.5, pc.y - bar_h * 0.5, bar_w, bar_h, ink);
        draw_rectangle(pc.x + gap * 0.5, pc.y - bar_h * 0.5, bar_w, bar_h, ink);
    }
}

/// OR keyboard with the touch source each tick. Captures a clone of the
/// shared `TouchInput` so the overlay's draw call sees the same state.
pub fn input_source(touch: Rc<RefCell<TouchInput>>) -> InputSource {
    Box::new(move || poll_keyboard() | touch.borrow_mut().poll())
}
