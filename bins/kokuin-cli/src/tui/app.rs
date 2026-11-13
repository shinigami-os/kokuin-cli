//! The state of the TUI, and what each key does to it. No terminal here: the
//! functions are tested as they are, and `view` draws whatever state they leave.

use kokuin_kb::Kb;
use kokuin_model::{BootRecord, Event, Stage};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::filter::Filter;

/// What the TUI shows.
pub struct App {
    /// The boot.
    pub record: BootRecord,
    /// Where it comes from, in words ("this boot", "boot 1351c542", a path).
    pub origin: String,
    /// The filters the user set (stage, search, explained only).
    pub filter: Filter,
    /// Indices in `record.events` of the events that pass the filter.
    pub visible: Vec<usize>,
    /// Position of the selected event in `visible`.
    pub selected: usize,
    /// The stage selected in the stage bar: the list shows only its events.
    pub stage: Option<Stage>,
    /// The search being typed, until Enter or Esc.
    pub typing: Option<String>,
    /// The search pattern is not a valid regular expression.
    pub search_error: bool,
    /// The window open over the main screen.
    pub overlay: Overlay,
    /// Rows of the event list on screen, for page moves.
    pub page: usize,
    /// The user asked to quit.
    pub quit: bool,
}

/// A window over the main screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    /// None.
    None,
    /// The key help.
    Help,
    /// An explanation. `history` holds the articles opened before, to go back.
    Article {
        /// The article shown.
        id: String,
        /// First line shown.
        scroll: usize,
        /// The related article selected, if any.
        related: Option<usize>,
        /// Articles to go back to.
        history: Vec<String>,
    },
    /// The boot picker.
    Picker {
        /// What can be picked.
        items: Vec<PickerItem>,
        /// The one selected.
        selected: usize,
    },
}

/// A boot the picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    /// What the user sees.
    pub label: String,
    /// What `records::load` takes to load it; `None` for this boot, rebuilt now.
    pub selector: Option<String>,
}

/// What a key asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    PreviousStage,
    NextStage,
    /// Esc: cancels the search being typed, closes what is open, or clears the
    /// stage, then the search.
    Back,
    StartSearch,
    Type(char),
    Erase,
    EndSearch,
    ToggleExplained,
    PreviousMilestone,
    NextMilestone,
    /// Enter: opens the explanation, or what is selected in an overlay.
    Open,
    Help,
    Boots,
    /// Tab in an explanation: the next related article.
    NextRelated,
    PreviousRelated,
    Quit,
}

/// What the TUI needs from the outside after an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Nothing.
    None,
    /// List the recorded boots, then call [`App::open_picker`].
    ListBoots,
    /// Load this boot, then show it.
    Load(Option<String>),
}

impl App {
    /// Shows a boot, with everything visible and the first event selected.
    pub fn new(record: BootRecord, origin: String) -> Self {
        let mut app = Self {
            record,
            origin,
            filter: Filter::default(),
            visible: Vec::new(),
            selected: 0,
            stage: None,
            typing: None,
            search_error: false,
            overlay: Overlay::None,
            page: 20,
            quit: false,
        };
        app.refilter();
        app
    }

    /// The selected event.
    pub fn selected_event(&self) -> Option<&Event> {
        self.visible
            .get(self.selected)
            .map(|&index| &self.record.events[index])
    }

    /// The stages of the stage bar, in boot order.
    pub fn stages(&self) -> Vec<Stage> {
        self.record.stages.iter().map(|s| s.stage).collect()
    }

    /// The explanation of the selected event, or else of its stage.
    pub fn explanation_id(&self) -> Option<String> {
        let event = self.selected_event()?;
        event.explain_id.clone().or_else(|| {
            self.record
                .stages
                .iter()
                .find(|s| s.stage == event.stage)
                .and_then(|s| s.explain_id.clone())
        })
    }

