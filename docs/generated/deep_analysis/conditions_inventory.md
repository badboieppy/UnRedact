# Conditions Inventory (Exhaustive Lexical Scan)

Total captured condition lines: 913

## Summary by Language/Kind

| Language | Kind | Count |
|---|---|---:|
| js | if | 105 |
| js | switch | 1 |
| js | ternary | 23 |
| js | while | 2 |
| rust | if | 626 |
| rust | inline_if | 55 |
| rust | match | 75 |
| rust | match_guard | 11 |
| rust | ternary | 1 |
| rust | while | 14 |

## Top Files by Condition Count

| File | Count |
|---|---:|
| src/logic/redaction_guessing_component.rs | 356 |
| src/dependency/pdf_redaction_accessor.rs | 116 |
| web/app.js | 82 |
| src/dependency/pdf_font_run_accessor.rs | 59 |
| src/bin/guess_accuracy_benchmark.rs | 57 |
| src/bin/visual_score_impact_benchmark.rs | 44 |
| src/data/visualization_data.rs | 44 |
| web/pkg/unredact.js | 42 |
| src/dependency/pdf_font_occurrence_accessor.rs | 28 |
| src/bin/pdf_to_png.rs | 11 |
| src/data/dictionary_data.rs | 10 |
| tests/efta00038617_guessing.rs | 10 |
| tests/shared_workflow_purity.rs | 9 |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 7 |
| src/logic/local_file_workflow_component.rs | 7 |
| src/dependency/file_store.rs | 5 |
| src/dependency/pdf_annotator.rs | 4 |
| tests/integration_black_box_boundary.rs | 4 |
| tests/dictionary_entry_format_behavior.rs | 3 |
| src/dependency/hayro_renderer.rs | 3 |
| src/service/unredact_cli_entry.rs | 3 |
| tests/web_ui_batch_benchmark.rs | 2 |
| src/types/redaction_types.rs | 2 |
| src/main.rs | 2 |
| tests/efta00101126_guessing.rs | 1 |
| src/data/result_data_publisher.rs | 1 |
| src/service/unredact_web_entry.rs | 1 |

## Full Condition List

