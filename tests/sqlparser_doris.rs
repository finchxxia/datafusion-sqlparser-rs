// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

#![warn(clippy::all)]
//! Test SQL syntax specific to Apache Doris.

#[macro_use]
mod test_utils;

use sqlparser::ast::helpers::attached_token::AttachedToken;
use sqlparser::ast::*;
use sqlparser::dialect::{AnsiDialect, Dialect, DorisDialect, GenericDialect};
use sqlparser::tokenizer::Token;
use test_utils::*;

fn doris() -> TestedDialects {
    TestedDialects::new(vec![Box::new(DorisDialect {})])
}

fn doris_and_generic() -> TestedDialects {
    TestedDialects::new(vec![Box::new(DorisDialect {}), Box::new(GenericDialect {})])
}

#[test]
fn doris_identifier_and_string_literal_gates() {
    let dialect = DorisDialect {};
    assert_eq!(dialect.identifier_quote_style("identifier"), Some('`'));
    assert!(dialect.is_delimited_identifier_start('`'));
    assert!(dialect.supports_string_literal_backslash_escape());
    assert!(dialect.ignores_wildcard_escapes());
    assert!(dialect.supports_numeric_prefix());
    assert!(dialect.supports_parenthesized_auto_increment_column_option());
    assert!(dialect.supports_column_aggregation_function_option());
    assert!(dialect.supports_double_quoted_comment_string());
    assert!(dialect.supports_column_on_update_option());
}

#[test]
fn generic_supports_doris_aggregate_column_options_only() {
    let dialect = GenericDialect {};
    assert!(!dialect.supports_parenthesized_auto_increment_column_option());
    assert!(dialect.supports_column_aggregation_function_option());
}

#[test]
fn doris_and_generic_enable_doris_create_table_model_gates() {
    let dialects = doris_and_generic();
    for dialect in dialects.dialects {
        assert!(dialect.supports_create_table_key_model_clause());
        assert!(dialect.supports_create_table_distribution_clause());
        assert!(dialect.supports_create_table_properties_clause());
        assert!(dialect.supports_load_data_infile());
        assert!(dialect.supports_create_routine_load());
    }
    assert!(DorisDialect {}.supports_create_table_model_clause_without_marker());
    assert!(!GenericDialect {}.supports_create_table_model_clause_without_marker());
}

#[test]
fn parse_doris_strings_and_identifiers() {
    doris().verified_stmt(
        r#"SELECT "double quoted string", 'single quoted string', `select` FROM `db`.`table`"#,
    );
}

#[test]
fn doris_and_generic_parse_common_sql_identically() {
    doris_and_generic().verified_stmt("SELECT 1 AS properties FROM t");
}

#[test]
fn parse_doris_auto_increment_column() {
    doris().verified_stmt("CREATE TABLE t (id BIGINT AUTO_INCREMENT(100), name STRING)");
}

#[test]
fn parse_doris_auto_increment_no_start_value() {
    doris().verified_stmt("CREATE TABLE t (id BIGINT AUTO_INCREMENT, name STRING)");
}

#[test]
fn parse_generic_auto_increment_uses_unified_ast() {
    let generic = TestedDialects::new(vec![Box::new(GenericDialect {})]);
    let sql = "CREATE TABLE t (id BIGINT AUTO_INCREMENT)";
    let stmt = generic.verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            assert_eq!(
                columns[0].options[0].option,
                ColumnOption::AutoIncrement(None)
            );
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn ast_doris_auto_increment_with_start() {
    let sql = "CREATE TABLE t (id BIGINT AUTO_INCREMENT(100), name STRING)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            let id_col = &columns[0];
            assert_eq!(id_col.name, Ident::new("id"));
            let auto_inc = id_col
                .options
                .iter()
                .find(|o| matches!(o.option, ColumnOption::AutoIncrement(_)));
            assert!(auto_inc.is_some());
            match &auto_inc.unwrap().option {
                ColumnOption::AutoIncrement(Some(100)) => {}
                other => panic!("Expected AutoIncrement(Some(100)), got {:?}", other),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn ast_doris_auto_increment_without_start() {
    let sql = "CREATE TABLE t (id BIGINT AUTO_INCREMENT, name STRING)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            let id_col = &columns[0];
            let auto_inc = id_col
                .options
                .iter()
                .find(|o| matches!(o.option, ColumnOption::AutoIncrement(_)));
            assert!(auto_inc.is_some());
            match &auto_inc.unwrap().option {
                ColumnOption::AutoIncrement(None) => {}
                other => panic!("Expected AutoIncrement(None), got {:?}", other),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_aggregate_column_options() {
    doris_and_generic()
        .verified_stmt("CREATE TABLE t (k BIGINT, v BIGINT SUM, bitmap_col BITMAP BITMAP_UNION)");
}

#[test]
fn parse_doris_all_aggregate_column_options() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k LARGEINT, v1 BIGINT SUM, v2 BIGINT MAX, v3 BIGINT MIN, v4 BIGINT REPLACE, v5 HLL HLL_UNION, v6 BITMAP BITMAP_UNION, v7 QUANTILESTATE QUANTILE_UNION)",
    );
}

#[test]
fn ast_doris_aggregate_column_option_is_dialect_specific() {
    let sql = "CREATE TABLE t (k BIGINT, v BIGINT SUM)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            let v_col = &columns[1];
            assert_eq!(v_col.name, Ident::new("v"));
            let agg_opt = v_col
                .options
                .iter()
                .find(|o| matches!(o.option, ColumnOption::DialectSpecific(_)));
            assert!(agg_opt.is_some());
            match &agg_opt.unwrap().option {
                ColumnOption::DialectSpecific(tokens) => {
                    assert_eq!(tokens.len(), 1);
                    assert_eq!(tokens[0], Token::make_keyword("SUM"));
                }
                other => panic!("Expected DialectSpecific, got {:?}", other),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_duplicate_key_hash_distribution() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, v STRING) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_unique_key_random_distribution() {
    doris_and_generic()
        .verified_stmt("CREATE TABLE t (k BIGINT, v STRING) UNIQUE KEY(k) DISTRIBUTED BY RANDOM");
}

#[test]
fn parse_doris_buckets_auto() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, v STRING) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS AUTO",
    );
}

#[test]
fn parse_doris_table_properties() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, v STRING) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8 PROPERTIES ('replication_num' = '1')",
    );
}

#[test]
fn parse_doris_engine_before_key_model() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT) ENGINE = OLAP DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_engine_with_comment_and_properties() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, v STRING) ENGINE = OLAP DUPLICATE KEY(k) COMMENT 'my table' DISTRIBUTED BY HASH(k) BUCKETS 8 PROPERTIES ('replication_num' = '1')",
    );
}

#[test]
fn parse_doris_unique_key_order_by() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, c BIGINT) UNIQUE KEY(k) ORDER BY(c) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn ast_doris_key_model_is_structured() {
    let sql =
        "CREATE TABLE t (k BIGINT, v STRING) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    key_model: Some(km),
                    ..
                }),
            ..
        }) => {
            assert_eq!(km.kind, TableKeyModelKind::Duplicate);
            assert_eq!(km.columns, vec![Ident::new("k")]);
        }
        _ => panic!("Expected CreateTable with key_model"),
    }
}

#[test]
fn ast_doris_key_model_order_by() {
    let sql =
        "CREATE TABLE t (k BIGINT, c BIGINT) UNIQUE KEY(k) ORDER BY(c) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    key_model: Some(km),
                    ..
                }),
            ..
        }) => {
            assert_eq!(km.kind, TableKeyModelKind::Unique);
            assert_eq!(km.columns, vec![Ident::new("k")]);
            assert_eq!(km.order_by, Some(vec![OrderByExpr::from(Ident::new("c"))]));
        }
        _ => panic!("Expected CreateTable with key_model"),
    }
}

#[test]
fn ast_doris_distribution_hash_is_structured() {
    let sql =
        "CREATE TABLE t (k BIGINT, v STRING) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    distribution: Some(TableDistribution::Hash { columns, buckets }),
                    ..
                }),
            ..
        }) => {
            assert_eq!(columns, vec![Ident::new("k")]);
            assert_eq!(buckets, Some(BucketCount::Count(8)));
        }
        _ => panic!("Expected CreateTable with Hash distribution"),
    }
}

#[test]
fn ast_doris_engine_comment_properties_are_structured() {
    let sql =
        "CREATE TABLE t (k BIGINT) ENGINE = OLAP COMMENT 'table comment' PROPERTIES ('replication_num' = '1')";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    engine: Some(engine),
                    comment: Some(comment),
                    properties,
                    ..
                }),
            table_options,
            ..
        }) => {
            assert_eq!(engine, Ident::new("OLAP"));
            assert_eq!(comment, "table comment");
            assert_eq!(properties.len(), 1);
            assert_eq!(table_options, CreateTableOptions::None);
        }
        _ => panic!("Expected CreateTable with table_model"),
    }
}

#[test]
fn parse_doris_range_partition() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01')) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_list_partition() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY LIST(dt) (PARTITION p1 VALUES IN (('2024-01-01'), ('2024-01-02'))) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_auto_partition_skeleton() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) AUTO PARTITION BY RANGE(date_trunc(dt, 'day')) DISTRIBUTED BY RANDOM",
    );
}

#[test]
fn parse_doris_partition_values_less_than_maxvalue() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01'), PARTITION pmax VALUES LESS THAN MAXVALUE) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_partition_with_properties() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01') PROPERTIES ('storage_medium' = 'SSD')) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_list_partition_single_values() {
    doris_and_generic().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, city STRING) DUPLICATE KEY(k) PARTITION BY LIST(city) (PARTITION p1 VALUES IN ('Beijing', 'Shanghai')) DISTRIBUTED BY HASH(k) BUCKETS 8",
        "CREATE TABLE t (k BIGINT, city STRING) DUPLICATE KEY(k) PARTITION BY LIST(city) (PARTITION p1 VALUES IN (('Beijing'), ('Shanghai'))) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_multi_column_range_partition() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k1 INT, k2 INT, v INT) DUPLICATE KEY(k1, k2) PARTITION BY RANGE(k1, k2) (PARTITION p1 VALUES LESS THAN ('100', '200')) DISTRIBUTED BY HASH(k1) BUCKETS 8",
    );
}

#[test]
fn parse_doris_auto_partition_by_list_multi_column() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k1 INT, k2 INT, v INT) DUPLICATE KEY(k1, k2) AUTO PARTITION BY LIST(k1, k2) DISTRIBUTED BY HASH(k1) BUCKETS 8",
    );
}

