//! Language-independent Lattice Path Edit Distance core.
//!
//! `moine-core` provides the [`Lattice`] DAG representation and exact edit
//! distance algorithms used by the language adapters. It intentionally has no
//! Japanese, Chinese, Unicode-normalization, dictionary, CLI, or artifact
//! loading logic.
//!
//! Use [`try_distance`], [`try_damerau_distance`], [`try_distance_with_trace`],
//! [`try_within_distance`], and [`try_within_damerau_distance`] when lattices
//! come from external input. The infallible convenience functions keep examples
//! short, but panic if the configured matrix limits would be exceeded. Trace
//! reconstruction stores more per cell than the plain distance path, so it uses
//! the lower [`MAX_TRACE_MATRIX_CELLS`] limit.
//!
//! ```
//! use moine_core::{distance, try_distance, Lattice};
//!
//! let left = Lattice::from_paths(["moine"]);
//! let right = Lattice::from_paths(["moinya"]);
//!
//! assert_eq!(distance(&left, &right), 2);
//! assert_eq!(try_distance(&left, &right).unwrap(), 2);
//! ```
//!
#![deny(missing_docs)]

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::error::Error;
use std::fmt;

pub mod dot;

/// Integer symbol stored on lattice arcs.
///
/// String constructors encode each Unicode scalar value as one `Symbol`.
pub type Symbol = u32;

/// Directed arc between two lattice nodes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Arc {
    /// Source node index.
    pub src: usize,
    /// Destination node index.
    pub dst: usize,
    /// Symbol consumed when traversing the arc.
    pub symbol: Symbol,
}

impl Arc {
    /// Creates an arc from `src` to `dst` carrying `symbol`.
    pub fn new(src: usize, dst: usize, symbol: Symbol) -> Self {
        Self { src, dst, symbol }
    }
}

/// A directed acyclic lattice with one start node and one end node.
///
/// Nodes are addressed by zero-based indices. Arcs must move from lower to
/// higher node indices so distance algorithms can process the lattice in
/// topological order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lattice {
    node_count: usize,
    start: usize,
    end: usize,
    arcs: Vec<Arc>,
    incoming: Vec<Vec<usize>>,
    outgoing: Vec<Vec<usize>>,
}

/// Errors returned when constructing an invalid lattice.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LatticeError {
    /// The lattice has no nodes.
    Empty,
    /// Empty paths were mixed with non-empty paths.
    MixedEmptyAndNonEmptyPaths,
    /// An arc endpoint is outside the node range.
    InvalidNode {
        /// Invalid node index.
        node: usize,
        /// Number of nodes in the lattice.
        node_count: usize,
    },
    /// An arc does not respect topological node order.
    InvalidArcOrder {
        /// Source node index.
        src: usize,
        /// Destination node index.
        dst: usize,
    },
    /// Start or end node indices are outside the valid range.
    InvalidEndpoint {
        /// Start node index.
        start: usize,
        /// End node index.
        end: usize,
    },
}

impl fmt::Display for LatticeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "lattice must contain at least one node"),
            Self::MixedEmptyAndNonEmptyPaths => write!(
                f,
                "mixed empty and non-empty paths need epsilon arcs, which moine-core does not model yet"
            ),
            Self::InvalidNode { node, node_count } => {
                write!(f, "node {node} is outside 0..{node_count}")
            }
            Self::InvalidArcOrder { src, dst } => {
                write!(f, "arc {src}->{dst} violates topological node order")
            }
            Self::InvalidEndpoint { start, end } => {
                write!(f, "invalid endpoints start={start}, end={end}")
            }
        }
    }
}

impl Error for LatticeError {}

impl Lattice {
    /// Builds a lattice from explicit nodes and arcs.
    ///
    /// `start` and `end` must be valid node indices with `start <= end`.
    /// Every arc must reference valid nodes and satisfy `src < dst`.
    pub fn from_edges(
        node_count: usize,
        start: usize,
        end: usize,
        arcs: Vec<Arc>,
    ) -> Result<Self, LatticeError> {
        if node_count == 0 {
            return Err(LatticeError::Empty);
        }
        if start >= node_count || end >= node_count || start > end {
            return Err(LatticeError::InvalidEndpoint { start, end });
        }

        let mut incoming = vec![Vec::new(); node_count];
        let mut outgoing = vec![Vec::new(); node_count];
        for (idx, arc) in arcs.iter().enumerate() {
            if arc.src >= node_count {
                return Err(LatticeError::InvalidNode {
                    node: arc.src,
                    node_count,
                });
            }
            if arc.dst >= node_count {
                return Err(LatticeError::InvalidNode {
                    node: arc.dst,
                    node_count,
                });
            }
            if arc.src >= arc.dst {
                return Err(LatticeError::InvalidArcOrder {
                    src: arc.src,
                    dst: arc.dst,
                });
            }
            incoming[arc.dst].push(idx);
            outgoing[arc.src].push(idx);
        }

        Ok(Self {
            node_count,
            start,
            end,
            arcs,
            incoming,
            outgoing,
        })
    }

    /// Builds a lattice from UTF-8 string paths.
    ///
    /// # Panics
    ///
    /// Panics when `paths` is empty or when empty and non-empty paths are
    /// mixed. Use [`Lattice::try_from_paths`] to handle those cases as input
    /// errors.
    pub fn from_paths<I, S>(paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self::try_from_paths(paths).expect("valid string path lattice")
    }

