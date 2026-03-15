use std::collections::BTreeSet;

const MAX_NAME_VARIANTS_PER_ENTRY: usize = 24;
const MAX_TOKEN_VARIANTS_PER_ROLE: usize = 8;
const MAX_ROLE_SIDE_VARIANTS: usize = 24;
const MAX_ROLE_COMBINATIONS_PER_ENTRY: usize = 64;
const NAME_PREFIX_TOKENS: [&str; 24] = [
    "mr",
    "mrs",
    "ms",
    "mx",
    "dr",
    "prof",
    "sir",
    "dame",
    "lady",
    "lord",
    "rev",
    "fr",
    "judge",
    "hon",
    "capt",
    "cmdr",
    "col",
    "gen",
    "adm",
    "pres",
    "president",
    "governor",
    "lt",
    "sgt",
];
const NAME_SUFFIX_TOKENS: [&str; 24] = [
    "jr", "sr", "ii", "iii", "iv", "v", "vi", "phd", "md", "esq", "esquire", "jd", "dds", "dmd",
    "do", "rn", "cpa", "mba", "qc", "kc", "ret", "retired", "junior", "senior",
];
const NAME_SURNAME_PARTICLE_TOKENS: [&str; 28] = [
    "al", "ap", "ben", "bin", "da", "dal", "de", "del", "dela", "della", "der", "di", "dos", "du",
    "el", "ibn", "la", "le", "st", "st.", "ter", "van", "vanden", "vander", "von", "zu", "zum",
    "zur",
];

#[inline]
pub fn build_dictionary_variants(dictionary: &[String]) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = BTreeSet::<String>::new();
    for entry in dictionary {
        let canonical = normalize_dictionary_entry(entry);
        if canonical.is_empty() {
            continue;
        }
        for variant in build_name_variants(&canonical) {
            let trimmed = variant.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_owned()) {
                out.push(trimmed.to_owned());
            }
        }
    }
    out
}

