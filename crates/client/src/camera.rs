use macroquad::prelude::{Camera2D, Vec2, vec2};

/// View-space dimensions at `zoom = 1.0`. The camera always shows
/// `PLAY_W / zoom` × `PLAY_H / zoom` world pixels. Centralised here so the
/// camera doesn't depend on `lib.rs` constants.
pub const VIEW_W: f32 = 1280.0;
pub const VIEW_H: f32 = 720.0;

pub struct FollowCamera {
    pub target: Vec2,
    pub zoom: f32,
    pub smoothing: f32,
}

impl FollowCamera {
    pub fn new(target: Vec2, zoom: f32, smoothing: f32) -> Self {
        Self { target, zoom, smoothing }
    }

    /// Snap the camera centre to a world position (no smoothing). Used at
    /// match start so the first frame doesn't lerp from a stale target.
    pub fn snap_to(&mut self, pos: Vec2, level_size: Vec2) {
        self.target = pos;
        self.target = clamp_target(self.target, self.view_size(), level_size);
    }

    /// Lerp toward `player_pos` and clamp the visible rect inside the level.
    /// `dt` is wall-clock seconds — uses `1 - exp(-smoothing * dt)` so the
    /// approach rate is frame-rate independent.
    pub fn update(&mut self, player_pos: Vec2, level_size: Vec2, dt: f32) {
        let t = 1.0 - (-self.smoothing * dt).exp();
        self.target = self.target.lerp(player_pos, t);
        self.target = clamp_target(self.target, self.view_size(), level_size);
    }

    /// Build the `Camera2D` for this frame. `viewport` is the GL pixel rect
    /// the play area occupies inside the window — same convention used by
    /// `draw_playing` today.
    pub fn macroquad_camera(&self, viewport: (i32, i32, i32, i32)) -> Camera2D {
        let view = self.view_size();
        Camera2D {
            target: self.target,
            zoom: vec2(2.0 / view.x, 2.0 / view.y),
            viewport: Some(viewport),
            ..Default::default()
        }
    }

    fn view_size(&self) -> Vec2 {
        vec2(VIEW_W, VIEW_H) / self.zoom
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
        let mut cam = FollowCamera::new(Vec2::ZERO, 1.0, 8.0);
        cam.snap_to(vec2(10.0, 10.0), vec2(2560.0, 1440.0));
        assert_eq!(cam.target, vec2(640.0, 360.0));
    }

    #[test]
    fn small_level_pins_centre() {
        let mut cam = FollowCamera::new(Vec2::ZERO, 1.0, 8.0);
        cam.snap_to(vec2(100.0, 100.0), vec2(1280.0, 720.0));
        assert_eq!(cam.target, vec2(640.0, 360.0));
    }

    #[test]
    fn update_lerps_toward_player() {
        let mut cam = FollowCamera::new(vec2(640.0, 360.0), 1.0, 8.0);
        cam.update(vec2(1900.0, 700.0), vec2(2560.0, 1440.0), 1.0 / 60.0);
        let t = 1.0 - (-8.0_f32 / 60.0).exp();
        let expected_x = 640.0 + (1900.0 - 640.0) * t;
        let expected_y = 360.0 + (700.0 - 360.0) * t;
        assert!((cam.target.x - expected_x).abs() < 1e-3);
        assert!((cam.target.y - expected_y).abs() < 1e-3);
    }
}
