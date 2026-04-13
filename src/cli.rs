use clap::{Args, Parser, Subcommand, ValueEnum};

#[allow(dead_code)]
const HELP_BANNER: &str = r#"▓ ▄▄
           ▀▄▄▐█
       ▐▌     █▌    ▓
      ▀▒█▄   ▄██▄▄▄▓▀▀
      ▄▀▀▀▀███▀▀███▌   ▄▄▀
           ██▌   ███▀▀▀▓▄
      ▄▄▄▄▄████▓███▄     ▀        ▄▄    ▄             ▄▄▄▄▄▄▄▄▄▄           ▄
       ▄▀▐▌   ██▌  ▀█▒▄▄     ██▀  █▀   ██▀       ▄▄▓█▀▀▀      ▀▀▀█▓▄      ▓█
              ▓█    ▀        ▐▌ ▄▓▀ ▄▓▀        ▄█▀       ▄         ▀██▄   ▓█
              ▓█             █▄▓█▀▀▀         ▓█▀      ▄  ▀▄▓▀         ▀█▄ ▓█
              ▓█           ▄██▌▄▄▄ ▄▄      ▄█▀         ▀▀▓█     ▌       █▓▓█
              ▓█         ▄▓█▀▀    ▀▀▀     ▐█▀     ▐▌     ▐█    ▓▌▄▒      ███
              ▓█       ▄█▀                ██     ▐▄█▓▄▄▄▓███▓▓█          ▐██
              ▓█     ▄█▀                 ▐█▌          ███▀  ███▄▄▄▄▄      ▓█
              ▓█  ▄▓██▄▄                 ▐█▌          ███▄▄▄███▀▀▀▀▀█▓▄   ▓█
              ▀ ▄▓█▀  ▀▀█▄                █▌     ▀▒▄▓▀▀▀▀███▀▀▓▄      ██  ▓█
               ▓█▀       ▀█▄              ▀█     ▄▀▐▌    ▓▌    █▀▀▀    █▌ ▓█
              ▐█           ▀█▄             ██          ▄▄█▄    ▐▌      █▌ ▓█
              ▓█             ▀█▓▄▄▄▄▄██     ▀█▄       ▀  █▀▓          ▓█  ▓█
              ▓█              ▐██▄            ▀█▄        ▀          ▄█▀   ▓█
              ▐█               ▐▌▀██▓▄▄         ▀██▄▄           ▄▄▓█▀     ▓█
              ▓█▄              ▐█  ▀▄  ▀▓▄          ▀▀▀█▓▓▓▓▓██▀▀         ▓█
             ▀███              ▀█▀ ▐██  ▀█▀                               ▓█
                                                                         ▐█▌
                                             ██▌                         █▌
                                          ▓▓▄   █                      ▄█▀
                                             ▀▀▓▄█▄                  ▄█▀
                                          ▄▄▄▄▄▓▓██▓▄▄           ▄▄▓█▀
                                          ▀▀▀       ▀▀▀█▓▓▓▓▓▓▓█▀▀▀"#;

const HELP_EXAMPLES: &str = "Examples:\n  kg create fridge\n  kg list\n  kg graph fridge node find lodowka\n  kg graph fridge node get concept:refrigerator\n  kg graph fridge kql \"node type=Process\"\n  kg graph fridge stats\n  kg graph fridge quality duplicates";

#[derive(Debug, Parser)]
#[command(
    name = "kg",
    version,
    about = "Knowledge graph CLI for AI chat workflows",
    after_help = HELP_EXAMPLES
)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        help = "Enable event log snapshots for mutating operations (default: off)"
    )]
    pub event_log: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Print init prompts/snippets")]
    Init(InitArgs),
    #[command(about = "Create a new graph")]
    Create { graph_name: String },
    #[command(about = "Compare two graph snapshots/files")]
    Diff {
        left: String,
        right: String,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Merge one graph into another")]
    Merge {
        target: String,
        source: String,
        #[arg(long, value_enum, default_value_t = MergeStrategy::PreferNew)]
        strategy: MergeStrategy,
    },
    #[command(about = "List available graphs")]
    List(ListGraphsArgs),
    #[command(name = "feedback-log", about = "Show recent kg-mcp feedback events")]
    FeedbackLog(FeedbackLogArgs),
    #[command(about = "Run commands against a graph")]
    Graph {
        graph: String,
        #[arg(long)]
        legacy: bool,
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
    #[command(about = "Find, inspect, and edit nodes")]
    Node {
        #[command(subcommand)]
        command: NodeCommand,
    },
    #[command(about = "Add and remove graph edges")]
    Edge {
        #[command(subcommand)]
        command: EdgeCommand,
    },
    #[command(about = "Manage node notes")]
    Note {
        #[command(subcommand)]
        command: NoteCommand,
    },
    #[command(about = "Show graph statistics")]
    Stats(StatsArgs),
    #[command(about = "Run integrity validation checks")]
    Check(CheckArgs),
    #[command(about = "Run deep audit validation checks")]
    Audit(AuditArgs),
    #[command(about = "Run graph quality reports")]
    Quality {
        #[command(subcommand)]
        command: QualityCommand,
    },
    #[command(about = "List nodes missing descriptions")]
    MissingDescriptions(MissingDescriptionsArgs),
    #[command(about = "List nodes missing facts")]
    MissingFacts(MissingFactsArgs),
    #[command(about = "Detect likely duplicate nodes")]
    Duplicates(DuplicatesArgs),
    #[command(about = "Find missing expected edges")]
    EdgeGaps(EdgeGapsArgs),
    #[command(about = "Show top similarity clusters from latest score cache")]
    Clusters(ClustersArgs),
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
    #[command(
        name = "baseline",
        about = "Compute quality and feedback baseline metrics"
    )]
    Baseline(BaselineArgs),
    #[command(
        name = "score-all",
        about = "Compute all-vs-all similarity scores into cache graph"
    )]
    ScoreAll(ScoreAllArgs),
}

