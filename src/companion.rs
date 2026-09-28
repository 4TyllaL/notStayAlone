//! A parte "companhia": percebe quando você sai e volta, lembra de água e
//! pausas, guarda o afeto e o resumo do dia. Lógica pura — o app só chama
//! `update` a cada poucos segundos e executa os eventos devolvidos.

use crate::{
    lang::{self, tr},
    phrases::Topic,
};

/// Lembrete periódico, contado só em tempo ativo no computador.
#[derive(Clone, PartialEq, Debug)]
pub struct Reminder {
    pub on: bool,
    pub minutes: u32,
    pub kind: ReminderKind,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ReminderKind {
    Water,
    Stretch,
    Eyes,
    /// Criado por você: o balão mostra exatamente este texto.
    Custom(String),
}

impl ReminderKind {
    pub const BUILT_IN: [ReminderKind; 3] = [ReminderKind::Water, ReminderKind::Stretch, ReminderKind::Eyes];

    /// Tópico de falas dos lembretes embutidos (os personalizados têm texto fixo).
    pub fn topic(&self) -> Option<Topic> {
        match self {
            ReminderKind::Water => Some(Topic::Water),
            ReminderKind::Stretch => Some(Topic::Stretch),
            ReminderKind::Eyes => Some(Topic::Eyes),
            ReminderKind::Custom(_) => None,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            ReminderKind::Water => tr("Beber água"),
            ReminderKind::Stretch => tr("Alongar"),
            ReminderKind::Eyes => tr("Descansar os olhos"),
            ReminderKind::Custom(text) => text,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    /// Os três embutidos primeiro (água, alongar, olhos), depois os seus.
    pub reminders: Vec<Reminder>,
    /// Minutos sem mexer no PC para contar como "saiu".
    pub away_minutes: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        let defaults = [(true, 45), (true, 60), (false, 20)];
        let reminders = ReminderKind::BUILT_IN
            .into_iter()
            .zip(defaults)
            .map(|(kind, (on, minutes))| Reminder { on, minutes, kind })
            .collect();
        Settings { reminders, away_minutes: 5 }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    Say(Topic),
    /// Hora do lembrete de índice `n` em `settings.reminders`.
    Remind(usize),
    /// Você saiu: ele dorme até você voltar.
    Doze,
    /// Você voltou.
    WakeUp,
}

/// O que o app observou neste momento.
pub struct Now {
    /// Relógio monotônico (segundos desde que o app abriu).
    pub secs: u64,
    /// Data local como AAAAMMDD.
    pub day: u32,
    pub hour: u32,
    /// Segundos desde o último uso de teclado/mouse.
    pub idle_secs: u64,
    /// Mascote escondido pelo usuário: lembretes são descartados.
    pub hidden: bool,
    /// App em tela cheia (ou reunião): lembretes esperam.
    pub busy: bool,
    /// Dia da semana (0 = domingo).
    pub weekday: u32,
    /// Hoje é o seu aniversário.
    pub birthday: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Stats {
    pub day: u32,
    pub together_secs: u64,
    pub breaks: u32,
    pub water: u32,
    pub pets: u32,
    pub summary_shown: bool,
    /// Falas da rotina já ditas hoje (bits `ROUTINE_*`).
    pub routine_said: u32,
}

const ROUTINE_BIRTHDAY: u32 = 1;
const ROUTINE_MONDAY: u32 = 2;
const ROUTINE_FRIDAY: u32 = 4;
const ROUTINE_WEEKEND: u32 = 8;
/// Tempo de uso contínuo, sem pausa, que merece um puxão de orelha.
const NO_BREAK_SECS: u64 = 3 * 3600;

impl Stats {
    fn new(day: u32) -> Stats {
        Stats { day, together_secs: 0, breaks: 0, water: 0, pets: 0, summary_shown: false, routine_said: 0 }
    }
}

pub const POMODORO_WORK: u64 = 25 * 60;
pub const POMODORO_BREAK: u64 = 5 * 60;
/// Intervalo mínimo entre dois lembretes, para nunca virar metralhadora.
const REMINDER_GAP: u64 = 120;
/// O afeto cai devagar (1 ponto por hora de uso), mas nunca abaixo disso.
const AFFECTION_FLOOR: f32 = 20.0;
const SILENCE_SECS: u64 = 3600;
/// Um petisco a cada 20 min, no máximo.
const FEED_COOLDOWN: u64 = 20 * 60;
/// Comenta a digitação intensa no máximo a cada 45 min.
const TYPING_COOLDOWN: u64 = 45 * 60;
const BATTERY_LOW: u8 = 20;

pub struct Companion {
    pub settings: Settings,
    pub stats: Stats,
    pub affection: f32,
    away: bool,
    away_since: u64,
    /// Segundos ativos desde o último aviso de cada lembrete.
    counters: Vec<u64>,
    last_reminder: Option<u64>,
    silenced_until: u64,
    /// (em pausa?, termina em)
    pomodoro: Option<(bool, u64)>,
    last_midnight: Option<u64>,
    last_update: Option<u64>,
    last_pet: Option<u64>,
    last_fed: Option<u64>,
    last_typing: Option<u64>,
    /// Tempo de uso desde a última pausa de verdade.
    since_break: u64,
    /// Avisou que a bateria está baixa (e ainda não foi carregada).
    pub low_battery: bool,
}

impl Companion {
    pub fn new(settings: Settings, day: u32) -> Companion {
        Companion {
            counters: vec![0; settings.reminders.len()],
            settings,
            stats: Stats::new(day),
            affection: 50.0,
            away: false,
            away_since: 0,
            last_reminder: None,
            silenced_until: 0,
            pomodoro: None,
            last_midnight: None,
            last_update: None,
            last_pet: None,
            last_fed: None,
            last_typing: None,
            since_break: 0,
            low_battery: false,
        }
    }

    pub fn away(&self) -> bool {
        self.away
    }

    /// Troca as configurações mantendo o progresso dos lembretes que continuam iguais.
    pub fn set_settings(&mut self, settings: Settings) {
        let counters = settings
            .reminders
            .iter()
            .map(|r| {
                let old = self.settings.reminders.iter().position(|o| o.kind == r.kind);
                old.map_or(0, |i| self.counters[i])
            })
            .collect();
        self.counters = counters;
        self.settings = settings;
    }

    /// Pode comer agora? (senão ele diz que está cheio)
    pub fn hungry(&self, secs: u64) -> bool {
        self.last_fed.is_none_or(|t| secs >= t + FEED_COOLDOWN)
    }

    pub fn on_fed(&mut self, secs: u64) {
        self.last_fed = Some(secs);
        self.bump(3.0);
    }

    pub fn on_played(&mut self) {
        self.bump(4.0);
    }

    /// Estado da energia. Avisa uma vez ao ficar baixa e agradece ao carregar.
    pub fn on_power(&mut self, on_battery: bool, percent: u8) -> Option<Topic> {
        if on_battery && percent <= BATTERY_LOW && !self.low_battery {
            self.low_battery = true;
            return Some(Topic::BatteryLow);
        }
        if self.low_battery && !on_battery {
            self.low_battery = false;
            return Some(Topic::Charging);
        }
        None
    }

    /// Você começou a digitar sem parar. Às vezes ele comenta.
    pub fn on_typing(&mut self, secs: u64) -> Option<Topic> {
        if self.silenced(secs) || self.last_typing.is_some_and(|t| secs < t + TYPING_COOLDOWN) {
            return None;
        }
        self.last_typing = Some(secs);
        Some(Topic::Typing)
    }

    pub fn greeting(hour: u32) -> Topic {
        match hour {
            5..=11 => Topic::Morning,
            12..=17 => Topic::Afternoon,
            18..=23 => Topic::Evening,
            _ => Topic::LateNight,
        }
    }

    pub fn update(&mut self, now: &Now) -> Vec<Event> {
        let mut events = Vec::new();
        if now.day != self.stats.day {
            self.stats = Stats::new(now.day);
        }
        let away_secs = self.settings.away_minutes.max(1) as u64 * 60;
        let dt = self.last_update.map_or(0, |t| now.secs.saturating_sub(t));
        if dt > away_secs && !self.away {
            // O PC ficou suspenso/hibernando: conta como se você tivesse saído.
            self.away = true;
            self.away_since = self.last_update.unwrap_or(now.secs);
        }
        self.last_update = Some(now.secs);
        let dt = dt.min(10);

        if self.away {
            if now.idle_secs < 5 {
                self.away = false;
                events.push(Event::WakeUp);
                let gone = now.secs.saturating_sub(self.away_since);
                if gone >= away_secs {
                    self.stats.breaks += 1;
                    self.since_break = 0;
                    // Levantou: já alongou e descansou os olhos.
                    for (r, c) in self.settings.reminders.iter().zip(&mut self.counters) {
                        if matches!(r.kind, ReminderKind::Stretch | ReminderKind::Eyes) {
                            *c = 0;
                        }
                    }
                    self.bump(3.0);
                    let topic = if gone >= 2 * 3600 { Topic::MissedYou } else { Topic::Welcome };
                    events.push(Event::Say(topic));
                }
            }
            return events;
        }
        if now.idle_secs >= away_secs {
            self.away = true;
            self.away_since = now.secs.saturating_sub(now.idle_secs);
            events.push(Event::Doze);
            return events;
        }

        // Só conta como "juntos" enquanto você está usando o PC de fato.
        if now.idle_secs < 60 {
            self.stats.together_secs += dt;
            self.since_break += dt;
            for c in &mut self.counters {
                *c += dt;
            }
            self.affection = (self.affection - dt as f32 / 3600.0).max(self.affection.min(AFFECTION_FLOOR));
        }

        // Pomodoro fala mesmo silenciado — foi você quem pediu.
        if let Some((on_break, ends)) = self.pomodoro {
            if now.secs >= ends {
                let (topic, next) = if on_break {
                    (Topic::PomodoroBack, POMODORO_WORK)
                } else {
                    (Topic::PomodoroBreak, POMODORO_BREAK)
                };
                self.pomodoro = Some((!on_break, now.secs + next));
                events.push(Event::Say(topic));
                return events;
            }
        }

        let quiet = self.silenced(now.secs) || now.hidden;
        for (i, r) in self.settings.reminders.iter().enumerate() {
            if !r.on || r.minutes == 0 {
                self.counters[i] = 0;
                continue;
            }
            if self.counters[i] < r.minutes as u64 * 60 {
                continue;
            }
            if quiet {
                self.counters[i] = 0; // descarta, sem acumular para depois
                continue;
            }
            if now.busy || self.last_reminder.is_some_and(|t| now.secs < t + REMINDER_GAP) {
                continue; // espera o momento certo
            }
            self.counters[i] = 0;
            self.last_reminder = Some(now.secs);
            events.push(Event::Remind(i));
            return events;
        }
        if quiet || now.busy {
            return events;
        }
        // Rotina: no máximo uma fala por tema por dia, sem atropelar os lembretes.
        if self.last_reminder.is_none_or(|t| now.secs >= t + REMINDER_GAP) {
            if let Some(topic) = self.routine(now) {
                self.last_reminder = Some(now.secs);
                events.push(Event::Say(topic));
                return events;
            }
        }

        if now.hour < 5 {
            match self.last_midnight {
                None => self.last_midnight = Some(now.secs),
                Some(t) if now.secs >= t + 3600 => {
                    self.last_midnight = Some(now.secs);
                    events.push(Event::Say(Topic::Midnight));
                    return events;
                }
                _ => {}
            }
        }

        if now.hour >= 18 && !self.stats.summary_shown && self.stats.together_secs >= 3600 {
            self.stats.summary_shown = true;
            events.push(Event::Say(Topic::Summary));
        }
        events
    }

    /// A fala da rotina que cabe agora (e a marca como dita).
    fn routine(&mut self, now: &Now) -> Option<Topic> {
        if self.since_break >= NO_BREAK_SECS {
            self.since_break = 0;
            return Some(Topic::NoBreak);
        }
        let candidates = [
            (ROUTINE_BIRTHDAY, now.birthday && now.hour >= 7, Topic::Birthday),
            (ROUTINE_MONDAY, now.weekday == 1 && (7..12).contains(&now.hour), Topic::Monday),
            (ROUTINE_FRIDAY, now.weekday == 5 && now.hour >= 15, Topic::Friday),
            (ROUTINE_WEEKEND, matches!(now.weekday, 0 | 6) && now.hour >= 10 && self.stats.together_secs >= 1800, Topic::Weekend),
        ];
        let (bit, _, topic) = candidates.into_iter().find(|&(bit, due, _)| due && self.stats.routine_said & bit == 0)?;
        self.stats.routine_said |= bit;
        if topic == Topic::Birthday {
            self.bump(10.0);
        }
        Some(topic)
    }

    /// Carinho. `roll` (0..100) decide se ele responde falando.
    pub fn on_pet(&mut self, secs: u64, roll: u32) -> Option<Topic> {
        let spam = self.last_pet.is_some_and(|t| secs < t + 3);
        self.last_pet = Some(secs);
        self.bump(if spam { 0.2 } else { 1.5 });
        self.stats.pets += 1;
        if self.silenced(secs) || roll >= 30 {
            return None;
        }
        Some(if self.affection >= 70.0 { Topic::PetLove } else { Topic::Pet })
    }

    /// Clique no balão: confirma o lembrete.
    pub fn on_ack(&mut self, topic: Topic) -> Option<Topic> {
        match topic {
            Topic::Water => Some(self.drank_water()),
            Topic::Stretch | Topic::Eyes | Topic::Reminder => {
                self.bump(2.0);
                Some(Topic::Thanks)
            }
            _ => None,
        }
    }

    /// Bebeu um copo d'água (pelo balão, pelo painel ou por `--agua`).
    pub fn drank_water(&mut self) -> Topic {
        self.stats.water += 1;
        self.bump(2.0);
        Topic::Thanks
    }

    pub fn pomodoro_active(&self) -> bool {
        self.pomodoro.is_some()
    }

    pub fn start_pomodoro(&mut self, secs: u64) {
        self.pomodoro = Some((false, secs + POMODORO_WORK));
    }

    pub fn stop_pomodoro(&mut self) {
        self.pomodoro = None;
    }

    pub fn silenced(&self, secs: u64) -> bool {
        secs < self.silenced_until
    }

    pub fn silence(&mut self, secs: u64) {
        self.silenced_until = secs + SILENCE_SECS;
    }

    pub fn unsilence(&mut self) {
        self.silenced_until = 0;
    }

    /// Afeto em corações (1 a 5).
    pub fn hearts(&self) -> u32 {
        ((self.affection / 20.0).ceil() as u32).clamp(1, 5)
    }

    fn bump(&mut self, amount: f32) {
        self.affection = (self.affection + amount).min(100.0);
    }

    // --- persistência (state.ini) ------------------------------------------

    pub fn save_string(&self) -> String {
        let s = &self.stats;
        format!(
            "day={}\ntogether={}\nbreaks={}\nwater={}\npets={}\nsummary={}\nroutine={}\naffection={:.1}\n",
            s.day, s.together_secs, s.breaks, s.water, s.pets, s.summary_shown as u8, s.routine_said, self.affection
        )
    }

    /// Restaura o afeto e, se for o mesmo dia, as estatísticas.
    pub fn load_string(&mut self, text: &str) {
        let mut saved = Stats::new(0);
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let value = value.trim();
            let n = value.parse::<u64>().unwrap_or(0);
            match key.trim() {
                "day" => saved.day = n as u32,
                "together" => saved.together_secs = n,
                "breaks" => saved.breaks = n as u32,
                "water" => saved.water = n as u32,
                "pets" => saved.pets = n as u32,
                "summary" => saved.summary_shown = n != 0,
                "routine" => saved.routine_said = n as u32,
                "affection" => {
                    if let Ok(a) = value.parse::<f32>() {
                        self.affection = a.clamp(0.0, 100.0);
                    }
                }
                _ => {}
            }
        }
        if saved.day == self.stats.day {
            self.stats = saved;
        }
    }
}

/// Detecta digitação intensa sem tocar no teclado: em cada amostra (a cada
/// 500 ms) houve alguma atividade *e* o cursor ficou parado? Se isso vale para
/// quase todas as amostras dos últimos 20 s, você está digitando sem parar.
#[derive(Default)]
pub struct TypingSensor {
    history: u64,
    last_input: u32,
    last_cursor: (i32, i32),
    active: bool,
}

const TYPING_WINDOW: u32 = 40;
const TYPING_THRESHOLD: u32 = 34;

impl TypingSensor {
    /// `last_input` = momento do último uso (GetLastInputInfo). Retorna `true` quando a sequência começa.
    pub fn sample(&mut self, last_input: u32, cursor: (i32, i32)) -> bool {
        let typed = last_input != self.last_input && cursor == self.last_cursor;
        self.last_input = last_input;
        self.last_cursor = cursor;
        self.history = ((self.history << 1) | typed as u64) & ((1 << TYPING_WINDOW) - 1);
        let count = self.history.count_ones();
        let started = count >= TYPING_THRESHOLD && !self.active;
        if count >= TYPING_THRESHOLD {
            self.active = true;
        } else if count < TYPING_WINDOW / 2 {
            self.active = false; // histerese: precisa parar de verdade para contar de novo
        }
        started
    }
}

/// Preenche {nome}, {juntos}, {pausas} e {agua}.
pub fn fill(text: &str, s: &Stats, name: &str) -> String {
    let (h, m) = (s.together_secs / 3600, s.together_secs % 3600 / 60);
    let together = if h > 0 { format!("{h}h{m:02}min") } else { format!("{m}min") };
    let breaks = match s.breaks {
        0 => tr("nenhuma pausa").to_string(),
        1 => tr("1 pausa").to_string(),
        n => lang::fill(tr("{} pausas"), &[&n]),
    };
    let water = match s.water {
        0 => tr("nenhuma vez").to_string(),
        1 => tr("1 vez").to_string(),
        n => lang::fill(tr("{} vezes"), &[&n]),
    };
    text.replace("{nome}", name)
        .replace("{juntos}", &together)
        .replace("{pausas}", &breaks)
        .replace("{agua}", &water)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u32 = 20260927;

    /// Água 45 min, alongar 60 min, olhos desligado.
    fn settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn custom_reminder_fires_on_its_own_interval() {
        let mut s = settings();
        s.reminders.iter_mut().for_each(|r| r.on = false);
        s.reminders.push(Reminder { on: true, minutes: 10, kind: ReminderKind::Custom("E-mail".into()) });
        let mut c = Companion::new(s, DAY);
        assert_eq!(active(&mut c, 0, 10 * 60 + 10), vec![Event::Remind(3)]);
    }

    #[test]
    fn changing_settings_keeps_progress_of_unchanged_reminders() {
        let mut c = Companion::new(settings(), DAY);
        active(&mut c, 0, 40 * 60);
        let mut s = settings();
        s.reminders.insert(0, Reminder { on: true, minutes: 500, kind: ReminderKind::Custom("x".into()) });
        c.set_settings(s);
        // A água já tinha 40 min: faltam só uns 5.
        let events = active(&mut c, 40 * 60, 46 * 60);
        assert_eq!(events, vec![Event::Remind(1)]);
    }

    fn now(secs: u64, idle: u64) -> Now {
        // Uma quarta-feira qualquer, 14h.
        Now { secs, day: DAY, hour: 14, idle_secs: idle, hidden: false, busy: false, weekday: 3, birthday: false }
    }

    /// Simula uso ativo de `from` até `to`, de 2 em 2 s, juntando os eventos.
    fn active(c: &mut Companion, from: u64, to: u64) -> Vec<Event> {
        (from..to).step_by(2).flat_map(|t| c.update(&now(t, 0))).collect()
    }

    #[test]
    fn leaving_and_returning_counts_a_break() {
        let mut c = Companion::new(settings(), DAY);
        active(&mut c, 0, 100);
        let idle: Vec<_> = (100..700).step_by(2).flat_map(|t| c.update(&now(t, t - 100))).collect();
        assert_eq!(idle, vec![Event::Doze]);
        let back = c.update(&now(700, 0));
        assert_eq!(back, vec![Event::WakeUp, Event::Say(Topic::Welcome)]);
        assert_eq!(c.stats.breaks, 1);
    }

    #[test]
    fn long_absence_says_it_missed_you() {
        let mut c = Companion::new(settings(), DAY);
        c.update(&now(0, 0));
        c.update(&now(10, 300));
        assert!(c.update(&now(3 * 3600, 0)).contains(&Event::Say(Topic::MissedYou)));
    }

    #[test]
    fn suspended_pc_counts_as_away() {
        let mut c = Companion::new(settings(), DAY);
        c.update(&now(0, 0));
        let events = c.update(&now(1800, 0));
        assert!(events.contains(&Event::WakeUp));
        assert_eq!(c.stats.breaks, 1);
    }

    #[test]
    fn water_reminder_after_interval_of_active_use() {
        let mut c = Companion::new(settings(), DAY);
        let events = active(&mut c, 0, 45 * 60 + 10);
        assert_eq!(events, vec![Event::Remind(0)]);
    }

    #[test]
    fn reminders_are_spaced_out() {
        let mut c = Companion::new(settings(), DAY);
        let events = active(&mut c, 0, 60 * 60 + 10);
        let at: Vec<_> = events.iter().filter(|e| matches!(e, Event::Remind(_))).collect();
        assert_eq!(at, vec![&Event::Remind(0), &Event::Remind(1)]);
    }

    #[test]
    fn silenced_reminders_are_dropped_not_queued() {
        let mut c = Companion::new(settings(), DAY);
        c.silence(0);
        assert!(active(&mut c, 0, 3500).is_empty());
        c.unsilence();
        assert!(active(&mut c, 3500, 3600).is_empty());
    }

    #[test]
    fn fullscreen_defers_reminder() {
        let mut c = Companion::new(settings(), DAY);
        let busy = |t| Now { busy: true, ..now(t, 0) };
        let during: Vec<_> = (0..45 * 60 + 100).step_by(2).flat_map(|t| c.update(&busy(t))).collect();
        assert!(during.is_empty());
        assert_eq!(c.update(&now(45 * 60 + 102, 0)), vec![Event::Remind(0)]);
    }

    #[test]
    fn pomodoro_cycles() {
        let mut c = Companion::new(settings(), DAY);
        c.settings.reminders[0].on = false;
        c.settings.reminders[1].on = false;
        c.start_pomodoro(0);
        let events = active(&mut c, 0, POMODORO_WORK + POMODORO_BREAK + 10);
        assert_eq!(events, vec![Event::Say(Topic::PomodoroBreak), Event::Say(Topic::PomodoroBack)]);
    }

    #[test]
    fn new_day_resets_stats_but_keeps_affection() {
        let mut c = Companion::new(settings(), DAY);
        c.stats.breaks = 3;
        c.affection = 80.0;
        c.update(&Now { day: DAY + 1, ..now(0, 0) });
        assert_eq!(c.stats.breaks, 0);
        assert_eq!(c.affection, 80.0);
    }

    fn quiet_settings() -> Settings {
        let mut s = settings();
        s.reminders.iter_mut().for_each(|r| r.on = false);
        s
    }

    #[test]
    fn friday_is_celebrated_once_in_the_afternoon() {
        let mut c = Companion::new(quiet_settings(), DAY);
        let friday = |t, hour| Now { weekday: 5, hour, ..now(t, 0) };
        assert!(c.update(&friday(0, 10)).is_empty());
        assert_eq!(c.update(&friday(10, 16)), vec![Event::Say(Topic::Friday)]);
        let later: Vec<_> = (12..4000).step_by(2).flat_map(|t| c.update(&friday(t, 17))).collect();
        assert!(!later.contains(&Event::Say(Topic::Friday)));
    }

    #[test]
    fn birthday_comes_first_and_brings_hearts() {
        let mut c = Companion::new(quiet_settings(), DAY);
        let before = c.affection;
        let party = Now { birthday: true, weekday: 5, hour: 16, ..now(0, 0) };
        assert_eq!(c.update(&party), vec![Event::Say(Topic::Birthday)]);
        assert!(c.affection > before);
    }

    #[test]
    fn three_hours_without_a_break_gets_a_nudge() {
        let mut c = Companion::new(quiet_settings(), DAY);
        let events = active(&mut c, 0, 3 * 3600 + 30);
        assert_eq!(events.iter().filter(|e| **e == Event::Say(Topic::NoBreak)).count(), 1);
    }

    #[test]
    fn routine_flags_survive_a_restart() {
        let mut c = Companion::new(quiet_settings(), DAY);
        c.update(&Now { weekday: 5, hour: 16, ..now(0, 0) });
        let mut d = Companion::new(quiet_settings(), DAY);
        d.load_string(&c.save_string());
        assert!(d.update(&Now { weekday: 5, hour: 16, ..now(200, 0) }).is_empty());
    }

    #[test]
    fn state_round_trip() {
        let mut c = Companion::new(settings(), DAY);
        c.stats.water = 4;
        c.affection = 77.0;
        let mut d = Companion::new(settings(), DAY);
        d.load_string(&c.save_string());
        assert_eq!(d.stats, c.stats);
        assert_eq!(d.affection, 77.0);
    }

    #[test]
    fn fill_placeholders() {
        let s = Stats { together_secs: 3 * 3600 + 7 * 60, breaks: 1, water: 0, ..Stats::new(DAY) };
        assert_eq!(fill("{nome}: {juntos} {pausas} {agua}", &s, "Lance"), "Lance: 3h07min 1 pausa nenhuma vez");
    }

    #[test]
    fn battery_warns_once_and_thanks_when_charging() {
        let mut c = Companion::new(settings(), DAY);
        assert_eq!(c.on_power(true, 50), None);
        assert_eq!(c.on_power(true, 20), Some(Topic::BatteryLow));
        assert_eq!(c.on_power(true, 15), None);
        assert_eq!(c.on_power(false, 15), Some(Topic::Charging));
        assert_eq!(c.on_power(false, 16), None);
    }

    #[test]
    fn feeding_has_cooldown() {
        let mut c = Companion::new(settings(), DAY);
        assert!(c.hungry(0));
        c.on_fed(0);
        assert!(!c.hungry(60));
        assert!(c.hungry(20 * 60));
    }

    #[test]
    fn typing_sensor_needs_steady_typing_with_still_mouse() {
        let mut s = TypingSensor::default();
        // Mexendo o mouse: não conta.
        let moved = (1..100).filter(|&i| s.sample(i, (i as i32, 0))).count();
        assert_eq!(moved, 0);
        // Digitando sem parar: dispara uma vez só.
        let typed = (100..300).filter(|&i| s.sample(i, (0, 0))).count();
        assert_eq!(typed, 1);
        // Para um pouco e volta: dispara de novo.
        (0..40).for_each(|_| {
            s.sample(300, (0, 0));
        });
        assert_eq!((300..400).filter(|&i| s.sample(i + 1, (0, 0))).count(), 1);
    }

    #[test]
    fn typing_comment_has_cooldown() {
        let mut c = Companion::new(settings(), DAY);
        assert_eq!(c.on_typing(0), Some(Topic::Typing));
        assert_eq!(c.on_typing(600), None);
        assert_eq!(c.on_typing(46 * 60), Some(Topic::Typing));
    }
}
