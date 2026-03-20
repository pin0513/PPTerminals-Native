use eframe::egui;
use std::collections::HashMap;

const FARM_W: f32 = 400.0;
const FARM_H: f32 = 260.0;
const GROUND_Y: f32 = 200.0;

struct Chicken {
    x: f32, y: f32,
    target_x: f32, target_y: f32,
    color: egui::Color32,
    label: String,
    cost: String,
    is_hen: bool,
    frame: u64,
    state_timer: f32,
    direction: f32, // 1.0 = right, -1.0 = left
}

pub struct AgentFarm {
    chickens: Vec<Chicken>,
    gravestones: Vec<(f32, f32, String)>,
    frame: u64,
}

impl AgentFarm {
    pub fn new() -> Self {
        // Start with a demo hen
        Self {
            chickens: vec![
                Chicken {
                    x: 100.0, y: GROUND_Y - 10.0,
                    target_x: 200.0, target_y: GROUND_Y - 8.0,
                    color: egui::Color32::from_rgb(240, 136, 62),
                    label: "A".to_string(),
                    cost: "$0.00".to_string(),
                    is_hen: true,
                    frame: 0,
                    state_timer: 60.0,
                    direction: 1.0,
                },
            ],
            gravestones: Vec::new(),
            frame: 0,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.frame += 1;
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(FARM_W, FARM_H),
            egui::Sense::click(),
        );
        let rect = response.rect;
        let origin = rect.min;

        // Sky
        painter.rect_filled(
            egui::Rect::from_min_max(origin, origin + egui::Vec2::new(FARM_W, GROUND_Y)),
            0.0,
            egui::Color32::from_rgb(10, 22, 40),
        );

        // Heaven zone
        painter.rect_filled(
            egui::Rect::from_min_max(origin, origin + egui::Vec2::new(FARM_W, 50.0)),
            0.0,
            egui::Color32::from_rgba_premultiplied(240, 224, 96, 8),
        );
        painter.text(
            origin + egui::Vec2::new(FARM_W / 2.0, 10.0),
            egui::Align2::CENTER_TOP,
            "✦ HEAVEN ✦",
            egui::FontId::monospace(8.0),
            egui::Color32::from_rgba_premultiplied(240, 224, 96, 30),
        );

        // Stars
        for i in 0..20 {
            let sx = ((i * 97 + 13) % FARM_W as u32) as f32;
            let sy = ((i * 53 + 7) % (GROUND_Y as u32 - 20)) as f32;
            painter.rect_filled(
                egui::Rect::from_min_size(origin + egui::Vec2::new(sx, sy), egui::Vec2::splat(1.0)),
                0.0,
                egui::Color32::from_rgba_premultiplied(255, 255, 255, 20),
            );
        }

        // Ground
        painter.rect_filled(
            egui::Rect::from_min_max(
                origin + egui::Vec2::new(0.0, GROUND_Y - 5.0),
                origin + egui::Vec2::new(FARM_W, FARM_H),
            ),
            0.0,
            egui::Color32::from_rgb(26, 40, 16),
        );

        // Fence
        let fence_color = egui::Color32::from_rgb(92, 64, 51);
        for x in (10..FARM_W as i32 - 10).step_by(30) {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    origin + egui::Vec2::new(x as f32, GROUND_Y - 20.0),
                    egui::Vec2::new(3.0, 20.0),
                ),
                0.0, fence_color,
            );
        }
        painter.rect_filled(
            egui::Rect::from_min_size(origin + egui::Vec2::new(10.0, GROUND_Y - 18.0), egui::Vec2::new(FARM_W - 20.0, 2.0)),
            0.0, fence_color,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(origin + egui::Vec2::new(10.0, GROUND_Y - 10.0), egui::Vec2::new(FARM_W - 20.0, 2.0)),
            0.0, fence_color,
        );

        // Gravestones
        for (gx, gy, cost) in &self.gravestones {
            let gp = origin + egui::Vec2::new(*gx, *gy);
            painter.rect_filled(egui::Rect::from_min_size(gp + egui::Vec2::new(-3.0, -8.0), egui::Vec2::new(6.0, 8.0)), 0.0, egui::Color32::from_rgb(74, 74, 74));
            painter.text(gp + egui::Vec2::new(0.0, 8.0), egui::Align2::CENTER_TOP, cost, egui::FontId::monospace(6.0), egui::Color32::from_rgb(139, 148, 158));
        }

        // Update and draw chickens
        for chicken in &mut self.chickens {
            chicken.frame += 1;
            chicken.state_timer -= 1.0;

            if chicken.state_timer <= 0.0 {
                chicken.target_x = 30.0 + rand_f32(chicken.frame) * (FARM_W - 60.0);
                chicken.target_y = GROUND_Y - 10.0 + rand_f32(chicken.frame + 100) * 12.0;
                chicken.state_timer = 60.0 + rand_f32(chicken.frame + 200) * 120.0;
            }

            // Move
            let dx = chicken.target_x - chicken.x;
            let dy = chicken.target_y - chicken.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > 1.0 {
                chicken.x += (dx / dist) * 0.4;
                chicken.y += (dy / dist) * 0.4;
                chicken.direction = if dx > 0.0 { 1.0 } else { -1.0 };
            }
            chicken.x = chicken.x.clamp(15.0, FARM_W - 15.0);
            chicken.y = chicken.y.clamp(GROUND_Y - 15.0, FARM_H - 15.0);

            let bob = (chicken.frame as f32 * 0.3).sin() * 1.5;
            let pos = origin + egui::Vec2::new(chicken.x, chicken.y + bob);

            if chicken.is_hen {
                // Hen body
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(-6.0, -4.0), egui::Vec2::new(12.0, 8.0)), 0.0, chicken.color);
                // Head
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(4.0 * chicken.direction, -8.0), egui::Vec2::new(6.0, 6.0)), 0.0, chicken.color);
                // Eye
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(7.0 * chicken.direction, -7.0), egui::Vec2::new(2.0, 2.0)), 0.0, egui::Color32::WHITE);
                // Beak
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(10.0 * chicken.direction, -5.0), egui::Vec2::new(3.0, 2.0)), 0.0, egui::Color32::from_rgb(240, 192, 64));
                // Comb
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(5.0 * chicken.direction, -10.0), egui::Vec2::new(2.0, 3.0)), 0.0, egui::Color32::from_rgb(224, 48, 48));
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(7.0 * chicken.direction, -11.0), egui::Vec2::new(2.0, 3.0)), 0.0, egui::Color32::from_rgb(224, 48, 48));
                // Legs
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(-2.0, 4.0), egui::Vec2::new(1.0, 4.0)), 0.0, egui::Color32::from_rgb(240, 192, 64));
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(2.0, 4.0), egui::Vec2::new(1.0, 4.0)), 0.0, egui::Color32::from_rgb(240, 192, 64));
            } else {
                // Chick
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(-3.0, -2.0), egui::Vec2::new(6.0, 5.0)), 0.0, egui::Color32::from_rgb(240, 224, 96));
                painter.rect_filled(egui::Rect::from_min_size(pos + egui::Vec2::new(1.0, -4.0), egui::Vec2::new(4.0, 3.0)), 0.0, egui::Color32::from_rgb(240, 224, 96));
            }

            // Label
            painter.text(pos + egui::Vec2::new(0.0, -14.0), egui::Align2::CENTER_BOTTOM, &chicken.label, egui::FontId::monospace(8.0), egui::Color32::from_rgba_premultiplied(255, 255, 255, 150));
            // Cost
            if !chicken.cost.is_empty() {
                painter.text(pos + egui::Vec2::new(0.0, -20.0), egui::Align2::CENTER_BOTTOM, &chicken.cost, egui::FontId::monospace(7.0), egui::Color32::from_rgb(63, 185, 80));
            }
        }
    }
}

fn rand_f32(seed: u64) -> f32 {
    ((seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)) >> 33) as f32 / (1u64 << 31) as f32
}
