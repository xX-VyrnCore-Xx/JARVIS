//! Avatar olografico di JARVIS per il client desktop (ADR-0129): la stessa scena della versione macOS
//! (ADR-0128) disegnata con il `Painter` di egui. Tre stati continui (riposo, elaborazione, parlato), onde,
//! anelli con tacche, scintille, scansione sul volto, sguardo a scatti, palpebre, bocca a barre e nucleo.
//! Con «Riduci animazioni» resta un disegno fermo.

use std::f32::consts::{PI, TAU};
use std::time::Duration;

use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Vec2};

const CYAN: Color32 = Color32::from_rgb(64, 224, 255);
const ALLOY: Color32 = Color32::from_rgb(54, 92, 125);
const DEEP: Color32 = Color32::from_rgb(10, 24, 40);
const VISOR: Color32 = Color32::from_rgb(18, 46, 69);
const INDIGO: Color32 = Color32::from_rgb(92, 96, 214);

/// Che cosa sta facendo JARVIS: pilota la scena senza dipendere dal resto dell'app.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Mood {
    pub thinking: bool,
    pub talking: bool,
    /// Un lavoro cloud prosegue in background: quattro satelliti orbitano attorno all'avatar.
    pub working: bool,
    /// Allerta (arresto di emergenza o Core in errore): colori caldi e respiro lento.
    pub alert: bool,
    /// Istante (tempo di egui) dell'ultimo successo: un'onda verde si espande dal centro. 0 = nessuno.
    pub celebrate: f64,
    /// Colore d'accento della personalità dell'utente (RGB); senza, il ciano di JARVIS.
    pub accent: Option<[u8; 3]>,
}

/// Valori che si avvicinano piano al bersaglio, così gli stati si fondono invece di scattare.
#[derive(Clone, Copy, Default)]
struct Blend {
    think: f32,
    talk: f32,
    hover: f32,
    work: f32,
    alert: f32,
}

