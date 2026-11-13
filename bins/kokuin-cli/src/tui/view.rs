//! Draws the TUI from its state: header, stage bar, event list, details, and the
//! window open over them.

use kokuin_kb::Kb;
use kokuin_model::{BootRecord, Event, MilestoneKind, Stage};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
};

use super::app::{App, Overlay};
use crate::{
    markdown::{self, Emphasis, LineKind},
    output::{self, confidence_mark, duration, milestone_label, time},
    records,
};

/// Below this, the screen is too small to show anything useful.
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 15;
/// Below this width, the details pane is left out.
const DETAILS_MIN_WIDTH: u16 = 100;
/// Width of the stage names in the stage bar.
const STAGE_LABEL: usize = 12;
/// Width of the times after the stage bars.
const STAGE_TIMING: usize = 22;

/// How to draw.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Whether to use colors (`--color never` and `NO_COLOR` turn them off).
    pub colors: bool,
}

impl Theme {
    fn color(&self, color: Color) -> Style {
        if self.colors {
            Style::default().fg(color)
        } else {
            Style::default()
        }
    }

    fn stage(&self, stage: Stage) -> Style {
        self.color(ansi(output::stage_color(stage)))
    }

    fn dim(&self) -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }

    fn bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    fn selected(&self) -> Style {
        Style::default().add_modifier(Modifier::REVERSED)
    }
}

/// ratatui's name for a terminal color.
fn ansi(color: anstyle::AnsiColor) -> Color {
    use anstyle::AnsiColor as A;
    match color {
        A::Black => Color::Black,
        A::Red => Color::Red,
        A::Green => Color::Green,
        A::Yellow => Color::Yellow,
        A::Blue => Color::Blue,
        A::Magenta => Color::Magenta,
        A::Cyan => Color::Cyan,
        A::White => Color::Gray,
        A::BrightBlack => Color::DarkGray,
        A::BrightRed => Color::LightRed,
        A::BrightGreen => Color::LightGreen,
        A::BrightYellow => Color::LightYellow,
        A::BrightBlue => Color::LightBlue,
        A::BrightMagenta => Color::LightMagenta,
        A::BrightCyan => Color::LightCyan,
        A::BrightWhite => Color::White,
    }
}

