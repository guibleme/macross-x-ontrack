mod app;
mod beads;
mod command_state;
mod detail;
mod epic;
mod event;
mod herdr;
mod hotkeys;
mod interface;
mod planning;
mod runtime;
mod screenshots;
mod sessions;
mod settings;
mod startup;
mod terminal;
mod theme;
mod ui;
mod uninstall;
mod update;
mod update_cli;
mod worktrees;

use std::{error::Error, io};

fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args_os().skip(1).collect::<Vec<_>>().as_slice() {
        [] => {}
        [command] if command == "serve" => { runtime::serve()?; return Ok(()); }
        [command] if command == "api" => {
            std::process::exit(if interface::run(false)? { 0 } else { 1 });
        }
        [command, flag] if command == "api" && flag == "--allow-writes" => {
            std::process::exit(if interface::run(true)? { 0 } else { 1 });
        }
        [command] if command == "update" => {
            update_cli::run()?;
            return Ok(());
        }
        [command] if command == "uninstall" => {
            uninstall::run(false)?;
            return Ok(());
        }
        [command, flag] if command == "uninstall" && flag == "--yes" => {
            uninstall::run(true)?;
            return Ok(());
        }
        [command, state] if command == "status" => {
            worktrees::report_activity(&std::env::current_dir()?, &state.to_string_lossy(), None)?;
            println!("Worktree status report saved: {}", state.to_string_lossy());
            return Ok(());
        }
        [command, state, detail] if command == "status" => {
            worktrees::report_activity(
                &std::env::current_dir()?,
                &state.to_string_lossy(),
                Some(&detail.to_string_lossy()),
            )?;
            println!("Worktree status report saved: {}", state.to_string_lossy());
            return Ok(());
        }
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "Usage: ontrack [serve | api [--allow-writes] | update | uninstall [--yes] | status <working|merging|blocked|clear> [detail]]\n\nRun without arguments to browse the current Beads workspace.\nAPI reads one versioned JSON request from stdin without a terminal; writes require --allow-writes.\nUpdate checks for a newer compatible release and asks before installing it.\nStatus reports advisory activity for the current managed checkout; clear removes it.\nUninstall removes the running OnTrack executable and user settings after confirmation."
            );
            return Ok(());
        }
        _ => return Err(
            "Usage: ontrack [serve | api [--allow-writes] | update | uninstall [--yes] | status <working|merging|blocked|clear> [detail]]"
                .into(),
        ),
    }
    startup::ensure_herdr_workspace()?;
    if !startup::ensure_workspace()? {
        return Ok(());
    }
    runtime::ensure_started()?;
    let mut terminal = terminal::init()?;
    let result = app::run(&mut terminal);
    let restore_result = terminal::restore(&mut terminal);
    result?;
    restore_result?;
    Ok(())
}

pub type Tui = ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>;
