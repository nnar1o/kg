use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "kg",
    version,
    about = "Knowledge graph CLI for AI chat workflows",
    after_help = "Examples:\n  kg create fridge\n  kg fridge node find lodowka\n  kg fridge node get concept:refrigerator\n  kg graph fridge stats\n  kg graph fridge quality duplicates"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Create {
        graph_name: String,
    },
    Graph {
        graph: String,
        #[command(subcommand)]
        command: GraphCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum GraphCommand {
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    Edge {
        #[command(subcommand)]
        command: EdgeCommand,
    },
    Stats(StatsArgs),
    Check(CheckArgs),
    Audit(AuditArgs),
    Quality {
        #[command(subcommand)]
        command: QualityCommand,
    },
    MissingDescriptions(MissingDescriptionsArgs),
    MissingFacts(MissingFactsArgs),
    Duplicates(DuplicatesArgs),
    EdgeGaps(EdgeGapsArgs),
    #[command(
        name = "export-html",
        about = "Export graph as interactive HTML visualization"
    )]
    ExportHtml(ExportHtmlArgs),
}

#[derive(Debug, Args)]
pub struct ExportHtmlArgs {
    /// Output file path (default: <graph_name>.html)
    #[arg(long, short)]
    pub output: Option<String>,
    /// Page title override
    #[arg(long)]
    pub title: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum NodeCommand {
    Find {
        queries: Vec<String>,
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(long)]
        include_features: bool,
        #[arg(long)]
        full: bool,
    },
    Get {
        id: String,
        #[arg(long)]
        include_features: bool,
        #[arg(long)]
        full: bool,
    },
    Add(AddNodeArgs),
    Modify(ModifyNodeArgs),
    Remove {
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum EdgeCommand {
    Add(AddEdgeArgs),
    Remove(RemoveEdgeArgs),
}

#[derive(Debug, Subcommand)]
pub enum QualityCommand {
    #[command(name = "missing-descriptions")]
    MissingDescriptions(MissingDescriptionsArgs),
    #[command(name = "missing-facts")]
    MissingFacts(MissingFactsArgs),
    Duplicates(DuplicatesArgs),
    #[command(name = "edge-gaps")]
    EdgeGaps(EdgeGapsArgs),
}

#[derive(Debug, Clone, ValueEnum)]
pub enum MissingFactsSort {
    Edges,
    Id,
}

#[derive(Debug, Args)]
pub struct StatsArgs {
    #[arg(long)]
    pub include_features: bool,
    #[arg(long)]
    pub by_type: bool,
    #[arg(long)]
    pub by_relation: bool,
    #[arg(long)]
    pub show_sources: bool,
}

#[derive(Debug, Args)]
pub struct AuditArgs {
    #[arg(long)]
    pub deep: bool,
    #[arg(long)]
    pub base_dir: Option<String>,
    #[arg(long)]
    pub errors_only: bool,
    #[arg(long)]
    pub warnings_only: bool,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(long)]
    pub deep: bool,
    #[arg(long)]
    pub base_dir: Option<String>,
    #[arg(long)]
    pub errors_only: bool,
    #[arg(long)]
    pub warnings_only: bool,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Debug, Args, Clone)]
pub struct MissingDescriptionsArgs {
    #[arg(long, default_value_t = 30)]
    pub limit: usize,
    #[arg(long = "type")]
    pub node_types: Vec<String>,
    #[arg(long)]
    pub include_features: bool,
}

#[derive(Debug, Args, Clone)]
pub struct MissingFactsArgs {
    #[arg(long, default_value_t = 30)]
    pub limit: usize,
    #[arg(long = "type")]
    pub node_types: Vec<String>,
    #[arg(long)]
    pub include_features: bool,
    #[arg(long, value_enum, default_value_t = MissingFactsSort::Edges)]
    pub sort: MissingFactsSort,
}

#[derive(Debug, Args, Clone)]
pub struct DuplicatesArgs {
    #[arg(long = "type")]
    pub node_types: Vec<String>,
    #[arg(long, default_value_t = 0.85)]
    pub threshold: f64,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    #[arg(long)]
    pub include_features: bool,
}

#[derive(Debug, Args, Clone)]
pub struct EdgeGapsArgs {
    #[arg(long = "type")]
    pub node_types: Vec<String>,
    #[arg(long)]
    pub relation: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Debug, Args)]
pub struct AddNodeArgs {
    pub id: String,
    #[arg(long = "type")]
    pub node_type: String,
    #[arg(long)]
    pub name: String,
    #[arg(long, default_value = "")]
    pub description: String,
    #[arg(long, default_value = "")]
    pub domain_area: String,
    #[arg(long, default_value = "")]
    pub provenance: String,
    #[arg(long)]
    pub confidence: Option<f64>,
    #[arg(long, default_value = "")]
    pub created_at: String,
    #[arg(long = "fact")]
    pub fact: Vec<String>,
    #[arg(long = "alias")]
    pub alias: Vec<String>,
    #[arg(long = "source")]
    pub source: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ModifyNodeArgs {
    pub id: String,
    #[arg(long = "type")]
    pub node_type: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long)]
    pub domain_area: Option<String>,
    #[arg(long)]
    pub provenance: Option<String>,
    #[arg(long)]
    pub confidence: Option<f64>,
    #[arg(long)]
    pub created_at: Option<String>,
    #[arg(long = "fact")]
    pub fact: Vec<String>,
    #[arg(long = "alias")]
    pub alias: Vec<String>,
    #[arg(long = "source")]
    pub source: Vec<String>,
}

#[derive(Debug, Args)]
pub struct AddEdgeArgs {
    pub source_id: String,
    pub relation: String,
    pub target_id: String,
    #[arg(long, default_value = "")]
    pub detail: String,
}

#[derive(Debug, Args)]
pub struct RemoveEdgeArgs {
    pub source_id: String,
    pub relation: String,
    pub target_id: String,
}