    /// Builds a lattice from UTF-8 string paths.
    pub fn try_from_paths<I, S>(paths: I) -> Result<Self, LatticeError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let symbol_paths = paths.into_iter().map(|path| {
            path.as_ref()
                .chars()
                .map(|ch| ch as Symbol)
                .collect::<Vec<_>>()
        });
        Self::try_from_symbol_paths(symbol_paths)
    }

    /// Builds a lattice from symbol paths.
    ///
    /// # Panics
    ///
    /// Panics when `paths` is empty or when empty and non-empty paths are
    /// mixed. Use [`Lattice::try_from_symbol_paths`] to handle those cases as
    /// input errors.
    pub fn from_symbol_paths<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: IntoIterator<Item = Symbol>,
    {
        Self::try_from_symbol_paths(paths).expect("valid symbol path lattice")
    }

    /// Builds a lattice from symbol paths.
    pub fn try_from_symbol_paths<I, P>(paths: I) -> Result<Self, LatticeError>
    where
        I: IntoIterator<Item = P>,
        P: IntoIterator<Item = Symbol>,
    {
        let paths = paths
            .into_iter()
            .map(|path| path.into_iter().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Err(LatticeError::Empty);
        }

        if paths.len() == 1 && paths[0].is_empty() {
            return Self::from_edges(1, 0, 0, Vec::new());
        }
        if !paths.iter().all(|path| !path.is_empty()) {
            return Err(LatticeError::MixedEmptyAndNonEmptyPaths);
        }

        let start = 0;
        let node_count = 2 + paths
            .iter()
            .map(|path| path.len().saturating_sub(1))
            .sum::<usize>();
        let end = node_count - 1;
        let mut next_node = 1;
        let mut arcs = Vec::new();

        for path in paths {
            let mut current = start;
            for (idx, symbol) in path.iter().copied().enumerate() {
                let dst = if idx + 1 == path.len() {
                    end
                } else {
                    let node = next_node;
                    next_node += 1;
                    node
                };
                arcs.push(Arc::new(current, dst, symbol));
                current = dst;
            }
        }

        debug_assert_eq!(next_node, end);
        Self::from_edges(node_count, start, end, arcs)
    }

    /// Builds a compact lattice from symbol paths by sharing common suffixes.
    ///
    /// # Panics
    ///
    /// Panics when `paths` is empty or when empty and non-empty paths are
    /// mixed. Use [`Lattice::try_from_symbol_paths_compact`] to handle those
    /// cases as input errors.
    pub fn from_symbol_paths_compact<I, P>(paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: IntoIterator<Item = Symbol>,
    {
        Self::try_from_symbol_paths_compact(paths).expect("valid compact symbol path lattice")
    }

    /// Builds a compact lattice from symbol paths by sharing common suffixes.
    pub fn try_from_symbol_paths_compact<I, P>(paths: I) -> Result<Self, LatticeError>
    where
        I: IntoIterator<Item = P>,
        P: IntoIterator<Item = Symbol>,
    {
        let paths = paths
            .into_iter()
            .map(|path| path.into_iter().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        if paths.is_empty() {
            return Err(LatticeError::Empty);
        }

        if paths.len() == 1 && paths[0].is_empty() {
            return Self::from_edges(1, 0, 0, Vec::new());
        }
        if !paths.iter().all(|path| !path.is_empty()) {
            return Err(LatticeError::MixedEmptyAndNonEmptyPaths);
        }

        let mut builder = CompactPathBuilder::default();
        for path in paths {
            builder.insert(&path);
        }
        Ok(builder.into_lattice())
    }

    /// Returns the number of nodes in the lattice.
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Returns the start node index.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Returns the end node index.
    pub fn end(&self) -> usize {
        self.end
    }

    /// Returns all arcs in insertion order.
    pub fn arcs(&self) -> &[Arc] {
        &self.arcs
    }

    /// Returns incoming arcs for `node`, or `None` when the node index is out
    /// of range.
    pub fn try_incoming_arcs(&self, node: usize) -> Option<impl Iterator<Item = &Arc>> {
        self.incoming
            .get(node)
            .map(|indices| indices.iter().map(|&idx| &self.arcs[idx]))
    }

    /// Returns incoming arcs for `node`.
    ///
    /// # Panics
    ///
    /// Panics when `node >= self.node_count()`.
    pub fn incoming_arcs(&self, node: usize) -> impl Iterator<Item = &Arc> {
        self.try_incoming_arcs(node)
            .expect("node should be inside lattice")
    }

    /// Returns outgoing arcs for `node`, or `None` when the node index is out
    /// of range.
    pub fn try_outgoing_arcs(&self, node: usize) -> Option<impl Iterator<Item = &Arc>> {
        self.outgoing
            .get(node)
            .map(|indices| indices.iter().map(|&idx| &self.arcs[idx]))
    }

    /// Returns outgoing arcs for `node`.
    ///
    /// # Panics
    ///
    /// Panics when `node >= self.node_count()`.
    pub fn outgoing_arcs(&self, node: usize) -> impl Iterator<Item = &Arc> {
        self.try_outgoing_arcs(node)
            .expect("node should be inside lattice")
    }
}

#[derive(Clone, Debug, Default)]
struct TrieNode {
    children: BTreeMap<Symbol, usize>,
    terminal_symbols: BTreeSet<Symbol>,
}

#[derive(Clone, Debug, Default)]
struct CompactPathBuilder {
    nodes: Vec<TrieNode>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CompactSignature {
    terminal_symbols: Vec<Symbol>,
    children: Vec<(Symbol, usize)>,
}

#[derive(Clone, Debug)]
struct CompactNode {
    terminal_symbols: Vec<Symbol>,
    children: Vec<(Symbol, usize)>,
}

impl CompactPathBuilder {
    fn insert(&mut self, path: &[Symbol]) {
        if self.nodes.is_empty() {
            self.nodes.push(TrieNode::default());
        }

        let mut current = 0;
        for (idx, symbol) in path.iter().copied().enumerate() {
            if idx + 1 == path.len() {
                self.nodes[current].terminal_symbols.insert(symbol);
                break;
            }

            let next = if let Some(&next) = self.nodes[current].children.get(&symbol) {
                next
            } else {
                let next = self.nodes.len();
                self.nodes.push(TrieNode::default());
                self.nodes[current].children.insert(symbol, next);
                next
            };
            current = next;
        }
    }

    fn into_lattice(self) -> Lattice {
        let mut minimizer = CompactMinimizer {
            trie_nodes: self.nodes,
            memo: HashMap::new(),
            interned: HashMap::new(),
            compact_nodes: Vec::new(),
        };
        let root = minimizer.compact_trie_node(0);
        build_compact_lattice(root, minimizer.compact_nodes)
    }
}

struct CompactMinimizer {
    trie_nodes: Vec<TrieNode>,
    memo: HashMap<usize, usize>,
    interned: HashMap<CompactSignature, usize>,
    compact_nodes: Vec<CompactNode>,
}

impl CompactMinimizer {
    fn compact_trie_node(&mut self, trie_id: usize) -> usize {
        if let Some(&compact_id) = self.memo.get(&trie_id) {
            return compact_id;
        }

        let children = self.trie_nodes[trie_id]
            .children
            .clone()
            .into_iter()
            .map(|(symbol, child)| (symbol, self.compact_trie_node(child)))
            .collect::<Vec<_>>();
        let terminal_symbols = self.trie_nodes[trie_id]
            .terminal_symbols
            .iter()
            .copied()
            .collect::<Vec<_>>();
        let signature = CompactSignature {
            terminal_symbols,
            children,
        };

        let compact_id = if let Some(&existing) = self.interned.get(&signature) {
            existing
        } else {
            let compact_id = self.compact_nodes.len();
            self.compact_nodes.push(CompactNode {
                terminal_symbols: signature.terminal_symbols.clone(),
                children: signature.children.clone(),
            });
            self.interned.insert(signature, compact_id);
            compact_id
        };

        self.memo.insert(trie_id, compact_id);
        compact_id
    }
}

fn build_compact_lattice(root: usize, compact_nodes: Vec<CompactNode>) -> Lattice {
    let mut reachable = Vec::new();
    collect_reachable_compact_nodes(root, &compact_nodes, &mut BTreeSet::new(), &mut reachable);

    let mut heights = HashMap::new();
    for &node in &reachable {
        compact_height(node, &compact_nodes, &mut heights);
    }

    reachable.sort_by_key(|node| {
        (
            usize::from(*node != root),
            std::cmp::Reverse(*heights.get(node).expect("height should be known")),
            *node,
        )
    });

    let node_ids = reachable
        .iter()
        .enumerate()
        .map(|(node_id, &compact_id)| (compact_id, node_id))
        .collect::<HashMap<_, _>>();
    let end = reachable.len();
    let mut arcs = Vec::new();

    for &compact_id in &reachable {
        let src = node_ids[&compact_id];
        let node = &compact_nodes[compact_id];
        for &symbol in &node.terminal_symbols {
            arcs.push(Arc::new(src, end, symbol));
        }
        for &(symbol, child) in &node.children {
            arcs.push(Arc::new(src, node_ids[&child], symbol));
        }
    }

    Lattice::from_edges(end + 1, 0, end, arcs).expect("valid compact path lattice")
}

fn collect_reachable_compact_nodes(
    node: usize,
    compact_nodes: &[CompactNode],
    seen: &mut BTreeSet<usize>,
    output: &mut Vec<usize>,
) {
    if !seen.insert(node) {
        return;
    }
    output.push(node);
    for &(_, child) in &compact_nodes[node].children {
        collect_reachable_compact_nodes(child, compact_nodes, seen, output);
    }
}

fn compact_height(
    node: usize,
    compact_nodes: &[CompactNode],
    memo: &mut HashMap<usize, usize>,
) -> usize {
    if let Some(&height) = memo.get(&node) {
        return height;
    }

    let child_height = compact_nodes[node]
        .children
        .iter()
        .map(|&(_, child)| compact_height(child, compact_nodes, memo) + 1)
        .max()
        .unwrap_or(0);
    let terminal_height = usize::from(!compact_nodes[node].terminal_symbols.is_empty());
    let height = child_height.max(terminal_height);
    memo.insert(node, height);
    height
}

/// Edit operation represented in a trace step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditOp {
    /// The left and right symbols matched exactly.
    Match,
    /// A left symbol was substituted for a right symbol.
    Substitute,
    /// A left symbol was deleted.
    Delete,
    /// A right symbol was inserted.
    Insert,
}

/// One operation in a reconstructed edit-distance trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceStep {
    /// Operation selected for this step.
    pub op: EditOp,
    /// Symbol consumed from the left lattice, if any.
    pub left: Option<Symbol>,
    /// Symbol consumed from the right lattice, if any.
    pub right: Option<Symbol>,
}