fn normalize_dictionary_entry(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_space = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !in_space && !out.is_empty() {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    out.trim().to_owned()
}

fn build_name_variants(canonical: &str) -> Vec<String> {
    let mut template_seen = BTreeSet::<String>::new();
    let mut templates = Vec::<String>::new();
    let tokens = canonical
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let preserve_raw_input_shape = should_preserve_raw_input_shape(canonical, &tokens);
    push_template_variant(
        &mut template_seen,
        &mut templates,
        canonical,
        preserve_raw_input_shape,
        canonical.to_owned(),
    );

    if !tokens.is_empty() && has_special_name_structure(canonical, &tokens) {
        let parts = parse_name_parts(canonical, &tokens);
        let core = join_name_tokens(&parts.core_tokens);
        let given = join_name_tokens(&parts.given_tokens);
        let surname = join_name_tokens(&parts.surname_tokens);
        let given_first = parts.given_tokens.first().cloned().unwrap_or_default();
        let surname_last = parts.surname_tokens.last().cloned().unwrap_or_default();
        let prefix = join_name_tokens(&parts.prefix_tokens);
        let suffix = join_name_tokens(&parts.suffix_tokens);

        if !core.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                core.clone(),
            );
        }
        if !given_first.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{given_first} {surname}"),
            );
        }
        if !surname.is_empty() && !given_first.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{surname}, {given_first}"),
            );
        }
        if !prefix.is_empty() && !given_first.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{prefix} {given_first} {surname}"),
            );
        }
        if !suffix.is_empty() && !given_first.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{given_first} {surname} {suffix}"),
            );
        }
        if !prefix.is_empty()
            && !suffix.is_empty()
            && !given_first.is_empty()
            && !surname.is_empty()
        {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{prefix} {given_first} {surname} {suffix}"),
            );
        }
        if !given.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{given} {surname}"),
            );
        }
        if !given.is_empty() && !surname.is_empty() && !suffix.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{given} {surname} {suffix}"),
            );
        }
        if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{prefix} {given} {surname}"),
            );
        }
        if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() && !suffix.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{prefix} {given} {surname} {suffix}"),
            );
        }
        if !given_first.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                given_first.clone(),
            );
        }
        if !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                surname.clone(),
            );
        }
        if !surname_last.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                surname_last,
            );
        }
        if !prefix.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{prefix} {surname}"),
            );
        }
        if !suffix.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{surname} {suffix}"),
            );
        }
        if !core.is_empty() && !canonical.contains(',') {
            let mut split = core.split_whitespace();
            if let (Some(first), Some(last)) = (split.next(), core.split_whitespace().last()) {
                if first != last {
                    push_template_variant(
                        &mut template_seen,
                        &mut templates,
                        canonical,
                        preserve_raw_input_shape,
                        format!("{last}, {first}"),
                    );
                }
            }
        }
        if parts.given_tokens.len() >= 2 && !surname.is_empty() {
            let middle_initials = parts.given_tokens[1..]
                .iter()
                .filter_map(|value| value.chars().next())
                .map(|ch| format!("{ch}."))
                .collect::<Vec<_>>()
                .join(" ");
            if !middle_initials.is_empty() && !given_first.is_empty() {
                push_template_variant(
                    &mut template_seen,
                    &mut templates,
                    canonical,
                    preserve_raw_input_shape,
                    format!("{given_first} {middle_initials} {surname}"),
                );
            }
        }
    } else if tokens.len() >= 2 {
        let first = tokens[0];
        let last = tokens[tokens.len() - 1];
        push_template_variant(
            &mut template_seen,
            &mut templates,
            canonical,
            preserve_raw_input_shape,
            format!("{first} {last}"),
        );
        if !canonical.contains(',') {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                canonical,
                preserve_raw_input_shape,
                format!("{last}, {first}"),
            );
        }
        push_template_variant(
            &mut template_seen,
            &mut templates,
            canonical,
            preserve_raw_input_shape,
            first.to_owned(),
        );
        push_template_variant(
            &mut template_seen,
            &mut templates,
            canonical,
            preserve_raw_input_shape,
            last.to_owned(),
        );
    }
    if tokens.len() >= 2 {
        let parts = parse_name_parts(canonical, &tokens);
        if should_add_role_aware_aliases(&parts) {
            add_role_aware_alias_templates(
                canonical,
                preserve_raw_input_shape,
                &parts,
                &mut template_seen,
                &mut templates,
            );
        }
    }

    finalize_name_variants(&templates)
}

fn has_special_name_structure(canonical: &str, tokens: &[&str]) -> bool {
    canonical.contains(',')
        || tokens
            .iter()
            .any(|token| is_name_prefix_token(token) || is_name_suffix_token(token))
        || tokens
            .iter()
            .take(tokens.len().saturating_sub(1))
            .any(|token| is_surname_particle_token(token))
}

