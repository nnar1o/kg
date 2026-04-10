use crate::cli::{InitArgs, InitTarget};

pub fn render_init(args: &InitArgs) -> String {
    match args.target {
        InitTarget::Cli => render_cli(),
        InitTarget::Mcp => render_mcp(args.client.as_deref()),
        InitTarget::Doc => render_doc_skill(),
    }
}

fn render_cli() -> String {
    [
        "# kg init: cli",
        "",
        "1) create graph",
        "   kg create <graph>",
        "",
        "2) add nodes and edges",
        "   kg <graph> node add concept:example --type Concept --name \"Example\" --source doc.md",
        "   kg <graph> edge add concept:example USES interface:example_api",
        "",
        "3) validate",
        "   kg <graph> check",
        "   kg <graph> stats --by-type --by-relation",
        "",
        "4) explore",
        "   kg <graph> node find <query>",
        "   kg <graph> node get <id>",
        "",
        "Tip: run `kg init --target doc` for the doc->graph skill text.",
        "",
    ]
    .join("\n")
}

fn render_mcp(client: Option<&str>) -> String {
    let header = "# kg init: mcp";
    let mut lines = vec![header.to_string(), "".to_string()];

    if let Some(name) = client {
        lines.push(format!("Client: {name}"));
        lines.push("".to_string());
    }

    lines.extend([
        "Use the kg-mcp server to expose kg tools to your client.",
        "",
        "Recommended usage in LLM prompts:",
        "- Prefer the single `kg` tool for multi-command scripts.",
        "- Include feedback lines before new searches when possible.",
        "",
        "Example script:",
        "  uid=ab12cd YES; fridge node find \"smart fridge\"; fridge node get concept:refrigerator",
        "",
        "If your client supports multiple tools, you can also call node/edge tools directly.",
        "",
    ]
    .iter()
    .map(|line| line.to_string()));

    lines.join("\n")
}

fn render_doc_skill() -> String {
    [
        "# kg init: doc -> graph",
        "",
        "The legacy bundled skill file was removed from the repository.",
        "Use docs in `docs/mcp.md` and `docs/node-and-edge-fields-reference.md`",
        "as the starting prompt/context for doc-to-graph extraction workflows.",
        "",
    ]
    .join("\n")
}
