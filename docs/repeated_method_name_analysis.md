# Repeated Method Name Analysis

Scope: `src/` Rust files only (no `tests/`, no web JS).  
Method: grouped all function names and analyzed names with count > 1.

## Summary
- Repeated function names found: `42`
- Total repeated occurrences: `147`
- Most repeats are intentional trait contracts (`read`, `render_page_to_rgba`, `page_indices`, etc.) or idiomatic constructors/defaults (`new`, `default`, `main`).
- Main simplification opportunity is duplicated low-level PDF helper logic (`object_to_f32`, `object_to_name_string`, `decode_pdf_text`, width table helpers, shaping helpers).

## Per-Name Assessment
| Name | Count | Why It Repeats | Needed? | Recommendation |
|---|---:|---|---|---|
| `advance_pt` | 2 | Same glyph-advance helper implemented in two logic/data files | No | Extract shared text-metrics helper module |
| `annotation_redactions` | 5 | Trait contract in dependency + data wrapper + fake test retriever | Yes | Keep; optionally unify trait declaration location |
| `as_str` | 2 | Two different enums expose string label (`AnchorMode`, `WidthSource`) | Yes | Keep |
| `create_dir_all` | 2 | Dependency file store + data-layer wrapper method | Partial | Keep boundary now; later consider thin trait-based pass-through removal |
| `decode_pdf_text` | 2 | Same PDF byte-string decoding logic in two dependency modules | No | Centralize in shared PDF text util |
| `default` | 18 | `Default` trait impl across many structs/configs | Yes | Keep (idiomatic Rust) |
| `default_output_dir` | 2 | Separate CLI and bin utility naming overlap | Partial | Optional dedupe in shared CLI util module |
| `drawn_redactions` | 5 | Trait contract + implementations/fakes | Yes | Keep |
| `exists` | 2 | Dependency + data wrapper | Partial | Same as `create_dir_all` |
| `extract_text_runs` | 2 | Similar text-run extraction in font and redaction dependency modules | Partial | Evaluate shared parser; keep split if output semantics differ |
| `helvetica_width` | 2 | Embedded core-font width table in two modules | No | Move to one core-font metrics module |
| `inherited_page_rect` | 2 | Same page-box traversal helper duplicated | No | Centralize PDF page geometry helper |
| `is_dir` | 2 | Dependency + data wrapper | Partial | Same as `create_dir_all` |
| `is_subset_prefix` | 2 | Duplicate subset-font normalization helper | No | Centralize font-name normalization helper |
| `load_dictionary_from_bytes` | 2 | Public method + internal helper in same file | Partial | Keep pattern, but rename helper for clarity (`parse_dictionary_bytes`) |
| `load_reports` | 3 | Trait method + concrete impl + trait-impl method | Yes | Keep contract shape |
| `main` | 4 | Separate binaries each need entrypoint | Yes | Keep |
| `new` | 14 | Constructors across many types/components | Yes | Keep (idiomatic Rust) |
| `new_from_bytes` | 3 | Multiple components expose byte constructors | Partial | Keep for API clarity; align naming conventions |
| `normalize_from_parts` | 2 | Duplicate font normalization helper | No | Centralize normalization code |
| `normalize_subset_font_name` | 2 | Duplicate subset-font cleanup logic | No | Centralize normalization code |
| `object_to_dict` | 3 | Duplicate lopdf object conversion helper | No | Centralize conversion helpers |
| `object_to_f32` | 5 | Duplicate lopdf numeric conversion helper | No | Centralize conversion helpers |
| `object_to_name_string` | 4 | Duplicate lopdf name conversion helper | No | Centralize conversion helpers |
| `object_to_rect` | 2 | Duplicate rectangle extraction helper | No | Centralize conversion helpers |
| `object_to_rect_resolved` | 2 | Duplicate rect + page-resource resolution helper | No | Centralize conversion helpers |
| `page_count` | 4 | Renderer trait + concrete/fake implementations | Yes | Keep |
| `page_indices` | 5 | Redaction retriever trait + concrete/fake implementations | Yes | Keep |
| `page_render_box_from_page` | 2 | Duplicate page render-box helper | No | Centralize geometry helper |
| `rank_in_guess` | 2 | Same benchmark scoring helper duplicated in two bins | No | Move to benchmark shared module |
| `raster_redactions` | 5 | Trait contract + concrete/fake implementations | Yes | Keep |
| `read` | 4 | File accessor trait + concrete/fake implementations | Yes | Keep |
| `render_page_to_rgba` | 4 | Renderer trait + concrete/fake implementations | Yes | Keep |
| `resolve_pdf_font_name` | 2 | Duplicate font name resolver in two dependency modules | No | Centralize font resolver helper |
| `run` | 5 | Conventional internal entry functions in binaries/services | Yes | Keep (optional rename only for style) |
| `sample_pdf_bytes` | 2 | Test helper duplicated in two test modules | No | Optional dedupe into common test util |
| `shaping_features` | 3 | Same shaping feature list in multiple modules | No | Centralize shaping feature config |
| `summarize_ranks` | 2 | Same rank summary helper duplicated in two bins | No | Move to benchmark shared module |
| `times_roman_width` | 2 | Embedded core-font width table duplicated | No | Move to one core-font metrics module |
| `underlying_text_hits` | 5 | Trait contract + concrete/fake implementations | Yes | Keep |
| `vertical_overlap_run` | 2 | Similar vertical-overlap helper duplicated | No | Centralize geometry helper |
| `width_from_table` | 2 | Same width lookup helper duplicated | No | Centralize metrics helper |

## What This Means
- The repeats that are **needed** are mostly interface contracts and idiomatic constructors.
- The repeats that are **not needed** cluster around:
  - PDF object conversion helpers
  - Font normalization/lookup helpers
  - Core-font width/shaping helpers
  - Benchmark utility helpers

## Suggested Simplification Order
1. Create `dependency/pdf_object_utils.rs` and move conversion helpers (`object_to_*`, `decode_pdf_text`, page rect helpers).
2. Create `dependency/font_name_utils.rs` for subset/font-name normalization + resolver.
3. Create `logic/text_metrics_utils.rs` for shared shaping/core-font width helpers (`times_roman_width`, `helvetica_width`, `shaping_features`, `width_from_table`, `advance_pt`).
4. Create `src/bin/benchmark_common.rs` for `rank_in_guess`/`summarize_ranks`.
5. Re-evaluate data wrappers (`exists`, `is_dir`, `create_dir_all`) after utility extraction; keep if layering boundary is still valuable.