#[test]
fn parse_doris_partition_fixed_range() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES [('2024-01-01'), ('2024-02-01'))) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_partition_batch_range() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_max_value_underscore() {
    doris_and_generic().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION pmax VALUES LESS THAN MAX_VALUE) DISTRIBUTED BY HASH(k) BUCKETS 8",
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION pmax VALUES LESS THAN MAXVALUE) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn ast_doris_partition_range_is_structured() {
    let sql = "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01')) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    partitioning: Some(dp),
                    ..
                }),
            ..
        }) => {
            assert!(!dp.auto);
            assert_eq!(dp.kind, TablePartitioningKind::Range);
            assert_eq!(dp.columns.len(), 1);
            assert_eq!(dp.partitions.len(), 1);
            match &dp.partitions[0] {
                TablePartitioningEntry::Definition(def) => {
                    assert_eq!(def.name, Ident::new("p1"));
                    match &def.values {
                        TablePartitioningValues::LessThan(values) => {
                            assert_eq!(values.len(), 1);
                        }
                        _ => panic!("Expected LessThan partition values"),
                    }
                }
                _ => panic!("Expected Definition entry"),
            }
        }
        _ => panic!("Expected CreateTable with partitioning"),
    }
}

#[test]
fn ast_doris_partition_maxvalue_is_structured() {
    let sql = "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION pmax VALUES LESS THAN MAXVALUE) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    partitioning: Some(dp),
                    ..
                }),
            ..
        }) => {
            assert_eq!(dp.partitions.len(), 1);
            match &dp.partitions[0] {
                TablePartitioningEntry::Definition(def) => {
                    assert_eq!(def.name, Ident::new("pmax"));
                    assert_eq!(def.values, TablePartitioningValues::LessThanMaxValue);
                }
                _ => panic!("Expected Definition entry"),
            }
        }
        _ => panic!("Expected CreateTable with partitioning"),
    }
}

#[test]
fn ast_doris_batch_range_partition() {
    let sql = "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    partitioning: Some(dp),
                    ..
                }),
            ..
        }) => {
            assert_eq!(dp.partitions.len(), 1);
            match &dp.partitions[0] {
                TablePartitioningEntry::BatchRange {
                    from,
                    to,
                    interval_value,
                    interval_unit,
                    properties,
                } => {
                    assert_eq!(from.len(), 1);
                    assert_eq!(to.len(), 1);
                    assert_eq!(*interval_value, 1);
                    assert_eq!(interval_unit.as_ref().unwrap(), &Ident::new("DAY"));
                    assert!(properties.is_empty());
                }
                _ => panic!("Expected BatchRange entry"),
            }
        }
        _ => panic!("Expected CreateTable with partitioning"),
    }
}

#[test]
fn parse_doris_load_data_infile() {
    doris_and_generic().verified_stmt(
        "LOAD DATA LOCAL INFILE 'testData' INTO TABLE testDb.testTbl PROPERTIES ('timeout' = '100')",
    );
}

#[test]
fn parse_doris_load_data_infile_no_local() {
    doris_and_generic().verified_stmt("LOAD DATA INFILE 'testData' INTO TABLE testDb.testTbl");
}

#[test]
fn ast_doris_load_data_is_structured() {
    let sql =
        "LOAD DATA LOCAL INFILE 'testData' INTO TABLE testDb.testTbl PROPERTIES ('timeout' = '100')";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::DorisLoadData {
            local,
            infile,
            table_name,
            properties,
        } => {
            assert!(local);
            assert_eq!(infile, "testData");
            assert_eq!(table_name.to_string(), "testDb.testTbl");
            assert_eq!(properties.len(), 1);
        }
        _ => panic!("Expected DorisLoadData"),
    }
}

#[test]
fn ast_doris_load_data_no_local_no_properties() {
    let sql = "LOAD DATA INFILE 'path/to/file' INTO TABLE db.tbl";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::DorisLoadData {
            local,
            infile,
            table_name,
            properties,
        } => {
            assert!(!local);
            assert_eq!(infile, "path/to/file");
            assert_eq!(table_name.to_string(), "db.tbl");
            assert!(properties.is_empty());
        }
        _ => panic!("Expected DorisLoadData"),
    }
}

#[test]
fn parse_doris_create_routine_load_minimal() {
    doris_and_generic().one_statement_parses_to(
        "CREATE ROUTINE LOAD db.job ON tbl COLUMNS(k1, k2) PROPERTIES ('format' = 'json') FROM KAFKA ('kafka_topic' = 'topic1')",
        "CREATE ROUTINE LOAD db.job ON tbl COLUMNS ( k1 , k2 ) PROPERTIES ('format' = 'json') FROM KAFKA ('kafka_topic' = 'topic1')",
    );
}

#[test]
fn parse_doris_create_routine_load_raw_load_properties_are_canonicalized() {
    doris_and_generic().one_statement_parses_to(
        "CREATE ROUTINE LOAD db.job ON tbl COLUMNS(k1, k2), WHERE k1 > 0 PROPERTIES ('format' = 'json') FROM KAFKA ('kafka_topic' = 'topic1')",
        "CREATE ROUTINE LOAD db.job ON tbl COLUMNS ( k1 , k2 ) , WHERE k1 > 0 PROPERTIES ('format' = 'json') FROM KAFKA ('kafka_topic' = 'topic1')",
    );
}

#[test]
fn parse_doris_create_routine_load_no_load_properties() {
    doris_and_generic().verified_stmt(
        "CREATE ROUTINE LOAD db.job ON tbl PROPERTIES ('format' = 'json') FROM KAFKA ('kafka_topic' = 'topic1')",
    );
}

#[test]
fn parse_doris_create_routine_load_with_comment() {
    doris_and_generic().verified_stmt(
        "CREATE ROUTINE LOAD db.job ON tbl FROM KAFKA ('kafka_topic' = 'topic1') COMMENT 'test load job'",
    );
}

#[test]
fn parse_doris_create_routine_load_minimal_no_table() {
    doris_and_generic()
        .verified_stmt("CREATE ROUTINE LOAD db.job FROM KAFKA ('kafka_topic' = 'topic1')");
}

#[test]
fn ast_doris_create_routine_load_is_structured() {
    let sql = "CREATE ROUTINE LOAD db.job ON tbl PROPERTIES ('format' = 'json') FROM KAFKA ('kafka_topic' = 'topic1') COMMENT 'my job'";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateRoutineLoad {
            name,
            table_name,
            load_properties,
            job_properties,
            data_source,
            data_source_properties,
            comment,
        } => {
            assert_eq!(name.to_string(), "db.job");
            assert_eq!(table_name.unwrap().to_string(), "tbl");
            assert!(load_properties.is_empty());
            assert_eq!(job_properties.len(), 1);
            assert_eq!(data_source, Ident::new("KAFKA"));
            assert_eq!(data_source_properties.len(), 1);
            assert_eq!(comment.unwrap(), "my job");
        }
        _ => panic!("Expected CreateRoutineLoad"),
    }
}

#[test]
fn ast_doris_create_routine_load_no_table_no_comment() {
    let sql = "CREATE ROUTINE LOAD db.job FROM KAFKA ('kafka_topic' = 'topic1')";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateRoutineLoad {
            name,
            table_name,
            load_properties,
            job_properties,
            data_source,
            data_source_properties,
            comment,
        } => {
            assert_eq!(name.to_string(), "db.job");
            assert!(table_name.is_none());
            assert!(load_properties.is_empty());
            assert!(job_properties.is_empty());
            assert_eq!(data_source, Ident::new("KAFKA"));
            assert_eq!(data_source_properties.len(), 1);
            assert!(comment.is_none());
        }
        _ => panic!("Expected CreateRoutineLoad"),
    }
}

