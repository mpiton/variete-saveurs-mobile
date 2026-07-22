use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, params};

pub fn open_database(path: &Path) -> rusqlite::Result<Mutex<Connection>> {
    let connection = Connection::open(path)?;
    migrate(&connection)?;
    seed_catalog(&connection)?;
    Ok(Mutex::new(connection))
}

pub fn migrate(connection: &Connection) -> rusqlite::Result<()> {
    // ponytail: v1 has no historical schemas; add numbered migrations when the schema first changes.
    connection.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS counters (
            name TEXT PRIMARY KEY,
            next_number INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS catalog_items (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            group_name TEXT,
            unit_price_cents INTEGER NOT NULL,
            unit TEXT,
            active INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS documents (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL CHECK (kind IN ('quote', 'invoice')),
            number INTEGER NOT NULL,
            issue_date TEXT NOT NULL,
            event_date TEXT NOT NULL,
            payment_terms TEXT NOT NULL,
            client_kind TEXT NOT NULL,
            client_name TEXT NOT NULL,
            client_address TEXT NOT NULL,
            client_email TEXT,
            client_phone TEXT,
            client_business_id TEXT,
            client_billing_address TEXT,
            lines_json TEXT NOT NULL,
            total_cents INTEGER NOT NULL,
            source_quote_id INTEGER REFERENCES documents(id),
            sent_at TEXT,
            created_at TEXT NOT NULL,
            UNIQUE (kind, number)
        );

        CREATE TABLE IF NOT EXISTS draft (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            payload_json TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO counters (name, next_number) VALUES (?1, ?2)",
        params!["quote", 10_i64],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO counters (name, next_number) VALUES (?1, ?2)",
        params!["invoice", 1_i64],
    )?;
    Ok(())
}

pub fn seed_catalog(connection: &Connection) -> rusqlite::Result<()> {
    let item_count = connection.query_row("SELECT COUNT(*) FROM catalog_items", [], |row| {
        row.get::<_, i64>(0)
    })?;
    if item_count != 0 {
        return Ok(());
    }

    let transaction = connection.unchecked_transaction()?;
    for (name, group_name, unit_price_cents) in [
        ("Mini Burgers", "Salé", 85_i64),
        ("Mini Pizzas", "Salé", 85_i64),
        ("Mini Quiches", "Salé", 80_i64),
        ("Mini Wraps", "Salé", 80_i64),
        ("Mini Feuilletés saucisse et thon", "Salé", 85_i64),
        ("Mini Brochettes de fruits", "Sucré", 85_i64),
    ] {
        transaction.execute(
            "INSERT INTO catalog_items (name, group_name, unit_price_cents, unit, active)
             VALUES (?1, ?2, ?3, 'pièce', 1)",
            params![name, group_name, unit_price_cents],
        )?;
    }
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::{Connection, params};

    use super::{migrate, open_database, seed_catalog};

    fn temp_connection() -> (tempfile::NamedTempFile, Connection) {
        let file = tempfile::NamedTempFile::new().expect("create temp db");
        let connection = Connection::open(file.path()).expect("open temp db");
        (file, connection)
    }

    fn table_columns(connection: &Connection, table: &str) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare table info");
        statement
            .query_map([], |row| row.get(1))
            .expect("query table info")
            .collect::<rusqlite::Result<_>>()
            .expect("collect table columns")
    }

    #[test]
    fn migrate_is_idempotent() {
        let (_file, connection) = temp_connection();

        migrate(&connection).expect("first migration");
        migrate(&connection).expect("second migration");
    }

    #[test]
    fn migrate_creates_the_architecture_schema() {
        let (_file, connection) = temp_connection();
        migrate(&connection).expect("migrate");

        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare table list");
        let tables = statement
            .query_map([], |row| row.get(0))
            .expect("query table list")
            .collect::<rusqlite::Result<Vec<String>>>()
            .expect("collect tables");

        assert_eq!(
            tables,
            [
                "catalog_items",
                "counters",
                "documents",
                "draft",
                "settings"
            ]
        );
        assert_eq!(
            table_columns(&connection, "counters"),
            ["name", "next_number"]
        );
        assert_eq!(
            table_columns(&connection, "catalog_items"),
            [
                "id",
                "name",
                "group_name",
                "unit_price_cents",
                "unit",
                "active"
            ]
        );
        assert_eq!(
            table_columns(&connection, "documents"),
            [
                "id",
                "kind",
                "number",
                "issue_date",
                "event_date",
                "payment_terms",
                "client_kind",
                "client_name",
                "client_address",
                "client_email",
                "client_phone",
                "client_business_id",
                "client_billing_address",
                "lines_json",
                "total_cents",
                "source_quote_id",
                "sent_at",
                "created_at"
            ]
        );
        assert_eq!(
            table_columns(&connection, "draft"),
            ["id", "payload_json", "updated_at"]
        );
        assert_eq!(table_columns(&connection, "settings"), ["key", "value"]);
    }

    #[test]
    fn migrate_enforces_document_and_draft_constraints() {
        let (_file, connection) = temp_connection();
        migrate(&connection).expect("migrate");

        let insert_document = |kind: &str, number: i64, source_quote_id: Option<i64>| {
            connection.execute(
                "INSERT INTO documents (
                    kind, number, issue_date, event_date, payment_terms,
                    client_kind, client_name, client_address, lines_json,
                    total_cents, source_quote_id, created_at
                 ) VALUES (?1, ?2, '2026-07-22', '2026-07-23', 'à réception',
                           'individual', 'Client', 'Adresse', '[]', 0, ?3,
                           '2026-07-22T10:00:00Z')",
                params![kind, number, source_quote_id],
            )
        };

        assert!(insert_document("receipt", 1, None).is_err());
        insert_document("quote", 10, None).expect("insert quote");
        assert!(insert_document("quote", 10, None).is_err());
        insert_document("invoice", 10, Some(1)).expect("insert converted invoice");
        assert!(insert_document("invoice", 11, Some(999)).is_err());
        assert!(
            connection
                .execute(
                    "INSERT INTO draft (id, payload_json, updated_at) VALUES (2, '{}', 'now')",
                    [],
                )
                .is_err()
        );
    }

    #[test]
    fn open_database_seeds_desktop_defaults_once() {
        let file = tempfile::NamedTempFile::new().expect("create temp db");
        let database: Mutex<Connection> = open_database(file.path()).expect("open initialized db");
        let connection = database.lock().expect("lock db");

        seed_catalog(&connection).expect("reseed catalog");
        assert_eq!(
            connection
                .query_row(
                    "SELECT next_number FROM counters WHERE name = 'quote'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("quote counter"),
            10
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT next_number FROM counters WHERE name = 'invoice'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("invoice counter"),
            1
        );

        let mut statement = connection
            .prepare(
                "SELECT name || '|' || group_name || '|' || unit_price_cents || '|' || unit || '|' || active
                 FROM catalog_items ORDER BY id",
            )
            .expect("prepare catalog");
        let items = statement
            .query_map([], |row| row.get(0))
            .expect("query catalog")
            .collect::<rusqlite::Result<Vec<String>>>()
            .expect("collect catalog");

        assert_eq!(
            items,
            [
                "Mini Burgers|Salé|85|pièce|1",
                "Mini Pizzas|Salé|85|pièce|1",
                "Mini Quiches|Salé|80|pièce|1",
                "Mini Wraps|Salé|80|pièce|1",
                "Mini Feuilletés saucisse et thon|Salé|85|pièce|1",
                "Mini Brochettes de fruits|Sucré|85|pièce|1"
            ]
        );
    }

    #[test]
    fn database_api_only_exposes_initialization() {
        let tokens = include_str!("db.rs").split_whitespace().collect::<Vec<_>>();
        let public_functions = tokens
            .windows(3)
            .filter(|tokens| {
                (tokens[0] == "pub" || tokens[0].starts_with("pub(")) && tokens[1] == "fn"
            })
            .map(|tokens| tokens[2].split('(').next().expect("function name"))
            .collect::<Vec<_>>();

        assert_eq!(
            public_functions,
            ["open_database", "migrate", "seed_catalog"]
        );
    }
}