/// Draws everything.
pub fn draw(frame: &mut Frame, app: &App, kb: &Kb, theme: &Theme) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let message = format!(
            "The terminal is too small ({}×{}): kokuin needs at least {MIN_WIDTH}×{MIN_HEIGHT}.",
            area.width, area.height
        );
        frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: true }), area);
        return;
    }
    let stage_rows = app.record.stages.len() as u16 + 1;
    let [header, stages, body, footer] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(stage_rows + 2),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_header(frame, header, app, theme);
    draw_stages(frame, stages, app, theme);
    if body.width >= DETAILS_MIN_WIDTH {
        let [list, details] =
            Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
                .areas(body);
        draw_events(frame, list, app, theme);
        draw_details(frame, details, app, kb, theme);
    } else {
        draw_events(frame, body, app, theme);
    }
    draw_footer(frame, footer, app, theme);

    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_help(frame, area, theme),
        Overlay::Article {
            id,
            scroll,
            related,
            ..
        } => draw_article(frame, area, kb, id, *scroll, *related, theme),
        Overlay::Picker { items, selected } => {
            let lines = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let style = if i == *selected {
                        theme.selected()
                    } else {
                        Style::default()
                    };
                    Line::styled(format!(" {} ", item.label), style)
                })
                .collect();
            draw_window(
                frame,
                area,
                " Boots: Enter to show, Esc to close ",
                lines,
                0,
            );
        }
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let record = &app.record;
    let boot = &record.boot;
    let os = boot
        .os
        .as_ref()
        .and_then(|os| os.pretty_name.clone().or(os.name.clone()))
        .unwrap_or_else(|| "unknown distribution".into());
    let started = boot.started_at.as_deref().unwrap_or(&boot.boot_id);
    let ready = record
        .milestones
        .iter()
        .find(|m| m.kind == MilestoneKind::SessionReady)
        .map(|m| format!("   desktop ready {}", duration(m.t_ns.max(0) as u64)))
        .unwrap_or_default();
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("Boot of {started}"), theme.bold()),
            Span::raw(format!(" · {os} · Linux {}", boot.kernel.release)),
        ]),
        Line::from(vec![
            Span::raw(records::boot_chain(record).join(" → ")),
            Span::styled(ready, theme.bold()),
        ]),
        Line::styled(
            format!("{} · {}", records::capture_wording(record), app.origin),
            theme.dim(),
        ),
    ];
    let block = Block::bordered().title(" kokuin ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// The time span the stage bar covers.
fn time_span(record: &BootRecord) -> Option<(i64, i64)> {
    let times = record
        .stages
        .iter()
        .flat_map(|s| [s.start_ns, s.end_ns])
        .chain(record.milestones.iter().map(|m| Some(m.t_ns)))
        .flatten();
    Some((times.clone().min()?, times.max()?))
}

fn draw_stages(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = Block::bordered().title(" Stages: ←/→ to show one, Esc for all ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let record = &app.record;
    let Some((min, max)) = time_span(record) else {
        frame.render_widget(Paragraph::new("No stage could be timed."), inner);
        return;
    };
    let width = (inner.width as usize)
        .saturating_sub(STAGE_LABEL + STAGE_TIMING + 2)
        .max(10);
    let span = (max - min).max(1) as f64;
    let column = |t: i64| (((t - min) as f64 / span) * width as f64).round() as usize;

    let mut lines = Vec::new();
    for stage in &record.stages {
        let selected = app.stage == Some(stage.stage);
        let label = format!(
            "{}{:<width$}",
            if selected { "▶" } else { " " },
            output::stage(stage.stage),
            width = STAGE_LABEL - 1
        );
        let label_style = if selected {
            theme.selected()
        } else {
            Style::default()
        };
        let (bar, timing) = match stage.start_ns {
            Some(start) => {
                let end = stage.end_ns.unwrap_or(max);
                let from = column(start).min(width - 1);
                let to = column(end).clamp(from + 1, width);
                let bar = format!(
                    "{}{}{}",
                    " ".repeat(from),
                    "█".repeat(to - from),
                    " ".repeat(width - to)
                );
                let length = stage.end_ns.map_or_else(
                    || "…".to_owned(),
                    |end| duration((end - start).max(0) as u64),
                );
                let timing = format!(
                    "{:>11}{} {:>9}",
                    time(start),
                    confidence_mark(stage.confidence),
                    length
                );
                (bar, timing)
            }
            None => (" ".repeat(width), format!("{:>21}", "not measured")),
        };
        lines.push(Line::from(vec![
            Span::styled(label, label_style),
            Span::styled(bar, theme.stage(stage.stage)),
            Span::raw(" "),
            Span::styled(timing, theme.dim()),
        ]));
    }
    // The milestones on the same axis.
    let mut marks = vec![' '; width];
    for milestone in &record.milestones {
        marks[column(milestone.t_ns).min(width - 1)] = '▲';
    }
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {:<width$}", "milestones", width = STAGE_LABEL - 1),
            theme.dim(),
        ),
        Span::styled(marks.into_iter().collect::<String>(), theme.bold()),
    ]));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_events(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let total = app.record.events.len();
    let mut title = format!(" Events: {} of {total}", app.visible.len());
    if let Some(stage) = app.stage {
        title.push_str(&format!(" · {}", output::stage(stage)));
    }
    if let Some(grep) = &app.filter.grep {
        title.push_str(&format!(" · /{grep}"));
    }
    if app.filter.explained_only {
        title.push_str(" · explained only");
    }
    title.push(' ');
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let height = inner.height as usize;
    if app.visible.is_empty() {
        let message = "No event passes the filters. Esc clears them, x shows all events.";
        frame.render_widget(Paragraph::new(message).style(theme.dim()), inner);
        return;
    }
    // Keep the selection in the middle of the list, once past the first page.
    let last_start = app.visible.len().saturating_sub(height);
    let start = app.selected.saturating_sub(height / 2).min(last_start);
    let lines: Vec<Line> = app.visible[start..]
        .iter()
        .take(height)
        .enumerate()
        .map(|(row, &index)| {
            let event = &app.record.events[index];
            let line = event_line(event, theme);
            if start + row == app.selected {
                line.style(theme.selected())
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn event_line<'a>(event: &'a Event, theme: &Theme) -> Line<'a> {
    let when = match event.t_ns {
        Some(t) => format!("{:>10}{}", time(t), confidence_mark(event.confidence)),
        None => format!("{:>10}{}", "·", confidence_mark(event.confidence)),
    };
    let mut spans = vec![
        Span::raw(when),
        Span::styled(" ▌", theme.stage(event.stage)),
        Span::styled(format!("{:<12} ", event.source), theme.dim()),
        Span::raw(event.title.as_str()),
    ];
    if let Some(length) = event.dur_ns {
        spans.push(Span::styled(format!("  {}", duration(length)), theme.dim()));
    }
    Line::from(spans)
}

fn draw_details(frame: &mut Frame, area: Rect, app: &App, kb: &Kb, theme: &Theme) {
    let block = Block::bordered().title(" Details ");
    let Some(event) = app.selected_event() else {
        frame.render_widget(block, area);
        return;
    };
    let record = &app.record;
    let mut lines = vec![
        Line::styled(event.title.clone(), theme.bold()),
        Line::raw(""),
    ];
    let mut facts = Vec::new();
    if let Some(t) = event.t_ns {
        facts.push(format!(
            "at {}{}",
            time(t),
            confidence_mark(event.confidence)
        ));
    }
    if let Some(length) = event.dur_ns {
        facts.push(format!("took {}", duration(length)));
    }
    facts.push(format!("{} stage", output::stage(event.stage)));
    facts.push(format!("from {}", event.source));
    lines.push(Line::raw(facts.join(" · ")));
    if let Some(message) = event.message.as_deref().filter(|m| *m != event.title) {
        lines.push(Line::raw(""));
        lines.push(Line::styled(message.to_owned(), theme.dim()));
    }

    if let Some(process) = event
        .process
        .and_then(|id| record.processes.iter().find(|p| p.id == id))
    {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Process", theme.bold()));
        lines.push(Line::raw(format!("{} (PID {})", process.comm, process.pid)));
        if !process.cmdline.is_empty() {
            lines.push(Line::styled(process.cmdline.join(" "), theme.dim()));
        }
        if let Some(parent) = process
            .parent
            .and_then(|id| record.processes.iter().find(|p| p.id == id))
        {
            lines.push(Line::raw(format!(
                "started by {} (PID {})",
                parent.comm, parent.pid
            )));
        }
        if let Some(service) = &process.service {
            lines.push(Line::raw(format!("service {service}")));
        }
    }

    if !event.data.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Data", theme.bold()));
        for (key, value) in &event.data {
            let value = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            lines.push(Line::from(vec![
                Span::styled(format!("{key}: "), theme.dim()),
                Span::raw(value),
            ]));
        }
    }

    if let Some(article) = app.explanation_id().and_then(|id| kb.article(&id)) {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Why: {}", article.title),
            theme.bold(),
        ));
        lines.push(Line::raw(article.summary.clone()));
        lines.push(Line::styled("Enter: read the explanation", theme.dim()));
    }
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let line = match &app.typing {
        Some(text) => {
            let mut spans = vec![Span::raw(format!(" /{text}▏"))];
            if app.search_error {
                spans.push(Span::styled("  not a valid pattern yet", theme.dim()));
            }
            spans.push(Span::styled("  Enter: keep  Esc: cancel", theme.dim()));
            Line::from(spans)
        }
        None => Line::styled(
            " ↑↓ move  ←→ stage  / search  x explained only  [ ] milestones  Enter explain  b boots  ? help  q quit",
            theme.dim(),
        ),
    };
    frame.render_widget(Paragraph::new(line), area);
}

