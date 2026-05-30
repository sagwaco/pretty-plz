mod api_key;
mod cli;
mod config;
mod context;
mod error;
mod oauth;
mod prompt;
mod provider;
mod secret_file;
mod shell;
mod spinner;
mod tui;
mod uninstall;
mod update;

use std::process::ExitCode;

use clap::Parser;

use crate::error::{Error, Result};
use crate::provider::Kind;
use crate::provider::Turn;
use crate::provider::auth::AuthPref;
use crate::provider::schema::Response;

fn main() -> ExitCode {
    let mut args = cli::Args::parse();
    let skip_update_check = matches!(
        args.command,
        Some(cli::Command::Update)
            | Some(cli::Command::Uninstall { .. })
            | Some(cli::Command::Init { .. })
            | Some(cli::Command::Configure {
                action: Some(cli::ConfigureAction::Update)
            })
    );
    if !skip_update_check {
        update::maybe_notify();
    }
    // OAuth flow errors during `plz login` are auth/config problems, not
    // "provider call failed". Map them to exit 3 by tagging the subcommand.
    let is_login_or_logout = matches!(
        args.command,
        Some(cli::Command::Login { .. })
            | Some(cli::Command::Logout { .. })
            | Some(cli::Command::Configure {
                action: Some(cli::ConfigureAction::Login { .. })
            })
    );
    let result = match args.command.take() {
        Some(cli::Command::Configure { action }) => match action {
            None => configure_cmd(),
            Some(cli::ConfigureAction::Login { provider }) => {
                login_cmd(provider.as_deref()).map(|_| ())
            }
            Some(cli::ConfigureAction::Model) => configure_model_cmd(),
            Some(cli::ConfigureAction::Update) => update::run(),
        },
        Some(cli::Command::Login { provider }) => login_cmd(provider.as_deref()).map(|_| ()),
        Some(cli::Command::Logout { provider }) => logout_cmd(&provider),
        Some(cli::Command::Status) => oauth::status(),
        Some(cli::Command::Update) => update::run(),
        Some(cli::Command::Uninstall { yes }) => uninstall::run(yes),
        Some(cli::Command::Init { shell }) => shell::init(shell.as_deref()),
        None => run_query(args),
    };
    match result {
        Ok(()) => ExitCode::from(0),
        Err(Error::Cancelled) => ExitCode::from(1),
        Err(
            e @ (Error::Config(_)
            | Error::NoApiKey
            | Error::MissingProviderKey(_, _)
            | Error::NotSignedIn(_)),
        ) => {
            eprintln!("plz: {e}");
            ExitCode::from(3)
        }
        Err(e @ (Error::OAuth(_) | Error::OAuthInvalidGrant(_))) if is_login_or_logout => {
            eprintln!("plz: {e}");
            ExitCode::from(3)
        }
        Err(e) => {
            eprintln!("plz: {e}");
            ExitCode::from(2)
        }
    }
}

fn kind_from_arg(s: &str) -> Result<Kind> {
    Kind::from_str(s).ok_or_else(|| {
        Error::Config(format!(
            "unknown provider {s:?}; use 'anthropic', 'openai', or 'chatgpt'"
        ))
    })
}

fn login_cmd(provider: Option<&str>) -> Result<Kind> {
    match provider {
        Some(s) => {
            let k = kind_from_arg(s)?;
            oauth::login(k)?;
            Ok(k)
        }
        None => match tui::pick_login_action()? {
            tui::LoginAction::Oauth(k) => {
                oauth::login(k)?;
                Ok(k)
            }
            tui::LoginAction::ApiKey(k) => {
                oauth::save_pasted_api_key(k)?;
                Ok(k)
            }
            tui::LoginAction::Codex => {
                oauth::login(Kind::Codex)?;
                Ok(Kind::Codex)
            }
        },
    }
}

fn logout_cmd(provider: &str) -> Result<()> {
    let kind = kind_from_arg(provider)?;
    oauth::logout(kind)
}

