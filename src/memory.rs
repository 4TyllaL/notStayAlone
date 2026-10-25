//! Memória do mascote: fatos curtos que você contou na conversa ("tem prova de
//! cálculo na sexta"), guardados em %APPDATA%\StayAlone\memoria.txt — um por
//! linha, texto puro, para você ler, editar ou apagar quando quiser.
//!
//! A IA marca o que vale lembrar numa linha "LEMBRAR: ..." no fim da resposta;
//! o app tira essa linha do balão, filtra o que parece dado sensível e guarda.

use std::{fs, path::PathBuf};

use crate::{config::read_file, lang, win::clean_line};

/// Quantos fatos ficam guardados (os mais antigos saem primeiro).
const MAX_FACTS: usize = 30;
const MAX_FACT: usize = 140;
const MARK: &str = "lembrar:";

/// `%APPDATA%\StayAlone\memoria.txt`
pub fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("StayAlone").join("memoria.txt"))
}

pub fn load() -> Vec<String> {
    let (day, _, _) = unsafe { crate::system::clock() };
    load_on((day % 100, day / 100 % 100))
}

/// Os fatos guardados que ainda valem em `today` (dia, mês).
fn load_on(today: (u32, u32)) -> Vec<String> {
    let Some(text) = path().and_then(|p| read_file(&p)) else { return Vec::new() };
    text.lines()
        .map(|l| clean_line(l, MAX_FACT))
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !expired(l, today))
        .take(MAX_FACTS)
        .collect()
}

/// Dia do ano (ano comum) de um (dia, mês).
fn year_day((day, month): (u32, u32)) -> u32 {
    const BEFORE: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    BEFORE[(month as usize).clamp(1, 12) - 1] + day
}

/// "28/09" no começo de `text` -> (28, 9).
fn parse_dm(text: &str) -> Option<(u32, u32)> {
    let (d, m) = text.get(..5)?.split_once('/')?;
    let (d, m) = (d.parse().ok()?, m.parse().ok()?);
    ((1..=31).contains(&d) && (1..=12).contains(&m)).then_some((d, m))
}

/// O fato "(28/09, domingo) [até 03/10] prova" já venceu em `today`? Sem ano: conta os dias
/// desde o dia em que foi guardado, dando a volta no ano se for preciso.
fn expired(line: &str, today: (u32, u32)) -> bool {
    let Some(rest) = line.strip_prefix('(') else { return false };
    let Some((head, body)) = rest.split_once(") ") else { return false };
    let Some(until) = body.strip_prefix("[até ").and_then(parse_dm) else { return false };
    let Some(saved) = parse_dm(head) else { return false };
    let span = |from, to| (year_day(to) + 365 - year_day(from)) % 365;
    span(saved, today) > span(saved, until)
}

fn save(facts: &[String]) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut text = String::from("# O que o mascote lembra de você (um fato por linha; edite ou apague à vontade)\n");
    for fact in facts {
        text += fact;
        text += "\n";
    }
    let _ = crate::config::write_atomic(&path, &text);
}

/// Guarda os fatos novos (com a data `today`, ex.: "28/09").
pub fn remember(new: &[String], today: &str) {
    if new.is_empty() {
        return;
    }
    let mut facts = load();
    merge(&mut facts, new, today);
    save(&facts);
}

/// Garante que o arquivo existe (para abrir no Bloco de Notas) e devolve o caminho.
pub fn create() -> Option<PathBuf> {
    let path = path()?;
    if !path.is_file() {
        save(&[]);
    }
    path.is_file().then_some(path)
}

pub fn forget_all() {
    if let Some(path) = path() {
        let _ = fs::remove_file(path);
    }
}

/// O texto do fato, sem a data nem o prazo (para não repetir o mesmo fato).
fn fact_key(line: &str) -> String {
    let body = line.split_once(") ").map_or(line, |(_, f)| f);
    let body = match body.strip_prefix("[até ").and_then(|b| b.split_once("] ")) {
        Some((_, f)) => f,
        None => body,
    };
    body.to_lowercase()
}

fn merge(facts: &mut Vec<String>, new: &[String], today: &str) {
    for fact in new {
        let fact = format!("({today}) {fact}");
        let key = fact_key(&fact);
        facts.retain(|f| fact_key(f) != key);
        facts.push(fact);
    }
    let excess = facts.len().saturating_sub(MAX_FACTS);
    facts.drain(..excess);
}

/// Separa a resposta da IA em (o que aparece no balão, fatos para lembrar).
pub fn split_reply(reply: &str) -> (String, Vec<String>) {
    let mut shown = Vec::new();
    let mut facts = Vec::new();
    for line in reply.lines() {
        let trimmed = line.trim();
        match remember_line(trimmed) {
            Some((until, text)) => {
                let fact = clean_line(text, MAX_FACT);
                if !fact.is_empty() && !sensitive(&fact) {
                    facts.push(match until {
                        Some((d, m)) => format!("[até {d:02}/{m:02}] {fact}"),
                        None => fact,
                    });
                }
            }
            None => shown.push(line),
        }
    }
    (shown.join("\n").trim().to_string(), facts)
}

/// "LEMBRAR: fato" ou "LEMBRAR(03/10): fato" -> (prazo, fato).
fn remember_line(line: &str) -> Option<(Option<(u32, u32)>, &str)> {
    let head = line.get(..MARK.len() - 1).filter(|h| h.eq_ignore_ascii_case("lembrar"))?;
    let rest = &line[head.len()..];
    if let Some(text) = rest.strip_prefix(':') {
        return Some((None, text.trim()));
    }
    let (date, text) = rest.strip_prefix('(')?.split_once("):")?;
    Some((parse_dm(date.trim()), text.trim()))
}

