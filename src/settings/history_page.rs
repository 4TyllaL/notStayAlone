//! Página Histórico: os últimos dias que o mascote anotou (água, pausas, tempo juntos).

use super::*;
use crate::companion::{Companion, DayRecord, Settings};

/// Quantos dias a lista mostra (o `state.ini` guarda até 60).
const SHOWN: usize = 30;

/// "20261027" -> "27/10".
fn short_date(day: u32) -> String {
    format!("{:02}/{:02}", day % 100, day / 100 % 100)
}

/// 7500 -> "2h05"; 2100 -> "35 min".
fn together(secs: u64) -> String {
    match secs / 60 {
        m if m >= 60 => format!("{}h{:02}", m / 60, m % 60),
        m => format!("{m} min"),
    }
}

/// Copos como quadradinhos (até 12) e o número.
fn glasses(water: u32) -> String {
    format!("{} {water}", "\u{25A0}".repeat(water.min(12) as usize))
}

/// As linhas da lista, do dia mais novo para o mais antigo: (dia, água, pausas, juntos).
pub(super) fn rows(companion: &Companion) -> Vec<[String; 4]> {
    let today = &companion.stats;
    let mut days: Vec<DayRecord> = companion.history.iter().copied().filter(|r| r.day != today.day).collect();
    if today.day != 0 {
        days.push(DayRecord { day: today.day, together_secs: today.together_secs, breaks: today.breaks, water: today.water });
    }
    days.sort_by_key(|r| std::cmp::Reverse(r.day));
    days.into_iter()
        .take(SHOWN)
        .map(|r| [short_date(r.day), glasses(r.water), r.breaks.to_string(), together(r.together_secs)])
        .collect()
}

pub(super) unsafe fn fill_history(hwnd: HWND) {
    let mut companion = Companion::new(Settings::default(), 0);
    if let Some(state) = config::read_state() {
        companion.load_string(&state);
    }
    let list = item(hwnd, IDC_HISTORY);
    SendMessageW(list, LVM_DELETEALLITEMS, 0, 0);
    for (i, row) in rows(&companion).iter().enumerate() {
        insert_row(list, i, &[&row[0], &row[1], &row[2], &row[3]]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_short_and_readable() {
        assert_eq!(short_date(20261027), "27/10");
        assert_eq!(together(2100), "35 min");
        assert_eq!(together(7500), "2h05");
        assert_eq!(glasses(3), "\u{25A0}\u{25A0}\u{25A0} 3");
        assert_eq!(glasses(20).matches("\u{25A0}").count(), 12);
    }

    #[test]
    fn rows_put_today_first_and_skip_the_duplicate() {
        let mut c = Companion::new(Settings::default(), 20261027);
        c.stats.water = 5;
        c.stats.together_secs = 3600;
        c.history = vec![
            DayRecord { day: 20261025, together_secs: 600, breaks: 1, water: 2 },
            DayRecord { day: 20261027, together_secs: 1, breaks: 0, water: 0 }, // o mesmo dia de hoje: vale o de hoje
            DayRecord { day: 20261026, together_secs: 7500, breaks: 3, water: 8 },
        ];
        let rows = rows(&c);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0], "27/10");
        assert_eq!(rows[0][3], "1h00");
        assert_eq!(rows[1][0], "26/10");
        assert_eq!(rows[2][0], "25/10");
    }
}
