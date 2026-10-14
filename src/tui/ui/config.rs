//! L'écran de configuration : menu, rubriques, liste des modèles.

use super::*;

/// Dessine l'écran de configuration, qui remplace la vue de lecture.
///
/// Deux pages, jamais côte à côte : le menu, puis la rubrique ouverte. On sait
/// toujours où l'on est, et les flèches n'ont qu'un sens à la fois.
pub(super) fn render_config(frame: &mut Frame, app: &App, area: Rect) {
    match app.screen.page {
        Page::Menu => render_config_menu(frame, app, area),
        Page::Detail => render_config_detail(frame, app, area),
    }
}

/// La liste des rubriques, seule à l'écran.
pub(super) fn render_config_menu(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // titre
            Constraint::Min(6),    // rubriques
            Constraint::Length(2), // aide et messages
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  Configuration",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("   moteur : {}", app.engine_name()),
                    Style::default().fg(DIM),
                ),
            ]),
        ]),
        rows[0],
    );

    // Une colonne de valeurs alignée sur le libellé le plus long : l'œil
    // descend la liste sans avoir à chercher où commence chaque état.
    let label_width = Section::ALL
        .iter()
        .map(|s| s.label().chars().count())
        .max()
        .unwrap_or(16)
        + 4;

    let mut lines: Vec<Line> = vec![Line::from("")];
    for (i, section) in Section::ALL.iter().enumerate() {
        let selected = i == app.screen.section;
        let style = if selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(NORMAL)
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("   {} ", if selected { "▸" } else { " " }),
                style,
            ),
            Span::styled(
                format!("{} ", i + 1),
                Style::default()
                    .fg(if selected { ACCENT } else { FAINT })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {:<label_width$}", section.label()), style),
            Span::styled(
                section.summary(&app.config),
                Style::default().fg(if selected { NORMAL } else { FAINT }),
            ),
        ]));
        lines.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(lines), rows[1]);
    render_config_footer(frame, app, rows[2]);
}

/// Une rubrique ouverte, en pleine largeur.
pub(super) fn render_config_detail(frame: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // fil d'Ariane
            Constraint::Min(4),    // contenu
            Constraint::Length(2), // aide et messages
        ])
        .split(area);

    let section = app.screen.current_section();
    // Le chevron rappelle d'où l'on vient, et que Échap y ramène.
    let mut head = vec![
        Span::styled("  ‹ ", Style::default().fg(FAINT)),
        Span::styled(
            format!("{} {}", app.screen.section + 1, section.label()),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
    ];
    if area.width >= 60 {
        head.push(Span::styled(
            format!("   moteur : {}", app.engine_name()),
            Style::default().fg(DIM),
        ));
    }
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(head),
            Line::from(Span::styled(
                format!("  {}", "─".repeat((area.width as usize).saturating_sub(4))),
                Style::default().fg(FAINT),
            )),
        ]),
        rows[0],
    );

    match section {
        Section::Models => render_model_list(frame, app, rows[1]),
        other => render_config_fields(frame, app, other, rows[1]),
    }
    render_config_footer(frame, app, rows[2]);
}

