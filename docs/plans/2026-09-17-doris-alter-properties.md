# Doris ALTER TABLE Implementation Plan

**Goal:** Parse and round-trip the seven reported Doris statements in four independent commits.

**Architecture:** Gate each syntax through the Dialect trait, enabled for Doris and Generic. Preserve structured properties and operation data in the AST and its SQL formatter.

**Tech Stack:** Rust, cargo tests, honggfuzz.

1. Trailing PROPERTIES: extend AlterTable in src/ast/ddl.rs, its constructors and span, parse_alter_table in src/parser/mod.rs, and dialect capabilities. Add positive, malformed-input and disabled-dialect tests at the end of tests/sqlparser_doris.rs. Include Doris in the fuzz target.
2. ENABLE FEATURE: add an AlterTableOperation variant, formatter and span, gate its parser branch, and test optional WITH PROPERTIES and errors.
3. MODIFY ENGINE TO: add a structured operation with optional properties, handle it before MODIFY COLUMN, and test missing engine and malformed properties.
4. MODIFY PARTITION: represent single, multiple and wildcard partition selectors, require SET properties, and test all reported forms plus malformed selectors.

For each step: run the new test before implementation to confirm failure, implement, run cargo test --all-features, cargo fmt --all, cargo clippy --all-targets --all-features -- -D warnings, run bounded fuzzing, inspect the changed code for unnecessary complexity, then commit only that fix and its tests. Do not create a PR.

Official syntax: https://doris.apache.org/zh-CN/docs/3.x/sql-manual/sql-statements/table-and-view/table/ALTER-TABLE-PROPERTY/
