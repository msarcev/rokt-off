use macroquad::prelude::{Camera2D, Vec2, vec2};

pub struct FollowCamera {
    pub target: Vec2,
    pub view_size: Vec2,
    pub smoothing: f32,
}

impl FollowCamera {
    pub fn new(target: Vec2, view_size: Vec2, smoothing: f32) -> Self {
        Self { target, view_size, smoothing }
    }

    /// Snap the camera centre to a world position (no smoothing). Used at
    /// match start so the first frame doesn't lerp from a stale target.
    pub fn snap_to(&mut self, pos: Vec2, level_size: Vec2) {
        self.target = pos;
        self.target = clamp_target(self.target, self.view_size, level_size);
    }

    /// Lerp toward `player_pos` and clamp the visible rect inside the level.
    /// `dt` is wall-clock seconds — uses `1 - exp(-smoothing * dt)` so the
    /// approach rate is frame-rate independent.
    pub fn update(&mut self, player_pos: Vec2, level_size: Vec2, dt: f32) {
        let t = 1.0 - (-self.smoothing * dt).exp();
        self.target = self.target.lerp(player_pos, t);
        self.target = clamp_target(self.target, self.view_size, level_size);
    }

    /// Build the `Camera2D` for this frame. `viewport` is the GL pixel rect
    /// the play area occupies inside the window.
    pub fn macroquad_camera(&self, viewport: (i32, i32, i32, i32)) -> Camera2D {
        Camera2D {
            target: self.target,
            zoom: vec2(2.0 / self.view_size.x, 2.0 / self.view_size.y),
            viewport: Some(viewport),
            ..Default::default()
        }
    }
}

fn clamp_target(target: Vec2, view: Vec2, level: Vec2) -> Vec2 {
    vec2(
        clamp_axis(target.x, view.x, level.x),
        clamp_axis(target.y, view.y, level.y),
    )
}

fn clamp_axis(t: f32, view: f32, level: f32) -> f32 {
    // If the level is smaller than the view, pin the camera to the level
    // centre — otherwise the clamp range inverts and the ship can leave the
    // visible rect.
    if level <= view {
        level * 0.5
    } else {
        t.clamp(view * 0.5, level - view * 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_clamps_to_level() {
        let mut cam = FollowCamera::new(Vec2::ZERO, vec2(1280.0, 720.0), 8.0);
        cam.snap_to(vec2(10.0, 10.0), vec2(2560.0, 1440.0));
        assert_eq!(cam.target, vec2(640.0, 360.0));
    }

    #[test]
    fn small_level_pins_centre() {
        let mut cam = FollowCamera::new(Vec2::ZERO, vec2(1280.0, 720.0), 8.0);
        cam.snap_to(vec2(100.0, 100.0), vec2(1280.0, 720.0));
        assert_eq!(cam.target, vec2(640.0, 360.0));
    }

    #[test]
    fn update_lerps_toward_player() {
        let mut cam = FollowCamera::new(vec2(640.0, 360.0), vec2(1280.0, 720.0), 8.0);
        cam.update(vec2(1900.0, 700.0), vec2(2560.0, 1440.0), 1.0 / 60.0);
        let t = 1.0 - (-8.0_f32 / 60.0).exp();
        let expected_x = 640.0 + (1900.0 - 640.0) * t;
        let expected_y = 360.0 + (700.0 - 360.0) * t;
        assert!((cam.target.x - expected_x).abs() < 1e-3);
        assert!((cam.target.y - expected_y).abs() < 1e-3);
    }
}