pub(super) fn render_config_fields(frame: &mut Frame, app: &App, _section: Section, area: Rect) {
    let volume = app.player.volume();
    let mut lines: Vec<Line> = Vec::new();

    let fields = app.screen.visible_fields(&app.config);
    if fields.is_empty() {
        lines.push(Line::from(Span::styled(
            "    Aucun réglage de cette rubrique n'agit sur le modèle chargé.",
            Style::default().fg(NORMAL),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "    a  pour les afficher quand même",
            Style::default().fg(FAINT),
        )));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    // Une rubrique plus haute que la fenêtre défile autour du réglage courant,
    // au lieu d'être tronquée sans que rien ne le signale.
    let reserved = 5; // ligne vide, mention des réglages masqués, explication
    let room = (area.height as usize).saturating_sub(reserved).max(3);
    let start = if fields.len() <= room {
        0
    } else {
        app.screen
            .row
            .saturating_sub(room / 2)
            .min(fields.len() - room)
    };
    let end = (start + room).min(fields.len());
    if start > 0 {
        lines.push(Line::from(Span::styled(
            format!("    ↑ {start} de plus"),
            Style::default().fg(FAINT),
        )));
    }

    for (i, field) in fields.iter().enumerate().take(end).skip(start) {
        let selected = i == app.screen.row;
        let marker = if selected { "▸" } else { " " };
        let label_style = if selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(NORMAL)
        };

        // Un champ en cours de saisie montre son tampon et un curseur.
        let value = match (&app.screen.editing, selected) {
            (Some(buffer), true) => format!("{buffer}▏"),
            _ => field.value(&app.config, volume),
        };
        let value_style = if app.screen.editing.is_some() && selected {
            Style::default().bg(ACCENT).fg(Color::Black)
        } else if selected {
            Style::default().fg(BRIGHT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(BRIGHT)
        };

        // Un réglage que la famille chargée ignore est marqué comme tel,
        // plutôt que de laisser croire qu'il agit.
        let ignored = field.is_ignored(&app.config);

        let mut spans = vec![
            Span::styled(format!("  {marker} {:<24}", field.label()), label_style),
            Span::styled(value, value_style),
        ];
        if ignored {
            spans.push(Span::styled(
                "  (ignoré par ce modèle)",
                Style::default().fg(FAINT),
            ));
        }
        lines.push(Line::from(spans));
    }

    if end < fields.len() {
        lines.push(Line::from(Span::styled(
            format!("    ↓ {} de plus", fields.len() - end),
            Style::default().fg(FAINT),
        )));
    }

    // Ce qui a été retiré de la vue doit rester récupérable d'un geste.
    let hidden = app.screen.hidden_count(&app.config);
    if hidden > 0 {
        lines.push(Line::from(""));
        let pluriel = if hidden > 1 { "s" } else { "" };
        // Sur une colonne étroite, la phrase complète serait coupée en plein
        // milieu : elle se réduit alors à ce qui compte.
        let message = if area.width >= 52 {
            format!("    {hidden} réglage{pluriel} sans effet sur ce modèle · a pour les voir")
        } else {
            format!("    {hidden} sans effet · a")
        };
        lines.push(Line::from(Span::styled(message, Style::default().fg(FAINT))));
    }

    lines.push(Line::from(""));

    if let Some(field) = fields.get(app.screen.row) {
        // L'explication du champ sélectionné, plutôt qu'un manuel séparé.
        let width = (area.width as usize).saturating_sub(6);
        for line in fold(&field.hint_for(&app.config), width) {
            lines.push(Line::from(Span::styled(
                format!("    {line}"),
                Style::default().fg(DIM),
            )));
        }
        if field.is_text(&app.config) && app.screen.editing.is_none() {
            lines.push(Line::from(Span::styled(
                "    Entrée pour éditer",
                Style::default().fg(FAINT),
            )));
        }
    }

    // Pas de coupe automatique : elle rognerait l'indentation des colonnes, et
    // les explications sont déjà pliées à la bonne largeur.
    frame.render_widget(Paragraph::new(lines), area);
}

/// Coupe un texte trop long, en signalant la coupe.
pub(super) fn ellipsis(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }
    text.chars().take(width - 1).collect::<String>() + "…"
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

pub(super) fn render_model_list(frame: &mut Frame, app: &App, area: Rect) {
    let packages = models::available();
    let mut lines: Vec<Line> = Vec::new();

    if packages.is_empty() {
        lines.push(Line::from(Span::styled(
            " Aucun modèle disponible.",
            Style::default().fg(NORMAL).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Ce binaire a été compilé sans Kokoro ; la lecture",
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(Span::styled(
            " utilise la voix du système. Pour un vrai modèle :",
            Style::default().fg(DIM),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "   cargo build --release --features kokoro-onnx",
            Style::default().fg(ACCENT),
        )));
        // `trim` rognerait l'indentation : la valeur du champ sélectionné se
    // décalerait de deux colonnes par rapport aux autres.
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
        return;
    }

    let active = app.config.engine.model_path.as_str();
    // La colonne des noms se règle sur le plus long, sans jamais écraser les
    // badges : un nom trop long est coupé plutôt que de les repousser.
    let longest = packages
        .iter()
        .map(|p| p.display_name.chars().count())
        .max()
        .unwrap_or(20);
    let name_width = longest.min((area.width as usize).saturating_sub(26)).max(12);

    for (i, package) in packages.iter().enumerate() {
        let selected = i == app.screen.row;
        let installed = models::is_installed(package);
        let truncated = models::is_truncated(package);
        let is_active = models::install_path(package)
            .is_some_and(|p| p.to_string_lossy() == active);

        let (badge, badge_style) = if truncated {
            ("⚠ tronqué ", Style::default().fg(Color::Black).bg(ACCENT))
        } else if is_active {
            ("● utilisé ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        } else if installed {
            ("○ installé", Style::default().fg(NORMAL))
        } else {
            ("  à venir ", Style::default().fg(FAINT))
        };

        let name_style = if selected {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(BRIGHT)
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "  {} {:<width$}",
                    if selected { "▸" } else { " " },
                    ellipsis(package.display_name, name_width),
                    width = name_width + 1
                ),
                name_style,
            ),
            Span::styled(badge, badge_style),
            Span::styled(
                format!("  {}", models::human_size(package.total_bytes())),
                Style::default().fg(DIM),
            ),
        ]));

        if selected {
            lines.push(Line::from(Span::styled(
                format!("      {}", package.description),
                Style::default().fg(DIM),
            )));
            if truncated {
                lines.push(Line::from(Span::styled(
                    format!(
                        "     Paquet incomplet : {} sur {}. Entrée pour réinstaller.",
                        models::human_size(models::installed_bytes(package)),
                        models::human_size(package.total_bytes())
                    ),
                    Style::default().fg(ACCENT),
                )));
            }
            if package.recommended && !installed {
                lines.push(Line::from(Span::styled(
                    "      Recommandé pour commencer.",
                    Style::default().fg(ACCENT),
                )));
            }
            lines.push(Line::from(Span::styled(
                format!("      {}", package.languages_label),
                Style::default().fg(FAINT),
            )));
        }
    }

    // Barre de progression du téléchargement en cours.
    if let Some(download) = app.screen.download.as_ref() {
        if let models::Progress::Running { received, total } = download.progress() {
            let width = (area.width as usize).saturating_sub(8).min(48);
            let ratio = if total > 0 {
                (received as f64 / total as f64).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let filled = (width as f64 * ratio).round() as usize;
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("█".repeat(filled), Style::default().fg(ACCENT)),
                Span::styled(
                    "─".repeat(width.saturating_sub(filled)),
                    Style::default().fg(FAINT),
                ),
                Span::styled(
                    format!(
                        "  {} / {}",
                        models::human_size(received),
                        models::human_size(total)
                    ),
                    Style::default().fg(NORMAL),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!("  {} — d pour interrompre", download.package.display_name),
                Style::default().fg(DIM),
            )));
        }
    } else if let Some(free) = models::free_space() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Espace disque libre : {}", models::human_size(free)),
            Style::default().fg(FAINT),
        )));
    }

    // `trim` rognerait l'indentation : la valeur du champ sélectionné se
    // décalerait de deux colonnes par rapport aux autres.
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

