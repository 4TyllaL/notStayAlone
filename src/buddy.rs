//! Amigo na tela: um segundo mascote que anda, visita o principal, brinca de
//! bolinha e dorme junto. Não fala (o balão é do principal) nem tem lembretes:
//! é só companhia. A janela repassa o mouse para a do mascote principal.

use std::ptr::null_mut;

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::*,
};

use crate::{
    gfx::Canvas,
    mascot::{Bounds, Mascot},
    rng::Rng,
    sprite::{Art, Frame},
};

/// Mouse na janela do amigo, repassado ao principal (wparam = mensagem original).
pub const WM_BUDDY_MOUSE: u32 = WM_APP + 14;

/// Intervalo (segundos) entre uma visita e outra ao mascote principal.
const VISIT_MIN: u32 = 40;
const VISIT_MAX: u32 = 120;

/// Onde o amigo chegou neste passo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arrived {
    /// Do lado do mascote principal (visita).
    Friend,
    /// No objeto que estava perseguindo.
    Toy,
}

pub struct Buddy {
    pub hwnd: HWND,
    pub art: Art,
    pub mascot: Mascot,
    canvas: Canvas,
    draw_scale: i32,
    rendered: Option<(Frame, bool)>,
    shown_at: (i32, i32),
    /// Segundos até a próxima visita.
    next_visit: f32,
    visiting: bool,
    /// Saindo de cima do amigo (depois de brincar, os dois acabam no mesmo lugar).
    stepping_aside: bool,
    rng: Rng,
}

impl Buddy {
    pub unsafe fn new(hwnd: HWND, art: Art, scale: i32, speed: u32, seed: u64, bounds: &Bounds) -> Buddy {
        let draw_scale = art.scale_for(scale);
        let body = art.size() as i32 * draw_scale;
        let mut mascot = Mascot::new(scale as f32, speed as f32, seed);
        mascot.resize(scale as f32, body as f32, bounds);
        mascot.drop_in(bounds);
        let mut rng = Rng::new(seed.rotate_left(29));
        Buddy {
            hwnd,
            canvas: Canvas::new(body, body),
            art,
            mascot,
            draw_scale,
            rendered: None,
            shown_at: (i32::MIN, i32::MIN),
            next_visit: rng.range(VISIT_MIN / 2, VISIT_MAX) as f32,
            visiting: false,
            stepping_aside: false,
            rng,
        }
    }

    /// Novo tamanho (a escala do mascote principal mudou).
    pub fn resize(&mut self, scale: i32, bounds: &Bounds) {
        self.draw_scale = self.art.scale_for(scale);
        let body = self.art.size() as i32 * self.draw_scale;
        if self.canvas.width != body {
            self.canvas = unsafe { Canvas::new(body, body) };
        }
        self.mascot.resize(scale as f32, body as f32, bounds);
        self.rendered = None;
    }

    /// Anda um passo. `toy` = centro do objeto que os dois perseguem; `friend` =
    /// (centro, tamanho) do principal.
    pub fn tick(&mut self, dt: f32, bounds: &Bounds, toy: Option<f32>, friend: (f32, f32)) -> Option<Arrived> {
        if let Some(x) = toy {
            self.visiting = false;
            self.mascot.move_target(x);
        } else if self.visiting {
            // Para do lado do amigo, não em cima dele.
            let (center, size) = friend;
            let side = if self.center() < center { -1.0 } else { 1.0 };
            self.mascot.move_target(center + side * size);
        } else if self.mascot.grounded() && !self.stepping_aside && self.overlaps(friend) {
            // Espaço pessoal: dá uns passos para o lado.
            let (center, size) = friend;
            let side = if self.center() < center { -1.0 } else { 1.0 };
            self.stepping_aside = true;
            self.mascot.set_target(Some(center + side * size * 1.2));
        } else if self.mascot.grounded() && !self.stepping_aside {
            self.next_visit -= dt;
            if self.next_visit <= 0.0 {
                self.visiting = true;
                self.next_visit = self.rng.range(VISIT_MIN, VISIT_MAX) as f32;
                let (center, size) = friend;
                let side = if self.center() < center { -1.0 } else { 1.0 };
                self.mascot.set_target(Some(center + side * size));
            }
        }
        self.mascot.update(dt, bounds);
        if !self.mascot.take_arrived() {
            return None;
        }
        if self.stepping_aside {
            self.stepping_aside = false;
            self.mascot.set_target(None);
            return None;
        }
        if self.visiting {
            self.visiting = false;
            self.mascot.set_target(None);
            return Some(Arrived::Friend);
        }
        toy.map(|_| Arrived::Toy)
    }

