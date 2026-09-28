//! Objetos de brincadeira (bolinha, petisco): física simples, sem Win32.

use crate::{mascot::Bounds, sprite::SPRITE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Food,
    Ball,
}

pub struct Prop {
    pub kind: Kind,
    /// Canto superior esquerdo da janela.
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub held: bool,
    size: f32,
    scale: f32,
    /// Distância rolada (para alternar o frame da bolinha).
    rolled: f32,
}

impl Prop {
    pub fn new(kind: Kind, x: f32, y: f32, scale: f32) -> Prop {
        Prop { kind, x, y, vx: 0.0, vy: 0.0, held: false, size: SPRITE as f32 * scale, scale, rolled: 0.0 }
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.size / 2.0
    }

    pub fn on_floor(&self, b: &Bounds) -> bool {
        self.y + self.size >= b.floor - 1.0
    }

    /// Parado no chão: não precisa de timer.
    pub fn resting(&self, b: &Bounds) -> bool {
        !self.held && self.on_floor(b) && self.vx == 0.0 && self.vy == 0.0
    }

    /// Frame 0 ou 1 da bolinha, conforme ela rola.
    pub fn roll_frame(&self) -> usize {
        ((self.rolled / (self.scale * 3.0)) as i64).rem_euclid(2) as usize
    }

    pub fn throw(&mut self, vx: f32, vy: f32) {
        let limit = 3000.0 * self.scale / 4.0;
        self.held = false;
        self.vx = vx.clamp(-limit, limit);
        self.vy = vy.clamp(-limit, limit);
    }

    pub fn update(&mut self, dt: f32, b: &Bounds) {
        if self.held {
            return;
        }
        let k = self.scale / 4.0;
        let ball = self.kind == Kind::Ball;
        let floor = b.floor - self.size;
        let grounded = self.y >= floor - 0.5;

        if !grounded || self.vy < 0.0 {
            self.vy += 2600.0 * k * dt;
            self.vx -= self.vx * (0.4 * dt).min(1.0);
        } else {
            // Rolando/deslizando no chão.
            let friction = if ball { 1.2 } else { 8.0 };
            self.vx -= self.vx * (friction * dt).min(1.0);
            if self.vx.abs() < 12.0 * k {
                self.vx = 0.0;
            }
        }
        let dx = self.vx * dt;
        self.x += dx;
        self.y += self.vy * dt;
        if ball {
            self.rolled += dx;
        }

        let bounce = if ball { 0.7 } else { 0.2 };
        if self.x < b.left {
            self.x = b.left;
            self.vx = self.vx.abs() * bounce;
        } else if self.x + self.size > b.right {
            self.x = b.right - self.size;
            self.vx = -self.vx.abs() * bounce;
        }
        if self.y < b.top && self.vy < 0.0 {
            self.y = b.top;
            self.vy = -self.vy * 0.3;
        }
        if self.y >= floor {
            self.y = floor;
            let restitution = if ball { 0.55 } else { 0.0 };
            self.vy = -self.vy * restitution;
            if self.vy.abs() < 90.0 * k {
                self.vy = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Bounds = Bounds { left: 0.0, top: 0.0, right: 1920.0, floor: 1040.0 };

    fn settle(p: &mut Prop) -> u32 {
        for i in 0..5000 {
            p.update(0.016, &B);
            if p.resting(&B) {
                return i;
            }
        }
        panic!("não parou: x={} y={} vx={} vy={}", p.x, p.y, p.vx, p.vy);
    }

    #[test]
    fn ball_bounces_then_rests_on_floor() {
        let mut p = Prop::new(Kind::Ball, 500.0, 200.0, 4.0);
        p.throw(800.0, -600.0);
        settle(&mut p);
        assert_eq!(p.y, B.floor - 64.0);
        assert!(p.x >= B.left && p.x + 64.0 <= B.right);
    }

    #[test]
    fn food_does_not_bounce() {
        let mut p = Prop::new(Kind::Food, 500.0, 600.0, 4.0);
        let mut max_up = 0.0f32;
        for _ in 0..300 {
            p.update(0.016, &B);
            max_up = max_up.min(p.vy);
        }
        assert!(max_up >= 0.0, "subiu depois de cair");
        assert!(p.resting(&B));
    }

    #[test]
    fn ball_stays_inside_walls() {
        let mut p = Prop::new(Kind::Ball, 1800.0, 900.0, 4.0);
        p.throw(3000.0, 0.0);
        settle(&mut p);
        assert!(p.x + 64.0 <= B.right);
    }
}
