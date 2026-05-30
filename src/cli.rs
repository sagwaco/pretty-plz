use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "plz",
    version,
    about = "Natural-language to shell-command CLI",
    long_about = "Type a request in plain English and get up to 3 candidate shell \
                  commands to pick from. Selected command is printed to stdout.",
    args_conflicts_with_subcommands = true
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// The natural-language request, e.g. "encode video files in this folder to 720p".
    /// Multiple words are joined with spaces — no need to quote.
    #[arg(trailing_var_arg = true)]
    pub query: Vec<String>,

    /// Override the LLM provider for this call. Persisted choice is unchanged.
    #[arg(long, value_name = "anthropic|openai|chatgpt", env = "PLZ_PROVIDER")]
    pub provider: Option<String>,

    /// Override the model for this call.
    #[arg(long, value_name = "MODEL_ID")]
    pub model: Option<String>,

    /// Force which credential type to use. Default `auto` is API-key first,
    /// OAuth second — set `api` to skip OAuth even when tokens are stored,
    /// or `oauth` to skip the env API key even when set.
    #[arg(
        long,
        value_name = "auto|api|oauth",
        env = "PLZ_AUTH",
        default_value = "auto"
    )]
    pub auth: String,

    /// Print raw provider responses to stderr for debugging.
    #[arg(long)]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Guided setup: connect a provider, then install the shell wrapper that
    /// auto-prefills the next prompt with the command you pick. Run this once
    /// after installing plz.
    ///
    /// Subcommands `login`, `model`, and `update` adjust one piece without
    /// rerunning the full wizard.
    Configure {
        #[command(subcommand)]
        action: Option<ConfigureAction>,
    },
    /// Sign in to a provider.
    ///
    /// `anthropic` runs the Claude OAuth browser flow; `openai` prompts you to
    /// paste an API key; `chatgpt` runs the ChatGPT OAuth browser flow. Omit the
    /// argument to pick interactively (the picker also lets you paste an
    /// Anthropic API key instead of using OAuth).
    Login {
        /// `anthropic` (also `claude`), `openai`, or `chatgpt` (also `codex`). Omit to pick interactively.
        #[arg(value_name = "anthropic|openai|chatgpt")]
        provider: Option<String>,
    },
    /// Forget stored sign-in state for a provider.
    Logout {
        /// `anthropic` (also `claude`), `openai`, or `chatgpt` (also `codex`).
        #[arg(value_name = "anthropic|openai|chatgpt")]
        provider: String,
    },
    /// Print sign-in status for providers.
    Status,
    /// Upgrade plz to the latest release.
    Update,
    /// Remove plz: forget credentials, shell integration, and the binary.
    Uninstall {
        /// Skip the confirmation prompt.
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
    /// Print a shell snippet that pre-fills the next prompt with the command
    /// you pick, instead of just printing it for you to copy.
    ///
    /// Add it to your shell rc file, e.g. `eval "$(plz init zsh)"` in
    /// `~/.zshrc` or `eval "$(plz init bash)"` in `~/.bashrc`. Omit the shell
    /// to detect it from `$SHELL`.
    Init {
        /// `zsh` or `bash`. Omit to detect from `$SHELL`.
        #[arg(value_name = "zsh|bash")]
        shell: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigureAction {
    /// Sign in to a provider (alias for `plz login`).
    Login {
        /// `anthropic` (also `claude`), `openai`, or `chatgpt` (also `codex`). Omit to pick interactively.
        #[arg(value_name = "anthropic|openai|chatgpt")]
        provider: Option<String>,
    },
    /// Change the default model for the configured provider.
    Model,
    /// Upgrade plz to the latest release (alias for `plz update`).
    Update,
}

impl Args {
    pub fn joined_query(&self) -> String {
        self.query.join(" ")
    }
}
