# Deep Simplification Analysis Data Pack

Scope: src/, tests/, web/

Detailed inventories:
- docs/generated/deep_analysis/flags_inventory.md
- docs/generated/deep_analysis/conditions_inventory.md
- docs/generated/deep_analysis/interfaces_inventory.md
- docs/generated/deep_analysis/pseudocode_inventory.md
- docs/generated/deep_analysis/call_patterns_inventory.md

## 1) All Flags Used Across the Package (Compile + Runtime)
- Compile features found: 5
- cfg mentions found: 31
- Clap-defined flags found: 17
- Runtime --flag mentions found: 24
- Environment variable mentions found: 4

## 2) All Conditions Across the Package
- Total captured conditional lines: 913
- Rust condition lines: 782
- JS condition lines: 131

## 3) Interfaces Between Files
- Rust public export lines captured: 214
- Rust cross-file import lines captured: 101
- JS function interface lines captured: 79
- HTML UI IDs captured: 19

## 4) Pseudocode of Each File
- Files covered: 62

## 5) Call Patterns Between Files
- Rust import-derived call edges captured: 123
- Layer-to-layer edge pairs captured: 23

## Method Notes
- Conditions are captured via lexical scan (if/else if/match/while/switch/ternary/inline if/match guards).
- Call patterns are derived from import edges and represent coupling agreements.
- Existing architecture narrative remains in docs/service_architecture_and_pseudocode.md.

