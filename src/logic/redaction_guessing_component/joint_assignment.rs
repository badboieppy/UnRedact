use crate::types::guess_types::RedactionGuess;
use std::collections::{BTreeMap, BTreeSet};

use super::common::{
    anchor_overlap_penalty_pt, candidate_width_penalty_pt, estimate_candidate_interval_pt,
    is_list_like_context, is_multi_span_row_guess, is_two_sided_anchor_context,
    normalize_candidate_key, promote_text_to_front, punctuation_context_penalty,
};

const JOINT_ASSIGNMENT_MIN_GROUP_ROWS: usize = 2;
const JOINT_ASSIGNMENT_MAX_ROWS: usize = 12;
const JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW: usize = 6;
const JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT: usize = 20;
const JOINT_ASSIGNMENT_BEAM_WIDTH: usize = 25;
const JOINT_ASSIGNMENT_DUPLICATE_PENALTY: f64 = 5.0;
const JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT: f64 = 1.0;
const JOINT_ASSIGNMENT_OVERLAP_PENALTY: f64 = 2.0;
const JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT: f64 = 40.0;
const JOINT_ASSIGNMENT_NULL_DELTA: f64 = 2.0;
const JOINT_ASSIGNMENT_NULL_MIN_BEST_COST: f64 = 1.5;

struct JointAssignmentOption {
    text: String,
    key: String,
    base_cost: f64,
    start_x_pt: f64,
    end_x_pt: f64,
}

struct JointAssignmentBeamState {
    cost: f64,
    selected: Vec<Option<String>>,
    used_keys: Vec<String>,
    prev_start_x_pt: f64,
    prev_end_x_pt: f64,
}

