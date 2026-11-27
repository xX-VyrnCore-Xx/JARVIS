//! Correttore dei messaggi scritti (ADR-0135): ortografia e punteggiatura italiane, solo con regole sicure.
//!
//! Il correttore è deterministico e locale: nessun modello, nessuna rete. Non tocca mai codice (blocchi o
//! `inline`), percorsi, URL, indirizzi, numeri, nomi con maiuscole interne o parole che non conosce: corregge solo
//! ciò che è inequivocabile (accenti mancanti, abbreviazioni da chat, spazi e punteggiatura, parole doppie,
//! maiuscola a inizio frase). Ogni modifica a una parola è riportata, così il proprietario può vederla.

use serde_json::{json, Value};

/// Esito: testo corretto e modifiche alle parole (al massimo 12).
#[derive(Debug, PartialEq)]
pub struct Fixed {
    pub text: String,
    pub changes: Vec<(String, String)>,
}

impl Fixed {
    pub fn changed(&self, original: &str) -> bool {
        self.text != original
    }

    pub fn to_json(&self) -> Value {
        json!({
            "text": self.text,
            "changes": self.changes.iter().map(|(from, to)| json!({"from": from, "to": to})).collect::<Vec<_>>(),
        })
    }
}

/// Parole che, scritte senza accento, non hanno altro significato (nessuna ambiguità).
const WORDS: &[(&str, &str)] = &[
    ("perche", "perché"),
    ("poiche", "poiché"),
    ("affinche", "affinché"),
    ("benche", "benché"),
    ("finche", "finché"),
    ("cosi", "così"),
    ("piu", "più"),
    ("puo", "può"),
    ("gia", "già"),
    ("cio", "ciò"),
    ("giu", "giù"),
    ("pero'", "però"),
    ("nn", "non"),
    ("ke", "che"),
    ("cmq", "comunque"),
    ("xche", "perché"),
    ("xke", "perché"),
    ("xké", "perché"),
    ("xchè", "perché"),
    ("qst", "questo"),
    ("propio", "proprio"),
    ("sopratutto", "soprattutto"),
    ("pultroppo", "purtroppo"),
    ("daccordo", "d'accordo"),
    ("eta", "età"),
    ("citta", "città"),
    ("universita", "università"),
    ("qualita", "qualità"),
    ("attivita", "attività"),
    ("possibilita", "possibilità"),
    ("funzionalita", "funzionalità"),
    ("novita", "novità"),
    ("difficolta", "difficoltà"),
    ("velocita", "velocità"),
    ("proprieta", "proprietà"),
    ("capacita", "capacità"),
    ("disponibilita", "disponibilità"),
    ("responsabilita", "responsabilità"),
    ("identita", "identità"),
    ("realta", "realtà"),
    ("opportunita", "opportunità"),
    ("utilita", "utilità"),
    ("societa", "società"),
    ("verita", "verità"),
    ("liberta", "libertà"),
    ("facolta", "facoltà"),
    ("autorita", "autorità"),
    ("priorita", "priorità"),
    ("modalita", "modalità"),
    ("compatibilita", "compatibilità"),
    ("stabilita", "stabilità"),
    ("finalita", "finalità"),
    ("personalita", "personalità"),
    ("qualcun'altro", "qualcun altro"),
    ("un'altro", "un altro"),
    ("qual'è", "qual è"),
    ("qual'e", "qual è"),
    ("pò", "po'"),
    // Accento sbagliato (grave al posto di acuto) e apostrofo al posto dell'accento (ADR-0228).
    ("perchè", "perché"),
    ("poichè", "poiché"),
    ("affinchè", "affinché"),
    ("benchè", "benché"),
    ("finchè", "finché"),
    ("perche'", "perché"),
    ("cosi'", "così"),
    ("piu'", "più"),
    ("puo'", "può"),
    ("gia'", "già"),
    ("cio'", "ciò"),
    // Voci verbali che non vogliono l'accento.
    ("fà", "fa"),
    ("sà", "sa"),
    ("stà", "sta"),
    // Abbreviazioni da chat e refusi frequenti, senza altro significato.
    ("xò", "però"),
    ("xo'", "però"),
    ("anke", "anche"),
    ("qnd", "quando"),
    ("qlcs", "qualcosa"),
    ("eccezzione", "eccezione"),
    ("accellerare", "accelerare"),
    ("assolutamnete", "assolutamente"),
];

