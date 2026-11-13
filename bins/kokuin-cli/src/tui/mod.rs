//! `tui`: a boot in the terminal, to move around in. See `app` for the state and
//! keys, `view` for the drawing.

pub mod app;
pub mod view;

use std::{
    io::{self, IsTerminal},
    time::Duration,
};

use kokuin_kb::Kb;
use ratatui::{
    DefaultTerminal,
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseEventKind,
        },
        execute,
    },
};

use self::{
    app::{Action, App, Effect, PickerItem},
    view::Theme,
};
use crate::{Failure, boots, records};

pub fn run(boot: Option<String>) -> Result<(), Failure> {
    if !io::stdout().is_terminal() {
        return Err(Failure(
            "the TUI needs a terminal: use `kokuin-cli show` or `kokuin-cli timeline` instead"
                .into(),
        ));
    }
    let record = records::load(boot.as_deref())?;
    let mut app = App::new(record, origin(boot.as_deref()));
    let theme = Theme {
        colors: colors_wanted(),
    };

    let mut terminal = ratatui::try_init()?;
    let _ = execute!(io::stdout(), EnableMouseCapture);
    let result = event_loop(&mut terminal, &mut app, &theme);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App, theme: &Theme) -> Result<(), Failure> {
    let kb = Kb::embedded();
    while !app.quit {
        terminal.draw(|frame| view::draw(frame, app, kb, theme))?;
        // Half the screen, for page moves: about the height of the event list.
        app.page = (terminal.size()?.height as usize / 2).max(1);
        if !event::poll(Duration::from_millis(500))? {
            continue;
        }
        let action = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.action_for(key),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => Some(Action::Up),
                MouseEventKind::ScrollDown => Some(Action::Down),
                _ => None,
            },
            _ => None,
        };
        let Some(action) = action else { continue };
        match app.handle(action, kb) {
            Effect::None => {}
            Effect::ListBoots => app.open_picker(picker_items()),
            Effect::Load(selector) => {
                // A boot that cannot be loaded leaves the current one shown.
                if let Ok(record) = records::load(selector.as_deref()) {
                    *app = App::new(record, origin(selector.as_deref()));
                }
            }
        }
    }
    Ok(())
}

/// What the header says the boot shown is.
fn origin(boot: Option<&str>) -> String {
    match boot {
        None => "this boot".into(),
        Some(selector) => format!("from {selector}"),
    }
}

/// This boot, rebuilt now, then the recorded boots, the last one first.
fn picker_items() -> Vec<PickerItem> {
    let mut items = vec![PickerItem {
        label: "This boot, as the system remembers it now".into(),
        selector: None,
    }];
    let current = boots::current_boot_id().ok();
    for boot in boots::list().into_iter().rev() {
        let record = &boot.record;
        let id: String = record.boot.boot_id.chars().take(8).collect();
        let ready = record
            .milestones
            .iter()
            .find(|m| m.kind == kokuin_model::MilestoneKind::SessionReady)
            .map(|m| {
                format!(
                    "desktop ready {}",
                    crate::output::duration(m.t_ns.max(0) as u64)
                )
            })
            .unwrap_or_default();
        let this = if current.as_deref() == Some(record.boot.boot_id.as_str()) {
            " (this boot)"
        } else {
            ""
        };
        items.push(PickerItem {
            label: format!(
                "{id}  {}  {ready}{this}",
                record.boot.started_at.as_deref().unwrap_or("?")
            ),
            selector: Some(boot.dir.display().to_string()),
        });
    }
    items
}

/// Colors unless `--color never` or `NO_COLOR` says otherwise.
fn colors_wanted() -> bool {
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    anstream::ColorChoice::global() != anstream::ColorChoice::Never && !no_color
}