/// Edit distance plus one best sequence of edit operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistanceTrace {
    /// Final edit distance.
    pub distance: usize,
    /// Reconstructed steps from start to end.
    pub steps: Vec<TraceStep>,
}

impl DistanceTrace {
    /// Returns the left-side symbols consumed by the trace.
    pub fn left_symbols(&self) -> Vec<Symbol> {
        self.steps.iter().filter_map(|step| step.left).collect()
    }

    /// Returns the right-side symbols consumed by the trace.
    pub fn right_symbols(&self) -> Vec<Symbol> {
        self.steps.iter().filter_map(|step| step.right).collect()
    }
}

/// Maximum DP matrix size accepted by non-trace distance functions.
pub const MAX_DISTANCE_MATRIX_CELLS: usize = 16_000_000;
/// Maximum DP matrix size accepted by trace reconstruction.
pub const MAX_TRACE_MATRIX_CELLS: usize = 2_000_000;

/// Errors returned by fallible distance functions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DistanceError {
    /// The requested dynamic-programming matrix exceeds the configured limit.
    MatrixTooLarge {
        /// Number of matrix rows.
        rows: usize,
        /// Number of matrix columns.
        cols: usize,
        /// Maximum allowed cell count.
        max_cells: usize,
    },
}

impl fmt::Display for DistanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MatrixTooLarge {
                rows,
                cols,
                max_cells,
            } => write!(
                f,
                "distance matrix {rows}x{cols} exceeds the maximum of {max_cells} cells"
            ),
        }
    }
}