#[test]
fn generic_engine_without_model_marker_remains_plain_options() {
    let generic = TestedDialects::new(vec![Box::new(GenericDialect {})]);
    let sql = "CREATE TABLE t (k BIGINT) ENGINE = InnoDB";
    match generic.verified_stmt(sql) {
        Statement::CreateTable(CreateTable {
            table_model,
            table_options,
            ..
        }) => {
            assert!(table_model.is_none());
            assert!(matches!(table_options, CreateTableOptions::Plain(_)));
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn ansi_rejects_doris_key_model() {
    let ansi = TestedDialects::new(vec![Box::new(AnsiDialect {})]);
    let sql =
        "CREATE TABLE t (k BIGINT, v STRING) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8";
    assert!(ansi.parse_sql_statements(sql).is_err());
}

#[test]
fn parse_doris_inline_inverted_index() {
    doris().verified_stmt(
        "CREATE TABLE t (k BIGINT, name STRING, INDEX idx_name (name) USING INVERTED) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_inline_inverted_index_with_comment() {
    doris().verified_stmt(
        "CREATE TABLE t (k BIGINT, name STRING, INDEX idx_name (name) USING INVERTED COMMENT 'inverted index for name') DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_inline_bitmap_index() {
    doris().verified_stmt(
        "CREATE TABLE t (k BIGINT, name STRING, INDEX idx_bm (name) USING BITMAP) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_inline_ngram_bf_index_with_properties() {
    doris().verified_stmt(
        r#"CREATE TABLE t (k BIGINT, name STRING, INDEX idx_ngram (name) USING NGRAM_BF PROPERTIES ("gram_size" = "3", "bf_size" = "256") COMMENT 'ngram') DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8"#,
    );
}

#[test]
fn ast_doris_inline_index_is_structured() {
    let sql = r#"CREATE TABLE t (k BIGINT, name STRING, INDEX idx_ngram (name) USING NGRAM_BF PROPERTIES ("gram_size" = "3") COMMENT 'ngram') DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8"#;
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { constraints, .. }) => {
            assert_eq!(constraints.len(), 1);
            match &constraints[0] {
                TableConstraint::Index(index) => {
                    assert!(!index.display_as_key);
                    assert_eq!(index.name, Some(Ident::new("idx_ngram")));
                    assert_eq!(index.index_type, None);
                    assert_eq!(index.columns.len(), 1);
                    assert_eq!(index.index_options.len(), 3);
                    assert_eq!(
                        index.index_options[0],
                        IndexOption::Using(IndexType::Custom(Ident::new("NGRAM_BF")))
                    );
                    match &index.index_options[1] {
                        IndexOption::Properties(props) => assert_eq!(props.len(), 1),
                        other => panic!("Expected Properties, got {other:?}"),
                    }
                    assert_eq!(
                        index.index_options[2],
                        IndexOption::Comment("ngram".to_string())
                    );
                }
                other => panic!("Expected Index constraint, got {other:?}"),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn ast_doris_inline_inverted_index_type_is_structured() {
    let sql = "CREATE TABLE t (k BIGINT, name STRING, INDEX idx_name (name) USING INVERTED) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { constraints, .. }) => match &constraints[0] {
            TableConstraint::Index(index) => {
                assert_eq!(
                    index.index_options[0],
                    IndexOption::Using(IndexType::Inverted)
                );
            }
            other => panic!("Expected Index constraint, got {other:?}"),
        },
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_array_type() {
    doris().verified_stmt("CREATE TABLE t (a ARRAY<VARCHAR(255)>)");
}

#[test]
fn parse_doris_map_type() {
    doris().verified_stmt("CREATE TABLE t (m MAP<STRING, INT>)");
}

#[test]
fn parse_doris_struct_type() {
    doris().one_statement_parses_to(
        "CREATE TABLE t (s STRUCT<x: INT, y: STRING>)",
        "CREATE TABLE t (s STRUCT<x INT, y STRING>)",
    );
}

#[test]
fn parse_doris_nested_complex_types() {
    doris().verified_stmt("CREATE TABLE t (a ARRAY<MAP<STRING, INT>>)");
}

#[test]
fn ansi_rejects_doris_load_data_infile() {
    let ansi = TestedDialects::new(vec![Box::new(AnsiDialect {})]);
    let sql = "LOAD DATA LOCAL INFILE 'test' INTO TABLE t PROPERTIES ('timeout' = '100')";
    assert!(ansi.parse_sql_statements(sql).is_err());
}

#[test]
fn ansi_rejects_doris_create_routine_load() {
    let ansi = TestedDialects::new(vec![Box::new(AnsiDialect {})]);
    let sql = "CREATE ROUTINE LOAD db.job ON tbl FROM KAFKA ('kafka_topic' = 'topic1')";
    assert!(ansi.parse_sql_statements(sql).is_err());
}

#[test]
fn parse_doris_create_table_with_on_update_timestamp() {
    let stmt = doris().one_statement_parses_to(
        r#"CREATE TABLE `sample_table` (
  `id` bigint NOT NULL COMMENT "primary key",
  `event_time` datetime NOT NULL COMMENT "event time",
  `name` varchar(64) NULL DEFAULT "" COMMENT "display name",
  `_sequence` bigint NULL COMMENT "sequence column",
  `updated_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT "updated at",
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP COMMENT "created at"
) ENGINE=OLAP
UNIQUE KEY(`id`, `event_time`)
AUTO PARTITION BY RANGE (date_trunc(`event_time`, 'month'))()
DISTRIBUTED BY HASH(`id`) BUCKETS 2
PROPERTIES (
"function_column.sequence_col" = "_sequence"
)"#,
        "",
    );

    match stmt {
        Statement::CreateTable(CreateTable {
            columns,
            table_model:
                Some(TableModel {
                    key_model: Some(key_model),
                    partitioning: Some(partitioning),
                    distribution:
                        Some(TableDistribution::Hash {
                            columns: dist_columns,
                            buckets,
                        }),
                    properties,
                    ..
                }),
            ..
        }) => {
            assert_eq!(columns.len(), 6);
            let update_column = columns
                .iter()
                .find(|column| column.name.value == "updated_at")
                .expect("updated_at column");
            assert!(update_column.options.iter().any(|option| {
                matches!(&option.option, ColumnOption::OnUpdate(expr) if expr.to_string() == "CURRENT_TIMESTAMP")
            }));

            assert_eq!(key_model.kind, TableKeyModelKind::Unique);
            assert!(partitioning.auto);
            assert_eq!(partitioning.kind, TablePartitioningKind::Range);
            assert!(partitioning.partitions.is_empty());
            assert_eq!(dist_columns, vec![Ident::with_quote('`', "id")]);
            assert_eq!(buckets, Some(BucketCount::Count(2)));
            assert_eq!(properties.len(), 1);
        }
        _ => panic!("Expected Doris CreateTable with table model"),
    }
}

#[test]
fn parse_doris_date_add_datetime_field_arg() {
    let select =
        doris().verified_only_select("SELECT date_add(HOUR, 1, current_timestamp()) AS _LOADED_AT");
    match &select.projection[0] {
        SelectItem::ExprWithAlias { expr, alias } => {
            assert_eq!(alias.value, "_LOADED_AT");
            let Expr::Function(func) = expr else {
                panic!("expected date_add function");
            };
            match &func.args {
                FunctionArguments::List(list) => {
                    assert_eq!(
                        FunctionArg::Unnamed(FunctionArgExpr::DateTimeField(DateTimeField::Hour)),
                        list.args[0]
                    );
                }
                other => panic!("expected argument list, got {other:?}"),
            }
        }
        other => panic!("expected aliased projection, got {other:?}"),
    }

    doris().one_statement_parses_to(
        "SELECT date_add (hour, 1, current_timestamp()) AS _LOADED_AT",
        "SELECT date_add(HOUR, 1, current_timestamp()) AS _LOADED_AT",
    );
    doris().verified_stmt("SELECT date_add(CURRENT_TIMESTAMP, INTERVAL 0 HOUR)");
    doris().one_statement_parses_to(
        "SELECT datediff(day, current_timestamp(), '2026-08-17')",
        "SELECT datediff(DAY, current_timestamp(), '2026-08-17')",
    );
}

#[test]
fn doris_column_key_option_gate() {
    assert!(DorisDialect {}.supports_column_key_option());
    assert!(!GenericDialect {}.supports_column_key_option());
    assert!(!AnsiDialect {}.supports_column_key_option());
}

#[test]
fn parse_doris_column_key_option() {
    doris().verified_stmt("CREATE TABLE t (k INT KEY, v INT)");
    doris().verified_stmt(
        "CREATE TABLE t (k INT KEY NOT NULL COMMENT 'id', v BIGINT REPLACE_IF_NOT_NULL)",
    );
}

#[test]
fn ast_doris_column_key_option() {
    let sql = "CREATE TABLE t (k INT KEY, v INT)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            let k_col = &columns[0];
            assert_eq!(k_col.name, Ident::new("k"));
            let key_opt = k_col
                .options
                .iter()
                .find(|o| matches!(o.option, ColumnOption::Key));
            assert!(key_opt.is_some());
            assert!(columns[1]
                .options
                .iter()
                .all(|o| !matches!(o.option, ColumnOption::Key)));
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn ast_doris_column_key_with_agg_option() {
    let sql = "CREATE TABLE t (k INT, v INT KEY SUM)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            let v_col = &columns[1];
            assert!(v_col
                .options
                .iter()
                .any(|o| matches!(o.option, ColumnOption::Key)));
            let agg_opt = v_col
                .options
                .iter()
                .find(|o| matches!(o.option, ColumnOption::DialectSpecific(_)));
            match &agg_opt.unwrap().option {
                ColumnOption::DialectSpecific(tokens) => {
                    assert_eq!(tokens.len(), 1);
                    assert_eq!(tokens[0], Token::make_keyword("SUM"));
                }
                other => panic!("Expected DialectSpecific, got {other:?}"),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_full_agg_type_column_options() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, v1 BIGINT REPLACE_IF_NOT_NULL, v2 BIGINT GENERIC)",
    );
}

#[test]
fn ast_doris_replace_if_not_null_and_generic_agg_options() {
    for (sql, keyword) in [
        (
            "CREATE TABLE t (k BIGINT, v BIGINT REPLACE_IF_NOT_NULL)",
            "REPLACE_IF_NOT_NULL",
        ),
        ("CREATE TABLE t (k BIGINT, v BIGINT GENERIC)", "GENERIC"),
    ] {
        let stmt = doris().verified_stmt(sql);
        match stmt {
            Statement::CreateTable(CreateTable { columns, .. }) => {
                match &columns[1].options[0].option {
                    ColumnOption::DialectSpecific(tokens) => {
                        assert_eq!(tokens.len(), 1);
                        assert_eq!(tokens[0], Token::make_keyword(keyword));
                    }
                    other => panic!("Expected DialectSpecific, got {other:?}"),
                }
            }
            _ => panic!("Expected CreateTable"),
        }
    }
}

#[test]
fn ast_doris_generated_column_as_expr() {
    let stmt = doris().one_statement_parses_to(
        "CREATE TABLE t (k INT, w INT AS (k*2))",
        "CREATE TABLE t (k INT, w INT AS (k * 2))",
    );
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            match &columns[1].options[0].option {
                ColumnOption::Generated {
                    generation_expr: Some(expr),
                    generated_keyword: false,
                    ..
                } => {
                    assert_eq!(expr.to_string(), "k * 2");
                }
                other => panic!("Expected Generated, got {other:?}"),
            }
        }
        _ => panic!("Expected CreateTable"),
    }

    doris().verified_stmt("CREATE TABLE t (k INT, w INT GENERATED ALWAYS AS (k * 2))");
}

#[test]
fn ansi_rejects_doris_column_key_option() {
    let ansi = TestedDialects::new(vec![Box::new(AnsiDialect {})]);
    assert!(ansi
        .parse_sql_statements("CREATE TABLE t (k INT KEY)")
        .is_err());
}
#[test]
fn parse_doris_agg_state_type() {
    doris_and_generic().verified_stmt("CREATE TABLE t (v AGG_STATE<sum(INT)>)");
    doris_and_generic()
        .verified_stmt("CREATE TABLE t (v AGG_STATE<group_concat(VARCHAR(20), VARCHAR NOT NULL)>)");
    doris_and_generic().verified_stmt("CREATE TABLE t (v AGG_STATE<sum(INT NULL)>)");
    doris_and_generic().verified_stmt("CREATE TABLE t (v AGG_STATE)");
}

#[test]
fn ast_doris_agg_state_type_is_structured() {
    let sql = "CREATE TABLE t (v AGG_STATE<group_concat(VARCHAR(20), VARCHAR NOT NULL)>)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            assert_eq!(columns[0].name, Ident::new("v"));
            match &columns[0].data_type {
                DataType::AggState {
                    function,
                    arg_types,
                } => {
                    assert_eq!(function, &Some(Ident::new("group_concat")));
                    assert_eq!(arg_types.len(), 2);
                    assert_eq!(arg_types[0].data_type.to_string(), "VARCHAR(20)");
                    assert_eq!(arg_types[0].nullable, None);
                    assert_eq!(arg_types[1].data_type.to_string(), "VARCHAR");
                    assert_eq!(arg_types[1].nullable, Some(false));
                }
                other => panic!("Expected AggState, got {other:?}"),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn ast_doris_bare_agg_state_type() {
    let sql = "CREATE TABLE t (v AGG_STATE)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => match &columns[0].data_type {
            DataType::AggState {
                function,
                arg_types,
            } => {
                assert!(function.is_none());
                assert!(arg_types.is_empty());
            }
            other => panic!("Expected AggState, got {other:?}"),
        },
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_variant_type() {
    doris_and_generic().verified_stmt("CREATE TABLE t (v VARIANT)");
    doris_and_generic().verified_stmt("CREATE TABLE t (v VARIANT<'a': INT, 'b': STRING>)");
    doris_and_generic().verified_stmt("CREATE TABLE t (v VARIANT<MATCH_NAME 'x': INT>)");
    doris_and_generic().verified_stmt("CREATE TABLE t (v VARIANT<'a': INT COMMENT 'column a'>)");
    // MAP<...> angle-bracket syntax and double-quoted strings differ
    // between the doris and generic dialects.
    doris().verified_stmt(
        r#"CREATE TABLE t (v VARIANT<MATCH_NAME_GLOB 'x': MAP<STRING, INT>, PROPERTIES ("k" = "v")>)"#,
    );
    doris().verified_stmt(r#"CREATE TABLE t (v VARIANT<PROPERTIES ("k" = "v")>)"#);
    doris_and_generic().verified_stmt("CREATE TABLE t (v VARIANT<PROPERTIES ('k' = 'v')>)");
}

#[test]
fn ast_doris_variant_type_is_structured() {
    let sql = r#"CREATE TABLE t (v VARIANT<MATCH_NAME_GLOB 'x': MAP<STRING, INT>, PROPERTIES ("k" = "v")>)"#;
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { columns, .. }) => {
            assert_eq!(columns[0].name, Ident::new("v"));
            match &columns[0].data_type {
                DataType::Variant { fields, properties } => {
                    assert_eq!(fields.len(), 1);
                    assert_eq!(
                        fields[0].match_type,
                        Some(VariantSubFieldMatchType::MatchNameGlob)
                    );
                    assert_eq!(fields[0].name, "x");
                    assert_eq!(fields[0].data_type.to_string(), "MAP<STRING, INT>");
                    assert!(fields[0].comment.is_none());
                    assert_eq!(properties.len(), 1);
                }
                other => panic!("Expected Variant, got {other:?}"),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_nested_parameterized_types() {
    doris_and_generic().verified_stmt("CREATE TABLE t (a ARRAY<AGG_STATE<sum(INT)>>)");
    doris_and_generic().verified_stmt("CREATE TABLE t (v VARIANT<'a': ARRAY<INT>>)");
}

#[test]
fn ansi_rejects_doris_parameterized_data_types() {
    let ansi = TestedDialects::new(vec![Box::new(AnsiDialect {})]);
    assert!(ansi
        .parse_sql_statements("CREATE TABLE t (v AGG_STATE<sum(INT)>)")
        .is_err());
    assert!(ansi
        .parse_sql_statements("CREATE TABLE t (v VARIANT<'a': INT>)")
        .is_err());
    // The bare names still parse as custom type names.
    ansi.verified_stmt("CREATE TABLE t (v AGG_STATE)");
    ansi.verified_stmt("CREATE TABLE t (v VARIANT)");
}
#[test]
fn parse_doris_rollup_clause() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, v BIGINT) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8 ROLLUP (r1 (k, v), r2 (k) DUPLICATE KEY (k) PROPERTIES ('a' = 'b'))",
    );
}

#[test]
fn ast_doris_rollup_is_structured() {
    let sql = r#"CREATE TABLE t (k BIGINT, v BIGINT) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8 ROLLUP (r1 (k, v), r2 (k) DUPLICATE KEY (k) PROPERTIES ("a" = "b"))"#;
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    rollups,
                    properties,
                    broker_properties,
                    ..
                }),
            ..
        }) => {
            assert_eq!(rollups.len(), 2);
            assert_eq!(rollups[0].name, Ident::new("r1"));
            assert_eq!(rollups[0].columns, vec![Ident::new("k"), Ident::new("v")]);
            assert!(rollups[0].duplicate_keys.is_none());
            assert!(rollups[0].properties.is_empty());
            assert_eq!(rollups[1].name, Ident::new("r2"));
            assert_eq!(rollups[1].columns, vec![Ident::new("k")]);
            assert_eq!(rollups[1].duplicate_keys, Some(vec![Ident::new("k")]));
            assert_eq!(rollups[1].properties.len(), 1);
            assert!(properties.is_empty());
            assert!(broker_properties.is_empty());
        }
        _ => panic!("Expected CreateTable with rollups"),
    }
}

#[test]
fn parse_doris_rollup_only_table_model_marker() {
    // ROLLUP is an unambiguous table model marker by itself.
    doris_and_generic().verified_stmt("CREATE TABLE t (k INT, v INT) ROLLUP (r1 (k))");
}

#[test]
fn parse_doris_broker_properties() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT) ENGINE = ODBC PROPERTIES ('a' = 'b') BROKER PROPERTIES ('k' = 'v')",
    );
}

