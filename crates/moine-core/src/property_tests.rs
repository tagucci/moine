use proptest::prelude::*;

use crate::{
    distance, distance_with_trace, try_distance_with_cutoff, within_distance, EditOp, Lattice,
    Symbol,
};

fn valid_path_set() -> impl Strategy<Value = Vec<Vec<Symbol>>> {
    // Empty paths are valid only as a singleton. A small alphabet and short
    // paths make shared compact prefixes and suffixes common while keeping the
    // all-path-pairs oracle cheap.
    prop_oneof![
        1 => Just(vec![Vec::new()]),
        9 => prop::collection::vec(prop::collection::vec(0u32..4, 1..5), 1..5),
    ]
}

fn reference_levenshtein(left: &[Symbol], right: &[Symbol]) -> usize {
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];

    for (left_index, left_symbol) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_symbol) in right.iter().enumerate() {
            let substitution = previous[right_index] + usize::from(left_symbol != right_symbol);
            let deletion = previous[right_index + 1] + 1;
            let insertion = current[right_index] + 1;
            current[right_index + 1] = substitution.min(deletion).min(insertion);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}

fn reference_lattice_distance(left_paths: &[Vec<Symbol>], right_paths: &[Vec<Symbol>]) -> usize {
    left_paths
        .iter()
        .flat_map(|left| {
            right_paths
                .iter()
                .map(move |right| reference_levenshtein(left, right))
        })
        .min()
        .expect("generated path sets are non-empty")
}

proptest! {
    #[test]
    fn lattice_distance_matches_minimum_path_pair(
        left_paths in valid_path_set(),
        right_paths in valid_path_set(),
    ) {
        let expected = reference_lattice_distance(&left_paths, &right_paths);
        let left = Lattice::from_symbol_paths(left_paths);
        let right = Lattice::from_symbol_paths(right_paths);

        prop_assert_eq!(distance(&left, &right), expected);
    }

    #[test]
    fn compact_lattices_preserve_distance(
        left_paths in valid_path_set(),
        right_paths in valid_path_set(),
    ) {
        let left = Lattice::from_symbol_paths(left_paths.clone());
        let right = Lattice::from_symbol_paths(right_paths.clone());
        let compact_left = Lattice::from_symbol_paths_compact(left_paths);
        let compact_right = Lattice::from_symbol_paths_compact(right_paths);
        let expected = distance(&left, &right);

        prop_assert_eq!(distance(&compact_left, &right), expected);
        prop_assert_eq!(distance(&left, &compact_right), expected);
        prop_assert_eq!(distance(&compact_left, &compact_right), expected);
    }

    #[test]
    fn lattice_distance_is_symmetric_and_reflexive(
        left_paths in valid_path_set(),
        right_paths in valid_path_set(),
    ) {
        let left = Lattice::from_symbol_paths_compact(left_paths);
        let right = Lattice::from_symbol_paths_compact(right_paths);

        prop_assert_eq!(distance(&left, &right), distance(&right, &left));
        prop_assert_eq!(distance(&left, &left), 0);
        prop_assert_eq!(distance(&right, &right), 0);
    }

    #[test]
    fn cutoff_apis_match_exact_distance(
        left_paths in valid_path_set(),
        right_paths in valid_path_set(),
        threshold in 0usize..9,
    ) {
        let left = Lattice::from_symbol_paths_compact(left_paths);
        let right = Lattice::from_symbol_paths_compact(right_paths);
        let exact = distance(&left, &right);
        let expected = (exact <= threshold).then_some(exact);

        prop_assert_eq!(
            try_distance_with_cutoff(&left, &right, threshold).unwrap(),
            expected,
        );
        prop_assert_eq!(within_distance(&left, &right, threshold), expected.is_some());
    }

    #[test]
    fn trace_matches_distance_and_selected_paths(
        left_paths in valid_path_set(),
        right_paths in valid_path_set(),
    ) {
        let left = Lattice::from_symbol_paths_compact(left_paths.clone());
        let right = Lattice::from_symbol_paths_compact(right_paths.clone());
        let trace = distance_with_trace(&left, &right);
        let edit_cost = trace
            .steps
            .iter()
            .filter(|step| step.op != EditOp::Match)
            .count();

        prop_assert_eq!(trace.distance, distance(&left, &right));
        prop_assert_eq!(edit_cost, trace.distance);
        prop_assert!(left_paths.contains(&trace.left_symbols()));
        prop_assert!(right_paths.contains(&trace.right_symbols()));

        for step in trace.steps {
            match step.op {
                EditOp::Match => {
                    prop_assert!(step.left.is_some());
                    prop_assert_eq!(step.left, step.right);
                }
                EditOp::Substitute => {
                    prop_assert!(step.left.is_some());
                    prop_assert!(step.right.is_some());
                    prop_assert_ne!(step.left, step.right);
                }
                EditOp::Delete => {
                    prop_assert!(step.left.is_some());
                    prop_assert_eq!(step.right, None);
                }
                EditOp::Insert => {
                    prop_assert_eq!(step.left, None);
                    prop_assert!(step.right.is_some());
                }
            }
        }
    }
}
