#![allow(clippy::unnecessary_sort_by)]

use crate::graph::{GraphFile, Note};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub struct ExportHtmlOptions<'a> {
    pub output: Option<&'a str>,
    pub title: Option<&'a str>,
}

pub fn export_html(graph: &GraphFile, graph_name: &str, opts: ExportHtmlOptions) -> Result<String> {
    let output_path = match opts.output {
        Some(p) => p.to_string(),
        None => format!("{graph_name}.html"),
    };
    let title = opts
        .title
        .unwrap_or(&graph.metadata.name)
        .replace('"', "&quot;");

    let html = render_html(graph, &title);
    fs::write(Path::new(&output_path), html)?;
    Ok(format!("+ exported {output_path}\n"))
}

fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn format_note(note: &Note) -> String {
    let mut out = String::new();
    if !note.created_at.is_empty() {
        out.push_str(&note.created_at);
        out.push(' ');
    }
    if !note.author.is_empty() {
        out.push_str(&note.author);
        out.push_str(": ");
    }
    out.push_str(&note.body);
    if !note.tags.is_empty() {
        out.push_str(" [");
        out.push_str(&note.tags.join(", "));
        out.push(']');
    }
    out
}

fn node_color(node_type: &str) -> &'static str {
    match node_type {
        "Concept" => "#4A90D9",
        "Process" => "#7ED321",
        "DataStore" => "#F5A623",
        "Interface" => "#BD10E0",
        "Rule" => "#D0021B",
        "Feature" => "#50E3C2",
        "Decision" => "#B8E986",
        "Convention" => "#9B9B9B",
        "Note" => "#F8E71C",
        "Bug" => "#FF6B6B",
        _ => "#CCCCCC",
    }
}

