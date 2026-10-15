//! Barre de lecture, ligne d'état, sommaire et aide.

use super::*;

/// Le rappel permanent de la ligne d'état, après « s réglages · ? » : les
/// touches qu'un nouveau venu cherche d'abord.
pub(super) const REMINDER: &str = " aide · espace lecture · ← → phrase · / chercher · t sommaire ";

pub(super) fn render_player(frame: &mut Frame, app: &App, area: Rect) {
    let doc = app.doc();
    let current = app.current_sentence();
    let total = doc.sentences.len().max(1);
    let stats = app.synth.stats();

    let icon = if app.player.is_playing() { "⏸" } else { "▶" };
    let ratio = (current as f64 / total as f64).clamp(0.0, 1.0);

    let inner_width = area.width.saturating_sub(4) as usize;
    let bar_width = inner_width.saturating_sub(46).max(10);
    let filled = (bar_width as f64 * ratio).round() as usize;

    let times = match (app.estimated_elapsed(), app.estimated_total()) {
        (Some(elapsed), Some(total_secs)) => {
            format!("{} / ~{}", clock(elapsed), clock(total_secs))
        }
        // Aucune mesure encore : afficher un temps serait l'inventer.
        _ => "--:-- / --:--".to_string(),
    };

    // Avant le premier son, la barre de progression n'a rien à montrer : elle
    // cède la place à l'étape en cours. Trois secondes de chargement sans
    // retour donnaient un lecteur qui paraissait figé.
    let first = match app.startup_phase() {
        Some(phase) => {
            let elapsed = app.startup_elapsed();
            Line::from(vec![
                Span::styled(
                    format!(" {}  ", startup::spinner(elapsed)),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    phase.label(&app.config.engine.family),
                    Style::default().fg(NORMAL),
                ),
                Span::styled(
                    format!("   {:.1} s", elapsed.as_secs_f32()),
                    Style::default().fg(DIM),
                ),
            ])
        }
        None => Line::from(vec![
        Span::styled(
            format!(" {icon} "),
            Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("█".repeat(filled), Style::default().fg(ACCENT)),
        Span::styled(
            "─".repeat(bar_width.saturating_sub(filled)),
            Style::default().fg(FAINT),
        ),
        Span::raw("  "),
        Span::styled(times, Style::default().fg(NORMAL)),
        Span::raw("  "),
        Span::styled(
            format!("{}/{}", current + 1, total),
            Style::default().fg(DIM),
        ),
        ]),
    };

    let mut second = vec![
        Span::styled(
            format!(" {:.2}× ", app.config.voice.speed),
            Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {} ", app.engine_name()),
            Style::default().fg(DIM),
        ),
        // L'avance de synthèse est visible : on sait si le moteur suit.
        Span::styled(
            format!(
                " ⟐ {} prêtes · {} en cours · {} sessions",
                stats.ready, stats.in_flight, stats.workers
            ),
            Style::default().fg(FAINT),
        ),
    ];
    if stats.failed > 0 {
        second.push(Span::styled(
            format!("  ⚠ {} échec(s)", stats.failed),
            Style::default().fg(ACCENT),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FAINT));

    frame.render_widget(
        Paragraph::new(vec![first, Line::from(second)]).block(block),
        area,
    );
}

/// Contenu de la ligne d'état.
///
/// Séparé du rendu : c'est la seule ligne qui parle à l'utilisateur en
/// permanence, et ce qu'elle dit se vérifie.
pub(super) fn status_spans(app: &App) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    // Une saisie en cours prend toute la ligne : ce qu'on tape doit se voir.
    if let Some(query) = app.search_input() {
        spans.push(Span::styled(
            " / ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {query}▏"),
            Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "  Entrée pour chercher · Échap pour annuler ",
            Style::default().fg(FAINT),
        ));
        return spans;
    }

    // Un repli sur la voix du système est un état dégradé : il doit se voir en
    // permanence, et dire comment en sortir. C'est la seule façon de rendre
    // l'écran de configuration découvrable sans avoir lu de documentation.
    let on_fallback = !models::available().is_empty() && models::first_installed().is_none();

    // La vue figée est un état, pas un événement : il survit au message qui
    // l'a annoncé, sans quoi on cherche pourquoi le texte ne défile plus.
    if !app.view.is_following() {
        spans.push(Span::styled(
            " ⏸ vue figée ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "f revient à la lecture · Pg↑ Pg↓ défilent ",
            Style::default().fg(NORMAL),
        ));
    } else if !app.status().is_empty() {
        spans.push(Span::styled(
            format!(" {} ", app.status()),
            Style::default().fg(ACCENT),
        ));
    } else if on_fallback {
        spans.push(Span::styled(
            " ⚠ voix système ",
            Style::default().fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" appuyez sur ", Style::default().fg(NORMAL)));
        spans.push(Span::styled(
            "s",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            " pour installer un vrai modèle ",
            Style::default().fg(NORMAL),
        ));
    } else if let Some(notice) = app.notices.last() {
        spans.push(Span::styled(format!(" {notice} "), Style::default().fg(DIM)));
    } else {
        // Le rappel permanent nomme la configuration en premier.
        spans.push(Span::styled(" ", Style::default()));
        spans.push(Span::styled(
            "s",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(" réglages · ", Style::default().fg(FAINT)));
        spans.push(Span::styled(
            "?",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(REMINDER, Style::default().fg(FAINT)));
    }

    // Où en est la recherche, tant qu'elle a des résultats à parcourir.
    if let Some(search) = app.search() {
        if !search.is_empty() {
            spans.push(Span::styled(
                format!("· « {} » {}/{} ", search.query, search.position(), search.count()),
                Style::default().fg(NORMAL),
            ));
        }
    }

    // Indicateur de ponctuation : quelle marque le lecteur est en train de dire.
    if let Some(punctuation) = current_punctuation(app) {
        spans.push(Span::styled(
            format!("· {} ", punctuation),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    }
    if app.skipped_count() > 0 {
        spans.push(Span::styled(
            format!("· {} phrase(s) sautée(s) ", app.skipped_count()),
            Style::default().fg(DIM),
        ));
    }
    if app.is_finished() {
        spans.push(Span::styled("· lecture terminée ", Style::default().fg(DIM)));
    }
    spans
}

pub(super) fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(status_spans(app))).alignment(Alignment::Left),
        area,
    );
}

/// Ponctuation portée par le mot en cours de lecture.
pub(super) fn current_punctuation(app: &App) -> Option<&'static str> {
    let index = app.current_word()?;
    let sentence = app.doc().sentence(app.current_sentence())?;
    sentence.words.get(index)?.punctuation.map(|p| p.label())
}