fn should_preserve_raw_input_shape(canonical: &str, tokens: &[&str]) -> bool {
    if tokens.len() <= 1 {
        return true;
    }
    !has_special_name_structure(canonical, tokens) && tokens.len() <= 2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameTokenRole {
    Given,
    Surname,
}

fn add_role_aware_alias_templates(
    canonical: &str,
    preserve_raw_input_shape: bool,
    parts: &NameParts,
    template_seen: &mut BTreeSet<String>,
    templates: &mut Vec<String>,
) {
    if parts.given_tokens.is_empty() || parts.surname_tokens.is_empty() {
        return;
    }
    let given_side = expand_role_side_variants(&parts.given_tokens, NameTokenRole::Given);
    let surname_side = expand_role_side_variants(&parts.surname_tokens, NameTokenRole::Surname);
    if given_side.is_empty() || surname_side.is_empty() {
        return;
    }

    let mut combo_count = 0_usize;
    for given in &given_side {
        for surname in &surname_side {
            if combo_count >= MAX_ROLE_COMBINATIONS_PER_ENTRY {
                return;
            }
            push_template_variant(
                template_seen,
                templates,
                canonical,
                preserve_raw_input_shape,
                format!("{given} {surname}"),
            );
            combo_count += 1;
            if combo_count >= MAX_ROLE_COMBINATIONS_PER_ENTRY {
                return;
            }
            push_template_variant(
                template_seen,
                templates,
                canonical,
                preserve_raw_input_shape,
                format!("{surname}, {given}"),
            );
            combo_count += 1;
        }
    }
}

fn should_add_role_aware_aliases(parts: &NameParts) -> bool {
    if parts.given_tokens.is_empty() || parts.surname_tokens.is_empty() {
        return false;
    }
    parts.given_tokens.iter().enumerate().any(|(idx, token)| {
        let allow_aliases = idx == 0_usize;
        token_has_role_alias_signal(token, NameTokenRole::Given, allow_aliases)
    }) || parts
        .surname_tokens
        .iter()
        .any(|token| token_has_role_alias_signal(token, NameTokenRole::Surname, true))
}

fn token_has_role_alias_signal(token: &str, role: NameTokenRole, allow_aliases: bool) -> bool {
    let normalized = normalize_dictionary_entry(token);
    if normalized.is_empty() {
        return false;
    }
    if fold_latin_text(&normalized) != normalized {
        return true;
    }
    if normalized.contains('-') {
        return true;
    }
    allow_aliases && !aliases_for_role_token(role, token).is_empty()
}

fn expand_role_side_variants(tokens: &[String], role: NameTokenRole) -> Vec<String> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut per_token_variants = Vec::<Vec<String>>::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        let allow_aliases = match role {
            NameTokenRole::Given => idx == 0_usize,
            NameTokenRole::Surname => true,
        };
        let mut expanded = expand_role_token_variants(token, role, allow_aliases);
        if expanded.len() > MAX_TOKEN_VARIANTS_PER_ROLE {
            expanded.truncate(MAX_TOKEN_VARIANTS_PER_ROLE);
        }
        if expanded.is_empty() {
            expanded.push(token.clone());
        }
        per_token_variants.push(expanded);
    }

    let mut states = vec![Vec::<String>::new()];
    for variants in per_token_variants {
        let mut next = Vec::<Vec<String>>::new();
        for state in &states {
            for variant in &variants {
                let mut joined = state.clone();
                joined.push(variant.clone());
                next.push(joined);
                if next.len() >= MAX_ROLE_SIDE_VARIANTS {
                    break;
                }
            }
            if next.len() >= MAX_ROLE_SIDE_VARIANTS {
                break;
            }
        }
        states = next;
        if states.is_empty() {
            break;
        }
    }

    let mut seen = BTreeSet::<String>::new();
    let mut out = Vec::<String>::new();
    for state in states {
        let phrase = normalize_dictionary_entry(&state.join(" "));
        if phrase.is_empty() {
            continue;
        }
        if seen.insert(phrase.clone()) {
            out.push(phrase);
        }
        if out.len() >= MAX_ROLE_SIDE_VARIANTS {
            break;
        }
    }
    if out.is_empty() {
        let fallback = join_name_tokens(tokens);
        if !fallback.is_empty() {
            out.push(fallback);
        }
    }
    out
}

fn expand_role_token_variants(
    token: &str,
    role: NameTokenRole,
    allow_aliases: bool,
) -> Vec<String> {
    let mut seen = BTreeSet::<String>::new();
    let mut out = Vec::<String>::new();
    push_unique_variant(&mut seen, &mut out, token.to_owned());
    add_orthographic_token_variants(token, &mut seen, &mut out);

    if allow_aliases {
        for alias in aliases_for_role_token(role, token) {
            push_unique_variant(&mut seen, &mut out, (*alias).to_owned());
            add_orthographic_token_variants(alias, &mut seen, &mut out);
            if out.len() >= MAX_TOKEN_VARIANTS_PER_ROLE {
                break;
            }
        }
    }

    if out.len() > MAX_TOKEN_VARIANTS_PER_ROLE {
        out.truncate(MAX_TOKEN_VARIANTS_PER_ROLE);
    }
    out
}

fn add_orthographic_token_variants(
    token: &str,
    seen: &mut BTreeSet<String>,
    out: &mut Vec<String>,
) {
    let normalized = normalize_dictionary_entry(token);
    if normalized.is_empty() {
        return;
    }
    let folded = fold_latin_text(&normalized);
    if folded != normalized {
        push_unique_variant(seen, out, folded.clone());
    }

    let candidates = [
        normalized.replace('-', " "),
        normalized.replace(' ', "-"),
        normalized.replace('-', ""),
        normalized.replace(' ', ""),
        folded.replace('-', " "),
        folded.replace(' ', "-"),
    ];
    for candidate in candidates {
        if !candidate.is_empty() {
            push_unique_variant(seen, out, candidate);
        }
    }
}

