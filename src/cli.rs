use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "kg",
    version,
    about = "Knowledge graph CLI for AI chat workflows",
    after_help = "Examples:\n  kg create fridge\n  kg list\n  kg fridge node find lodowka\n  kg fridge node get concept:refrigerator\n  kg fridge list --type Process\n  kg graph fridge stats\n  kg graph fridge quality duplicates"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Print init prompts/snippets")]
    Init(InitArgs),
    Create {
        graph_name: String,
    },
    Diff {
        left: String,
        right: String,
        #[arg(long)]
        json: bool,
    },
    Merge {
        target: String,
        source: String,
        #[arg(long, value_enum, default_value_t = MergeStrategy::PreferNew)]
        strategy: MergeStrategy,
    },
    List(ListGraphsArgs),
    #[command(name = "feedback-log", about = "Show recent kg-mcp feedback events")]
    FeedbackLog(FeedbackLogArgs),
    Graph {
        graph: String,
        #[command(subcommand)]
        command: GraphCommand,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MergeStrategy {
    PreferNew,
    PreferOld,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long, value_enum, default_value_t = InitTarget::Cli)]
    pub target: InitTarget,
    #[arg(long)]
    pub client: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum InitTarget {
    Cli,
    Mcp,
    Doc,
}

#[derive(Debug, Args)]
pub struct ListGraphsArgs {
    #[arg(long)]
    pub full: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct FeedbackLogArgs {
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    #[arg(long)]
    pub uid: Option<String>,
    #[arg(long)]
    pub graph: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct FeedbackSummaryArgs {
    #[arg(long, default_value_t = 30)]
    pub limit: usize,
    pub graph: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TemporalSource {
    Auto,
    Backups,
    EventLog,
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
    Note {
        #[command(subcommand)]
        command: NoteCommand,
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
    #[command(name = "access-log", about = "Show recent search/access history")]
    AccessLog(AccessLogArgs),
    #[command(name = "access-stats", about = "Show access log statistics")]
    AccessStats(AccessStatsArgs),
    #[command(name = "import-csv", about = "Import nodes/edges/notes from CSV")]
    ImportCsv(ImportCsvArgs),
    #[command(name = "import-md", about = "Import nodes/notes from Markdown")]
    ImportMarkdown(ImportMarkdownArgs),
    #[command(name = "kql", about = "Query graph with KQL")]
    Kql(KqlArgs),
    #[command(name = "export-json", about = "Export graph to JSON file")]
    ExportJson(ExportJsonArgs),
    #[command(name = "import-json", about = "Import graph from JSON file")]
    ImportJson(ImportJsonArgs),
    #[command(name = "export-dot", about = "Export graph as Graphviz DOT")]
    ExportDot(ExportDotArgs),
    #[command(name = "export-mermaid", about = "Export graph as Mermaid")]
    ExportMermaid(ExportMermaidArgs),
    #[command(name = "export-graphml", about = "Export graph as GraphML")]
    ExportGraphml(ExportGraphmlArgs),
    #[command(name = "export-md", about = "Export graph as Markdown folder")]
    ExportMd(ExportMdArgs),
    #[command(name = "split", about = "Split graph into separate files")]
    Split(SplitArgs),
    #[command(name = "vectors", about = "Import/query vectors for semantic search")]
    Vector {
        #[command(subcommand)]
        command: VectorCommand,
    },
    #[command(name = "as-of", about = "Export graph snapshot by timestamp")]
    AsOf(AsOfArgs),
    #[command(name = "history", about = "List graph backup snapshots")]
    History(HistoryArgs),
    #[command(name = "timeline", about = "List event log snapshots")]
    Timeline(TimelineArgs),
    #[command(name = "diff-as-of", about = "Diff two snapshots by timestamp")]
    DiffAsOf(DiffAsOfArgs),
    #[command(name = "feedback-summary", about = "Human-readable feedback summary")]
    FeedbackSummary(FeedbackSummaryArgs),
    List(ListNodesArgs),
}

#[derive(Debug, Args)]
pub struct AccessLogArgs {
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    #[arg(long)]
    pub show_empty: bool,
}

#[derive(Debug, Args)]
pub struct AccessStatsArgs {}

#[derive(Debug, Args)]
pub struct ImportCsvArgs {
    #[arg(long)]
    pub nodes: Option<String>,
    #[arg(long)]
    pub edges: Option<String>,
    #[arg(long)]
    pub notes: Option<String>,
    #[arg(long, value_enum, default_value_t = MergeStrategy::PreferNew)]
    pub strategy: MergeStrategy,
}

#[derive(Debug, Args)]
pub struct ImportMarkdownArgs {
    #[arg(long)]
    pub path: String,
    #[arg(long)]
    pub notes_as_nodes: bool,
    #[arg(long, value_enum, default_value_t = MergeStrategy::PreferNew)]
    pub strategy: MergeStrategy,
}

#[derive(Debug, Args)]
pub struct KqlArgs {
    pub query: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ExportJsonArgs {
    #[arg(long, short)]
    pub output: Option<String>,
}

#[derive(Debug, Args)]
pub struct ImportJsonArgs {
    #[arg(long)]
    pub input: String,
}

#[derive(Debug, Args)]
pub struct ExportDotArgs {
    #[arg(long, short)]
    pub output: Option<String>,
    #[arg(long)]
    pub focus: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long = "type")]
    pub node_types: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ExportMermaidArgs {
    #[arg(long, short)]
    pub output: Option<String>,
    #[arg(long)]
    pub focus: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long = "type")]
    pub node_types: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ExportGraphmlArgs {
    #[arg(long, short)]
    pub output: Option<String>,
    #[arg(long)]
    pub focus: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long = "type")]
    pub node_types: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ExportMdArgs {
    #[arg(long, short)]
    pub output: Option<String>,
    #[arg(long)]
    pub focus: Option<String>,
    #[arg(long, default_value_t = 1)]
    pub depth: usize,
    #[arg(long = "type")]
    pub node_types: Vec<String>,
}

#[derive(Debug, Args)]
pub struct SplitArgs {
    #[arg(long, short)]
    pub output: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum VectorCommand {
    #[command(name = "import", about = "Import vectors from JSONL file")]
    Import(VectorImportArgs),
    #[command(name = "stats", about = "Show vector store statistics")]
    Stats(VectorStatsArgs),
}

#[derive(Debug, Args)]
pub struct VectorStatsArgs {}

#[derive(Debug, Args)]
pub struct VectorImportArgs {
    #[arg(long, short)]
    pub input: String,
}

#[derive(Debug, Args)]
pub struct AsOfArgs {
    #[arg(long)]
    pub ts_ms: u64,
    #[arg(long, short)]
    pub output: Option<String>,
    #[arg(long, value_enum, default_value_t = TemporalSource::Auto)]
    pub source: TemporalSource,
}

#[derive(Debug, Args)]
pub struct HistoryArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct TimelineArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub since_ts_ms: Option<u64>,
    #[arg(long)]
    pub until_ts_ms: Option<u64>,
}

#[derive(Debug, Args)]
pub struct DiffAsOfArgs {
    #[arg(long)]
    pub from_ts_ms: u64,
    #[arg(long)]
    pub to_ts_ms: u64,
    #[arg(long, value_enum, default_value_t = TemporalSource::Auto)]
    pub source: TemporalSource,
    #[arg(long)]
    pub json: bool,
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
        #[arg(long, value_enum, default_value_t = FindMode::Fuzzy)]
        mode: FindMode,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
        /// Query vector for --mode vector (comma-separated f32 values)
        #[arg(long, value_delimiter = ',')]
        vector_query: Option<Vec<f32>>,
    },
    Get {
        id: String,
        #[arg(long)]
        include_features: bool,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
    },
    Add(AddNodeArgs),
    Modify(ModifyNodeArgs),
    Rename {
        from: String,
        to: String,
    },
    Remove {
        id: String,
    },
    List(ListNodesArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum FindMode {
    Fuzzy,
    Bm25,
    Vector,
}

#[derive(Debug, Args, Clone)]
pub struct ListNodesArgs {
    #[arg(long = "type")]
    pub node_types: Vec<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub include_features: bool,
    #[arg(long)]
    pub full: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Subcommand)]
pub enum EdgeCommand {
    Add(AddEdgeArgs),
    Remove(RemoveEdgeArgs),
}

#[derive(Debug, Subcommand)]
pub enum NoteCommand {
    Add(NoteAddArgs),
    List(NoteListArgs),
    Remove { id: String },
}

#[derive(Debug, Args)]
pub struct NoteAddArgs {
    pub node_id: String,
    #[arg(long)]
    pub text: String,
    #[arg(long)]
    pub tag: Vec<String>,
    #[arg(long)]
    pub author: Option<String>,
    #[arg(long)]
    pub created_at: Option<String>,
    #[arg(long)]
    pub provenance: Option<String>,
    #[arg(long)]
    pub source: Vec<String>,
    #[arg(long)]
    pub id: Option<String>,
}

#[derive(Debug, Args)]
pub struct NoteListArgs {
    #[arg(long)]
    pub node: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
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
    #[arg(long)]
    pub json: bool,
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
    #[arg(long)]
    pub json: bool,
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
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args, Clone)]
pub struct EdgeGapsArgs {
    #[arg(long = "type")]
    pub node_types: Vec<String>,
    #[arg(long)]
    pub relation: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
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