fn configure_cmd() -> Result<()> {
    eprintln!("\x1b[1m1. Connect a provider\x1b[0m");
    let connected = login_cmd(None)?;

    eprintln!();
    eprintln!("\x1b[1m2. Pick a default model\x1b[0m");
    // load_or_init writes a default config keyed on whatever auth is now
    // present — the just-connected provider qualifies, so this works on a
    // first run as well as on re-runs of `plz configure`.
    let mut cfg = config::load_or_init()?;
    // Make the just-connected provider the default for queries — otherwise
    // step 2 would silently configure the model for some *other* provider
    // the user previously set up.
    cfg.provider = connected.as_str().to_string();
    pick_and_save_model(connected, &mut cfg)?;

    eprintln!();
    eprintln!("\x1b[1m3. Enable auto-prefill\x1b[0m");
    eprintln!(
        "After you pick a command, plz can drop it onto your next shell prompt —\n\
         pre-filled and editable, not yet run — instead of just printing it for you to copy."
    );
    if !tui::confirm("Enable auto-prefill?", true)? {
        eprintln!(
            "Skipped. Run `plz configure` again later, or add `eval \"$(plz init zsh|bash)\"`\n\
             to your shell's rc file by hand."
        );
        return Ok(());
    }

    match shell::install_wrapper(None)? {
        shell::InstallOutcome::Wrote(path) => {
            eprintln!("Added wrapper to {}.", path.display());
            if shell::plz_on_path() {
                eprintln!(
                    "Open a new shell — or run `source {}` — to activate.",
                    path.display()
                );
            } else {
                eprintln!(
                    "Note: `plz` isn't on your PATH, so the wrapper will be a silent\n\
                     no-op until you install it (e.g. `cargo install --path .`) and ensure\n\
                     the install dir (usually ~/.cargo/bin) is on PATH. The wrapper also\n\
                     only intercepts `plz …`, not `./path/to/plz …` — shells skip function\n\
                     lookup for any command containing a `/`."
                );
            }
        }
        shell::InstallOutcome::AlreadyPresent(path) => {
            eprintln!(
                "Auto-prefill already enabled (found `plz init` in {}).",
                path.display()
            );
        }
    }
    Ok(())
}

fn configure_model_cmd() -> Result<()> {
    let mut cfg = config::load_or_init()?;
    let kind = cfg.kind().ok_or_else(|| {
        Error::Config(format!(
            "config has unknown provider {:?} — edit config.toml or run `plz configure`",
            cfg.provider
        ))
    })?;
    pick_and_save_model(kind, &mut cfg)?;
    eprintln!(
        "Default model for {} set to {}.",
        kind.as_str(),
        cfg.model_for(kind)
    );
    Ok(())
}

fn pick_and_save_model(kind: Kind, cfg: &mut config::Config) -> Result<()> {
    let current = cfg.model_for(kind);
    let cred = provider::auth::credential(kind).ok_or_else(|| {
        Error::Config(format!(
            "no credential found for {} — run `plz login {}` first",
            kind.as_str(),
            kind.as_str()
        ))
    })?;
    eprintln!("\x1b[2m· fetching available models from {}…\x1b[0m", kind.as_str());
    let models = match provider::models::list(kind, &cred) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "\x1b[2m· couldn't list models ({e}); using built-in list\x1b[0m"
            );
            provider::models::curated(kind)
        }
    };
    let model = tui::pick_model(kind, &models, &current)?;
    match kind {
        Kind::Anthropic => cfg.anthropic_model = model,
        Kind::OpenAi => cfg.openai_model = model,
        Kind::Codex => cfg.codex_model = model,
    }
    config::save(cfg)
}

fn run_query(args: cli::Args) -> Result<()> {
    if args.query.is_empty() {
        return Err(Error::Config(
            "no query — type a request, e.g. `plz find big files`, or run `plz --help`".into(),
        ));
    }

    let cfg = config::load_or_init()?;

    let kind = match &args.provider {
        Some(s) => kind_from_arg(s)?,
        None => cfg.kind().ok_or_else(|| {
            Error::Config(format!("config has unknown provider {:?}", cfg.provider))
        })?,
    };
    let auth_pref = AuthPref::from_str(&args.auth).ok_or_else(|| {
        Error::Config(format!(
            "unknown --auth {:?}; use 'auto', 'api', or 'oauth'",
            args.auth
        ))
    })?;
    let model = args.model.clone().unwrap_or_else(|| cfg.model_for(kind));
    let provider = provider::build(kind, model, args.debug, auth_pref)?;

    let query = args.joined_query();
    let context = context::build();
    let first_user = format!("Request: {query}\n\n{context}");

    let mut turns: Vec<Turn> = vec![Turn::User(first_user)];

    let spin = spinner::Spinner::start(provider.model().to_string());
    let first = provider.complete(&turns);
    spin.stop();
    let first = first?;

    let final_response = match first {
        Response::Commands { .. } => first,
        Response::Clarify {
            ref question,
            ref choices,
        } => {
            let answer = tui::ask_clarify(question, choices)?;
            turns.push(Turn::Assistant(first.clone()));
            turns.push(Turn::ClarifyAnswer(answer));
            let spin = spinner::Spinner::start(provider.model().to_string());
            let second = provider.complete(&turns);
            spin.stop();
            let second = second?;
            match second {
                Response::Commands { .. } => second,
                Response::Clarify { .. } => return Err(Error::ClarifyLoop),
            }
        }
    };

    let commands = match final_response {
        Response::Commands { commands } => commands,
        Response::Clarify { .. } => unreachable!(),
    };

    if commands.is_empty() {
        return Err(Error::BadResponse("model returned zero commands".into()));
    }

    let chosen = tui::pick_command(&commands)?;
    // Stdout carries only the chosen command, so the shell wrapper installed by
    // `plz configure` can capture it via `$(plz …)` and prefill the next
    // prompt. Without the wrapper, the command lands on stdout for the user to
    // copy — `cmd=$(plz …)` keeps working either way.
    println!("{chosen}");
    Ok(())
}