fn aliases_for_role_token(role: NameTokenRole, token: &str) -> &'static [&'static str] {
    let key = alias_lookup_key(token);
    match role {
        NameTokenRole::Given => given_name_aliases(&key),
        NameTokenRole::Surname => surname_aliases(&key),
    }
}

fn given_name_aliases(key: &str) -> &'static [&'static str] {
    match key {
        "leslie" => &["Leslie", "Lesley", "Les"],
        "lesley" => &["Lesley", "Leslie", "Les"],
        "les" => &["Les", "Leslie", "Lesley"],
        "jeanluc" => &["Jean-Luc", "Jean Luc", "Jeanluc"],
        _ => &[],
    }
}

fn surname_aliases(key: &str) -> &'static [&'static str] {
    match key {
        "rodgers" => &["Rodgers", "Rogers"],
        "rogers" => &["Rogers", "Rodgers"],
        "mucinska" => &["Mucińska", "Mucinska"],
        "marcinko" => &["Marcinko", "Marcinkova"],
        "marcinkova" => &["Marcinkova", "Marcinko"],
        _ => &[],
    }
}

fn alias_lookup_key(value: &str) -> String {
    fold_latin_text(value)
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>()
}

fn fold_latin_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        let folded = fold_latin_char(ch);
        if ch.is_uppercase() {
            out.push(folded.to_ascii_uppercase());
        } else {
            out.push(folded);
        }
    }
    out
}

fn fold_latin_char(ch: char) -> char {
    match ch {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'ç' | 'ć' | 'č' | 'Ç' | 'Ć' | 'Č' => 'c',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ł' | 'Ł' => 'l',
        'ñ' | 'ń' | 'Ñ' | 'Ń' => 'n',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ś' | 'Ś' => 's',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => 'y',
        'ż' | 'ź' | 'Ž' | 'ž' | 'Ż' | 'Ź' => 'z',
        _ => ch,
    }
}

fn finalize_name_variants(templates: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::<String>::new();
    let mut out = Vec::<String>::new();
    for template in templates {
        push_unique_variant(&mut seen, &mut out, template.clone());
        push_unique_variant(&mut seen, &mut out, template.to_uppercase());
        push_unique_variant(&mut seen, &mut out, template.to_lowercase());
        push_unique_variant(&mut seen, &mut out, title_case_text(template));
        if out.len() >= MAX_NAME_VARIANTS_PER_ENTRY {
            break;
        }
    }
    if out.len() > MAX_NAME_VARIANTS_PER_ENTRY {
        out.truncate(MAX_NAME_VARIANTS_PER_ENTRY);
    }
    out
}

#[derive(Debug, Clone, Default)]
struct NameParts {
    prefix_tokens: Vec<String>,
    given_tokens: Vec<String>,
    surname_tokens: Vec<String>,
    suffix_tokens: Vec<String>,
    core_tokens: Vec<String>,
}

fn parse_name_parts(canonical: &str, tokens: &[&str]) -> NameParts {
    parse_comma_name_parts(canonical).unwrap_or_else(|| parse_space_name_parts(tokens))
}

fn parse_comma_name_parts(canonical: &str) -> Option<NameParts> {
    let (left, right) = canonical.split_once(',')?;
    let left_tokens = split_name_tokens(left);
    let right_tokens = split_name_tokens(right);
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return None;
    }
    let (prefix_tokens, mut right_core_tokens, suffix_tokens) = split_prefix_suffix(&right_tokens);
    if right_core_tokens.is_empty() {
        right_core_tokens = right_tokens;
    }
    let given_tokens = right_core_tokens;
    let surname_tokens = left_tokens;
    let mut core_tokens = Vec::<String>::new();
    core_tokens.extend(given_tokens.iter().cloned());
    core_tokens.extend(surname_tokens.iter().cloned());
    if core_tokens.is_empty() {
        core_tokens.extend(surname_tokens.iter().cloned());
    }
    Some(NameParts {
        prefix_tokens,
        given_tokens,
        surname_tokens,
        suffix_tokens,
        core_tokens,
    })
}

