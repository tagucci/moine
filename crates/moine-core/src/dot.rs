//! DOT rendering helpers for lattice comparison graphs.

use std::collections::BTreeSet;

use crate::{DistanceTrace, Lattice, Symbol};

const DOT_BEST_PATH_COLOR: &str = "#9a5b38";
const DOT_DEFAULT_NODE_COLOR: &str = "#495057";
const DOT_MUTED_EDGE_COLOR: &str = "#868e96";

/// Input data for rendering a pair of lattices as a Graphviz DOT graph.
#[derive(Clone, Copy, Debug)]
pub struct LatticeDotData<'a> {
    /// Original left input text.
    pub left_input: &'a str,
    /// Original right input text.
    pub right_input: &'a str,
    /// Left-side lattice.
    pub left_lattice: &'a Lattice,
    /// Right-side lattice.
    pub right_lattice: &'a Lattice,
    /// LPED distance between the two lattices.
    pub distance: usize,
    /// Optional best-path trace used to highlight one optimal path.
    pub trace: Option<&'a DistanceTrace>,
    /// Optional trace reconstruction error shown in the graph label.
    pub trace_error: Option<&'a str>,
}

/// Renders a Japanese romaji lattice comparison as Graphviz DOT.
pub fn romaji_lattice_dot(data: &LatticeDotData<'_>) -> String {
    lattice_pair_dot("moine_romaji_lattice", data)
}

/// Renders a Chinese pinyin lattice comparison as Graphviz DOT.
pub fn pinyin_lattice_dot(data: &LatticeDotData<'_>) -> String {
    lattice_pair_dot("moine_pinyin_lattice", data)
}

fn lattice_pair_dot(graph_name: &str, data: &LatticeDotData<'_>) -> String {
    let left_symbols = data.trace.as_ref().map(|trace| trace.left_symbols());
    let right_symbols = data.trace.as_ref().map(|trace| trace.right_symbols());
    let left_best_arcs = left_symbols
        .as_deref()
        .map(|symbols| best_arc_keys(data.left_lattice, symbols))
        .unwrap_or_default();
    let right_best_arcs = right_symbols
        .as_deref()
        .map(|symbols| best_arc_keys(data.right_lattice, symbols))
        .unwrap_or_default();
    let left_best_nodes = best_nodes(&left_best_arcs);
    let right_best_nodes = best_nodes(&right_best_arcs);

    let best_path_label = match (&left_symbols, &right_symbols, data.trace_error) {
        (Some(left), Some(right), _) => format!(
            "best_left={}\\nbest_right={}",
            dot_escape(&symbols_to_string(left)),
            dot_escape(&symbols_to_string(right))
        ),
        (_, _, Some(error)) => format!("best path unavailable: {}", dot_escape(error)),
        _ => "best path unavailable".to_string(),
    };
    let graph_label = format!("distance={}\\n{}", data.distance, best_path_label);

    let mut dot = String::new();
    dot.push_str(&format!("digraph {graph_name} {{\n"));
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  graph [fontname=\"Helvetica\", labelloc=\"t\", label=\"");
    dot.push_str(&graph_label);
    dot.push_str("\"];\n");
    dot.push_str(&format!(
        "  node [fontname=\"Helvetica\", shape=circle, width=0.48, fixedsize=true, color=\"{DOT_DEFAULT_NODE_COLOR}\"];\n",
    ));
    dot.push_str(&format!(
        "  edge [fontname=\"Helvetica\", color=\"{DOT_DEFAULT_NODE_COLOR}\", arrowsize=0.7];\n\n"
    ));

    append_lattice_cluster(
        &mut dot,
        "right",
        "RIGHT",
        data.right_input,
        data.right_lattice,
        &right_best_arcs,
        &right_best_nodes,
    );
    dot.push('\n');
    append_lattice_cluster(
        &mut dot,
        "left",
        "LEFT",
        data.left_input,
        data.left_lattice,
        &left_best_arcs,
        &left_best_nodes,
    );
    dot.push_str("}\n");
    dot
}

