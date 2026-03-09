use crate::types::guess_types::RedactionGuess;
use std::collections::{BTreeMap, BTreeSet};

use super::common::{
    anchor_overlap_penalty_pt, candidate_width_penalty_pt, estimate_candidate_interval_pt,
    is_list_like_context, is_multi_span_row_guess, is_two_sided_anchor_context,
    normalize_candidate_key, promote_text_to_front, punctuation_context_penalty,
};

const JOINT_ASSIGNMENT_MIN_GROUP_ROWS: usize = 2;
const JOINT_ASSIGNMENT_MAX_ROWS: usize = 12;
const JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW: usize = 120;
const JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT: usize = 500;
const JOINT_ASSIGNMENT_BEAM_WIDTH: usize = 80;
const JOINT_ASSIGNMENT_DUPLICATE_PENALTY: f64 = 15.0;
const JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT: f64 = 1.0;
const JOINT_ASSIGNMENT_OVERLAP_PENALTY: f64 = 2.0;
const JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT: f64 = 40.0;
const JOINT_ASSIGNMENT_DENSE_GROUP_MAX_GAP_PT: f64 = 96.0;
const JOINT_ASSIGNMENT_WRAP_LINE_MAX_Y_GAP_PT: f64 = 18.0;
const JOINT_ASSIGNMENT_SAME_LINE_MAX_Y_DELTA_PT: f64 = 4.0;
const JOINT_ASSIGNMENT_WRAP_RESET_X_DELTA_PT: f64 = 16.0;
const JOINT_ASSIGNMENT_WRAP_FORWARD_X_ALLOWANCE_PT: f64 = 28.0;
const JOINT_ASSIGNMENT_NULL_DELTA: f64 = 2.0;
const JOINT_ASSIGNMENT_NULL_MIN_BEST_COST: f64 = 1.5;
const JOINT_ASSIGNMENT_EMPTY_ROW_NULL_COST: f64 = 0.0;

struct JointAssignmentOption {
    text: String,
    key: String,
    base_cost: f64,
    start_x_pt: f64,
    end_x_pt: f64,
    center_y_pt: f64,
}

struct JointAssignmentBeamState {
    cost: f64,
    selected: Vec<Option<String>>,
    used_keys: Vec<String>,
    prev_start_x_pt: f64,
    prev_end_x_pt: f64,
    prev_center_y_pt: f64,
}

struct RedactionGroup {
    indices: Vec<usize>,
    is_list_like: bool,
}

pub fn apply_row_joint_assignment(guesses: &mut [RedactionGuess]) -> BTreeSet<usize> {
    let groups = collect_redaction_groups(guesses, None, true, true, false);
    let mut promotions = Vec::<(usize, String)>::new();
    for group in groups {
        if let Some(selected) = solve_joint_assignment_group(guesses, &group) {
            for (guess_index, selected_text) in
                group.indices.iter().copied().zip(selected.into_iter())
            {
                if let Some(text) = selected_text {
                    promotions.push((guess_index, text));
                }
            }
        }
    }

    let mut assigned = BTreeSet::<usize>::new();
    for (guess_index, selected_text) in promotions {
        if let Some(guess) = guesses.get_mut(guess_index) {
            promote_text_to_front(guess, &selected_text);
            assigned.insert(guess_index);
        }
    }
    assigned
}