| File | Line | Kind | Snippet |
|---|---:|---|---|
| src/bin/guess_accuracy_benchmark.rs | 299 | while | while let Some(arg) = args.next() { |
| src/bin/guess_accuracy_benchmark.rs | 300 | match | match arg.as_str() { |
| src/bin/guess_accuracy_benchmark.rs | 314 | if | if repeats == 0 { |
| src/bin/guess_accuracy_benchmark.rs | 380 | if | if !normalized.is_empty() && seen.insert(normalized.clone()) { |
| src/bin/guess_accuracy_benchmark.rs | 386 | if | if !normalized.is_empty() && seen.insert(normalized.clone()) { |
| src/bin/guess_accuracy_benchmark.rs | 406 | if | if target_upper.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 428 | if | if evaluated_items == 0 { |
| src/bin/guess_accuracy_benchmark.rs | 438 | inline_if | let mrr = if evaluated_items == 0 { |
| src/bin/guess_accuracy_benchmark.rs | 447 | inline_if | let mean_rank_found = if found.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 469 | if | if values.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 478 | if | if values.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 485 | if | if values.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 506 | if | if has_top_guess(guess) { |
| src/bin/guess_accuracy_benchmark.rs | 509 | if | if guess.visual_dropped { |
| src/bin/guess_accuracy_benchmark.rs | 512 | if | if let Some(value) = guess.visual_mean_abs_diff { |
| src/bin/guess_accuracy_benchmark.rs | 515 | if | if let Some(value) = guess.visual_changed_pixel_ratio { |
| src/bin/guess_accuracy_benchmark.rs | 518 | if | if let Some(value) = guess.visual_compared_pixels { |
| src/bin/guess_accuracy_benchmark.rs | 567 | if | if let Some(value) = token.strip_prefix("rows_considered=") { |
| src/bin/guess_accuracy_benchmark.rs | 581 | if | if let Some(gain) = mean_gain { |
| src/bin/guess_accuracy_benchmark.rs | 620 | if | if !guess.context.has_anchor_pair { |
| src/bin/guess_accuracy_benchmark.rs | 624 | if | if width <= 0.0_f64 { |
| src/bin/guess_accuracy_benchmark.rs | 637 | if | if count > 0.0_f64 { |
| src/bin/guess_accuracy_benchmark.rs | 641 | if | if is_multi_span_guess(guess) { |
| src/bin/guess_accuracy_benchmark.rs | 693 | if | if guess.context.has_anchor_pair { |
| src/bin/guess_accuracy_benchmark.rs | 696 | match | match guess.context.anchor_mode.as_deref() { |
| src/bin/guess_accuracy_benchmark.rs | 706 | match | match width_source { |
| src/bin/guess_accuracy_benchmark.rs | 713 | if | if guess.context.width_fallback_reason.is_some() { |
| src/bin/guess_accuracy_benchmark.rs | 739 | if | if !line.starts_with("timing_ms stage=") { |
| src/bin/guess_accuracy_benchmark.rs | 745 | if | if let Some(rest) = token.strip_prefix("stage=") { |
| src/bin/guess_accuracy_benchmark.rs | 754 | match | match stage.as_str() { |
| src/bin/guess_accuracy_benchmark.rs | 802 | if | if trimmed.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 805 | if | if target_set.contains(&trimmed.to_ascii_uppercase()) { |
| src/bin/guess_accuracy_benchmark.rs | 809 | if | if lines.len() >= 1_200 { |
| src/bin/guess_accuracy_benchmark.rs | 850 | if | if left_set.is_empty() && right_set.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 855 | if | if union <= 0.0_f64 { |
| src/bin/guess_accuracy_benchmark.rs | 863 | if | if row_sets.is_empty() \\|\\| row_sets[0].is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 892 | if | if same_top1 { |
| src/bin/guess_accuracy_benchmark.rs | 922 | if | if rank_sets.is_empty() \\|\\| rank_sets[0].is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 935 | if | if let Some(value) = stddev(&series) { |
| src/bin/guess_accuracy_benchmark.rs | 944 | if | if !input.exists() { |
| src/bin/guess_accuracy_benchmark.rs | 954 | if | if report.guesses.len() >= 2 { |
| src/bin/guess_accuracy_benchmark.rs | 1032 | if | if !input.exists() { |
| src/bin/guess_accuracy_benchmark.rs | 1335 | if | if run_snapshots.is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 1492 | if | if benchmark_root.exists() { |
| src/bin/guess_accuracy_benchmark.rs | 1494 | if | if let Err(error) = remove_result { |
| src/bin/guess_accuracy_benchmark.rs | 1499 | if | if let Err(error) = std::fs::create_dir_all(&benchmark_root) { |
| src/bin/guess_accuracy_benchmark.rs | 1602 | if | if selected_payload.is_none() { |
| src/bin/guess_accuracy_benchmark.rs | 1616 | if | if options.require_deterministic && !payload.consistency.all_hashes_identical { |
| src/bin/guess_accuracy_benchmark.rs | 1846 | if | if let Some(parent) = options.out_path.parent() { |
| src/bin/guess_accuracy_benchmark.rs | 1847 | if | if !parent.as_os_str().is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 1849 | if | if let Err(error) = create_result { |
| src/bin/guess_accuracy_benchmark.rs | 1865 | if | if let Err(error) = std::fs::write(&options.out_path, encoded) { |
| src/bin/guess_accuracy_benchmark.rs | 1871 | if | if let Some(path) = options.consistency_out.as_deref() { |
| src/bin/guess_accuracy_benchmark.rs | 1872 | if | if let Some(parent) = path.parent() { |
| src/bin/guess_accuracy_benchmark.rs | 1873 | if | if !parent.as_os_str().is_empty() { |
| src/bin/guess_accuracy_benchmark.rs | 1875 | if | if let Err(error) = create_result { |
| src/bin/guess_accuracy_benchmark.rs | 1891 | if | if let Err(error) = std::fs::write(path, encoded) { |
| src/bin/pdf_to_png.rs | 36 | match | match run() { |
| src/bin/pdf_to_png.rs | 48 | if | if !args.dpi.is_finite() \\|\\| args.dpi <= 0.0 { |
| src/bin/pdf_to_png.rs | 51 | if | if args.page == 0 { |
| src/bin/pdf_to_png.rs | 54 | if | if args.all_pages && args.output.is_some() { |
| src/bin/pdf_to_png.rs | 60 | if | if page_count == 0 { |
| src/bin/pdf_to_png.rs | 64 | if | if args.all_pages { |
| src/bin/pdf_to_png.rs | 86 | if | if page_index >= page_count { |
| src/bin/pdf_to_png.rs | 93 | inline_if | let output_path = if let Some(path) = args.output { |
| src/bin/pdf_to_png.rs | 115 | if | if let Some(parent) = path.parent() { |
| src/bin/pdf_to_png.rs | 116 | if | if !parent.as_os_str().is_empty() { |
| src/bin/pdf_to_png.rs | 141 | if | if page == 1 { |
| src/bin/visual_score_impact_benchmark.rs | 147 | if | if values.len() <= 1 { |
| src/bin/visual_score_impact_benchmark.rs | 159 | if | if let Err(error) = run(options) { |
| src/bin/visual_score_impact_benchmark.rs | 166 | if | if !options.input.exists() { |
| src/bin/visual_score_impact_benchmark.rs | 169 | if | if options.trials == 0 { |
| src/bin/visual_score_impact_benchmark.rs | 186 | if | if candidate_pool.len() < targets_per_trial { |
| src/bin/visual_score_impact_benchmark.rs | 215 | if | if sampled.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 279 | if | if let Some(parent) = options.out.parent() { |
| src/bin/visual_score_impact_benchmark.rs | 280 | if | if !parent.as_os_str().is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 303 | if | if !hits.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 312 | if | if trimmed.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 317 | if | if ch.is_ascii_alphabetic() \\|\\| ch == '\'' \\|\\| ch == '-' { |
| src/bin/visual_score_impact_benchmark.rs | 321 | if | if out.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 325 | if | if !(MIN_TARGET_WORD_LEN..=MAX_TARGET_WORD_LEN).contains(&alpha_len) { |
| src/bin/visual_score_impact_benchmark.rs | 328 | if | if !out |
| src/bin/visual_score_impact_benchmark.rs | 341 | if | if full_text.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 348 | if | if canonical_word(&token).is_none() { |
| src/bin/visual_score_impact_benchmark.rs | 355 | if | if x1 <= x0 { |
| src/bin/visual_score_impact_benchmark.rs | 373 | if | if ch.is_ascii_alphabetic() \\|\\| ch == '\'' \\|\\| ch == '-' { |
| src/bin/visual_score_impact_benchmark.rs | 374 | if | if token.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 383 | if | if !token.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 409 | if | if let Some(text) = canonical_word(&hit.text) { |
| src/bin/visual_score_impact_benchmark.rs | 419 | if | if frequencies.get(&text).copied().unwrap_or(0_usize) != 1_usize { |
| src/bin/visual_score_impact_benchmark.rs | 424 | if | if canonical_word(&left.text).is_none() \\|\\| canonical_word(&right.text).is_none() { |
| src/bin/visual_score_impact_benchmark.rs | 427 | if | if (center_y(&left.bbox) - center_y(&hit.bbox)).abs() > SAME_LINE_DELTA_PT { |
| src/bin/visual_score_impact_benchmark.rs | 430 | if | if (center_y(&right.bbox) - center_y(&hit.bbox)).abs() > SAME_LINE_DELTA_PT { |
| src/bin/visual_score_impact_benchmark.rs | 433 | if | if left.bbox.x1 > hit.bbox.x0 \\|\\| right.bbox.x0 < hit.bbox.x1 { |
| src/bin/visual_score_impact_benchmark.rs | 438 | if | if width < 6.0_f32 \\|\\| height < 5.0_f32 { |
| src/bin/visual_score_impact_benchmark.rs | 472 | if | if let Some(word) = canonical_word(&hit.text) { |
| src/bin/visual_score_impact_benchmark.rs | 474 | if | if set.len() >= dictionary_size { |
| src/bin/visual_score_impact_benchmark.rs | 481 | while | while set.len() < dictionary_size { |
| src/bin/visual_score_impact_benchmark.rs | 503 | if | if !used_text.insert(candidate.text.clone()) { |
| src/bin/visual_score_impact_benchmark.rs | 506 | if | if chosen |
| src/bin/visual_score_impact_benchmark.rs | 513 | if | if chosen.len() >= desired { |
| src/bin/visual_score_impact_benchmark.rs | 582 | if | if guess.page_index != target.page_index { |
| src/bin/visual_score_impact_benchmark.rs | 585 | if | if overlap_ratio(guess.bbox, target.bbox) < TARGET_OVERLAP_RATIO_MIN { |
| src/bin/visual_score_impact_benchmark.rs | 588 | if | if let Some(rank) = rank_in_guess(guess, &target.text) { |
| src/bin/visual_score_impact_benchmark.rs | 601 | if | if exact.eq_ignore_ascii_case(target) { |
| src/bin/visual_score_impact_benchmark.rs | 607 | if | if candidate.text.eq_ignore_ascii_case(target) { |
| src/bin/visual_score_impact_benchmark.rs | 620 | if | if evaluated_items == 0 { |
| src/bin/visual_score_impact_benchmark.rs | 626 | inline_if | let mrr = if evaluated_items == 0 { |
| src/bin/visual_score_impact_benchmark.rs | 635 | inline_if | let mean_rank_found = if found.is_empty() { |
| src/bin/visual_score_impact_benchmark.rs | 659 | match | match (left, right) { |
| src/bin/visual_score_impact_benchmark.rs | 661 | if | if vis_rank < no_rank { |
| src/bin/visual_score_impact_benchmark.rs | 681 | inline_if | let mean_rank_delta_visual_minus_no_visual = if deltas.is_empty() { |
| src/data/dictionary_data.rs | 41 | inline_if | let dictionary = if let Some(bytes) = dictionary_bytes { |
| src/data/dictionary_data.rs | 45 | if | if let Some(parsed) = parse_dictionary_line(line) { |
| src/data/dictionary_data.rs | 67 | if | if normalized.is_empty() { |
| src/data/dictionary_data.rs | 78 | if | if trimmed.is_empty() { |
| src/data/dictionary_data.rs | 81 | inline_if | let parsed = if trimmed.contains('\\|') { |
| src/data/dictionary_data.rs | 87 | if | if tokens.is_empty() { |
| src/data/dictionary_data.rs | 95 | if | if normalized.is_empty() { |
| src/data/dictionary_data.rs | 106 | if | if let Some(parsed) = parse_dictionary_line(value) { |
| src/data/dictionary_data.rs | 117 | if | if ch.is_whitespace() { |
| src/data/dictionary_data.rs | 118 | if | if !in_space && !out.is_empty() { |
| src/data/result_data_publisher.rs | 48 | if | if let (Some(path), Some(bytes)) = ( |
| src/data/visualization_data.rs | 125 | if | if guess.context.has_anchor_pair |
| src/data/visualization_data.rs | 144 | if | if matches!(redaction.kind, RedactionKind::RasterDarkRegion) { |
| src/data/visualization_data.rs | 156 | inline_if | let (font_key, requested_font_size_pt, h_scale_pct) = if let Some(font) = anchor_font { |
| src/data/visualization_data.rs | 191 | if | if context_left.is_empty() \\|\\| context_right.is_empty() { |
| src/data/visualization_data.rs | 198 | match_guard | Some(text) if !text.is_empty() => text, |
| src/data/visualization_data.rs | 202 | match_guard | Some(text) if !text.is_empty() => text, |
| src/data/visualization_data.rs | 256 | inline_if | let guess_font = if left_font == right_font && (left_size - right_size).abs() <= 0.01 { |
| src/data/visualization_data.rs | 342 | if | if context_left.is_empty() \\|\\| context_right.is_empty() { |
| src/data/visualization_data.rs | 356 | if | if !font_size_pt.is_finite() \\|\\| font_size_pt <= 0.0_f32 { |
| src/data/visualization_data.rs | 390 | match_guard | Some(x) if x.is_finite() => { |
| src/data/visualization_data.rs | 476 | if | if let Some(first) = guess.exact_matches.first() { |
| src/data/visualization_data.rs | 490 | if | if run.page_index != page_index { |
| src/data/visualization_data.rs | 493 | if | if run.text.trim() != text { |
| src/data/visualization_data.rs | 496 | if | if let Some(b) = bbox { |
| src/data/visualization_data.rs | 497 | if | if vertical_overlap_run(&run.bbox, &b) <= 0.0 { |
| src/data/visualization_data.rs | 502 | match | match best { |
| src/data/visualization_data.rs | 504 | match_guard | Some((_, best_score)) if dist < best_score => best = Some((run, dist)), |
| src/data/visualization_data.rs | 519 | if | if run.page_index != page_index { |
| src/data/visualization_data.rs | 523 | if | if overlap <= 0.0 { |
| src/data/visualization_data.rs | 527 | match | match best { |
| src/data/visualization_data.rs | 530 | if | if overlap > best_overlap \\|\\| (overlap == best_overlap && dist < best_dist) { |
| src/data/visualization_data.rs | 565 | if | if let Some(asset) = assets.get(font_key) { |
| src/data/visualization_data.rs | 566 | if | if let Some(width) = advance_pt(asset, text, font_size_pt) { |
| src/data/visualization_data.rs | 567 | if | if width.is_finite() && width > 0.0 { |
| src/data/visualization_data.rs | 576 | if | if let Some(table) = width_map.get(&key) { |
| src/data/visualization_data.rs | 577 | if | if let Some(width) = width_from_table(table, text, font_size_pt) { |
| src/data/visualization_data.rs | 578 | if | if width.is_finite() && width > 0.0 { |
| src/data/visualization_data.rs | 623 | if | if code > u16::MAX as u32 { |
| src/data/visualization_data.rs | 627 | if | if code < table.first_char { |
| src/data/visualization_data.rs | 631 | if | if idx >= table.widths.len() { |
| src/data/visualization_data.rs | 671 | match | match deref_to_dict(&doc, value_object).or_else(\\|\\| object_to_dict(value_object)) { |
| src/data/visualization_data.rs | 683 | match_guard | (Some(f), Some(w)) if !w.is_empty() => (f, w), |
| src/data/visualization_data.rs | 701 | if | if dict.has(b"Widths") { |
| src/data/visualization_data.rs | 705 | if | if subtype.as_deref() == Some("Type0") { |
| src/data/visualization_data.rs | 710 | if | if let Some(desc) = first { |
| src/data/visualization_data.rs | 711 | if | if desc.has(b"Widths") { |
| src/data/visualization_data.rs | 720 | match | match object { |
| src/data/visualization_data.rs | 728 | match | match object { |
| src/data/visualization_data.rs | 736 | match | match object { |
| src/data/visualization_data.rs | 740 | if | if let Some(v) = object_to_f32(item) { |
| src/data/visualization_data.rs | 751 | match | match object { |
| src/data/visualization_data.rs | 758 | match | match object { |
| src/data/visualization_data.rs | 765 | match | match object { |
| src/data/visualization_data.rs | 772 | match | match object { |
| src/dependency/file_store.rs | 76 | match | match fs::metadata(path) { |
| src/dependency/file_store.rs | 78 | match_guard | Err(error) if error.kind() == ErrorKind::NotFound => Ok(false), |
| src/dependency/file_store.rs | 96 | if | if empty { |
| src/dependency/file_store.rs | 105 | if | if let Some(parent) = path.parent() { |
| src/dependency/file_store.rs | 106 | if | if !parent.as_os_str().is_empty() { |
| src/dependency/hayro_renderer.rs | 31 | if | if page_count == 0 { |
| src/dependency/hayro_renderer.rs | 55 | if | if !target_dpi.is_finite() \\|\\| target_dpi <= 0.0 { |
| src/dependency/hayro_renderer.rs | 58 | if | if page_index >= self.page_count { |
| src/dependency/pdf_annotator.rs | 49 | if | if let Some(rect_items) = rects_by_page.get(&page_index) { |
| src/dependency/pdf_annotator.rs | 56 | if | if let Some(overlay_items) = overlays_by_page.get(&page_index) { |
| src/dependency/pdf_annotator.rs | 63 | if | if !content.is_empty() { |
| src/dependency/pdf_annotator.rs | 136 | match | match ch { |
| src/dependency/pdf_font_occurrence_accessor.rs | 88 | match | match kind { |
| src/dependency/pdf_font_occurrence_accessor.rs | 189 | if | if let Some(base_font_name) = base { |
| src/dependency/pdf_font_occurrence_accessor.rs | 226 | if | if op_name == "BT" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 235 | if | if op_name == "ET" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 244 | if | if op_name == "Tf" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 247 | if | if op_name == "Tm" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 250 | if | if op_name == "Td" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 253 | if | if op_name == "TD" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 256 | if | if op_name == "T*" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 266 | inline_if | let occ = if op_name == "TJ" \\|\\| op_name == "Tj" \\|\\| op_name == "'" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 333 | if | if !in_text { |
| src/dependency/pdf_font_occurrence_accessor.rs | 339 | if | if trimmed.is_empty() { |
| src/dependency/pdf_font_occurrence_accessor.rs | 378 | match | match first { |
| src/dependency/pdf_font_occurrence_accessor.rs | 399 | match | match object { |
| src/dependency/pdf_font_occurrence_accessor.rs | 414 | match | match object { |
| src/dependency/pdf_font_occurrence_accessor.rs | 429 | match | match object { |
| src/dependency/pdf_font_occurrence_accessor.rs | 444 | match | match object { |
| src/dependency/pdf_font_occurrence_accessor.rs | 559 | if | if !is_subset { |
| src/dependency/pdf_font_occurrence_accessor.rs | 564 | if | if second.is_empty() { |
| src/dependency/pdf_font_occurrence_accessor.rs | 573 | if | if !has_two { |
| src/dependency/pdf_font_occurrence_accessor.rs | 579 | if | if !len_ok { |
| src/dependency/pdf_font_occurrence_accessor.rs | 595 | if | if trimmed.is_empty() { |
| src/dependency/pdf_font_occurrence_accessor.rs | 602 | if | if !two { |
| src/dependency/pdf_font_occurrence_accessor.rs | 609 | inline_if | let variant_opt = if variant.is_empty() { |
| src/dependency/pdf_font_occurrence_accessor.rs | 622 | if | if ext_lower == "pdf" { |
| src/dependency/pdf_font_occurrence_accessor.rs | 630 | if | if is_image { |
| src/dependency/pdf_font_occurrence_accessor.rs | 639 | match | match kind { |
| src/dependency/pdf_font_occurrence_accessor.rs | 695 | if | if let Some(message) = &self.err { |
| src/dependency/pdf_font_run_accessor.rs | 93 | if | if let Some(bytes) = &info.bytes { |
| src/dependency/pdf_font_run_accessor.rs | 133 | match | match op.operator.as_str() { |
| src/dependency/pdf_font_run_accessor.rs | 183 | if | if let Some(tx) = op.operands.get(4).and_then(object_to_f32) { |
| src/dependency/pdf_font_run_accessor.rs | 186 | if | if let Some(ty) = op.operands.get(5).and_then(object_to_f32) { |
| src/dependency/pdf_font_run_accessor.rs | 208 | if | if !st.in_text { |
| src/dependency/pdf_font_run_accessor.rs | 215 | if | if show.text.trim().is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 258 | if | if !st.in_text { |
| src/dependency/pdf_font_run_accessor.rs | 261 | if | if let Some(value) = op.operands.first().and_then(object_to_f32) { |
| src/dependency/pdf_font_run_accessor.rs | 264 | if | if let Some(value) = op.operands.get(1).and_then(object_to_f32) { |
| src/dependency/pdf_font_run_accessor.rs | 271 | if | if show.text.trim().is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 314 | if | if !st.in_text { |
| src/dependency/pdf_font_run_accessor.rs | 320 | if | if show.text.trim().is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 417 | if | if let Some(base_font_name) = base { |
| src/dependency/pdf_font_run_accessor.rs | 443 | if | if let Some(stream) = desc.get(key).ok().and_then(\\|o\\| deref_to_stream(doc, o)) { |
| src/dependency/pdf_font_run_accessor.rs | 445 | if | if let Some(b) = bytes { |
| src/dependency/pdf_font_run_accessor.rs | 458 | if | if op.operator.as_str() == "TJ" { |
| src/dependency/pdf_font_run_accessor.rs | 467 | if | if let Some(value) = decode_pdf_text(item) { |
| src/dependency/pdf_font_run_accessor.rs | 468 | if | if value.is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 475 | if | if let Some(value) = object_to_f32(item) { |
| src/dependency/pdf_font_run_accessor.rs | 476 | if | if total_chars == 0 { |
| src/dependency/pdf_font_run_accessor.rs | 480 | if | if adj.is_finite() && adj.abs() > f32::EPSILON { |
| src/dependency/pdf_font_run_accessor.rs | 501 | if | if leading.is_finite() && leading.abs() > 0.01_f32 { |
| src/dependency/pdf_font_run_accessor.rs | 519 | if | if show.text.is_empty() \\|\\| metrics.char_advances_pt.is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 530 | if | if ch.is_whitespace() { |
| src/dependency/pdf_font_run_accessor.rs | 533 | if | if delta.abs() <= f32::EPSILON { |
| src/dependency/pdf_font_run_accessor.rs | 536 | if | if idx < metrics.char_advances_pt.len() { |
| src/dependency/pdf_font_run_accessor.rs | 543 | if | if !delta.is_finite() \\|\\| delta.abs() <= f32::EPSILON { |
| src/dependency/pdf_font_run_accessor.rs | 546 | if | if *char_idx < metrics.char_advances_pt.len() { |
| src/dependency/pdf_font_run_accessor.rs | 553 | if | if !metrics.width_pt.is_finite() \\|\\| metrics.width_pt <= 0.0_f32 { |
| src/dependency/pdf_font_run_accessor.rs | 576 | if | if text.is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 587 | if | if let Some(info) = font_info { |
| src/dependency/pdf_font_run_accessor.rs | 588 | if | if let Some(bytes) = info.bytes.as_ref() { |
| src/dependency/pdf_font_run_accessor.rs | 589 | if | if let Some(face) = rustybuzz::Face::from_slice(bytes, 0) { |
| src/dependency/pdf_font_run_accessor.rs | 591 | if | if let Some(metrics) = shape_text_metrics( |
| src/dependency/pdf_font_run_accessor.rs | 618 | if | if glyph_positions.is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 627 | if | if !width_pt.is_finite() \\|\\| width_pt <= 0.0_f32 { |
| src/dependency/pdf_font_run_accessor.rs | 676 | inline_if | let table: fn(char) -> i32 = if normalized.contains("times") && normalized.contains("roman") { |
| src/dependency/pdf_font_run_accessor.rs | 690 | if | if !width_pt.is_finite() \\|\\| width_pt <= 0.0_f32 { |
| src/dependency/pdf_font_run_accessor.rs | 707 | if | if char_starts.is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 720 | if | if cluster_advances.is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 734 | if | if end_char <= start_char { |
| src/dependency/pdf_font_run_accessor.rs | 747 | match | match char_starts.binary_search(&byte_offset) { |
| src/dependency/pdf_font_run_accessor.rs | 756 | match | match char_starts.binary_search(&byte_offset) { |
| src/dependency/pdf_font_run_accessor.rs | 764 | if | if char_count == 0 { |
| src/dependency/pdf_font_run_accessor.rs | 767 | if | if advances.len() != char_count |
| src/dependency/pdf_font_run_accessor.rs | 776 | if | if !sum.is_finite() \\|\\| sum <= 0.0_f32 \\|\\| !width_pt.is_finite() { |
| src/dependency/pdf_font_run_accessor.rs | 810 | match | match ch { |
| src/dependency/pdf_font_run_accessor.rs | 911 | match | match ch { |
| src/dependency/pdf_font_run_accessor.rs | 1012 | match | match obj { |
| src/dependency/pdf_font_run_accessor.rs | 1017 | match | match decoded { |
| src/dependency/pdf_font_run_accessor.rs | 1027 | match | match object { |
| src/dependency/pdf_font_run_accessor.rs | 1035 | match | match object { |
| src/dependency/pdf_font_run_accessor.rs | 1042 | match | match object { |
| src/dependency/pdf_font_run_accessor.rs | 1049 | match | match object { |
| src/dependency/pdf_font_run_accessor.rs | 1060 | match | match object { |
| src/dependency/pdf_font_run_accessor.rs | 1078 | if | if !is_subset { |
| src/dependency/pdf_font_run_accessor.rs | 1083 | if | if second.is_empty() { |
| src/dependency/pdf_font_run_accessor.rs | 1092 | if | if !has_two { |
| src/dependency/pdf_font_run_accessor.rs | 1098 | if | if !len_ok { |
| src/dependency/pdf_redaction_accessor.rs | 191 | if | if !s.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 203 | if | if !is_redact_like { |
| src/dependency/pdf_redaction_accessor.rs | 214 | if | if include_details { |
| src/dependency/pdf_redaction_accessor.rs | 215 | if | if !subtype.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 218 | if | if !rt.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 221 | if | if !it.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 224 | if | if !ft.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 227 | if | if !nm.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 230 | if | if !contents.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 332 | if | if op.operator.as_str() != "Do" { |
| src/dependency/pdf_redaction_accessor.rs | 341 | if | if name.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 346 | if | if !visited.insert(key) { |
| src/dependency/pdf_redaction_accessor.rs | 430 | match | match name { |
| src/dependency/pdf_redaction_accessor.rs | 442 | if | if let Some(rect) = rect_from_path_if_axis_aligned_rect(&path) { |
| src/dependency/pdf_redaction_accessor.rs | 444 | inline_if | let score = if is_black { |
| src/dependency/pdf_redaction_accessor.rs | 455 | if | if keep { |
| src/dependency/pdf_redaction_accessor.rs | 457 | if | if include_details { |
| src/dependency/pdf_redaction_accessor.rs | 481 | if | if let Some(rect) = rect_from_re(op.operands.as_slice()) { |
| src/dependency/pdf_redaction_accessor.rs | 483 | inline_if | let score = if is_black { |
| src/dependency/pdf_redaction_accessor.rs | 495 | if | if keep { |
| src/dependency/pdf_redaction_accessor.rs | 497 | if | if include_details { |
| src/dependency/pdf_redaction_accessor.rs | 580 | if | if let Some(f) = self.stack.pop() { |
| src/dependency/pdf_redaction_accessor.rs | 655 | if | if operands.len() == 1 { |
| src/dependency/pdf_redaction_accessor.rs | 658 | if | if operands.len() == 3 { |
| src/dependency/pdf_redaction_accessor.rs | 661 | if | if operands.len() == 4 { |
| src/dependency/pdf_redaction_accessor.rs | 678 | if | if !x.is_finite() \\|\\| !y.is_finite() \\|\\| !w.is_finite() \\|\\| !h.is_finite() { |
| src/dependency/pdf_redaction_accessor.rs | 690 | if | if w <= 0.0 \\|\\| h <= 0.0 { |
| src/dependency/pdf_redaction_accessor.rs | 702 | if | if w <= 0.0 \\|\\| h <= 0.0 \\|\\| page_width_pt <= 0.0 \\|\\| page_height_pt <= 0.0 { |
| src/dependency/pdf_redaction_accessor.rs | 714 | if | if w <= 0.0 \\|\\| h <= 0.0 { |
| src/dependency/pdf_redaction_accessor.rs | 718 | inline_if | let aspect = if h > 0.0 { w / h } else { 0.0 }; |
| src/dependency/pdf_redaction_accessor.rs | 723 | if | if area >= 25.0 { |
| src/dependency/pdf_redaction_accessor.rs | 726 | if | if area >= 200.0 { |
| src/dependency/pdf_redaction_accessor.rs | 729 | if | if aspect >= 2.0 { |
| src/dependency/pdf_redaction_accessor.rs | 732 | if | if aspect >= 6.0 { |
| src/dependency/pdf_redaction_accessor.rs | 735 | if | if w >= 20.0 && h >= 6.0 { |
| src/dependency/pdf_redaction_accessor.rs | 762 | match | match (x_opt, y_opt) { |
| src/dependency/pdf_redaction_accessor.rs | 778 | match | match (x_opt, y_opt) { |
| src/dependency/pdf_redaction_accessor.rs | 795 | if | if !path.closed { |
| src/dependency/pdf_redaction_accessor.rs | 798 | if | if path.points.len() < 4 { |
| src/dependency/pdf_redaction_accessor.rs | 805 | if | if !x.is_finite() \\|\\| !y.is_finite() { |
| src/dependency/pdf_redaction_accessor.rs | 817 | if | if w <= 0.0 \\|\\| h <= 0.0 { |
| src/dependency/pdf_redaction_accessor.rs | 839 | if | if corners_hit < 3 { |
| src/dependency/pdf_redaction_accessor.rs | 852 | if | if v < min_v { |
| src/dependency/pdf_redaction_accessor.rs | 855 | if | if v > max_v { |
| src/dependency/pdf_redaction_accessor.rs | 874 | if | if width == 0 \\|\\| height == 0 { |
| src/dependency/pdf_redaction_accessor.rs | 970 | if | if width == 0 \\|\\| height == 0 { |
| src/dependency/pdf_redaction_accessor.rs | 983 | if | if gray.len() < total_pixels { |
| src/dependency/pdf_redaction_accessor.rs | 993 | if | if px < min_v { |
| src/dependency/pdf_redaction_accessor.rs | 1047 | if | if visited[idx] \\|\\| cell_avg[idx] > threshold { |
| src/dependency/pdf_redaction_accessor.rs | 1061 | while | while let Some((row, col)) = queue.pop_front() { |
| src/dependency/pdf_redaction_accessor.rs | 1066 | if | if row < min_row { |
| src/dependency/pdf_redaction_accessor.rs | 1069 | if | if row > max_row { |
| src/dependency/pdf_redaction_accessor.rs | 1072 | if | if col < min_col { |
| src/dependency/pdf_redaction_accessor.rs | 1075 | if | if col > max_col { |
| src/dependency/pdf_redaction_accessor.rs | 1087 | if | if visited[neighbor_index] { |
| src/dependency/pdf_redaction_accessor.rs | 1090 | if | if cell_avg[neighbor_index] > threshold { |
| src/dependency/pdf_redaction_accessor.rs | 1098 | if | if pixel_area == 0 { |
| src/dependency/pdf_redaction_accessor.rs | 1102 | if | if !(0.0005..=0.9).contains(&area_fraction) { |
| src/dependency/pdf_redaction_accessor.rs | 1105 | if | if min_col >= cols \\|\\| min_row >= rows { |
| src/dependency/pdf_redaction_accessor.rs | 1112 | if | if x1 <= x0 \\|\\| y1 <= y0 { |
| src/dependency/pdf_redaction_accessor.rs | 1122 | if | if short_edge < 4.0 { |
| src/dependency/pdf_redaction_accessor.rs | 1135 | if | if h > 0.0 && w > 0.0 { |
| src/dependency/pdf_redaction_accessor.rs | 1182 | if | if x1_px <= x0_px \\|\\| y1_px <= y0_px { |
| src/dependency/pdf_redaction_accessor.rs | 1200 | if | if size <= 16 { |
| src/dependency/pdf_redaction_accessor.rs | 1208 | if | if size == 0 { |
| src/dependency/pdf_redaction_accessor.rs | 1222 | while | while remaining_bins > 0 { |
| src/dependency/pdf_redaction_accessor.rs | 1238 | if | if result.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 1251 | if | if width == 0 \\|\\| height == 0 \\|\\| gray.len() < width.saturating_mul(height) { |
| src/dependency/pdf_redaction_accessor.rs | 1264 | if | if x1 <= x0 \\|\\| y1 <= y0 { |
| src/dependency/pdf_redaction_accessor.rs | 1287 | if | if px <= dark_threshold { |
| src/dependency/pdf_redaction_accessor.rs | 1305 | while | while idx < dark_cols.len() { |
| src/dependency/pdf_redaction_accessor.rs | 1306 | if | if !dark_cols[idx] { |
| src/dependency/pdf_redaction_accessor.rs | 1311 | while | while idx < dark_cols.len() && dark_cols[idx] { |
| src/dependency/pdf_redaction_accessor.rs | 1315 | if | if end.saturating_sub(start) >= min_run_px { |
| src/dependency/pdf_redaction_accessor.rs | 1327 | inline_if | let split_confidence = if runs.len() <= 1 { |
| src/dependency/pdf_redaction_accessor.rs | 1340 | if | if max_gap_px == 0 { |
| src/dependency/pdf_redaction_accessor.rs | 1343 | if | if dark_ratio < 0.08 { |
| src/dependency/pdf_redaction_accessor.rs | 1358 | if | if columns.is_empty() \\|\\| max_gap == 0 { |
| src/dependency/pdf_redaction_accessor.rs | 1362 | while | while idx < columns.len() { |
| src/dependency/pdf_redaction_accessor.rs | 1363 | if | if columns[idx] { |
| src/dependency/pdf_redaction_accessor.rs | 1368 | while | while idx < columns.len() && !columns[idx] { |
| src/dependency/pdf_redaction_accessor.rs | 1375 | if | if bounded && gap_len <= max_gap { |
| src/dependency/pdf_redaction_accessor.rs | 1384 | if | if profile.dark_runs.len() <= 1 \\|\\| profile.split_confidence < 0.25 { |
| src/dependency/pdf_redaction_accessor.rs | 1395 | if | if abs_x1 <= abs_x0 { |
| src/dependency/pdf_redaction_accessor.rs | 1399 | if | if span < 2 { |
| src/dependency/pdf_redaction_accessor.rs | 1415 | if | if split.len() <= 1 { |
| src/dependency/pdf_redaction_accessor.rs | 1432 | if | if depth > 32 { |
| src/dependency/pdf_redaction_accessor.rs | 1443 | if | if let Ok(obj) = current_dict.get(key) { |
| src/dependency/pdf_redaction_accessor.rs | 1444 | if | if let Some(rect) = object_to_rect_resolved(doc, obj) { |
| src/dependency/pdf_redaction_accessor.rs | 1458 | match | match obj { |
| src/dependency/pdf_redaction_accessor.rs | 1478 | if | if rendered.width_px == 0 \\|\\| rendered.height_px == 0 { |
| src/dependency/pdf_redaction_accessor.rs | 1490 | if | if detection.detections.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 1519 | if | if rect_is_near_full_page_with_size( |
| src/dependency/pdf_redaction_accessor.rs | 1526 | if | if page_rect.width().abs() < 2.0 \\|\\| page_rect.height().abs() < 2.0 { |
| src/dependency/pdf_redaction_accessor.rs | 1543 | if | if cfg.include_details { |
| src/dependency/pdf_redaction_accessor.rs | 1585 | if | if split_count > 1 { |
| src/dependency/pdf_redaction_accessor.rs | 1610 | if | if rgba.len() < expected { |
| src/dependency/pdf_redaction_accessor.rs | 1682 | if | if op.operator.as_str() != "Do" { |
| src/dependency/pdf_redaction_accessor.rs | 1691 | if | if name.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 1696 | if | if !visited.insert(key) { |
| src/dependency/pdf_redaction_accessor.rs | 1758 | match | match op.operator.as_str() { |
| src/dependency/pdf_redaction_accessor.rs | 1787 | if | if !st.in_text { |
| src/dependency/pdf_redaction_accessor.rs | 1792 | if | if text.is_empty() { |
| src/dependency/pdf_redaction_accessor.rs | 1817 | if | if op.operator.as_str() == "TJ" { |
| src/dependency/pdf_redaction_accessor.rs | 1818 | if | if let Some(Object::Array(a)) = op.operands.first() { |
| src/dependency/pdf_redaction_accessor.rs | 1827 | if | if let Some(text) = op.operands.last().and_then(decode_pdf_text) { |
| src/dependency/pdf_redaction_accessor.rs | 1835 | match | match obj { |
| src/dependency/pdf_redaction_accessor.rs | 1840 | match | match decoded { |
| src/dependency/pdf_redaction_accessor.rs | 1860 | match | match obj { |
| src/dependency/pdf_redaction_accessor.rs | 1871 | match | match obj { |
| src/dependency/pdf_redaction_accessor.rs | 1882 | match | match obj { |
| src/dependency/pdf_redaction_accessor.rs | 1896 | match | match o { |
| src/dependency/pdf_redaction_accessor.rs | 1904 | match | match o { |
| src/dependency/pdf_redaction_accessor.rs | 1911 | match | match o { |
| src/dependency/pdf_redaction_accessor.rs | 1924 | if | if a.len() != 4 { |
| src/dependency/pdf_redaction_accessor.rs | 1963 | if | if page_index >= self.page_count { |
| src/logic/local_file_workflow_component.rs | 82 | if | if !exists { |
| src/logic/local_file_workflow_component.rs | 88 | if | if !local_data.is_dir(input_dir)? { |
| src/logic/local_file_workflow_component.rs | 103 | while | while let Some(dir) = dirs.pop() { |
| src/logic/local_file_workflow_component.rs | 112 | if | if local_data.is_dir(&path)? { |
| src/logic/local_file_workflow_component.rs | 116 | if | if is_supported_batch_input(path.as_path()) { |
| src/logic/local_file_workflow_component.rs | 145 | if | if let Some(parent) = relative.parent() { |
| src/logic/local_file_workflow_component.rs | 146 | if | if !parent.as_os_str().is_empty() { |
| src/logic/redaction_guessing_component.rs | 32 | inline_if | let output = if cfg.enable_image_analysis { |
| src/logic/redaction_guessing_component.rs | 65 | inline_if | let visualization_payload = if cfg.visualize { |
| src/logic/redaction_guessing_component.rs | 182 | if | if cfg.visual_score { |
| src/logic/redaction_guessing_component.rs | 190 | inline_if | let visual_result = if let Some(pdf_bytes) = inputs.pdf_bytes { |
| src/logic/redaction_guessing_component.rs | 202 | match | match visual_result { |
| src/logic/redaction_guessing_component.rs | 226 | inline_if | let base = if !guess.exact_matches.is_empty() { |
| src/logic/redaction_guessing_component.rs | 235 | inline_if | let anchor = if !guess.context.has_anchor_pair { |
| src/logic/redaction_guessing_component.rs | 258 | inline_if | let fallback_penalty = if guess.context.width_fallback_reason.is_some() { |
| src/logic/redaction_guessing_component.rs | 283 | match | match self { |
| src/logic/redaction_guessing_component.rs | 452 | match | match self { |
| src/logic/redaction_guessing_component.rs | 647 | if | if !guess.context.has_anchor_pair { |
| src/logic/redaction_guessing_component.rs | 650 | inline_if | let top = if let Some(first) = guess.exact_matches.first() { |
| src/logic/redaction_guessing_component.rs | 697 | if | if !guess.context.has_anchor_pair \\|\\| guess.candidates.is_empty() { |
| src/logic/redaction_guessing_component.rs | 700 | if | if !is_two_sided_anchor_context(guess) { |
| src/logic/redaction_guessing_component.rs | 705 | if | if gap_ratio >= CLUSTER_CONSENSUS_MAX_GAP_RATIO { |
| src/logic/redaction_guessing_component.rs | 729 | if | if indices.len() < 2 { |
| src/logic/redaction_guessing_component.rs | 824 | if | if !guess.context.has_anchor_pair { |
| src/logic/redaction_guessing_component.rs | 827 | if | if !is_two_sided_anchor_context(guess) { |
| src/logic/redaction_guessing_component.rs | 831 | if | if width <= 0.0_f64 { |
| src/logic/redaction_guessing_component.rs | 851 | if | if !guess.context.has_anchor_pair \\|\\| guess.candidates.is_empty() { |
| src/logic/redaction_guessing_component.rs | 863 | if | if indices.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS { |
| src/logic/redaction_guessing_component.rs | 882 | if | if let Some(selected) = solve_joint_assignment_group(guesses, &group) { |
| src/logic/redaction_guessing_component.rs | 886 | if | if let Some(text) = selected_text { |
| src/logic/redaction_guessing_component.rs | 896 | if | if let Some(guess) = guesses.get_mut(guess_index) { |
| src/logic/redaction_guessing_component.rs | 913 | if | if !is_joint_assignment_candidate_row(guess) { |
| src/logic/redaction_guessing_component.rs | 914 | if | if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS { |
| src/logic/redaction_guessing_component.rs | 922 | if | if current.is_empty() { |
| src/logic/redaction_guessing_component.rs | 932 | if | if contiguous { |
| src/logic/redaction_guessing_component.rs | 935 | if | if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS { |
| src/logic/redaction_guessing_component.rs | 944 | if | if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS { |
| src/logic/redaction_guessing_component.rs | 1009 | if | if group.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS \\|\\| group.len() > JOINT_ASSIGNMENT_MAX_ROWS |
| src/logic/redaction_guessing_component.rs | 1024 | if | if options.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1043 | inline_if | let duplicate_penalty_amount = if group.len() >= 3 { |
| src/logic/redaction_guessing_component.rs | 1056 | if | if allow_null { |
| src/logic/redaction_guessing_component.rs | 1069 | if | if duplicate_penalty_amount > 0.0_f64 |
| src/logic/redaction_guessing_component.rs | 1074 | if | if state.prev_end_x_pt.is_finite() { |
| src/logic/redaction_guessing_component.rs | 1079 | if | if option.start_x_pt + 0.5_f64 < state.prev_start_x_pt { |
| src/logic/redaction_guessing_component.rs | 1087 | if | if !used_keys.iter().any(\\|key\\| key == &option.key) { |
| src/logic/redaction_guessing_component.rs | 1100 | if | if next.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1120 | if | if guess.candidates.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1128 | if | if text.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1131 | if | if !seen.insert(text.to_owned()) { |
| src/logic/redaction_guessing_component.rs | 1141 | inline_if | let exact_bonus = if guess.exact_matches.iter().any(\\|value\\| value == text) { |
| src/logic/redaction_guessing_component.rs | 1208 | if | if skip_indices.contains(&index) { |
| src/logic/redaction_guessing_component.rs | 1211 | if | if !guess.context.has_anchor_pair \\|\\| guess.candidates.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1222 | if | if indices.len() < 2 { |
| src/logic/redaction_guessing_component.rs | 1241 | inline_if | let duplicate_penalty_amount = if indices.len() >= 3 { 3.0_f64 } else { 0.0_f64 }; |
| src/logic/redaction_guessing_component.rs | 1244 | if | if guess.candidates.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1252 | if | if duplicate_penalty_amount > 0.0_f64 && used.contains(&key) { |
| src/logic/redaction_guessing_component.rs | 1263 | match | match &best { |
| src/logic/redaction_guessing_component.rs | 1265 | match_guard | Some((_, best_cost)) if cost < *best_cost => { |
| src/logic/redaction_guessing_component.rs | 1291 | if | if target <= 0.0_f64 { |
| src/logic/redaction_guessing_component.rs | 1309 | if | if !target_width_pt.is_finite() |
| src/logic/redaction_guessing_component.rs | 1332 | if | if anchor_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1336 | if | if candidate_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1341 | if | if anchor_tokens.contains(token) { |
| src/logic/redaction_guessing_component.rs | 1345 | if | if matches == 0 { |
| src/logic/redaction_guessing_component.rs | 1349 | if | if matches >= 2 { |
| src/logic/redaction_guessing_component.rs | 1365 | if | if let Some(pos) = guess |
| src/logic/redaction_guessing_component.rs | 1373 | if | if let Some(pos) = guess |
| src/logic/redaction_guessing_component.rs | 1389 | if | if matches!(redaction.kind, RedactionKind::RasterDarkRegion) { |
| src/logic/redaction_guessing_component.rs | 1496 | if | if candidate_width_index.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1542 | if | if multi_span_mode { |
| src/logic/redaction_guessing_component.rs | 1547 | inline_if | let mut band = if ranged.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1560 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1564 | if | if char_units < min_char_units \\|\\| char_units > max_char_units { |
| src/logic/redaction_guessing_component.rs | 1568 | if | if !passes_context_filter( |
| src/logic/redaction_guessing_component.rs | 1576 | if | if list_like_context && !looks_like_alpha_phrase_candidate(trimmed) { |
| src/logic/redaction_guessing_component.rs | 1590 | if | if box_err > box_filter_limit_pt { |
| src/logic/redaction_guessing_component.rs | 1619 | inline_if | let mut band = if ranged.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1631 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1636 | if | if char_units < min_char_units \\|\\| char_units > max_char_units { |
| src/logic/redaction_guessing_component.rs | 1640 | if | if !passes_context_filter( |
| src/logic/redaction_guessing_component.rs | 1648 | if | if list_like_context && !looks_like_alpha_phrase_candidate(trimmed) { |
| src/logic/redaction_guessing_component.rs | 1679 | if | if side_alignment_err > side_alignment_limit { |
| src/logic/redaction_guessing_component.rs | 1729 | inline_if | let selected = if exact_scored.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1743 | inline_if | let denom = if exact_scored.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1762 | if | if asset.is_none() { |
| src/logic/redaction_guessing_component.rs | 1765 | if | if !has_width_table_for_anchor { |
| src/logic/redaction_guessing_component.rs | 1768 | inline_if | let width_fallback_reason = if width_fallback_parts.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1774 | inline_if | let char_width = if !anchor.left_anchor_text.trim().is_empty() { |
| src/logic/redaction_guessing_component.rs | 1843 | if | if row_runs.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1846 | if | if row_runs.is_empty() { |
| src/logic/redaction_guessing_component.rs | 1865 | if | if let (Some(left_hint_text), Some(right_hint_text)) = (left_hint, right_hint) { |
| src/logic/redaction_guessing_component.rs | 1870 | if | if let Some(run) = same_run { |
| src/logic/redaction_guessing_component.rs | 1888 | if | if right_x > left_x { |
| src/logic/redaction_guessing_component.rs | 1922 | if | if right_start <= left_end { |
| src/logic/redaction_guessing_component.rs | 1925 | inline_if | let font_penalty = if left_run.font_key == right_run.font_key |
| src/logic/redaction_guessing_component.rs | 1963 | if | if pairs.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2029 | if | if left_anchor_text.trim().is_empty() \\|\\| right_anchor_text.trim().is_empty() { |
| src/logic/redaction_guessing_component.rs | 2032 | if | if right_x <= left_x { |
| src/logic/redaction_guessing_component.rs | 2106 | if | if let Some(left_run) = left_only { |
| src/logic/redaction_guessing_component.rs | 2118 | if | if !left_anchor_text.trim().is_empty() && right_x > left_x { |
| src/logic/redaction_guessing_component.rs | 2146 | if | if let Some(right_run) = right_only { |
| src/logic/redaction_guessing_component.rs | 2158 | if | if !right_anchor_text.trim().is_empty() && right_x > left_x { |
| src/logic/redaction_guessing_component.rs | 2198 | if | if run_text.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2204 | if | if hint_text.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2207 | if | if run_text == hint_text { |
| src/logic/redaction_guessing_component.rs | 2210 | if | if let Some(prefix_bytes) = run_text.find(hint_text) { |
| src/logic/redaction_guessing_component.rs | 2236 | if | if hint_text.contains(run_text) { |
| src/logic/redaction_guessing_component.rs | 2243 | if | if run.char_advances_pt.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2246 | if | if prefix_bytes == 0 { |
| src/logic/redaction_guessing_component.rs | 2250 | if | if prefix_char_count == 0 { |
| src/logic/redaction_guessing_component.rs | 2253 | if | if prefix_char_count > run.char_advances_pt.len() { |
| src/logic/redaction_guessing_component.rs | 2322 | if | if current_text.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2360 | if | if residual.is_finite() { |
| src/logic/redaction_guessing_component.rs | 2370 | if | if residuals.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2388 | inline_if | let epsilon = if centered.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2412 | if | if !width_pt.is_finite() \\|\\| width_pt <= 0.0_f64 { |
| src/logic/redaction_guessing_component.rs | 2427 | if | if input.text.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2434 | if | if let Some(asset_value) = asset { |
| src/logic/redaction_guessing_component.rs | 2435 | if | if let Some(width) = measure_text_width_pt( |
| src/logic/redaction_guessing_component.rs | 2442 | if | if width.pt.is_finite() && width.pt > 0.0_f64 { |
| src/logic/redaction_guessing_component.rs | 2451 | if | if let Some(table) = width_tables.get(&key) { |
| src/logic/redaction_guessing_component.rs | 2452 | if | if let Some(width) = width_from_table(table, input.text, input.font_size_pt) { |
| src/logic/redaction_guessing_component.rs | 2453 | if | if width.is_finite() && width > 0.0_f64 { |
| src/logic/redaction_guessing_component.rs | 2521 | match_guard | (Some(first), Some(widths)) if !widths.is_empty() => (first, widths), |
| src/logic/redaction_guessing_component.rs | 2541 | if | if dict.has(b"Widths") { |
| src/logic/redaction_guessing_component.rs | 2545 | if | if subtype.as_deref() == Some("Type0") { |
| src/logic/redaction_guessing_component.rs | 2553 | if | if let Some(descendant) = first { |
| src/logic/redaction_guessing_component.rs | 2554 | if | if descendant.has(b"Widths") { |
| src/logic/redaction_guessing_component.rs | 2567 | if | if codepoint > u16::MAX as u32 { |
| src/logic/redaction_guessing_component.rs | 2571 | if | if codepoint < table.first_char { |
| src/logic/redaction_guessing_component.rs | 2575 | if | if index >= table.widths.len() { |
| src/logic/redaction_guessing_component.rs | 2586 | inline_if | let table: fn(char) -> i32 = if normalized.contains("times") && normalized.contains("roman") |
| src/logic/redaction_guessing_component.rs | 2602 | match | match object { |
| src/logic/redaction_guessing_component.rs | 2610 | match | match object { |
| src/logic/redaction_guessing_component.rs | 2618 | match | match object { |
| src/logic/redaction_guessing_component.rs | 2622 | if | if let Some(value) = object_to_width_f64(item) { |
| src/logic/redaction_guessing_component.rs | 2633 | match | match object { |
| src/logic/redaction_guessing_component.rs | 2640 | match | match object { |
| src/logic/redaction_guessing_component.rs | 2647 | match | match object { |
| src/logic/redaction_guessing_component.rs | 2657 | match | match object { |
| src/logic/redaction_guessing_component.rs | 2714 | match | match ch { |
| src/logic/redaction_guessing_component.rs | 2815 | match | match ch { |
| src/logic/redaction_guessing_component.rs | 2934 | if | if !left_anchor_text.trim().is_empty() && !right_anchor_text.trim().is_empty() { |
| src/logic/redaction_guessing_component.rs | 2935 | if | if let (Some(l), Some(r)) = (left_bbox, right_bbox) { |
| src/logic/redaction_guessing_component.rs | 2940 | if | if w > 0.0 { |
| src/logic/redaction_guessing_component.rs | 2954 | if | if let (Some(b), count) = (left_bbox, left_anchor_text.chars().count()) { |
| src/logic/redaction_guessing_component.rs | 2955 | if | if count > 0 { |
| src/logic/redaction_guessing_component.rs | 2957 | if | if w > 0.0_f64 { |
| src/logic/redaction_guessing_component.rs | 2962 | if | if let (Some(b), count) = (right_bbox, right_anchor_text.chars().count()) { |
| src/logic/redaction_guessing_component.rs | 2963 | if | if count > 0 { |
| src/logic/redaction_guessing_component.rs | 2965 | if | if w > 0.0_f64 { |
| src/logic/redaction_guessing_component.rs | 2970 | if | if !samples.is_empty() { |
| src/logic/redaction_guessing_component.rs | 2975 | if | if fallback > 0.0 { |
| src/logic/redaction_guessing_component.rs | 2991 | if | if ch.is_whitespace() { |
| src/logic/redaction_guessing_component.rs | 3045 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3049 | if | if char_units < min_char_units \\|\\| char_units > max_char_units { |
| src/logic/redaction_guessing_component.rs | 3052 | if | if !seen.insert(trimmed.to_owned()) { |
| src/logic/redaction_guessing_component.rs | 3078 | if | if let Some(existing) = cache |
| src/logic/redaction_guessing_component.rs | 3099 | if | if canonical.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3102 | if | if let Some(variants) = cache.variants.get(&canonical) { |
| src/logic/redaction_guessing_component.rs | 3114 | if | if ch.is_whitespace() { |
| src/logic/redaction_guessing_component.rs | 3115 | if | if !in_space && !out.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3136 | if | if !tokens.is_empty() && has_special_name_structure(canonical, &tokens) { |
| src/logic/redaction_guessing_component.rs | 3146 | if | if !core.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3149 | if | if !given_first.is_empty() && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3156 | if | if !surname.is_empty() && !given_first.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3163 | if | if !prefix.is_empty() && !given_first.is_empty() && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3170 | if | if !suffix.is_empty() && !given_first.is_empty() && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3177 | if | if !prefix.is_empty() |
| src/logic/redaction_guessing_component.rs | 3188 | if | if !given.is_empty() && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3195 | if | if !given.is_empty() && !surname.is_empty() && !suffix.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3202 | if | if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3209 | if | if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() && !suffix.is_empty() |
| src/logic/redaction_guessing_component.rs | 3217 | if | if !given_first.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3220 | if | if !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3223 | if | if !surname_last.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3226 | if | if !prefix.is_empty() && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3233 | if | if !suffix.is_empty() && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3240 | if | if !core.is_empty() && !canonical.contains(',') { |
| src/logic/redaction_guessing_component.rs | 3242 | if | if let (Some(first), Some(last)) = (split.next(), core.split_whitespace().last()) { |
| src/logic/redaction_guessing_component.rs | 3243 | if | if first != last { |
| src/logic/redaction_guessing_component.rs | 3252 | if | if parts.given_tokens.len() >= 2 && !surname.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3259 | if | if !middle_initials.is_empty() && !given_first.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3275 | if | if !canonical.contains(',') { |
| src/logic/redaction_guessing_component.rs | 3308 | if | if out.len() >= MAX_NAME_VARIANTS_PER_ENTRY { |
| src/logic/redaction_guessing_component.rs | 3312 | if | if out.len() > MAX_NAME_VARIANTS_PER_ENTRY { |
| src/logic/redaction_guessing_component.rs | 3335 | if | if left_tokens.is_empty() \\|\\| right_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3340 | if | if right_core_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3348 | if | if core_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3367 | if | if core_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3371 | if | if given_tokens.is_empty() && !core_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3374 | if | if surname_tokens.is_empty() && !core_tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3396 | while | while prefix_end < tokens.len() && is_name_prefix_token(&tokens[prefix_end]) { |
| src/logic/redaction_guessing_component.rs | 3400 | while | while suffix_start > prefix_end && is_name_suffix_token(&tokens[suffix_start - 1]) { |
| src/logic/redaction_guessing_component.rs | 3411 | if | if tokens.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3414 | if | if tokens.len() == 1 { |
| src/logic/redaction_guessing_component.rs | 3418 | while | while surname_start > 0 && is_surname_particle_token(&tokens[surname_start - 1]) { |
| src/logic/redaction_guessing_component.rs | 3421 | if | if surname_start == 0 { |
| src/logic/redaction_guessing_component.rs | 3462 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3466 | if | if normalized.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3469 | if | if seen.insert(normalized.clone()) { |
| src/logic/redaction_guessing_component.rs | 3478 | if | if ch.is_alphabetic() { |
| src/logic/redaction_guessing_component.rs | 3479 | if | if new_word { |
| src/logic/redaction_guessing_component.rs | 3510 | inline_if | let y_tolerance = if tight { 12.0_f32 } else { 20.0_f32 }; |
| src/logic/redaction_guessing_component.rs | 3511 | inline_if | let baseline_tolerance = if tight { 8.0_f32 } else { 16.0_f32 }; |
| src/logic/redaction_guessing_component.rs | 3512 | inline_if | let x_tolerance = if tight { 120.0_f64 } else { 220.0_f64 }; |
| src/logic/redaction_guessing_component.rs | 3528 | if | if tight { |
| src/logic/redaction_guessing_component.rs | 3538 | if | if run_text == target { |
| src/logic/redaction_guessing_component.rs | 3549 | if | if entries.is_empty() \\|\\| !min_width_pt.is_finite() \\|\\| !max_width_pt.is_finite() { |
| src/logic/redaction_guessing_component.rs | 3552 | if | if min_width_pt > max_width_pt { |
| src/logic/redaction_guessing_component.rs | 3557 | if | if start >= end \\|\\| start >= entries.len() { |
| src/logic/redaction_guessing_component.rs | 3568 | if | if entries.len() <= limit \\|\\| limit == 0 { |
| src/logic/redaction_guessing_component.rs | 3575 | if | if end - start < limit { |
| src/logic/redaction_guessing_component.rs | 3589 | if | if right_lower.starts_with("and") && candidate.contains(',') { |
| src/logic/redaction_guessing_component.rs | 3592 | if | if left_lower.contains("including") && right_lower.starts_with("and") { |
| src/logic/redaction_guessing_component.rs | 3594 | if | if count > 3 { |
| src/logic/redaction_guessing_component.rs | 3616 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3619 | if | if trimmed.contains(',') |
| src/logic/redaction_guessing_component.rs | 3627 | if | if trimmed.chars().any(\\|ch\\| ch.is_ascii_digit()) { |
| src/logic/redaction_guessing_component.rs | 3631 | if | if words.is_empty() \\|\\| words.len() > 4 { |
| src/logic/redaction_guessing_component.rs | 3650 | if | if candidate_trim.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3658 | if | if list_context { |
| src/logic/redaction_guessing_component.rs | 3659 | if | if word_count == 1 \\|\\| word_count >= 5 { |
| src/logic/redaction_guessing_component.rs | 3662 | if | if candidate_trim.contains('-') { |
| src/logic/redaction_guessing_component.rs | 3665 | if | if candidate_trim.contains(',') { |
| src/logic/redaction_guessing_component.rs | 3668 | if | if candidate_trim.contains('(') \\|\\| candidate_trim.contains(')') { |
| src/logic/redaction_guessing_component.rs | 3671 | if | if candidate_trim.contains('/') \\|\\| candidate_trim.contains('&') { |
| src/logic/redaction_guessing_component.rs | 3676 | if | if (right_lower.starts_with(',') \\|\\| right_lower.starts_with("and ")) |
| src/logic/redaction_guessing_component.rs | 3682 | if | if candidate_trim.chars().any(\\|ch\\| ch.is_ascii_digit()) { |
| src/logic/redaction_guessing_component.rs | 3872 | match | match cfg.mode { |
| src/logic/redaction_guessing_component.rs | 3874 | match | match retriever.annotation_redactions(*page_index, cfg.include_details) { |
| src/logic/redaction_guessing_component.rs | 3883 | match | match cfg.mode { |
| src/logic/redaction_guessing_component.rs | 3885 | match | match retriever.drawn_redactions(*page_index, cfg.include_details, false) { |
| src/logic/redaction_guessing_component.rs | 3896 | if | if cfg.enable_image_analysis { |
| src/logic/redaction_guessing_component.rs | 3961 | if | if seen.insert(key) { |
| src/logic/redaction_guessing_component.rs | 3975 | if | if page_indices.is_empty() { |
| src/logic/redaction_guessing_component.rs | 3988 | match | match retriever.raster_redactions(*page_index, &prepass_cfg) { |
| src/logic/redaction_guessing_component.rs | 3990 | if | if !v.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4012 | match | match retriever.raster_redactions(*page_index, &highpass_cfg) { |
| src/logic/redaction_guessing_component.rs | 4013 | match_guard | Ok(v) if !v.is_empty() => out.extend(v), |
| src/logic/redaction_guessing_component.rs | 4016 | if | if let Some(prepass_hits) = prepass_by_page.remove(page_index) { |
| src/logic/redaction_guessing_component.rs | 4023 | if | if let Some(prepass_hits) = prepass_by_page.remove(page_index) { |
| src/logic/redaction_guessing_component.rs | 4057 | if | if page_redactions.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4069 | if | if hits.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4094 | if | if vertical_overlap(&hit.bbox, &band) <= 0.0 && !close_in_y { |
| src/logic/redaction_guessing_component.rs | 4160 | if | if before_anchor.is_none() && after_anchor.is_none() { |
| src/logic/redaction_guessing_component.rs | 4168 | if | if overlap_pt > LARGE_OVERLAP_PT && before_anchor.is_none() { |
| src/logic/redaction_guessing_component.rs | 4186 | if | if overlap_pt > LARGE_OVERLAP_PT && before_anchor.is_some() && after_anchor.is_some() { |
| src/logic/redaction_guessing_component.rs | 4190 | inline_if | let before_gap_rank = if let Some(pos) = before_anchor { |
| src/logic/redaction_guessing_component.rs | 4196 | inline_if | let after_gap_rank = if let Some(pos) = after_anchor { |
| src/logic/redaction_guessing_component.rs | 4204 | match | match &best_line { |
| src/logic/redaction_guessing_component.rs | 4206 | match_guard | Some((_, _, _, best_score)) if score < *best_score => { |
| src/logic/redaction_guessing_component.rs | 4253 | while | while start > 0 { |
| src/logic/redaction_guessing_component.rs | 4256 | if | if word_gap(&hits[prev].bbox, &hits[cur].bbox) > WORD_JOIN_GAP_PT { |
| src/logic/redaction_guessing_component.rs | 4262 | if | if phrase.len() > MAX_CONTEXT_WORDS_PER_SIDE { |
| src/logic/redaction_guessing_component.rs | 4274 | while | while end + 1 < line.len() { |
| src/logic/redaction_guessing_component.rs | 4277 | if | if word_gap(&hits[cur].bbox, &hits[next].bbox) > WORD_JOIN_GAP_PT { |
| src/logic/redaction_guessing_component.rs | 4283 | if | if phrase.len() > MAX_CONTEXT_WORDS_PER_SIDE { |
| src/logic/redaction_guessing_component.rs | 4312 | if | if phrase_indices.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4334 | if | if !trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4375 | if | if page_index >= self.page_count { |
| src/logic/redaction_guessing_component.rs | 4435 | if | if (cfg.raster_dpi - 18.0_f32).abs() < f32::EPSILON { |
| src/logic/redaction_guessing_component.rs | 4438 | if | if (cfg.raster_dpi - 96.0_f32).abs() < f32::EPSILON { |
| src/logic/redaction_guessing_component.rs | 4683 | if | if !cfg.enabled { |
| src/logic/redaction_guessing_component.rs | 4686 | if | if !cfg.dpi.is_finite() \\|\\| cfg.dpi <= 0.0_f32 { |
| src/logic/redaction_guessing_component.rs | 4689 | if | if cfg.min_ink_pixels == 0 { |
| src/logic/redaction_guessing_component.rs | 4692 | if | if let Some(threshold) = cfg.drop_threshold { |
| src/logic/redaction_guessing_component.rs | 4693 | if | if !threshold.is_finite() \\|\\| threshold < 0.0_f32 { |
| src/logic/redaction_guessing_component.rs | 4699 | if | if max_items == 0 { |
| src/logic/redaction_guessing_component.rs | 4728 | inline_if | let dpi_ratio = if cfg.dpi <= 0.0_f32 { |
| src/logic/redaction_guessing_component.rs | 4740 | if | if overlays_by_redaction.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4771 | if | if page_crop_boxes.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4800 | if | if let Some(first) = overlays.first() { |
| src/logic/redaction_guessing_component.rs | 4840 | if | if top_guess_text(guess).is_none() { |
| src/logic/redaction_guessing_component.rs | 4850 | if | if overlays.is_empty() { |
| src/logic/redaction_guessing_component.rs | 4875 | if | if let Some(context_overlays) = context_overlays_by_redaction.get(&index) { |
| src/logic/redaction_guessing_component.rs | 4876 | if | if let Some(context_window_bbox) = |
| src/logic/redaction_guessing_component.rs | 4879 | if | if let Some(context_score) = score_row_overlay( |
| src/logic/redaction_guessing_component.rs | 4888 | if | if context_score.mean_abs_diff > CONTEXT_ALIGNMENT_MAX_DIFF { |
| src/logic/redaction_guessing_component.rs | 4899 | if | if rerank_enabled && should_visual_rerank_row(guess, overlays) { |
| src/logic/redaction_guessing_component.rs | 4902 | match | match score_top_k_candidates_for_row( |
| src/logic/redaction_guessing_component.rs | 4914 | match_guard | Ok(mut candidate_scores) if !candidate_scores.is_empty() => { |
| src/logic/redaction_guessing_component.rs | 4935 | if | if let Some(before) = top_before.as_deref() { |
| src/logic/redaction_guessing_component.rs | 4936 | if | if !before.eq_ignore_ascii_case(&chosen.text) |
| src/logic/redaction_guessing_component.rs | 4939 | if | if let Some(original) = candidate_scores |
| src/logic/redaction_guessing_component.rs | 4954 | if | if let Some(before) = top_before.as_deref() { |
| src/logic/redaction_guessing_component.rs | 4955 | if | if !before.eq_ignore_ascii_case(&chosen.text) { |
| src/logic/redaction_guessing_component.rs | 4962 | if | if let Some(threshold) = cfg.drop_threshold { |
| src/logic/redaction_guessing_component.rs | 4963 | if | if chosen.score.mean_abs_diff > threshold { |
| src/logic/redaction_guessing_component.rs | 4981 | if | if row_scored { |
| src/logic/redaction_guessing_component.rs | 4994 | if | if guess.visual_reason.is_none() { |
| src/logic/redaction_guessing_component.rs | 5005 | if | if let Some(threshold) = cfg.drop_threshold { |
| src/logic/redaction_guessing_component.rs | 5006 | if | if score.mean_abs_diff > threshold { |
| src/logic/redaction_guessing_component.rs | 5032 | inline_if | let rerank_changed_ratio = if rerank_rows_scored == 0 { |
| src/logic/redaction_guessing_component.rs | 5037 | inline_if | let rerank_mean_gain = if rerank_rows_scored == 0 { |
| src/logic/redaction_guessing_component.rs | 5054 | inline_if | let rerank_mean_eval_ms_per_candidate = if rerank_candidate_evals == 0 { |
| src/logic/redaction_guessing_component.rs | 5059 | inline_if | let rerank_mean_eval_ms_per_row = if rerank_rows_scored == 0 { |
| src/logic/redaction_guessing_component.rs | 5106 | if | if overlays.len() < 3 { |
| src/logic/redaction_guessing_component.rs | 5121 | if | if let Some(exact) = guess.exact_matches.first() { |
| src/logic/redaction_guessing_component.rs | 5131 | if | if let Some(pos) = guess |
| src/logic/redaction_guessing_component.rs | 5139 | if | if let Some(pos) = guess |
| src/logic/redaction_guessing_component.rs | 5154 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5158 | if | if seen.insert(key) { |
| src/logic/redaction_guessing_component.rs | 5161 | if | if out.len() >= top_k { |
| src/logic/redaction_guessing_component.rs | 5167 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5171 | if | if seen.insert(key) { |
| src/logic/redaction_guessing_component.rs | 5174 | if | if out.len() >= top_k { |
| src/logic/redaction_guessing_component.rs | 5187 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5191 | if | if seen.insert(key) { |
| src/logic/redaction_guessing_component.rs | 5194 | if | if out.len() >= VISUAL_RERANK_MAX_EVAL_CANDIDATES { |
| src/logic/redaction_guessing_component.rs | 5208 | if | if trimmed.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5212 | if | if seen.contains(&key) { |
| src/logic/redaction_guessing_component.rs | 5221 | if | if width_delta_ratio > VISUAL_RERANK_MAX_WIDTH_DELTA_RATIO_FOR_EXPANSION { |
| src/logic/redaction_guessing_component.rs | 5225 | if | if score_gap > VISUAL_RERANK_MAX_SCORE_GAP_FOR_EXPANSION { |
| src/logic/redaction_guessing_component.rs | 5243 | if | if seen.insert(key) { |
| src/logic/redaction_guessing_component.rs | 5246 | if | if out.len() >= VISUAL_RERANK_MAX_EVAL_CANDIDATES { |
| src/logic/redaction_guessing_component.rs | 5254 | if | if overlays.len() < 3 { |
| src/logic/redaction_guessing_component.rs | 5257 | if | if !guess.exact_matches.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5272 | if | if left.text.trim().is_empty() \\|\\| right.text.trim().is_empty() { |
| src/logic/redaction_guessing_component.rs | 5277 | if | if texts.len() < 2 { |
| src/logic/redaction_guessing_component.rs | 5282 | if | if !top.is_finite() \\|\\| !second.is_finite() { |
| src/logic/redaction_guessing_component.rs | 5285 | if | if top > VISUAL_RERANK_MAX_TOP_SCORE { |
| src/logic/redaction_guessing_component.rs | 5295 | inline_if | let edge_overlap_quality = if score.edge_ink_overlap_ratio.is_finite() { |
| src/logic/redaction_guessing_component.rs | 5300 | inline_if | let edge_clean_quality = if score.edge_ink_mismatch_ratio.is_finite() { |
| src/logic/redaction_guessing_component.rs | 5313 | if | if let Some(candidate) = guess |
| src/logic/redaction_guessing_component.rs | 5320 | if | if guess |
| src/logic/redaction_guessing_component.rs | 5331 | if | if let Some(width) = guess |
| src/logic/redaction_guessing_component.rs | 5360 | if | if template_overlays.len() < 3 { |
| src/logic/redaction_guessing_component.rs | 5401 | if | if texts.len() < 2 { |
| src/logic/redaction_guessing_component.rs | 5429 | if | if !text.eq_ignore_ascii_case(&top_text) { |
| src/logic/redaction_guessing_component.rs | 5431 | if | if geometric_gap > VISUAL_RERANK_MAX_GEOMETRIC_GAP_FOR_EVAL { |
| src/logic/redaction_guessing_component.rs | 5434 | if | if (candidate_width - top_width).abs() < min_shift_pt { |
| src/logic/redaction_guessing_component.rs | 5476 | if | if out.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5554 | if | if let Some(page_box) = page_boxes.get(page_index).copied() { |
| src/logic/redaction_guessing_component.rs | 5556 | if | if coverage >= VISUAL_TILE_MAX_COVERAGE_FOR_CROP { |
| src/logic/redaction_guessing_component.rs | 5568 | if | if page_crop_boxes.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5606 | if | if base.width_px != overlaid.width_px \\|\\| base.height_px != overlaid.height_px { |
| src/logic/redaction_guessing_component.rs | 5609 | if | if base.pixels.len() != overlaid.pixels.len() \\|\\| base.pixels.is_empty() { |
| src/logic/redaction_guessing_component.rs | 5642 | if | if let Some(red_box) = redaction { |
| src/logic/redaction_guessing_component.rs | 5643 | if | if point_in_rect_px(x, y, red_box) { |
| src/logic/redaction_guessing_component.rs | 5644 | if | if !point_in_inner_edge_band_px(x, y, red_box, edge_band_px) { |
| src/logic/redaction_guessing_component.rs | 5653 | if | if index + 2 >= base.pixels.len() { |
| src/logic/redaction_guessing_component.rs | 5658 | if | if base_luma >= BACKGROUND_LUMA_THRESHOLD && over_luma >= BACKGROUND_LUMA_THRESHOLD |
| src/logic/redaction_guessing_component.rs | 5662 | if | if inside_redaction_edge_band |
| src/logic/redaction_guessing_component.rs | 5670 | inline_if | let edge_weight = if inside_redaction_edge_band { |
| src/logic/redaction_guessing_component.rs | 5677 | if | if (inside_redaction_edge_band \\|\\| outside_redaction_edge_band) |
| src/logic/redaction_guessing_component.rs | 5681 | if | if base_luma <= EDGE_INK_MATCH_BASE_LUMA_THRESHOLD { |
| src/logic/redaction_guessing_component.rs | 5690 | if | if delta >= CHANGED_LUMA_DELTA { |
| src/logic/redaction_guessing_component.rs | 5696 | if | if compared_pixels < min_ink_pixels { |
| src/logic/redaction_guessing_component.rs | 5700 | inline_if | let (edge_ink_overlap_ratio, edge_ink_mismatch_ratio) = if edge_ink_weight <= 0.0_f32 { |
| src/logic/redaction_guessing_component.rs | 5719 | if | if rgba.len() < 3 { |
| src/logic/redaction_guessing_component.rs | 5735 | if | if band_px == 0 { |
| src/logic/redaction_guessing_component.rs | 5740 | if | if y < y_min \\|\\| y >= y_max { |
| src/logic/redaction_guessing_component.rs | 5756 | if | if band_px == 0 { |
| src/logic/redaction_guessing_component.rs | 5759 | if | if !point_in_rect_px(x, y, rect) { |
| src/logic/redaction_guessing_component.rs | 5776 | if | if dpi <= 0.0_f32 \\|\\| width_px == 0 \\|\\| height_px == 0 { |
| src/logic/redaction_guessing_component.rs | 5790 | if | if x1_px <= x0_px \\|\\| y1_px <= y0_px { |
| src/logic/redaction_guessing_component.rs | 5817 | if | if depth > 32 { |
| src/logic/redaction_guessing_component.rs | 5827 | if | if let Ok(value) = dict.get(key) { |
| src/logic/redaction_guessing_component.rs | 5828 | if | if let Some(rect) = object_to_rect_resolved(doc, value) { |
| src/logic/redaction_guessing_component.rs | 5842 | match | match object { |
| src/logic/redaction_guessing_component.rs | 5855 | if | if values.len() < 4 { |
| src/logic/redaction_guessing_component.rs | 5866 | match | match object { |
| src/main.rs | 35 | match | match run() { |
| src/main.rs | 64 | if | if args.input.is_dir() { |
| src/service/unredact_cli_entry.rs | 144 | inline_if | let visualize_ms = if req.cfg.visualize { |
| src/service/unredact_cli_entry.rs | 176 | if | if inputs.is_empty() { |
| src/service/unredact_cli_entry.rs | 259 | match | match run { |
| src/service/unredact_web_entry.rs | 77 | inline_if | let visualize_ms = if should_visualize { |
| src/types/redaction_types.rs | 15 | inline_if | let (min_x, max_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) }; |
| src/types/redaction_types.rs | 16 | inline_if | let (min_y, max_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) }; |
| tests/dictionary_entry_format_behavior.rs | 94 | if | if !normalized.is_empty() && seen.insert(normalized.clone()) { |
| tests/dictionary_entry_format_behavior.rs | 100 | if | if !normalized.is_empty() && seen.insert(normalized.clone()) { |
| tests/dictionary_entry_format_behavior.rs | 146 | if | if output_dir.exists() { |
| tests/efta00038617_guessing.rs | 42 | if | if trimmed.is_empty() { |
| tests/efta00038617_guessing.rs | 45 | if | if target_set.contains(&trimmed.to_ascii_uppercase()) { |
| tests/efta00038617_guessing.rs | 49 | if | if lines.len() >= 1_200 { |
| tests/efta00038617_guessing.rs | 98 | if | if !normalized.is_empty() && seen.insert(normalized.clone()) { |
| tests/efta00038617_guessing.rs | 104 | if | if !normalized.is_empty() && seen.insert(normalized.clone()) { |
| tests/efta00038617_guessing.rs | 126 | if | if !row.context.has_anchor_pair { |
| tests/efta00038617_guessing.rs | 130 | if | if width <= 0.0 { |
| tests/efta00038617_guessing.rs | 146 | if | if output_dir.exists() { |
| tests/efta00038617_guessing.rs | 214 | if | if (left_center_y - right_center_y).abs() <= 4.0_f32 { |
| tests/efta00038617_guessing.rs | 217 | ternary | "first-bullet redaction boxes overlap on same row: left={:?} right={:?}", |
| tests/efta00101126_guessing.rs | 17 | if | if output_dir.exists() { |
| tests/integration_black_box_boundary.rs | 19 | if | if !path.is_file() { |
| tests/integration_black_box_boundary.rs | 22 | if | if path.extension().and_then(\\|value\\| value.to_str()) != Some("rs") { |
| tests/integration_black_box_boundary.rs | 25 | if | if path.file_name().and_then(\\|value\\| value.to_str()) |
| tests/integration_black_box_boundary.rs | 40 | if | if line.trim_start().starts_with(prefix) { |
| tests/shared_workflow_purity.rs | 34 | if | if line.contains(pattern) { |
| tests/shared_workflow_purity.rs | 70 | if | if source.contains(token) { |
| tests/shared_workflow_purity.rs | 91 | if | if in_test_module { |
| tests/shared_workflow_purity.rs | 94 | if | if test_module_depth <= 0 { |
| tests/shared_workflow_purity.rs | 101 | if | if trimmed.starts_with("#[cfg(test)]") { |
| tests/shared_workflow_purity.rs | 106 | if | if pending_cfg_test { |
| tests/shared_workflow_purity.rs | 107 | if | if trimmed.starts_with("mod ") && trimmed.ends_with('{') { |
| tests/shared_workflow_purity.rs | 111 | if | if test_module_depth <= 0 { |
| tests/shared_workflow_purity.rs | 119 | if | if !trimmed.is_empty() && !trimmed.starts_with('#') { |
| tests/web_ui_batch_benchmark.rs | 6 | if | if !should_run { |
| tests/web_ui_batch_benchmark.rs | 11 | inline_if | let npm = if cfg!(target_os = "windows") { |
| web/app.js | 58 | if | if (items.length === 0) { |
| web/app.js | 72 | if | if (!downloadBatchZipButton) { |
| web/app.js | 83 | if | if (value == null) { |
| web/app.js | 86 | if | if (value instanceof Uint8Array) { |
| web/app.js | 89 | if | if (ArrayBuffer.isView(value)) { |
| web/app.js | 92 | if | if (Array.isArray(value)) { |
| web/app.js | 100 | ternary | return Number.isFinite(parsed) ? parsed : fallback; |
| web/app.js | 104 | if | if (!Number.isFinite(value) \\|\\| value < 0) { |
| web/app.js | 110 | while | while (scaled >= 1024 && index < units.length - 1) { |
| web/app.js | 114 | ternary | return `${scaled.toFixed(index === 0 ? 0 : 2)} ${units[index]}`; |
| web/app.js | 118 | if | if (!Number.isFinite(value) \\|\\| value < 0) { |
| web/app.js | 137 | while | while (usedLowerPaths.has(candidate.toLowerCase())) { |
| web/app.js | 139 | if | if (dot > 0) { |
| web/app.js | 163 | ternary | value = (value & 1) !== 0 ? 0xedb88320 ^ (value >>> 1) : value >>> 1; |
| web/app.js | 184 | ternary | const safeDate = date instanceof Date ? date : new Date(); |
| web/app.js | 267 | if | if (entries.length > 0xffff) { |
| web/app.js | 273 | if | if (typeof onProgress === "function") { |
| web/app.js | 277 | if | if (nameBytes.length > 0xffff) { |
| web/app.js | 282 | if | if (size > 0xffffffff) { |
| web/app.js | 318 | if | if ( |
| web/app.js | 331 | if | if ( |
| web/app.js | 340 | if | if (seen.has(key)) { |
| web/app.js | 359 | if | if (Array.isArray(row?.exact_matches) && row.exact_matches.length > 0) { |
| web/app.js | 362 | if | if (Array.isArray(row?.candidates) && row.candidates.length > 0) { |
| web/app.js | 369 | ternary | const guesses = Array.isArray(report?.guesses) ? report.guesses : []; |
| web/app.js | 384 | ternary | const guesses = Array.isArray(report?.guesses) ? report.guesses : []; |
| web/app.js | 387 | ternary | topGuess: guesses.length > 0 ? topGuessText(guesses[0]) : "(no guess)", |
| web/app.js | 399 | ternary | return Number.isFinite(number) ? number.toFixed(digits) : "—"; |
| web/app.js | 403 | if | if (value == null) { |
| web/app.js | 407 | ternary | return text === "" ? "—" : text; |
| web/app.js | 411 | if | if (!Array.isArray(row?.exact_matches) \\|\\| row.exact_matches.length === 0) { |
| web/app.js | 418 | if | if (!Array.isArray(row?.candidates) \\|\\| row.candidates.length === 0) { |
| web/app.js | 427 | if | if (key === "—" && name === "—") { |
| web/app.js | 430 | if | if (key === "—") { |
| web/app.js | 433 | if | if (name === "—") { |
| web/app.js | 450 | ternary | const guesses = Array.isArray(report?.guesses) ? report.guesses : []; |
| web/app.js | 472 | ternary | visualDropped: row?.visual_dropped ? "yes" : "no", |
| web/app.js | 481 | if | if (className) { |
| web/app.js | 561 | if | if (rows.length === 0) { |
| web/app.js | 626 | ternary | return normalized.trim() !== "" ? normalized : `file_${id}`; |
| web/app.js | 648 | if | if (openDbPromise) { |
| web/app.js | 652 | if | if (!("indexedDB" in window)) { |
| web/app.js | 659 | if | if (!db.objectStoreNames.contains(RESULTS_STORE)) { |
| web/app.js | 705 | if | if (typeof onProgress === "function") { |
| web/app.js | 710 | if | if (!stored) { |
| web/app.js | 716 | if | if (!blob \\|\\| !name) { |
| web/app.js | 766 | if | if (isRunning \\|\\| isExportingBatchZip) { |
| web/app.js | 771 | if | if (results.length === 0) { |
| web/app.js | 796 | if | if (shouldLog) { |
| web/app.js | 817 | if | if (!selectedOutputUrls) { |
| web/app.js | 820 | if | if (selectedOutputUrls.redactionsJsonUrl) { |
| web/app.js | 823 | if | if (selectedOutputUrls.fontsJsonUrl) { |
| web/app.js | 826 | if | if (selectedOutputUrls.guessesJsonUrl) { |
| web/app.js | 829 | if | if (selectedOutputUrls.visualizedPdfUrl) { |
| web/app.js | 842 | if | if (!url) { |
| web/app.js | 854 | if | if (!result \\|\\| !urls) { |
| web/app.js | 866 | if | if (urls.visualizedPdfUrl && result.outputNames.visualized) { |
| web/app.js | 897 | if | if (batchResults.length === 0) { |
| web/app.js | 910 | if | if (result.id === selectedResultId) { |
| web/app.js | 923 | ternary | badge.textContent = result.status === "ok" ? "OK" : "ERROR"; |
| web/app.js | 929 | if | if (result.status === "ok") { |
| web/app.js | 938 | if | if (result.status === "ok") { |
| web/app.js | 959 | if | if (!memory) { |
| web/app.js | 973 | if | if (!navigator.storage?.estimate) { |
| web/app.js | 1000 | if | if (activeLabel) { |
| web/app.js | 1004 | if | if (metrics.storageBefore) { |
| web/app.js | 1009 | if | if (metrics.storageAfter) { |
| web/app.js | 1015 | if | if (metrics.heapSamples.length > 0) { |
| web/app.js | 1053 | if | if (isRunning) { |
| web/app.js | 1086 | if | if (!result \\|\\| result.status !== "ok") { |
| web/app.js | 1092 | if | if (!stored) { |
| web/app.js | 1111 | if | if (selectedGuessCache && selectedGuessCache.resultId === result.id) { |
| web/app.js | 1189 | ternary | ? new Blob([visualizedBytes], { type: "application/pdf" }) |
| web/app.js | 1196 | ternary | (visualizedBlob ? visualizedBlob.size : 0); |
| web/app.js | 1206 | ternary | visualized: visualizedBlob ? `${baseName}.visualized.pdf` : null, |
| web/app.js | 1239 | if | if (!wasmReady \\|\\| isRunning) { |
| web/app.js | 1244 | if | if (files.length === 0) { |
| web/app.js | 1273 | if | if (initialHeap) { |
| web/app.js | 1291 | if | if (resultMeta.status === "ok") { |
| web/app.js | 1297 | if | if (heapSample) { |
| web/app.js | 1308 | if | if (finalHeap) { |
| web/app.js | 1316 | if | if (lastSuccess) { |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 30 | ternary | const normalized = clean === "/" ? "/index.html" : clean; |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 32 | if | if (!filePath.startsWith(webRoot)) { |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 42 | if | if (!resolved) { |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 56 | if | if (error && error.code === "ENOENT") { |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 71 | if | if (!address \\|\\| typeof address === "string") { |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 89 | if | if (!exists) { |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 102 | if | if (!serverHandle) { |
| web/pkg/unredact.js | 14 | if | if (r2) { |
| web/pkg/unredact.js | 43 | ternary | const ret = typeof(v) === 'boolean' ? v : undefined; |
| web/pkg/unredact.js | 44 | ternary | return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0; |
| web/pkg/unredact.js | 76 | ternary | const ret = typeof(obj) === 'number' ? obj : undefined; |
| web/pkg/unredact.js | 77 | ternary | getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true); |
| web/pkg/unredact.js | 82 | ternary | const ret = typeof(obj) === 'string' ? obj : undefined; |
| web/pkg/unredact.js | 83 | ternary | var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2); |
| web/pkg/unredact.js | 213 | if | if (heap_next === heap.length) heap.push(heap.length + 1); |
| web/pkg/unredact.js | 224 | if | if (type == 'number' \\|\\| type == 'boolean' \\|\\| val == null) { |
| web/pkg/unredact.js | 227 | if | if (type == 'string') { |
| web/pkg/unredact.js | 230 | if | if (type == 'symbol') { |
| web/pkg/unredact.js | 232 | if | if (description == null) { |
| web/pkg/unredact.js | 238 | if | if (type == 'function') { |
| web/pkg/unredact.js | 240 | if | if (typeof name == 'string' && name.length > 0) { |
| web/pkg/unredact.js | 247 | if | if (Array.isArray(val)) { |
| web/pkg/unredact.js | 250 | if | if (length > 0) { |
| web/pkg/unredact.js | 262 | if | if (builtInMatches && builtInMatches.length > 1) { |
| web/pkg/unredact.js | 268 | if | if (className == 'Object') { |
| web/pkg/unredact.js | 279 | if | if (val instanceof Error) { |
| web/pkg/unredact.js | 287 | if | if (idx < 132) return; |
| web/pkg/unredact.js | 299 | if | if (cachedDataViewMemory0 === null \\|\\| cachedDataViewMemory0.buffer.detached === true \\|\\| (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) { |
| web/pkg/unredact.js | 312 | if | if (cachedUint8ArrayMemory0 === null \\|\\| cachedUint8ArrayMemory0.byteLength === 0) { |
| web/pkg/unredact.js | 338 | if | if (realloc === undefined) { |
| web/pkg/unredact.js | 355 | if | if (code > 0x7F) break; |
| web/pkg/unredact.js | 358 | if | if (offset !== len) { |
| web/pkg/unredact.js | 359 | if | if (offset !== 0) { |
| web/pkg/unredact.js | 386 | if | if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) { |
| web/pkg/unredact.js | 396 | if | if (!('encodeInto' in cachedTextEncoder)) { |
| web/pkg/unredact.js | 419 | if | if (typeof Response === 'function' && module instanceof Response) { |
| web/pkg/unredact.js | 420 | if | if (typeof WebAssembly.instantiateStreaming === 'function') { |
| web/pkg/unredact.js | 426 | if | if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') { |
| web/pkg/unredact.js | 438 | if | if (instance instanceof WebAssembly.Instance) { |
| web/pkg/unredact.js | 446 | switch | switch (type) { |
| web/pkg/unredact.js | 454 | if | if (wasm !== undefined) return wasm; |
| web/pkg/unredact.js | 457 | if | if (module !== undefined) { |
| web/pkg/unredact.js | 458 | if | if (Object.getPrototypeOf(module) === Object.prototype) { |
| web/pkg/unredact.js | 466 | if | if (!(module instanceof WebAssembly.Module)) { |
| web/pkg/unredact.js | 474 | if | if (wasm !== undefined) return wasm; |
| web/pkg/unredact.js | 477 | if | if (module_or_path !== undefined) { |
| web/pkg/unredact.js | 478 | if | if (Object.getPrototypeOf(module_or_path) === Object.prototype) { |
| web/pkg/unredact.js | 485 | if | if (module_or_path === undefined) { |
| web/pkg/unredact.js | 490 | if | if (typeof module_or_path === 'string' \\|\\| (typeof Request === 'function' && module_or_path instanceof Request) \\|\\| (typeof URL === 'function' && module_or_path instanceof URL)) { |

