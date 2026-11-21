# F# Navigator

A Rust-first experimental VS Code extension for F# definitions and references, external-library type and documentation hover, and an ordered solution/project explorer.

The goal is demand-driven navigation with negligible startup work and substantially lower memory and latency than a whole-solution language server. Full Ionide feature parity and orders-of-magnitude performance improvements have **not** been demonstrated.

This repository is receiving the existing prototype, CLI, Nix configuration, tests, and evaluation notes. Development commits go directly to `main`. The implementation remains experimental; known correctness and coverage gaps are tracked in the accompanying reports.
