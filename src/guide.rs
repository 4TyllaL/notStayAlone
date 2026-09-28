//! Pausa guiada: depois do lembrete de olhos ou de alongar, um clique no balão e
//! o mascote conduz a pausa, passo a passo, com contagem regressiva. Lógica pura —
//! o app chama `tick` uma vez por segundo e mostra `text` no balão.

use crate::lang::{fill, tr};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Eyes,
    Stretch,
}

/// (texto com `{}` para os segundos que faltam, duração em segundos)
const EYES: [(&str, u32); 2] = [
    ("Olhe para algo bem longe, pela janela se der... {}", 20),
    ("Agora pisque devagar, várias vezes... {}", 5),
];

const STRETCH: [(&str, u32); 4] = [
    ("Gire os ombros para trás, devagar... {}", 10),
    ("Incline a cabeça para a direita... {}", 8),
    ("Agora para a esquerda... {}", 8),
    ("Estique os braços para cima, bem alto! {}", 10),
];

pub struct Guide {
    pub kind: Kind,
    step: usize,
    left: u32,
}

impl Guide {
    pub fn new(kind: Kind) -> Guide {
        let left = Self::steps(kind)[0].1;
        Guide { kind, step: 0, left }
    }

    fn steps(kind: Kind) -> &'static [(&'static str, u32)] {
        match kind {
            Kind::Eyes => &EYES,
            Kind::Stretch => &STRETCH,
        }
    }

    /// O que o balão mostra agora.
    pub fn text(&self) -> String {
        fill(tr(Self::steps(self.kind)[self.step].0), &[&self.left])
    }

    /// Passou um segundo. `false` quando a pausa terminou.
    pub fn tick(&mut self) -> bool {
        if self.left > 1 {
            self.left -= 1;
            return true;
        }
        let steps = Self::steps(self.kind);
        if self.step + 1 < steps.len() {
            self.step += 1;
            self.left = steps[self.step].1;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_each_step_then_ends() {
        let mut g = Guide::new(Kind::Eyes);
        assert!(g.text().ends_with("... 20"), "{}", g.text());
        let mut seconds = 0;
        while g.tick() {
            seconds += 1;
        }
        assert_eq!(seconds, 20 + 5 - 1);
        assert!(g.text().starts_with("Agora pisque"), "{}", g.text());
    }

    #[test]
    fn stretching_has_every_step() {
        let mut g = Guide::new(Kind::Stretch);
        let mut texts = vec![g.text()];
        while g.tick() {
            texts.push(g.text());
        }
        assert_eq!(texts.len() as u32, STRETCH.iter().map(|s| s.1).sum::<u32>());
        assert!(texts.iter().any(|t| t.starts_with("Estique os braços")));
    }
}
