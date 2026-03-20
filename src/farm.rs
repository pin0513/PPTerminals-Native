use eframe::egui;

// ─── Layout constants ──────────────────────────────────────────────────────────
const FARM_W: f32 = 500.0;
const FARM_H: f32 = 320.0;
const HEAVEN_H: f32 = 60.0;       // top heaven zone height
const GROUND_Y: f32 = 250.0;      // y where ground starts
const ASCEND_SPEED: f32 = 0.6;    // pixels per frame for ascension
const WALK_SPEED: f32 = 0.5;      // constant walk speed (pixels / frame)

// Palette of hen body colours (one per session, cycling)
const HEN_COLORS: &[(u8, u8, u8)] = &[
    (240, 136,  62),  // orange
    ( 88, 166, 255),  // blue
    (188, 140, 255),  // purple
    ( 63, 185,  80),  // green
    (248,  81,  73),  // red
    ( 57, 197, 207),  // cyan
    (227, 179,  65),  // gold
    (230, 100, 180),  // pink
];

// ─── Chicken state ─────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum MoveState { Idle, Walking, Pecking }

#[derive(Clone)]
struct Chicken {
    // position (farm-local coords)
    x: f32,
    y: f32,
    target_x: f32,
    target_y: f32,
    color: egui::Color32,
    /// Short display label (e.g. "A", "A1")
    label: String,
    /// "$0.xx" cost string
    cost: String,
    is_hen: bool,
    /// Unique index used as animation seed
    id: u64,
    frame: u64,
    state_timer: f32,
    move_state: MoveState,
    direction: f32,
    /// Sub-agent: which hen index this chick belongs to
    hen_idx: Option<usize>,
}

// ─── Ascending soul ───────────────────────────────────────────────────────────
#[derive(Clone)]
struct AscendingSoul {
    x: f32,
    y: f32,
    cost: String,
    color: egui::Color32,
    frame: u64,
    /// Alpha 0..255 for fade-in / float
    alpha: u8,
    done: bool,
}

// ─── Gravestone ───────────────────────────────────────────────────────────────
#[derive(Clone)]
struct Gravestone {
    x: f32,
    y: f32,
    cost: String,
}

// ─── Heaven soul (resting in heaven zone) ────────────────────────────────────
#[derive(Clone)]
struct HeavenSoul {
    x: f32,
    y: f32,           // farm-local, will be within HEAVEN_H
    drift_x: f32,     // pixels/frame drift direction
    cost: String,
    color: egui::Color32,
    frame: u64,
}

// ─── Main public struct ───────────────────────────────────────────────────────
pub struct AgentFarm {
    chickens: Vec<Chicken>,
    gravestones: Vec<Gravestone>,
    ascending: Vec<AscendingSoul>,
    heaven: Vec<HeavenSoul>,
    /// Global animation frame counter
    frame: u64,
    /// Next unique id for newly spawned chickens
    next_id: u64,
}

impl AgentFarm {
    pub fn new() -> Self {
        let mut farm = Self {
            chickens: Vec::new(),
            gravestones: Vec::new(),
            ascending: Vec::new(),
            heaven: Vec::new(),
            frame: 0,
            next_id: 0,
        };

        // Demo: one hen + two chicks
        let hen_idx = farm.add_hen("A", "$0.00", 0);
        farm.add_chick(hen_idx, "A1", "$0.00");
        farm.add_chick(hen_idx, "A2", "$0.00");
        farm
    }

    // ── Public API ─────────────────────────────────────────────────────────────

