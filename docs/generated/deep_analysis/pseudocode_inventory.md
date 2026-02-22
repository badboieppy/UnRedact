# Pseudocode Inventory (Per File)

This section is mechanically generated to cover every file in scope (src/, tests/, web/).

### src/bin/guess_accuracy_benchmark.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call parse_options(...) in this file flow.
4. Call benchmark_config(...) in this file flow.
5. Call load_report(...) in this file flow.
6. Call run_report(...) in this file flow.
7. Call ordered_guess_texts_upper(...) in this file flow.
8. Call top1_guess_text(...) in this file flow.
9. Call top5_guess_texts(...) in this file flow.
10. Call rank_in_guess(...) in this file flow.
11. Call best_rank_in_guesses(...) in this file flow.
12. Call summarize_ranks(...) in this file flow.
13. Additional helper functions omitted (45 total).

### src/bin/pdf_to_png.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call main(...) in this file flow.
4. Call run(...) in this file flow.
5. Call write_png(...) in this file flow.
6. Call default_output_dir(...) in this file flow.
7. Call default_single_page_path(...) in this file flow.
8. Call default_all_pages_path(...) in this file flow.
9. Call file_stem_or_default(...) in this file flow.
10. Call single_page_default_path_uses_plain_png_for_page_one(...) in this file flow.
11. Call single_page_default_path_includes_page_for_non_first_page(...) in this file flow.
12. Call all_pages_default_path_is_zero_padded(...) in this file flow.

### src/bin/visual_score_impact_benchmark.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call next_u64(...) in this file flow.
5. Call main(...) in this file flow.
6. Call run(...) in this file flow.
7. Call collect_hits_by_page(...) in this file flow.
8. Call canonical_word(...) in this file flow.
9. Call extract_word_hits(...) in this file flow.
10. Call tokenize_word_ranges(...) in this file flow.
11. Call center_y(...) in this file flow.
12. Call build_target_candidates(...) in this file flow.
13. Additional helper functions omitted (21 total).

### src/data/default_name_dictionary.rs
Purpose: Define constants/types/contracts used by other files.
Pseudo:
1. Define domain types and defaults.
2. Provide serde/utility behavior where needed.

### src/data/dictionary_data.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call load_dictionary_from_bytes(...) in this file flow.
5. Call default(...) in this file flow.
6. Call normalize_dictionary(...) in this file flow.
7. Call parse_dictionary_line(...) in this file flow.
8. Call build_default_name_dictionary(...) in this file flow.
9. Call normalize_whitespace(...) in this file flow.
10. Call load_dictionary_from_bytes_dedupes_and_parses_pipe_format(...) in this file flow.
11. Call missing_dictionary_bytes_falls_back_to_default_names(...) in this file flow.

### src/data/fonts_data.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call detect_fonts_from_bytes(...) in this file flow.
5. Call load_font_runs_from_bytes(...) in this file flow.
6. Call default(...) in this file flow.
7. Call finalize_file_font_report(...) in this file flow.
8. Call detect_fonts_from_bytes_hides_occurrences_when_details_are_disabled(...) in this file flow.

### src/data/guess_validation_data.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call load_reports(...) in this file flow.
4. Call new(...) in this file flow.
5. Call write_guesses(...) in this file flow.
6. Call default(...) in this file flow.

### src/data/local_file_workflow_data.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call read_bytes(...) in this file flow.
5. Call write_bytes_exact(...) in this file flow.
6. Call create_dir_all(...) in this file flow.
7. Call read_dir_paths(...) in this file flow.
8. Call exists(...) in this file flow.
9. Call is_dir(...) in this file flow.
10. Call default(...) in this file flow.

### src/data/mod.rs
Purpose: Declare module boundaries and re-exports.
Pseudo:
1. Declare submodules.
2. Re-export selected symbols for parent layer consumption.
3. Submodules: default_name_dictionary, dictionary_data, fonts_data, guess_validation_data, local_file_workflow_data, redactions_data, result_data_publisher, visualization_data.

### src/data/redactions_data.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call build_renderer(...) in this file flow.
5. Call default(...) in this file flow.
6. Call page_indices(...) in this file flow.
7. Call annotation_redactions(...) in this file flow.
8. Call drawn_redactions(...) in this file flow.
9. Call raster_redactions(...) in this file flow.
10. Call underlying_text_hits(...) in this file flow.
11. Call new_from_bytes(...) in this file flow.

### src/data/result_data_publisher.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call publish(...) in this file flow.
5. Call publish_bytes(...) in this file flow.
6. Call default(...) in this file flow.