pub fn apply_row_sequence_consensus(
    guesses: &mut [RedactionGuess],
    skip_indices: &BTreeSet<usize>,
) {
    let groups = collect_redaction_groups(guesses, Some(skip_indices), false, false, true);
    for group in groups {
        if group.indices.len() < 2 {
            continue;
        }

        let mut used = BTreeSet::<String>::new();
        let duplicate_penalty_amount = if group.indices.len() >= 3 {
            3.0_f64
        } else {
            0.0_f64
        };
        // Greedy assignment for rows that didn't go through joint assignment.
        for guess_index in group.indices.iter().copied() {
            let guess = &mut guesses[guess_index];
            if guess.candidates.is_empty() {
                continue;
            }

            let mut best: Option<(String, f64)> = None;
            let max_scan = guess
                .candidates
                .len()
                .min(JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT);
            for (rank, candidate) in guess.candidates.iter().take(max_scan).enumerate() {
                let key = normalize_candidate_key(&candidate.text);
                let duplicate_penalty = if duplicate_penalty_amount > 0.0_f64 && used.contains(&key)
                {
                    duplicate_penalty_amount
                } else {
                    0.0_f64
                };

                let width_penalty = candidate_width_penalty_pt(guess, &candidate.text);
                let rank_penalty = rank as f64 * 0.05_f64;
                let cost =
                    (candidate.error_pt as f64) + duplicate_penalty + width_penalty + rank_penalty;

                match &best {
                    None => best = Some((candidate.text.clone(), cost)),
                    Some((_, best_cost)) if cost < *best_cost => {
                        best = Some((candidate.text.clone(), cost))
                    }
                    _ => {}
                }
            }

            if let Some((selected_text, _)) = best {
                used.insert(normalize_candidate_key(&selected_text));
                promote_text_to_front(guess, &selected_text);
            }
        }
    }
}

