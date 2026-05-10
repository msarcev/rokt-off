use std::cell::RefCell;
use std::rc::Rc;

use macroquad::prelude::*;
use sim::Input;

use crate::session::{InputSource, poll_keyboard};

const BTN_R: f32 = 34.0;
const BTN_GAP: f32 = 10.0;
const EDGE_MARGIN: f32 = 22.0;
const BOTTOM_MARGIN: f32 = 28.0;
const PAUSE_R: f32 = 22.0;
const PAUSE_MARGIN: f32 = 18.0;

#[derive(Copy, Clone)]
struct Zone {
    center: Vec2,
    radius: f32,
}

impl Zone {
    fn contains(&self, p: Vec2) -> bool {
        (p - self.center).length() <= self.radius
    }
}

struct Layout {
    rotate_left: Zone,
    rotate_right: Zone,
    thrust: Zone,
    fire: Zone,
    pause: Zone,
}

fn layout() -> Layout {
    let r = BTN_R;
    let gap = BTN_GAP;
    let edge = EDGE_MARGIN;
    let bottom = BOTTOM_MARGIN;
    let pause_r = PAUSE_R;
    let pause_m = PAUSE_MARGIN;
    let sw = screen_width();
    let sh = screen_height();

    let row_y = sh - bottom - r;
    Layout {
        rotate_left: Zone {
            center: vec2(edge + r, row_y),
            radius: r,
        },
        rotate_right: Zone {
            center: vec2(edge + 3.0 * r + gap, row_y),
            radius: r,
        },
        thrust: Zone {
            center: vec2(edge + 2.0 * r + gap * 0.5, row_y - 2.0 * r - gap),
            radius: r,
        },
        fire: Zone {
            center: vec2(sw - edge - r, row_y),
            radius: r,
        },
        pause: Zone {
            center: vec2(sw - pause_m - pause_r, pause_m + pause_r),
            radius: pause_r,
        },
    }
}

fn portrait() -> bool {
    screen_height() > screen_width()
}

pub struct TouchInput {
    left_ids: Vec<u64>,
    right_ids: Vec<u64>,
    thrust_ids: Vec<u64>,
    fire_ids: Vec<u64>,
    pause_id: Option<u64>,
    menu_pressed: bool,
    landscape_active: bool,
}

impl TouchInput {
    pub fn new() -> Self {
        Self {
            left_ids: Vec::new(),
            right_ids: Vec::new(),
            thrust_ids: Vec::new(),
            fire_ids: Vec::new(),
            pause_id: None,
            menu_pressed: false,
            landscape_active: false,
        }
    }

    pub fn is_active(&self) -> bool {
        portrait() || self.landscape_active
    }

    pub fn take_menu_press(&mut self) -> bool {
        let pressed = self.menu_pressed;
        self.menu_pressed = false;
        pressed
    }

    pub fn poll(&mut self) -> Input {
        let l = layout();
        let dpr = screen_dpi_scale();

        for t in touches() {
            self.landscape_active = true;
            let pos = t.position / dpr;

            if self.pause_id == Some(t.id) {
                let inside = l.pause.contains(pos);
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

            if t.phase == TouchPhase::Started {
                if l.pause.contains(pos) {
                    self.pause_id = Some(t.id);
                } else if l.fire.contains(pos) {
                    self.fire_ids.push(t.id);
                } else if l.thrust.contains(pos) {
                    self.thrust_ids.push(t.id);
                } else if l.rotate_left.contains(pos) {
                    self.left_ids.push(t.id);
                } else if l.rotate_right.contains(pos) {
                    self.right_ids.push(t.id);
                }
                continue;
            }

            if matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.left_ids.retain(|&id| id != t.id);
                self.right_ids.retain(|&id| id != t.id);
                self.thrust_ids.retain(|&id| id != t.id);
                self.fire_ids.retain(|&id| id != t.id);
            }
        }

        let mut input = Input::empty();
        if !self.left_ids.is_empty() {
            input |= Input::ROTATE_LEFT;
        }
        if !self.right_ids.is_empty() {
            input |= Input::ROTATE_RIGHT;
        }
        if !self.thrust_ids.is_empty() {
            input |= Input::THRUST;
        }
        if !self.fire_ids.is_empty() {
            input |= Input::FIRE;
        }
        input
    }

    pub fn note_keyboard_press(&mut self) {
        self.landscape_active = false;
    }

    pub fn draw_overlay(&self) {
        if !self.is_active() {
            return;
        }
        let l = layout();
        let ink = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.7);
        let idle = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.18);
        let held = Color::new(238.0 / 255.0, 232.0 / 255.0, 213.0 / 255.0, 0.45);

        draw_button(
            l.rotate_left,
            "<",
            !self.left_ids.is_empty(),
            ink,
            idle,
            held,
        );
        draw_button(
            l.rotate_right,
            ">",
            !self.right_ids.is_empty(),
            ink,
            idle,
            held,
        );
        draw_button(l.thrust, "^", !self.thrust_ids.is_empty(), ink, idle, held);
        draw_button(l.fire, "FIRE", !self.fire_ids.is_empty(), ink, idle, held);
        draw_pause(l.pause, self.pause_id.is_some(), ink, idle, held);
    }
}

fn draw_button(z: Zone, label: &str, is_held: bool, ink: Color, idle: Color, held: Color) {
    let fill = if is_held { held } else { idle };
    draw_circle(z.center.x, z.center.y, z.radius, fill);
    draw_circle_lines(z.center.x, z.center.y, z.radius, 2.0, ink);
    let size = if label.len() <= 1 {
        z.radius * 0.9
    } else {
        z.radius * 0.55
    };
    let dim = measure_text(label, None, size as u16, 1.0);
    draw_text(
        label,
        z.center.x - dim.width * 0.5,
        z.center.y + dim.height * 0.5,
        size,
        ink,
    );
}

fn draw_pause(z: Zone, is_held: bool, ink: Color, idle: Color, held: Color) {
    let fill = if is_held { held } else { idle };
    draw_circle(z.center.x, z.center.y, z.radius, fill);
    draw_circle_lines(z.center.x, z.center.y, z.radius, 2.0, ink);
    let bar_w = 4.0;
    let bar_h = z.radius * 0.8;
    let gap = 4.0;
    draw_rectangle(
        z.center.x - bar_w - gap * 0.5,
        z.center.y - bar_h * 0.5,
        bar_w,
        bar_h,
        ink,
    );
    draw_rectangle(
        z.center.x + gap * 0.5,
        z.center.y - bar_h * 0.5,
        bar_w,
        bar_h,
        ink,
    );
}

pub fn input_source(touch: Rc<RefCell<TouchInput>>) -> InputSource {
    Box::new(move || poll_keyboard() | touch.borrow_mut().poll())
}