### src/data/visualization_data.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call load_inputs_from_bytes(...) in this file flow.
5. Call render_visualized_pdf_from_bytes(...) in this file flow.
6. Call default(...) in this file flow.
7. Call build_overlays(...) in this file flow.
8. Call push_anchor_pair_overlays(...) in this file flow.
9. Call raster_overlay_layout(...) in this file flow.
10. Call pick_best_guess(...) in this file flow.
11. Call select_run_by_bbox(...) in this file flow.
12. Call vertical_overlap_run(...) in this file flow.
13. Additional helper functions omitted (29 total).

### src/dependency/file_store.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call read(...) in this file flow.
4. Call write_exact(...) in this file flow.
5. Call write(...) in this file flow.
6. Call create_dir_all(...) in this file flow.
7. Call read_dir(...) in this file flow.
8. Call exists(...) in this file flow.
9. Call is_dir(...) in this file flow.
10. Call validate_read_request(...) in this file flow.
11. Call ensure_parent_dir(...) in this file flow.

### src/dependency/hayro_renderer.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new_from_bytes(...) in this file flow.
4. Call is_available(...) in this file flow.
5. Call page_count(...) in this file flow.
6. Call render_page_to_rgba(...) in this file flow.

### src/dependency/mod.rs
Purpose: Declare module boundaries and re-exports.
Pseudo:
1. Declare submodules.
2. Re-export selected symbols for parent layer consumption.
3. Submodules: file_store, hayro_renderer, pdf_annotator, pdf_font_occurrence_accessor, pdf_font_run_accessor, pdf_redaction_accessor.

### src/dependency/pdf_annotator.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call annotate(...) in this file flow.
4. Call add_page_content(...) in this file flow.
5. Call build_text_ops(...) in this file flow.
6. Call build_rect_ops(...) in this file flow.
7. Call escape_pdf_string(...) in this file flow.

### src/dependency/pdf_font_occurrence_accessor.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call build_file_font_report(...) in this file flow.
5. Call build_file_font_report_from_bytes(...) in this file flow.
6. Call extract_occurrences(...) in this file flow.
7. Call extract_pdf_occurrences(...) in this file flow.
8. Call extract_pdf_occurrences_from_bytes(...) in this file flow.
9. Call extract_pdf_page_occurrences(...) in this file flow.
10. Call extract_pdf_page_fonts(...) in this file flow.
11. Call resolve_pdf_font_name(...) in this file flow.
12. Call occurrences_from_ops(...) in this file flow.
13. Additional helper functions omitted (43 total).

### src/dependency/pdf_font_run_accessor.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call default(...) in this file flow.
4. Call build_font_run_report(...) in this file flow.
5. Call build_font_run_report_from_input_name(...) in this file flow.
6. Call extract_text_runs(...) in this file flow.
7. Call extract_page_font_info(...) in this file flow.
8. Call resolve_pdf_font_name(...) in this file flow.
9. Call extract_font_bytes(...) in this file flow.
10. Call parse_show_text_op(...) in this file flow.
11. Call next_line_delta_y(...) in this file flow.
12. Call pdf_spacing_pt(...) in this file flow.
13. Additional helper functions omitted (35 total).

### src/dependency/pdf_redaction_accessor.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call page_indices(...) in this file flow.
4. Call annotation_redactions(...) in this file flow.
5. Call drawn_redactions(...) in this file flow.
6. Call raster_redactions(...) in this file flow.
7. Call underlying_text_hits(...) in this file flow.
8. Call new_from_bytes(...) in this file flow.
9. Call new(...) in this file flow.
10. Call page_id(...) in this file flow.
11. Call extract_annotation_redactions(...) in this file flow.
12. Call extract_page_drawn_redactions(...) in this file flow.
13. Additional helper functions omitted (62 total).

### src/lib.rs
Purpose: Define constants/types/contracts used by other files.
Pseudo:
1. Define domain types and defaults.
2. Provide serde/utility behavior where needed.

### src/logic/dictionary_list_convertion_component.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call run_dictionary_list_convertion_component(...) in this file flow.
4. Call bytes_dictionary_is_converted_to_entries(...) in this file flow.
5. Call missing_dictionary_bytes_uses_default_fallback(...) in this file flow.

### src/logic/file_byte_convertion_component.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call e(...) in this file flow.

### src/logic/local_file_workflow_component.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call read_input_pdf_bytes(...) in this file flow.
4. Call build_output_file_paths(...) in this file flow.
5. Call write_encoded_outputs(...) in this file flow.
6. Call read_dictionary_input(...) in this file flow.
7. Call validate_batch_input_directory(...) in this file flow.
8. Call discover_pdf_inputs(...) in this file flow.
9. Call ensure_batch_output_dir_for_input(...) in this file flow.
10. Call write_batch_manifest(...) in this file flow.
11. Call is_supported_batch_input(...) in this file flow.
12. Call build_output_file_paths_uses_stem_and_dir(...) in this file flow.