fn parse_space_name_parts(tokens: &[&str]) -> NameParts {
    let all_tokens = tokens
        .iter()
        .map(|token| normalize_dictionary_entry(token))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let (prefix_tokens, mut core_tokens, suffix_tokens) = split_prefix_suffix(&all_tokens);
    if core_tokens.is_empty() {
        core_tokens = all_tokens;
    }
    let (mut given_tokens, mut surname_tokens) = split_given_surname(&core_tokens);
    if given_tokens.is_empty() && !core_tokens.is_empty() {
        given_tokens.push(core_tokens[0].clone());
    }
    if surname_tokens.is_empty() && !core_tokens.is_empty() {
        surname_tokens.push(core_tokens[core_tokens.len() - 1].clone());
    }
    NameParts {
        prefix_tokens,
        given_tokens,
        surname_tokens,
        suffix_tokens,
        core_tokens,
    }
}

fn split_name_tokens(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(normalize_dictionary_entry)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
}

fn split_prefix_suffix(tokens: &[String]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut prefix_end = 0_usize;
    while prefix_end < tokens.len() && is_name_prefix_token(&tokens[prefix_end]) {
        prefix_end += 1;
    }
    let mut suffix_start = tokens.len();
    while suffix_start > prefix_end && is_name_suffix_token(&tokens[suffix_start - 1]) {
        suffix_start -= 1;
    }
    (
        tokens[..prefix_end].to_vec(),
        tokens[prefix_end..suffix_start].to_vec(),
        tokens[suffix_start..].to_vec(),
    )
}

fn split_given_surname(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    if tokens.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if tokens.len() == 1 {
        return (vec![tokens[0].clone()], Vec::new());
    }
    let mut surname_start = tokens.len() - 1;
    while surname_start > 0 && is_surname_particle_token(&tokens[surname_start - 1]) {
        surname_start -= 1;
    }
    if surname_start == 0 {
        return (Vec::new(), tokens.to_vec());
    }
    (
        tokens[..surname_start].to_vec(),
        tokens[surname_start..].to_vec(),
    )
}

fn join_name_tokens(tokens: &[String]) -> String {
    tokens.join(" ")
}

fn name_token_lookup_key(value: &str) -> String {
    value
        .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '\'' && ch != '-')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn is_name_prefix_token(value: &str) -> bool {
    let key = name_token_lookup_key(value);
    !key.is_empty() && NAME_PREFIX_TOKENS.contains(&key.as_str())
}

fn is_name_suffix_token(value: &str) -> bool {
    let key = name_token_lookup_key(value);
    !key.is_empty() && NAME_SUFFIX_TOKENS.contains(&key.as_str())
}

fn is_surname_particle_token(value: &str) -> bool {
    let key = name_token_lookup_key(value);
    !key.is_empty() && NAME_SURNAME_PARTICLE_TOKENS.contains(&key.as_str())
}

fn push_unique_variant(seen: &mut BTreeSet<String>, out: &mut Vec<String>, value: String) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let normalized = normalize_dictionary_entry(trimmed);
    if normalized.is_empty() {
        return;
    }
    if seen.insert(normalized.clone()) {
        out.push(normalized);
    }
}

fn push_template_variant(
    seen: &mut BTreeSet<String>,
    out: &mut Vec<String>,
    canonical: &str,
    preserve_raw_input_shape: bool,
    value: String,
) {
    let normalized = normalize_dictionary_entry(&value);
    if normalized.is_empty() {
        return;
    }
    let canonical = normalize_dictionary_entry(canonical);
    if !preserve_raw_input_shape && normalized == canonical {
        return;
    }
    if canonical.contains(',') && normalized.contains(',') {
        return;
    }
    push_unique_variant(seen, out, normalized);
}