#[test]
fn parse_doris_broker_properties_without_properties() {
    doris_and_generic()
        .verified_stmt("CREATE TABLE t (k BIGINT) ENGINE = ODBC BROKER PROPERTIES ('k' = 'v')");
}

#[test]
fn ast_doris_broker_properties_is_structured() {
    let sql = "CREATE TABLE t (k BIGINT) ENGINE = ODBC PROPERTIES ('a' = 'b') BROKER PROPERTIES ('k' = 'v')";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    engine: Some(engine),
                    properties,
                    broker_properties,
                    ..
                }),
            ..
        }) => {
            assert_eq!(engine, Ident::new("ODBC"));
            assert_eq!(properties.len(), 1);
            assert_eq!(broker_properties.len(), 1);
        }
        _ => panic!("Expected CreateTable with broker_properties"),
    }
}

#[test]
fn parse_doris_key_model_order_by_sort_items() {
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k BIGINT, c BIGINT, d BIGINT) UNIQUE KEY(k) ORDER BY(c DESC NULLS LAST, d) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn ast_doris_key_model_order_by_sort_items() {
    let sql =
        "CREATE TABLE t (k BIGINT, c BIGINT, d BIGINT) UNIQUE KEY(k) ORDER BY(c DESC NULLS LAST, d ASC NULLS FIRST) DISTRIBUTED BY HASH(k)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    key_model: Some(km),
                    ..
                }),
            ..
        }) => {
            let order_by = km.order_by.expect("expected ORDER BY items");
            assert_eq!(order_by.len(), 2);
            assert_eq!(order_by[0].expr, Expr::Identifier(Ident::new("c")));
            assert_eq!(order_by[0].options.sort, Some(OrderBySort::Desc));
            assert_eq!(order_by[0].options.nulls_first, Some(false));
            assert_eq!(order_by[1].expr, Expr::Identifier(Ident::new("d")));
            assert_eq!(order_by[1].options.sort, Some(OrderBySort::Asc));
            assert_eq!(order_by[1].options.nulls_first, Some(true));
        }
        _ => panic!("Expected CreateTable with key_model"),
    }
}

#[test]
fn parse_doris_like_with_rollup() {
    doris_and_generic().verified_stmt("CREATE TABLE t2 LIKE t1 WITH ROLLUP (r1, r2)");
}

#[test]
fn parse_doris_like_with_rollup_bare() {
    doris_and_generic().verified_stmt("CREATE TABLE t2 LIKE t1 WITH ROLLUP");
}

#[test]
fn ast_doris_like_with_rollup_is_structured() {
    let sql = "CREATE TABLE t2 LIKE t1 WITH ROLLUP (r1, r2)";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            like: Some(CreateTableLikeKind::Plain(like)),
            ..
        }) => {
            assert_eq!(like.name.to_string(), "t1");
            assert_eq!(
                like.rollup_names,
                Some(vec![Ident::new("r1"), Ident::new("r2")])
            );
        }
        _ => panic!("Expected CreateTable with LIKE"),
    }

    let sql = "CREATE TABLE t2 LIKE t1 WITH ROLLUP";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable {
            like: Some(CreateTableLikeKind::Plain(like)),
            ..
        }) => {
            assert_eq!(like.rollup_names, Some(vec![]));
        }
        _ => panic!("Expected CreateTable with LIKE"),
    }
}

#[test]
fn parse_doris_partition_by_without_kind_keyword() {
    // Doris makes the kind keyword optional; the AST normalizes to RANGE.
    doris_and_generic().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY (dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01')) DISTRIBUTED BY HASH(k) BUCKETS 8",
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01')) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn ast_doris_partition_by_without_kind_is_structured() {
    let stmt = doris().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY (dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01')) DISTRIBUTED BY HASH(k)",
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01')) DISTRIBUTED BY HASH(k)",
    );
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    partitioning: Some(dp),
                    ..
                }),
            ..
        }) => {
            assert!(!dp.auto);
            assert_eq!(dp.kind, TablePartitioningKind::Range);
            assert_eq!(dp.partitions.len(), 1);
        }
        _ => panic!("Expected CreateTable with partitioning"),
    }
}

