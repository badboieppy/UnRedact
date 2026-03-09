pub(crate) fn normalize_transport_text(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn join_adjacent_run_text(left_text: &str, right_text: &str, gap_pt: f64) -> String {
    let left = normalize_transport_text(left_text);
    let right = normalize_transport_text(right_text);
    if left.is_empty() {
        return right;
    }
    if right.is_empty() {
        return left;
    }
    if should_insert_join_space(&left, &right, gap_pt) {
        format!("{left} {right}")
    } else {
        format!("{left}{right}")
    }
}

fn should_insert_join_space(left: &str, right: &str, gap_pt: f64) -> bool {
    let Some(left_last) = left.chars().last() else {
        return false;
    };
    let Some(right_first) = right.chars().next() else {
        return false;
    };
    if left_last.is_whitespace() || right_first.is_whitespace() {
        return false;
    }
    if matches!(right_first, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}') {
        return false;
    }
    if matches!(left_last, '(' | '[' | '{' | '/') {
        return false;
    }
    let left_wordish = left_last.is_alphanumeric() || left_last == ',';
    let right_wordish = right_first.is_alphanumeric() || right_first == '(';
    left_wordish && right_wordish && gap_pt >= 0.25_f64
}
