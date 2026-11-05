use crate::app::{App, Modal};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Issues,
    Details,
    EpicIssues,
    EpicDetails,
    Worktrees,
    Form,
    FormChoices,
    Settings,
    SettingsChoices,
    EditLoading,
    Close,
    Move,
    Delete,
    GenerateEpic,
    WorktreeConfirm,
    Planning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Help {
    pub context: Context,
    pub offset: usize,
}

impl Context {
    pub fn current(app: &App) -> Self {
        match app.modal.as_ref() {
            Some(Modal::Planning(_)) => Self::Planning,
            Some(Modal::Create(form) | Modal::Edit { form, .. }) if form.dropdown.is_some() => {
                Self::FormChoices
            }
            Some(Modal::Create(_) | Modal::Edit { .. }) => Self::Form,
            Some(Modal::Settings {
                dropdown_open: true,
                ..
            }) => Self::SettingsChoices,
            Some(Modal::Settings { .. }) => Self::Settings,
            Some(Modal::EditLoading { .. }) => Self::EditLoading,
            Some(Modal::ConfirmGenerateEpic { .. }) => Self::GenerateEpic,
            Some(Modal::ConfirmClose { .. }) => Self::Close,
            Some(Modal::Move { .. }) => Self::Move,
            Some(Modal::ConfirmDelete { .. }) => Self::Delete,
            Some(Modal::ConfirmWorktree { .. }) | Some(Modal::ReviewResult { .. }) => {
                Self::WorktreeConfirm
            }
            None if app.worktrees_view => Self::Worktrees,
            None if app.selected_is_epic() && app.focus_detail => Self::EpicDetails,
            None if app.selected_is_epic() => Self::EpicIssues,
            None if app.focus_detail => Self::Details,
            None => Self::Issues,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Issues | Self::EpicIssues => "Kanban board",
            Self::Details | Self::EpicDetails => "Issue details",
            Self::Worktrees => "Outstanding worktrees",
            Self::Form => "Create / edit form",
            Self::FormChoices => "Form choice list",
            Self::Settings => "Settings",
            Self::SettingsChoices => "Setting choices",
            Self::EditLoading => "Loading edit",
            Self::GenerateEpic => "Generate Epic tasks",
            Self::Close => "Close confirmation",
            Self::Move => "Move Bead",
            Self::Delete => "Delete confirmation",
            Self::WorktreeConfirm => "Worktree confirmation",
            Self::Planning => "Plan new work",
        }
    }