impl Error for DistanceError {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Backpointer {
    prev_i: usize,
    prev_j: usize,
    step: TraceStep,
}

const INF: usize = usize::MAX / 4;

/// Computes lattice edit distance.
///
/// # Panics
///
/// Panics when the DP matrix would exceed [`MAX_DISTANCE_MATRIX_CELLS`].
/// Use [`try_distance`] for external or otherwise untrusted lattices.
pub fn distance(left: &Lattice, right: &Lattice) -> usize {
    try_distance(left, right).expect("distance matrix should fit")
}

/// Computes lattice edit distance with explicit matrix-size validation.
pub fn try_distance(left: &Lattice, right: &Lattice) -> Result<usize, DistanceError> {
    let rows = left.node_count();
    let cols = right.node_count();
    let cells = checked_distance_matrix_cells(rows, cols)?;
    Ok(distance_impl(left, right, cells))
}

fn distance_impl(left: &Lattice, right: &Lattice, cells: usize) -> usize {
    let cols = right.node_count();
    let mut dp = vec![INF; cells];
    dp[distance_index(left.start(), right.start(), cols)] = 0;

    for i in left.start()..=left.end() {
        for j in right.start()..=right.end() {
            if i == left.start() && j == right.start() {
                continue;
            }

            let mut best = dp[distance_index(i, j, cols)];

            for left_arc in left.incoming_arcs(i) {
                for right_arc in right.incoming_arcs(j) {
                    let cost = usize::from(left_arc.symbol != right_arc.symbol);
                    let candidate =
                        dp[distance_index(left_arc.src, right_arc.src, cols)].saturating_add(cost);
                    best = best.min(candidate);
                }
            }

            for left_arc in left.incoming_arcs(i) {
                let candidate = dp[distance_index(left_arc.src, j, cols)].saturating_add(1);
                best = best.min(candidate);
            }

            for right_arc in right.incoming_arcs(j) {
                let candidate = dp[distance_index(i, right_arc.src, cols)].saturating_add(1);
                best = best.min(candidate);
            }

            dp[distance_index(i, j, cols)] = best;
        }
    }

    dp[distance_index(left.end(), right.end(), cols)]
}

/// Computes lattice edit distance with adjacent transpositions.
///
/// # Panics
///
/// Panics when the DP matrix would exceed [`MAX_DISTANCE_MATRIX_CELLS`].
/// Use [`try_damerau_distance`] for external or otherwise untrusted lattices.
pub fn damerau_distance(left: &Lattice, right: &Lattice) -> usize {
    try_damerau_distance(left, right).expect("distance matrix should fit")
}

/// Computes lattice edit distance with adjacent transpositions and explicit
/// matrix-size validation.
pub fn try_damerau_distance(left: &Lattice, right: &Lattice) -> Result<usize, DistanceError> {
    let rows = left.node_count();
    let cols = right.node_count();
    let cells = checked_distance_matrix_cells(rows, cols)?;
    Ok(damerau_distance_impl(left, right, cells))
}

fn damerau_distance_impl(left: &Lattice, right: &Lattice, cells: usize) -> usize {
    let cols = right.node_count();
    let mut dp = vec![INF; cells];
    dp[distance_index(left.start(), right.start(), cols)] = 0;

    for i in left.start()..=left.end() {
        for j in right.start()..=right.end() {
            if i == left.start() && j == right.start() {
                continue;
            }

            let mut best = dp[distance_index(i, j, cols)];

            for left_arc in left.incoming_arcs(i) {
                for right_arc in right.incoming_arcs(j) {
                    let cost = usize::from(left_arc.symbol != right_arc.symbol);
                    let candidate =
                        dp[distance_index(left_arc.src, right_arc.src, cols)].saturating_add(cost);
                    best = best.min(candidate);
                }
            }

            for left_arc in left.incoming_arcs(i) {
                let candidate = dp[distance_index(left_arc.src, j, cols)].saturating_add(1);
                best = best.min(candidate);
            }

            for right_arc in right.incoming_arcs(j) {
                let candidate = dp[distance_index(i, right_arc.src, cols)].saturating_add(1);
                best = best.min(candidate);
            }

            for left_second in left.incoming_arcs(i) {
                for right_second in right.incoming_arcs(j) {
                    for left_first in left.incoming_arcs(left_second.src) {
                        for right_first in right.incoming_arcs(right_second.src) {
                            if left_first.symbol == right_second.symbol
                                && left_second.symbol == right_first.symbol
                            {
                                let candidate = dp
                                    [distance_index(left_first.src, right_first.src, cols)]
                                .saturating_add(1);
                                best = best.min(candidate);
                            }
                        }
                    }
                }
            }

            dp[distance_index(i, j, cols)] = best;
        }
    }

    dp[distance_index(left.end(), right.end(), cols)]
}

/// Computes edit distance and one best trace.
///
/// The trace is meaningful for lattices whose `start` and `end` states are
/// reachable through modeled arcs. `Lattice::from_paths` and
/// `Lattice::from_symbol_paths_compact` construct that shape; arbitrary
/// `from_edges` DAGs can represent unreachable endpoints, where the distance
/// remains effectively infinite and the returned trace can be empty.
///
/// # Panics
///
/// Panics when the trace DP matrix would exceed [`MAX_TRACE_MATRIX_CELLS`].
/// Use [`try_distance_with_trace`] for external or otherwise untrusted
/// lattices.
pub fn distance_with_trace(left: &Lattice, right: &Lattice) -> DistanceTrace {
    try_distance_with_trace(left, right).expect("distance matrix should fit")
}

/// Computes edit distance and one best trace with explicit matrix-size
/// validation.
pub fn try_distance_with_trace(
    left: &Lattice,
    right: &Lattice,
) -> Result<DistanceTrace, DistanceError> {
    let rows = left.node_count();
    let cols = right.node_count();
    let cells = checked_trace_matrix_cells(rows, cols)?;
    Ok(distance_with_trace_impl(left, right, cells))
}

fn distance_with_trace_impl(left: &Lattice, right: &Lattice, cells: usize) -> DistanceTrace {
    let cols = right.node_count();
    let mut dp = vec![INF; cells];
    let mut back = vec![None; cells];
    dp[distance_index(left.start(), right.start(), cols)] = 0;

    for i in left.start()..=left.end() {
        for j in right.start()..=right.end() {
            if i == left.start() && j == right.start() {
                continue;
            }

            let index = distance_index(i, j, cols);
            let mut best = dp[index];
            let mut best_back = back[index].clone();

            for left_arc in left.incoming_arcs(i) {
                for right_arc in right.incoming_arcs(j) {
                    let cost = usize::from(left_arc.symbol != right_arc.symbol);
                    let candidate =
                        dp[distance_index(left_arc.src, right_arc.src, cols)].saturating_add(cost);
                    if candidate < best {
                        best = candidate;
                        best_back = Some(Backpointer {
                            prev_i: left_arc.src,
                            prev_j: right_arc.src,
                            step: TraceStep {
                                op: if cost == 0 {
                                    EditOp::Match
                                } else {
                                    EditOp::Substitute
                                },
                                left: Some(left_arc.symbol),
                                right: Some(right_arc.symbol),
                            },
                        });
                    }
                }
            }

            for left_arc in left.incoming_arcs(i) {
                let candidate = dp[distance_index(left_arc.src, j, cols)].saturating_add(1);
                if candidate < best {
                    best = candidate;
                    best_back = Some(Backpointer {
                        prev_i: left_arc.src,
                        prev_j: j,
                        step: TraceStep {
                            op: EditOp::Delete,
                            left: Some(left_arc.symbol),
                            right: None,
                        },
                    });
                }
            }

            for right_arc in right.incoming_arcs(j) {
                let candidate = dp[distance_index(i, right_arc.src, cols)].saturating_add(1);
                if candidate < best {
                    best = candidate;
                    best_back = Some(Backpointer {
                        prev_i: i,
                        prev_j: right_arc.src,
                        step: TraceStep {
                            op: EditOp::Insert,
                            left: None,
                            right: Some(right_arc.symbol),
                        },
                    });
                }
            }

            dp[index] = best;
            back[index] = best_back;
        }
    }

    let mut steps = Vec::new();
    let mut i = left.end();
    let mut j = right.end();
    while i != left.start() || j != right.start() {
        let Some(prev) = &back[distance_index(i, j, cols)] else {
            break;
        };
        steps.push(prev.step.clone());
        i = prev.prev_i;
        j = prev.prev_j;
    }
    steps.reverse();

    DistanceTrace {
        distance: dp[distance_index(left.end(), right.end(), cols)],
        steps,
    }
}

/// Returns whether lattice edit distance is at most `threshold`.
///
/// # Panics
///
/// Panics when the DP matrix would exceed [`MAX_DISTANCE_MATRIX_CELLS`].
/// Use [`try_within_distance`] for external or otherwise untrusted lattices.
pub fn within_distance(left: &Lattice, right: &Lattice, threshold: usize) -> bool {
    try_within_distance(left, right, threshold).expect("distance matrix should fit")
}

/// Returns whether lattice edit distance is at most `threshold`, with explicit
/// matrix-size validation.
pub fn try_within_distance(
    left: &Lattice,
    right: &Lattice,
    threshold: usize,
) -> Result<bool, DistanceError> {
    let rows = left.node_count();
    let cols = right.node_count();
    let cells = checked_distance_matrix_cells(rows, cols)?;
    Ok(within_distance_impl(left, right, threshold, cells))
}

fn within_distance_impl(left: &Lattice, right: &Lattice, threshold: usize, cells: usize) -> bool {
    let cols = right.node_count();
    let mut dp = vec![INF; cells];
    let mut queued = vec![false; cells];
    let mut queue = VecDeque::new();
    let start = distance_index(left.start(), right.start(), cols);
    dp[start] = 0;
    queued[start] = true;
    queue.push_back((left.start(), right.start()));
    let mut search = ThresholdSearch {
        threshold,
        cols,
        dp: &mut dp,
        queued: &mut queued,
        queue: &mut queue,
    };

    while let Some((i, j)) = search.queue.pop_front() {
        let index = distance_index(i, j, cols);
        search.queued[index] = false;
        let current = search.dp[index];
        if current > threshold {
            continue;
        }
        if i == left.end() && j == right.end() {
            return true;
        }

        for right_arc in right.outgoing_arcs(j) {
            search.relax(i, right_arc.dst, current.saturating_add(1));
        }

        for left_arc in left.outgoing_arcs(i) {
            search.relax(left_arc.dst, j, current.saturating_add(1));
        }

        for left_arc in left.outgoing_arcs(i) {
            for right_arc in right.outgoing_arcs(j) {
                let cost = usize::from(left_arc.symbol != right_arc.symbol);
                search.relax(left_arc.dst, right_arc.dst, current.saturating_add(cost));
            }
        }
    }

    false
}

/// Returns whether lattice Damerau-Levenshtein distance is at most
/// `threshold`.
///
/// # Panics
///
/// Panics when the DP matrix would exceed [`MAX_DISTANCE_MATRIX_CELLS`].
/// Use [`try_within_damerau_distance`] for external or otherwise untrusted
/// lattices.
pub fn within_damerau_distance(left: &Lattice, right: &Lattice, threshold: usize) -> bool {
    try_within_damerau_distance(left, right, threshold).expect("distance matrix should fit")
}

/// Returns whether lattice Damerau-Levenshtein distance is at most
/// `threshold`, with explicit matrix-size validation.
pub fn try_within_damerau_distance(
    left: &Lattice,
    right: &Lattice,
    threshold: usize,
) -> Result<bool, DistanceError> {
    let rows = left.node_count();
    let cols = right.node_count();
    let cells = checked_distance_matrix_cells(rows, cols)?;
    Ok(within_damerau_distance_impl(left, right, threshold, cells))
}

fn within_damerau_distance_impl(
    left: &Lattice,
    right: &Lattice,
    threshold: usize,
    cells: usize,
) -> bool {
    let cols = right.node_count();
    let mut dp = vec![INF; cells];
    let mut queued = vec![false; cells];
    let mut queue = VecDeque::new();
    let start = distance_index(left.start(), right.start(), cols);
    dp[start] = 0;
    queued[start] = true;
    queue.push_back((left.start(), right.start()));
    let mut search = ThresholdSearch {
        threshold,
        cols,
        dp: &mut dp,
        queued: &mut queued,
        queue: &mut queue,
    };

    while let Some((i, j)) = search.queue.pop_front() {
        let index = distance_index(i, j, cols);
        search.queued[index] = false;
        let current = search.dp[index];
        if current > threshold {
            continue;
        }
        if i == left.end() && j == right.end() {
            return true;
        }

        for right_arc in right.outgoing_arcs(j) {
            search.relax(i, right_arc.dst, current.saturating_add(1));
        }

        for left_arc in left.outgoing_arcs(i) {
            search.relax(left_arc.dst, j, current.saturating_add(1));
        }

        for left_arc in left.outgoing_arcs(i) {
            for right_arc in right.outgoing_arcs(j) {
                let cost = usize::from(left_arc.symbol != right_arc.symbol);
                search.relax(left_arc.dst, right_arc.dst, current.saturating_add(cost));
            }
        }

        for left_first in left.outgoing_arcs(i) {
            for right_first in right.outgoing_arcs(j) {
                for left_second in left.outgoing_arcs(left_first.dst) {
                    for right_second in right.outgoing_arcs(right_first.dst) {
                        if left_first.symbol == right_second.symbol
                            && left_second.symbol == right_first.symbol
                        {
                            search.relax(
                                left_second.dst,
                                right_second.dst,
                                current.saturating_add(1),
                            );
                        }
                    }
                }
            }
        }
    }

    false
}

fn checked_distance_matrix_cells(rows: usize, cols: usize) -> Result<usize, DistanceError> {
    checked_matrix_cells(rows, cols, MAX_DISTANCE_MATRIX_CELLS)
}

fn checked_trace_matrix_cells(rows: usize, cols: usize) -> Result<usize, DistanceError> {
    checked_matrix_cells(rows, cols, MAX_TRACE_MATRIX_CELLS)
}

fn checked_matrix_cells(
    rows: usize,
    cols: usize,
    max_cells: usize,
) -> Result<usize, DistanceError> {
    let cells = rows
        .checked_mul(cols)
        .ok_or(DistanceError::MatrixTooLarge {
            rows,
            cols,
            max_cells,
        })?;
    if cells > max_cells {
        return Err(DistanceError::MatrixTooLarge {
            rows,
            cols,
            max_cells,
        });
    }
    Ok(cells)
}

fn distance_index(i: usize, j: usize, cols: usize) -> usize {
    i * cols + j
}

struct ThresholdSearch<'a> {
    threshold: usize,
    cols: usize,
    dp: &'a mut [usize],
    queued: &'a mut [bool],
    queue: &'a mut VecDeque<(usize, usize)>,
}