/// A window in the middle of the screen, over the rest.
fn draw_window(frame: &mut Frame, area: Rect, title: &str, lines: Vec<Line>, scroll: usize) {
    let [_, middle, _] = Layout::vertical([
        Constraint::Percentage(8),
        Constraint::Percentage(84),
        Constraint::Percentage(8),
    ])
    .areas(area);
    let [_, window, _] = Layout::horizontal([
        Constraint::Percentage(10),
        Constraint::Percentage(80),
        Constraint::Percentage(10),
    ])
    .areas(middle);
    frame.render_widget(Clear, window);
    let scroll = scroll.min(lines.len().saturating_sub(1)) as u16;
    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(title.to_owned()))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(paragraph, window);
}

fn draw_article(
    frame: &mut Frame,
    area: Rect,
    kb: &Kb,
    id: &str,
    scroll: usize,
    related: Option<usize>,
    theme: &Theme,
) {
    let Some(article) = kb.article(id) else {
        draw_window(
            frame,
            area,
            " Explanation ",
            vec![Line::raw(format!("No article {id}"))],
            0,
        );
        return;
    };
    let mut lines = vec![
        Line::styled(
            article.title.clone(),
            theme.bold().add_modifier(Modifier::UNDERLINED),
        ),
        Line::styled(article.summary.clone(), theme.dim()),
        Line::raw(""),
    ];
    for line in markdown::parse(&article.body) {
        lines.push(match line.kind {
            LineKind::Heading => Line::styled(
                line.spans
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<String>(),
                theme.bold(),
            ),
            LineKind::Code => Line::styled(
                format!(
                    "    {}",
                    line.spans
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<String>()
                ),
                theme.color(Color::Cyan),
            ),
            LineKind::Text => Line::from(
                line.spans
                    .into_iter()
                    .map(|span| match span.emphasis {
                        Emphasis::Plain => Span::raw(span.text),
                        Emphasis::Code => Span::styled(span.text, theme.color(Color::Cyan)),
                        Emphasis::Bold => Span::styled(span.text, theme.bold()),
                    })
                    .collect::<Vec<_>>(),
            ),
        });
    }
    if !article.related.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Related (Tab to choose, Enter to open)",
            theme.bold(),
        ));
        for (i, other) in article.related.iter().enumerate() {
            let title = kb.article(other).map_or("", |a| a.title.as_str());
            let style = if related == Some(i) {
                theme.selected()
            } else {
                Style::default()
            };
            lines.push(Line::styled(format!("  {title} ({other})"), style));
        }
    }
    if !article.references.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("Learn more", theme.bold()));
        for reference in &article.references {
            lines.push(Line::from(vec![
                Span::raw(format!("  {}: ", reference.title)),
                Span::styled(reference.url.clone(), theme.dim()),
            ]));
        }
    }
    let title = format!(" {id}: ↑↓ scroll, Esc back ");
    draw_window(frame, area, &title, lines, scroll);
}