pub(super) fn render_config_footer(frame: &mut Frame, app: &App, area: Rect) {
    // Sans document ouvert, « Échap retour » n'aurait nulle part où revenir.
    let exit = if app.doc().is_empty() { "q quitter" } else { "Échap retour" };
    // Le rappel ne montre que ce qui agit à cet instant : dans la colonne des
    // rubriques, « ←→ valeur » n'aurait rien à modifier.
    let keys = if app.screen.editing.is_some() {
        "Entrée valider · Échap annuler".to_string()
    } else if app.screen.page == Page::Menu {
        format!("↑↓ choisir · Entrée ouvrir · 1…4 accès direct · t écouter · {exit}")
    } else if app.screen.current_section() == Section::Models {
        "↑↓ modèle · Entrée installer/choisir · t écouter · d d supprimer · Échap menu".to_string()
    } else {
        "↑↓ réglage · ←→ valeur · Entrée éditer · a tout voir · w enregistrer · Échap menu"
            .to_string()
    };

    let mut lines = vec![Line::from(Span::styled(
        format!(" {keys}"),
        Style::default().fg(FAINT),
    ))];

    // L'aperçu passe devant : c'est le seul message qui décrit une action en
    // train de se produire, et il dure le temps qu'on l'écoute.
    let message = if let Some(preview) = app.preview_status() {
        preview
    } else if !app.screen.notice.is_empty() {
        app.screen.notice.clone()
    } else if !app.status().is_empty() {
        app.status().to_string()
    } else {
        String::new()
    };
    if !message.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" {message}"),
            Style::default().fg(ACCENT),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}