#[test]
fn generic_partition_by_expression_still_parses() {
    // `PARTITION BY (expr)` without a partition definition list must not be
    // claimed by the Doris table model path.
    let generic = TestedDialects::new(vec![Box::new(GenericDialect {})]);
    match generic.verified_stmt("CREATE TABLE t (a INT) PARTITION BY (a)") {
        Statement::CreateTable(CreateTable {
            table_model,
            partition_by,
            ..
        }) => {
            assert!(table_model.is_none());
            assert!(partition_by.is_some());
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_create_table_trailing_comma() {
    doris().one_statement_parses_to(
        r#"CREATE TABLE t (
  c VARCHAR(117) NOT NULL,
) ENGINE=OLAP DUPLICATE KEY(c)"#,
        "CREATE TABLE t (c VARCHAR(117) NOT NULL) ENGINE = OLAP DUPLICATE KEY(c)",
    );
}

#[test]
fn ansi_rejects_create_table_trailing_comma() {
    let ansi = TestedDialects::new(vec![Box::new(AnsiDialect {})]);
    let sql = "CREATE TABLE t (c VARCHAR(117) NOT NULL,)";
    assert!(ansi.parse_sql_statements(sql).is_err());
}

#[test]
fn parse_doris_inline_index_if_not_exists() {
    doris().verified_stmt(
        "CREATE TABLE t (k BIGINT, name STRING, INDEX IF NOT EXISTS idx_name (name) USING INVERTED) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn ast_doris_inline_index_if_not_exists() {
    let sql = "CREATE TABLE t (k BIGINT, name STRING, INDEX IF NOT EXISTS idx_name (name) USING INVERTED) DUPLICATE KEY(k) DISTRIBUTED BY HASH(k) BUCKETS 8";
    let stmt = doris().verified_stmt(sql);
    match stmt {
        Statement::CreateTable(CreateTable { constraints, .. }) => {
            assert_eq!(constraints.len(), 1);
            match &constraints[0] {
                TableConstraint::Index(index) => {
                    assert!(index.if_not_exists);
                    assert_eq!(index.name, Some(Ident::new("idx_name")));
                    assert_eq!(
                        index.index_options[0],
                        IndexOption::Using(IndexType::Inverted)
                    );
                }
                other => panic!("Expected Index constraint, got {other:?}"),
            }
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn parse_doris_ctas_bare_column_list() {
    doris().verified_stmt("CREATE TABLE t (a, b) AS SELECT 1, 2");
}

#[test]
fn ast_doris_ctas_bare_column_list() {
    let stmt = doris().verified_stmt("CREATE TABLE t (a, b) AS SELECT 1, 2");
    match stmt {
        Statement::CreateTable(CreateTable { columns, query, .. }) => {
            assert_eq!(columns.len(), 2);
            assert!(columns
                .iter()
                .all(|column| column.data_type == DataType::Unspecified));
            assert!(query.is_some());
        }
        _ => panic!("Expected CreateTable"),
    }
}

#[test]
fn ansi_rejects_ctas_bare_column_list() {
    let ansi = TestedDialects::new(vec![Box::new(AnsiDialect {})]);
    let sql = "CREATE TABLE t (a, b) AS SELECT 1, 2";
    assert!(ansi.parse_sql_statements(sql).is_err());
}

#[test]
fn parse_doris_partition_bare_properties() {
    doris().one_statement_parses_to(
        r#"CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01') ("replication_num" = "1")) DISTRIBUTED BY HASH(k) BUCKETS 8"#,
        r#"CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01') PROPERTIES ("replication_num" = "1")) DISTRIBUTED BY HASH(k) BUCKETS 8"#,
    );
    doris_and_generic().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01') ('storage_medium' = 'SSD')) DISTRIBUTED BY HASH(k) BUCKETS 8",
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01') PROPERTIES ('storage_medium' = 'SSD')) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_partition_batch_range_properties() {
    doris().one_statement_parses_to(
        r#"CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY ("k" = "v")) DISTRIBUTED BY HASH(k) BUCKETS 8"#,
        r#"CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY PROPERTIES ("k" = "v")) DISTRIBUTED BY HASH(k) BUCKETS 8"#,
    );
    doris_and_generic().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY ('k' = 'v')) DISTRIBUTED BY HASH(k) BUCKETS 8",
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY PROPERTIES ('k' = 'v')) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn ast_doris_batch_range_partition_properties() {
    let sql = r#"CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY ("k" = "v")) DISTRIBUTED BY HASH(k) BUCKETS 8"#;
    let stmt = doris().one_statement_parses_to(
        sql,
        r#"CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (FROM ('2024-01-01') TO ('2024-02-01') INTERVAL 1 DAY PROPERTIES ("k" = "v")) DISTRIBUTED BY HASH(k) BUCKETS 8"#,
    );
    match stmt {
        Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    partitioning: Some(dp),
                    ..
                }),
            ..
        }) => {
            assert_eq!(dp.partitions.len(), 1);
            match &dp.partitions[0] {
                TablePartitioningEntry::BatchRange {
                    interval_value,
                    interval_unit,
                    properties,
                    ..
                } => {
                    assert_eq!(*interval_value, 1);
                    assert_eq!(interval_unit.as_ref().unwrap(), &Ident::new("DAY"));
                    assert_eq!(properties.len(), 1);
                }
                _ => panic!("Expected BatchRange entry"),
            }
        }
        _ => panic!("Expected CreateTable with partitioning"),
    }
}

#[test]
fn parse_doris_partition_in_without_values_keyword() {
    doris_and_generic().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, city STRING) DUPLICATE KEY(k) PARTITION BY LIST(city) (PARTITION p1 (('a'), ('b'))) DISTRIBUTED BY HASH(k) BUCKETS 8",
        "CREATE TABLE t (k BIGINT, city STRING) DUPLICATE KEY(k) PARTITION BY LIST(city) (PARTITION p1 VALUES IN (('a'), ('b'))) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_partition_bare_name_no_values() {
    doris_and_generic().one_statement_parses_to(
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01'), PARTITION p2) DISTRIBUTED BY HASH(k) BUCKETS 8",
        "CREATE TABLE t (k BIGINT, dt DATE) DUPLICATE KEY(k) PARTITION BY RANGE(dt) (PARTITION p1 VALUES LESS THAN ('2024-01-01'), PARTITION p2 VALUES IN ()) DISTRIBUTED BY HASH(k) BUCKETS 8",
    );
}

#[test]
fn parse_doris_alter_table_properties() {
    doris().one_statement_parses_to(
        r#"ALTER TABLE example_db.my_table
DROP COLUMN col2
PROPERTIES ("bloom_filter_columns"="k1,k2,k3");"#,
        r#"ALTER TABLE example_db.my_table DROP COLUMN col2 PROPERTIES ("bloom_filter_columns" = "k1,k2,k3")"#,
    );
    doris_and_generic()
        .verified_stmt("ALTER TABLE t DROP COLUMN c, ADD COLUMN d INT PROPERTIES ('k' = 'v')");
}

#[test]
fn doris_alter_table_properties_errors() {
    for (sql, expected) in [
        (
            "ALTER TABLE t DROP COLUMN c PROPERTIES",
            "Expected: (, found: EOF",
        ),
        (
            "ALTER TABLE t DROP COLUMN c PROPERTIES ('k')",
            "Expected: =, found: )",
        ),
        (
            "ALTER TABLE t DROP COLUMN c PROPERTIES ('k' = 'v'",
            "Expected: ), found: EOF",
        ),
    ] {
        assert_eq!(
            doris_and_generic()
                .parse_sql_statements(sql)
                .unwrap_err()
                .to_string(),
            format!("sql parser error: {expected}")
        );
    }
    let stmt =
        doris_and_generic().verified_stmt("ALTER TABLE t DROP COLUMN c PROPERTIES ('k' = 'v')");
    let Statement::AlterTable(table) = stmt else {
        panic!("Expected ALTER TABLE")
    };
    assert_eq!(table.properties.len(), 1);
    assert_eq!(table.properties[0].to_string(), "'k' = 'v'");
    assert_eq!(
        TestedDialects::new(vec![Box::new(AnsiDialect {})])
            .parse_sql_statements("ALTER TABLE t DROP COLUMN c PROPERTIES ('k' = 'v')")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: end of statement, found: PROPERTIES"
    );
}

