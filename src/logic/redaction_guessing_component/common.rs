use crate::types::guess_types::RedactionGuess;
use std::collections::BTreeSet;

pub fn promote_text_to_front(guess: &mut RedactionGuess, selected_text: &str) {
    if let Some(pos) = guess
        .candidates
        .iter()
        .position(|candidate| candidate.text == selected_text)
    {
        let chosen = guess.candidates.remove(pos);
        guess.candidates.insert(0, chosen);
    }
    if let Some(pos) = guess
        .exact_matches
        .iter()
        .position(|value| value == selected_text)
    {
        let chosen = guess.exact_matches.remove(pos);
        guess.exact_matches.insert(0, chosen);
    }
}

pub fn is_multi_span_row_guess(guess: &RedactionGuess) -> bool {
    if !guess.context.has_anchor_pair {
        return false;
    }
    if !is_two_sided_anchor_context(guess) {
        return false;
    }
    let width = guess.bbox.width().abs() as f64;
    if width <= 0.0_f64 {
        return false;
    }
    // If the gap between anchors is significantly larger than the redaction box,
    // it implies multiple words might fit or it's a wide spacing scenario.
    // However, the original logic in guess_logic was checking aspect ratio or similar.
    // In joint_assignment.rs I used:
    // let ratio = w / h; ratio >= 8.0
    // Let's stick to the one used in guess_logic if possible, or a robust shared one.
    // guess_logic.rs L665:
    /*
    let width = guess.bbox.width().abs() as f64;
    if width <= 0.0_f64 { return false; }
    let height = guess.bbox.height().abs() as f64;
    if height <= 0.0_f64 { return false; }
    let ratio = width / height;
    ratio >= 8.0_f64
    */
    let height = guess.bbox.height().abs() as f64;
    if height <= 0.0_f64 {
        return false;
    }
    let ratio = width / height;
    ratio >= 8.0_f64
}

pub fn is_two_sided_anchor_context(guess: &RedactionGuess) -> bool {
    guess
        .context
        .anchor_mode
        .as_deref()
        .map(|mode| mode == "two_sided")
        .unwrap_or(true)
}

pub fn is_list_like_context(left_anchor_text: &str, right_anchor_text: &str) -> bool {
    let left_lower = left_anchor_text.trim().to_ascii_lowercase();
    let right_lower = right_anchor_text.trim().to_ascii_lowercase();
    left_lower.contains("including")
        || left_lower.contains("included")
        || left_lower.contains("among")
        || left_lower.contains("served")
        || left_lower.ends_with(',')
        || right_lower.starts_with(',')
        || right_lower.ends_with(',')
        || right_lower.starts_with("and ")
}

pub fn normalize_candidate_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

pub fn candidate_width_penalty_pt(guess: &RedactionGuess, text: &str) -> f64 {
    let char_width = effective_char_width_pt(guess);
    let target = guess.bbox.width().abs() as f64;
    if target <= 0.0_f64 {
        return 0.0_f64;
    }
    let estimated = candidate_char_units(text) * char_width;
    ((estimated - target).abs() / target.max(1.0_f64)).min(3.0_f64)
}

pub fn candidate_char_units(text: &str) -> f64 {
    let glyph_count = text
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != ',')
        .count()
        .max(1) as f64;
    let spaces = text.chars().filter(|ch| ch.is_whitespace()).count() as f64;
    glyph_count + spaces * 0.45_f64
}

fn effective_char_width_pt(guess: &RedactionGuess) -> f64 {
    let width = guess.context.char_width_pt as f64;
    if width.is_finite() && width > 0.0_f64 {
        width.max(0.1_f64)
    } else {
        4.0_f64
    }
}

pub fn anchor_overlap_penalty_pt(
    left_anchor_text: &str,
    right_anchor_text: &str,
    candidate: &str,
) -> f64 {
    let anchor_tokens = tokenize_alpha_words(left_anchor_text)
        .into_iter()
        .chain(tokenize_alpha_words(right_anchor_text))
        .collect::<BTreeSet<_>>();
    if anchor_tokens.is_empty() {
        return 0.0_f64;
    }
    let candidate_tokens = tokenize_alpha_words(candidate);
    let overlap = candidate_tokens
        .iter()
        .filter(|t| anchor_tokens.contains(*t))
        .count();
    if overlap > 0 {
        5.0_f64
    } else {
        0.0_f64
    }
}

pub fn tokenize_alpha_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '\'' && ch != '-')
        .map(|token| token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric()))
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>()
}

pub fn punctuation_context_penalty(
    left_anchor_text: &str,
    right_anchor_text: &str,
    candidate: &str,
) -> f64 {
    let left_lower = left_anchor_text.trim().to_ascii_lowercase();
    let right_lower = right_anchor_text.trim().to_ascii_lowercase();
    let candidate_trim = candidate.trim();
    if candidate_trim.is_empty() {
        return 5.0;
    }

    let word_count = candidate_trim.split_whitespace().count();
    let mut penalty = 0.0_f64;
    let list_context = is_list_like_context(&left_lower, &right_lower);

    if list_context {
        if word_count == 1 || word_count >= 5 {
            penalty += 0.20_f64;
        }
        if candidate_trim.contains('-') {
            penalty += 0.30_f64;
        }
        if candidate_trim.contains(',') {
            penalty += 0.45_f64;
        }
        if candidate_trim.contains('(') || candidate_trim.contains(')') {
            penalty += 0.40_f64;
        }
        if candidate_trim.contains('/') || candidate_trim.contains('&') {
            penalty += 0.50_f64;
        }
    }

    if (right_lower.starts_with(',') || right_lower.starts_with("and "))
        && (candidate_trim.ends_with(',') || candidate_trim.ends_with(';'))
    {
        penalty += 0.25_f64;
    }

    if candidate_trim.chars().any(|ch| ch.is_ascii_digit()) {
        penalty += 0.20_f64;
    }

    penalty.max(0.0)
}

pub fn estimate_candidate_interval_pt(
    guess: &RedactionGuess,
    text: &str,
    measured_width_pt: Option<f32>,
) -> (f64, f64) {
    let char_width = effective_char_width_pt(guess);
    let fallback_width = candidate_char_units(text) * char_width;
    let width_pt = measured_width_pt
        .map(|value| value as f64)
        .filter(|value| value.is_finite() && *value > 0.0_f64)
        .unwrap_or(fallback_width)
        .max(0.1_f64);

    let start = match guess.context.anchor_mode.as_deref() {
        Some("right_only") => {
            let right_x = guess
                .context
                .anchor_right_x
                .map(|value| value as f64)
                .unwrap_or(guess.bbox.x1 as f64);
            (right_x - width_pt).min(guess.bbox.x1 as f64 - 0.1_f64)
        }
        _ => guess.bbox.x0 as f64,
    };
    (start, start + width_pt)
}