#[derive(Debug, Args, Clone)]
pub struct ScoreAllArgs {
    #[arg(long, default_value_t = 120)]
    pub min_desc_len: usize,
    #[arg(long, default_value_t = 0.45)]
    pub desc_weight: f64,
    #[arg(long, default_value_t = 0.55)]
    pub bundle_weight: f64,
    #[arg(long, default_value_t = 42)]
    pub cluster_seed: u64,
    #[arg(long, default_value_t = 1.0)]
    pub cluster_resolution: f64,
    #[arg(long, default_value_t = 5)]
    pub membership_top_k: usize,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ClusterSkill {
    Gardener,
}

#[derive(Debug, Args, Clone)]
pub struct ClustersArgs {
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
    #[arg(long, value_enum)]
    pub skill: Option<ClusterSkill>,
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
    #[arg(help = "KQL query: node type=X, neighbors id=X, path from=X to=Y, count type=X")]
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
    #[command(about = "Search nodes by query")]
    Find {
        queries: Vec<String>,
        #[arg(long, default_value_t = 5)]
        limit: usize,
        #[arg(long, value_enum, default_value_t = FindMode::Hybrid)]
        mode: FindMode,
        #[arg(long)]
        full: bool,
        #[arg(long = "output-size")]
        output_size: Option<usize>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        debug_score: bool,
        #[arg(long)]
        include_metadata: bool,
        /// Optional tuning weights for find ranking, e.g. bm25=0.6,fuzzy=0.3,vector=0.1
        #[arg(long)]
        tune: Option<String>,
        /// Query vector for --mode vector (comma-separated f32 values)
        #[arg(long, value_delimiter = ',')]
        vector_query: Option<Vec<f32>>,
    },
    #[command(about = "Get a node by ID")]
    Get {
        id: String,
        #[arg(long)]
        full: bool,
        #[arg(long = "output-size")]
        output_size: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    #[command(about = "Add a new node")]
    Add(AddNodeArgs),
    #[command(about = "Modify an existing node")]
    Modify(ModifyNodeArgs),
    #[command(about = "Rename a node ID")]
    Rename { from: String, to: String },
    #[command(about = "Remove a node and its incident edges")]
    Remove { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum FindMode {
    Fuzzy,
    Bm25,
    Hybrid,
    Vector,
}

#[derive(Debug, Subcommand)]
pub enum EdgeCommand {
    #[command(about = "Add an edge between two nodes")]
    Add(AddEdgeArgs),
    #[command(name = "add-batch", about = "Add multiple edges from CSV/JSON")]
    AddBatch(AddEdgeBatchArgs),
    #[command(about = "Remove an edge between two nodes")]
    Remove(RemoveEdgeArgs),
}

#[derive(Debug, Subcommand)]
pub enum NoteCommand {
    #[command(about = "Add a note to a node")]
    Add(NoteAddArgs),
    #[command(about = "List notes")]
    List(NoteListArgs),
    #[command(about = "Remove a note")]
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
    #[command(
        name = "missing-descriptions",
        about = "List nodes missing descriptions"
    )]
    MissingDescriptions(MissingDescriptionsArgs),
    #[command(name = "missing-facts", about = "List nodes missing facts")]
    MissingFacts(MissingFactsArgs),
    #[command(about = "Detect likely duplicate nodes")]
    Duplicates(DuplicatesArgs),
    #[command(name = "edge-gaps", about = "Find missing expected edges")]
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

#[derive(Debug, Args, Clone)]
pub struct BaselineArgs {
    #[arg(long, default_value_t = 5)]
    pub find_limit: usize,
    #[arg(long)]
    pub include_features: bool,
    #[arg(long, value_enum, default_value_t = FindMode::Fuzzy)]
    pub mode: FindMode,
    #[arg(long)]
    pub golden: Option<String>,
    #[arg(long)]
    pub json: bool,
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
    #[arg(long, default_value_t = 0.5)]
    pub importance: f64,
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
    #[arg(long)]
    pub importance: Option<f64>,
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
pub struct AddEdgeBatchArgs {
    #[arg(help = "CSV file with columns: source_id,relation,target_id,detail")]
    pub file: String,
}

#[derive(Debug, Args)]
pub struct RemoveEdgeArgs {
    pub source_id: String,
    pub relation: String,
    pub target_id: String,
}
