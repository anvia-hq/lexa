use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use lexa::edit;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "lexa",
    disable_version_flag = true,
    about = "Fast code intelligence engine for AI agents"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,

    #[arg(
        id = "print_version",
        long = "version",
        global = true,
        action = ArgAction::SetTrue,
        help = "Print version and check for updates"
    )]
    pub(crate) version: bool,

    #[arg(long, global = true)]
    pub(crate) graph: Option<PathBuf>,

    #[arg(long = "no-graph", global = true)]
    pub(crate) no_graph: bool,

    #[arg(long, global = true, hide = true)]
    pub(crate) json: bool,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    Index {
        path: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Reindex {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    #[command(name = "clear-index")]
    ClearIndex,

    #[command(name = "files")]
    Files {
        #[arg(default_value = "")]
        path: String,

        #[arg(long)]
        path_glob: Option<String>,

        #[arg(long)]
        language: Option<String>,

        #[arg(long)]
        min_lines: Option<u32>,

        #[arg(long)]
        max_lines: Option<u32>,

        #[arg(long, alias = "max")]
        max_results: Option<usize>,
    },

    List {
        #[arg(default_value = "")]
        path: String,
    },

    #[command(name = "path-search")]
    PathSearch {
        pattern: Option<String>,

        #[arg(long)]
        query: Option<String>,

        #[arg(short, long)]
        max: Option<usize>,

        #[arg(long)]
        max_results: Option<usize>,
    },

    #[command(
        name = "text-search",
        after_help = "Examples:\n  lexa text-search \"uploadMutation\" --max 20\n  lexa text-search --query \"uploadMutation\" --max-results 20\n  lexa text-search \"useMutation\" --path-glob \"**/*.{ts,tsx}\""
    )]
    TextSearch {
        query: Option<String>,

        #[arg(long = "query", value_name = "QUERY")]
        query_flag: Option<String>,

        #[arg(short, long)]
        max: Option<usize>,

        #[arg(long)]
        max_results: Option<usize>,

        #[arg(short, long)]
        regex: bool,

        #[arg(long)]
        scope: bool,

        #[arg(short, long)]
        compact: bool,

        #[arg(long)]
        paths_only: bool,

        #[arg(long)]
        path_glob: Option<String>,
    },

    Outline {
        path: String,
    },

    #[command(name = "symbol-defs")]
    SymbolDefs {
        name: String,
    },

    #[command(name = "symbol-search")]
    SymbolSearch {
        query: Option<String>,

        #[arg(long = "query", value_name = "QUERY")]
        query_flag: Option<String>,

        #[arg(short, long)]
        max: Option<usize>,

        #[arg(long)]
        max_results: Option<usize>,
    },

    #[command(name = "word-refs")]
    WordRefs {
        word: String,

        #[arg(short, long)]
        max: Option<usize>,

        #[arg(long)]
        max_results: Option<usize>,

        #[arg(long, default_value = "0")]
        cursor: usize,

        #[arg(long)]
        path_prefix: Option<String>,

        #[arg(long = "path")]
        path: Option<String>,

        #[arg(long)]
        path_glob: Option<String>,
    },

    #[command(name = "trace-deps")]
    Deps {
        path: String,

        #[arg(short, long)]
        reverse: bool,

        #[arg(short, long)]
        transitive: bool,
    },

    Recent {
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    Callers {
        name: Option<String>,

        #[arg(long)]
        query: Option<String>,

        #[arg(short, long)]
        max: Option<usize>,

        #[arg(long)]
        max_results: Option<usize>,
    },

    Brief {
        task: Option<String>,

        #[arg(long)]
        query: Option<String>,

        #[arg(short, long)]
        max: Option<usize>,

        #[arg(long)]
        max_results: Option<usize>,

        #[arg(long)]
        path_prefix: Option<String>,

        #[arg(long)]
        path_glob: Option<String>,

        #[arg(long)]
        language: Option<String>,
    },

    Changes {
        #[arg(default_value = "0")]
        since: u64,
    },

    #[command(
        after_help = "Examples:\n  lexa read src/main.rs -L 20-80 --hash\n  lexa read src/main.rs --line-start 20 --line-end 80\n  lexa read src/main.rs --if-hash <hash>"
    )]
    Read {
        path: String,

        #[arg(short = 'L', long)]
        line_range: Option<String>,

        #[arg(long)]
        line_start: Option<u32>,

        #[arg(long)]
        line_end: Option<u32>,

        #[arg(short, long)]
        compact: bool,

        #[arg(long)]
        if_hash: Option<String>,

        #[arg(long)]
        hash: bool,
    },

    #[command(
        after_help = "Examples:\n  lexa patch src/main.rs replace -L 12 --content '    println!(\"updated\");'\n  lexa patch src/main.rs insert --after 20 --content '// new comment' --preview compact --dry-run\n  lexa patch src/main.rs --replace-text 'old block' --content 'new block'\n  lexa patch src/main.rs --anchor 'const uploadMutation' --placement after --content 'const helper = ...;'"
    )]
    Patch {
        path: String,

        #[arg(value_enum)]
        op: Option<edit::EditOp>,

        #[arg(short = 'L', long)]
        line_range: Option<String>,

        #[arg(long)]
        after: Option<u32>,

        #[arg(long)]
        replace_text: Option<String>,

        #[arg(long)]
        anchor: Option<String>,

        #[arg(long, value_enum)]
        placement: Option<edit::AnchorPlacement>,

        #[arg(long, value_enum, default_value = "compact")]
        preview: edit::PreviewMode,

        #[arg(long)]
        content: Option<String>,

        #[arg(long)]
        content_file: Option<PathBuf>,

        #[arg(long)]
        if_hash: Option<String>,

        #[arg(long)]
        dry_run: bool,
    },

    Create {
        path: String,

        #[arg(long)]
        content: Option<String>,

        #[arg(long)]
        content_file: Option<PathBuf>,

        #[arg(long)]
        overwrite: bool,

        #[arg(long)]
        dry_run: bool,
    },

    Glob {
        pattern: String,
    },

    Status,

    Audit {
        #[arg(short, long)]
        max: Option<usize>,

        #[arg(long)]
        since: Option<String>,

        #[arg(long)]
        strict: bool,

        #[arg(long)]
        config: Option<PathBuf>,

        #[arg(long)]
        no_config: bool,

        #[arg(long, value_enum)]
        include: Vec<AuditInclude>,
    },

    #[command(
        alias = "update",
        about = "Upgrade the Lexa binary, not the project index"
    )]
    Upgrade {
        #[arg(id = "upgrade_version", default_value = "latest")]
        version: String,

        #[arg(long, help = "Directory to install the upgraded Lexa binary into")]
        install_dir: Option<PathBuf>,
    },

    Watch {
        #[arg(default_value = ".")]
        path: String,

        #[arg(short, long, default_value = "500")]
        debounce: u64,
    },

    Pipeline {
        #[arg(trailing_var_arg = true)]
        pipeline: Vec<String>,
    },

    Mcp {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long)]
        no_refresh: bool,

        #[arg(long, default_value = "500")]
        debounce: u64,

        #[arg(long = "structured-content", alias = "json-output", hide = true)]
        structured_content: bool,

        #[arg(long = "log-file")]
        log_file: Option<PathBuf>,
    },

    /// Dump MCP tool specs as JSON for repository tooling. Internal use.
    #[command(name = "dump-tools", hide = true)]
    DumpTools,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum AuditInclude {
    #[value(name = "dead-code")]
    DeadCode,
}
