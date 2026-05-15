use crate::cli::AnnotateArgs;
use crate::graph::GraphFile;
use crate::output::render_annotate;

pub(crate) fn execute_annotate(graph: &GraphFile, args: &AnnotateArgs) -> String {
    render_annotate(graph, &args.text)
}