    /// Para de visitar (ex.: dormindo, sendo arrastado).
    pub fn stop_visiting(&mut self) {
        if self.visiting || self.stepping_aside {
            self.visiting = false;
            self.stepping_aside = false;
            self.mascot.set_target(None);
        }
    }

    /// Está praticamente em cima do amigo?
    fn overlaps(&self, (center, size): (f32, f32)) -> bool {
        (self.center() - center).abs() < size * 0.6
    }

    pub fn center(&self) -> f32 {
        self.mascot.x + self.mascot.size() / 2.0
    }

    /// Como `App::present`: redesenha só quando o frame muda.
    pub unsafe fn present(&mut self) {
        let frame = (self.mascot.frame(), self.mascot.facing_left);
        let pos = (self.mascot.x.round() as i32, self.mascot.y.round() as i32);
        if self.rendered != Some(frame) {
            self.art.draw(frame.0, frame.1, self.draw_scale as usize, self.canvas.pixels());
            self.canvas.present(self.hwnd, (pos != self.shown_at).then_some(pos));
            self.rendered = Some(frame);
        } else if pos != self.shown_at {
            SetWindowPos(self.hwnd, null_mut(), pos.0, pos.1, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        self.shown_at = pos;
    }

    /// Força redesenhar (depois de voltar a aparecer).
    pub fn invalidate(&mut self) {
        self.rendered = None;
        self.shown_at = (i32::MIN, i32::MIN);
    }
}

/// A janela do amigo só repassa o mouse para a janela do mascote principal.
pub unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP | WM_CAPTURECHANGED | WM_RBUTTONUP => {
            let owner = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HWND;
            SendMessageW(owner, WM_BUDDY_MOUSE, msg as WPARAM, 0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: Bounds = Bounds { left: 0.0, top: 0.0, right: 1600.0, floor: 900.0 };

    fn buddy_at(x: f32) -> Buddy {
        let art = Art::parse(crate::pack::EMBEDDED[3].1).unwrap();
        let mut b = unsafe { Buddy::new(std::ptr::null_mut(), art, 4, 2, 7, &BOUNDS) };
        b.mascot.x = x;
        b.mascot.y = BOUNDS.floor - b.mascot.size();
        b
    }

    /// Roda `secs` segundos de passos de 1/10 s com o amigo principal parado em `friend_x`.
    fn run(b: &mut Buddy, friend_x: f32, secs: u32) -> Vec<Arrived> {
        let size = b.mascot.size();
        (0..secs * 10).filter_map(|_| b.tick(0.1, &BOUNDS, None, (friend_x + size / 2.0, size))).collect()
    }

    #[test]
    fn steps_aside_when_on_top_of_the_friend() {
        let mut b = buddy_at(800.0);
        run(&mut b, 800.0, 10);
        assert!((b.center() - (800.0 + b.mascot.size() / 2.0)).abs() >= b.mascot.size() * 0.6);
    }

    #[test]
    fn visits_the_friend_and_stops_beside_it() {
        let mut b = buddy_at(100.0);
        let met = run(&mut b, 1200.0, 200);
        assert!(met.contains(&Arrived::Friend), "não visitou");
        assert!(met.iter().all(|a| *a == Arrived::Friend));
    }
}
