//! Comportamento do mascote: máquina de estados + física simples.
//! Não conhece nada de Win32 — só posições em pixels de tela.

use crate::rng::Rng;
use crate::sprite::{Frame, SPRITE};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Idle,
    Walk,
    Yawn,
    Sleep,
    Happy,
    Held,
    Fall,
    Land,
    /// Correndo atrás de um objeto (bolinha, petisco).
    Chase,
    Eat,
}

/// Área útil do monitor (sem a barra de tarefas). `floor` é onde ele pisa.
#[derive(Clone, Copy, Default, Debug)]
pub struct Bounds {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub floor: f32,
}

/// Ticks de 100 ms acordado antes de começar a ter sono: 3 min de dia, 1 min à noite.
pub const SLEEPY_DAY: u32 = 3 * 60 * 10;
pub const SLEEPY_NIGHT: u32 = 60 * 10;

pub struct Mascot {
    /// Canto superior esquerdo da janela.
    pub x: f32,
    pub y: f32,
    pub facing_left: bool,
    size: f32,
    scale: f32,
    speed: f32,
    state: State,
    vx: f32,
    vy: f32,
    ticks_left: u32,
    anim: u32,
    blink_in: u32,
    blinking: u32,
    /// "Manias" quando parado: olhar em volta ou abanar o rabo/mexer a orelha.
    fidget_in: u32,
    fidget: u32,
    fidget_look: bool,
    awake: u32,
    sleepy_after: u32,
    /// Chance (%) de ficar feliz do nada — cresce com o afeto.
    cheer: u32,
    /// Dorme até alguém acordá-lo (você saiu do PC).
    deep: bool,
    /// Está com balão de fala aberto: fica parado.
    talking: bool,
    /// Bateria baixa: anda devagar.
    tired: bool,
    /// Você está digitando: não sai do lugar (mover a janela reexibe o ponteiro).
    still: bool,
    /// X (centro) do objeto que ele quer alcançar.
    target: Option<f32>,
    arrived: bool,
    rng: Rng,
}

