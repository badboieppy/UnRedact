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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryVariantRecord {
    pub text: String,
    pub raw_entry_index: usize,
    pub raw_entry_text: String,
    pub raw_entry_normalized: String,
    pub template_id: String,
    pub template_family: String,
    pub variant_family: String,
    pub alias_source: Option<String>,
    pub orthographic_source: Option<String>,
    pub case_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryVariantBuildSummary {
    pub input_entry_count: usize,
    pub kept_variant_count: usize,
    pub skipped_comma_family_count: usize,
    pub skipped_generated_single_from_multi_raw_count: usize,
    pub skipped_comma_family_examples: Vec<String>,
    pub skipped_generated_single_from_multi_raw_examples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryVariantBuildResult {
    pub records: Vec<DictionaryVariantRecord>,
    pub summary: DictionaryVariantBuildSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TemplateVariantRecord {
    text: String,
    template_id: String,
    template_family: String,
    alias_source: Option<String>,
    orthographic_source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TemplateContext<'a> {
    canonical: &'a str,
    preserve_raw_input_shape: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FinalVariantContext<'a> {
    raw_entry_index: usize,
    raw_entry_text: &'a str,
    canonical: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DictionaryVariantPolicy {
    allow_comma_family: bool,
    allow_generated_single_from_multi_raw: bool,
}

impl DictionaryVariantPolicy {
    const RUNTIME_DEFAULT: Self = Self {
        allow_comma_family: false,
        allow_generated_single_from_multi_raw: false,
    };

    #[cfg(test)]
    const FULL_RESEARCH: Self = Self {
        allow_comma_family: true,
        allow_generated_single_from_multi_raw: true,
    };
}

#[cfg(test)]
#[inline]
pub fn build_dictionary_variants(dictionary: &[String]) -> Vec<String> {
    build_dictionary_variant_build_result(dictionary)
        .records
        .into_iter()
        .map(|record| record.text)
        .collect::<Vec<_>>()
}

#[inline]
pub fn build_dictionary_variant_build_result(
    dictionary: &[String],
) -> DictionaryVariantBuildResult {
    build_dictionary_variant_build_result_with_policy(
        dictionary,
        DictionaryVariantPolicy::RUNTIME_DEFAULT,
    )
}

#[cfg(test)]
#[inline]
fn build_dictionary_variant_records_full_research(
    dictionary: &[String],
) -> Vec<DictionaryVariantRecord> {
    build_dictionary_variant_build_result_with_policy(
        dictionary,
        DictionaryVariantPolicy::FULL_RESEARCH,
    )
    .records
}

fn build_dictionary_variant_build_result_with_policy(
    dictionary: &[String],
    policy: DictionaryVariantPolicy,
) -> DictionaryVariantBuildResult {
    let mut out = Vec::<DictionaryVariantRecord>::new();
    let mut seen = BTreeSet::<String>::new();
    let mut summary = DictionaryVariantBuildSummary {
        input_entry_count: dictionary.len(),
        kept_variant_count: 0,
        skipped_comma_family_count: 0,
        skipped_generated_single_from_multi_raw_count: 0,
        skipped_comma_family_examples: Vec::new(),
        skipped_generated_single_from_multi_raw_examples: Vec::new(),
    };
    for (raw_entry_index, entry) in dictionary.iter().enumerate() {
        let canonical = normalize_dictionary_entry(entry);
        if canonical.is_empty() {
            continue;
        }
        for variant in
            build_name_variant_records(raw_entry_index, entry, &canonical, policy, &mut summary)
        {
            let trimmed = variant.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_owned()) {
                out.push(DictionaryVariantRecord {
                    text: trimmed.to_owned(),
                    ..variant
                });
            }
        }
    }
    summary.kept_variant_count = out.len();
    DictionaryVariantBuildResult {
        records: out,
        summary,
    }
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

fn build_name_variant_records(
    raw_entry_index: usize,
    raw_entry_text: &str,
    canonical: &str,
    policy: DictionaryVariantPolicy,
    summary: &mut DictionaryVariantBuildSummary,
) -> Vec<DictionaryVariantRecord> {
    let mut template_seen = BTreeSet::<String>::new();
    let mut templates = Vec::<TemplateVariantRecord>::new();
    let tokens = canonical
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let preserve_raw_input_shape = should_preserve_raw_input_shape(canonical, &tokens);
    let template_context = TemplateContext {
        canonical,
        preserve_raw_input_shape,
    };
    push_template_variant(
        &mut template_seen,
        &mut templates,
        &template_context,
        "canonical",
        None,
        None,
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
                &template_context,
                "core_tokens",
                None,
                None,
                core.clone(),
            );
        }
        if !given_first.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "given_first_surname",
                None,
                None,
                format!("{given_first} {surname}"),
            );
        }
        if !surname.is_empty() && !given_first.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "surname_comma_given_first",
                None,
                None,
                format!("{surname}, {given_first}"),
            );
        }
        if !prefix.is_empty() && !given_first.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "prefix_given_first_surname",
                None,
                None,
                format!("{prefix} {given_first} {surname}"),
            );
        }
        if !suffix.is_empty() && !given_first.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "given_first_surname_suffix",
                None,
                None,
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
                &template_context,
                "prefix_given_first_surname_suffix",
                None,
                None,
                format!("{prefix} {given_first} {surname} {suffix}"),
            );
        }
        if !given.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "given_surname",
                None,
                None,
                format!("{given} {surname}"),
            );
        }
        if !given.is_empty() && !surname.is_empty() && !suffix.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "given_surname_suffix",
                None,
                None,
                format!("{given} {surname} {suffix}"),
            );
        }
        if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "prefix_given_surname",
                None,
                None,
                format!("{prefix} {given} {surname}"),
            );
        }
        if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() && !suffix.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "prefix_given_surname_suffix",
                None,
                None,
                format!("{prefix} {given} {surname} {suffix}"),
            );
        }
        if !given_first.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "given_first_only",
                None,
                None,
                given_first.clone(),
            );
        }
        if !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "surname_only",
                None,
                None,
                surname.clone(),
            );
        }
        if !surname_last.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "surname_last_only",
                None,
                None,
                surname_last,
            );
        }
        if !prefix.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "prefix_surname",
                None,
                None,
                format!("{prefix} {surname}"),
            );
        }
        if !suffix.is_empty() && !surname.is_empty() {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "surname_suffix",
                None,
                None,
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
                        &template_context,
                        "last_comma_first_from_core",
                        None,
                        None,
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
                    &template_context,
                    "given_first_middle_initials_surname",
                    None,
                    None,
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
            &template_context,
            "first_last",
            None,
            None,
            format!("{first} {last}"),
        );
        if !canonical.contains(',') {
            push_template_variant(
                &mut template_seen,
                &mut templates,
                &template_context,
                "last_comma_first",
                None,
                None,
                format!("{last}, {first}"),
            );
        }
        push_template_variant(
            &mut template_seen,
            &mut templates,
            &template_context,
            "first_only",
            None,
            None,
            first.to_owned(),
        );
        push_template_variant(
            &mut template_seen,
            &mut templates,
            &template_context,
            "last_only",
            None,
            None,
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

    finalize_name_variant_records(
        &FinalVariantContext {
            raw_entry_index,
            raw_entry_text,
            canonical,
        },
        &templates,
        policy,
        summary,
    )
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoleSideVariant {
    text: String,
    alias_source: Option<String>,
    orthographic_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoleTokenVariant {
    text: String,
    alias_source: Option<String>,
    orthographic_source: Option<String>,
}

fn add_role_aware_alias_templates(
    canonical: &str,
    preserve_raw_input_shape: bool,
    parts: &NameParts,
    template_seen: &mut BTreeSet<String>,
    templates: &mut Vec<TemplateVariantRecord>,
) {
    if parts.given_tokens.is_empty() || parts.surname_tokens.is_empty() {
        return;
    }
    let template_context = TemplateContext {
        canonical,
        preserve_raw_input_shape,
    };
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
                &template_context,
                "role_alias_pair",
                merge_sources(&given.alias_source, &surname.alias_source),
                merge_sources(&given.orthographic_source, &surname.orthographic_source),
                format!("{} {}", given.text, surname.text),
            );
            combo_count += 1;
            if combo_count >= MAX_ROLE_COMBINATIONS_PER_ENTRY {
                return;
            }
            push_template_variant(
                template_seen,
                templates,
                &template_context,
                "role_alias_comma_pair",
                merge_sources(&given.alias_source, &surname.alias_source),
                merge_sources(&given.orthographic_source, &surname.orthographic_source),
                format!("{}, {}", surname.text, given.text),
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

fn expand_role_side_variants(tokens: &[String], role: NameTokenRole) -> Vec<RoleSideVariant> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut per_token_variants = Vec::<Vec<RoleTokenVariant>>::with_capacity(tokens.len());
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
            expanded.push(RoleTokenVariant {
                text: token.clone(),
                alias_source: None,
                orthographic_source: None,
            });
        }
        per_token_variants.push(expanded);
    }

    let mut states = vec![Vec::<RoleTokenVariant>::new()];
    for variants in per_token_variants {
        let mut next = Vec::<Vec<RoleTokenVariant>>::new();
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
    let mut out = Vec::<RoleSideVariant>::new();
    for state in states {
        let phrase = normalize_dictionary_entry(
            &state
                .iter()
                .map(|variant| variant.text.clone())
                .collect::<Vec<_>>()
                .join(" "),
        );
        if phrase.is_empty() {
            continue;
        }
        if seen.insert(phrase.clone()) {
            let alias_sources = state
                .iter()
                .filter_map(|variant| variant.alias_source.clone())
                .collect::<BTreeSet<_>>();
            let orthographic_sources = state
                .iter()
                .filter_map(|variant| variant.orthographic_source.clone())
                .collect::<BTreeSet<_>>();
            out.push(RoleSideVariant {
                text: phrase,
                alias_source: (!alias_sources.is_empty())
                    .then(|| alias_sources.into_iter().collect::<Vec<_>>().join(",")),
                orthographic_source: (!orthographic_sources.is_empty()).then(|| {
                    orthographic_sources
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(",")
                }),
            });
        }
        if out.len() >= MAX_ROLE_SIDE_VARIANTS {
            break;
        }
    }
    if out.is_empty() {
        let fallback = join_name_tokens(tokens);
        if !fallback.is_empty() {
            out.push(RoleSideVariant {
                text: fallback,
                alias_source: None,
                orthographic_source: None,
            });
        }
    }
    out
}

fn expand_role_token_variants(
    token: &str,
    role: NameTokenRole,
    allow_aliases: bool,
) -> Vec<RoleTokenVariant> {
    let mut seen = BTreeSet::<String>::new();
    let mut out = Vec::<RoleTokenVariant>::new();
    push_unique_role_token_variant(&mut seen, &mut out, token.to_owned(), None, None);
    add_orthographic_token_variants(token, &mut seen, &mut out);

    if allow_aliases {
        for alias in aliases_for_role_token(role, token) {
            let alias_source = Some(alias_lookup_key(token));
            push_unique_role_token_variant(
                &mut seen,
                &mut out,
                (*alias).to_owned(),
                alias_source.clone(),
                None,
            );
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
    out: &mut Vec<RoleTokenVariant>,
) {
    let normalized = normalize_dictionary_entry(token);
    if normalized.is_empty() {
        return;
    }
    let folded = fold_latin_text(&normalized);
    if folded != normalized {
        push_unique_role_token_variant(
            seen,
            out,
            folded.clone(),
            None,
            Some("latin_fold".to_owned()),
        );
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
            let source = Some("token_shape".to_owned());
            push_unique_role_token_variant(seen, out, candidate, None, source);
        }
    }
}

fn push_unique_role_token_variant(
    seen: &mut BTreeSet<String>,
    out: &mut Vec<RoleTokenVariant>,
    value: String,
    alias_source: Option<String>,
    orthographic_source: Option<String>,
) {
    let normalized = normalize_dictionary_entry(&value);
    if normalized.is_empty() {
        return;
    }
    if seen.insert(normalized.clone()) {
        out.push(RoleTokenVariant {
            text: normalized,
            alias_source,
            orthographic_source,
        });
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

fn finalize_name_variant_records(
    context: &FinalVariantContext<'_>,
    templates: &[TemplateVariantRecord],
    policy: DictionaryVariantPolicy,
    summary: &mut DictionaryVariantBuildSummary,
) -> Vec<DictionaryVariantRecord> {
    let mut seen = BTreeSet::<String>::new();
    let mut out = Vec::<DictionaryVariantRecord>::new();
    for template in templates {
        for (value, case_source) in [
            (template.text.clone(), Some("raw".to_owned())),
            (template.text.to_uppercase(), Some("uppercase".to_owned())),
            (template.text.to_lowercase(), Some("lowercase".to_owned())),
            (
                title_case_text(&template.text),
                Some("titlecase".to_owned()),
            ),
        ] {
            let normalized = normalize_dictionary_entry(&value);
            if normalized.is_empty()
                || should_skip_final_variant(context, &normalized, policy, summary)
            {
                continue;
            }
            push_unique_final_variant(
                &mut seen,
                &mut out,
                context,
                template,
                normalized,
                case_source,
            );
        }
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

fn push_template_variant(
    seen: &mut BTreeSet<String>,
    out: &mut Vec<TemplateVariantRecord>,
    context: &TemplateContext<'_>,
    template_family: &str,
    alias_source: Option<String>,
    orthographic_source: Option<String>,
    value: String,
) {
    let normalized = normalize_dictionary_entry(&value);
    if normalized.is_empty() {
        return;
    }
    let canonical = normalize_dictionary_entry(context.canonical);
    if !context.preserve_raw_input_shape && normalized == canonical {
        return;
    }
    if canonical.contains(',') && normalized.contains(',') {
        return;
    }
    if seen.insert(normalized.clone()) {
        out.push(TemplateVariantRecord {
            text: normalized,
            template_id: template_family.to_owned(),
            template_family: template_family.to_owned(),
            alias_source,
            orthographic_source,
        });
    }
}

fn push_unique_final_variant(
    seen: &mut BTreeSet<String>,
    out: &mut Vec<DictionaryVariantRecord>,
    context: &FinalVariantContext<'_>,
    template: &TemplateVariantRecord,
    value: String,
    case_source: Option<String>,
) {
    let normalized = normalize_dictionary_entry(&value);
    if normalized.is_empty() {
        return;
    }
    let variant_family = classify_name_family(&normalized);
    if seen.insert(normalized.clone()) {
        out.push(DictionaryVariantRecord {
            text: normalized.clone(),
            raw_entry_index: context.raw_entry_index,
            raw_entry_text: context.raw_entry_text.to_owned(),
            raw_entry_normalized: normalize_dictionary_entry(context.canonical),
            template_id: template.template_id.clone(),
            template_family: template.template_family.clone(),
            variant_family,
            alias_source: template.alias_source.clone(),
            orthographic_source: template.orthographic_source.clone(),
            case_source,
        });
    }
}

fn should_skip_final_variant(
    context: &FinalVariantContext<'_>,
    normalized: &str,
    policy: DictionaryVariantPolicy,
    summary: &mut DictionaryVariantBuildSummary,
) -> bool {
    let variant_family = classify_name_family(normalized);
    if !policy.allow_comma_family && variant_family == "comma" {
        summary.skipped_comma_family_count += 1;
        push_policy_example(&mut summary.skipped_comma_family_examples, normalized);
        return true;
    }
    if !policy.allow_generated_single_from_multi_raw
        && variant_family == "single_token"
        && raw_entry_token_count(context.canonical) >= 2
    {
        summary.skipped_generated_single_from_multi_raw_count += 1;
        push_policy_example(
            &mut summary.skipped_generated_single_from_multi_raw_examples,
            normalized,
        );
        return true;
    }
    false
}

fn raw_entry_token_count(value: &str) -> usize {
    value
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .count()
}

fn push_policy_example(examples: &mut Vec<String>, value: &str) {
    if examples.len() >= 8 {
        return;
    }
    if !examples.iter().any(|existing| existing == value) {
        examples.push(value.to_owned());
    }
}

fn merge_sources(left: &Option<String>, right: &Option<String>) -> Option<String> {
    let mut out = BTreeSet::<String>::new();
    if let Some(left) = left {
        if !left.is_empty() {
            out.insert(left.clone());
        }
    }
    if let Some(right) = right {
        if !right.is_empty() {
            out.insert(right.clone());
        }
    }
    (!out.is_empty()).then(|| out.into_iter().collect::<Vec<_>>().join(","))
}

fn classify_name_family(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "empty".to_owned();
    }
    if trimmed.contains(',') {
        return "comma".to_owned();
    }
    let tokens = trimmed
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return "empty".to_owned();
    }
    let alpha_count = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();
    let punct_count = trimmed
        .chars()
        .filter(|ch| !ch.is_ascii_alphanumeric() && !ch.is_whitespace())
        .count();
    if punct_count > alpha_count {
        return "punctuation_heavy".to_owned();
    }
    if tokens.len() == 1 {
        let token = tokens[0];
        let letters = token.chars().filter(|ch| ch.is_ascii_alphabetic()).count();
        if letters <= 2 || token.ends_with('.') {
            return "initial".to_owned();
        }
        return "single_token".to_owned();
    }
    if tokens.iter().any(|token| {
        let letters = token.chars().filter(|ch| ch.is_ascii_alphabetic()).count();
        letters <= 1 || token.ends_with('.')
    }) {
        return "initial".to_owned();
    }
    "plain_multi_token".to_owned()
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
    use super::{
        build_dictionary_variant_build_result, build_dictionary_variant_records_full_research,
        build_dictionary_variants,
    };

    #[test]
    fn build_dictionary_variants_runtime_default_keeps_only_runtime_safe_forms() {
        let dictionary = vec!["Sarah Kellen".to_owned()];
        let variants = build_dictionary_variants(&dictionary);
        assert!(variants.iter().any(|value| value == "Sarah Kellen"));
        assert!(!variants.iter().any(|value| value == "Kellen, Sarah"));
        assert!(!variants.iter().any(|value| value == "Sarah"));
        assert!(!variants.iter().any(|value| value == "Kellen"));
    }

    #[test]
    fn build_dictionary_variants_full_research_preserves_comma_and_single_forms() {
        let dictionary = vec!["Sarah Kellen".to_owned()];
        let variants = build_dictionary_variant_records_full_research(&dictionary)
            .into_iter()
            .map(|record| record.text)
            .collect::<Vec<_>>();
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

    #[test]
    fn build_dictionary_variant_build_result_reports_runtime_policy_counts() {
        let dictionary = vec!["Sarah Kellen".to_owned()];
        let result = build_dictionary_variant_build_result(&dictionary);

        assert_eq!(result.summary.input_entry_count, 1);
        assert!(result.summary.kept_variant_count >= 1);
        assert!(result.summary.skipped_comma_family_count >= 1);
        assert!(result.summary.skipped_generated_single_from_multi_raw_count >= 2);
        assert!(result
            .summary
            .skipped_comma_family_examples
            .iter()
            .any(|value| value == "Kellen, Sarah"));
        assert!(result
            .summary
            .skipped_generated_single_from_multi_raw_examples
            .iter()
            .any(|value| value == "Sarah"));
    }
}