/// Parece senha, documento, cartão ou contato? Então não guarda.
fn sensitive(fact: &str) -> bool {
    let lower = fact.to_lowercase();
    // Palavras soltas (senão "pixel" casaria com "pix"); as de duas palavras valem como trecho.
    let words = ["senha", "password", "cpf", "rg", "cartão", "cartao", "pix", "token", "chave", "passport"];
    let phrases = ["conta bancária", "credit card", "bank account", "social security"];
    let tokens: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).collect();
    let long_number = fact.split(|c: char| !c.is_ascii_digit()).any(|digits| digits.len() >= 6);
    words.iter().any(|w| tokens.contains(w)) || phrases.iter().any(|p| lower.contains(p)) || long_number || fact.contains('@')
}

/// Trecho do prompt com as lembranças e a instrução de marcar coisas novas.
pub fn prompt(facts: &[String], today: &str) -> String {
    if lang::is_english() {
        let mut text = format!(
            " Today is {today}. If the person tells you something personal worth remembering later (plans, \
             likes, events like an exam or a trip), add at the end of your answer a separate line starting \
             with \"LEMBRAR:\" (exactly this word) and the fact in a few words, in the third person. Never \
             keep passwords, documents, health or financial data."
        );
        if !facts.is_empty() {
            text += " What you remember about the person (date in parentheses; use it naturally, don't list it): ";
            text += &facts.join("; ");
            text += ".";
        }
        return text;
    }
    let mut text = format!(
        " Hoje é {today}. Se a pessoa contar algo pessoal que valha lembrar depois (planos, gostos, \
         eventos como uma prova ou uma viagem), acrescente no fim da resposta uma linha separada \
         começando com \"LEMBRAR:\" e o fato em poucas palavras, na terceira pessoa e com acentos.          Se ele deixa de valer numa data (uma prova, uma viagem), escreva \"LEMBRAR(DD/MM):\" com essa data. Nunca guarde \
         senhas, documentos, dados de saúde ou financeiros."
    );
    if !facts.is_empty() {
        text += " O que você lembra sobre a pessoa (data entre parênteses; use com naturalidade, sem listar): ";
        text += &facts.join("; ");
        text += ".";
    }
    text
}

#[cfg(test)]
mod tests {
    #[test]
    fn sensitive_matches_whole_words_only() {
        assert!(!super::sensitive("gosta de pixel art"));
        assert!(!super::sensitive("comprou um chaveiro"));
        assert!(super::sensitive("a senha é azul"));
        assert!(super::sensitive("manda um Pix, por favor"));
        assert!(super::sensitive("tem conta bancária no banco X"));
    }

    use super::*;

    #[test]
    fn deadline_is_parsed_and_kept_in_the_fact() {
        let (shown, facts) = split_reply("Boa!
LEMBRAR(03/10): tem prova
LEMBRAR: gosta de gatos");
        assert_eq!(shown, "Boa!");
        assert_eq!(facts, vec!["[até 03/10] tem prova", "gosta de gatos"]);
    }

    #[test]
    fn facts_expire_after_their_date_even_across_new_year() {
        let line = "(28/09, domingo) [até 03/10] tem prova";
        assert!(!expired(line, (3, 10)));
        assert!(expired(line, (4, 10)));
        assert!(expired(line, (1, 12)));
        assert!(!expired("(28/09, domingo) gosta de gatos", (1, 12)));
        let wrap = "(28/12, sábado) [até 02/01] viagem";
        assert!(!expired(wrap, (1, 1)));
        assert!(expired(wrap, (3, 1)));
    }

    #[test]
    fn same_fact_with_a_new_deadline_replaces_the_old_one() {
        let mut facts = vec!["(01/10) [até 03/10] tem prova".to_string()];
        merge(&mut facts, &["[até 05/10] tem prova".to_string()], "02/10");
        assert_eq!(facts, vec!["(02/10) [até 05/10] tem prova"]);
    }

    #[test]
    fn remembered_lines_are_taken_out_of_the_bubble() {
        let (shown, facts) = split_reply("Boa sorte na prova!\nLEMBRAR: tem prova de cálculo na sexta\nlembrar: gosta de gatos");
        assert_eq!(shown, "Boa sorte na prova!");
        assert_eq!(facts, vec!["tem prova de cálculo na sexta", "gosta de gatos"]);
    }

    #[test]
    fn sensitive_things_are_never_kept() {
        let reply = "Ok!\nLEMBRAR: a senha do wifi é abc\nLEMBRAR: CPF 12345678900\nLEMBRAR: email eu@x.com\nLEMBRAR: gosta de café";
        assert_eq!(split_reply(reply).1, vec!["gosta de café"]);
    }

    #[test]
    fn facts_are_dated_deduplicated_and_capped() {
        let mut facts = Vec::new();
        merge(&mut facts, &["gosta de café".into()], "01/09");
        merge(&mut facts, &["Gosta de café".into()], "28/09");
        assert_eq!(facts, vec!["(28/09) Gosta de café"]);
        for i in 0..40 {
            merge(&mut facts, &[format!("fato {i}")], "28/09");
        }
        assert_eq!(facts.len(), MAX_FACTS);
        assert_eq!(facts.last().map(String::as_str), Some("(28/09) fato 39"));
    }
}