pub(super) fn render_panel(frame: &mut Frame, app: &App, area: Rect) {
    let (title, lines) = match app.panel {
        Panel::Contents => ("Sommaire", contents_lines(app)),
        Panel::None => return,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(FAINT))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    // Garde la ligne sélectionnée dans le cadre.
    let height = area.height.saturating_sub(2) as usize;
    let scroll = app.selection.saturating_sub(height.saturating_sub(1)) as u16;

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((scroll, 0)),
        area,
    );
}

pub(super) fn contents_lines(app: &App) -> Vec<Line<'static>> {
    let doc = app.doc();
    let current = doc
        .section_of_sentence(app.current_sentence())
        .map(|s| s.id);

    // Même liste que celle qu'interroge la sélection : les index concordent.
    app.navigable_sections()
        .into_iter()
        .filter_map(|id| doc.section(id))
        .enumerate()
        .map(|(i, section)| {
            let indent = "  ".repeat(section.level.saturating_sub(1) as usize);
            let selected = i == app.selection;
            let active = current == Some(section.id);
            let style = match (selected, active) {
                (true, _) => Style::default().bg(ACCENT).fg(Color::Black),
                (false, true) => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(NORMAL),
            };
            Line::from(Span::styled(
                format!("{indent}{}", truncate(&section.title, 30)),
                style,
            ))
        })
        .collect()
}

pub(super) fn render_help(
    frame: &mut Frame,
    area: Rect,
    entries: &[(&str, &str)],
    title: &str,
) {
    // La boîte se règle sur son contenu : une largeur fixe rognait les
    // descriptions les plus longues, en plein milieu d'un mot.
    let needed = entries
        .iter()
        .map(|(keys, description)| keys.chars().count().max(12) + description.chars().count() + 4)
        .max()
        .unwrap_or(40) as u16;
    let width = needed.max(34).min(area.width.saturating_sub(4));
    let height = (entries.len() as u16 + 4).min(area.height.saturating_sub(2));
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let lines: Vec<Line> = entries
        .iter()
        .map(|(keys, description)| {
            Line::from(vec![
                Span::styled(
                    format!(" {keys:<12}"),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(*description, Style::default().fg(NORMAL)),
            ])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

pub(super) fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Formate des secondes en `M:SS` ou `H:MM:SS`.
pub(super) fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

// ------------------------------------------------------- écran de configuration