impl ThresholdSearch<'_> {
    fn relax(&mut self, i: usize, j: usize, candidate: usize) {
        if candidate > self.threshold {
            return;
        }
        let index = distance_index(i, j, self.cols);
        if candidate >= self.dp[index] {
            return;
        }
        self.dp[index] = candidate;
        if !self.queued[index] {
            self.queued[index] = true;
            self.queue.push_back((i, j));
        }
    }
}

/// Converts an edit distance and sequence lengths to a similarity score.
///
/// The result is clamped to `0.0..=1.0`; equal empty inputs return `1.0`.
pub fn normalized_similarity_from_distance(
    distance: usize,
    left_len: usize,
    right_len: usize,
) -> f64 {
    let max_len = left_len.max(right_len);
    if max_len == 0 {
        return 1.0;
    }

    (1.0 - distance as f64 / max_len as f64).clamp(0.0, 1.0)
}

/// Computes normalized Levenshtein similarity for two strings.
pub fn normalized_similarity_str(left: &str, right: &str) -> f64 {
    normalized_similarity_from_distance(
        levenshtein_str(left, right),
        left.chars().count(),
        right.chars().count(),
    )
}

/// Computes Levenshtein distance over Unicode scalar values.
pub fn levenshtein_str(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    levenshtein_chars(&left, &right)
}

