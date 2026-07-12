mod graph;
mod maintenance;
mod mutation;
mod retrieval;
mod shared;

pub(crate) use graph::*;
pub(crate) use maintenance::*;
pub(crate) use mutation::*;
pub(crate) use retrieval::*;
pub(crate) use shared::*;

use anyhow::Result;
use clap::CommandFactory;
use lexa::engine::{ContextOptions, FileFilterOptions, SearchOptions};

use crate::cli::{Cli, Commands};
use crate::cli_upgrade;

pub(crate) fn run(cli: &Cli) -> Result<()> {
    if cli.version {
        return cli_upgrade::cmd_version(false);
    }

    let Some(command) = &cli.command else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };

    match command {
        Commands::Index { path, output } => cmd_index(path, output.as_ref(), cli),
        Commands::Reindex { path } => cmd_reindex(path, cli),
        Commands::ClearIndex => cmd_clear_index(cli),
        Commands::Files {
            path,
            path_glob,
            language,
            min_lines,
            max_lines,
            max_results,
        } => cmd_tree(
            FileFilterOptions {
                path_prefix: (!path.is_empty()).then(|| path.clone()),
                path_glob: path_glob.clone(),
                language: language.clone(),
                min_lines: *min_lines,
                max_lines: *max_lines,
                max_results: *max_results,
            },
            cli,
        ),
        Commands::List { path } => cmd_ls(path, cli),
        Commands::PathSearch {
            pattern,
            query,
            max,
            max_results,
        } => cmd_find(
            &required_text(pattern.as_deref(), query.as_deref(), "path-search", "query")?,
            max_limit(*max, *max_results, 20)?,
            cli,
        ),
        Commands::TextSearch {
            query,
            query_flag,
            max,
            max_results,
            regex,
            scope,
            compact,
            paths_only,
            path_glob,
        } => cmd_search(
            &required_text(
                query.as_deref(),
                query_flag.as_deref(),
                "text-search",
                "query",
            )?,
            SearchOptions {
                max_results: max_limit(*max, *max_results, 20)?,
                regex: *regex,
                scope: *scope,
                compact: *compact,
                paths_only: *paths_only,
                path_glob: path_glob.clone(),
            },
            cli,
        ),
        Commands::Outline { path } => cmd_outline(path, cli),
        Commands::SymbolDefs { name } => cmd_symbol(name, cli),
        Commands::SymbolSearch {
            query,
            query_flag,
            max,
            max_results,
        } => cmd_symbol_search(
            &required_text(
                query.as_deref(),
                query_flag.as_deref(),
                "symbol-search",
                "query",
            )?,
            max_limit(*max, *max_results, 20)?,
            cli,
        ),
        Commands::WordRefs {
            word,
            max,
            max_results,
            cursor,
            path_prefix,
            path,
            path_glob,
        } => cmd_word(
            word,
            max_limit(*max, *max_results, 50)?,
            *cursor,
            path_prefix.as_deref().or(path.as_deref()),
            path_glob.as_deref(),
            cli,
        ),
        Commands::Deps {
            path,
            reverse,
            transitive,
        } => cmd_deps(path, *reverse, *transitive, cli),
        Commands::Recent { limit } => cmd_hot(*limit, cli),
        Commands::Callers {
            name,
            query,
            max,
            max_results,
        } => cmd_callers(
            &required_text(name.as_deref(), query.as_deref(), "callers", "name")?,
            max_limit(*max, *max_results, 20)?,
            cli,
        ),
        Commands::Brief {
            task,
            query,
            max,
            max_results,
            path_prefix,
            path_glob,
            language,
        } => cmd_context(
            &required_text(task.as_deref(), query.as_deref(), "brief", "task")?,
            ContextOptions {
                max_results: max_limit(*max, *max_results, 10)?,
                path_prefix: path_prefix.clone(),
                path_glob: path_glob.clone(),
                language: language.clone(),
            },
            cli,
        ),
        Commands::Changes { since } => cmd_changes(*since, cli),
        Commands::Read {
            path,
            line_range,
            line_start,
            line_end,
            compact,
            if_hash,
            hash,
        } => {
            let (line_start, line_end) =
                resolve_line_range(line_range.as_deref(), *line_start, *line_end)?;
            cmd_read(
                path,
                line_start,
                line_end,
                *compact,
                if_hash.as_deref(),
                *hash,
                cli,
            )
        }
        Commands::Patch {
            path,
            op,
            line_range,
            after,
            replace_text,
            anchor,
            placement,
            preview,
            content,
            content_file,
            if_hash,
            dry_run,
        } => cmd_edit(
            path,
            *op,
            line_range.as_deref(),
            *after,
            replace_text.as_deref(),
            anchor.as_deref(),
            *placement,
            *preview,
            content.as_deref(),
            content_file.as_ref(),
            if_hash.as_deref(),
            *dry_run,
            cli,
        ),
        Commands::Create {
            path,
            content,
            content_file,
            overwrite,
            dry_run,
        } => cmd_create(
            path,
            content.as_deref(),
            content_file.as_ref(),
            *overwrite,
            *dry_run,
            cli,
        ),
        Commands::Glob { pattern } => cmd_glob(pattern, cli),
        Commands::Status => cmd_status(cli),
        Commands::Audit {
            max,
            since,
            strict,
            config,
            no_config,
            include,
        } => cmd_audit(
            *max,
            since.as_deref(),
            *strict,
            config.as_ref(),
            *no_config,
            include,
            cli,
        ),
        Commands::Upgrade {
            version,
            install_dir,
        } => cli_upgrade::cmd_upgrade(version, install_dir.as_ref(), false),
        Commands::Watch { path, debounce } => cmd_watch(path, *debounce, cli),
        Commands::Pipeline { pipeline } => cmd_pipeline(pipeline, cli),
        Commands::Mcp {
            path,
            no_refresh,
            debounce,
            structured_content: _,
            log_file,
        } => cmd_mcp(path, *no_refresh, *debounce, log_file.as_ref(), cli),
        Commands::DumpTools => cmd_dump_tools(),
    }
}