/// Converts trace symbols back to a string.
pub fn symbols_to_string(symbols: &[Symbol]) -> String {
    symbols
        .iter()
        .map(|&symbol| char::from_u32(symbol).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn append_lattice_cluster(
    dot: &mut String,
    prefix: &str,
    lane_label: &str,
    input: &str,
    lattice: &Lattice,
    best_arcs: &BTreeSet<ArcKey>,
    best_nodes: &BTreeSet<usize>,
) {
    dot.push_str(&format!("  subgraph cluster_{prefix} {{\n"));
    dot.push_str("    style=\"rounded\";\n");
    dot.push_str("    color=\"#ced4da\";\n");
    dot.push_str("    label=\"");
    dot.push_str(&format!("{lane_label}\\ninput={}", dot_escape(input)));
    dot.push_str("\";\n");
    for node in 0..lattice.node_count() {
        let label = if node == lattice.start() {
            "BOS".to_string()
        } else if node == lattice.end() {
            "EOS".to_string()
        } else {
            node.to_string()
        };
        let shape = if node == lattice.end() {
            "doublecircle"
        } else {
            "circle"
        };
        let color = if best_nodes.contains(&node) {
            DOT_BEST_PATH_COLOR
        } else {
            DOT_DEFAULT_NODE_COLOR
        };
        let penwidth = if best_nodes.contains(&node) {
            "2.4"
        } else {
            "1.2"
        };
        dot.push_str(&format!(
            "    {prefix}_{node} [label=\"{}\", shape={shape}, color=\"{color}\", penwidth={penwidth}];\n",
            dot_escape(&label)
        ));
    }
    for arc in lattice.arcs() {
        let key = arc_key(arc.src, arc.dst, arc.symbol);
        let is_best = best_arcs.contains(&key);
        let color = if is_best {
            DOT_BEST_PATH_COLOR
        } else {
            DOT_MUTED_EDGE_COLOR
        };
        let penwidth = if is_best { "3.0" } else { "1.1" };
        dot.push_str(&format!(
            "    {prefix}_{} -> {prefix}_{} [label=\"{}\", color=\"{color}\", fontcolor=\"{color}\", penwidth={penwidth}];\n",
            arc.src,
            arc.dst,
            dot_escape(&symbol_to_string(arc.symbol))
        ));
    }
    dot.push_str("  }\n");
}

type ArcKey = (usize, usize, Symbol);

fn arc_key(src: usize, dst: usize, symbol: Symbol) -> ArcKey {
    (src, dst, symbol)
}

fn best_arc_keys(lattice: &Lattice, symbols: &[Symbol]) -> BTreeSet<ArcKey> {
    let mut path = Vec::new();
    if find_arc_path(lattice, lattice.start(), symbols, 0, &mut path) {
        path.into_iter().collect()
    } else {
        BTreeSet::new()
    }
}

fn find_arc_path(
    lattice: &Lattice,
    node: usize,
    symbols: &[Symbol],
    symbol_idx: usize,
    path: &mut Vec<ArcKey>,
) -> bool {
    if symbol_idx == symbols.len() {
        return node == lattice.end();
    }

    for arc in lattice.outgoing_arcs(node) {
        if arc.symbol != symbols[symbol_idx] {
            continue;
        }
        path.push(arc_key(arc.src, arc.dst, arc.symbol));
        if find_arc_path(lattice, arc.dst, symbols, symbol_idx + 1, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn best_nodes(best_arcs: &BTreeSet<ArcKey>) -> BTreeSet<usize> {
    let mut nodes = BTreeSet::new();
    for &(src, dst, _) in best_arcs {
        nodes.insert(src);
        nodes.insert(dst);
    }
    nodes
}

fn symbol_to_string(symbol: Symbol) -> String {
    char::from_u32(symbol)
        .unwrap_or(char::REPLACEMENT_CHARACTER)
        .to_string()
}

fn dot_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => {}
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{try_distance_with_trace, Lattice};

    #[test]
    fn renders_best_path_in_dot() {
        let left_lattice = Lattice::from_paths(["insat"]);
        let right_lattice = Lattice::from_paths(["insatu", "zzzzz"]);
        let trace = try_distance_with_trace(&left_lattice, &right_lattice).unwrap();
        let dot = romaji_lattice_dot(&LatticeDotData {
            left_input: "いんさt",
            right_input: "印刷",
            left_lattice: &left_lattice,
            right_lattice: &right_lattice,
            distance: trace.distance,
            trace: Some(&trace),
            trace_error: None,
        });

        assert!(dot.contains("digraph moine_romaji_lattice"));
        assert!(dot.contains("best_left=insat"));
        assert!(dot.contains("best_right=insatu"));
        assert!(dot.contains("label=\"u\", color=\"#9a5b38\""));
    }
}