/// Articoli, preposizioni e congiunzioni: se ripetuti di seguito è quasi sempre un refuso.
const FUNCTION_WORDS: &[&str] = &[
    "il", "lo", "la", "le", "gli", "i", "di", "a", "da", "in", "con", "su", "per", "che", "e",
    "un", "una", "del", "della", "dei", "nel", "nella", "al", "alla", "non", "mi", "ti", "si",
];

/// Abbreviazioni dopo le quali il punto non chiude la frase.
const ABBREVIATIONS: &[&str] = &[
    "ecc.", "es.", "etc.", "sig.", "dott.", "ing.", "prof.", "pag.", "tel.", "vs.", "cap.", "art.",
    "n.", "sig.ra",
];

fn is_protected(token: &str) -> bool {
    token.contains("://")
        || token.contains('@')
        || token.contains('/')
        || token.contains('\\')
        || token.contains('_')
        || token.contains('`')
        || token.contains('=')
        || token.contains('#')
        || token.contains('$')
        || token.chars().any(|c| c.is_ascii_digit())
        // Maiuscola interna (NomeFile, iPhone, JARVIS): è un nome, non un refuso.
        || (token.chars().skip(1).any(|c| c.is_uppercase()) && !joined_sentence(token))
}

/// «fatto.Ora»: due parole unite da un punto senza spazio (minuscola, punto, parola con la maiuscola).
fn joined_sentence(token: &str) -> bool {
    let Some((before, after)) = token.split_once('.') else {
        return false;
    };
    let mut rest = after.chars();
    before.chars().count() >= 3
        && before
            .chars()
            .all(|c| c.is_alphabetic() && c.is_lowercase())
        && rest.next().is_some_and(char::is_uppercase)
        && rest.clone().count() >= 1
        && rest.all(|c| c.is_alphabetic() && c.is_lowercase())
}