#[test]
fn parse_doris_alter_table_enable_feature() {
    doris().verified_stmt(r#"ALTER TABLE example_db.my_table ENABLE FEATURE "SEQUENCE_LOAD" WITH PROPERTIES ("function_column.sequence_type" = "Date")"#);
    doris_and_generic().verified_stmt("ALTER TABLE t ENABLE FEATURE 'BATCH_DELETE'");
    doris_and_generic().verified_stmt("ALTER TABLE t ENABLE FEATURE 'SEQUENCE_LOAD' WITH PROPERTIES ('function_column.sequence_type' = 'Date')");
}

#[test]
fn doris_alter_table_enable_feature_ast_and_errors() {
    let dialects = all_dialects_where(|d| d.supports_alter_table_enable_feature());
    let stmt = dialects.verified_stmt("ALTER TABLE t ENABLE FEATURE 'SEQUENCE_LOAD' WITH PROPERTIES ('function_column.sequence_type' = 'Date')");
    let Statement::AlterTable(table) = stmt else {
        panic!("Expected ALTER TABLE")
    };
    let AlterTableOperation::EnableFeature { name, properties } = &table.operations[0] else {
        panic!("Expected ENABLE FEATURE")
    };
    assert_eq!(
        name.value,
        Value::SingleQuotedString("SEQUENCE_LOAD".into())
    );
    assert_eq!(properties.len(), 1);
    assert_eq!(
        properties[0].to_string(),
        "'function_column.sequence_type' = 'Date'"
    );
    assert!(table.properties.is_empty());
    for (sql, expected) in [
        (
            "ALTER TABLE t ENABLE FEATURE",
            "Expected: a value, found: EOF",
        ),
        (
            "ALTER TABLE t ENABLE FEATURE 1",
            "Expected: quoted feature name, found: 1",
        ),
        (
            "ALTER TABLE t ENABLE FEATURE 'x' WITH",
            "Expected: PROPERTIES, found: EOF",
        ),
        (
            "ALTER TABLE t ENABLE FEATURE 'x' WITH PROPERTIES",
            "Expected: (, found: EOF",
        ),
        (
            "ALTER TABLE t ENABLE FEATURE 'x' WITH PROPERTIES ('k')",
            "Expected: =, found: )",
        ),
    ] {
        assert_eq!(
            dialects.parse_sql_statements(sql).unwrap_err().to_string(),
            format!("sql parser error: {expected}")
        );
    }
    assert_eq!(all_dialects_where(|d| !d.supports_alter_table_enable_feature()).parse_sql_statements("ALTER TABLE t ENABLE FEATURE 'BATCH_DELETE'").unwrap_err().to_string(), "sql parser error: Expected: ALWAYS, REPLICA, ROW LEVEL SECURITY, RULE, or TRIGGER after ENABLE, found: FEATURE");
}

#[test]
fn parse_doris_alter_table_modify_engine() {
    doris().one_statement_parses_to(
        r#"ALTER TABLE example_db.mysql_table MODIFY ENGINE TO odbc PROPERTIES("driver" = "MySQL");"#,
        r#"ALTER TABLE example_db.mysql_table MODIFY ENGINE TO odbc PROPERTIES ("driver" = "MySQL")"#,
    );
    doris_and_generic().verified_stmt("ALTER TABLE t MODIFY ENGINE TO odbc");
    doris_and_generic()
        .verified_stmt("ALTER TABLE t MODIFY ENGINE TO odbc PROPERTIES ('driver' = 'MySQL')");
}

#[test]
fn doris_alter_table_modify_engine_ast_and_errors() {
    let dialects = all_dialects_where(|d| d.supports_alter_table_modify_engine());
    let stmt = dialects.verified_stmt(
        "ALTER TABLE t MODIFY ENGINE TO odbc PROPERTIES ('driver' = 'MySQL', 'charset' = 'utf8')",
    );
    let Statement::AlterTable(table) = stmt else {
        panic!("Expected ALTER TABLE")
    };
    let AlterTableOperation::ModifyEngine { engine, properties } = &table.operations[0] else {
        panic!("Expected MODIFY ENGINE")
    };
    assert_eq!(engine, &Ident::new("odbc"));
    assert_eq!(properties.len(), 2);
    assert_eq!(properties[0].to_string(), "'driver' = 'MySQL'");
    assert!(table.properties.is_empty());
    dialects.verified_stmt("ALTER TABLE t MODIFY COLUMN engine INT");
    dialects.one_statement_parses_to(
        "ALTER TABLE t MODIFY engine INT",
        "ALTER TABLE t MODIFY COLUMN engine INT",
    );
    dialects.verified_stmt("ALTER TABLE t MODIFY COLUMN c INT");
    for (sql, expected) in [
        (
            "ALTER TABLE t MODIFY ENGINE",
            "Expected: a data type name, found: EOF",
        ),
        (
            "ALTER TABLE t MODIFY ENGINE TO",
            "Expected: identifier, found: EOF",
        ),
        (
            "ALTER TABLE t MODIFY ENGINE TO odbc PROPERTIES",
            "Expected: (, found: EOF",
        ),
        (
            "ALTER TABLE t MODIFY ENGINE TO odbc PROPERTIES ('driver')",
            "Expected: =, found: )",
        ),
        (
            "ALTER TABLE t MODIFY ENGINE TO odbc PROPERTIES ('driver' = 'MySQL'",
            "Expected: ), found: EOF",
        ),
    ] {
        assert_eq!(
            dialects.parse_sql_statements(sql).unwrap_err().to_string(),
            format!("sql parser error: {expected}")
        );
    }
    assert_eq!(
        all_dialects_where(|d| !d.supports_alter_table_modify_engine())
            .parse_sql_statements("ALTER TABLE t MODIFY ENGINE TO odbc")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: end of statement, found: odbc"
    );
}

#[test]
fn parse_doris_alter_table_modify_partition() {
    for sql in [
        r#"ALTER TABLE create_table_partition MODIFY PARTITION (*) SET("storage_policy"="created_create_table_partition_alter_policy");"#,
        r#"ALTER TABLE example_db.my_table
MODIFY PARTITION p1 SET("replication_num"="1");"#,
        r#"ALTER TABLE example_db.my_table
MODIFY PARTITION (p1, p2, p4) SET("replication_num"="1");"#,
        r#"ALTER TABLE example_db.my_table
MODIFY PARTITION (*) SET("storage_medium"="HDD");"#,
    ] {
        let expected = sql
            .replace('\n', " ")
            .replace("SET(", "SET (")
            .replace("\"=\"", "\" = \"");
        doris().one_statement_parses_to(sql, expected.trim_end_matches(';'));
    }
    for selector in ["p1", "(p1)", "(p1, p2, p4)", "(*)", "`partition-name`"] {
        doris_and_generic().verified_stmt(&format!(
            "ALTER TABLE t MODIFY PARTITION {selector} SET ('replication_num' = '1')"
        ));
    }
}

#[test]
fn doris_alter_table_modify_partition_ast_and_errors() {
    let dialects = all_dialects_where(|d| d.supports_alter_table_modify_partition());
    for (selector, expected) in [
        ("p1", Partition::Expr(Expr::Identifier(Ident::new("p1")))),
        (
            "(p1, p2)",
            Partition::Partitions(vec![
                Expr::Identifier(Ident::new("p1")),
                Expr::Identifier(Ident::new("p2")),
            ]),
        ),
        (
            "(*)",
            Partition::Partitions(vec![Expr::Wildcard(AttachedToken::empty())]),
        ),
    ] {
        let stmt = dialects.verified_stmt(&format!("ALTER TABLE t MODIFY PARTITION {selector} SET ('replication_num' = '1', 'storage_medium' = 'HDD')"));
        let Statement::AlterTable(table) = stmt else {
            panic!("Expected ALTER TABLE")
        };
        let AlterTableOperation::ModifyPartition {
            partition,
            properties,
        } = &table.operations[0]
        else {
            panic!("Expected MODIFY PARTITION")
        };
        assert_eq!(partition, &expected);
        assert_eq!(properties.len(), 2);
        assert_eq!(properties[0].to_string(), "'replication_num' = '1'");
        assert!(table.properties.is_empty());
    }
    dialects.verified_stmt(
        "ALTER TABLE t MODIFY PARTITION p1 SET ('k' = 'v'), MODIFY PARTITION p2 SET ('k' = 'w')",
    );
    dialects.verified_stmt("ALTER TABLE t MODIFY COLUMN partition INT");
    dialects.one_statement_parses_to(
        "ALTER TABLE t MODIFY partition INT",
        "ALTER TABLE t MODIFY COLUMN partition INT",
    );
    for (sql, expected) in [
        (
            "ALTER TABLE t MODIFY PARTITION () SET ('k' = 'v')",
            "Expected: identifier, found: )",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION (*, p1) SET ('k' = 'v')",
            "Expected: ), found: ,",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION (p1, *) SET ('k' = 'v')",
            "Expected: identifier, found: *",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION (p1,) SET ('k' = 'v')",
            "Expected: identifier, found: )",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION * SET ('k' = 'v')",
            "Expected: identifier, found: *",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION (p1)",
            "Expected: SET, found: EOF",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION (p1 + 1) SET ('k' = 'v')",
            "Expected: ), found: +",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION p1 SET",
            "Expected: (, found: EOF",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION p1 SET ('k')",
            "Expected: =, found: )",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION p1 SET ()",
            "Expected: identifier, found: )",
        ),
        (
            "ALTER TABLE t MODIFY PARTITION p1 SET ('k' = 'v'",
            "Expected: ), found: EOF",
        ),
    ] {
        assert_eq!(
            dialects.parse_sql_statements(sql).unwrap_err().to_string(),
            format!("sql parser error: {expected}")
        );
    }
    assert_eq!(
        all_dialects_where(|d| !d.supports_alter_table_modify_partition())
            .parse_sql_statements("ALTER TABLE t MODIFY PARTITION (*) SET ('k' = 'v')")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: a data type name, found: ("
    );
}

#[test]
fn parse_doris_alter_table_add_columns() {
    doris().one_statement_parses_to(
        r#"ALTER TABLE bps_cdp.user_tag_wide
ADD COLUMN (
    total_game_cnt BIGINT COMMENT '总游戏场次',
    real_cash_game_cnt BIGINT COMMENT '真金游戏场次',
    win_game_cnt BIGINT COMMENT '游戏赢局数',
    real_cash_win_cnt BIGINT COMMENT '真金游戏赢局数'
);"#,
        "ALTER TABLE bps_cdp.user_tag_wide ADD COLUMN (total_game_cnt BIGINT COMMENT '总游戏场次', real_cash_game_cnt BIGINT COMMENT '真金游戏场次', win_game_cnt BIGINT COMMENT '游戏赢局数', real_cash_win_cnt BIGINT COMMENT '真金游戏赢局数')",
    );
    doris_and_generic().verified_stmt("ALTER TABLE t ADD COLUMN (a BIGINT, b INT, c VARCHAR(10))");
    doris_and_generic().verified_stmt("ALTER TABLE t ADD (a BIGINT, b INT)");
    doris().verified_stmt("ALTER TABLE t ADD COLUMN (a BIGINT KEY, b BIGINT SUM DEFAULT '0')");
    doris_and_generic()
        .verified_stmt("ALTER TABLE t ADD COLUMN (a BIGINT, b INT) PROPERTIES ('k' = 'v')");
    // A single column in parentheses is equivalent to the unparenthesized form.
    doris_and_generic().one_statement_parses_to(
        "ALTER TABLE t ADD COLUMN (a INT)",
        "ALTER TABLE t ADD COLUMN a INT",
    );
}

#[test]
fn doris_alter_table_add_columns_ast_and_errors() {
    let dialects = all_dialects_where(|d| d.supports_alter_table_add_column_parenthesized_list());
    let stmt = dialects.verified_stmt("ALTER TABLE t ADD COLUMN (a BIGINT, b INT)");
    let Statement::AlterTable(table) = stmt else {
        panic!("Expected ALTER TABLE")
    };
    let [AlterTableOperation::AddColumn {
        column_keyword,
        if_not_exists,
        column_defs,
        column_position,
    }] = table.operations.as_slice()
    else {
        panic!("Expected a single ADD COLUMN operation")
    };
    assert!(column_keyword);
    assert!(!if_not_exists);
    assert_eq!(&None, column_position);
    assert_eq!(
        vec!["a", "b"],
        column_defs
            .iter()
            .map(|c| c.name.to_string())
            .collect::<Vec<_>>()
    );

    for (sql, expected) in [
        (
            "ALTER TABLE t ADD COLUMN ()",
            "Expected: identifier, found: )",
        ),
        (
            "ALTER TABLE t ADD COLUMN (a INT,)",
            "Expected: identifier, found: )",
        ),
        ("ALTER TABLE t ADD COLUMN (a INT", "Expected: ), found: EOF"),
    ] {
        assert_eq!(
            dialects.parse_sql_statements(sql).unwrap_err().to_string(),
            format!("sql parser error: {expected}")
        );
    }
    assert_eq!(
        all_dialects_where(|d| !d.supports_alter_table_add_column_parenthesized_list())
            .parse_sql_statements("ALTER TABLE t ADD COLUMN (a INT)")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: identifier, found: ("
    );
}

#[test]
fn parse_doris_partition_empty_definition_list() {
    // Doris writes `AUTO PARTITION BY RANGE(...) ()` with an explicit empty
    // definition list; the `()` must round-trip instead of being dropped.
    let with_list = doris_and_generic().verified_stmt(
        "CREATE TABLE t (k DATE, v INT) DUPLICATE KEY(k) AUTO PARTITION BY RANGE(date_trunc(k, 'day')) () DISTRIBUTED BY HASH(k) BUCKETS 3",
    );
    let without_list = doris_and_generic().verified_stmt(
        "CREATE TABLE t (k DATE, v INT) DUPLICATE KEY(k) AUTO PARTITION BY RANGE(date_trunc(k, 'day')) DISTRIBUTED BY HASH(k) BUCKETS 3",
    );

    fn partitioning(stmt: Statement) -> TablePartitioning {
        let Statement::CreateTable(CreateTable {
            table_model:
                Some(TableModel {
                    partitioning: Some(partitioning),
                    ..
                }),
            ..
        }) = stmt
        else {
            panic!("Expected CreateTable with partitioning")
        };
        partitioning
    }

    let with_list = partitioning(with_list);
    assert!(with_list.auto);
    assert!(with_list.has_partition_list);
    assert!(with_list.partitions.is_empty());

    let without_list = partitioning(without_list);
    assert!(without_list.auto);
    assert!(!without_list.has_partition_list);
    assert!(without_list.partitions.is_empty());

    // An explicit empty list is preserved without AUTO as well.
    doris_and_generic().verified_stmt(
        "CREATE TABLE t (k DATE, v INT) PARTITION BY RANGE(k) () DISTRIBUTED BY HASH(k) BUCKETS 3",
    );
}

#[test]
fn parse_doris_create_external_table() {
    doris().verified_stmt(
        r#"CREATE EXTERNAL TABLE t (k INT) ENGINE = MYSQL PROPERTIES ("host" = "127.0.0.1")"#,
    );
    doris().verified_stmt(
        "CREATE EXTERNAL TABLE IF NOT EXISTS db.t (k INT) ENGINE = HIVE COMMENT 'c' PROPERTIES ('k' = 'v')",
    );
    doris().verified_stmt(
        "CREATE EXTERNAL TABLE t (k INT) ENGINE = BROKER BROKER PROPERTIES ('broker_name' = 'hdfs')",
    );
    // Properties alone, and no table model clauses at all.
    doris().verified_stmt("CREATE EXTERNAL TABLE t (k INT) PROPERTIES ('k' = 'v')");
    doris().verified_stmt("CREATE EXTERNAL TABLE t (k INT)");

    let stmt = doris()
        .verified_stmt("CREATE EXTERNAL TABLE t (k INT) ENGINE = MYSQL PROPERTIES ('h' = '1')");
    match stmt {
        Statement::CreateTable(CreateTable {
            external,
            table_model: Some(model),
            ..
        }) => {
            assert!(external);
            assert_eq!(model.engine, Some(Ident::new("MYSQL")));
            assert_eq!(model.properties.len(), 1);
        }
        _ => panic!("Expected CreateTable"),
    }

    // Dialects without the gate still reject a trailing ENGINE clause.
    assert_eq!(
        all_dialects_where(|d| !d.supports_create_external_table_model_clauses())
            .parse_sql_statements("CREATE EXTERNAL TABLE t (k INT) ENGINE = MYSQL")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: end of statement, found: ENGINE"
    );
}

#[test]
fn parse_doris_alter_table_rename() {
    // <https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/ALTER-TABLE-RENAME/>
    let dialects = all_dialects_where(|d| d.supports_alter_table_rename_without_to());
    dialects.verified_stmt("ALTER TABLE table1 RENAME table2");
    dialects.verified_stmt("ALTER TABLE example_table RENAME PARTITION p1 p2");
    dialects.verified_stmt("ALTER TABLE example_table RENAME ROLLUP rollup1 rollup2");
    // The existing `RENAME TO`/`AS`/`COLUMN` forms keep working.
    dialects.verified_stmt("ALTER TABLE t RENAME TO t2");
    dialects.verified_stmt("ALTER TABLE t RENAME AS t2");
    dialects.verified_stmt("ALTER TABLE t RENAME COLUMN c1 TO c2");
}

#[test]
fn doris_alter_table_rename_ast_and_errors() {
    let dialects = all_dialects_where(|d| d.supports_alter_table_rename_without_to());

    let Statement::AlterTable(table) = dialects.verified_stmt("ALTER TABLE t RENAME t2") else {
        panic!("Expected ALTER TABLE")
    };
    assert_eq!(
        table.operations[0],
        AlterTableOperation::RenameTable {
            table_name: RenameTableNameKind::Bare(ObjectName::from(vec![Ident::new("t2")])),
        }
    );

    let Statement::AlterTable(table) =
        dialects.verified_stmt("ALTER TABLE t RENAME PARTITION p1 p2")
    else {
        panic!("Expected ALTER TABLE")
    };
    assert_eq!(
        table.operations[0],
        AlterTableOperation::RenamePartition {
            old_name: Ident::new("p1"),
            new_name: Ident::new("p2"),
        }
    );

    let Statement::AlterTable(table) = dialects.verified_stmt("ALTER TABLE t RENAME ROLLUP r1 r2")
    else {
        panic!("Expected ALTER TABLE")
    };
    assert_eq!(
        table.operations[0],
        AlterTableOperation::RenameRollup {
            old_name: Ident::new("r1"),
            new_name: Ident::new("r2"),
        }
    );

    for (sql, expected) in [
        (
            "ALTER TABLE t RENAME PARTITION",
            "Expected: identifier, found: EOF",
        ),
        (
            "ALTER TABLE t RENAME PARTITION p1",
            "Expected: identifier, found: EOF",
        ),
        (
            "ALTER TABLE t RENAME ROLLUP r1",
            "Expected: identifier, found: EOF",
        ),
    ] {
        assert_eq!(
            dialects.parse_sql_statements(sql).unwrap_err().to_string(),
            format!("sql parser error: {expected}")
        );
    }

    assert_eq!(
        all_dialects_where(|d| !d.supports_alter_table_rename_without_to())
            .parse_sql_statements("ALTER TABLE t RENAME t2")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: TO, found: EOF"
    );
    assert_eq!(
        all_dialects_where(|d| !d.supports_alter_table_rename_without_to())
            .parse_sql_statements("ALTER TABLE t RENAME PARTITION p1 p2")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: TO, found: p1"
    );
}

#[test]
fn parse_doris_alter_table_add_partition() {
    // <https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/table/ALTER-TABLE-PARTITION/>
    let dialects = all_dialects_where(|d| d.supports_alter_table_add_partition());
    // Double-quoted literals are strings in Doris but identifiers elsewhere.
    doris().verified_stmt(
        r#"ALTER TABLE example_db.my_table ADD PARTITION p1 VALUES LESS THAN ("2014-01-01")"#,
    );
    doris().verified_stmt(
        r#"ALTER TABLE example_db.my_table ADD PARTITION p1 VALUES [("2014-01-01"), ("2014-02-01"))"#,
    );
    dialects.verified_stmt(
        "ALTER TABLE example_db.my_table ADD PARTITION p1 VALUES LESS THAN ('2015-01-01') DISTRIBUTED BY HASH(k1) BUCKETS 20",
    );
    dialects.verified_stmt("ALTER TABLE t ADD PARTITION p1 VALUES IN (('Beijing'), ('Shanghai'))");
    dialects.one_statement_parses_to(
        "ALTER TABLE t ADD PARTITION IF NOT EXISTS p1 VALUES LESS THAN (MAXVALUE)",
        "ALTER TABLE t ADD PARTITION IF NOT EXISTS p1 VALUES LESS THAN MAXVALUE",
    );
    dialects.verified_stmt(
        "ALTER TABLE t ADD PARTITION p1 VALUES LESS THAN ('2024-01-01') PROPERTIES ('replication_num' = '1')",
    );
    dialects.verified_stmt("ALTER TABLE t ADD TEMPORARY PARTITION tp1 VALUES LESS THAN ('x')");
    // Bare `("k" = "v")` properties normalize to `PROPERTIES (...)`, matching
    // CREATE TABLE partition definitions.
    doris().one_statement_parses_to(
        r#"ALTER TABLE t ADD PARTITION p1 VALUES LESS THAN ("2015-01-01") ("replication_num"="1")"#,
        r#"ALTER TABLE t ADD PARTITION p1 VALUES LESS THAN ("2015-01-01") PROPERTIES ("replication_num" = "1")"#,
    );
    // A default partition has no VALUES clause.
    dialects.one_statement_parses_to(
        "ALTER TABLE t ADD PARTITION p1",
        "ALTER TABLE t ADD PARTITION p1 VALUES IN ()",
    );
    // The Hive-style `ADD PARTITION (col = val)` path still works.
    dialects.verified_stmt("ALTER TABLE t ADD PARTITION (dt = '2024-01-01')");
}

#[test]
fn doris_alter_table_add_partition_ast_and_errors() {
    let dialects = all_dialects_where(|d| d.supports_alter_table_add_partition());

    let Statement::AlterTable(table) = dialects.verified_stmt(
        "ALTER TABLE t ADD TEMPORARY PARTITION IF NOT EXISTS tp1 VALUES LESS THAN ('x') DISTRIBUTED BY RANDOM BUCKETS 4",
    ) else {
        panic!("Expected ALTER TABLE")
    };
    let AlterTableOperation::AddDorisPartition {
        temporary,
        definition,
        distribution,
    } = &table.operations[0]
    else {
        panic!("Expected ADD PARTITION")
    };
    assert!(temporary);
    assert!(definition.if_not_exists);
    assert_eq!(definition.name, Ident::new("tp1"));
    assert!(matches!(
        definition.values,
        TablePartitioningValues::LessThan(_)
    ));
    assert!(matches!(
        distribution,
        Some(TableDistribution::Random { .. })
    ));

    for (sql, expected) in [
        (
            "ALTER TABLE t ADD PARTITION p1 VALUES LESS THAN",
            "Expected: (, found: EOF",
        ),
        (
            "ALTER TABLE t ADD TEMPORARY",
            "Expected: PARTITION, found: EOF",
        ),
        (
            "ALTER TABLE t ADD PARTITION p1 VALUES LESS THAN ('x') DISTRIBUTED BY",
            "Expected: HASH or RANDOM after DISTRIBUTED BY, found: EOF",
        ),
    ] {
        assert_eq!(
            dialects.parse_sql_statements(sql).unwrap_err().to_string(),
            format!("sql parser error: {expected}")
        );
    }

    assert_eq!(
        all_dialects_where(|d| !d.supports_alter_table_add_partition())
            .parse_sql_statements("ALTER TABLE t ADD PARTITION p1 VALUES LESS THAN ('x')")
            .unwrap_err()
            .to_string(),
        "sql parser error: Expected: (, found: p1"
    );
}

#[test]
fn parse_doris_create_view_comment() {
    // <https://doris.apache.org/docs/sql-manual/sql-statements/table-and-view/view/CREATE-VIEW/>
    let dialects = all_dialects_where(|d| d.supports_create_view_comment_without_eq());
    dialects.verified_stmt("CREATE VIEW v COMMENT 'x' AS SELECT 1");
    dialects.verified_stmt("CREATE VIEW v (c1, c2) COMMENT 'x' AS SELECT 1, 2");
    dialects.verified_stmt("CREATE VIEW v (c1 COMMENT 'a', c2) COMMENT 'x' AS SELECT 1, 2");
    dialects.verified_stmt("CREATE OR REPLACE VIEW IF NOT EXISTS db.v COMMENT 'x' AS SELECT 1");
    // Double-quoted comments normalize to single-quoted on display.
    doris().one_statement_parses_to(
        r#"CREATE OR REPLACE VIEW IF NOT EXISTS db.v COMMENT "x" AS SELECT 1"#,
        "CREATE OR REPLACE VIEW IF NOT EXISTS db.v COMMENT 'x' AS SELECT 1",
    );
}

#[test]
fn doris_create_view_comment_ast_and_errors() {
    let dialects = all_dialects_where(|d| d.supports_create_view_comment_without_eq());

    let Statement::CreateView(view) =
        dialects.verified_stmt("CREATE VIEW v COMMENT 'hello' AS SELECT 1")
    else {
        panic!("Expected CREATE VIEW")
    };
    assert_eq!(
        view.comment,
        Some(CommentDef::WithoutEq("hello".to_string()))
    );

    for (sql, expected) in [
        (
            "CREATE VIEW v COMMENT AS SELECT 1",
            "Expected: string literal, found: AS",
        ),
        (
            "CREATE VIEW v COMMENT = 'x' AS SELECT 1",
            "Expected: string literal, found: =",
        ),
        ("CREATE VIEW v COMMENT 'x'", "Expected: AS, found: EOF"),
    ] {
        assert_eq!(
            dialects.parse_sql_statements(sql).unwrap_err().to_string(),
            format!("sql parser error: {expected}")
        );
    }

    assert_eq!(
        all_dialects_where(|d| !d.supports_create_view_comment_without_eq()
            && !d.supports_create_view_comment_syntax())
        .parse_sql_statements("CREATE VIEW v COMMENT 'x' AS SELECT 1")
        .unwrap_err()
        .to_string(),
        "sql parser error: Expected: AS, found: COMMENT"
    );
}

#[test]
fn parse_insert_with_label() {
    let dialects = all_dialects_where(|d| d.supports_insert_with_label());

    dialects.verified_stmt("INSERT INTO t WITH LABEL l SELECT * FROM t2");
    dialects.verified_stmt("INSERT INTO t WITH LABEL l (c1, c2) SELECT * FROM t2");
    dialects.verified_stmt("INSERT OVERWRITE TABLE t WITH LABEL `l` (c1) SELECT * FROM t2");
    dialects.verified_stmt("INSERT INTO t PARTITION (p1, p2) WITH LABEL `l` SELECT * FROM t2");
    dialects.verified_stmt("INSERT INTO t WITH LABEL l (c1) VALUES (1)");
    // A column list before the label is normalized to after it.
    dialects.one_statement_parses_to(
        "INSERT INTO t (c1) WITH LABEL l SELECT * FROM t2",
        "INSERT INTO t WITH LABEL l (c1) SELECT * FROM t2",
    );
    // A `WITH` not followed by `LABEL` still starts a CTE in the source query.
    dialects.verified_stmt("INSERT INTO t WITH cte AS (SELECT 1) SELECT * FROM cte");

    let Statement::Insert(Insert { label, columns, .. }) =
        dialects.verified_stmt("INSERT INTO t WITH LABEL l (c1) SELECT * FROM t2")
    else {
        panic!("expected INSERT")
    };
    assert_eq!(label, Some(Ident::new("l")));
    assert_eq!(columns, vec![ObjectName::from(Ident::new("c1"))]);

    assert!(dialects
        .parse_sql_statements("INSERT INTO t WITH LABEL SELECT * FROM t2")
        .is_err());
    assert!(TestedDialects::new(vec![Box::new(GenericDialect {})])
        .parse_sql_statements("INSERT INTO t WITH LABEL l SELECT * FROM t2")
        .is_err());
}

#[test]
fn parse_doris_broker_load() {
    let dialects = all_dialects_where(|d| d.supports_broker_load());

    dialects.verified_stmt(
        "LOAD LABEL db.label1 (DATA INFILE ('s3://bucket/file.csv') INTO TABLE db.tbl)",
    );
    dialects.verified_stmt("LOAD LABEL label1 (DATA INFILE ('a.csv', 'b.csv') INTO TABLE tbl)");
    dialects
        .verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t COLUMNS TERMINATED BY ',')");
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t LINES TERMINATED BY '|')");
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t FORMAT AS 'parquet')");
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t (c1, c2))");
    dialects.verified_stmt(
        "LOAD LABEL l (DATA INFILE ('f') INTO TABLE t COLUMNS TERMINATED BY '|' FORMAT AS 'csv' (c1, c2))",
    );
    dialects.verified_stmt(
        "LOAD LABEL l (DATA INFILE ('f') INTO TABLE t, DATA INFILE ('g') INTO TABLE t2)",
    );
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t) WITH S3 ('k' = 'v')");
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t) WITH HDFS ('k' = 'v')");
    dialects.verified_stmt(
        "LOAD LABEL l (DATA INFILE ('f') INTO TABLE t) WITH BROKER broker1 ('k' = 'v', 'k2' = 'v2')",
    );
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t) PROPERTIES ('k' = 'v')");
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t) COMMENT 'load job'");
    dialects.verified_stmt(
        "LOAD LABEL l (DATA INFILE ('f') INTO TABLE t) WITH S3 ('k' = 'v') PROPERTIES ('p' = 'q') COMMENT 'c'",
    );
    // Doris documentation uses double-quoted strings.
    dialects.one_statement_parses_to(
        r#"LOAD LABEL l (DATA INFILE ("s3://b/f") INTO TABLE t FORMAT AS "parquet")"#,
        "LOAD LABEL l (DATA INFILE ('s3://b/f') INTO TABLE t FORMAT AS 'parquet')",
    );
}

