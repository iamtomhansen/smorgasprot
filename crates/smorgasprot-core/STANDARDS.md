# Scientific Coding Standards (`smorgasprot-core`)

This document defines the self-imposed coding, documentation, and reproducibility standards for `smorgasprot-core`. My goal is **scientific legibility**: every module and algorithm should be accurate, reproducible, interpretable, and explainable to a domain scientist. This is my best attempt at bringing *literate programming* (Ziemann, Poulain, & Bora, 2023) to idiomatic Rust.

---

## 1. Module-Level Reader's Guide (`//!`)

Every `.rs` file in `smorgasprot-core` must begin with a `//!` module-level doc comment that serves as a top-to-bottom reader's guide for `cargo doc` and human readers.

### Required Structure

```rust
//! One-to-two sentence summary of what this module does (displayed in parent module listings).
//!
//! ## Summary
//! Plain-English, explanation of the scientific or geometric problem solved here.
//! Introduce the main entry points (e.g., [`function_one`]) and primary data structures
//! (e.g., [`StructOne`]) using intra-doc links, and describe the high-level data flow.
//!
//! ## Considerations
//! - Corner cases that are handled vs. explicitly not handled.
//! - Algorithmic or numerical trade-offs (e.g., why Algorithm B was chosen over Algorithm A).
//! - Physical units, coordinate conventions, and key invariants.
//!
//! ## References
//! - Author, A., Author, B., Author, C. (Year). Title of the work. https://doi.org/d.o.i.
//!
//! ## TODO
//! - Known improvements, optimizations, or future extensions.
```

---

## 2. Function-Level Literate Programming

- **Numbered Algorithmic Blocks**: Inside non-trivial scientific functions, use numbered comments (`// 1. ...`, `// 2. ...`, `// 3. ...`) to narrate *why* each computational step exists and how it transforms the data, even when helper functions perform the underlying work.
- **Docstrings (`///`)**: Document public functions, structs, and fields with their purpose, units (`Å`, `kcal/mol`, `radians`), and any preconditions.

---

## 3. Citations & Bibliography

- **Module-Level `## References`**: List full citations at the bottom of the module's `//!` doc using the format:
  ```text
  Name, A., Name, B., Name, C. (Year). Title. https://doi.org/d.o.i.
  ```
  List up to 3 authors; for 4 or more authors, use `First, A. et al.`.
- **Inline Citations**: In function or struct docstrings, cite the method concisely (e.g., `Shrake & Rupley (1973)`) rather than repeating the full bibliographic entry.
- **Central Bibliography**: All references are also collected in `REFERENCES.md` (maintained via Zotero).

---

## 4. Formatting & `cargo doc` Conventions

- **Line Length**: Wrap doc comments and code at **100 characters** (matching `rustfmt`).
- **Math**: `cargo doc` does not render LaTeX `$...$` without custom HTML headers. Use **Unicode math** (`Å`, `Δ`, `σ`, `π`, `≤`, `≥`, `×`, `·`, `²`) in prose and docstrings for clean readability in both the editor and generated docs.
- **Code Examples (` ``` `)**: Reserve ` ```rust ` blocks for public API usage examples (since `cargo test` compiles them as doctests). Use numbered `// 1. ...` comments for internal algorithmic narration.

---

## 5. Numerical & Reproducibility Hygiene

- **Determinism**: Avoid letting non-deterministic iteration orders (e.g., `HashMap` / `HashSet`) leak into numerical results, mesh topology, or bond ordering.
- **Tolerances & Edge Cases**: Document floating-point tolerances (`eps`) with their physical/numerical rationale rather than leaving bare magic numbers. Explicitly handle or document degenerate inputs (e.g., collinear points, zero-norm vectors, empty selections).
- **Verification**: Pair core algorithms with unit tests that verify physical or geometric invariants (symmetry, conservation, known analytical values, or reference structures).

---

## 6. AI Agent Rule: Documentation Maintenance

Once a module's `## Summary` and `## Considerations` sections are written, **AI agents must not automatically rewrite or churn them on every subsequent edit**. If a code change invalidates, contradicts, or warrants an update to `## Summary` or `## Considerations`, the agent should simply **inform the human scientist** so they can decide what to update.
