//! La page des réglages : six lignes, l'explication de la ligne choisie, le
//! rappel des touches.

use super::*;

/// Colonne des libellés : le plus long, « Largeur de colonne », et de l'air.
const LABEL_WIDTH: usize = 22;

/// Dessine la page des réglages, qui remplace la vue de lecture.
pub(super) fn render_config(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // titre
            Constraint::Min(8),    // réglages et explication
            Constraint::Length(2), // touches et messages
        ])
        .split(area);
    frame.render_widget(Paragraph::new(config_title(app)), rows[0]);
    frame.render_widget(Paragraph::new(config_lines(app, rows[1].width as usize)), rows[1]);
    frame.render_widget(Paragraph::new(config_footer(app)), rows[2]);
}

fn config_title(app: &App) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Réglages", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Span::styled(format!("   moteur : {}", app.engine_name()), Style::default().fg(DIM)),
        ]),
    ]
}

/// Les six réglages, puis l'explication de celui qui est choisi.
pub(super) fn config_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let current = app.screen.current_field();
    let mut lines = vec![Line::from("")];
    for field in FIELDS {
        let selected = field == current;
        let (label_style, value_style) = if selected {
            (
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD),
            )
        } else {
            (Style::default().fg(NORMAL), Style::default().fg(BRIGHT))
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {} {:<LABEL_WIDTH$}", if selected { "▸" } else { " " }, field.label()),
                label_style,
            ),
            Span::styled(field.value(&app.config), value_style),
        ]));
    }
    lines.push(Line::from(""));
    for line in fold(current.hint(&app.config), width.saturating_sub(6)) {
        lines.push(Line::from(Span::styled(format!("    {line}"), Style::default().fg(DIM))));
    }
    lines
}

/// Le rappel des touches, puis le message du moment.
pub(super) fn config_footer(app: &App) -> Vec<Line<'static>> {
    // Sans document derrière, fermer la page quitte : autant le dire.
    let exit = if app.doc().is_empty() { "q quitter" } else { "Échap fermer" };
    let mut lines = vec![Line::from(Span::styled(
        format!(" ↑↓ réglage · ←→ valeur, enregistrée aussitôt · t écouter · {exit}"),
        Style::default().fg(FAINT),
    ))];
    // L'aperçu passe devant : c'est le seul message qui décrit une action en
    // train de se produire.
    let message = app
        .preview_status()
        .or_else(|| (!app.screen.notice.is_empty()).then(|| app.screen.notice.clone()))
        .or_else(|| (!app.status().is_empty()).then(|| app.status().to_string()));
    if let Some(message) = message {
        lines.push(Line::from(Span::styled(format!(" {message}"), Style::default().fg(ACCENT))));
    }
    lines
}

/// Plie un texte à la largeur donnée, sur les espaces.
pub(super) fn fold(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let cost = word.chars().count() + usize::from(!current.is_empty());
        if current.chars().count() + cost > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        } else if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
