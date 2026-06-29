use crate::cli::FactsArgs;
use crate::graph::GraphFile;
use crate::output::render_facts;

pub(crate) fn execute_facts(graph: &GraphFile, args: &FactsArgs) -> String {
    render_facts(graph, &args.text, args.limit, args.json)
}