fn title_case_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut new_word = true;
    for ch in value.chars() {
        if ch.is_alphabetic() {
            if new_word {
                out.extend(ch.to_uppercase());
                new_word = false;
            } else {
                out.extend(ch.to_lowercase());
            }
        } else {
            new_word = ch == ' ' || ch == '-' || ch == '\'';
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::build_dictionary_variants;

    #[test]
    fn build_dictionary_variants_adds_common_name_forms() {
        let dictionary = vec!["Sarah Kellen".to_owned()];
        let variants = build_dictionary_variants(&dictionary);
        assert!(variants.iter().any(|value| value == "Sarah Kellen"));
        assert!(variants.iter().any(|value| value == "Kellen, Sarah"));
        assert!(variants.iter().any(|value| value == "Sarah"));
        assert!(variants.iter().any(|value| value == "Kellen"));
    }

    #[test]
    fn build_dictionary_variants_preserves_special_name_parts() {
        let dictionary = vec!["Dr. Jean Luc Brunel Jr.".to_owned()];
        let variants = build_dictionary_variants(&dictionary);
        assert!(!variants.is_empty());
        assert!(variants
            .iter()
            .any(|value| value.contains("Jean Luc Brunel")));
    }

    #[test]
    fn build_dictionary_variants_adds_role_aware_alias_combinations() {
        let dictionary = vec![
            "Leslie Wexner".to_owned(),
            "David Rogers".to_owned(),
            "Jean-Luc Brunel".to_owned(),
            "Adriana Ross Mucińska".to_owned(),
        ];
        let variants = build_dictionary_variants(&dictionary);

        assert!(variants.iter().any(|value| value == "Les Wexner"));
        assert!(variants.iter().any(|value| value == "Lesley Wexner"));
        assert!(variants.iter().any(|value| value == "David Rodgers"));
        assert!(variants.iter().any(|value| value == "Jean Luc Brunel"));
        assert!(variants
            .iter()
            .any(|value| value == "Adriana Ross Mucinska"));
    }

    #[test]
    fn build_dictionary_variants_does_not_mix_first_with_first_or_last_with_last() {
        let dictionary = vec!["Leslie Wexner".to_owned()];
        let variants = build_dictionary_variants(&dictionary);

        assert!(!variants.iter().any(|value| value == "Lesley Leslie"));
        assert!(!variants.iter().any(|value| value == "Wexner Wexner"));
    }

    #[test]
    fn build_dictionary_variants_does_not_cross_combine_between_entries() {
        let dictionary = vec!["Leslie Wexner".to_owned(), "Sarah Kellen".to_owned()];
        let variants = build_dictionary_variants(&dictionary);

        assert!(!variants.iter().any(|value| value == "Les Kellen"));
        assert!(!variants.iter().any(|value| value == "Sarah Wexner"));
    }

    #[test]
    fn build_dictionary_variants_drops_raw_alternate_input_shapes() {
        let dictionary = vec![
            "KELLEN, SARAH".to_owned(),
            "GROFF, LESLEY".to_owned(),
            "MR. LES WEXNER".to_owned(),
            "DR. RICHARD BARNETT".to_owned(),
            "OMEGA BRUNEL TOKEN".to_owned(),
            "RODGERS, DAVID".to_owned(),
        ];
        let variants = build_dictionary_variants(&dictionary);

        assert!(!variants.iter().any(|value| value == "KELLEN, SARAH"));
        assert!(!variants.iter().any(|value| value == "GROFF, LESLEY"));
        assert!(!variants.iter().any(|value| value == "MR. LES WEXNER"));
        assert!(!variants.iter().any(|value| value == "DR. RICHARD BARNETT"));
        assert!(!variants.iter().any(|value| value == "OMEGA BRUNEL TOKEN"));
        assert!(!variants.iter().any(|value| value == "RODGERS, DAVID"));
        assert!(variants.iter().any(|value| value == "SARAH KELLEN"));
        assert!(variants.iter().any(|value| value == "LESLEY GROFF"));
        assert!(variants.iter().any(|value| value == "MR. WEXNER"));
        assert!(variants.iter().any(|value| value == "DR. BARNETT"));
        assert!(variants.iter().any(|value| value == "DAVID ROGERS"));
    }
}
