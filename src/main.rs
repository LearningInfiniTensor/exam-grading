use anyhow::{bail, Context, Result};
use app_state::StateFileStatus;
use clap::{Parser, Subcommand};
use std::{
    io::{self, BufRead, IsTerminal, StdoutLock, Write},
    path::Path,
    process::exit,
};
use tokio::runtime::Runtime;

use self::{app_state::AppState, dev::DevCommands, info_file::InfoFile, watch::WatchExit};

mod app_state;
mod cargo_toml;
mod cmd;
mod cicv_verify;
mod dev;
mod embedded;
mod exercise;
mod info_file;
mod init;
mod list;
mod progress_bar;
mod run;
mod terminal_link;
mod watch;

const CURRENT_FORMAT_VERSION: u8 = 1;
const DEBUG_PROFILE: bool = {
    #[allow(unused_assignments, unused_mut)]
    let mut debug_profile = false;

    #[cfg(debug_assertions)]
    {
        debug_profile = true;
    }

    debug_profile
};

fn in_official_repo() -> bool {
    Path::new("dev/rustlings-repo.txt").exists()
}

fn clear_terminal(stdout: &mut StdoutLock) -> io::Result<()> {
    stdout.write_all(b"\x1b[H\x1b[2J\x1b[3J")
}

fn press_enter_prompt() -> io::Result<()> {
    io::stdin().lock().read_until(b'\n', &mut Vec::new())?;
    Ok(())
}

#[derive(Parser)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Option<Subcommands>,
    #[arg(long)]
    manual_run: bool,
}

#[derive(Subcommand)]
enum Subcommands {
    Init,
    Run {
        name: Option<String>,
    },
    Reset {
        name: String,
    },
    Hint {
        name: Option<String>,
    },
    #[command(subcommand)]
    Dev(DevCommands),
    CicvVerify,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !DEBUG_PROFILE && in_official_repo() {
        bail!("{OLD_METHOD_ERR}");
    }

    match args.command {
        Some(Subcommands::Init) => {
            if DEBUG_PROFILE {
                bail!("Disabled in the debug build");
            }

            let mut stdout = io::stdout().lock();
            stdout.write_all(b"This command will create the directory `rustlings/` which will contain the exercises.\nPress ENTER to continue ")?;
            stdout.flush()?;
            press_enter_prompt()?;
            stdout.write_all(b"\n")?;

            return init::init().context("Initialization failed");
        }
        Some(Subcommands::Dev(dev_command)) => return dev_command.run(),
        Some(Subcommands::CicvVerify) => {
            if !Path::new("exercises").is_dir() {
                println!("{PRE_INIT_MSG}");
                exit(1);
            }

            let info_file = InfoFile::parse()?;
            if info_file.format_version > CURRENT_FORMAT_VERSION {
                bail!(FORMAT_VERSION_HIGHER_ERR);
            }

            let exercises = info_file.exercises;
            let (mut app_state, _state_file_status) = AppState::new(
                exercises,
                info_file.final_message.unwrap_or_default(),
            )?;

            let rt = Runtime::new().context("Failed to create Tokio runtime")?;
            rt.block_on(cicv_verify::cicv_verify(&mut app_state))?;
        }
        _ => (),
    }

    if !Path::new("exercises").is_dir() {
        println!("{PRE_INIT_MSG}");
        exit(1);
    }

    let info_file = InfoFile::parse()?;

    if info_file.format_version > CURRENT_FORMAT_VERSION {
        bail!(FORMAT_VERSION_HIGHER_ERR);
    }

    let (mut app_state, state_file_status) = AppState::new(
        info_file.exercises,
        info_file.final_message.unwrap_or_default(),
    )?;

    if let Some(welcome_message) = info_file.welcome_message {
        match state_file_status {
            StateFileStatus::NotRead => {
                let mut stdout = io::stdout().lock();
                clear_terminal(&mut stdout)?;

                let welcome_message = welcome_message.trim();
                write!(stdout, "{welcome_message}\n\nPress ENTER to continue ")?;
                stdout.flush()?;
                press_enter_prompt()?;
                clear_terminal(&mut stdout)?;
            }
            StateFileStatus::Read => (),
        }
    }

    match args.command {
        None => {
            if !io::stdout().is_terminal() {
                bail!("Unsupported or missing terminal/TTY");
            }

            let notify_exercise_names = if args.manual_run {
                None
            } else {
                Some(
                    &*app_state
                        .exercises()
                        .iter()
                        .map(|exercise| exercise.name.as_bytes())
                        .collect::<Vec<_>>()
                        .leak(),
                )
            };

            loop {
                match watch::watch(&mut app_state, notify_exercise_names)? {
                    WatchExit::Shutdown => break,
                    WatchExit::List => list::list(&mut app_state)?,
                }
            }
        }
        Some(Subcommands::Run { name }) => {
            if let Some(name) = name {
                app_state.set_current_exercise_by_name(&name)?;
            }
            run::run(&mut app_state)?;
        }
        Some(Subcommands::Reset { name }) => {
            app_state.set_current_exercise_by_name(&name)?;
            let exercise_path = app_state.reset_current_exercise()?;
            println!("The exercise {exercise_path} has been reset");
        }
        Some(Subcommands::Hint { name }) => {
            if let Some(name) = name {
                app_state.set_current_exercise_by_name(&name)?;
            }
            println!("{}", app_state.current_exercise().hint);
        }
        Some(Subcommands::Init | Subcommands::Dev(_)) => (),
        Some(Subcommands::CicvVerify) => (),
    }

    Ok(())
}

const OLD_METHOD_ERR: &str =
    "You are trying to run Rustlings using the old method before version 6.
The new method doesn't include cloning the Rustlings' repository.
Please follow the instructions in `README.md`:
https://github.com/rust-lang/rustlings#getting-started";

const FORMAT_VERSION_HIGHER_ERR: &str =
    "The format version specified in the `info.toml` file is higher than the last one supported.
It is possible that you have an outdated version of Rustlings.
Try to install the latest Rustlings version first.";

const PRE_INIT_MSG: &str = r"
       Welcome to...
                 _   _ _
  _ __ _   _ ___| |_| (_)_ __   __ _ ___
 | '__| | | / __| __| | | '_ \ / _` / __|
 | |  | |_| \__ \ |_| | | | | | (_| \__ \
 |_|   \__,_|___/\__|_|_|_| |_|\__, |___/
                               |___/

The `exercises/` directory couldn't be found in the current directory.
If you are just starting with Rustlings, run the command `rustlings init` to initialize it.";
