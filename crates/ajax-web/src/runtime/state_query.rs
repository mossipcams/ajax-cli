use super::{
    json_value_response, AxumResponse, CommandRunner, Json, RuntimeBridge, State, WebAppState,
};
use rusqlite::{limits::Limit, OpenFlags};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use std::path::Path;

const MAX_STATE_QUERY_ROWS: usize = 1000;

#[derive(Debug, Deserialize)]
pub(super) struct StateQueryRequest {
    sql: String,
}

fn to_json_value(value: rusqlite::types::Value) -> JsonValue {
    match value {
        rusqlite::types::Value::Null => JsonValue::Null,
        rusqlite::types::Value::Integer(integer) => integer.into(),
        rusqlite::types::Value::Real(real) => real.into(),
        rusqlite::types::Value::Text(text) => JsonValue::String(text),
        rusqlite::types::Value::Blob(blob) => {
            JsonValue::String(blob.iter().map(|byte| format!("{byte:02x}")).collect())
        }
    }
}

pub(super) async fn axum_state_query<C, B>(
    State(state): State<WebAppState<C, B>>,
    Json(body): Json<StateQueryRequest>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let db_path = state.shared().context.runtime_paths.state_db.clone();
    let sql = body.sql;
    match tokio::task::spawn_blocking(move || run_state_query(&db_path, &sql)).await {
        Ok(Ok(result)) => json_value_response(200, result),
        Ok(Err(error)) => {
            json_value_response(400, serde_json::json!({ "ok": false, "error": error }))
        }
        Err(_) => json_value_response(
            502,
            serde_json::json!({ "ok": false, "error": "state query worker failed" }),
        ),
    }
}

