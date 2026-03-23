use crate::cli::{InitArgs, InitTarget};

const DOC_SKILL: &str = include_str!("../skills/kg-builder/SKILL.md");

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
    let mut out = String::new();
    out.push_str("# kg init: doc -> graph\n\n");
    out.push_str("Copy the skill text below into your client:\n\n");
    out.push_str(DOC_SKILL);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