### src/logic/mod.rs
Purpose: Declare module boundaries and re-exports.
Pseudo:
1. Declare submodules.
2. Re-export selected symbols for parent layer consumption.
3. Submodules: dictionary_list_convertion_component, file_byte_convertion_component, local_file_workflow_component, redaction_guessing_component, time, types, visualization_render_component.

### src/logic/redaction_guessing_component.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call run_redaction_guessing_component(...) in this file flow.
4. Call run_from_bytes(...) in this file flow.
5. Call build_report_from_parts_with_fonts_inputs(...) in this file flow.
6. Call annotate_guess_confidence(...) in this file flow.
7. Call as_str(...) in this file flow.
8. Call build_anchor_validated_guesses(...) in this file flow.
9. Call apply_cluster_consensus(...) in this file flow.
10. Call is_multi_span_row_guess(...) in this file flow.
11. Call is_two_sided_anchor_context(...) in this file flow.
12. Call apply_row_joint_assignment(...) in this file flow.
13. Additional helper functions omitted (150 total).

### src/logic/time.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call now(...) in this file flow.
4. Call elapsed(...) in this file flow.

### src/logic/types/mod.rs
Purpose: Declare module boundaries and re-exports.
Pseudo:
1. Declare submodules.
2. Re-export selected symbols for parent layer consumption.

### src/logic/visualization_render_component.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call r(...) in this file flow.

### src/main.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call main(...) in this file flow.
4. Call run(...) in this file flow.
5. Call default_output_dir(...) in this file flow.

### src/service/mod.rs
Purpose: Declare module boundaries and re-exports.
Pseudo:
1. Declare submodules.
2. Re-export selected symbols for parent layer consumption.
3. Submodules: unredact_cli_entry, unredact_web_bindings, unredact_web_entry.

### src/service/unredact_cli_entry.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call default(...) in this file flow.
4. Call run_from_paths(...) in this file flow.
5. Call run_batch_from_paths(...) in this file flow.
6. Call run(...) in this file flow.
7. Call run_batch(...) in this file flow.
8. Call run_batch_serial(...) in this file flow.
9. Call run_batch_item(...) in this file flow.
10. Call test_dir(...) in this file flow.
11. Call run_batch_errors_when_directory_has_no_supported_files(...) in this file flow.
12. Call run_batch_recurses_pdf_inputs_and_preserves_relative_output_paths(...) in this file flow.

### src/service/unredact_web_bindings.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call r(...) in this file flow.

### src/service/unredact_web_entry.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call default(...) in this file flow.
4. Call run(...) in this file flow.

### src/types/file_types.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call default_h_scale_pct(...) in this file flow.
5. Call distinct_fonts_from_counts(...) in this file flow.
6. Call aggregate_counts(...) in this file flow.
7. Call output_format_serializes_json(...) in this file flow.
8. Call rect_dimensions(...) in this file flow.
9. Call font_id_ordering_is_deterministic(...) in this file flow.
10. Call distinct_fonts_from_counts_deduplicates_and_sorts(...) in this file flow.
11. Call counts_as_map(...) in this file flow.
12. Call counts_as_map_builds_expected_map(...) in this file flow.
13. Additional helper functions omitted (16 total).

### src/types/guess_types.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call default(...) in this file flow.
4. Call default_visual_score_enabled(...) in this file flow.
5. Call default_visual_score_dpi(...) in this file flow.

### src/types/mod.rs
Purpose: Declare module boundaries and re-exports.
Pseudo:
1. Declare submodules.
2. Re-export selected symbols for parent layer consumption.
3. Submodules: file_types, guess_types, redaction_types, text_overlay, visualizer_config.

### src/types/redaction_types.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call new(...) in this file flow.
4. Call width(...) in this file flow.
5. Call height(...) in this file flow.
6. Call area(...) in this file flow.
7. Call default(...) in this file flow.
8. Call page_count(...) in this file flow.
9. Call render_page_to_rgba(...) in this file flow.

### src/types/text_overlay.rs
Purpose: Define constants/types/contracts used by other files.
Pseudo:
1. Define domain types and defaults.
2. Provide serde/utility behavior where needed.

### src/types/visualizer_config.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call d(...) in this file flow.

### tests/dictionary_entry_format_behavior.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call test_output_dir(...) in this file flow.
4. Call write_dictionary(...) in this file flow.
5. Call load_report(...) in this file flow.
6. Call first_bullet_rows(...) in this file flow.
7. Call ordered_guess_texts_upper(...) in this file flow.
8. Call target_tokens_upper(...) in this file flow.
9. Call contains_all_tokens(...) in this file flow.
10. Call pool_contains_target(...) in this file flow.
11. Call rank_in_guess_by_tokens(...) in this file flow.
12. Call best_rank_in_rows(...) in this file flow.
13. Additional helper functions omitted (11 total).

