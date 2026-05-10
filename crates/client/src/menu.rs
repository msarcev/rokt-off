use macroquad::prelude::*;

#[derive(Clone, Copy, Debug)]
pub enum MenuChoice {
    Local,
    Host,
    Join,
}

pub struct Menu {
    choice: Option<MenuChoice>,
}

impl Menu {
    pub fn new() -> Self {
        Self { choice: None }
    }

    pub fn take_choice(&mut self) -> Option<MenuChoice> {
        self.choice.take()
    }

    pub fn tick(&mut self) {
        let (mx, my) = mouse_position();
        let click = is_mouse_button_released(MouseButton::Left);
        if !click {
            return;
        }
        for (i, c) in CHOICES.iter().enumerate() {
            let r = button_rect(i);
            if r.contains(vec2(mx, my)) {
                self.choice = Some(*c);
                return;
            }
        }
    }

    pub fn draw(&self) {
        clear_background(Color::from_rgba(12, 14, 20, 255));

        let sw = screen_width();
        let sh = screen_height();
        let title = "ROKT-OFF";
        let title_size = 96.0;
        let dim = measure_text(title, None, title_size as u16, 1.0);
        draw_text(
            title,
            (sw - dim.width) / 2.0,
            sh * 0.30,
            title_size,
            Color::from_rgba(238, 232, 213, 255),
        );

        let (mx, my) = mouse_position();
        let mouse_pos = vec2(mx, my);
        for (i, c) in CHOICES.iter().enumerate() {
            let r = button_rect(i);
            let hovered = r.contains(mouse_pos);
            let fill = if hovered {
                Color::from_rgba(70, 70, 90, 255)
            } else {
                Color::from_rgba(40, 40, 55, 255)
            };
            draw_rectangle(r.x, r.y, r.w, r.h, fill);
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                2.0,
                Color::from_rgba(238, 232, 213, 255),
            );

            let label = match c {
                MenuChoice::Local => "LOCAL",
                MenuChoice::Host => "HOST 1V1",
                MenuChoice::Join => "JOIN 1V1",
            };
            let label_size = 36.0;
            let ldim = measure_text(label, None, label_size as u16, 1.0);
            draw_text(
                label,
                r.x + (r.w - ldim.width) / 2.0,
                r.y + (r.h + ldim.height) / 2.0,
                label_size,
                Color::from_rgba(238, 232, 213, 255),
            );
        }
    }
}

const CHOICES: [MenuChoice; 3] = [MenuChoice::Local, MenuChoice::Host, MenuChoice::Join];

const BUTTON_W: f32 = 360.0;
const BUTTON_H: f32 = 64.0;
const BUTTON_GAP: f32 = 20.0;

fn button_rect(i: usize) -> Rect {
    let sw = screen_width();
    let sh = screen_height();
    let x = (sw - BUTTON_W) / 2.0;
    let total = BUTTON_H * CHOICES.len() as f32 + BUTTON_GAP * (CHOICES.len() as f32 - 1.0);
    let y0 = sh * 0.45 - total / 2.0 + sh * 0.10;
    let y = y0 + (BUTTON_H + BUTTON_GAP) * i as f32;
    Rect::new(x, y, BUTTON_W, BUTTON_H)
}