    /// Add a new mother hen for a Claude session.
    /// `session_hash` is used to pick a stable colour.
    /// Returns the index of the hen in `self.chickens`.
    pub fn add_hen(&mut self, label: &str, cost: &str, session_hash: u64) -> usize {
        let color_idx = (session_hash as usize) % HEN_COLORS.len();
        let (r, g, b) = HEN_COLORS[color_idx];
        let id = self.next_id;
        self.next_id += 1;

        // Spread hens horizontally
        let hen_count = self.chickens.iter().filter(|c| c.is_hen).count() as f32;
        let start_x = 40.0 + hen_count * 120.0;
        let start_x = (start_x % (FARM_W - 60.0)) + 30.0;

        self.chickens.push(Chicken {
            x: start_x,
            y: GROUND_Y - 12.0,
            target_x: start_x + 30.0,
            target_y: GROUND_Y - 10.0,
            color: egui::Color32::from_rgb(r, g, b),
            label: label.to_string(),
            cost: cost.to_string(),
            is_hen: true,
            id,
            frame: 0,
            state_timer: rand_f32(id) * 120.0 + 40.0,
            move_state: MoveState::Idle,
            direction: 1.0,
            hen_idx: None,
        });
        self.chickens.len() - 1
    }

    /// Add a sub-agent chick, attached to hen at `hen_idx`.
    pub fn add_chick(&mut self, hen_idx: usize, label: &str, cost: &str) {
        let id = self.next_id;
        self.next_id += 1;

        let (hx, hy) = if let Some(h) = self.chickens.get(hen_idx) {
            (h.x, h.y)
        } else {
            (100.0, GROUND_Y - 8.0)
        };

        // Offset slightly from hen
        let offset_x = (rand_f32(id) - 0.5) * 30.0;
        let sx = (hx + offset_x).clamp(15.0, FARM_W - 15.0);

        self.chickens.push(Chicken {
            x: sx,
            y: hy,
            target_x: sx + 10.0,
            target_y: hy,
            color: egui::Color32::from_rgb(240, 224, 96), // chicks are yellow
            label: label.to_string(),
            cost: cost.to_string(),
            is_hen: false,
            id,
            frame: 0,
            state_timer: rand_f32(id) * 80.0 + 20.0,
            move_state: MoveState::Walking,
            direction: 1.0,
            hen_idx: Some(hen_idx),
        });
    }

    /// Trigger ascension animation for a chick by label.
    pub fn ascend_chick(&mut self, label: &str) {
        if let Some(pos) = self.chickens.iter().position(|c| !c.is_hen && c.label == label) {
            let c = self.chickens.remove(pos);
            // Leave gravestone
            self.gravestones.push(Gravestone {
                x: c.x,
                y: c.y,
                cost: c.cost.clone(),
            });
            // Start ascending soul
            self.ascending.push(AscendingSoul {
                x: c.x,
                y: c.y,
                cost: c.cost,
                color: c.color,
                frame: 0,
                alpha: 255,
                done: false,
            });
        }
    }

    /// Update cost label for a chicken (hen or chick).
    pub fn set_cost(&mut self, label: &str, cost: &str) {
        if let Some(c) = self.chickens.iter_mut().find(|c| c.label == label) {
            c.cost = cost.to_string();
        }
    }

