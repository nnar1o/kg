use std::path::Path;

use anyhow::Result;

use crate::access_log;
use crate::cli::{
    AccessLogArgs, AsOfArgs, DiffAsOfArgs, ExportDotArgs, ExportGraphmlArgs, ExportHtmlArgs,
    ExportJsonArgs, ExportMdArgs, ExportMermaidArgs, HistoryArgs, ImportCsvArgs, ImportJsonArgs,
    ImportMarkdownArgs, SplitArgs, TimelineArgs, VectorCommand,
};
use crate::graph::GraphFile;
use crate::schema::GraphSchema;
use crate::storage::GraphStore;

pub(crate) struct GraphTransferContext<'a> {
    pub(crate) cwd: &'a Path,
    pub(crate) graph_name: &'a str,
    pub(crate) path: &'a Path,
    pub(crate) graph_file: &'a mut GraphFile,
    pub(crate) schema: Option<&'a GraphSchema>,
    pub(crate) store: &'a dyn GraphStore,
}

pub(crate) fn execute_export_html(
    graph_name: &str,
    graph_file: &GraphFile,
    args: ExportHtmlArgs,
) -> Result<String> {
    let ExportHtmlArgs { output, title } = args;
    crate::export_html::export_html(
        graph_file,
        graph_name,
        crate::export_html::ExportHtmlOptions {
            output: output.as_deref(),
            title: title.as_deref(),
        },
    )
}

pub(crate) fn execute_access_log(path: &Path, args: AccessLogArgs) -> Result<String> {
    Ok(access_log::read_log(path, args.limit, args.show_empty)?)
}

pub(crate) fn execute_access_stats(path: &Path) -> Result<String> {
    Ok(access_log::log_stats(path)?)
}

pub(crate) fn execute_import_csv(
    context: GraphTransferContext<'_>,
    args: ImportCsvArgs,
) -> Result<String> {
    crate::import_graph_csv(
        context.path,
        context.graph_name,
        context.graph_file,
        context.store,
        &args,
        context.schema,
    )
}

pub(crate) fn execute_import_markdown(
    context: GraphTransferContext<'_>,
    args: ImportMarkdownArgs,
) -> Result<String> {
    crate::import_graph_markdown(
        context.path,
        context.graph_name,
        context.graph_file,
        context.store,
        &args,
        context.schema,
    )
}

pub(crate) fn execute_export_json(
    graph_name: &str,
    graph_file: &GraphFile,
    args: ExportJsonArgs,
) -> Result<String> {
    crate::export_graph_json(graph_name, graph_file, args.output.as_deref())
}

pub(crate) fn execute_import_json(
    path: &Path,
    graph_name: &str,
    store: &dyn GraphStore,
    args: ImportJsonArgs,
) -> Result<String> {
    crate::import_graph_json(path, graph_name, &args.input, store)
}

pub(crate) fn execute_export_dot(
    graph_name: &str,
    graph_file: &GraphFile,
    args: ExportDotArgs,
) -> Result<String> {
    crate::export_graph_dot(graph_name, graph_file, &args)
}

pub(crate) fn execute_export_mermaid(
    graph_name: &str,
    graph_file: &GraphFile,
    args: ExportMermaidArgs,
) -> Result<String> {
    crate::export_graph_mermaid(graph_name, graph_file, &args)
}

pub(crate) fn execute_export_graphml(
    graph_name: &str,
    graph_file: &GraphFile,
    args: ExportGraphmlArgs,
) -> Result<String> {
    crate::export_graph_graphml(graph_name, graph_file, &args)
}

pub(crate) fn execute_export_md(
    context: GraphTransferContext<'_>,
    args: ExportMdArgs,
) -> Result<String> {
    crate::export_graph_md(context.graph_name, context.graph_file, &args, context.cwd)
}

pub(crate) fn execute_split(
    graph_name: &str,
    graph_file: &GraphFile,
    args: SplitArgs,
) -> Result<String> {
    crate::split_graph(graph_name, graph_file, &args)
}

pub(crate) fn execute_vector(
    context: GraphTransferContext<'_>,
    command: VectorCommand,
) -> Result<String> {
    crate::handle_vector_command(
        context.path,
        context.graph_name,
        context.graph_file,
        &command,
        context.cwd,
    )
}

pub(crate) fn execute_as_of(path: &Path, graph_name: &str, args: AsOfArgs) -> Result<String> {
    crate::export_graph_as_of(path, graph_name, &args)
}

pub(crate) fn execute_history(path: &Path, graph_name: &str, args: HistoryArgs) -> Result<String> {
    Ok(crate::render_graph_history(path, graph_name, &args)?)
}

pub(crate) fn execute_timeline(
    path: &Path,
    graph_name: &str,
    args: TimelineArgs,
) -> Result<String> {
    Ok(crate::render_graph_timeline(path, graph_name, &args)?)
}

pub(crate) fn execute_diff_as_of(
    path: &Path,
    graph_name: &str,
    args: DiffAsOfArgs,
) -> Result<String> {
    Ok(if args.json {
        crate::render_graph_diff_as_of_json(path, graph_name, &args)?
    } else {
        crate::render_graph_diff_as_of(path, graph_name, &args)?
    })
}