#[test]
fn ast_doris_broker_load_is_structured() {
    let sql = "LOAD LABEL db.l (DATA INFILE ('a', 'b') INTO TABLE db.t FORMAT AS 'csv' (c1, c2)) WITH BROKER b1 ('k' = 'v') PROPERTIES ('p' = '1') COMMENT 'c'";
    let stmt = doris().verified_stmt(sql);
    let Statement::DorisBrokerLoad {
        label,
        data_descs,
        with,
        properties,
        comment,
    } = stmt
    else {
        panic!("expected DorisBrokerLoad")
    };
    assert_eq!(label.to_string(), "db.l");
    assert_eq!(comment.as_deref(), Some("c"));
    assert_eq!(properties.len(), 1);
    assert_eq!(data_descs.len(), 1);
    let desc = &data_descs[0];
    assert_eq!(desc.files, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(desc.into_table.to_string(), "db.t");
    assert_eq!(desc.format_as.as_deref(), Some("csv"));
    assert_eq!(
        desc.column_list,
        Some(vec![Ident::new("c1"), Ident::new("c2")])
    );
    let Some(DorisLoadSource::Broker { name, properties }) = with else {
        panic!("expected WITH BROKER")
    };
    assert_eq!(name.value, "b1");
    assert_eq!(properties.len(), 1);
}

#[test]
fn parse_doris_broker_load_negative() {
    let dialects = all_dialects_where(|d| d.supports_broker_load());

    // Missing the parenthesized data_desc list.
    assert!(dialects.parse_sql_statements("LOAD LABEL l").is_err());
    // Missing INTO TABLE in a data_desc.
    assert!(dialects
        .parse_sql_statements("LOAD LABEL l (DATA INFILE ('f'))")
        .is_err());
    // Empty file list.
    assert!(dialects
        .parse_sql_statements("LOAD LABEL l (DATA INFILE () INTO TABLE t)")
        .is_err());
    // Unknown source after WITH.
    assert!(dialects
        .parse_sql_statements("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t) WITH GCS ('k'='v')")
        .is_err());
    // Not enabled for other dialects.
    assert!(TestedDialects::new(vec![Box::new(GenericDialect {})])
        .parse_sql_statements("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t)")
        .is_err());
}

#[test]
fn parse_doris_broker_load_full_data_desc() {
    let dialects = all_dialects_where(|d| d.supports_broker_load());

    dialects.verified_stmt(
        "LOAD LABEL l (MERGE DATA INFILE ('f1', 'f2') NEGATIVE INTO TABLE db.t PARTITION (p1, p2) COLUMNS TERMINATED BY ',' LINES TERMINATED BY '|' FORMAT AS 'csv' COMPRESS_TYPE AS 'gz' (c1, c2) COLUMNS FROM PATH AS (c3, c4) SET (k1 = c1 * 2, k2 = year(c4)) PRECEDING FILTER c1 > 0 WHERE c2 IS NOT NULL DELETE ON c3 = 'x' ORDER BY c1 PROPERTIES ('k' = 'v'))",
    );
    dialects.verified_stmt("LOAD LABEL l (APPEND DATA INFILE ('f') INTO TABLE t)");
    dialects.verified_stmt("LOAD LABEL l (DELETE DATA INFILE ('f') INTO TABLE t)");
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t WHERE dt = '2024-01-01')");
    dialects.verified_stmt("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t SET (k = lower(c1)))");
    dialects.verified_stmt(
        "LOAD LABEL l (DATA INFILE ('f') INTO TABLE t PROPERTIES ('strict_mode' = 'true'))",
    );
}

