# Spark `read_files` TVF Support Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add regression coverage proving that Spark SQL `INSERT INTO TABLE ... SELECT ... FROM read_files(...)` statements with `=>` named TVF arguments parse and round-trip correctly.

**Architecture:** Reuse the existing generic table-function representation: a function-shaped table source is stored as `TableFactor::Table` with `TableFunctionArgs`, and named arguments use the existing `FunctionArg::Named` plus `FunctionArgOperator::RightArrow` nodes. No Spark-specific parser branch or AST change is expected because the current parser already supports this syntax.

**Tech Stack:** Rust, `sqlparser` AST/parser, integration tests in `tests/sqlparser_spark.rs`, Cargo.

---

### Task 1: Add a Spark `read_files` TVF regression test

**Files:**
- Modify: `tests/sqlparser_spark.rs` at the end of the file

**Step 1: Write the regression test**

Add a test using a representative `INSERT INTO TABLE` statement with a multiline `read_files` source, one positional S3 path argument, and two `=>` named arguments. Use `one_statement_parses_to` because the parser normalizes whitespace around the named-argument operator.

**Step 2: Run the focused test**

Run: `cargo test --test sqlparser_spark test_insert_from_read_files_table_function`

Expected: PASS, confirming the existing parser and AST support the Spark TVF syntax.

### Task 2: Run repository validation

**Files:**
- No additional files

**Step 1: Format the workspace**

Run: `cargo fmt --all`

Expected: Rust formatting completes successfully.

**Step 2: Run all tests**

Run: `cargo test --all-features`

Expected: PASS.

**Step 3: Run Clippy with warnings denied**

Run: `cargo clippy --all-targets --all-features -- -D warnings`

Expected: PASS with no warnings.