fn draw_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let keys = [
        ("↑ ↓  j k", "move in the list (PgUp PgDn, g G for the ends)"),
        ("← →  h l", "show one stage only; past the ends, all again"),
        ("/", "search titles and messages (a regular expression)"),
        ("x", "only events that have an explanation or a duration"),
        ("[ ]", "previous / next milestone"),
        ("Enter  e", "read the explanation of the selected event"),
        (
            "Tab",
            "in an explanation: choose a related one, Enter opens it",
        ),
        ("b", "pick another recorded boot"),
        ("Esc", "close, or clear the stage, then the search"),
        ("q", "quit"),
    ];
    let mut lines = vec![Line::styled("Keys", theme.bold()), Line::raw("")];
    for (key, what) in keys {
        lines.push(Line::from(vec![
            Span::styled(format!("  {key:<12}"), theme.bold()),
            Span::raw(what),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Times count from the kernel start. ~ approximate, ? placed by order only.",
        theme.dim(),
    ));
    lines.push(Line::raw(""));
    lines.push(Line::raw(format!(
        "Milestones: {}.",
        [
            MilestoneKind::KernelStart,
            MilestoneKind::InitStart,
            MilestoneKind::DisplayManagerStart,
            MilestoneKind::SessionReady
        ]
        .map(milestone_label)
        .join(", ")
    )));
    draw_window(frame, area, " Help: Esc to close ", lines, 0);
}

#[cfg(test)]
pub(crate) mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::tui::app::{Action, PickerItem};

    /// A recorded boot from the test VMs.
    pub(crate) fn fixture() -> App {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/snapshots/ubuntu-26.04-qemu-deep"
        );
        App::new(records::load(Some(dir)).unwrap(), "test".into())
    }

    /// The screen as text, one string per row.
    pub(crate) fn render(app: &App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let theme = Theme { colors: false };
        terminal
            .draw(|frame| draw(frame, app, Kb::embedded(), &theme))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn draws_the_boot_its_stages_and_the_selected_event() {
        let mut app = fixture();
        let kb = Kb::embedded();
        app.handle(Action::NextStage, kb);
        app.handle(Action::NextStage, kb);
        let screen = render(&app, 140, 45).join("\n");
        assert!(screen.contains("Boot of 2026-11-13"));
        assert!(screen.contains("GNU GRUB (started by shim)"));
        assert!(screen.contains("desktop ready 13.06 s"));
        for stage in ["kernel", "initramfs", "services", "runtime"] {
            assert!(screen.contains(stage), "{stage}");
        }
        assert!(screen.contains("▶bootloader"));
        assert!(screen.contains("Events: 80 of 3137 · bootloader"));
        // The details of the first event of the stage, and why it happens.
        assert!(screen.contains("Why: Measured boot and the TPM event log"));
    }

    #[test]
    fn leaves_the_details_out_on_a_narrow_screen() {
        let screen = render(&fixture(), 90, 30).join("\n");
        assert!(screen.contains("Events:"));
        assert!(!screen.contains("Details"));
    }

    #[test]
    fn says_when_the_terminal_is_too_small() {
        let screen = render(&fixture(), 50, 10).join(" ");
        assert!(screen.contains("too small"));
    }

    #[test]
    fn draws_an_explanation_and_the_help() {
        let mut app = fixture();
        let kb = Kb::embedded();
        app.handle(Action::Open, kb);
        let screen = render(&app, 140, 45).join("\n");
        assert!(screen.contains("Esc back"));
        assert!(screen.contains("Related (Tab to choose, Enter to open)"));

        app.handle(Action::Back, kb);
        app.handle(Action::Help, kb);
        let screen = render(&app, 140, 45).join("\n");
        assert!(screen.contains("Help: Esc to close"));
        assert!(screen.contains("pick another recorded boot"));
    }

    #[test]
    fn draws_the_boot_picker() {
        let mut app = fixture();
        app.open_picker(vec![
            PickerItem {
                label: "This boot, as the system remembers it now".into(),
                selector: None,
            },
            PickerItem {
                label: "d8cdbed6  2026-11-13T08:28  desktop ready 13.06 s".into(),
                selector: Some("dir".into()),
            },
        ]);
        let screen = render(&app, 140, 45).join("\n");
        assert!(screen.contains("Boots: Enter to show"));
        assert!(screen.contains("d8cdbed6  2026-11-13T08:28"));
    }
}