#[test]
fn ast_doris_broker_load_full_desc_is_structured() {
    let stmt = doris().verified_stmt(
        "LOAD LABEL l (MERGE DATA INFILE ('f') NEGATIVE INTO TABLE db.t PARTITION (p1) SET (k = f(c1)) WHERE c1 > 0 DELETE ON c2 = 'x' ORDER BY c1)",
    );
    let Statement::DorisBrokerLoad { data_descs, .. } = stmt else {
        panic!("expected DorisBrokerLoad")
    };
    let desc = &data_descs[0];
    assert_eq!(desc.merge_type, Some(DorisLoadMergeType::Merge));
    assert!(desc.negative);
    assert_eq!(desc.partition, Some(vec![Ident::new("p1")]));
    assert_eq!(desc.set.as_ref().map(|s| s.len()), Some(1));
    assert_eq!(
        desc.where_clause,
        Some(Expr::BinaryOp {
            left: Box::new(Expr::Identifier(Ident::new("c1"))),
            op: BinaryOperator::Gt,
            right: Box::new(Expr::value(number("0"))),
        })
    );
    assert_eq!(desc.order_by, Some(Ident::new("c1")),);
    assert!(desc.delete_on.is_some());
}

#[test]
fn parse_doris_broker_load_data_desc_negative() {
    let dialects = all_dialects_where(|d| d.supports_broker_load());

    // Merge type must be followed by DATA INFILE.
    assert!(dialects
        .parse_sql_statements("LOAD LABEL l (MERGE INTO TABLE t)")
        .is_err());
    // SET requires a parenthesized assignment list.
    assert!(dialects
        .parse_sql_statements("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t SET k = c1)")
        .is_err());
    // DELETE ON requires an expression.
    assert!(dialects
        .parse_sql_statements("LOAD LABEL l (DATA INFILE ('f') INTO TABLE t DELETE ON)")
        .is_err());
}