    pub fn entries(self) -> Vec<(&'static str, &'static str)> {
        let mut rows = match self {
            Self::Issues | Self::EpicIssues => vec![
                ("↑ / ↓", "Select a Bead within the focused column"),
                (
                    "← / →",
                    "Focus the adjacent column, including empty columns",
                ),
                (
                    "Shift+← / Shift+→",
                    "Move the selected Bead to the adjacent column",
                ),
                ("m", "Choose any destination column for this Bead"),
                ("Enter / Tab", "Open the detail drawer"),
                ("r", "Toggle Ready for an open issue; Epic includes tasks"),
                ("s", "Open settings"),
                ("n", "Create a bead"),
                ("Shift+N", "Create child of selected open task"),
                ("e", "Edit selected bead"),
                (
                    "c",
                    "Close after confirmation; verified integration required",
                ),
                ("d", "Delete selected bead after confirmation"),
                ("h", "Hand selected bead to Herdr"),
                (
                    "Shift+R",
                    "Toggle Ready for an open issue; Epic includes tasks",
                ),
                ("w", "Open outstanding worktrees"),
                ("Esc", "Stay in the issue browser"),
            ],
            Self::Details | Self::EpicDetails => vec![
                ("↑ / ↓", "Scroll issue details one line"),
                ("PgUp / PgDn", "Scroll issue details one page"),
                (
                    "Enter / Tab / Esc",
                    "Close the detail drawer and return to the board",
                ),
                ("r", "Toggle Ready for an open issue; Epic includes tasks"),
                (
                    "Shift+R",
                    "Toggle Ready for an open issue; Epic includes tasks",
                ),
                ("s", "Open settings"),
                ("n", "Create a bead"),
                ("Shift+N", "Create child of selected open task"),
                ("e", "Edit selected bead"),
                (
                    "c",
                    "Close after confirmation; verified integration required",
                ),
                ("d", "Delete selected bead after confirmation"),
                ("h", "Hand selected bead to Herdr"),
                ("w", "Open outstanding worktrees"),
            ],
            Self::Worktrees => vec![
                ("↑ / k", "Select previous worktree"),
                ("↓ / j", "Select next worktree"),
                ("PgUp / PgDn", "Scroll worktree details one page"),
                ("Enter", "Focus selected Herdr workspace"),
                ("m", "Request merge for selected handoff"),
                ("c", "Close integrated Herdr workspace"),
                ("x", "Remove integrated checkout"),
                (
                    "Shift+X",
                    "Force remove checkout (discard uncommitted files)",
                ),
                ("a", "Ask original idle agent to continue in this checkout"),
                ("Shift+R", "Restart selected active task"),
                (
                    "v / Shift+V",
                    "Accept exact result / request changes with evidence",
                ),
                ("r", "Refresh outstanding worktrees"),
                ("w / Esc / Backspace", "Return to issue browser"),
            ],
            Self::Form => vec![
                ("Text / Space", "Type in title or description"),
                ("Tab / Shift+Tab", "Move between form controls"),
                ("← / →", "Move text cursor or cycle focused choice"),
                ("↑ / ↓", "Move description cursor between visible lines"),
                ("Home / End", "Move to start / end of text line"),
                ("Backspace / Delete", "Remove text beside the cursor"),
                (
                    "Enter",
                    "Advance from text; open choice; submit or cancel on buttons",
                ),
                ("Space", "Open focused choice list"),
                ("Shift+Enter", "Insert a description newline"),
                ("Esc", "Discard form and close"),
            ],
            Self::FormChoices => vec![
                ("↑ / ↓", "Move through choices"),
                ("← / →", "Cycle choices with wraparound"),
                ("Enter / Space", "Accept selected choice"),
                ("Esc", "Restore original choice and close list"),
            ],
            Self::Settings => vec![
                ("↑ / ↓", "Move between setting rows"),
                ("Tab / Shift+Tab", "Move between setting rows"),
                ("← / →", "Cycle focused theme, agent, or limit"),
                ("Enter / Space", "Open choices, toggle preference, or close"),
                ("Esc", "Close settings"),
            ],
            Self::SettingsChoices => vec![
                ("↑ / ↓", "Move to another setting and close choices"),
                (
                    "Tab / Shift+Tab",
                    "Move to another setting and close choices",
                ),
                ("← / →", "Cycle choices and save immediately"),
                ("Enter / Space", "Close choices and save selection"),
                ("Esc", "Close choices"),
            ],
            Self::EditLoading => vec![
                ("e", "Retry a failed edit load"),
                ("Esc", "Cancel edit loading"),
            ],
            Self::Move => vec![
                ("↑ / ↓", "Choose a destination column"),
                (
                    "Enter",
                    "Move Bead; Closed requires confirmation and integration",
                ),
                ("Esc", "Cancel and return to the board"),
            ],
            Self::Close | Self::Delete | Self::WorktreeConfirm | Self::GenerateEpic => vec![
                ("y / Y", "Confirm action"),
                ("n / N / Enter", "Cancel action"),
                ("Esc", "Cancel action"),
            ],
            Self::Planning => vec![
                ("Enter", "Start, send answer, or send revision"),
                ("↑ / ↓", "Choose an agent option or scroll the plan"),
                ("Tab", "Write a custom answer"),
                ("y", "Create the reviewed Beads"),
                ("r", "Request a plan revision or retry reading output"),
                ("Esc", "Cancel planning"),
            ],
        };
        rows.extend([
            ("Shift+?", "Open or close this hotkey reference"),
            ("↑ / k · ↓ / j", "Scroll this reference while it is open"),
            ("PgUp / PgDn", "Scroll this reference by ten lines"),
            ("Home / End", "Jump to start / end of this reference"),
            ("Esc", "Close this reference while it is open"),
        ]);
        if matches!(
            self,
            Self::Issues | Self::Details | Self::EpicIssues | Self::EpicDetails | Self::Worktrees
        ) {
            rows.push(("q / Ctrl+C", "Quit ontrack"));
        }
        rows
    }
}

pub fn is_help_key(key: KeyEvent) -> bool {
    (key.code == KeyCode::Char('?')
        && matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT))
        || (key.code == KeyCode::Char('/') && key.modifiers == KeyModifiers::SHIFT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_and_drawer_help_describes_actual_navigation() {
        let board = Context::Issues.entries();
        assert!(board.iter().any(|&(key, _)| key == "m"));
        assert!(board.iter().any(|&(key, _)| key == "Shift+← / Shift+→"));
        assert!(board.iter().any(|&(key, _)| key == "Enter / Tab"));
        assert!(
            !board
                .iter()
                .any(|&(_, action)| action.contains("collapsed") || action.contains("Collapse"))
        );
        assert!(
            Context::Details
                .entries()
                .iter()
                .any(|&(key, _)| key == "Enter / Tab / Esc")
        );
        assert!(
            Context::Move
                .entries()
                .iter()
                .any(|&(key, _)| key == "Enter")
        );
        assert_eq!(Context::Issues.title(), "Kanban board");
        for context in [
            Context::Issues,
            Context::EpicIssues,
            Context::Details,
            Context::EpicDetails,
        ] {
            assert!(context.entries().iter().any(
                |&(key, action)| key == "c" && action.contains("verified integration required")
            ));
        }
    }

    #[test]
    fn recognizes_question_mark_help_and_rejects_old_control_shortcuts() {
        for (code, modifiers) in [
            (KeyCode::Char('?'), KeyModifiers::NONE),
            (KeyCode::Char('?'), KeyModifiers::SHIFT),
            (KeyCode::Char('/'), KeyModifiers::SHIFT),
        ] {
            assert!(is_help_key(KeyEvent::new(code, modifiers)));
        }
        for code in [KeyCode::Char('/'), KeyCode::Backspace] {
            assert!(!is_help_key(KeyEvent::new(code, KeyModifiers::NONE)));
        }
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyModifiers::ALT,
            KeyModifiers::ALT | KeyModifiers::SHIFT,
        ] {
            for code in [KeyCode::Char('?'), KeyCode::Char('/'), KeyCode::Char('_')] {
                assert!(!is_help_key(KeyEvent::new(code, modifiers)));
            }
        }
    }
}