pub(crate) fn alpha(color: Color32, value: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (value.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

fn unit(value: f32) -> f32 {
    value - value.floor()
}

/// Rumore deterministico in [0, 1): stessa sequenza a ogni avvio, nessuna dipendenza da un generatore.
fn hash(value: f32) -> f32 {
    unit((value * 127.1 + 311.7).sin() * 43_758.547)
}

fn smooth(value: f32) -> f32 {
    let clamped = value.clamp(0.0, 1.0);
    clamped * clamped * (3.0 - 2.0 * clamped)
}

const ALERT: Color32 = Color32::from_rgb(255, 150, 84);

/// Colore principale della scena: il ciano di JARVIS o l'accento dell'utente, che vira al caldo in allerta.
fn tint(accent: Option<[u8; 3]>, alert: f32) -> Color32 {
    let base = accent.map_or(CYAN, |[r, g, b]| Color32::from_rgb(r, g, b));
    mix(base, ALERT, alert)
}

const SATELLITES: usize = 4;

/// Angolo del satellite `index` al tempo `t`: orbita lenta, i quattro satelliti sono sfasati di un quarto di giro.
fn satellite_angle(t: f32, index: usize) -> f32 {
    (t * 0.9 + index as f32 * TAU / SATELLITES as f32).rem_euclid(TAU)
}

/// Altezza relativa della palpebra: 1 aperta, vicino a 0 a metà ammiccamento (ogni 4,9 s).
fn blink(time: f32) -> f32 {
    let phase = time.rem_euclid(4.9);
    if phase < 0.16 {
        (phase - 0.08).abs() / 0.08
    } else {
        1.0
    }
}

/// Luminosità di una tacca (0–1) in base alla distanza angolare dall'onda di luce.
fn tick_glow(angle_degrees: f32, sweep_degrees: f32) -> f32 {
    let mut delta = (angle_degrees - sweep_degrees).abs();
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    (1.0 - delta / 50.0).max(0.0)
}

/// Altezza di una barra della bocca: sillabe quando parla, onda di luce quando elabora.
fn mouth_height(time: f32, index: f32, think: f32, talk: f32) -> f32 {
    let speech =
        (time * 13.0 + index * 1.7).sin().abs() * (time * 5.3 + index * 0.9).sin().abs() * 10.0;
    let wave = (0.5 + 0.5 * (time * 6.0 - index * 0.9).sin()) * 4.0;
    2.0 + talk * speech + (1.0 - talk) * think * wave
}

/// Mezza larghezza della testa all'altezza `y` (spazio dell'avatar), per ritagliare la scansione.
fn head_half_width(y: f32) -> f32 {
    const PROFILE: [(f32, f32); 7] = [
        (-87.0, 24.0),
        (-74.0, 43.0),
        (-43.0, 57.0),
        (12.0, 48.0),
        (42.0, 26.0),
        (43.0, 0.0),
        (200.0, 0.0),
    ];
    if y <= PROFILE[0].0 {
        return 0.0;
    }
    for pair in PROFILE.windows(2) {
        let (y0, w0) = pair[0];
        let (y1, w1) = pair[1];
        if y <= y1 {
            let t = (y - y0) / (y1 - y0);
            return w0 + (w1 - w0) * t;
        }
    }
    0.0
}

pub(crate) fn arc(
    center: Pos2,
    radius: f32,
    from_degrees: f32,
    to_degrees: f32,
    stroke: Stroke,
) -> Shape {
    let steps = (((to_degrees - from_degrees).abs() / 6.0).ceil() as usize).max(2);
    let points = (0..=steps)
        .map(|step| {
            let angle = (from_degrees + (to_degrees - from_degrees) * step as f32 / steps as f32)
                .to_radians();
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    Shape::line(points, stroke)
}

/// Disegna l'avatar in un quadrato di lato `side` e restituisce la risposta (clic e passaggio del puntatore).
pub fn draw(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    reduce_motion: bool,
    side: f32,
    mood: Mood,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "Avatar di JARVIS"));
    let painter = ui.painter_at(rect);

    let now = ctx.input(|input| input.time);
    let blend_id = egui::Id::unique("jarvis-avatar-blend");
    let tap_id = egui::Id::unique("jarvis-avatar-tap");
    let mut blend = ctx
        .data_mut(|data| data.get_temp::<Blend>(blend_id))
        .unwrap_or_default();
    if response.clicked() {
        ctx.data_mut(|data| data.insert_temp(tap_id, now));
    }
    let tapped = ctx
        .data_mut(|data| data.get_temp::<f64>(tap_id))
        .unwrap_or(-1_000.0);
    if reduce_motion {
        blend = Blend {
            think: if mood.thinking { 1.0 } else { 0.0 },
            talk: if mood.talking { 1.0 } else { 0.0 },
            hover: 0.0,
            work: if mood.working { 1.0 } else { 0.0 },
            alert: if mood.alert { 1.0 } else { 0.0 },
        };
    } else {
        let step = 1.0 - (-ctx.input(|input| input.stable_dt).min(0.1) * 3.5).exp();
        blend.think += ((mood.thinking as u8 as f32) - blend.think) * step;
        blend.talk += ((mood.talking as u8 as f32) - blend.talk) * step;
        blend.hover += ((response.hovered() as u8 as f32) - blend.hover) * step * 1.6;
        blend.work += ((mood.working as u8 as f32) - blend.work) * step;
        blend.alert += ((mood.alert as u8 as f32) - blend.alert) * step * 0.7;
    }
    ctx.data_mut(|data| data.insert_temp(blend_id, blend));

    let time = if reduce_motion { 0.0 } else { now as f32 };
    scene(
        &painter,
        rect.center(),
        side,
        time,
        now,
        tapped,
        blend,
        reduce_motion,
        tint(mood.accent, blend.alert),
        mood.celebrate,
    );
    if !reduce_motion {
        ctx.request_repaint_after(Duration::from_millis(33));
    }
    response
}

/// Onda sonora a barre: lenta a riposo, vivace quando JARVIS lavora.
pub fn waveform(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    reduce_motion: bool,
    size: Vec2,
    active: bool,
) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let time = if reduce_motion {
        0.0
    } else {
        ctx.input(|input| input.time) as f32
    };
    let bars = ((size.x / 6.0) as usize).clamp(8, 40);
    for index in 0..bars {
        let envelope = (index as f32 / (bars - 1).max(1) as f32 * PI).sin();
        let wave = if reduce_motion {
            0.35
        } else {
            0.5 + 0.5 * (time * if active { 7.0 } else { 2.2 } + index as f32 * 0.55).sin()
        };
        let level = 0.15 + envelope * if active { 0.85 } else { 0.45 } * wave;
        let x = rect.left() + (index as f32 + 0.5) * rect.width() / bars as f32;
        let height = (4.0 + (size.y - 4.0) * level).min(size.y);
        painter.line_segment(
            [
                egui::pos2(x, rect.center().y - height / 2.0),
                egui::pos2(x, rect.center().y + height / 2.0),
            ],
            Stroke::new(2.4, alpha(CYAN, 0.35 + 0.65 * level)),
        );
    }
    if !reduce_motion {
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

#[allow(clippy::too_many_arguments)]
fn scene(
    painter: &egui::Painter,
    center: Pos2,
    side: f32,
    t: f32,
    now: f64,
    tapped: f64,
    blend: Blend,
    still: bool,
    cyan: Color32,
    celebrate: f64,
) {
    let scale = side / 276.0 * (1.0 + 0.045 * blend.hover);
    let energy = 0.28 + 0.45 * blend.think + 0.30 * blend.talk + 0.12 * blend.hover;
    let at = |x: f32, y: f32| center + Vec2::new(x, y) * scale;
    let line = |width: f32| width * scale.max(0.5);

    // Alone che respira.
    let breath = 0.5 + 0.5 * (t * 1.4).sin();
    for layer in 0..7 {
        let fraction = 1.0 - layer as f32 / 7.0;
        painter.circle_filled(
            center,
            140.0 * scale * (0.35 + 0.65 * fraction),
            alpha(
                cyan,
                (0.012 + 0.020 * energy + 0.006 * breath) * (1.0 - fraction * 0.35),
            ),
        );
    }

    // Lavoro in background: satelliti con scia che orbitano attorno all'avatar.
    if blend.work > 0.02 {
        for index in 0..SATELLITES {
            for trail in 0..6 {
                let angle = satellite_angle(t, index) - trail as f32 * 0.07;
                let at_orbit = center + Vec2::new(angle.cos(), angle.sin()) * 128.0 * scale;
                painter.circle_filled(
                    at_orbit,
                    (3.4 - trail as f32 * 0.45).max(0.8) * scale.max(0.6),
                    alpha(INDIGO, blend.work * (0.9 - trail as f32 * 0.15).max(0.05)),
                );
            }
        }
    }

    // Onde dal centro e onda al clic.
    if !still {
        for k in 0..3 {
            let phase = unit(t * (0.18 + 0.5 * energy) + k as f32 / 3.0);
            painter.circle_stroke(
                center,
                (46.0 + 86.0 * phase) * scale,
                Stroke::new(
                    line(1.2 + blend.talk),
                    alpha(cyan, (1.0 - phase) * (0.05 + 0.20 * energy)),
                ),
            );
        }
        let age = (now - tapped) as f32;
        if (0.0..1.4).contains(&age) {
            let p = age / 1.4;
            painter.circle_stroke(
                center,
                (30.0 + 112.0 * smooth(p)) * scale,
                Stroke::new(
                    line(3.0 * (1.0 - p) + 0.4),
                    alpha(Color32::WHITE, (1.0 - p) * 0.8),
                ),
            );
            painter.circle_stroke(
                center,
                (20.0 + 100.0 * smooth(p)) * scale,
                Stroke::new(line(2.0 * (1.0 - p) + 0.4), alpha(cyan, (1.0 - p) * 0.9)),
            );
        }
    }

    // Successo: due onde verdi si espandono dal centro (una volta sola, con «Riduci animazioni» restano ferme).
    if !still {
        let age = (now - celebrate) as f32;
        if celebrate > 0.0 && (0.0..1.8).contains(&age) {
            let p = age / 1.8;
            for (k, delay) in [0.0_f32, 0.18].into_iter().enumerate() {
                let q = ((p - delay) / (1.0 - delay)).clamp(0.0, 1.0);
                if q > 0.0 {
                    painter.circle_stroke(
                        center,
                        (24.0 + 120.0 * smooth(q)) * scale,
                        Stroke::new(
                            line(3.2 * (1.0 - q) + 0.4),
                            alpha(
                                Color32::from_rgb(105, 220, 174),
                                (1.0 - q) * (0.85 - 0.3 * k as f32),
                            ),
                        ),
                    );
                }
            }
        }
    }
    // Allerta: anello che pulsa piano, diverso dal respiro normale.
    if blend.alert > 0.02 {
        let beat = 0.5 + 0.5 * (t * 2.1).sin();
        painter.circle_stroke(
            center,
            (134.0 + 4.0 * beat) * scale,
            Stroke::new(
                line(2.0 + beat),
                alpha(cyan, blend.alert * (0.35 + 0.4 * beat)),
            ),
        );
    }

    // Anelli.
    let spin = 1.0 + 2.2 * energy;
    let dashes = 54;
    for index in 0..dashes {
        let a0 = (index as f32 / dashes as f32) * 360.0 - t * 14.0 * spin;
        painter.add(arc(
            center,
            128.0 * scale,
            a0,
            a0 + 360.0 / dashes as f32 * 0.4,
            Stroke::new(line(1.0), alpha(cyan, 0.22 + 0.2 * energy)),
        ));
    }
    for k in 0..3 {
        let base = k as f32 * 120.0 + t * 26.0 * spin;
        painter.add(arc(
            center,
            120.0 * scale,
            base,
            base + 46.0,
            Stroke::new(line(2.2), alpha(cyan, 0.5 + 0.4 * energy)),
        ));
    }
    for k in 0..2 {
        let base = k as f32 * 180.0 + 30.0 - t * 18.0 * spin;
        painter.add(arc(
            center,
            111.0 * scale,
            base,
            base + 70.0,
            Stroke::new(line(2.6), alpha(INDIGO, 0.55 + 0.3 * energy)),
        ));
    }
    let sweep = unit(t * 0.12 * spin) * 360.0;
    let ticks = 72;
    for index in 0..ticks {
        let angle = index as f32 / ticks as f32 * 360.0;
        let glow = tick_glow(angle, sweep);
        let direction = Vec2::new(angle.to_radians().cos(), angle.to_radians().sin());
        painter.line_segment(
            [
                center + direction * (101.0 - glow * 3.0) * scale,
                center + direction * (105.0 + glow * 4.0) * scale,
            ],
            Stroke::new(
                line(if index % 6 == 0 { 1.6 } else { 0.9 }),
                alpha(cyan, 0.18 + 0.75 * glow),
            ),
        );
    }
    if blend.think > 0.02 {
        let start = t * 240.0;
        painter.add(arc(
            center,
            132.0 * scale,
            start,
            start + 64.0,
            Stroke::new(line(3.0), alpha(Color32::WHITE, 0.85 * blend.think)),
        ));
        painter.add(arc(
            center,
            132.0 * scale,
            start + 180.0,
            start + 214.0,
            Stroke::new(line(2.0), alpha(cyan, 0.9 * blend.think)),
        ));
    }

    // Scintille in orbita.
    for i in 0..42 {
        let seed = i as f32;
        let radius = 88.0 + 44.0 * hash(seed + 1.0);
        let speed = (0.04 + 0.10 * hash(seed + 2.0))
            * (1.0 + 2.0 * energy)
            * if i % 2 == 0 { 1.0 } else { -1.0 };
        let angle = (hash(seed + 3.0) + t * speed) * TAU;
        let twinkle = 0.25 + 0.75 * (t * (0.8 + hash(seed + 4.0) * 1.6) + seed).sin().abs();
        let size = 0.8 + 1.7 * hash(seed + 5.0);
        let spot = at(angle.cos() * radius, angle.sin() * radius * 0.96);
        painter.circle_filled(spot, size * 2.4 * scale, alpha(cyan, 0.07 * twinkle));
        painter.circle_filled(
            spot,
            size * scale.max(0.45),
            alpha(Color32::WHITE, 0.35 + 0.5 * twinkle),
        );
    }

    // Testa: oscilla, si inclina, respira; ogni ~9 s un breve glitch.
    let sway = if still { 0.0 } else { (t * 0.7).sin() * 2.2 };
    let lift = if still {
        0.0
    } else {
        (t * 1.8).sin() * 3.2 - blend.talk * (t * 9.0).sin().abs() * 1.2
    };
    let tilt = if still {
        0.0
    } else {
        ((t * 0.5).sin() * 1.6 + blend.think * (t * 1.3).sin() * 1.4).to_radians()
    };
    let (sin_tilt, cos_tilt) = tilt.sin_cos();
    let head_at = |x: f32, y: f32| {
        at(
            x * cos_tilt - y * sin_tilt + sway,
            x * sin_tilt + y * cos_tilt + lift,
        )
    };

    let glitch = !still && unit(t / 9.3) < 0.0125;
    if glitch {
        let shift = (hash((t * 40.0).floor()) - 0.5) * 14.0;
        let ghost = |points: &[(f32, f32)], color: Color32| {
            let mut path: Vec<Pos2> = points.iter().map(|(x, y)| head_at(x + shift, *y)).collect();
            path.push(path[0]);
            painter.add(Shape::line(path, Stroke::new(line(1.3), alpha(color, 0.7))));
        };
        ghost(&HEAD, Color32::from_rgb(255, 80, 170));
        ghost(&SHOULDERS, cyan);
    }

    let polygon = |points: &[(f32, f32)], fill: Color32, edge: f32, width: f32| {
        let path: Vec<Pos2> = points.iter().map(|(x, y)| head_at(*x, *y)).collect();
        painter.add(Shape::convex_polygon(
            path,
            fill,
            Stroke::new(line(width), alpha(cyan, edge)),
        ));
    };
    polygon(&SHOULDERS, mix(ALLOY, DEEP, 0.55), 0.5, 1.0);
    polygon(&NECK, mix(ALLOY, DEEP, 0.6), 0.5, 1.0);
    polygon(&HEAD, mix(ALLOY, DEEP, 0.35), 0.55, 1.1);
    polygon(&EYE_BAND, VISOR, 0.4, 1.0);
    for side_sign in [-1.0_f32, 1.0] {
        painter.add(Shape::line(
            vec![
                head_at(side_sign * 20.0, -86.0),
                head_at(side_sign * 36.0, -66.0),
                head_at(side_sign * 45.0, -58.0),
            ],
            Stroke::new(line(1.0), alpha(cyan, 0.4)),
        ));
        painter.line_segment(
            [
                head_at(side_sign * 44.0, -8.0),
                head_at(side_sign * 30.0, 28.0),
            ],
            Stroke::new(line(1.0), alpha(cyan, 0.3)),
        );
    }

    // Fascio di scansione ritagliato sulla testa.
    let sweep_y = -92.0 + 140.0 * unit(t * (0.16 + 0.35 * energy));
    for (offset, strength) in [
        (-10.0, 0.25),
        (-5.0, 0.55),
        (0.0, 1.0),
        (5.0, 0.55),
        (10.0, 0.25),
    ] {
        let y = sweep_y + offset;
        let half = head_half_width(y) * 0.97;
        if half > 1.0 {
            let glow = if offset == 0.0 {
                0.35 + 0.45 * energy
            } else {
                (0.04 + 0.12 * energy) * strength
            };
            painter.line_segment(
                [head_at(-half, y), head_at(half, y)],
                Stroke::new(
                    line(if offset == 0.0 { 1.2 } else { 4.0 }),
                    alpha(cyan, glow),
                ),
            );
        }
    }

    // Occhi: sguardo a scatti, quando elabora cerca avanti e indietro.
    let interval = 1.7_f32;
    let segment = (t / interval).floor();
    let ease = smooth((t / interval - segment) * interval / 0.18);
    let mut gaze_x =
        (hash(segment - 1.0) - 0.5) * 7.0 * (1.0 - ease) + (hash(segment) - 0.5) * 7.0 * ease;
    let mut gaze_y = (hash(segment + 40.0) - 0.5) * 2.4;
    if still {
        gaze_x = 0.0;
        gaze_y = 0.0;
    }
    gaze_x = gaze_x * (1.0 - blend.think) + 7.0 * (t * 3.1).sin() * blend.think;
    gaze_x *= 1.0 - 0.7 * blend.hover;
    let open = 7.0 + blend.hover * 1.5 - blend.think * 1.4;
    let lid = if still { 1.0 } else { blink(t) };
    let height = (open * lid).max(1.6);
    let pulse = 0.82 + 0.18 * (t * (4.0 + 6.0 * blend.think)).sin() + blend.talk * 0.05;
    for x in [-27.0_f32, 13.0] {
        let top = -36.0 + gaze_y + (open - height) / 2.0;
        let corners = [
            head_at(x + gaze_x, top),
            head_at(x + gaze_x + 14.0, top + height),
        ];
        let eye = egui::Rect::from_two_pos(corners[0], corners[1]);
        painter.rect_filled(
            eye.expand(4.0 * scale),
            egui::CornerRadius::same(6),
            alpha(cyan, 0.10 * pulse),
        );
        painter.rect_filled(
            eye.expand(2.0 * scale),
            egui::CornerRadius::same(4),
            alpha(cyan, 0.22 * pulse),
        );
        painter.rect_filled(eye, egui::CornerRadius::same(2), alpha(cyan, pulse));
        painter.rect_filled(
            eye.shrink2(Vec2::new(3.0 * scale, height * 0.28 * scale)),
            egui::CornerRadius::same(1),
            alpha(Color32::WHITE, 0.85 * lid),
        );
    }

    // Bocca a barre.
    let bars = 7;
    for i in 0..bars {
        let index = i as f32;
        let envelope = 1.0 - (index - (bars - 1) as f32 / 2.0).abs() / bars as f32 * 0.8;
        let height = mouth_height(t, index, blend.think, blend.talk) * envelope + 0.6;
        let x = -13.0 + index * 4.2;
        let bar = egui::Rect::from_two_pos(
            head_at(x, 22.0 - height / 2.0),
            head_at(x + 2.8, 22.0 + height / 2.0),
        );
        painter.rect_filled(
            bar,
            egui::CornerRadius::same(1),
            alpha(cyan, 0.40 + 0.5 * blend.think.max(blend.talk)),
        );
    }

    // Nucleo sul petto.
    let core = head_at(0.0, 92.0);
    let pulse_core = 0.5 + 0.5 * (t * (2.0 + 4.0 * energy)).sin();
    painter.circle_filled(core, 24.0 * scale, alpha(cyan, 0.06 + 0.08 * pulse_core));
    painter.circle_filled(core, 15.0 * scale, alpha(cyan, 0.10 + 0.14 * pulse_core));
    let ring_turn = t * 6.0 * (1.0 + energy) * 12.0;
    for k in 0..6 {
        let base = ring_turn + k as f32 * 60.0;
        painter.add(arc(
            core,
            10.0 * scale,
            base,
            base + 34.0,
            Stroke::new(line(1.4), alpha(cyan, 0.9)),
        ));
    }
    let spin_core = t * (1.4 + 2.6 * energy);
    let mut triangle: Vec<Pos2> = (0..3)
        .map(|k| {
            let angle = spin_core + k as f32 * TAU / 3.0;
            core + Vec2::new(angle.cos(), angle.sin()) * 6.5 * scale
        })
        .collect();
    triangle.push(triangle[0]);
    painter.add(Shape::line(
        triangle,
        Stroke::new(line(1.1), alpha(Color32::WHITE, 0.9)),
    ));
    painter.circle_filled(core, (2.6 + pulse_core * 1.2) * scale, Color32::WHITE);
}

fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let blend = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * amount) as u8;
    Color32::from_rgb(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
    )
}

const SHOULDERS: [(f32, f32); 8] = [
    (-101.0, 98.0),
    (-91.0, 65.0),
    (-43.0, 45.0),
    (41.0, 45.0),
    (92.0, 65.0),
    (101.0, 98.0),
    (66.0, 110.0),
    (-66.0, 110.0),
];
const NECK: [(f32, f32); 5] = [
    (-30.0, 59.0),
    (-23.0, 34.0),
    (23.0, 34.0),
    (30.0, 59.0),
    (0.0, 78.0),
];
const HEAD: [(f32, f32); 10] = [
    (-57.0, -43.0),
    (-43.0, -74.0),
    (-24.0, -87.0),
    (24.0, -87.0),
    (43.0, -74.0),
    (57.0, -43.0),
    (48.0, 12.0),
    (26.0, 42.0),
    (-26.0, 42.0),
    (-48.0, 12.0),
];
const EYE_BAND: [(f32, f32); 6] = [
    (-52.0, -40.0),
    (-34.0, -55.0),
    (34.0, -55.0),
    (52.0, -40.0),
    (41.0, -17.0),
    (-41.0, -17.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satellites_are_evenly_spaced_and_stay_on_the_circle() {
        for step in 0..50 {
            let t = step as f32 * 0.37;
            let first = satellite_angle(t, 0);
            let second = satellite_angle(t, 1);
            assert!((0.0..TAU).contains(&first));
            let gap = (second - first).rem_euclid(TAU);
            assert!((gap - PI / 2.0).abs() < 1e-3, "{gap}");
        }
    }

    #[test]
    fn hash_is_deterministic_and_in_range() {
        for i in 0..200 {
            let value = hash(i as f32);
            assert!((0.0..1.0).contains(&value), "{value}");
            assert_eq!(value, hash(i as f32));
        }
    }

    #[test]
    fn blink_closes_briefly_and_then_stays_open() {
        assert!(blink(0.08) < 0.1);
        assert_eq!(blink(1.0), 1.0);
        assert_eq!(blink(4.9 + 2.0), 1.0);
    }

    #[test]
    fn tick_glow_peaks_at_the_sweep_and_wraps_around() {
        assert_eq!(tick_glow(90.0, 90.0), 1.0);
        assert_eq!(tick_glow(0.0, 180.0), 0.0);
        assert!(tick_glow(359.0, 1.0) > 0.9);
    }

    #[test]
    fn mouth_is_still_at_rest_and_moves_when_talking() {
        assert_eq!(mouth_height(1.234, 3.0, 0.0, 0.0), 2.0);
        let moving = (0..200)
            .map(|step| mouth_height(step as f32 * 0.05, 2.0, 0.0, 1.0))
            .fold(0.0, f32::max);
        assert!(moving > 4.0, "{moving}");
    }

    #[test]
    fn head_width_follows_the_silhouette() {
        assert_eq!(head_half_width(-100.0), 0.0);
        assert!((head_half_width(-43.0) - 57.0).abs() < 0.01);
        assert!(head_half_width(0.0) > 40.0);
        assert!(head_half_width(100.0) < 0.01);
    }

    #[test]
    fn smoothing_stays_between_zero_and_one() {
        assert_eq!(smooth(-1.0), 0.0);
        assert_eq!(smooth(2.0), 1.0);
        assert!((smooth(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn tint_follows_the_accent_and_turns_warm_on_alert() {
        assert_eq!(tint(None, 0.0), CYAN);
        assert_eq!(tint(None, 1.0), ALERT);
        assert_eq!(tint(Some([10, 20, 30]), 0.0), Color32::from_rgb(10, 20, 30));
        let half = tint(None, 0.5);
        assert!(half.r() > CYAN.r() && half.r() < ALERT.r());
    }
}