### tests/efta00038617_guessing.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call test_output_dir(...) in this file flow.
4. Call write_name_dictionary(...) in this file flow.
5. Call load_report(...) in this file flow.
6. Call collect_candidate_text_upper(...) in this file flow.
7. Call ordered_guess_texts_upper(...) in this file flow.
8. Call rank_in_guess(...) in this file flow.
9. Call best_rank_in_rows(...) in this file flow.
10. Call is_multi_span_row(...) in this file flow.
11. Call horizontal_overlap_pt(...) in this file flow.
12. Call efta00038617_page2_served_names_are_present_with_full_name_dictionary(...) in this file flow.

### tests/efta00101126_guessing.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call test_output_dir(...) in this file flow.
4. Call efta00101126_last_two_redactions_include_sarah_kellen_uppercase(...) in this file flow.

### tests/generalization_smoke.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call smoke_output_dir(...) in this file flow.
4. Call load_report(...) in this file flow.
5. Call smoke_cfg(...) in this file flow.
6. Call additional_epstein_files_run_without_file_specific_tuning(...) in this file flow.
7. Call fallback_dictionary_flow_runs_end_to_end(...) in this file flow.

### tests/integration_black_box_boundary.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call i(...) in this file flow.

### tests/raster_api.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call output_dir(...) in this file flow.
4. Call load_redactions(...) in this file flow.
5. Call service_image_analysis_toggle_controls_raster_detection(...) in this file flow.

### tests/shared_workflow_purity.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call shared_workflow_sources_avoid_native_only_dependencies(...) in this file flow.
4. Call web_entry_does_not_reference_local_file_workflow_exports(...) in this file flow.
5. Call strip_cfg_test_modules(...) in this file flow.

### tests/web_entry.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call w(...) in this file flow.

### tests/web_entry_dto_boundary.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call web_request_dto_roundtrips_via_json(...) in this file flow.
4. Call web_output_dto_roundtrips_via_json(...) in this file flow.

### tests/web_ui_batch_benchmark.rs
Purpose: Implement file-local workflow and helper logic.
Pseudo:
1. Initialize constants/types.
2. Execute entry points and helper flow.
3. Call w(...) in this file flow.

### web/app.js
Purpose: Implement browser/runtime behavior and event handling.
Pseudo:
1. Read UI/runtime inputs.
2. Run processing flow and update UI state.
3. Invoke setStatus(...) during workflow.
4. Invoke setPdfPreviewState(...) during workflow.
5. Invoke setBenchmarkSummary(...) during workflow.
6. Invoke clearDownloads(...) during workflow.
7. Invoke setDownloads(...) during workflow.
8. Invoke successfulBatchResults(...) during workflow.
9. Invoke updateBatchZipButtonState(...) during workflow.
10. Invoke asUint8Array(...) during workflow.
11. Invoke normalizeNumber(...) during workflow.
12. Invoke formatBytes(...) during workflow.
13. Invoke formatMs(...) during workflow.
14. Invoke safeZipPath(...) during workflow.
15. Additional helper functions omitted (61 total).

### web/e2e/web_ui_batch_benchmark.spec.mjs
Purpose: Implement browser/runtime behavior and event handling.
Pseudo:
1. Read UI/runtime inputs.
2. Run processing flow and update UI state.
3. Invoke r(...) during workflow.

### web/index.html
Purpose: Define static UI structure and control surface.
Pseudo:
1. Declare UI controls and output sections.
2. Load runtime script entrypoint.

### web/pkg/unredact.js
Purpose: Implement browser/runtime behavior and event handling.
Pseudo:
1. Read UI/runtime inputs.
2. Run processing flow and update UI state.
3. Invoke __wbg_get_imports(...) during workflow.
4. Invoke addHeapObject(...) during workflow.
5. Invoke debugString(...) during workflow.
6. Invoke dropObject(...) during workflow.
7. Invoke getArrayU8FromWasm0(...) during workflow.
8. Invoke getDataViewMemory0(...) during workflow.
9. Invoke getStringFromWasm0(...) during workflow.
10. Invoke getUint8ArrayMemory0(...) during workflow.
11. Invoke getObject(...) during workflow.
12. Invoke handleError(...) during workflow.
13. Invoke isLikeNone(...) during workflow.
14. Invoke passStringToWasm0(...) during workflow.
15. Additional helper functions omitted (17 total).

### web/styles.css
Purpose: Define styling rules for web UI states and layouts.
Pseudo:
1. Declare base styles and component classes.
2. Apply responsive behavior for smaller screens.