fn collect_redaction_groups(
    guesses: &[RedactionGuess],
    skip_indices: Option<&BTreeSet<usize>>,
    require_two_sided: bool,
    include_empty_candidate_rows: bool,
    allow_dense_geometry_signal: bool,
) -> Vec<RedactionGroup> {
    let mut by_page = BTreeMap::<u32, Vec<usize>>::new();
    for (index, guess) in guesses.iter().enumerate() {
        if skip_indices.is_some_and(|skip| skip.contains(&index)) {
            continue;
        }
        let has_candidates = !guess.candidates.is_empty() || !guess.exact_matches.is_empty();
        if !(guess.context.has_anchor_pair || (allow_dense_geometry_signal && has_candidates)) {
            continue;
        }
        if require_two_sided && !is_two_sided_anchor_context(guess) {
            continue;
        }
        if !include_empty_candidate_rows && guess.candidates.is_empty() {
            continue;
        }
        by_page.entry(guess.page_index).or_default().push(index);
    }

    let mut groups = Vec::<RedactionGroup>::new();
    for page_indices in by_page.values_mut() {
        page_indices.sort_by(|left_idx, right_idx| {
            let left = &guesses[*left_idx];
            let right = &guesses[*right_idx];
            let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
            let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
            left_center_y
                .partial_cmp(&right_center_y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.bbox
                        .x0
                        .partial_cmp(&right.bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left.bbox
                        .x1
                        .partial_cmp(&right.bbox.x1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let mut current = Vec::<usize>::new();
        for guess_index in page_indices.iter().copied() {
            if current.is_empty() {
                current.push(guess_index);
                continue;
            }
            let prev_index = *current.last().unwrap_or(&guess_index);
            let prev = &guesses[prev_index];
            let next = &guesses[guess_index];
            if rows_are_group_contiguous(prev, next, allow_dense_geometry_signal) {
                current.push(guess_index);
            } else {
                maybe_push_group(
                    &mut groups,
                    &mut current,
                    guesses,
                    allow_dense_geometry_signal,
                );
                current.push(guess_index);
            }
        }
        maybe_push_group(
            &mut groups,
            &mut current,
            guesses,
            allow_dense_geometry_signal,
        );
    }
    groups
}

fn maybe_push_group(
    groups: &mut Vec<RedactionGroup>,
    current: &mut Vec<usize>,
    guesses: &[RedactionGuess],
    allow_dense_geometry_signal: bool,
) {
    if current.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS || current.len() > JOINT_ASSIGNMENT_MAX_ROWS
    {
        current.clear();
        return;
    }
    let has_joint_signal = group_has_joint_assignment_signal(current, guesses);
    let has_dense_signal =
        allow_dense_geometry_signal && group_has_dense_geometry_signal(current, guesses);
    if !has_joint_signal && !has_dense_signal {
        current.clear();
        return;
    }
    let has_any_options = current.iter().copied().any(|index| {
        let guess = &guesses[index];
        !guess.candidates.is_empty() || !guess.exact_matches.is_empty()
    });
    if !has_any_options {
        current.clear();
        return;
    }
    let is_list_like = current.iter().copied().any(|index| {
        let guess = &guesses[index];
        is_list_like_context(
            guess.context.left_anchor_text.as_str(),
            guess.context.right_anchor_text.as_str(),
        )
    });
    groups.push(RedactionGroup {
        indices: std::mem::take(current),
        is_list_like,
    });
}

fn rows_are_group_contiguous(
    left: &RedactionGuess,
    right: &RedactionGuess,
    allow_dense_geometry_signal: bool,
) -> bool {
    if !joint_assignment_rows_are_compatible(left, right) {
        return false;
    }
    let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
    let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
    let y_delta = (right_center_y - left_center_y).abs();
    if y_delta <= JOINT_ASSIGNMENT_SAME_LINE_MAX_Y_DELTA_PT {
        let x_gap = (right.bbox.x0 as f64 - left.bbox.x1 as f64).max(0.0_f64);
        return x_gap <= JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT;
    }
    if right_center_y <= left_center_y || y_delta > JOINT_ASSIGNMENT_WRAP_LINE_MAX_Y_GAP_PT {
        return false;
    }
    let has_list_signal = is_multi_span_row_guess(left)
        || is_multi_span_row_guess(right)
        || is_list_like_context(
            left.context.left_anchor_text.as_str(),
            left.context.right_anchor_text.as_str(),
        )
        || is_list_like_context(
            right.context.left_anchor_text.as_str(),
            right.context.right_anchor_text.as_str(),
        );
    if !has_list_signal && !allow_dense_geometry_signal {
        return false;
    }
    let wraps_to_new_line =
        right.bbox.x0 as f64 + JOINT_ASSIGNMENT_WRAP_RESET_X_DELTA_PT < left.bbox.x0 as f64;
    let continues_forward =
        right.bbox.x0 as f64 <= left.bbox.x1 as f64 + JOINT_ASSIGNMENT_WRAP_FORWARD_X_ALLOWANCE_PT;
    wraps_to_new_line || continues_forward
}

fn group_has_dense_geometry_signal(group: &[usize], guesses: &[RedactionGuess]) -> bool {
    if group.len() < 3 {
        return false;
    }
    let mut dense_adjacencies = 0_usize;
    for window in group.windows(2) {
        let left = &guesses[window[0]];
        let right = &guesses[window[1]];
        if left.page_index != right.page_index {
            continue;
        }
        let left_center_y = ((left.bbox.y0 + left.bbox.y1) * 0.5_f32) as f64;
        let right_center_y = ((right.bbox.y0 + right.bbox.y1) * 0.5_f32) as f64;
        let y_delta = (right_center_y - left_center_y).abs();
        if y_delta <= JOINT_ASSIGNMENT_WRAP_LINE_MAX_Y_GAP_PT {
            let horizontal_gap = if right.bbox.x0 as f64 >= left.bbox.x1 as f64 {
                (right.bbox.x0 - left.bbox.x1) as f64
            } else if left.bbox.x0 as f64 >= right.bbox.x1 as f64 {
                (left.bbox.x0 - right.bbox.x1) as f64
            } else {
                0.0_f64
            };
            if horizontal_gap <= JOINT_ASSIGNMENT_DENSE_GROUP_MAX_GAP_PT {
                dense_adjacencies += 1;
            }
        }
    }
    dense_adjacencies >= 2
}

fn group_has_joint_assignment_signal(group: &[usize], guesses: &[RedactionGuess]) -> bool {
    group.iter().copied().any(|guess_index| {
        let guess = &guesses[guess_index];
        is_multi_span_row_guess(guess)
            || is_list_like_context(
                guess.context.left_anchor_text.as_str(),
                guess.context.right_anchor_text.as_str(),
            )
    })
}

fn joint_assignment_rows_are_compatible(left: &RedactionGuess, right: &RedactionGuess) -> bool {
    let same_font_name = match (
        left.context.anchor_font_name.as_deref(),
        right.context.anchor_font_name.as_deref(),
    ) {
        (Some(l), Some(r)) => l == r,
        _ => true,
    };
    let similar_font_size = match (
        left.context.anchor_font_size_pt,
        right.context.anchor_font_size_pt,
    ) {
        (Some(l), Some(r)) => (l - r).abs() <= 0.75_f32,
        _ => true,
    };
    let similar_h_scale = match (
        left.context.anchor_h_scale_pct,
        right.context.anchor_h_scale_pct,
    ) {
        (Some(l), Some(r)) => (l - r).abs() <= 8.0_f32,
        _ => true,
    };
    let close_row_bias = match (
        left.context.anchor_row_bias_pt,
        right.context.anchor_row_bias_pt,
    ) {
        (Some(l), Some(r)) => (l - r).abs() <= 5.0_f32,
        _ => true,
    };
    same_font_name && similar_font_size && similar_h_scale && close_row_bias
}

fn solve_joint_assignment_group(
    guesses: &[RedactionGuess],
    group: &RedactionGroup,
) -> Option<Vec<Option<String>>> {
    if group.indices.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS
        || group.indices.len() > JOINT_ASSIGNMENT_MAX_ROWS
    {
        return None;
    }

    let mut options_by_row = Vec::<Vec<JointAssignmentOption>>::with_capacity(group.indices.len());
    let mut null_costs = Vec::<f64>::with_capacity(group.indices.len());
    let mut allow_null_by_row = Vec::<bool>::with_capacity(group.indices.len());

    for guess_index in group.indices.iter().copied() {
        let guess = guesses.get(guess_index)?;
        let options = build_joint_assignment_options(
            guess,
            JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT,
            JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW,
        );
        if options.is_empty() {
            options_by_row.push(Vec::new());
            null_costs.push(JOINT_ASSIGNMENT_EMPTY_ROW_NULL_COST);
            allow_null_by_row.push(true);
            continue;
        }
        let best_cost = options
            .first()
            .map(|option| option.base_cost)
            .unwrap_or(5.0_f64);
        null_costs.push(best_cost + JOINT_ASSIGNMENT_NULL_DELTA);
        allow_null_by_row.push(best_cost >= JOINT_ASSIGNMENT_NULL_MIN_BEST_COST);
        options_by_row.push(options);
    }

    let mut beam = vec![JointAssignmentBeamState {
        cost: 0.0_f64,
        selected: Vec::new(),
        used_keys: Vec::new(),
        prev_start_x_pt: f64::NEG_INFINITY,
        prev_end_x_pt: f64::NEG_INFINITY,
        prev_center_y_pt: f64::NEG_INFINITY,
    }];
    let duplicate_penalty_amount = if group.indices.len() >= 3 {
        JOINT_ASSIGNMENT_DUPLICATE_PENALTY
    } else {
        0.0_f64
    };

    for ((row_options, null_cost), allow_null) in options_by_row
        .iter()
        .zip(null_costs.iter().copied())
        .zip(allow_null_by_row.iter().copied())
    {
        let mut next = Vec::<JointAssignmentBeamState>::new();
        for state in &beam {
            if allow_null {
                let mut selected_skip = state.selected.clone();
                selected_skip.push(None);
                next.push(JointAssignmentBeamState {
                    cost: state.cost + null_cost,
                    selected: selected_skip,
                    used_keys: state.used_keys.clone(),
                    prev_start_x_pt: state.prev_start_x_pt,
                    prev_end_x_pt: state.prev_end_x_pt,
                    prev_center_y_pt: state.prev_center_y_pt,
                });
            }
            for option in row_options {
                let mut cost = state.cost + option.base_cost;
                if duplicate_penalty_amount > 0.0_f64
                    && state.used_keys.iter().any(|key| key == &option.key)
                {
                    cost += duplicate_penalty_amount;
                }
                let same_line = state.prev_center_y_pt.is_finite()
                    && (option.center_y_pt - state.prev_center_y_pt).abs()
                        <= JOINT_ASSIGNMENT_SAME_LINE_MAX_Y_DELTA_PT;
                if same_line && state.prev_end_x_pt.is_finite() {
                    let overlap_pt = (state.prev_end_x_pt - option.start_x_pt
                        + JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT)
                        .max(0.0_f64);
                    cost += overlap_pt * JOINT_ASSIGNMENT_OVERLAP_PENALTY;
                    if option.start_x_pt + 0.5_f64 < state.prev_start_x_pt {
                        cost += 2.5_f64;
                    }
                }

                let mut selected = state.selected.clone();
                selected.push(Some(option.text.clone()));
                let mut used_keys = state.used_keys.clone();
                if !used_keys.iter().any(|key| key == &option.key) {
                    used_keys.push(option.key.clone());
                }
                next.push(JointAssignmentBeamState {
                    cost,
                    selected,
                    used_keys,
                    prev_start_x_pt: option.start_x_pt,
                    prev_end_x_pt: option.end_x_pt,
                    prev_center_y_pt: option.center_y_pt,
                });
            }
        }

        if next.is_empty() {
            return None;
        }
        next.sort_by(|left, right| {
            left.cost
                .partial_cmp(&right.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        next.truncate(JOINT_ASSIGNMENT_BEAM_WIDTH);
        beam = next;
    }

    let best_state = if group.is_list_like {
        beam.into_iter().min_by(|left, right| {
            left.cost
                .partial_cmp(&right.cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    } else {
        beam.into_iter().next()
    };
    best_state.map(|state| state.selected)
}

fn build_joint_assignment_options(
    guess: &RedactionGuess,
    scan_limit: usize,
    max_options: usize,
) -> Vec<JointAssignmentOption> {
    if guess.candidates.is_empty() && guess.exact_matches.is_empty() {
        return Vec::new();
    }
    let mut options = Vec::<JointAssignmentOption>::new();
    let mut seen = BTreeSet::<String>::new();
    let candidate_lookup = guess
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.text.trim().to_owned(),
                (candidate.error_pt as f64, candidate.width_pt),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let exact_lookup = guess
        .exact_matches
        .iter()
        .map(|value| value.trim().to_owned())
        .collect::<BTreeSet<_>>();
    let ordered = ordered_option_texts(guess, scan_limit);
    let center_y_pt = ((guess.bbox.y0 + guess.bbox.y1) * 0.5_f32) as f64;

    for (rank, text) in ordered.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        if !seen.insert(text.clone()) {
            continue;
        }

        let candidate_error = candidate_lookup
            .get(text)
            .map(|(error_pt, _)| *error_pt)
            .unwrap_or(0.75_f64 + rank as f64 * 0.15_f64);
        let candidate_width = candidate_lookup
            .get(text)
            .and_then(|(_, width_pt)| *width_pt);

        let context_penalty = punctuation_context_penalty(
            guess.context.left_anchor_text.as_str(),
            guess.context.right_anchor_text.as_str(),
            text,
        );
        let width_penalty = candidate_width_penalty_pt(guess, text);
        let rank_penalty = ((rank + 2) as f64).ln() * 0.10_f64;
        let exact_bonus = if exact_lookup.contains(text) {
            -0.25_f64
        } else {
            0.0_f64
        };
        let anchor_overlap_penalty = anchor_overlap_penalty_pt(
            guess.context.left_anchor_text.as_str(),
            guess.context.right_anchor_text.as_str(),
            text,
        );
        let base_cost = candidate_error
            + context_penalty
            + width_penalty
            + rank_penalty
            + exact_bonus
            + anchor_overlap_penalty;
        let (start_x_pt, end_x_pt) = estimate_candidate_interval_pt(guess, text, candidate_width);

        options.push(JointAssignmentOption {
            text: text.clone(),
            key: normalize_candidate_key(text),
            base_cost,
            start_x_pt,
            end_x_pt,
            center_y_pt,
        });
    }
    options.sort_by(|left, right| {
        left.base_cost
            .partial_cmp(&right.base_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.text.cmp(&right.text))
    });
    options.truncate(max_options);
    options
}

fn ordered_option_texts(guess: &RedactionGuess, scan_limit: usize) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();
    if scan_limit == 0 {
        return out;
    }
    for text in guess
        .candidates
        .iter()
        .map(|candidate| candidate.text.as_str())
        .chain(guess.exact_matches.iter().map(String::as_str))
    {
        let normalized = text.trim();
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.to_owned()) {
            out.push(normalized.to_owned());
        }
        if out.len() >= scan_limit {
            break;
        }
    }
    out
}