pub fn apply_row_joint_assignment(guesses: &mut [RedactionGuess]) -> BTreeSet<usize> {
    let mut rows = BTreeMap::<(u32, i32), Vec<usize>>::new();
    for (index, guess) in guesses.iter().enumerate() {
        if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
            continue;
        }
        let center_y = ((guess.bbox.y0 + guess.bbox.y1) * 0.5_f32) as f64;
        let y_bucket = (center_y / 6.0_f64).round() as i32;
        rows.entry((guess.page_index, y_bucket))
            .or_default()
            .push(index);
    }

    let mut promotions = Vec::<(usize, String)>::new();
    for indices in rows.values_mut() {
        if indices.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
            continue;
        }
        indices.sort_by(|left_idx, right_idx| {
            guesses[*left_idx]
                .bbox
                .x0
                .partial_cmp(&guesses[*right_idx].bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    guesses[*left_idx]
                        .bbox
                        .x1
                        .partial_cmp(&guesses[*right_idx].bbox.x1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let groups = collect_contiguous_multi_span_groups(indices, guesses);
        for group in groups {
            if let Some(selected) = solve_joint_assignment_group(guesses, &group) {
                for (guess_index, selected_text) in group.iter().copied().zip(selected.into_iter())
                {
                    if let Some(text) = selected_text {
                        promotions.push((guess_index, text));
                    }
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
    let mut rows = BTreeMap::<(u32, i32), Vec<usize>>::new();
    for (index, guess) in guesses.iter().enumerate() {
        if skip_indices.contains(&index) {
            continue;
        }
        if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
            continue;
        }
        let center_y = ((guess.bbox.y0 + guess.bbox.y1) * 0.5_f32) as f64;
        let y_bucket = (center_y / 6.0_f64).round() as i32;
        rows.entry((guess.page_index, y_bucket))
            .or_default()
            .push(index);
    }

    for indices in rows.values_mut() {
        if indices.len() < 2 {
            continue;
        }
        indices.sort_by(|left_idx, right_idx| {
            guesses[*left_idx]
                .bbox
                .x0
                .partial_cmp(&guesses[*right_idx].bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    guesses[*left_idx]
                        .bbox
                        .x1
                        .partial_cmp(&guesses[*right_idx].bbox.x1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let mut used = BTreeSet::<String>::new();
        let duplicate_penalty_amount = if indices.len() >= 3 { 3.0_f64 } else { 0.0_f64 };
        // Simple greedy assignment for rows that didn't go through joint assignment
        // but are on the same line (likely a table row or sentence).
        for guess_index in indices.iter().copied() {
            let guess = &mut guesses[guess_index];
            if guess.candidates.is_empty() {
                continue;
            }

            // Find best candidate considering duplicates in this row
            let mut best: Option<(String, f64)> = None;
            let max_scan = guess.candidates.len().min(80);

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

fn collect_contiguous_multi_span_groups(
    indices: &[usize],
    guesses: &[RedactionGuess],
) -> Vec<Vec<usize>> {
    let mut groups = Vec::<Vec<usize>>::new();
    let mut current = Vec::<usize>::new();

    for guess_index in indices.iter().copied() {
        let guess = &guesses[guess_index];
        if !is_joint_assignment_candidate_row(guess) {
            if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
                groups.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            continue;
        }

        if current.is_empty() {
            current.push(guess_index);
            continue;
        }

        let prev_index = *current.last().unwrap_or(&guess_index);
        let prev = &guesses[prev_index];
        let x_gap = (guess.bbox.x0 as f64 - prev.bbox.x1 as f64).max(0.0_f64);
        let contiguous = x_gap <= JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT
            && joint_assignment_rows_are_compatible(prev, guess);
        if contiguous {
            current.push(guess_index);
        } else {
            if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
                groups.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            current.push(guess_index);
        }
    }

    if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
        groups.push(current);
    }
    groups
        .into_iter()
        .filter(|group| {
            group.len() <= JOINT_ASSIGNMENT_MAX_ROWS
                && group_has_joint_assignment_signal(group, guesses)
        })
        .collect::<Vec<_>>()
}

fn is_joint_assignment_candidate_row(guess: &RedactionGuess) -> bool {
    guess.context.has_anchor_pair
        && !guess.candidates.is_empty()
        && is_two_sided_anchor_context(guess)
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
    let same_font_key = match (
        left.context.anchor_font_key.as_deref(),
        right.context.anchor_font_key.as_deref(),
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
    same_font_key && similar_font_size && similar_h_scale && close_row_bias
}

fn solve_joint_assignment_group(
    guesses: &[RedactionGuess],
    group: &[usize],
) -> Option<Vec<Option<String>>> {
    if group.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS || group.len() > JOINT_ASSIGNMENT_MAX_ROWS {
        return None;
    }

    let mut options_by_row = Vec::<Vec<JointAssignmentOption>>::with_capacity(group.len());
    let mut null_costs = Vec::<f64>::with_capacity(group.len());
    let mut allow_null_by_row = Vec::<bool>::with_capacity(group.len());

    for guess_index in group.iter().copied() {
        let guess = guesses.get(guess_index)?;
        let options = build_joint_assignment_options(
            guess,
            JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT,
            JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW,
        );
        if options.is_empty() {
            return None;
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
    }];
    let duplicate_penalty_amount = if group.len() >= 3 {
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
                });
            }
            for option in row_options {
                let mut cost = state.cost + option.base_cost;
                if duplicate_penalty_amount > 0.0_f64
                    && state.used_keys.iter().any(|key| key == &option.key)
                {
                    cost += duplicate_penalty_amount;
                }
                if state.prev_end_x_pt.is_finite() {
                    let overlap_pt = (state.prev_end_x_pt - option.start_x_pt
                        + JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT)
                        .max(0.0_f64);
                    cost += overlap_pt * JOINT_ASSIGNMENT_OVERLAP_PENALTY;
                    // Additional small penalty for being slightly out of order if we assume left-to-right reading
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

    beam.into_iter().next().map(|state| state.selected)
}

fn build_joint_assignment_options(
    guess: &RedactionGuess,
    scan_limit: usize,
    max_options: usize,
) -> Vec<JointAssignmentOption> {
    if guess.candidates.is_empty() {
        return Vec::new();
    }
    let mut options = Vec::<JointAssignmentOption>::new();
    let mut seen = BTreeSet::<String>::new();
    let scan = guess.candidates.len().min(scan_limit);
    for (rank, candidate) in guess.candidates.iter().take(scan).enumerate() {
        let text = candidate.text.trim();
        if text.is_empty() {
            continue;
        }
        if !seen.insert(text.to_owned()) {
            continue;
        }

        let context_penalty = punctuation_context_penalty(
            guess.context.left_anchor_text.as_str(),
            guess.context.right_anchor_text.as_str(),
            text,
        );
        let width_penalty = candidate_width_penalty_pt(guess, text);
        let rank_penalty = ((rank + 2) as f64).ln() * 0.10_f64;

        let exact_bonus = if guess.exact_matches.iter().any(|value| value == text) {
            -0.25_f64
        } else {
            0.0_f64
        };

        let anchor_overlap_penalty = anchor_overlap_penalty_pt(
            guess.context.left_anchor_text.as_str(),
            guess.context.right_anchor_text.as_str(),
            text,
        );

        let base_cost = (candidate.error_pt as f64)
            + context_penalty
            + width_penalty
            + rank_penalty
            + exact_bonus
            + anchor_overlap_penalty;

        let (start_x_pt, end_x_pt) =
            estimate_candidate_interval_pt(guess, text, candidate.width_pt);

        options.push(JointAssignmentOption {
            text: text.to_owned(),
            key: normalize_candidate_key(text),
            base_cost,
            start_x_pt,
            end_x_pt,
        });
    }
    options.sort_by(|left, right| {
        left.base_cost
            .partial_cmp(&right.base_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    options.truncate(max_options);
    options
}