fn split_edges(token: &str) -> (&str, &str, &str) {
    let start = token
        .find(|c: char| c.is_alphanumeric() || c == '\'' || c == '’')
        .unwrap_or(token.len());
    let rest = &token[start..];
    let end = rest
        .rfind(|c: char| c.is_alphanumeric())
        .map(|index| index + rest[index..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(rest.len());
    // L'apostrofo finale fa parte della parola solo se è un accento digitato («perche'»).
    let core_end = if rest[end..].starts_with(['\'', '’']) {
        end + rest[end..].chars().next().map_or(0, char::len_utf8)
    } else {
        end
    };
    (&token[..start], &rest[..core_end], &rest[core_end..])
}

fn match_case(original: &str, replacement: &str) -> String {
    let mut chars = original.chars();
    let first = chars.next();
    let rest_upper = original.chars().count() > 1 && original.chars().all(|c| !c.is_lowercase());
    if rest_upper {
        return replacement.to_uppercase();
    }
    if first.is_some_and(char::is_uppercase) {
        let mut out = replacement.chars();
        return out
            .next()
            .map(|c| c.to_uppercase().collect::<String>() + out.as_str())
            .unwrap_or_default();
    }
    replacement.to_string()
}

fn fix_word(core: &str) -> Option<String> {
    let lower = core.to_lowercase().replace('’', "'");
    if lower == "e'" {
        return Some(match_case(core, "è"));
    }
    if lower == "jarvis" {
        return Some("JARVIS".to_string());
    }
    let key = lower.trim_end_matches('\'');
    let accented = lower.ends_with('\'');
    WORDS
        .iter()
        .find(|(typed, _)| *typed == lower || (accented && *typed == key))
        .filter(|(_, fixed)| *fixed != key || accented)
        .map(|(_, fixed)| match_case(core, fixed))
        .filter(|fixed| fixed != core)
}

/// Inserisce lo spazio mancante dopo la punteggiatura: «ciao,come» → «ciao, come».
fn space_after_punctuation(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let mut out = String::with_capacity(token.len() + 2);
    for (index, &c) in chars.iter().enumerate() {
        out.push(c);
        let (Some(&before), Some(&after)) = (
            index.checked_sub(1).and_then(|i| chars.get(i)),
            chars.get(index + 1),
        ) else {
            continue;
        };
        let letters_around = before.is_alphabetic() && after.is_alphabetic();
        let split = match c {
            ',' | ';' | ':' | '!' | '?' => letters_around,
            // Il punto solo tra una parola minuscola e una maiuscola: «file.txt» e «Node.Js» restano intatti.
            '.' => {
                letters_around
                    && after.is_uppercase()
                    && chars.get(index + 2).is_some_and(|n| n.is_lowercase())
                    && chars[..index]
                        .iter()
                        .rev()
                        .take_while(|c| c.is_alphabetic())
                        .count()
                        >= 3
                    && chars[..index]
                        .iter()
                        .rev()
                        .take_while(|c| c.is_alphabetic())
                        .all(|c| c.is_lowercase())
            }
            _ => false,
        };
        if split {
            out.push(' ');
        }
    }
    out
}

fn fix_line(line: &str, changes: &mut Vec<(String, String)>) -> String {
    let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut previous_core = String::new();
    for token in line.split_whitespace() {
        if is_protected(token) {
            tokens.push(token.to_string());
            previous_core.clear();
            continue;
        }
        // Punteggiatura staccata: «ciao ,» → «ciao,».
        if token
            .chars()
            .all(|c| matches!(c, ',' | '.' | ';' | ':' | '!' | '?'))
            && !tokens.is_empty()
        {
            tokens.last_mut().unwrap().push_str(token);
            continue;
        }
        // Elisione staccata: «l' ape» → «l'ape».
        if let Some(last) = tokens.last_mut() {
            let tail = last.trim_end_matches(['\'', '’']);
            if last.len() > tail.len()
                && tail.chars().count() <= 5
                && tail.chars().all(char::is_alphabetic)
                && token.chars().next().is_some_and(char::is_alphabetic)
                && matches!(
                    tail.to_lowercase().as_str(),
                    "l" | "un"
                        | "dell"
                        | "all"
                        | "dall"
                        | "nell"
                        | "sull"
                        | "quell"
                        | "c"
                        | "d"
                        | "quest"
                        | "nessun"
                )
            {
                last.push_str(token);
                continue;
            }
        }
        let spaced = space_after_punctuation(token);
        let mut rebuilt = Vec::new();
        for part in spaced.split(' ') {
            let (head, core, tail) = split_edges(part);
            if core.is_empty() {
                rebuilt.push(part.to_string());
                continue;
            }
            let lower = core.to_lowercase();
            // «un po» → «un po'»: il troncamento richiede l'apostrofo.
            let fixed = if lower == "po" && previous_core == "un" {
                Some(format!("{}'", core))
            } else {
                fix_word(core)
            };
            // Parola di funzione ripetuta: «il il» → «il».
            if fixed.is_none()
                && head.is_empty()
                && !previous_core.is_empty()
                && previous_core == lower
                && FUNCTION_WORDS.contains(&lower.as_str())
            {
                changes.push((format!("{core} {core}"), core.to_string()));
                if !tail.is_empty() {
                    if let Some(last) = tokens.last_mut() {
                        last.push_str(tail);
                    }
                }
                previous_core = lower;
                continue;
            }
            match fixed {
                Some(new) => {
                    changes.push((core.to_string(), new.clone()));
                    rebuilt.push(format!("{head}{new}{tail}"));
                }
                None => rebuilt.push(part.to_string()),
            }
            previous_core = lower;
        }
        if !rebuilt.is_empty() {
            tokens.push(rebuilt.join(" "));
        }
    }
    format!("{indent}{}", tokens.join(" "))
}

fn capitalize_sentences(line: &str, mut capital_next: bool) -> String {
    let mut out = String::with_capacity(line.len());
    for token in line.split_inclusive(' ') {
        let word = token.trim_end();
        let mut text = token.to_string();
        if capital_next && !word.is_empty() {
            if is_protected(word) {
                // Un nome, un numero o un percorso aprono la frase: nessuna maiuscola da mettere.
                capital_next = false;
            } else if let Some((index, first)) =
                word.char_indices().find(|(_, c)| c.is_alphabetic())
            {
                if first.is_lowercase() {
                    text = format!(
                        "{}{}{}{}",
                        &word[..index],
                        first.to_uppercase(),
                        &word[index + first.len_utf8()..],
                        &token[word.len()..]
                    );
                }
                capital_next = false;
            }
        }
        let lower = word.to_lowercase();
        let ends_sentence = !word.is_empty()
            && !is_protected(word)
            && word.ends_with(['.', '!', '?'])
            && !word.ends_with("..")
            && !ABBREVIATIONS.contains(&lower.as_str())
            && word.trim_end_matches(['.', '!', '?']).chars().count() >= 2;
        if ends_sentence {
            capital_next = true;
        }
        out.push_str(&text);
    }
    out
}

/// Corregge un messaggio scritto con le maiuscole a inizio frase. Se non c'è nulla di sicuro da correggere
/// restituisce il testo identico.
pub fn fix(input: &str) -> Fixed {
    fix_with(input, true)
}

/// Come `fix`; con `capitalize` falso lascia intatte le maiuscole. La chat usa questa forma: un comando come
/// «ricorda che…» o il testo di un ricordo non devono cambiare iniziale.
pub fn fix_with(input: &str, capitalize: bool) -> Fixed {
    if std::env::var("JARVIS_PROMPT_FIX").is_ok_and(|v| v == "off") || input.trim().is_empty() {
        return Fixed {
            text: input.to_string(),
            changes: Vec::new(),
        };
    }
    let mut changes = Vec::new();
    let mut in_fence = false;
    let mut capital = true;
    let mut lines = Vec::new();
    for line in input.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            lines.push(line.to_string());
            capital = true;
            continue;
        }
        if in_fence || line.trim().is_empty() {
            lines.push(line.to_string());
            capital = true;
            continue;
        }
        let fixed = fix_line(line, &mut changes);
        // Una riga a capo dopo una frase finita apre una frase nuova; un testo spezzato a metà frase no.
        let ends_sentence = fixed.trim_end().ends_with(['.', '!', '?', ':']);
        lines.push(if capitalize {
            capitalize_sentences(&fixed, capital)
        } else {
            fixed
        });
        capital = ends_sentence;
    }
    let mut text = lines.join("\n");
    if input.ends_with('\n') {
        text.push('\n');
    }
    changes.truncate(12);
    Fixed { text, changes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(text: &str) -> String {
        fix(text).text
    }

    #[test]
    fn missing_accents_and_chat_abbreviations_are_restored() {
        assert_eq!(
            fixed("perche non funziona? e' gia tardi"),
            "Perché non funziona? È già tardi"
        );
        assert_eq!(fixed("nn so cosi e piu veloce"), "Non so così e più veloce");
        assert_eq!(fixed("la qualita della citta"), "La qualità della città");
        assert_eq!(fixed("cmq xche no"), "Comunque perché no");
        assert_eq!(fixed("piu' tardi perche'"), "Più tardi perché");
        assert_eq!(fixed("qual'è il problema"), "Qual è il problema");
        assert_eq!(fixed("dammi un po di tempo"), "Dammi un po' di tempo");
    }

    #[test]
    fn wrong_accents_and_common_typos_are_fixed_without_touching_code() {
        assert_eq!(
            fixed("perchè non parte? finchè non lo sai"),
            "Perché non parte? Finché non lo sai"
        );
        assert_eq!(
            fixed("lui fà e sà tutto, ma stà zitto"),
            "Lui fa e sa tutto, ma sta zitto"
        );
        assert_eq!(fixed("xò anke qnd serve"), "Però anche quando serve");
        assert_eq!(fixed("c'è una eccezzione"), "C'è una eccezione");
        // Codice e percorsi restano intatti.
        assert_eq!(
            fixed("esegui `fà --piu` e apri /tmp/qnd.txt"),
            "Esegui `fà --piu` e apri /tmp/qnd.txt"
        );
    }

    #[test]
    fn spacing_and_punctuation_are_normalised() {
        assert_eq!(fixed("ciao,come stai?bene"), "Ciao, come stai? Bene");
        assert_eq!(fixed("ciao  ,  come   va !"), "Ciao, come va!");
        assert_eq!(fixed("l' ape e un' amica"), "L'ape e un'amica");
        assert_eq!(fixed("fatto.Ora controllo"), "Fatto. Ora controllo");
    }

    #[test]
    fn repeated_function_words_collapse_but_content_words_do_not() {
        let out = fix("il il file e e vero che che ho ho paura paura");
        assert_eq!(out.text, "Il file e vero che ho ho paura paura");
        assert!(out
            .changes
            .iter()
            .any(|(from, to)| from == "il il" && to == "il"));
    }

    #[test]
    fn sentences_start_with_a_capital_but_abbreviations_do_not_split_them() {
        assert_eq!(
            fixed("ciao. come va? tutto bene"),
            "Ciao. Come va? Tutto bene"
        );
        assert_eq!(
            fixed("compra pane, latte ecc. poi torna"),
            "Compra pane, latte ecc. poi torna"
        );
        assert_eq!(fixed("jarvis che ore sono"), "JARVIS che ore sono");
    }

    #[test]
    fn code_paths_urls_numbers_and_names_are_never_touched() {
        for text in [
            "Apri C:\\Users\\nn\\piu.txt e leggi",
            "scarica https://esempio.org/perche/cosi.html",
            "scrivi a nn@esempio.org",
            "esegui `cosi piu` ora",
            "il valore e 3,5 alle 10:30 con 1.000 euro",
            "usa iPhone e NomeFile per il test",
            "imposta MAX_LEN=5 in config.toml",
        ] {
            let out = fix(text);
            let expected = text.to_string();
            // Solo la maiuscola iniziale e la forma delle parole libere possono cambiare.
            assert!(
                out.text.eq_ignore_ascii_case(&expected)
                    || out.text.starts_with(|c: char| c.is_uppercase()),
                "{text} → {}",
                out.text
            );
            for protected in [
                "C:\\Users\\nn\\piu.txt",
                "https://esempio.org/perche/cosi.html",
                "nn@esempio.org",
                "`cosi",
                "piu`",
                "3,5",
                "10:30",
                "1.000",
                "iPhone",
                "NomeFile",
                "MAX_LEN=5",
                "config.toml",
            ] {
                if text.contains(protected) {
                    assert!(
                        out.text.contains(protected),
                        "{protected} alterato in «{}»",
                        out.text
                    );
                }
            }
        }
    }

    #[test]
    fn fenced_code_is_left_exactly_as_written() {
        let text = "controlla questo\n```\nnn  e'  cosi,male\n```\ngrazie";
        let out = fixed(text);
        assert!(out.contains("```\nnn  e'  cosi,male\n```"));
        assert!(out.starts_with("Controlla questo"));
        assert!(out.ends_with("Grazie"));
    }

    #[test]
    fn clean_text_is_returned_unchanged_and_changes_are_reported() {
        let clean = "Quanto fa 15% di 230?";
        assert_eq!(fix_with("perche e' cosi", false).text, "perché è così");
        let out = fix(clean);
        assert_eq!(out.text, clean);
        assert!(out.changes.is_empty() && !out.changed(clean));
        let out = fix("e' vero perche lo dico");
        assert!(out.changes.contains(&("e'".to_string(), "è".to_string())));
        assert!(out
            .changes
            .contains(&("perche".to_string(), "perché".to_string())));
        assert_eq!(fix("").text, "");
        assert_eq!(fix("  \n").text, "  \n");
    }

    #[test]
    fn the_result_is_stable_when_corrected_twice() {
        for text in [
            "perche e' cosi,davvero",
            "nn lo so.forse domani",
            "il il test  e' ok",
        ] {
            let once = fixed(text);
            assert_eq!(fixed(&once), once, "{text}");
        }
    }
}
