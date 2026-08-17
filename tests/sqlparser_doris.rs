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
            assert_eq!(km.order_by, Some(vec![Ident::new("c")]));
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
                } => {
                    assert_eq!(from.len(), 1);
                    assert_eq!(to.len(), 1);
                    assert_eq!(*interval_value, 1);
                    assert_eq!(interval_unit.as_ref().unwrap(), &Ident::new("DAY"));
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