/// Computes optimal string alignment Damerau-Levenshtein distance.
///
/// # Panics
///
/// Panics when the DP matrix would exceed [`MAX_DISTANCE_MATRIX_CELLS`].
/// Use [`try_damerau_levenshtein_str`] for external or otherwise untrusted
/// strings.
pub fn damerau_levenshtein_str(left: &str, right: &str) -> usize {
    try_damerau_levenshtein_str(left, right).expect("distance matrix should fit")
}

/// Computes optimal string alignment Damerau-Levenshtein distance with
/// explicit matrix-size validation.
pub fn try_damerau_levenshtein_str(left: &str, right: &str) -> Result<usize, DistanceError> {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    damerau_levenshtein_chars(&left, &right)
}

fn levenshtein_chars(left: &[char], right: &[char]) -> usize {
    let (shorter, longer) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    let mut previous = (0..=shorter.len()).collect::<Vec<_>>();
    let mut current = vec![0; shorter.len() + 1];

    for (i, &longer_ch) in longer.iter().enumerate() {
        current[0] = i + 1;
        for (j, &shorter_ch) in shorter.iter().enumerate() {
            let substitution_cost = usize::from(longer_ch != shorter_ch);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[shorter.len()]
}

fn damerau_levenshtein_chars(left: &[char], right: &[char]) -> Result<usize, DistanceError> {
    let rows = left.len() + 1;
    let cols = right.len() + 1;
    let cells = checked_distance_matrix_cells(rows, cols)?;
    let mut dp = vec![0; cells];

    for i in 0..rows {
        dp[distance_index(i, 0, cols)] = i;
    }
    for j in 0..cols {
        dp[distance_index(0, j, cols)] = j;
    }

    for i in 1..rows {
        for j in 1..cols {
            let substitution_cost = usize::from(left[i - 1] != right[j - 1]);
            let mut best = (dp[distance_index(i - 1, j, cols)] + 1)
                .min(dp[distance_index(i, j - 1, cols)] + 1)
                .min(dp[distance_index(i - 1, j - 1, cols)] + substitution_cost);

            if i > 1 && j > 1 && left[i - 1] == right[j - 2] && left[i - 2] == right[j - 1] {
                best = best.min(dp[distance_index(i - 2, j - 2, cols)] + 1);
            }

            dp[distance_index(i, j, cols)] = best;
        }
    }

    Ok(dp[distance_index(left.len(), right.len(), cols)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols_to_string(symbols: &[Symbol]) -> String {
        symbols
            .iter()
            .map(|&symbol| char::from_u32(symbol).expect("test symbol should be a char"))
            .collect()
    }

    fn string_distance(left: &str, right: &str) -> usize {
        distance(&Lattice::from_paths([left]), &Lattice::from_paths([right]))
    }

    fn string_damerau_distance(left: &str, right: &str) -> usize {
        damerau_distance(&Lattice::from_paths([left]), &Lattice::from_paths([right]))
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < f64::EPSILON);
    }

    #[test]
    fn try_path_constructors_report_invalid_inputs() {
        assert!(matches!(
            Lattice::try_from_paths(std::iter::empty::<&str>()),
            Err(LatticeError::Empty)
        ));
        assert!(matches!(
            Lattice::try_from_paths(["", "a"]),
            Err(LatticeError::MixedEmptyAndNonEmptyPaths)
        ));
        assert!(matches!(
            Lattice::try_from_symbol_paths_compact([Vec::<Symbol>::new(), vec![1]]),
            Err(LatticeError::MixedEmptyAndNonEmptyPaths)
        ));
    }

    #[test]
    fn linear_lattice_matches_levenshtein_distance() {
        assert_eq!(string_distance("kitten", "sitting"), 3);
        assert_eq!(string_distance("insat", "insatu"), 1);
        assert_eq!(string_distance("abc", "abc"), 0);
        assert_eq!(string_distance("abc", "axc"), 1);
    }

    #[test]
    fn normalized_similarity_uses_max_length() {
        assert_close(
            normalized_similarity_from_distance(1, "abc".chars().count(), "adc".chars().count()),
            2.0 / 3.0,
        );
        assert_close(normalized_similarity_str("abc", "adc"), 2.0 / 3.0);
        assert_eq!(normalized_similarity_str("", ""), 1.0);
        assert_eq!(normalized_similarity_from_distance(4, 1, 2), 0.0);
    }

    #[test]
    fn parallel_paths_take_minimum_distance() {
        let left = Lattice::from_paths(["insat"]);
        let right = Lattice::from_paths(["insatu", "insat", "zzzzz"]);

        assert_eq!(distance(&left, &right), 0);
    }

    #[test]
    fn fallible_distance_apis_match_convenience_apis() {
        let left = Lattice::from_paths(["abcd"]);
        let right = Lattice::from_paths(["acbd"]);

        assert_eq!(
            try_distance(&left, &right).unwrap(),
            distance(&left, &right)
        );
        assert_eq!(
            try_damerau_distance(&left, &right).unwrap(),
            damerau_distance(&left, &right)
        );
        assert_eq!(
            try_distance_with_trace(&left, &right).unwrap(),
            distance_with_trace(&left, &right)
        );
        assert_eq!(
            try_within_distance(&left, &right, 2).unwrap(),
            within_distance(&left, &right, 2)
        );
        assert_eq!(
            try_within_damerau_distance(&left, &right, 1).unwrap(),
            within_damerau_distance(&left, &right, 1)
        );
    }

    #[test]
    fn fallible_distance_apis_reject_large_matrices() {
        let node_count = 4001;
        let lattice = Lattice::from_edges(node_count, 0, node_count - 1, Vec::new()).unwrap();

        assert!(matches!(
            try_distance(&lattice, &lattice),
            Err(DistanceError::MatrixTooLarge {
                rows: 4001,
                cols: 4001,
                max_cells: MAX_DISTANCE_MATRIX_CELLS,
            })
        ));
        assert!(matches!(
            try_damerau_distance(&lattice, &lattice),
            Err(DistanceError::MatrixTooLarge { .. })
        ));
        assert!(matches!(
            try_distance_with_trace(&lattice, &lattice),
            Err(DistanceError::MatrixTooLarge { .. })
        ));
        assert!(matches!(
            try_within_distance(&lattice, &lattice, 1),
            Err(DistanceError::MatrixTooLarge { .. })
        ));
        assert!(matches!(
            try_within_damerau_distance(&lattice, &lattice, 1),
            Err(DistanceError::MatrixTooLarge { .. })
        ));
    }

    #[test]
    fn trace_uses_lower_matrix_limit_than_plain_distance() {
        let node_count = 1415;
        let lattice = Lattice::from_edges(node_count, 0, node_count - 1, Vec::new()).unwrap();

        assert!(try_distance(&lattice, &lattice).is_ok());
        assert!(matches!(
            try_distance_with_trace(&lattice, &lattice),
            Err(DistanceError::MatrixTooLarge {
                rows: 1415,
                cols: 1415,
                max_cells: MAX_TRACE_MATRIX_CELLS,
            })
        ));
    }

    #[test]
    fn fallible_arc_accessors_report_out_of_range_nodes() {
        let lattice = Lattice::from_paths(["ab"]);

        assert_eq!(
            lattice.try_incoming_arcs(999).map(|arcs| arcs.count()),
            None
        );
        assert_eq!(
            lattice.try_outgoing_arcs(999).map(|arcs| arcs.count()),
            None
        );
        assert_eq!(
            lattice.try_outgoing_arcs(0).map(|arcs| arcs.count()),
            Some(1)
        );
    }

    #[test]
    fn trace_free_distance_matches_trace_distance() {
        let left = Lattice::from_paths(["insat", "insatu"]);
        let right = Lattice::from_paths(["inzatu", "insatsu"]);

        assert_eq!(
            distance(&left, &right),
            distance_with_trace(&left, &right).distance
        );
    }

    #[test]
    fn compact_paths_share_prefix_nodes() {
        let lattice = Lattice::from_symbol_paths_compact([
            "chadougu"
                .chars()
                .map(|ch| ch as Symbol)
                .collect::<Vec<_>>(),
            "chadoogu"
                .chars()
                .map(|ch| ch as Symbol)
                .collect::<Vec<_>>(),
        ]);

        assert_eq!(distance(&lattice, &Lattice::from_paths(["chadougu"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["chadoogu"])), 0);
        assert!(lattice.node_count() < Lattice::from_paths(["chadougu", "chadoogu"]).node_count());
    }

    #[test]
    fn compact_paths_share_equivalent_suffix_nodes() {
        let lattice = Lattice::from_symbol_paths_compact([
            "xab".chars().map(|ch| ch as Symbol).collect::<Vec<_>>(),
            "yab".chars().map(|ch| ch as Symbol).collect::<Vec<_>>(),
        ]);

        assert_eq!(distance(&lattice, &Lattice::from_paths(["xab"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["yab"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["zab"])), 1);
        assert!(lattice.node_count() < Lattice::from_paths(["xab", "yab"]).node_count());
    }

    #[test]
    fn compact_paths_preserve_prefix_words() {
        let lattice = Lattice::from_symbol_paths_compact([
            "a".chars().map(|ch| ch as Symbol).collect::<Vec<_>>(),
            "ab".chars().map(|ch| ch as Symbol).collect::<Vec<_>>(),
        ]);

        assert_eq!(distance(&lattice, &Lattice::from_paths(["a"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["ab"])), 0);
    }

    #[test]
    fn distance_with_trace_returns_best_path_pair() {
        let left = Lattice::from_paths(["chadougu"]);
        let right = Lattice::from_paths(["tyadougu", "chadougu"]);

        let trace = distance_with_trace(&left, &right);

        assert_eq!(trace.distance, 0);
        assert_eq!(symbols_to_string(&trace.left_symbols()), "chadougu");
        assert_eq!(symbols_to_string(&trace.right_symbols()), "chadougu");
        assert!(trace.steps.iter().all(|step| step.op == EditOp::Match));
    }

    #[test]
    fn trace_includes_insertions_and_deletions() {
        let trace = distance_with_trace(
            &Lattice::from_paths(["insat"]),
            &Lattice::from_paths(["insatu"]),
        );

        assert_eq!(trace.distance, 1);
        assert_eq!(symbols_to_string(&trace.left_symbols()), "insat");
        assert_eq!(symbols_to_string(&trace.right_symbols()), "insatu");
        assert_eq!(trace.steps.last().map(|step| step.op), Some(EditOp::Insert));
    }

    #[test]
    fn threshold_check_uses_distance() {
        let left = Lattice::from_paths(["insat"]);
        let right = Lattice::from_paths(["insatu"]);

        assert!(within_distance(&left, &right, 1));
        assert!(!within_distance(&left, &right, 0));
    }

    #[test]
    fn threshold_check_prunes_but_preserves_lattice_paths() {
        let left = Lattice::from_paths(["chadougu", "tyadougu"]);
        let right = Lattice::from_paths(["chadoogu", "zzzzzzzz"]);

        assert_eq!(distance(&left, &right), 1);
        assert!(within_distance(&left, &right, 1));
        assert!(!within_distance(&left, &right, 0));
    }

    #[test]
    fn linear_lattice_damerau_matches_string_damerau_distance() {
        for (left, right) in [
            ("ca", "ac"),
            ("abc", "acb"),
            ("abcdef", "abcedf"),
            ("moine", "mione"),
            ("マトリッツォ", "マリトッツォ"),
        ] {
            assert_eq!(
                string_damerau_distance(left, right),
                damerau_levenshtein_str(left, right),
                "{left:?} / {right:?}"
            );
        }
    }

    #[test]
    fn lattice_damerau_takes_transposition_across_candidate_paths() {
        let left = Lattice::from_paths(["abc", "axc"]);
        let right = Lattice::from_paths(["acb"]);

        assert_eq!(distance(&left, &right), 2);
        assert_eq!(damerau_distance(&left, &right), 1);
    }

    #[test]
    fn lattice_damerau_supports_branched_two_arc_transposition() {
        let left = Lattice::from_edges(
            4,
            0,
            3,
            vec![
                Arc::new(0, 1, 'a' as Symbol),
                Arc::new(0, 1, 'x' as Symbol),
                Arc::new(1, 3, 'b' as Symbol),
                Arc::new(1, 3, 'y' as Symbol),
            ],
        )
        .unwrap();
        let right = Lattice::from_paths(["ba"]);

        assert_eq!(damerau_distance(&left, &right), 1);
    }

    #[test]
    fn threshold_damerau_check_uses_lattice_damerau_distance() {
        let left = Lattice::from_paths(["abc", "axc"]);
        let right = Lattice::from_paths(["acb"]);

        assert!(within_damerau_distance(&left, &right, 1));
        assert!(!within_damerau_distance(&left, &right, 0));
    }

    #[test]
    fn from_edges_rejects_non_topological_arcs() {
        let result = Lattice::from_edges(2, 0, 1, vec![Arc::new(1, 0, 'a' as Symbol)]);

        assert!(matches!(
            result,
            Err(LatticeError::InvalidArcOrder { src: 1, dst: 0 })
        ));
    }

    #[test]
    fn surface_levenshtein_counts_unicode_chars() {
        assert_eq!(levenshtein_str("kitten", "sitting"), 3);
        assert_eq!(levenshtein_str("いんさt", "印刷"), 4);
        assert_eq!(levenshtein_str("マトリッツォ", "マリトッツォ"), 2);
    }

    #[test]
    fn surface_damerau_counts_adjacent_transposition() {
        assert_eq!(damerau_levenshtein_str("ca", "ac"), 1);
        assert_eq!(try_damerau_levenshtein_str("ca", "ac").unwrap(), 1);
        assert_eq!(damerau_levenshtein_str("マトリッツォ", "マリトッツォ"), 1);
        assert_eq!(damerau_levenshtein_str("いんさt", "印刷"), 4);
    }
}