    /// The key's action, in the current mode.
    pub fn action_for(&self, key: KeyEvent) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Some(Action::Quit);
        }
        if self.typing.is_some() {
            return match key.code {
                KeyCode::Enter => Some(Action::EndSearch),
                KeyCode::Esc => Some(Action::Back),
                KeyCode::Backspace => Some(Action::Erase),
                KeyCode::Char(c) => Some(Action::Type(c)),
                _ => None,
            };
        }
        Some(match key.code {
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::PageUp => Action::PageUp,
            KeyCode::PageDown | KeyCode::Char(' ') => Action::PageDown,
            KeyCode::Home | KeyCode::Char('g') => Action::Top,
            KeyCode::End | KeyCode::Char('G') => Action::Bottom,
            KeyCode::Left | KeyCode::Char('h') => Action::PreviousStage,
            KeyCode::Right | KeyCode::Char('l') => Action::NextStage,
            KeyCode::Esc | KeyCode::Backspace => Action::Back,
            KeyCode::Char('/') => Action::StartSearch,
            KeyCode::Char('x') => Action::ToggleExplained,
            KeyCode::Char('[') => Action::PreviousMilestone,
            KeyCode::Char(']') => Action::NextMilestone,
            KeyCode::Enter | KeyCode::Char('e') => Action::Open,
            KeyCode::Tab => Action::NextRelated,
            KeyCode::BackTab => Action::PreviousRelated,
            KeyCode::Char('?') => Action::Help,
            KeyCode::Char('b') => Action::Boots,
            KeyCode::Char('q') => Action::Quit,
            _ => return None,
        })
    }

    /// Does what an action asks.
    pub fn handle(&mut self, action: Action, kb: &Kb) -> Effect {
        if action == Action::Quit {
            self.quit = true;
            return Effect::None;
        }
        match &self.overlay {
            Overlay::None => self.handle_main(action),
            Overlay::Help => {
                if matches!(action, Action::Back | Action::Help | Action::Open) {
                    self.overlay = Overlay::None;
                }
                Effect::None
            }
            Overlay::Article { .. } => {
                self.handle_article(action, kb);
                Effect::None
            }
            Overlay::Picker { .. } => self.handle_picker(action),
        }
    }

    fn handle_main(&mut self, action: Action) -> Effect {
        let last = self.visible.len().saturating_sub(1);
        match action {
            Action::Up => self.selected = self.selected.saturating_sub(1),
            Action::Down => self.selected = (self.selected + 1).min(last),
            Action::PageUp => self.selected = self.selected.saturating_sub(self.page),
            Action::PageDown => self.selected = (self.selected + self.page).min(last),
            Action::Top => self.selected = 0,
            Action::Bottom => self.selected = last,
            Action::PreviousStage => self.move_stage(-1),
            Action::NextStage => self.move_stage(1),
            Action::Back => {
                if self.typing.take().is_some() {
                    // Esc while typing: no search.
                    self.filter.grep = None;
                    self.refilter();
                } else if self.stage.is_some() {
                    self.set_stage(None);
                } else if self.filter.grep.is_some() {
                    self.filter.grep = None;
                    self.refilter();
                }
            }
            Action::StartSearch => {
                self.typing = Some(self.filter.grep.clone().unwrap_or_default());
            }
            Action::Type(c) => {
                if let Some(text) = &mut self.typing {
                    text.push(c);
                    self.search_as_typed();
                }
            }
            Action::Erase => {
                if let Some(text) = &mut self.typing {
                    text.pop();
                    self.search_as_typed();
                }
            }
            Action::EndSearch => self.typing = None,
            Action::ToggleExplained => {
                self.filter.explained_only = !self.filter.explained_only;
                self.refilter();
            }
            Action::PreviousMilestone => self.jump_to_milestone(false),
            Action::NextMilestone => self.jump_to_milestone(true),
            Action::Open => {
                if let Some(id) = self.explanation_id() {
                    self.overlay = Overlay::Article {
                        id,
                        scroll: 0,
                        related: None,
                        history: Vec::new(),
                    };
                }
            }
            Action::Help => self.overlay = Overlay::Help,
            Action::Boots => return Effect::ListBoots,
            Action::NextRelated | Action::PreviousRelated | Action::Quit => {}
        }
        Effect::None
    }

    /// Filters as the search is typed.
    fn search_as_typed(&mut self) {
        let text = self.typing.clone().unwrap_or_default();
        self.filter.grep = (!text.is_empty()).then_some(text);
        self.refilter();
    }

    fn handle_article(&mut self, action: Action, kb: &Kb) {
        let Overlay::Article {
            id,
            scroll,
            related,
            history,
        } = &mut self.overlay
        else {
            return;
        };
        let related_ids = kb
            .article(id)
            .map(|a| a.related.clone())
            .unwrap_or_default();
        match action {
            Action::Up => *scroll = scroll.saturating_sub(1),
            Action::Down => *scroll += 1,
            Action::PageUp => *scroll = scroll.saturating_sub(self.page),
            Action::PageDown => *scroll += self.page,
            Action::Top => *scroll = 0,
            Action::NextRelated | Action::PreviousRelated if !related_ids.is_empty() => {
                let count = related_ids.len();
                *related = Some(match (*related, action) {
                    (None, Action::NextRelated) => 0,
                    (None, _) => count - 1,
                    (Some(i), Action::NextRelated) => (i + 1) % count,
                    (Some(i), _) => (i + count - 1) % count,
                });
            }
            Action::Open => {
                if let Some(next) = related.and_then(|i| related_ids.get(i)) {
                    history.push(std::mem::replace(id, next.clone()));
                    *scroll = 0;
                    *related = None;
                }
            }
            Action::Back => match history.pop() {
                Some(previous) => {
                    *id = previous;
                    *scroll = 0;
                    *related = None;
                }
                None => self.overlay = Overlay::None,
            },
            _ => {}
        }
    }

    fn handle_picker(&mut self, action: Action) -> Effect {
        let Overlay::Picker { items, selected } = &mut self.overlay else {
            return Effect::None;
        };
        let last = items.len().saturating_sub(1);
        match action {
            Action::Up => *selected = selected.saturating_sub(1),
            Action::Down => *selected = (*selected + 1).min(last),
            Action::Top => *selected = 0,
            Action::Bottom => *selected = last,
            Action::Open => {
                let selector = items.get(*selected).map(|item| item.selector.clone());
                self.overlay = Overlay::None;
                if let Some(selector) = selector {
                    return Effect::Load(selector);
                }
            }
            Action::Back | Action::Boots => self.overlay = Overlay::None,
            _ => {}
        }
        Effect::None
    }

    /// Opens the boot picker on these boots, the shown one selected if it is there.
    pub fn open_picker(&mut self, items: Vec<PickerItem>) {
        let shown = &self.record.boot.boot_id;
        let selected = items
            .iter()
            .position(|item| {
                item.label
                    .contains(&shown.chars().take(8).collect::<String>())
            })
            .unwrap_or(0);
        self.overlay = Overlay::Picker { items, selected };
    }

    fn move_stage(&mut self, step: isize) {
        let stages = self.stages();
        if stages.is_empty() {
            return;
        }
        let position = self.stage.and_then(|s| stages.iter().position(|&x| x == s));
        let next = match (position, step) {
            (None, s) if s > 0 => Some(0),
            (None, _) => Some(stages.len() - 1),
            (Some(i), s) => i.checked_add_signed(s).filter(|&j| j < stages.len()),
        };
        // Past either end: back to all stages.
        self.set_stage(next.map(|i| stages[i]));
    }

    fn set_stage(&mut self, stage: Option<Stage>) {
        self.stage = stage;
        self.filter.stages = stage.into_iter().collect();
        self.refilter();
    }

    /// Recomputes the visible events, keeping the selected one when it stays.
    fn refilter(&mut self) {
        let kept = self.visible.get(self.selected).copied();
        match self.filter.matcher() {
            Ok(keep) => {
                self.search_error = false;
                self.visible = (0..self.record.events.len())
                    .filter(|&i| keep(&self.record.events[i]))
                    .collect();
            }
            // While typing, an unfinished pattern is not valid yet: keep the list.
            Err(_) => self.search_error = true,
        }
        self.selected = kept
            .and_then(|index| self.visible.iter().position(|&i| i == index))
            .unwrap_or(0);
    }

    fn jump_to_milestone(&mut self, forward: bool) {
        let milestone_events: Vec<usize> = self
            .record
            .milestones
            .iter()
            .filter_map(|m| m.event)
            .filter_map(|id| self.record.events.iter().position(|e| e.id == id))
            .filter_map(|index| self.visible.iter().position(|&i| i == index))
            .collect();
        let target = if forward {
            milestone_events
                .iter()
                .copied()
                .filter(|&p| p > self.selected)
                .min()
        } else {
            milestone_events
                .iter()
                .copied()
                .filter(|&p| p < self.selected)
                .max()
        };
        if let Some(position) = target {
            self.selected = position;
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::tui::view::tests::fixture;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn moves_stay_in_the_list() {
        let mut app = fixture();
        let kb = Kb::embedded();
        app.handle(Action::Up, kb);
        assert_eq!(app.selected, 0);
        app.handle(Action::Bottom, kb);
        assert_eq!(app.selected, app.visible.len() - 1);
        app.handle(Action::Down, kb);
        app.handle(Action::PageDown, kb);
        assert_eq!(app.selected, app.visible.len() - 1);
        app.handle(Action::Top, kb);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn stages_filter_the_list_and_cycle_back_to_all() {
        let mut app = fixture();
        let kb = Kb::embedded();
        let all = app.visible.len();
        app.handle(Action::NextStage, kb);
        let first = app.stages()[0];
        assert_eq!(app.stage, Some(first));
        assert!(app.visible.len() < all);
        assert!(
            app.visible
                .iter()
                .all(|&i| app.record.events[i].stage == first)
        );
        // Before the first stage: all of them again.
        app.handle(Action::PreviousStage, kb);
        assert_eq!((app.stage, app.visible.len()), (None, all));
        // Esc clears the stage.
        app.handle(Action::PreviousStage, kb);
        assert!(app.stage.is_some());
        app.handle(Action::Back, kb);
        assert_eq!(app.stage, None);
    }

    #[test]
    fn searches_as_typed_and_waits_for_a_valid_pattern() {
        let mut app = fixture();
        let kb = Kb::embedded();
        let all = app.visible.len();
        assert_eq!(
            app.action_for(key(KeyCode::Char('/'))),
            Some(Action::StartSearch)
        );
        app.handle(Action::StartSearch, kb);
        // While typing, letters are text, not commands.
        assert_eq!(
            app.action_for(key(KeyCode::Char('q'))),
            Some(Action::Type('q'))
        );
        for c in "sway(".chars() {
            app.handle(Action::Type(c), kb);
        }
        // `sway(` is not a valid pattern yet: the list stays as `sway` left it.
        assert!(app.search_error);
        let with_sway = app.visible.len();
        assert!(with_sway > 0 && with_sway < all);
        app.handle(Action::Erase, kb);
        assert!(!app.search_error);
        app.handle(Action::EndSearch, kb);
        assert_eq!(app.typing, None);
        assert_eq!(app.filter.grep.as_deref(), Some("sway"));
        // Esc clears the search.
        app.handle(Action::Back, kb);
        assert_eq!(app.visible.len(), all);
    }

    #[test]
    fn esc_while_typing_cancels_the_search_only() {
        let mut app = fixture();
        let kb = Kb::embedded();
        app.handle(Action::NextStage, kb);
        app.handle(Action::StartSearch, kb);
        app.handle(Action::Type('x'), kb);
        app.handle(Action::Back, kb);
        assert_eq!(app.typing, None);
        assert_eq!(app.filter.grep, None);
        assert!(app.stage.is_some());
    }

    #[test]
    fn explained_only_leaves_out_raw_log_lines() {
        let mut app = fixture();
        let kb = Kb::embedded();
        let all = app.visible.len();
        app.handle(Action::ToggleExplained, kb);
        assert!(app.visible.len() < all);
        assert!(app.visible.iter().all(|&i| {
            let event = &app.record.events[i];
            event.explain_id.is_some() || event.dur_ns.is_some()
        }));
        app.handle(Action::ToggleExplained, kb);
        assert_eq!(app.visible.len(), all);
    }

    #[test]
    fn jumps_from_milestone_to_milestone() {
        let mut app = fixture();
        let kb = Kb::embedded();
        app.handle(Action::NextMilestone, kb);
        let first = app.selected;
        let event = app.selected_event().unwrap().id;
        assert!(app.record.milestones.iter().any(|m| m.event == Some(event)));
        app.handle(Action::NextMilestone, kb);
        assert!(app.selected > first);
        app.handle(Action::PreviousMilestone, kb);
        assert_eq!(app.selected, first);
    }

    #[test]
    fn explanations_open_related_ones_and_come_back() {
        let mut app = fixture();
        let kb = Kb::embedded();
        let id = app.explanation_id().unwrap();
        app.handle(Action::Open, kb);
        assert!(matches!(&app.overlay, Overlay::Article { id: shown, .. } if *shown == id));
        app.handle(Action::NextRelated, kb);
        app.handle(Action::Open, kb);
        let Overlay::Article {
            id: related,
            history,
            ..
        } = &app.overlay
        else {
            panic!("an explanation is open");
        };
        assert_ne!(*related, id);
        assert_eq!(history.as_slice(), [id.as_str()]);
        app.handle(Action::Back, kb);
        assert!(matches!(&app.overlay, Overlay::Article { id: shown, .. } if *shown == id));
        app.handle(Action::Back, kb);
        assert_eq!(app.overlay, Overlay::None);
    }

    #[test]
    fn the_picker_loads_the_chosen_boot() {
        let mut app = fixture();
        let kb = Kb::embedded();
        assert_eq!(app.handle(Action::Boots, kb), Effect::ListBoots);
        app.open_picker(vec![
            PickerItem {
                label: "This boot".into(),
                selector: None,
            },
            PickerItem {
                label: "other".into(),
                selector: Some("/var/lib/kokuin/boots/x".into()),
            },
        ]);
        app.handle(Action::Down, kb);
        assert_eq!(
            app.handle(Action::Open, kb),
            Effect::Load(Some("/var/lib/kokuin/boots/x".into()))
        );
        assert_eq!(app.overlay, Overlay::None);
    }

    #[test]
    fn q_quits_except_while_typing() {
        let mut app = fixture();
        let kb = Kb::embedded();
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        app.handle(Action::StartSearch, kb);
        assert_eq!(
            app.action_for(key(KeyCode::Char('q'))),
            Some(Action::Type('q'))
        );
        assert_eq!(app.action_for(ctrl_c), Some(Action::Quit));
        app.handle(Action::Back, kb);
        assert_eq!(app.action_for(key(KeyCode::Char('q'))), Some(Action::Quit));
        app.handle(Action::Quit, kb);
        assert!(app.quit);
    }
}