    // ── Render ─────────────────────────────────────────────────────────────────

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.frame = self.frame.wrapping_add(1);

        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(FARM_W, FARM_H),
            egui::Sense::hover(),
        );
        let o = response.rect.min; // farm origin in screen space

        // ── Background layers ──────────────────────────────────────────────────
        self.draw_sky(&painter, o);
        self.draw_heaven_zone(&painter, o);
        self.draw_stars(&painter, o);
        self.draw_ground(&painter, o);
        self.draw_fence(&painter, o);
        self.draw_session_zones(&painter, o);

        // ── Simulate ──────────────────────────────────────────────────────────
        self.update_chickens();
        self.update_ascending();
        self.update_heaven_souls();

        // ── Draw ──────────────────────────────────────────────────────────────
        self.draw_gravestones(&painter, o);
        self.draw_ascending_souls(&painter, o);
        self.draw_heaven_souls(&painter, o);
        self.draw_chickens(&painter, o);
    }

    // ── Sky + background ───────────────────────────────────────────────────────

    fn draw_sky(&self, painter: &egui::Painter, o: egui::Pos2) {
        painter.rect_filled(
            egui::Rect::from_min_size(o, egui::Vec2::new(FARM_W, GROUND_Y)),
            0.0,
            egui::Color32::from_rgb(8, 16, 32),
        );
    }

    fn draw_heaven_zone(&self, painter: &egui::Painter, o: egui::Pos2) {
        // Soft golden glow strip
        painter.rect_filled(
            egui::Rect::from_min_size(o, egui::Vec2::new(FARM_W, HEAVEN_H)),
            0.0,
            egui::Color32::from_rgba_premultiplied(200, 170, 40, 12),
        );
        // Bottom edge golden line
        painter.rect_filled(
            egui::Rect::from_min_size(
                o + egui::Vec2::new(0.0, HEAVEN_H - 1.0),
                egui::Vec2::new(FARM_W, 1.0),
            ),
            0.0,
            egui::Color32::from_rgba_premultiplied(220, 190, 60, 40),
        );
        // Label
        let pulse = ((self.frame as f32 * 0.04).sin() * 0.5 + 0.5) * 30.0 + 20.0;
        painter.text(
            o + egui::Vec2::new(FARM_W / 2.0, 6.0),
            egui::Align2::CENTER_TOP,
            "✦ HEAVEN ✦",
            egui::FontId::monospace(9.0),
            egui::Color32::from_rgba_premultiplied(240, 220, 80, pulse as u8),
        );
    }

    fn draw_stars(&self, painter: &egui::Painter, o: egui::Pos2) {
        for i in 0u64..28 {
            let sx = ((i.wrapping_mul(97).wrapping_add(13)) % FARM_W as u64) as f32;
            let sy = HEAVEN_H + ((i.wrapping_mul(53).wrapping_add(7)) % (GROUND_Y as u64 - HEAVEN_H as u64 - 10)) as f32;
            let blink_alpha = ((self.frame as f32 * 0.02 + i as f32 * 0.7).sin() * 0.5 + 0.5) * 40.0 + 10.0;
            painter.rect_filled(
                egui::Rect::from_min_size(
                    o + egui::Vec2::new(sx, sy),
                    egui::Vec2::splat(1.0),
                ),
                0.0,
                egui::Color32::from_rgba_premultiplied(255, 255, 255, blink_alpha as u8),
            );
        }
    }

    fn draw_ground(&self, painter: &egui::Painter, o: egui::Pos2) {
        painter.rect_filled(
            egui::Rect::from_min_max(
                o + egui::Vec2::new(0.0, GROUND_Y - 4.0),
                o + egui::Vec2::new(FARM_W, FARM_H),
            ),
            0.0,
            egui::Color32::from_rgb(22, 36, 14),
        );
        // Bright grass line
        painter.rect_filled(
            egui::Rect::from_min_size(
                o + egui::Vec2::new(0.0, GROUND_Y - 4.0),
                egui::Vec2::new(FARM_W, 2.0),
            ),
            0.0,
            egui::Color32::from_rgb(44, 74, 26),
        );
    }

    fn draw_fence(&self, painter: &egui::Painter, o: egui::Pos2) {
        let fc = egui::Color32::from_rgb(90, 62, 48);
        // Vertical posts
        for x in (10i32..(FARM_W as i32 - 10)).step_by(28) {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    o + egui::Vec2::new(x as f32, GROUND_Y - 22.0),
                    egui::Vec2::new(3.0, 22.0),
                ),
                0.0, fc,
            );
        }
        // Horizontal rails
        for ry in &[GROUND_Y - 18.0, GROUND_Y - 10.0] {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    o + egui::Vec2::new(10.0, *ry),
                    egui::Vec2::new(FARM_W - 20.0, 2.0),
                ),
                0.0, fc,
            );
        }
    }

    /// Draw dashed vertical separators between session zones.
    fn draw_session_zones(&self, painter: &egui::Painter, o: egui::Pos2) {
        // Find unique hen indices
        let hen_count = self.chickens.iter().filter(|c| c.is_hen).count();
        if hen_count <= 1 { return; }

        let zone_w = FARM_W / hen_count as f32;
        let dash_color = egui::Color32::from_rgba_premultiplied(88, 166, 255, 25);
        for i in 1..hen_count {
            let sep_x = zone_w * i as f32;
            // Draw dashed line
            let mut dy = GROUND_Y - 22.0;
            while dy > HEAVEN_H {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        o + egui::Vec2::new(sep_x, dy - 4.0),
                        egui::Vec2::new(1.0, 4.0),
                    ),
                    0.0, dash_color,
                );
                dy -= 8.0;
            }
        }
    }

    // ── Simulation ─────────────────────────────────────────────────────────────

    fn update_chickens(&mut self) {
        let frame = self.frame;

        // Collect hen positions first so chicks can reference them
        let hen_positions: Vec<(usize, f32, f32)> = self.chickens.iter().enumerate()
            .filter(|(_, c)| c.is_hen)
            .map(|(i, c)| (i, c.x, c.y))
            .collect();

        for chicken in &mut self.chickens {
            chicken.frame = chicken.frame.wrapping_add(1);
            chicken.state_timer -= 1.0;

            if chicken.state_timer <= 0.0 {
                let seed = chicken.id.wrapping_add(frame);
                let r1 = rand_f32(seed);
                let r2 = rand_f32(seed.wrapping_add(1));
                let r3 = rand_f32(seed.wrapping_add(2));

                // Hens roam freely; chicks orbit their hen
                if chicken.is_hen {
                    chicken.target_x = 20.0 + r1 * (FARM_W - 40.0);
                    chicken.target_y = GROUND_Y - 8.0 + r2 * 10.0;
                    // Pick state
                    chicken.move_state = if r3 < 0.3 {
                        MoveState::Idle
                    } else if r3 < 0.65 {
                        MoveState::Pecking
                    } else {
                        MoveState::Walking
                    };
                    chicken.state_timer = 60.0 + r1 * 140.0;
                } else if let Some(hidx) = chicken.hen_idx {
                    // Find hen pos
                    if let Some(&(_, hx, hy)) = hen_positions.iter().find(|&&(i, _, _)| i == hidx) {
                        let spread = 20.0 + r1 * 20.0;
                        chicken.target_x = (hx + (r1 - 0.5) * 2.0 * spread).clamp(15.0, FARM_W - 15.0);
                        chicken.target_y = (hy + (r2 - 0.5) * 8.0).clamp(GROUND_Y - 16.0, FARM_H - 10.0);
                    } else {
                        chicken.target_x = 15.0 + r1 * (FARM_W - 30.0);
                        chicken.target_y = GROUND_Y - 8.0 + r2 * 8.0;
                    }
                    chicken.move_state = if r3 < 0.4 { MoveState::Idle } else { MoveState::Walking };
                    chicken.state_timer = 40.0 + r1 * 80.0;
                }
            }

            // Move with constant speed
            if chicken.move_state == MoveState::Walking {
                let dx = chicken.target_x - chicken.x;
                let dy = chicken.target_y - chicken.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > 0.5 {
                    chicken.x += (dx / dist) * WALK_SPEED;
                    chicken.y += (dy / dist) * WALK_SPEED;
                    chicken.direction = if dx > 0.0 { 1.0 } else { -1.0 };
                }
            }

            // Clamp to ground
            chicken.x = chicken.x.clamp(15.0, FARM_W - 15.0);
            chicken.y = chicken.y.clamp(GROUND_Y - 16.0, FARM_H - 10.0);
        }
    }

    fn update_ascending(&mut self) {
        for soul in &mut self.ascending {
            soul.frame = soul.frame.wrapping_add(1);
            soul.y -= ASCEND_SPEED;
            // Fade in
            if soul.alpha < 240 && soul.frame < 30 {
                soul.alpha = ((soul.frame as f32 / 30.0) * 240.0) as u8;
            }
            // When reaching heaven zone, mark done
            if soul.y < HEAVEN_H + 5.0 {
                soul.done = true;
            }
        }

        // Promote done souls to heaven, then remove
        let mut promoted: Vec<HeavenSoul> = Vec::new();
        self.ascending.retain(|soul| {
            if soul.done {
                let id = soul.frame;
                promoted.push(HeavenSoul {
                    x: soul.x,
                    y: HEAVEN_H * 0.3 + rand_f32(id) * (HEAVEN_H * 0.6),
                    drift_x: (rand_f32(id.wrapping_add(1)) - 0.5) * 0.3,
                    cost: soul.cost.clone(),
                    color: soul.color,
                    frame: 0,
                });
                false
            } else {
                true
            }
        });
        self.heaven.extend(promoted);
    }

    fn update_heaven_souls(&mut self) {
        for soul in &mut self.heaven {
            soul.frame = soul.frame.wrapping_add(1);
            soul.x += soul.drift_x;
            // Bounce off walls
            if soul.x < 10.0 || soul.x > FARM_W - 10.0 {
                soul.drift_x = -soul.drift_x;
            }
            soul.x = soul.x.clamp(10.0, FARM_W - 10.0);
            // Gentle vertical float
            soul.y += ((soul.frame as f32 * 0.03).sin()) * 0.15;
            soul.y = soul.y.clamp(HEAVEN_H * 0.1, HEAVEN_H - 12.0);
        }
    }

    // ── Drawing helpers ────────────────────────────────────────────────────────

    fn draw_gravestones(&self, painter: &egui::Painter, o: egui::Pos2) {
        for g in &self.gravestones {
            let gp = o + egui::Vec2::new(g.x, g.y);
            // Stone base
            painter.rect_filled(
                egui::Rect::from_min_size(gp + egui::Vec2::new(-4.0, -9.0), egui::Vec2::new(8.0, 9.0)),
                1.0,
                egui::Color32::from_rgb(68, 68, 78),
            );
            // Cross vertical
            painter.rect_filled(
                egui::Rect::from_min_size(gp + egui::Vec2::new(-1.0, -15.0), egui::Vec2::new(2.0, 8.0)),
                0.0,
                egui::Color32::from_rgb(100, 100, 110),
            );
            // Cross horizontal
            painter.rect_filled(
                egui::Rect::from_min_size(gp + egui::Vec2::new(-4.0, -13.0), egui::Vec2::new(8.0, 2.0)),
                0.0,
                egui::Color32::from_rgb(100, 100, 110),
            );
            // Cost label
            painter.text(
                gp + egui::Vec2::new(0.0, 2.0),
                egui::Align2::CENTER_TOP,
                &g.cost,
                egui::FontId::monospace(6.0),
                egui::Color32::from_rgb(120, 130, 140),
            );
        }
    }

    fn draw_ascending_souls(&self, painter: &egui::Painter, o: egui::Pos2) {
        for soul in &self.ascending {
            let pos = o + egui::Vec2::new(soul.x, soul.y);
            let a = soul.alpha;
            let glow = soul.color.linear_multiply(0.6);

            // Glow halo
            painter.circle_filled(
                pos,
                8.0,
                egui::Color32::from_rgba_premultiplied(glow.r(), glow.g(), glow.b(), (a as u32 * 30 / 255) as u8),
            );

            // Soul body (small chick shape, glowing)
            painter.rect_filled(
                egui::Rect::from_min_size(pos + egui::Vec2::new(-3.0, -3.0), egui::Vec2::new(6.0, 5.0)),
                1.0,
                egui::Color32::from_rgba_premultiplied(soul.color.r(), soul.color.g(), soul.color.b(), a),
            );

            // Wings: two small rects flapping
            let wing_y = ((soul.frame as f32 * 0.3).sin() * 2.0) as f32;
            // Left wing
            painter.rect_filled(
                egui::Rect::from_min_size(pos + egui::Vec2::new(-7.0, -2.0 + wing_y), egui::Vec2::new(4.0, 2.0)),
                0.0,
                egui::Color32::from_rgba_premultiplied(soul.color.r(), soul.color.g(), soul.color.b(), (a as u32 * 180 / 255) as u8),
            );
            // Right wing
            painter.rect_filled(
                egui::Rect::from_min_size(pos + egui::Vec2::new(3.0, -2.0 - wing_y), egui::Vec2::new(4.0, 2.0)),
                0.0,
                egui::Color32::from_rgba_premultiplied(soul.color.r(), soul.color.g(), soul.color.b(), (a as u32 * 180 / 255) as u8),
            );

            // Cost label floating above
            painter.text(
                pos + egui::Vec2::new(0.0, -10.0),
                egui::Align2::CENTER_BOTTOM,
                &soul.cost,
                egui::FontId::monospace(7.0),
                egui::Color32::from_rgba_premultiplied(63, 185, 80, a),
            );
        }
    }

    fn draw_heaven_souls(&self, painter: &egui::Painter, o: egui::Pos2) {
        for soul in &self.heaven {
            let pos = o + egui::Vec2::new(soul.x, soul.y);
            let float_alpha: u8 = 160;

            // Soft circle aura
            painter.circle_filled(
                pos,
                5.0,
                egui::Color32::from_rgba_premultiplied(soul.color.r(), soul.color.g(), soul.color.b(), 30),
            );
            // Small circle body
            painter.circle_filled(
                pos,
                3.0,
                egui::Color32::from_rgba_premultiplied(soul.color.r(), soul.color.g(), soul.color.b(), float_alpha),
            );
            // Tiny star above
            let star_y = ((soul.frame as f32 * 0.05).sin() * 1.5) as f32;
            painter.text(
                pos + egui::Vec2::new(0.0, -8.0 + star_y),
                egui::Align2::CENTER_BOTTOM,
                "✦",
                egui::FontId::monospace(5.0),
                egui::Color32::from_rgba_premultiplied(240, 220, 80, 100),
            );
            // Cost label
            painter.text(
                pos + egui::Vec2::new(0.0, 4.0),
                egui::Align2::CENTER_TOP,
                &soul.cost,
                egui::FontId::monospace(6.0),
                egui::Color32::from_rgba_premultiplied(180, 200, 180, 140),
            );
        }
    }

    fn draw_chickens(&self, painter: &egui::Painter, o: egui::Pos2) {
        for chicken in &self.chickens {
            // Bob animation: walking bobs, idle/pecking stays
            let bob = if chicken.move_state == MoveState::Walking {
                (chicken.frame as f32 * 0.35).sin() * 1.5
            } else {
                0.0
            };
            // Peck animation: head dips forward/down
            let peck_offset = if chicken.move_state == MoveState::Pecking {
                ((chicken.frame as f32 * 0.15).sin().max(0.0)) * 3.0
            } else {
                0.0
            };

            let pos = o + egui::Vec2::new(chicken.x, chicken.y + bob);
            let d = chicken.direction; // 1.0 = right, -1.0 = left

            if chicken.is_hen {
                self.draw_hen(painter, pos, chicken.color, d, peck_offset);
            } else {
                self.draw_chick(painter, pos, chicken.color, d);
            }

            // Label above head
            painter.text(
                pos + egui::Vec2::new(0.0, -16.0),
                egui::Align2::CENTER_BOTTOM,
                &chicken.label,
                egui::FontId::monospace(8.0),
                egui::Color32::from_rgba_premultiplied(200, 210, 220, 180),
            );
            // Cost above label
            if !chicken.cost.is_empty() && chicken.cost != "$0.00" {
                painter.text(
                    pos + egui::Vec2::new(0.0, -23.0),
                    egui::Align2::CENTER_BOTTOM,
                    &chicken.cost,
                    egui::FontId::monospace(7.0),
                    egui::Color32::from_rgb(63, 185, 80),
                );
            }
        }
    }

    fn draw_hen(
        &self,
        painter: &egui::Painter,
        pos: egui::Pos2,
        color: egui::Color32,
        d: f32,
        peck: f32,
    ) {
        let tail_x = if d > 0.0 { -10.0 } else { 6.0 };
        let head_x = if d > 0.0 { 6.0 } else { -10.0 };
        let beak_x = if d > 0.0 { 12.0 } else { -15.0 };
        let eye_x  = if d > 0.0 { 9.0 } else { -11.0 };
        let comb_x = if d > 0.0 { 7.0 } else { -11.0 };

        // Body
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(-6.0, -5.0), egui::Vec2::new(12.0, 8.0)),
            1.0, color,
        );
        // Tail feather (rounded bump)
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(tail_x, -7.0), egui::Vec2::new(5.0, 5.0)),
            2.0, color,
        );
        // Head (shifted up by peck when pecking)
        let head_y = -9.0 + peck;
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(head_x, head_y), egui::Vec2::new(6.0, 6.0)),
            1.0, color,
        );
        // Eye
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(eye_x, head_y + 1.0), egui::Vec2::new(2.0, 2.0)),
            0.0, egui::Color32::WHITE,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(eye_x + 0.5, head_y + 1.5), egui::Vec2::new(1.0, 1.0)),
            0.0, egui::Color32::BLACK,
        );
        // Beak
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(beak_x, head_y + 3.0), egui::Vec2::new(3.0, 2.0)),
            0.0, egui::Color32::from_rgb(230, 180, 40),
        );
        // Comb (2 bumps)
        let comb_color = egui::Color32::from_rgb(210, 40, 40);
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(comb_x, head_y - 3.0), egui::Vec2::new(2.0, 3.0)),
            0.0, comb_color,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(comb_x + 2.5, head_y - 4.0), egui::Vec2::new(2.0, 3.0)),
            0.0, comb_color,
        );
        // Wattle (red chin)
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(head_x + 1.0, head_y + 5.0), egui::Vec2::new(2.0, 3.0)),
            1.0, comb_color,
        );
        // Legs
        let leg_anim = ((self.frame as f32 * 0.25).sin() * 1.5) as f32;
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(-3.0 + leg_anim, 3.0), egui::Vec2::new(2.0, 4.0)),
            0.0, egui::Color32::from_rgb(220, 170, 40),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(1.0 - leg_anim, 3.0), egui::Vec2::new(2.0, 4.0)),
            0.0, egui::Color32::from_rgb(220, 170, 40),
        );
    }

    fn draw_chick(&self, painter: &egui::Painter, pos: egui::Pos2, color: egui::Color32, d: f32) {
        let head_x = if d > 0.0 { 1.0 } else { -5.0 };
        let beak_x = if d > 0.0 { 4.5 } else { -5.5 };
        let eye_x  = if d > 0.0 { 3.0 } else { -5.0 };

        // Fluffy body
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(-4.0, -3.0), egui::Vec2::new(8.0, 6.0)),
            2.0, color,
        );
        // Head
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(head_x, -6.0), egui::Vec2::new(5.0, 4.0)),
            1.0, color,
        );
        // Eye
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(eye_x, -5.0), egui::Vec2::new(1.0, 1.0)),
            0.0, egui::Color32::BLACK,
        );
        // Beak
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(beak_x, -3.5), egui::Vec2::new(2.0, 1.0)),
            0.0, egui::Color32::from_rgb(220, 170, 40),
        );
        // Tiny legs
        let leg_anim = ((self.frame as f32 * 0.35).sin() * 1.0) as f32;
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(-2.0 + leg_anim, 3.0), egui::Vec2::new(1.0, 3.0)),
            0.0, egui::Color32::from_rgb(220, 170, 40),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(pos + egui::Vec2::new(1.0 - leg_anim, 3.0), egui::Vec2::new(1.0, 3.0)),
            0.0, egui::Color32::from_rgb(220, 170, 40),
        );
    }
}

// ── PRNG helper ────────────────────────────────────────────────────────────────
fn rand_f32(seed: u64) -> f32 {
    let h = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (h >> 33) as f32 / (1u64 << 31) as f32
}