impl Mascot {
    pub fn new(scale: f32, speed: f32, seed: u64) -> Self {
        Mascot {
            x: 0.0,
            y: 0.0,
            facing_left: false,
            size: SPRITE as f32 * scale,
            scale,
            speed,
            state: State::Fall,
            vx: 0.0,
            vy: 0.0,
            ticks_left: 0,
            anim: 0,
            blink_in: 30,
            blinking: 0,
            fidget_in: 40,
            fidget: 0,
            fidget_look: false,
            awake: 0,
            sleepy_after: SLEEPY_DAY,
            cheer: 0,
            deep: false,
            talking: false,
            tired: false,
            still: false,
            target: None,
            arrived: false,
            rng: Rng::new(seed),
        }
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    pub fn set_tired(&mut self, tired: bool) {
        self.tired = tired;
    }

    /// Fica parado no lugar (animando só o desenho) enquanto `still` for verdadeiro.
    pub fn set_still(&mut self, still: bool) {
        if still && !self.still && matches!(self.state, State::Walk | State::Chase) {
            self.set(State::Idle, 10);
        }
        self.still = still;
    }

    /// Passa a perseguir (ou esquecer) um objeto.
    pub fn set_target(&mut self, x: Option<f32>) {
        self.target = x;
        self.arrived = false;
        match (x, self.state) {
            (Some(_), State::Sleep | State::Yawn) => {
                self.wake();
                self.set(State::Chase, 600);
            }
            (Some(_), State::Idle | State::Walk) if !self.talking && !self.still => self.set(State::Chase, 600),
            (None, State::Chase) => self.set(State::Idle, 10),
            _ => {}
        }
    }

    /// O objeto se mexeu.
    pub fn move_target(&mut self, x: f32) {
        if self.target.is_some() {
            self.target = Some(x);
        }
    }

    /// Chegou no objeto desde a última pergunta?
    pub fn take_arrived(&mut self) -> bool {
        std::mem::take(&mut self.arrived)
    }

    pub fn eat(&mut self) {
        self.set(State::Eat, 24);
    }

    /// Pulinho rápido (depois de chutar a bolinha).
    pub fn hop(&mut self) {
        self.set(State::Happy, 6);
    }

    /// Comemora (você está mandando ver no teclado) — só se estiver de boa.
    pub fn cheer(&mut self) {
        if matches!(self.state, State::Idle | State::Walk) {
            self.set(State::Happy, 12);
        }
    }

    pub fn set_mood(&mut self, sleepy_after: u32, cheer: u32) {
        self.sleepy_after = sleepy_after;
        self.cheer = cheer;
    }

    pub fn grounded(&self) -> bool {
        !matches!(self.state, State::Held | State::Fall)
    }

    pub fn size(&self) -> f32 {
        self.size
    }

    /// Entrada: cai do topo da tela numa posição aleatória.
    pub fn drop_in(&mut self, b: &Bounds) {
        let span = (b.right - b.left - self.size).max(0.0);
        self.x = b.left + span * (0.2 + 0.6 * self.rng.range(0, 1000) as f32 / 1000.0);
        self.y = b.top - self.size;
        self.vx = 0.0;
        self.vy = 0.0;
        self.set(State::Fall, 0);
    }

    /// Intervalo do timer para o estado atual: só anima rápido quando precisa.
    pub fn interval_ms(&self) -> u32 {
        match self.state {
            State::Fall => 16,
            State::Held => 150,
            State::Sleep => 700,
            _ => 100,
        }
    }

    pub fn frame(&self) -> Frame {
        let alt = (self.anim / 2).is_multiple_of(2);
        match self.state {
            State::Idle if self.fidget > 0 && self.fidget_look => Frame::Look,
            State::Idle if self.blinking > 0 => Frame::Blink,
            State::Idle if self.fidget > 0 && alt => Frame::Idle2,
            State::Idle => Frame::Idle,
            State::Walk if alt => Frame::Walk1,
            State::Walk => Frame::Walk2,
            State::Yawn => Frame::Yawn,
            State::Sleep if self.anim.is_multiple_of(2) => Frame::Sleep1,
            State::Sleep => Frame::Sleep2,
            State::Happy if alt => Frame::Happy1,
            State::Happy => Frame::Happy2,
            State::Held if alt => Frame::Held1,
            State::Held => Frame::Held2,
            State::Fall => Frame::Fall,
            State::Land => Frame::Land,
            State::Chase if self.anim.is_multiple_of(2) => Frame::Walk1,
            State::Chase => Frame::Walk2,
            State::Eat if alt => Frame::Eat1,
            State::Eat => Frame::Eat2,
        }
    }

    /// Avança um tick. `dt` (segundos) só importa para a física da queda.
    pub fn update(&mut self, dt: f32, b: &Bounds) {
        self.anim = self.anim.wrapping_add(1);
        match self.state {
            State::Fall => return self.fall(dt, b),
            State::Held => return,
            State::Sleep => {}
            State::Idle => {
                self.awake += 1;
                self.tick_blink();
                self.tick_fidget();
            }
            State::Walk => {
                self.awake += 1;
                self.walk(b);
            }
            State::Chase => {
                self.awake += 1;
                self.chase(b);
            }
            _ => self.awake += 1,
        }
        if self.ticks_left > 0 {
            self.ticks_left -= 1;
        } else {
            self.next();
        }
    }

    // --- interações -------------------------------------------------------

    pub fn grab(&mut self) {
        if matches!(self.state, State::Sleep | State::Yawn) {
            self.awake = 0;
            self.deep = false;
        }
        self.vx = 0.0;
        self.vy = 0.0;
        self.set(State::Held, 0);
    }

    pub fn release(&mut self, vx: f32, vy: f32) {
        let limit = 2500.0 * self.scale / 4.0;
        self.vx = vx.clamp(-limit, limit);
        self.vy = vy.clamp(-limit, limit);
        self.set(State::Fall, 0);
    }

    /// Clique sem arrastar: carinho (ou acorda, se estiver dormindo).
    pub fn pet(&mut self) {
        match self.state {
            State::Sleep | State::Yawn => self.wake(),
            State::Held | State::Fall => {}
            _ => self.set(State::Happy, 16),
        }
    }

    /// Está dormindo (ou bocejando para dormir)?
    pub fn sleeping(&self) -> bool {
        matches!(self.state, State::Sleep | State::Yawn)
    }

    /// Cochilo normal, que acaba sozinho (dormir junto do amigo).
    pub fn nap(&mut self) {
        if matches!(self.state, State::Idle | State::Walk | State::Happy | State::Land) {
            self.set(State::Yawn, 14);
        }
    }

    /// Você saiu do PC: boceja e dorme até ser acordado.
    pub fn doze(&mut self) {
        self.deep = true;
        match self.state {
            State::Sleep => self.ticks_left = u32::MAX,
            State::Yawn | State::Held | State::Fall => {}
            _ => self.set(State::Yawn, 14),
        }
    }

    /// Você voltou: acorda feliz.
    pub fn wake_happy(&mut self) {
        self.deep = false;
        self.awake = 0;
        if self.grounded() {
            self.set(State::Happy, 20);
        }
    }

    /// Balão de fala abriu: para de andar (e acorda, se preciso) para o balão não sair do lugar.
    pub fn talk(&mut self) {
        self.talking = true;
        match self.state {
            State::Walk | State::Chase => self.set(State::Idle, 20),
            State::Sleep | State::Yawn => self.wake(),
            _ => {}
        }
    }

    pub fn quiet(&mut self) {
        self.talking = false;
    }

    /// Nova escala (movimento) e tamanho do corpo em pixels (o desenho pode ser 16 ou 32).
    pub fn resize(&mut self, scale: f32, body: f32, b: &Bounds) {
        let bottom = self.y + self.size;
        self.scale = scale;
        self.size = body;
        self.y = bottom - self.size;
        self.settle(b);
    }

    /// Reajusta a posição quando a área útil muda (barra de tarefas, resolução).
    pub fn settle(&mut self, b: &Bounds) {
        if matches!(self.state, State::Held | State::Fall) {
            return;
        }
        self.x = self.x.clamp(b.left, (b.right - self.size).max(b.left));
        let floor = b.floor - self.size;
        if self.y < floor - 1.0 {
            self.set(State::Fall, 0);
        } else {
            self.y = floor;
        }
    }

    // --- internos ---------------------------------------------------------

    fn set(&mut self, state: State, ticks: u32) {
        self.state = state;
        self.ticks_left = ticks;
        self.anim = 0;
        self.fidget = 0;
    }

    fn wake(&mut self) {
        self.deep = false;
        self.awake = 0;
        self.blinking = 3;
        self.set(State::Idle, 25);
    }

    fn next(&mut self) {
        match self.state {
            State::Yawn => {
                let t = if self.deep { u32::MAX } else { self.rng.range(90, 260) };
                self.set(State::Sleep, t);
            }
            State::Sleep => self.wake(),
            State::Eat => self.set(State::Happy, 16),
            _ if self.talking || self.still => self.set(State::Idle, 10),
            _ if self.target.is_some() => self.set(State::Chase, 600),
            _ => {
                let r = self.rng.range(0, 100);
                if self.awake > self.sleepy_after && r < 10 {
                    self.set(State::Yawn, 14);
                } else if r >= 100 - self.cheer {
                    self.set(State::Happy, 16);
                } else if r < 55 {
                    self.facing_left = self.rng.range(0, 2) == 0;
                    let t = self.rng.range(20, 90);
                    self.set(State::Walk, t);
                } else {
                    let t = self.rng.range(15, 60);
                    self.set(State::Idle, t);
                }
            }
        }
    }

    fn tick_fidget(&mut self) {
        if self.fidget > 0 {
            self.fidget -= 1;
        } else if self.fidget_in == 0 {
            self.fidget_look = self.rng.range(0, 3) == 0;
            // Olhar para um lado ou para o outro.
            if self.fidget_look {
                self.facing_left = !self.facing_left;
            }
            self.fidget = self.rng.range(10, 24);
            self.fidget_in = self.rng.range(20, 60);
        } else {
            self.fidget_in -= 1;
        }
    }

    fn tick_blink(&mut self) {
        if self.blinking > 0 {
            self.blinking -= 1;
        } else if self.blink_in == 0 {
            self.blinking = 2;
            self.blink_in = self.rng.range(25, 70);
        } else {
            self.blink_in -= 1;
        }
    }

    fn walk(&mut self, b: &Bounds) {
        // Anda de "pixel de sprite" em "pixel de sprite" — fica com cara de pixel art.
        // Cansado (bateria baixa), só dá um passo a cada dois ticks.
        if self.tired && self.anim % 2 == 1 {
            return;
        }
        let step = self.speed * self.scale;
        self.x += if self.facing_left { -step } else { step };
        if self.x <= b.left {
            self.x = b.left;
            self.facing_left = false;
        } else if self.x + self.size >= b.right {
            self.x = b.right - self.size;
            self.facing_left = true;
        }
        self.y = b.floor - self.size;
    }

    fn chase(&mut self, b: &Bounds) {
        let Some(target) = self.target else {
            self.set(State::Idle, 10);
            return;
        };
        let dx = target - (self.x + self.size / 2.0);
        if dx.abs() <= self.size * 0.35 {
            self.arrived = true;
            self.set(State::Idle, 3);
            return;
        }
        self.facing_left = dx < 0.0;
        let run = if self.tired { 1.5 } else { 3.0 } * self.scale;
        self.x += run.min(dx.abs()) * dx.signum();
        self.x = self.x.clamp(b.left, (b.right - self.size).max(b.left));
        self.y = b.floor - self.size;
    }

    fn fall(&mut self, dt: f32, b: &Bounds) {
        let k = self.scale / 4.0;
        self.vy += 2600.0 * k * dt;
        self.vx -= self.vx * (1.2 * dt).min(1.0);
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        if self.x < b.left {
            self.x = b.left;
            self.vx = self.vx.abs() * 0.5;
        } else if self.x + self.size > b.right {
            self.x = b.right - self.size;
            self.vx = -self.vx.abs() * 0.5;
        }
        if self.y < b.top && self.vy < 0.0 {
            self.y = b.top;
            self.vy = -self.vy * 0.3;
        }
        if self.vx.abs() > 30.0 {
            self.facing_left = self.vx < 0.0;
        }

        let floor = b.floor - self.size;
        if self.y >= floor {
            self.y = floor;
            let hard = self.vy > 900.0 * k;
            self.vx = 0.0;
            self.vy = 0.0;
            if hard {
                self.set(State::Land, 4);
            } else {
                let t = self.rng.range(10, 30);
                self.set(State::Idle, t);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Bounds = Bounds { left: 0.0, top: 0.0, right: 1920.0, floor: 1040.0 };

    fn run(m: &mut Mascot, ticks: u32) {
        for _ in 0..ticks {
            m.update(0.016, &B);
        }
    }

    #[test]
    fn falls_and_lands_on_floor() {
        let mut m = Mascot::new(4.0, 1.0, 42);
        m.drop_in(&B);
        run(&mut m, 500);
        assert!(!matches!(m.state, State::Fall | State::Held));
        assert_eq!(m.y, B.floor - m.size());
    }

    #[test]
    fn never_leaves_the_screen() {
        for seed in 1..20 {
            let mut m = Mascot::new(4.0, 4.0, seed);
            m.drop_in(&B);
            run(&mut m, 20_000);
            assert!(m.x >= B.left && m.x + m.size() <= B.right, "seed {seed}: x={}", m.x);
        }
    }

    #[test]
    fn thrown_mascot_bounces_off_walls() {
        let mut m = Mascot::new(4.0, 1.0, 7);
        m.x = 1800.0;
        m.y = 500.0;
        m.grab();
        m.release(5000.0, -2000.0);
        run(&mut m, 500);
        assert!(m.x + m.size() <= B.right);
        assert_eq!(m.y, B.floor - m.size());
    }

    #[test]
    fn dozes_until_woken() {
        let mut m = Mascot::new(4.0, 1.0, 5);
        m.drop_in(&B);
        run(&mut m, 500);
        m.doze();
        run(&mut m, 50_000);
        assert_eq!(m.state, State::Sleep);
        m.wake_happy();
        assert_eq!(m.state, State::Happy);
    }

    #[test]
    fn stands_still_while_talking() {
        let mut m = Mascot::new(4.0, 1.0, 11);
        m.drop_in(&B);
        run(&mut m, 500);
        m.set(State::Walk, 50);
        m.talk();
        let x = m.x;
        run(&mut m, 2_000);
        assert_eq!(m.x, x);
        assert_eq!(m.state, State::Idle);
    }

    #[test]
    fn stays_put_while_still() {
        let mut m = Mascot::new(4.0, 1.0, 21);
        m.drop_in(&B);
        run(&mut m, 500);
        m.set(State::Walk, 50);
        m.set_still(true);
        m.set_target(Some(10.0));
        let x = m.x;
        run(&mut m, 3_000);
        assert_eq!(m.x, x);
        m.set_still(false);
        run(&mut m, 3_000);
        assert_ne!(m.x, x, "deveria voltar a andar/perseguir");
    }

    #[test]
    fn chases_target_and_arrives() {
        let mut m = Mascot::new(4.0, 1.0, 13);
        m.drop_in(&B);
        run(&mut m, 500);
        let target = if m.x > 900.0 { 100.0 } else { 1800.0 };
        m.set_target(Some(target));
        let arrived = (0..2_000).any(|_| {
            m.update(0.1, &B);
            m.take_arrived()
        });
        assert!(arrived);
        assert!((m.x + m.size() / 2.0 - target).abs() <= m.size() * 0.35);
        assert!(m.x >= B.left && m.x + m.size() <= B.right);
    }

    #[test]
    fn eats_then_gets_happy() {
        let mut m = Mascot::new(4.0, 1.0, 17);
        m.drop_in(&B);
        run(&mut m, 500);
        m.eat();
        assert_eq!(m.frame(), Frame::Eat1);
        (0..25).for_each(|_| m.update(0.1, &B));
        assert_eq!(m.state, State::Happy);
    }

    #[test]
    fn pet_wakes_sleeping_mascot() {
        let mut m = Mascot::new(4.0, 1.0, 3);
        m.set(State::Sleep, 100);
        m.pet();
        assert_eq!(m.state, State::Idle);
    }
}