fn run_state_query(db_path: &Path, sql: &str) -> Result<JsonValue, String> {
    if sql.trim().is_empty() {
        return Err("sql must not be empty".to_string());
    }

    let connection = rusqlite::Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("open state database failed: {error}"))?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(|error| format!("limit attached databases failed: {error}"))?;
    connection
        .execute_batch("PRAGMA query_only = ON")
        .map_err(|error| format!("enable read-only mode failed: {error}"))?;

    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    if !statement.readonly() {
        return Err("statement must be read-only".to_string());
    }

    let column_count = statement.column_count();
    let columns: Vec<String> = statement.column_names().map(str::to_owned).collect();
    let mut rows = Vec::new();
    let mut truncated = false;
    let mut query_rows = statement.query([]).map_err(|error| error.to_string())?;
    while let Some(row) = query_rows.next().map_err(|error| error.to_string())? {
        if rows.len() == MAX_STATE_QUERY_ROWS {
            truncated = true;
            break;
        }
        let cells = (0..column_count)
            .map(|index| {
                row.get::<_, rusqlite::types::Value>(index)
                    .map(to_json_value)
            })
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| format!("read state query row failed: {error}"))?;
        rows.push(JsonValue::Array(cells));
    }

    Ok(json!({ "columns": columns, "rows": rows, "truncated": truncated }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn open_test_db(tag: &str) -> std::path::PathBuf {
        let directory = crate::runtime::tests::scratch_dir(&format!("state-query-{tag}"));
        fs::create_dir_all(&directory).unwrap();
        directory.join("state.db")
    }

    fn seed(db: &Path) {
        let connection = rusqlite::Connection::open(db).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, blob_data BLOB); \
             INSERT INTO items (name) VALUES ('alpha'), ('beta');",
            )
            .unwrap();
    }

    fn query(db: &Path, sql: &str) -> Result<JsonValue, String> {
        run_state_query(db, sql)
    }

    #[test]
    fn select_returns_rows_columns_and_values() {
        let db = open_test_db("select");
        seed(&db);
        let result = query(&db, "SELECT id, name FROM items ORDER BY id").unwrap();
        assert_eq!(result["columns"], json!(["id", "name"]));
        assert_eq!(result["rows"][0], json!([1, "alpha"]));
        assert_eq!(result["truncated"], json!(false));
        assert_eq!(
            query(&db, "SELECT ';'; \n").unwrap()["rows"],
            json!([[";"]])
        );
    }

    #[test]
    fn select_maps_value_types() {
        let db = open_test_db("values");
        seed(&db);
        let result = query(
            &db,
            "SELECT name, CAST(NULL AS TEXT), 42, 1.5, X'00FFDEAD' FROM items WHERE id = 1",
        )
        .unwrap();
        assert_eq!(
            result["rows"][0],
            json!(["alpha", null, 42, 1.5, "00ffdead"])
        );
    }

    #[test]
    fn write_statements_are_rejected_and_db_is_unchanged() {
        let db = open_test_db("writes");
        seed(&db);
        for sql in [
            "INSERT INTO items (name) VALUES ('intruder')",
            "UPDATE items SET name = 'renamed' WHERE id = 1",
            "DELETE FROM items",
            "DROP TABLE items",
            "PRAGMA journal_mode = WAL",
            "PRAGMA user_version = 123",
            "CREATE TABLE snooped (value TEXT)",
        ] {
            let error = query(&db, sql).unwrap_err();
            assert!(
                error.contains("read-only") || error.contains("readonly"),
                "{sql}: {error}"
            );
        }
        let result = query(&db, "SELECT COUNT(*) FROM items").unwrap();
        assert_eq!(result["rows"][0][0], json!(2));
        assert_eq!(
            query(&db, "SELECT id, name FROM items ORDER BY id").unwrap()["rows"],
            json!([[1, "alpha"], [2, "beta"]])
        );
        assert_eq!(
            query(&db, "PRAGMA user_version").unwrap()["rows"],
            json!([[0]])
        );
        let connection = rusqlite::Connection::open_with_flags(
            &db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "delete");
    }

    #[test]
    fn attach_database_is_rejected() {
        let db = open_test_db("attach");
        seed(&db);
        let foreign = open_test_db("attach-target");
        seed(&foreign);
        let error = query(
            &db,
            &format!("ATTACH DATABASE '{}' AS other", foreign.display()),
        )
        .unwrap_err();
        assert!(!error.is_empty());
        assert!(query(&db, "SELECT * FROM other.items").is_err());
        assert_eq!(
            query(&db, "PRAGMA database_list").unwrap()["rows"],
            json!([[0, "main", db.canonicalize().unwrap().to_str().unwrap()]])
        );
    }

    #[test]
    fn vacuum_into_is_rejected_without_creating_a_file() {
        let db = open_test_db("vacuum-into");
        seed(&db);
        let target = db.with_file_name("vacuum-out.db");
        assert!(query(&db, &format!("VACUUM INTO '{}'", target.display())).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn multi_statement_input_is_rejected() {
        let db = open_test_db("multi");
        seed(&db);
        for sql in [
            "SELECT 1; SELECT 2",
            "PRAGMA query_only = OFF; SELECT 1",
            "SELECT load_extension('x')",
            "SELECT ';' ; PRAGMA query_only = OFF",
        ] {
            assert!(query(&db, sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn row_cap_truncates() {
        let db = open_test_db("cap");
        seed(&db);
        let connection = rusqlite::Connection::open(&db).unwrap();
        for index in 3..=MAX_STATE_QUERY_ROWS + 50 {
            connection
                .execute(
                    "INSERT INTO items (name) VALUES (?1)",
                    rusqlite::params![format!("row-{index}")],
                )
                .unwrap();
        }
        let result = query(&db, "SELECT id FROM items").unwrap();
        assert_eq!(
            result["rows"].as_array().unwrap().len(),
            MAX_STATE_QUERY_ROWS
        );
        assert_eq!(result["truncated"], json!(true));
        let exact = query(&db, "SELECT id FROM items LIMIT 1000").unwrap();
        assert_eq!(
            exact["rows"].as_array().unwrap().len(),
            MAX_STATE_QUERY_ROWS
        );
        assert_eq!(exact["truncated"], json!(false));
    }

    #[test]
    fn sql_errors_surface_as_string_not_panic() {
        let db = open_test_db("errors");
        seed(&db);
        assert!(query(&db, "SELECT * FROM missing_table").is_err());
        assert!(query(&db, "").is_err());
        assert!(query(&db, "   ").is_err());
    }
}
