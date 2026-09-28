//! Falas do mascote, carregadas de um arquivo de texto agrupado por [tópico].

use crate::rng::Rng;

pub const EMBEDDED: &str = include_str!("../assets/phrases.txt");
pub const EMBEDDED_EN: &str = include_str!("../assets/phrases_en.txt");

/// Falas padrão embutidas, no idioma atual.
pub fn embedded() -> &'static str {
    if crate::lang::is_english() {
        EMBEDDED_EN
    } else {
        EMBEDDED
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Topic {
    Morning,
    Afternoon,
    Evening,
    LateNight,
    Welcome,
    MissedYou,
    Water,
    Stretch,
    Eyes,
    Midnight,
    Pet,
    PetLove,
    Thanks,
    PomodoroStart,
    PomodoroBreak,
    PomodoroBack,
    Summary,
    Silence,
    Hello,
    Yummy,
    Full,
    BallDone,
    BatteryLow,
    Charging,
    Typing,
    Birthday,
    Monday,
    Friday,
    Weekend,
    /// Três horas usando o PC sem pausa.
    NoBreak,
    /// Lembrete personalizado: o texto vem da configuração, não do arquivo de falas.
    Reminder,
    /// Resposta da conversa com IA (texto vem do plugin).
    Chat,
    /// Aviso de um plugin (texto vem do plugin).
    Plugin,
    /// Aviso do próprio app (versão nova etc.), com texto pronto.
    App,
}

/// Nomes no arquivo, na mesma ordem do enum `Topic`.
const KEYS: [&str; 30] = [
    "bom_dia", "boa_tarde", "boa_noite", "madrugada", "voltou", "saudade", "agua", "alongar",
    "olhos", "meia_noite", "carinho", "carinho_muito", "obrigado", "pomodoro_inicio",
    "pomodoro_pausa", "pomodoro_volta", "resumo", "silencio", "ola", "petisco", "cheio",
    "bolinha_fim", "bateria_baixa", "carregando", "digitando", "aniversario", "segunda", "sexta", "fim_de_semana",
    "sem_pausa",
];

#[derive(Clone)]
pub struct Phrases {
    lines: Vec<Vec<String>>,
    last: Vec<usize>,
}

impl Phrases {
    /// Falas completas: todo tópico precisa ter pelo menos uma fala.
    pub fn parse(src: &str) -> Result<Phrases, String> {
        let phrases = Self::parse_partial(src)?;
        if let Some(i) = phrases.lines.iter().position(|l| l.is_empty()) {
            return Err(format!("o tópico [{}] está vazio", KEYS[i]));
        }
        Ok(phrases)
    }

    /// Substitui os tópicos que `other` define (falas próprias de um mascote).
    pub fn overlay(&mut self, other: &Phrases) {
        for (mine, theirs) in self.lines.iter_mut().zip(&other.lines) {
            if !theirs.is_empty() {
                *mine = theirs.clone();
            }
        }
    }

    /// Só os tópicos presentes no arquivo.
    pub fn parse_partial(src: &str) -> Result<Phrases, String> {
        let mut lines = vec![Vec::new(); KEYS.len()];
        let mut current = None;
        for (i, line) in src.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(key) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let slot = KEYS.iter().position(|k| *k == key);
                current = Some(slot.ok_or(format!("linha {}: tópico desconhecido '[{key}]'", i + 1))?);
                continue;
            }
            let Some(slot) = current else {
                return Err(format!("linha {}: fala fora de um [tópico]", i + 1));
            };
            lines[slot].push(line.to_string());
        }
        Ok(Phrases { last: vec![usize::MAX; KEYS.len()], lines })
    }

    /// Sorteia uma fala do tópico, evitando repetir a última.
    pub fn pick(&mut self, topic: Topic, rng: &mut Rng) -> &str {
        let Some(options) = self.lines.get(topic as usize) else { return "" };
        let mut i = rng.range(0, options.len() as u32) as usize;
        if options.len() > 1 && i == self.last[topic as usize] {
            i = (i + 1) % options.len();
        }
        self.last[topic as usize] = i;
        &options[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_phrases_are_valid() {
        Phrases::parse(EMBEDDED).unwrap();
    }

    #[test]
    fn never_repeats_twice_in_a_row() {
        let mut p = Phrases::parse(EMBEDDED).unwrap();
        let mut rng = Rng::new(9);
        let mut last = String::new();
        for _ in 0..50 {
            let s = p.pick(Topic::Pet, &mut rng).to_string();
            assert_ne!(s, last);
            last = s;
        }
    }

    #[test]
    fn rejects_unknown_topic() {
        assert!(Phrases::parse("[nada]\noi").is_err());
    }

    #[test]
    fn mascot_phrases_overlay_only_their_topics() {
        let mut p = Phrases::parse(EMBEDDED).unwrap();
        let before = p.lines[Topic::Water as usize].clone();
        for (id, _, own, own_en) in crate::pack::EMBEDDED {
            let own = Phrases::parse_partial(own).unwrap_or_else(|e| panic!("{id}: {e}"));
            Phrases::parse_partial(own_en).unwrap_or_else(|e| panic!("{id} (en): {e}"));
            p.overlay(&own);
        }
        assert_eq!(p.lines[Topic::Water as usize], before);
        // O último da lista (Jujubs) é quem vale no fim.
        assert!(p.lines[Topic::Pet as usize].iter().any(|l| l.contains("Rawr")));
    }

    #[test]
    fn english_phrases_cover_every_topic() {
        Phrases::parse(EMBEDDED_EN).unwrap();
    }

    #[test]
    fn feminine_overlay_fixes_gendered_lines() {
        let mut p = Phrases::parse(EMBEDDED).unwrap();
        p.overlay(&Phrases::parse_partial(crate::pack::FEMININE).unwrap());
        for topic in [Topic::Thanks, Topic::Full, Topic::Silence, Topic::Hello, Topic::Yummy, Topic::Charging] {
            let lines = &p.lines[topic as usize];
            assert!(
                !lines.iter().any(|l| l.contains("Obrigado") || l.contains("cheio") || l.contains("orgulhoso") || l.contains("quietinho") || l.contains(" o {nome}") || l.starts_with("O {nome}")),
                "{topic:?} ainda no masculino: {lines:?}"
            );
        }
    }
}