fn render_html(graph: &GraphFile, title: &str) -> String {
    let mut notes_by_node: HashMap<&str, Vec<String>> = HashMap::new();
    for note in &graph.notes {
        notes_by_node
            .entry(note.node_id.as_str())
            .or_default()
            .push(format_note(note));
    }

    let nodes_js = graph
        .nodes
        .iter()
        .map(|n| {
            let id = escape_js_string(&n.id);
            let name = escape_js_string(&n.name);
            let ntype = escape_js_string(&n.r#type);
            let desc = escape_js_string(&n.properties.description);
            let facts_js = n
                .properties
                .key_facts
                .iter()
                .map(|f| format!("\"{}\"", escape_js_string(f)))
                .collect::<Vec<_>>()
                .join(",");
            let aliases_js = n
                .properties
                .alias
                .iter()
                .map(|a| format!("\"{}\"", escape_js_string(a)))
                .collect::<Vec<_>>()
                .join(",");
            let sources_js = n
                .source_files
                .iter()
                .map(|s| format!("\"{}\"", escape_js_string(s)))
                .collect::<Vec<_>>()
                .join(",");
            let notes_js = notes_by_node
                .get(n.id.as_str())
                .map(|notes| {
                    notes
                        .iter()
                        .map(|note| format!("\"{}\"", escape_js_string(note)))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            let confidence = n
                .properties
                .confidence
                .map(|c| format!("{c:.2}"))
                .unwrap_or_default();
            let color = node_color(&n.r#type);
            format!(
                r#"{{data:{{id:"{id}",label:"{name}",type:"{ntype}",desc:"{desc}",facts:[{facts_js}],aliases:[{aliases_js}],sources:[{sources_js}],notes:[{notes_js}],confidence:"{confidence}",color:"{color}"}}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let edges_js = graph
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let src = escape_js_string(&e.source_id);
            let tgt = escape_js_string(&e.target_id);
            let rel = escape_js_string(&e.relation);
            let detail = escape_js_string(&e.properties.detail);
            format!(
                r#"{{data:{{id:"e{i}",source:"{src}",target:"{tgt}",relation:"{rel}",detail:"{detail}"}}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let mut types: Vec<String> = graph
        .nodes
        .iter()
        .map(|n| n.r#type.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    types.sort();

    let mut relations: Vec<String> = graph
        .edges
        .iter()
        .map(|e| e.relation.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    relations.sort();

    let type_checkboxes = types
        .iter()
        .map(|t| {
            let color = node_color(t);
            format!(
                r#"<label class="filter-label"><input type="checkbox" class="type-filter" value="{t}" checked> <span class="dot" style="background:{color}"></span>{t}</label>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let rel_checkboxes = relations
        .iter()
        .map(|r| {
            format!(
                r#"<label class="filter-label rel-filter-label"><input type="checkbox" class="rel-filter" value="{r}" checked> <span class="rel-tag">{r}</span></label>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut degree_map: HashMap<&str, usize> = HashMap::new();
    for n in &graph.nodes {
        degree_map.insert(n.id.as_str(), 0);
    }
    for e in &graph.edges {
        *degree_map.entry(&e.source_id).or_insert(0) += 1;
        *degree_map.entry(&e.target_id).or_insert(0) += 1;
    }

    let mut hubs: Vec<(&str, usize)> = graph
        .nodes
        .iter()
        .map(|n| {
            let deg = *degree_map.get(n.id.as_str()).unwrap_or(&0);
            (n.id.as_str(), deg)
        })
        .collect();
    hubs.sort_by(|a, b| b.1.cmp(&a.1));

    let top_hubs_js: String = hubs
        .into_iter()
        .take(10)
        .map(|(id, deg)| {
            let node = graph.nodes.iter().find(|n| n.id.as_str() == id).unwrap();
            let color = node_color(&node.r#type);
            let label = escape_js_string(&node.name);
            let nid = escape_js_string(id);
            let ntype = escape_js_string(&node.r#type);
            format!(
                r#"{{id:"{nid}",label:"{label}",type_:"{ntype}",color:"{color}",degree:{deg}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let graph_name_escaped = escape_js_string(&graph.metadata.name);
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<script src="https://unpkg.com/cytoscape@3.29.2/dist/cytoscape.min.js"></script>
<script src="https://unpkg.com/cytoscape-navigator@2.0.0/cytoscape-navigator.js"></script>
<style>
*,*::before,*::after{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:#0f1117;color:#e0e0e0;height:100vh;display:flex;flex-direction:column;overflow:hidden}}

/* Top bar */
#topbar{{display:flex;align-items:center;gap:10px;padding:8px 16px;background:#1a1d27;border-bottom:1px solid #2a2d3a;flex-shrink:0}}
#topbar h1{{font-size:15px;font-weight:600;color:#a0cfff;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:200px;flex-shrink:0}}
#search{{flex:1;max-width:260px;padding:6px 12px;border-radius:6px;border:1px solid #3a3d4a;background:#252833;color:#e0e0e0;font-size:13px;outline:none}}
#search:focus{{border-color:#4a90d9}}
#search::placeholder{{color:#555}}
#stats-bar{{font-size:11px;color:#555;white-space:nowrap;flex-shrink:0}}
.kbd{{display:inline-block;padding:1px 5px;border-radius:3px;background:#252833;border:1px solid #3a3d4a;font-size:9px;color:#555;font-family:monospace}}
.btn{{padding:5px 12px;border-radius:6px;border:1px solid #3a3d4a;background:#252833;color:#aaa;font-size:12px;cursor:pointer;flex-shrink:0}}
.btn:hover{{background:#2f3244;color:#e0e0e0}}
#btn-isolate.isolating{{background:#1e3a5f;border-color:#4a90d9;color:#a0cfff}}

/* Main layout */
#main{{display:flex;flex:1;overflow:hidden}}

/* Sidebar */
#sidebar{{width:260px;flex-shrink:0;background:#13151f;border-right:1px solid #2a2d3a;display:flex;flex-direction:column;overflow:hidden}}

.filter-section{{padding:10px 12px;border-bottom:1px solid #2a2d3a;overflow:hidden}}
.filter-section h3{{font-size:10px;text-transform:uppercase;letter-spacing:.1em;color:#555;margin-bottom:7px;display:flex;align-items:center;gap:6px}}
.filter-scroll{{max-height:110px;overflow-y:auto;display:flex;flex-direction:column;gap:2px}}
.filter-scroll::-webkit-scrollbar{{width:3px}}
.filter-scroll::-webkit-scrollbar-thumb{{background:#2a2d3a;border-radius:2px}}
.filter-label{{display:flex;align-items:center;gap:5px;font-size:11px;color:#bbb;padding:2px 0;cursor:pointer;user-select:none;white-space:nowrap}}
.filter-label input{{accent-color:#4a90d9;cursor:pointer;flex-shrink:0}}
.dot{{width:9px;height:9px;border-radius:50%;flex-shrink:0}}
.rel-tag{{font-family:monospace;font-size:10px;color:#7ed321}}

/* Hub section */
#hub-section{{padding:10px 12px;border-bottom:1px solid #2a2d3a;flex-shrink:0;max-height:180px;display:flex;flex-direction:column;overflow:hidden}}
#hub-section h3{{font-size:10px;text-transform:uppercase;letter-spacing:.1em;color:#555;margin-bottom:7px;flex-shrink:0}}
#hub-list{{overflow-y:auto;display:flex;flex-direction:column;gap:2px}}
#hub-list::-webkit-scrollbar{{width:3px}}
#hub-list::-webkit-scrollbar-thumb{{background:#2a2d3a;border-radius:2px}}
.hub-item{{display:flex;align-items:center;gap:7px;padding:4px 6px;border-radius:5px;cursor:pointer;font-size:11px;color:#ccc;transition:background 100ms}}
.hub-item:hover{{background:#1e2133}}
.hub-dot{{width:8px;height:8px;border-radius:50%;flex-shrink:0}}
.hub-name{{flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
.hub-deg{{font-size:10px;color:#444;flex-shrink:0;min-width:18px;text-align:right}}
.hub-item.active{{background:#1a2440}}

/* Detail section */
#detail-section{{flex:1;overflow-y:auto;padding:10px 12px}}
#detail-section h3{{font-size:10px;text-transform:uppercase;letter-spacing:.1em;color:#555;margin-bottom:8px}}
#detail-content{{font-size:12px;line-height:1.6;color:#ccc}}
#detail-content .detail-empty{{color:#444;font-style:italic}}
#detail-content .d-id{{font-family:monospace;font-size:10px;color:#4a90d9;word-break:break-all;margin-bottom:5px}}
#detail-content .d-type{{display:inline-block;padding:2px 8px;border-radius:4px;font-size:10px;font-weight:600;color:#fff;margin-bottom:6px}}
#detail-content .d-name{{font-size:14px;font-weight:600;color:#e8e8e8;margin-bottom:5px}}
#detail-content .d-desc{{color:#aaa;margin-bottom:8px;line-height:1.5}}
#detail-content .d-section{{font-size:10px;font-weight:600;text-transform:uppercase;letter-spacing:.06em;color:#444;margin:8px 0 3px}}
#detail-content .d-pill{{display:inline-block;padding:2px 6px;border-radius:4px;background:#252833;border:1px solid #3a3d4a;font-size:10px;color:#aaa;margin:2px 2px 2px 0}}
  #detail-content .d-fact{{padding:3px 0;border-bottom:1px solid #1e2030;color:#bbb;font-size:11px}}
  #detail-content .d-note{{padding:3px 0;border-bottom:1px solid #1e2030;color:#bbb;font-size:11px}}
#detail-content .d-edge{{display:flex;align-items:flex-start;gap:4px;padding:3px 0;font-size:10px;border-bottom:1px solid #1e2030}}
#detail-content .d-edge:last-child{{border-bottom:none}}
.d-edge-dir{{color:#555;font-size:9px;flex-shrink:0;width:12px}}
.d-edge-rel{{font-family:monospace;color:#7ed321;flex-shrink:0;width:78px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
.d-edge-node{{color:#4a90d9;cursor:pointer;word-break:break-all;flex:1}}
.d-edge-node:hover{{text-decoration:underline}}
#detail-content .d-conf{{font-size:10px;color:#555}}

/* History nav */
#history-nav{{display:flex;align-items:center;gap:6px;padding:6px 12px;border-top:1px solid #2a2d3a;flex-shrink:0}}
.btn-hist{{padding:4px 10px;border-radius:5px;border:1px solid #3a3d4a;background:#252833;color:#aaa;font-size:11px;cursor:pointer}}
.btn-hist:hover{{background:#2f3244;color:#e0e0e0}}
.btn-hist:disabled{{opacity:0.3;cursor:not-allowed}}
#history-pos{{font-size:10px;color:#444;white-space:nowrap}}

/* Graph canvas */
#cy{{flex:1;background:#0f1117;position:relative}}

/* Navigator */
.cnav-navigator{{position:absolute!important;right:14px!important;bottom:14px!important;width:150px!important;height:100px!important;background:#13151f!important;border:1px solid #2a2d3a!important;border-radius:8px!important;overflow:hidden!important;box-shadow:0 4px 20px rgba(0,0,0,.5)!important;z-index:100!important}}
.cnav-navigator .cnav-minimap{{background:#0f1117!important}}
.cnav-navigator canvas{{background:#0f1117!important}}
.cnav-navigator .cnav-overlay{{background:rgba(74,144,217,.12)!important;border:1px solid #4a90d9!important}}
.cnav-navigator .cnav-collapse,.cnav-navigator .cnav-expand{{display:none!important}}

/* Toast */
#toast{{position:fixed;bottom:60px;left:50%;transform:translateX(-50%);background:#252833;border:1px solid #3a3d4a;color:#ccc;padding:6px 16px;border-radius:8px;font-size:12px;opacity:0;transition:opacity 250ms;pointer-events:none;z-index:1000;white-space:nowrap}}
#toast.show{{opacity:1}}

/* Hint bar */
#kb-hint{{position:absolute;bottom:14px;left:14px;font-size:10px;color:#333;pointer-events:none;z-index:10}}
#kb-hint span{{background:#1a1d27;padding:2px 5px;border-radius:3px;border:1px solid #2a2d3a;color:#333;margin-right:3px}}
</style>
</head>
<body>

<div id="topbar">
  <h1>{title}</h1>
  <input id="search" type="search" placeholder="Search nodes…" autocomplete="off">
  <span id="stats-bar">{node_count} nodes · {edge_count} edges</span>
  <button id="btn-isolate" class="btn" title="Toggle isolate mode [I]">Isolate</button>
  <button id="btn-export" class="btn" title="Export PNG">PNG</button>
  <button id="btn-reset" class="btn" title="Reset view">Reset</button>
</div>

<div id="main">
  <div id="sidebar">
    <div class="filter-section">
      <h3>Node types <span class="kbd">T</span></h3>
      <div class="filter-scroll" id="type-filters">{type_checkboxes}</div>
    </div>
    <div class="filter-section">
      <h3>Relations <span class="kbd">R</span></h3>
      <div class="filter-scroll" id="rel-filters">{rel_checkboxes}</div>
    </div>
    <div id="hub-section">
      <h3>Top hubs</h3>
      <div id="hub-list"></div>
    </div>
    <div id="detail-section">
      <h3>Node detail</h3>
      <div id="detail-content"><p class="detail-empty">Click a node to see details</p></div>
    </div>
    <div id="history-nav">
      <button id="btn-back" class="btn-hist" disabled>&#8592; Back</button>
      <span id="history-pos"></span>
      <button id="btn-fwd" class="btn-hist" disabled>Forward &#8594;</button>
    </div>
  </div>
  <div id="cy"></div>
</div>
<div id="toast"></div>
<div id="kb-hint">
  <span>/ search</span><span>Esc clear</span><span>f fit</span><span>+/- zoom</span><span>&#8592;&#8594; history</span><span>I isolate</span>
</div>

<script>
(function(){{
"use strict";

window.GRAPH_NAME = "{graph_name_escaped}";
const TOP_HUBS = [{top_hubs_js}];

const rawNodes = [
{nodes_js}
];

const rawEdges = [
{edges_js}
];

function showToast(msg, ms) {{
  var t = document.getElementById('toast');
  t.textContent = msg;
  t.classList.add('show');
  setTimeout(function() {{ t.classList.remove('show'); }}, ms || 2000);
}}

var cy = cytoscape({{
  container: document.getElementById('cy'),
  elements: {{ nodes: rawNodes, edges: rawEdges }},
  style: [
    {{
      selector: 'node',
      style: {{
        'background-color': 'data(color)',
        'label': 'data(label)',
        'color': '#ccc',
        'font-size': '10px',
        'text-valign': 'bottom',
        'text-halign': 'center',
        'text-margin-y': '4px',
        'text-outline-color': '#0f1117',
        'text-outline-width': '2px',
        'width': 26,
        'height': 26,
        'border-width': '2px',
        'border-color': 'data(color)',
        'border-opacity': 0.5,
        'transition-property': 'background-color,border-color,width,height,opacity',
        'transition-duration': 100,
      }}
    }},
    {{
      selector: 'node:selected',
      style: {{ 'width': 38, 'height': 38, 'border-width': '3px', 'border-color': '#fff', 'z-index': 999 }}
    }},
    {{
      selector: 'node.highlighted',
      style: {{ 'border-color': '#fff', 'opacity': 1 }}
    }},
    {{
      selector: 'node.faded',
      style: {{ 'opacity': 0.1 }}
    }},
    {{
      selector: 'node.search-match',
      style: {{ 'border-color': '#ffdd57', 'border-width': '3px', 'z-index': 100 }}
    }},
    {{
      selector: 'node.isolated-root',
      style: {{ 'width': 42, 'height': 42, 'border-color': '#4a90d9', 'border-width': '3px', 'z-index': 200 }}
    }},
    {{
      selector: 'edge',
      style: {{
        'width': 1.2,
        'line-color': '#253050',
        'target-arrow-color': '#253050',
        'target-arrow-shape': 'triangle',
        'curve-style': 'bezier',
        'label': 'data(relation)',
        'font-size': '8px',
        'color': '#334',
        'text-rotation': 'autorotate',
        'text-rotation-anchor': 'end',
        'text-margin-x': 3,
        'text-outline-color': '#0f1117',
        'text-outline-width': '1px',
        'opacity': 0.7,
        'transition-property': 'line-color,opacity,width',
        'transition-duration': 100,
      }}
    }},
    {{
      selector: 'edge.highlighted',
      style: {{ 'line-color': '#4a90d9', 'target-arrow-color': '#4a90d9', 'opacity': 1, 'width': 2 }}
    }},
    {{
      selector: 'edge.faded',
      style: {{ 'opacity': 0.02 }}
    }},
  ],
  layout: {{
    name: 'cose',
    animate: true,
    animationDuration: 500,
    nodeRepulsion: 8000,
    idealEdgeLength: 80,
    gravity: 0.25,
    numIter: 800,
    randomize: true,
    fit: true,
    padding: 40,
  }},
  wheelSensitivity: 0.3,
}});

// Navigator
(function(){{
  var navReady = function() {{
    try {{
      var nav = new window.cytoscape_navigator(cy, {{
        container: document.getElementById('cy'),
        viewLiveFrustum: false,
        minZoom: 0.05,
        maxZoom: 0.5,
      }});
    }} catch(e) {{}}
  }};
  if (window.cytoscape_navigator) {{ navReady(); return; }}
  var s = document.createElement('script');
  s.src = 'https://unpkg.com/cytoscape-navigator@2.0.0/cytoscape-navigator.js';
  s.onload = navReady;
  document.head.appendChild(s);
}})();

// History
var hist = [];
var histIdx = -1;

function pushHist(nodeId) {{
  hist.splice(histIdx + 1);
  hist.push(nodeId);
  histIdx = hist.length - 1;
  updHist();
}}

function goBack() {{
  if (histIdx > 0) {{ histIdx--; focusNode(hist[histIdx], false); updHist(); }}
}}

function goFwd() {{
  if (histIdx < hist.length - 1) {{ histIdx++; focusNode(hist[histIdx], false); updHist(); }}
}}

function updHist() {{
  document.getElementById('btn-back').disabled = histIdx <= 0;
  document.getElementById('btn-fwd').disabled = histIdx >= hist.length - 1;
  document.getElementById('history-pos').textContent = histIdx >= 0 ? (histIdx + 1) + '/' + hist.length : '';
}}

// State
var isolateMode = false;

function clearAll() {{
  cy.elements().removeClass('highlighted faded search-match isolated-root');
  cy.elements().style('display', 'element');
  cy.edges().removeClass('rel-hidden');
  isolateMode = false;
  document.getElementById('btn-isolate').classList.remove('isolating');
}}

function focusNode(nodeId, addHist) {{
  if (addHist === undefined) addHist = true;
  var node = cy.getElementById(nodeId);
  if (!node.length) return;
  if (addHist) pushHist(nodeId);

  if (isolateMode) {{
    node.select();
    node.addClass('isolated-root');
    var nb = node.neighborhood();
    node.addClass('highlighted');
    nb.addClass('highlighted');
    cy.elements().not(node).not(nb).addClass('faded');
    nb.edges().addClass('highlighted');
    showDetail(node);
    cy.animate({{ fit: {{ eles: node.closedNeighborhood(), padding: 90 }}, duration: 300 }});
  }} else {{
    clearAll();
    node.select();
    var nb = node.neighborhood();
    node.addClass('highlighted');
    node.addClass('isolated-root');
    nb.addClass('highlighted');
    cy.elements().not(node).not(nb).addClass('faded');
    nb.edges().addClass('highlighted');
    showDetail(node);
    cy.animate({{ fit: {{ eles: node.closedNeighborhood(), padding: 90 }}, duration: 300 }});
  }}

  document.querySelectorAll('.hub-item').forEach(function(el) {{
    el.classList.toggle('active', el.dataset.id === nodeId);
  }});
}}

window._focusNode = focusNode;

function showDetail(node) {{
  var d = node.data();
  var outE = node.outgoers('edge');
  var inE = node.incomers('edge');
  var edgesHtml = '';

  outE.forEach(function(e) {{
    if (e.style('display') === 'none') return;
    var tgt = e.target().data();
    edgesHtml += '<div class="d-edge"><span class="d-edge-dir">&#8594;</span><span class="d-edge-rel">' + e.data('relation') + '</span><span class="d-edge-node" onclick="window._focusNode(\'' + tgt.id + '\')">' + tgt.label + '</span></div>';
  }});
  inE.forEach(function(e) {{
    if (e.style('display') === 'none') return;
    var src = e.source().data();
    edgesHtml += '<div class="d-edge"><span class="d-edge-dir">&#8592;</span><span class="d-edge-rel">' + e.data('relation') + '</span><span class="d-edge-node" onclick="window._focusNode(\'' + src.id + '\')">' + src.label + '</span></div>';
  }});

  var factsHtml = d.facts.length
    ? d.facts.map(function(f) {{ return '<div class="d-fact">' + f + '</div>'; }}).join('')
    : '<span style="color:#444">&#8212;</span>';
  var aliasesHtml = d.aliases.length ? d.aliases.map(function(a) {{ return '<span class="d-pill">' + a + '</span>'; }}).join('') : '';
  var sourcesHtml = d.sources.length ? d.sources.map(function(s) {{ return '<span class="d-pill">' + s + '</span>'; }}).join('') : '';
  var notesHtml = d.notes.length ? d.notes.map(function(n) {{ return '<div class="d-note">' + n + '</div>'; }}).join('') : '';
  var confHtml = d.confidence ? '<div class="d-conf">Confidence: ' + d.confidence + '</div>' : '';
  var degree = outE.length + inE.length;

  document.getElementById('detail-content').innerHTML =
    '<div class="d-id">' + d.id + '</div>' +
    '<span class="d-type" style="background:' + d.color + '">' + d.type + ' &#183; ' + degree + ' conn</span>' +
    '<div class="d-name">' + d.label + '</div>' +
    (d.desc ? '<div class="d-desc">' + d.desc + '</div>' : '') +
    confHtml +
    (aliasesHtml ? '<div class="d-section">Aliases</div>' + aliasesHtml : '') +
    '<div class="d-section">Facts (' + d.facts.length + ')</div>' + factsHtml +
    (notesHtml ? '<div class="d-section">Notes (' + d.notes.length + ')</div>' + notesHtml : '') +
    '<div class="d-section">Connections (' + degree + ')</div>' + (edgesHtml || '<span style="color:#444">&#8212;</span>') +
    (sourcesHtml ? '<div class="d-section">Sources</div>' + sourcesHtml : '');
}}

// Click handlers
cy.on('tap', 'node', function(evt) {{ focusNode(evt.target.id()); }});

cy.on('tap', function(evt) {{
  if (evt.target === cy) {{
    if (!isolateMode) {{
      clearAll();
      cy.elements().unselect();
      document.getElementById('detail-content').innerHTML = '<p class="detail-empty">Click a node to see details</p>';
    }}
  }}
}});

// Search
var searchTimeout;
document.getElementById('search').addEventListener('input', function() {{
  clearTimeout(searchTimeout);
  var q = this.value.trim().toLowerCase();
  if (!q) {{
    clearAll();
    cy.elements().unselect();
    return;
  }}
  searchTimeout = setTimeout(function() {{
    clearAll();
    cy.nodes().forEach(function(n) {{
      var d = n.data();
      var matches = d.id.toLowerCase().indexOf(q) !== -1 ||
        d.label.toLowerCase().indexOf(q) !== -1 ||
        d.aliases.some(function(a) {{ return a.toLowerCase().indexOf(q) !== -1; }}) ||
        d.desc.toLowerCase().indexOf(q) !== -1;
      if (matches) n.addClass('search-match');
      else n.addClass('faded');
    }});
    var matched = cy.nodes('.search-match');
    if (matched.length) cy.animate({{ fit: {{ eles: matched, padding: 80 }}, duration: 300 }});
  }}, 180);
}});

// Filters
function applyFilters() {{
  var activeTypes = {{}};
  document.querySelectorAll('.type-filter:checked').forEach(function(cb) {{ activeTypes[cb.value] = true; }});
  var activeRels = {{}};
  document.querySelectorAll('.rel-filter:checked').forEach(function(cb) {{ activeRels[cb.value] = true; }});

  cy.nodes().forEach(function(n) {{
    n.style('display', activeTypes[n.data('type')] ? 'element' : 'none');
  }});
  cy.edges().forEach(function(e) {{
    var srcOk = e.source().style('display') !== 'none';
    var tgtOk = e.target().style('display') !== 'none';
    var relOk = !!activeRels[e.data('relation')];
    e.style('display', srcOk && tgtOk && relOk ? 'element' : 'none');
  }});
}}

document.querySelectorAll('.type-filter,.rel-filter').forEach(function(cb) {{
  cb.addEventListener('change', applyFilters);
}});

// Isolate
document.getElementById('btn-isolate').addEventListener('click', function() {{
  var sel = cy.$(':selected');
  if (!sel.length) {{ showToast('Select a node first'); return; }}
  isolateMode = !isolateMode;
  this.classList.toggle('isolating', isolateMode);
  focusNode(sel.id());
}});

// Export PNG
document.getElementById('btn-export').addEventListener('click', function() {{
  var png = cy.png({{ bg: '#0f1117', scale: 2, full: true }});
  var a = document.createElement('a');
  a.href = png;
  a.download = (window.GRAPH_NAME || 'graph') + '.png';
  a.click();
  showToast('PNG exported');
}});

// Reset
function resetAll() {{
  clearAll();
  cy.elements().style('display', 'element');
  document.querySelectorAll('.type-filter,.rel-filter').forEach(function(cb) {{ cb.checked = true; }});
  cy.fit(undefined, 40);
  cy.nodes().unselect();
  document.getElementById('detail-content').innerHTML = '<p class="detail-empty">Click a node to see details</p>';
  document.getElementById('search').value = '';
  hist.length = 0; histIdx = -1; updHist();
  document.querySelectorAll('.hub-item').forEach(function(el) {{ el.classList.remove('active'); }});
}}

document.getElementById('btn-reset').addEventListener('click', resetAll);
document.getElementById('btn-back').addEventListener('click', goBack);
document.getElementById('btn-fwd').addEventListener('click', goFwd);

// Hub list
(function() {{
  var list = document.getElementById('hub-list');
  TOP_HUBS.forEach(function(hub) {{
    var el = document.createElement('div');
    el.className = 'hub-item';
    el.dataset.id = hub.id;
    el.innerHTML = '<span class="hub-dot" style="background:' + hub.color + '"></span><span class="hub-name">' + hub.label + '</span><span class="hub-deg">' + hub.degree + '</span>';
    el.addEventListener('click', function() {{ focusNode(hub.id); }});
    list.appendChild(el);
  }});
}})();

// Keyboard shortcuts
document.addEventListener('keydown', function(e) {{
  var tag = document.activeElement.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA') return;
  switch(e.key) {{
    case '/': e.preventDefault(); document.getElementById('search').focus(); break;
    case 'Escape': clearAll(); cy.nodes().unselect(); document.getElementById('detail-content').innerHTML = '<p class="detail-empty">Click a node to see details</p>'; isolateMode = false; document.getElementById('btn-isolate').classList.remove('isolating'); break;
    case 'f': case 'F': cy.fit(undefined, 40); break;
    case '+': case '=': cy.zoom(cy.zoom() * 1.25); break;
    case '-': cy.zoom(cy.zoom() / 1.25); break;
    case 'ArrowLeft': goBack(); break;
    case 'ArrowRight': goFwd(); break;
    case 'i': case 'I': document.getElementById('btn-isolate').click(); break;
  }}
}});

// Fit on load
cy.ready(function() {{ setTimeout(function() {{ cy.fit(undefined, 40); }}, 600); }});

}})();
</script>
</body>
</html>"#,
        title = title,
        graph_name_escaped = graph_name_escaped,
        node_count = node_count,
        edge_count = edge_count,
        nodes_js = nodes_js,
        edges_js = edges_js,
        type_checkboxes = type_checkboxes,
        rel_checkboxes = rel_checkboxes,
        top_hubs_js = top_hubs_js,
    )
}
