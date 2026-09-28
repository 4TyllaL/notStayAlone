//! Memória do mascote: fatos curtos que você contou na conversa ("tem prova de
//! cálculo na sexta"), guardados em %APPDATA%\StayAlone\memoria.txt — um por
//! linha, texto puro, para você ler, editar ou apagar quando quiser.
//!
//! A IA marca o que vale lembrar numa linha "LEMBRAR: ..." no fim da resposta;
//! o app tira essa linha do balão, filtra o que parece dado sensível e guarda.

use std::{fs, path::PathBuf};

use crate::{config::read_file, win::clean_line};

/// Quantos fatos ficam guardados (os mais antigos saem primeiro).
const MAX_FACTS: usize = 30;
const MAX_FACT: usize = 140;
const MARK: &str = "lembrar:";

/// `%APPDATA%\StayAlone\memoria.txt`
pub fn path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("StayAlone").join("memoria.txt"))
}

pub fn load() -> Vec<String> {
    let Some(text) = path().and_then(|p| read_file(&p)) else { return Vec::new() };
    text.lines()
        .map(|l| clean_line(l, MAX_FACT))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .take(MAX_FACTS)
        .collect()
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
    let _ = fs::write(path, text);
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

fn merge(facts: &mut Vec<String>, new: &[String], today: &str) {
    for fact in new {
        let fact = format!("({today}) {fact}");
        let key = fact.split_once(") ").map_or(fact.as_str(), |(_, f)| f).to_lowercase();
        facts.retain(|f| f.split_once(") ").map_or(f.as_str(), |(_, old)| old).to_lowercase() != key);
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
        match trimmed.get(..MARK.len()).filter(|head| head.eq_ignore_ascii_case(MARK)) {
            Some(_) => {
                let fact = clean_line(trimmed[MARK.len()..].trim(), MAX_FACT);
                if !fact.is_empty() && !sensitive(&fact) {
                    facts.push(fact);
                }
            }
            None => shown.push(line),
        }
    }
    (shown.join("\n").trim().to_string(), facts)
}

/// Parece senha, documento, cartão ou contato? Então não guarda.
fn sensitive(fact: &str) -> bool {
    let lower = fact.to_lowercase();
    let words = ["senha", "password", "cpf", "rg ", "cartão", "cartao", "conta bancária", "pix", "token", "chave"];
    let long_number = fact.split(|c: char| !c.is_ascii_digit()).any(|digits| digits.len() >= 6);
    words.iter().any(|w| lower.contains(w)) || long_number || fact.contains('@')
}

/// Trecho do prompt com as lembranças e a instrução de marcar coisas novas.
pub fn prompt(facts: &[String], today: &str) -> String {
    let mut text = format!(
        " Hoje é {today}. Se a pessoa contar algo pessoal que valha lembrar depois (planos, gostos, \
         eventos como uma prova ou uma viagem), acrescente no fim da resposta uma linha separada \
         começando com \"LEMBRAR:\" e o fato em poucas palavras, na terceira pessoa. Nunca guarde \
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
    use super::*;

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
