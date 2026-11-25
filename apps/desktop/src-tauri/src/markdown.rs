//! Resa leggera del Markdown nei messaggi della chat: blocchi di codice con «Copia», grassetto, codice in linea,
//! elenchi e titoli. Nessuna dipendenza: il testo del modello è un dato e qui si formatta soltanto.

use eframe::egui::{self, text::LayoutJob, Color32, FontId, TextFormat};

#[derive(Debug, PartialEq)]
pub enum Block {
    Prose(String),
    Code { language: String, body: String },
}

/// Divide il testo in parti normali e blocchi delimitati da tre apici inversi; un blocco non chiuso (risposta
/// ancora in arrivo) resta codice.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let (mut prose, mut code): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let (mut language, mut inside) = (String::new(), false);
    let flush = |prose: &mut Vec<&str>, out: &mut Vec<Block>| {
        let joined = prose.join("\n");
        let trimmed = joined.trim();
        if !trimmed.is_empty() {
            out.push(Block::Prose(trimmed.to_string()));
        }
        prose.clear();
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("```") {
            if inside {
                out.push(Block::Code {
                    language: std::mem::take(&mut language),
                    body: code.join("\n"),
                });
                code.clear();
                inside = false;
            } else {
                flush(&mut prose, &mut out);
                language = rest.trim().to_string();
                inside = true;
            }
        } else if inside {
            code.push(line);
        } else {
            prose.push(line);
        }
    }
    if inside {
        out.push(Block::Code {
            language,
            body: code.join("\n"),
        });
    } else {
        flush(&mut prose, &mut out);
    }
    out
}

/// Pezzi di una riga: testo, grassetto o codice in linea.
#[derive(Debug, PartialEq)]
pub enum Span {
    Plain(String),
    Bold(String),
    Code(String),
}

pub fn spans(line: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let push_plain = |plain: &mut String, out: &mut Vec<Span>| {
        if !plain.is_empty() {
            out.push(Span::Plain(std::mem::take(plain)));
        }
    };
    while i < chars.len() {
        if chars[i] == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = (i + 2..chars.len().saturating_sub(1))
                .find(|&j| chars[j] == '*' && chars[j + 1] == '*')
                .filter(|&j| j > i + 2)
            {
                push_plain(&mut plain, &mut out);
                out.push(Span::Bold(chars[i + 2..end].iter().collect()));
                i = end + 2;
                continue;
            }
        }
        if chars[i] == '`' {
            if let Some(end) = (i + 1..chars.len())
                .find(|&j| chars[j] == '`')
                .filter(|&j| j > i + 1)
            {
                push_plain(&mut plain, &mut out);
                out.push(Span::Code(chars[i + 1..end].iter().collect()));
                i = end + 1;
                continue;
            }
        }
        plain.push(chars[i]);
        i += 1;
    }
    push_plain(&mut plain, &mut out);
    out
}

/// Riga con i marcatori di elenco e di titolo già risolti: (testo, è un titolo).
fn line_kind(line: &str) -> (String, bool) {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
    {
        return (format!("{indent}•  {rest}"), false);
    }
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if (1..=4).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
        return (trimmed[hashes..].trim().to_string(), true);
    }
    (line.to_string(), false)
}

/// Testo formattato di un blocco normale, pronto per un `Label`.
pub fn prose_job(text: &str, ink: Color32, size: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let normal = TextFormat {
        font_id: FontId::proportional(size),
        color: ink,
        ..Default::default()
    };
    for (index, raw) in text.lines().enumerate() {
        if index > 0 {
            job.append("\n", 0.0, normal.clone());
        }
        let (line, heading) = line_kind(raw);
        for span in spans(&line) {
            let (text, format) = match span {
                Span::Plain(t) if heading => (
                    t,
                    TextFormat {
                        font_id: FontId::proportional(size + 2.0),
                        color: Color32::WHITE,
                        ..Default::default()
                    },
                ),
                Span::Plain(t) => (t, normal.clone()),
                Span::Bold(t) => (
                    t,
                    TextFormat {
                        color: Color32::WHITE,
                        underline: egui::Stroke::NONE,
                        ..normal.clone()
                    },
                ),
                Span::Code(t) => (
                    t,
                    TextFormat {
                        font_id: FontId::monospace(size - 1.0),
                        color: Color32::from_rgb(177, 245, 246),
                        background: Color32::from_rgba_unmultiplied(255, 255, 255, 18),
                        ..Default::default()
                    },
                ),
            };
            job.append(&text, 0.0, format);
        }
    }
    job
}

/// Disegna un messaggio: testo formattato e blocchi di codice con il pulsante «Copia».
pub fn show(ui: &mut egui::Ui, ctx: &egui::Context, text: &str, ink: Color32) {
    for block in blocks(text) {
        match block {
            Block::Prose(body) => {
                ui.add(
                    egui::Label::new(prose_job(&body, ink, 14.5))
                        .wrap()
                        .selectable(true),
                );
            }
            Block::Code { language, body } => {
                egui::Frame::new()
                    .fill(Color32::from_rgb(8, 18, 30))
                    .corner_radius(8)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(if language.is_empty() {
                                    "codice"
                                } else {
                                    language.as_str()
                                })
                                .small()
                                .weak(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Copia").clicked() {
                                        ctx.copy_text(body.clone());
                                    }
                                },
                            );
                        });
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&body)
                                    .monospace()
                                    .color(Color32::from_rgb(210, 232, 242)),
                            )
                            .selectable(true),
                        );
                    });
            }
        }
        ui.add_space(4.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_fences_split_the_message_and_unclosed_ones_stay_code() {
        let text = "Ecco:\n```rust\nfn main() {}\n```\nFatto.";
        assert_eq!(
            blocks(text),
            vec![
                Block::Prose("Ecco:".into()),
                Block::Code {
                    language: "rust".into(),
                    body: "fn main() {}".into()
                },
                Block::Prose("Fatto.".into()),
            ]
        );
        assert_eq!(
            blocks("Prima\n```\nlet x = 1;"),
            vec![
                Block::Prose("Prima".into()),
                Block::Code {
                    language: String::new(),
                    body: "let x = 1;".into()
                },
            ]
        );
        assert!(blocks("   \n ").is_empty());
    }

    #[test]
    fn bold_and_inline_code_are_recognized_and_stray_marks_are_kept() {
        assert_eq!(
            spans("Usa **cargo** e `fmt` ora"),
            vec![
                Span::Plain("Usa ".into()),
                Span::Bold("cargo".into()),
                Span::Plain(" e ".into()),
                Span::Code("fmt".into()),
                Span::Plain(" ora".into()),
            ]
        );
        assert_eq!(
            spans("2 ** 3 e `aperto"),
            vec![Span::Plain("2 ** 3 e `aperto".into())]
        );
        assert_eq!(spans("****"), vec![Span::Plain("****".into())]);
        assert_eq!(spans("àè **ò** `ù`").len(), 4);
    }

    #[test]
    fn lists_and_headings_are_resolved() {
        assert_eq!(line_kind("- uno"), ("•  uno".into(), false));
        assert_eq!(line_kind("  * due"), ("  •  due".into(), false));
        assert_eq!(line_kind("## Titolo"), ("Titolo".into(), true));
        assert_eq!(line_kind("#nontitolo"), ("#nontitolo".into(), false));
        assert_eq!(line_kind("1. voce"), ("1. voce".into(), false));
        let job = prose_job("**Sì**\n- a\n# H", Color32::GRAY, 14.0);
        assert!(job.text.contains("•  a") && job.text.contains('H') && !job.text.contains("**"));
    }
}
