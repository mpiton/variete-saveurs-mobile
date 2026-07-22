use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, Row, Transaction, params, types::Type};

use super::models::{ClientInput, ClientKind, Document, DocumentInput, DocumentKind, LineInput};

const DOCUMENT_SELECT: &str = "
    SELECT d.id, d.kind, d.number, d.issue_date, d.event_date, d.payment_terms,
           d.client_kind, d.client_name, d.client_address, d.client_email,
           d.client_phone, d.client_business_id, d.client_billing_address,
           d.lines_json, d.total_cents, d.source_quote_id, d.sent_at, d.created_at,
           EXISTS (
               SELECT 1 FROM documents invoice WHERE invoice.source_quote_id = d.id
           ) AS is_invoiced
    FROM documents d
";

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
            client_kind TEXT NOT NULL CHECK (client_kind IN ('individual', 'professional')),
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
            CHECK (source_quote_id IS NULL OR kind = 'invoice'),
            UNIQUE (kind, number)
        );

        CREATE TRIGGER IF NOT EXISTS documents_source_quote_is_quote
        BEFORE INSERT ON documents
        WHEN NEW.source_quote_id IS NOT NULL
             AND NOT EXISTS (
                 SELECT 1 FROM documents
                 WHERE id = NEW.source_quote_id AND kind = 'quote'
             )
        BEGIN
            SELECT RAISE(ABORT, 'source_quote_id must reference a quote');
        END;

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

pub fn insert_document(
    transaction: &Transaction<'_>,
    number: i64,
    input: &DocumentInput,
    source_quote_id: Option<i64>,
    created_at: &str,
) -> rusqlite::Result<i64> {
    let lines_json = serde_json::to_string(&input.lines)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    transaction.execute(
        "INSERT INTO documents (
            kind, number, issue_date, event_date, payment_terms,
            client_kind, client_name, client_address, client_email, client_phone,
            client_business_id, client_billing_address, lines_json, total_cents,
            source_quote_id, created_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
         )",
        params![
            input.kind.as_str(),
            number,
            &input.issue_date,
            &input.event_date,
            &input.payment_terms,
            client_kind_text(&input.client.kind),
            &input.client.name,
            &input.client.address,
            input.client.email.as_deref(),
            input.client.phone.as_deref(),
            input.client.business_id.as_deref(),
            input.client.billing_address.as_deref(),
            lines_json,
            input.total_cents(),
            source_quote_id,
            created_at,
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

pub fn list_documents(
    connection: &Connection,
    filter: Option<&DocumentKind>,
) -> rusqlite::Result<Vec<Document>> {
    let query = format!(
        "{DOCUMENT_SELECT}
         WHERE (?1 IS NULL OR d.kind = ?1)
         ORDER BY d.created_at DESC, d.id DESC"
    );
    let mut statement = connection.prepare(&query)?;
    statement
        .query_map(params![filter.map(DocumentKind::as_str)], document_from_row)?
        .collect()
}

pub fn get_document(connection: &Connection, id: i64) -> rusqlite::Result<Document> {
    connection.query_row(
        &format!("{DOCUMENT_SELECT} WHERE d.id = ?1"),
        [id],
        document_from_row,
    )
}

pub fn mark_sent(connection: &Connection, id: i64, sent_at: &str) -> rusqlite::Result<bool> {
    if sent_at.trim().is_empty() {
        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "sent_at must not be blank",
            ),
        )));
    }
    Ok(connection.execute(
        "UPDATE documents SET sent_at = ?1 WHERE id = ?2 AND sent_at IS NULL",
        params![sent_at, id],
    )? == 1)
}

pub fn search_clients(connection: &Connection, prefix: &str) -> rusqlite::Result<Vec<ClientInput>> {
    // ponytail: scans distinct local history for accent folding; add a normalized
    // indexed column if the single-user history grows enough to matter.
    let mut statement = connection.prepare(
        "SELECT DISTINCT client_kind, client_name, client_address, client_email,
                         client_phone, client_business_id, client_billing_address
         FROM documents
         GROUP BY client_kind, client_name, client_address, client_email,
                  client_phone, client_business_id, client_billing_address
         ORDER BY MAX(created_at) DESC, MAX(id) DESC",
    )?;
    let clients = statement
        .query_map([], |row| {
            let kind_column = row.as_ref().column_index("client_kind")?;
            let kind = row.get::<_, String>("client_kind")?;
            Ok(ClientInput {
                kind: parse_client_kind(&kind, kind_column)?,
                name: row.get("client_name")?,
                address: row.get("client_address")?,
                email: row.get("client_email")?,
                phone: row.get("client_phone")?,
                business_id: row.get("client_business_id")?,
                billing_address: row.get("client_billing_address")?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let prefix = normalize_client_search(prefix);
    Ok(clients
        .into_iter()
        .filter(|client| normalize_client_search(&client.name).starts_with(&prefix))
        .take(5)
        .collect())
}

fn document_from_row(row: &Row<'_>) -> rusqlite::Result<Document> {
    let kind_column = row.as_ref().column_index("kind")?;
    let client_kind_column = row.as_ref().column_index("client_kind")?;
    let lines_column = row.as_ref().column_index("lines_json")?;
    let kind = row.get::<_, String>("kind")?;
    let client_kind = row.get::<_, String>("client_kind")?;
    let lines_json = row.get::<_, String>("lines_json")?;
    let lines = serde_json::from_str::<Vec<LineInput>>(&lines_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(lines_column, Type::Text, Box::new(error))
    })?;

    Ok(Document {
        id: row.get("id")?,
        number: row.get("number")?,
        input: DocumentInput {
            kind: parse_document_kind(&kind, kind_column)?,
            issue_date: row.get("issue_date")?,
            event_date: row.get("event_date")?,
            payment_terms: row.get("payment_terms")?,
            client: ClientInput {
                kind: parse_client_kind(&client_kind, client_kind_column)?,
                name: row.get("client_name")?,
                address: row.get("client_address")?,
                email: row.get("client_email")?,
                phone: row.get("client_phone")?,
                business_id: row.get("client_business_id")?,
                billing_address: row.get("client_billing_address")?,
            },
            lines,
        },
        total_cents: row.get("total_cents")?,
        source_quote_id: row.get("source_quote_id")?,
        sent_at: row.get("sent_at")?,
        created_at: row.get("created_at")?,
        is_invoiced: row.get::<_, i64>("is_invoiced")? != 0,
    })
}

fn client_kind_text(kind: &ClientKind) -> &'static str {
    match kind {
        ClientKind::Individual => "individual",
        ClientKind::Professional => "professional",
    }
}

fn parse_document_kind(value: &str, column: usize) -> rusqlite::Result<DocumentKind> {
    match value {
        "quote" => Ok(DocumentKind::Quote),
        "invoice" => Ok(DocumentKind::Invoice),
        _ => Err(invalid_text(column, value)),
    }
}

fn parse_client_kind(value: &str, column: usize) -> rusqlite::Result<ClientKind> {
    match value {
        "individual" => Ok(ClientKind::Individual),
        "professional" => Ok(ClientKind::Professional),
        _ => Err(invalid_text(column, value)),
    }
}

fn normalize_client_search(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|character| match character {
            '\u{0300}'..='\u{036f}' => None,
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => Some('a'),
            'ç' => Some('c'),
            'è' | 'é' | 'ê' | 'ë' => Some('e'),
            'ì' | 'í' | 'î' | 'ï' => Some('i'),
            'ñ' => Some('n'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => Some('o'),
            'ù' | 'ú' | 'û' | 'ü' => Some('u'),
            'ý' | 'ÿ' => Some('y'),
            _ => Some(character),
        })
        .collect()
}

fn invalid_text(column: usize, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid persisted value: {value}"),
        )),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::{Connection, params};

    use super::{
        get_document, insert_document, list_documents, mark_sent, migrate, open_database,
        search_clients, seed_catalog,
    };
    use crate::domain::models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput};

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

    fn insert_raw_document(
        connection: &Connection,
        kind: &str,
        client_kind: &str,
        number: i64,
        source_quote_id: Option<i64>,
    ) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO documents (
                kind, number, issue_date, event_date, payment_terms,
                client_kind, client_name, client_address, lines_json,
                total_cents, source_quote_id, created_at
             ) VALUES (?1, ?2, '2026-07-22', '2026-07-23', 'à réception',
                       ?3, 'Client', 'Adresse', '[]', 0, ?4,
                       '2026-07-22T10:00:00Z')",
            params![kind, number, client_kind, source_quote_id],
        )
    }

    fn initialized_connection() -> (tempfile::NamedTempFile, Connection) {
        let (file, connection) = temp_connection();
        migrate(&connection).expect("migrate");
        (file, connection)
    }

    fn document_input(kind: DocumentKind, client_name: &str) -> DocumentInput {
        DocumentInput {
            kind,
            issue_date: "2026-07-22".to_string(),
            event_date: "2026-08-15".to_string(),
            payment_terms: "à réception".to_string(),
            client: ClientInput {
                kind: ClientKind::Professional,
                name: client_name.to_string(),
                address: "12 rue Émile Zola, Lyon".to_string(),
                email: Some("contact@example.com".to_string()),
                phone: Some("0601020304".to_string()),
                business_id: Some("123 456 789 00010".to_string()),
                billing_address: Some("Étage 1".to_string()),
            },
            lines: vec![
                LineInput {
                    group: Some("Salé".to_string()),
                    description: "Mini burgers".to_string(),
                    quantity: 50,
                    unit_price_cents: 85,
                },
                LineInput {
                    group: Some("Sucré".to_string()),
                    description: "Mini brochettes de fruits".to_string(),
                    quantity: 10,
                    unit_price_cents: 85,
                },
            ],
        }
    }

    fn persist_document(
        connection: &mut Connection,
        number: i64,
        input: &DocumentInput,
        source_quote_id: Option<i64>,
        created_at: &str,
    ) -> i64 {
        let transaction = connection.transaction().expect("begin transaction");
        let id = insert_document(&transaction, number, input, source_quote_id, created_at)
            .expect("insert document");
        transaction.commit().expect("commit document");
        id
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

        assert!(insert_raw_document(&connection, "receipt", "individual", 1, None).is_err());
        insert_raw_document(&connection, "quote", "individual", 10, None).expect("insert quote");
        assert!(insert_raw_document(&connection, "quote", "individual", 10, None).is_err());
        insert_raw_document(&connection, "invoice", "professional", 10, Some(1))
            .expect("insert converted invoice");
        assert!(insert_raw_document(&connection, "invoice", "individual", 11, Some(999)).is_err());
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
    fn rejects_unknown_client_kinds() {
        let (_file, connection) = temp_connection();
        migrate(&connection).expect("migrate");

        assert!(insert_raw_document(&connection, "quote", "association", 10, None).is_err());
    }

    #[test]
    fn only_invoices_can_reference_quotes() {
        let (_file, connection) = temp_connection();
        migrate(&connection).expect("migrate");
        insert_raw_document(&connection, "quote", "individual", 10, None).expect("insert quote");
        insert_raw_document(&connection, "invoice", "professional", 1, None)
            .expect("insert invoice");

        assert!(insert_raw_document(&connection, "quote", "individual", 11, Some(1)).is_err());
        assert!(insert_raw_document(&connection, "invoice", "individual", 2, Some(2)).is_err());
        insert_raw_document(&connection, "invoice", "individual", 2, Some(1))
            .expect("reference quote from invoice");
    }

    #[test]
    fn inserted_document_roundtrips_all_persisted_fields() {
        let (_file, mut connection) = initialized_connection();
        let input = document_input(DocumentKind::Quote, "Mairie de Lyon");

        let id = persist_document(&mut connection, 10, &input, None, "2026-07-22T10:00:00Z");
        let saved = get_document(&connection, id).expect("get document");

        assert_eq!(saved.id, id);
        assert_eq!(saved.number, 10);
        assert_eq!(saved.input, input);
        assert_eq!(saved.total_cents, 5_100);
        assert_eq!(saved.source_quote_id, None);
        assert_eq!(saved.sent_at, None);
        assert_eq!(saved.created_at, "2026-07-22T10:00:00Z");
        assert!(!saved.is_invoiced);

        let (lines_json, total_cents): (String, i64) = connection
            .query_row(
                "SELECT lines_json, total_cents FROM documents WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read serialized fields");
        assert_eq!(
            serde_json::from_str::<Vec<LineInput>>(&lines_json).expect("deserialize lines"),
            saved.input.lines
        );
        assert_eq!(total_cents, saved.total_cents);
    }

    #[test]
    fn lists_recent_documents_by_kind_with_derived_invoice_status() {
        let (_file, mut connection) = initialized_connection();
        let quote = document_input(DocumentKind::Quote, "Premier client");
        let latest_quote = document_input(DocumentKind::Quote, "Dernier client");
        let invoice = document_input(DocumentKind::Invoice, "Facture directe");
        let converted_invoice = document_input(DocumentKind::Invoice, "Client converti");

        let quote_id = persist_document(&mut connection, 10, &quote, None, "2026-07-22T09:00:00Z");
        let invoice_id =
            persist_document(&mut connection, 1, &invoice, None, "2026-07-22T10:00:00Z");
        let converted_id = persist_document(
            &mut connection,
            2,
            &converted_invoice,
            Some(quote_id),
            "2026-07-22T11:00:00Z",
        );
        let latest_quote_id = persist_document(
            &mut connection,
            11,
            &latest_quote,
            None,
            "2026-07-22T12:00:00Z",
        );

        let all = list_documents(&connection, None).expect("list all documents");
        assert_eq!(
            all.iter().map(|document| document.id).collect::<Vec<_>>(),
            [latest_quote_id, converted_id, invoice_id, quote_id]
        );
        assert!(
            all.iter()
                .find(|document| document.id == quote_id)
                .expect("source quote")
                .is_invoiced
        );
        assert!(
            !all.iter()
                .find(|document| document.id == converted_id)
                .expect("converted invoice")
                .is_invoiced
        );

        let quotes =
            list_documents(&connection, Some(&DocumentKind::Quote)).expect("list quote documents");
        assert_eq!(
            quotes
                .iter()
                .map(|document| document.id)
                .collect::<Vec<_>>(),
            [latest_quote_id, quote_id]
        );
        let invoices = list_documents(&connection, Some(&DocumentKind::Invoice))
            .expect("list invoice documents");
        assert_eq!(
            invoices
                .iter()
                .map(|document| document.id)
                .collect::<Vec<_>>(),
            [converted_id, invoice_id]
        );
    }

    #[test]
    fn marking_sent_preserves_the_first_timestamp() {
        let (_file, mut connection) = initialized_connection();
        let input = document_input(DocumentKind::Quote, "Client");
        let id = persist_document(&mut connection, 10, &input, None, "2026-07-22T09:00:00Z");

        assert!(mark_sent(&connection, id, "   ").is_err());
        assert_eq!(
            get_document(&connection, id)
                .expect("get unsent document")
                .sent_at,
            None
        );
        assert!(mark_sent(&connection, id, "2026-07-22T10:00:00Z").expect("mark first send"));
        assert!(!mark_sent(&connection, id, "2026-07-22T11:00:00Z").expect("mark second send"));
        assert_eq!(
            get_document(&connection, id)
                .expect("get sent document")
                .sent_at
                .as_deref(),
            Some("2026-07-22T10:00:00Z")
        );
    }

    #[test]
    fn searches_distinct_recent_clients_with_accents_and_a_limit() {
        let (_file, mut connection) = initialized_connection();
        let clients = [
            ("Mairie de Lyon", "2026-07-22T09:00:00Z"),
            ("Maison Élodie", "2026-07-22T10:00:00Z"),
            ("Maillane", "2026-07-22T11:00:00Z"),
            ("Main Street", "2026-07-22T12:00:00Z"),
            ("Maillot", "2026-07-22T13:00:00Z"),
            ("Maille", "2026-07-22T14:00:00Z"),
            ("Mairie de Lyon", "2026-07-22T15:00:00Z"),
        ];
        for (index, (name, created_at)) in clients.into_iter().enumerate() {
            let input = document_input(DocumentKind::Quote, name);
            persist_document(
                &mut connection,
                10 + i64::try_from(index).expect("small index"),
                &input,
                None,
                created_at,
            );
        }

        let matches = search_clients(&connection, "mai").expect("search clients");
        assert_eq!(matches.len(), 5);
        assert_eq!(matches[0].name, "Mairie de Lyon");
        assert_eq!(matches[0].address, "12 rue Émile Zola, Lyon");
        assert_eq!(
            matches
                .iter()
                .filter(|client| client.name == "Mairie de Lyon")
                .count(),
            1
        );

        let accented = document_input(DocumentKind::Quote, "Élodie Traiteur");
        persist_document(&mut connection, 17, &accented, None, "2026-07-22T16:00:00Z");
        assert_eq!(
            search_clients(&connection, "elodie")
                .expect("search accent-insensitively")
                .iter()
                .map(|client| client.name.as_str())
                .collect::<Vec<_>>(),
            ["Élodie Traiteur"]
        );

        let literal_wildcard = document_input(DocumentKind::Quote, "Mai_Client");
        persist_document(
            &mut connection,
            18,
            &literal_wildcard,
            None,
            "2026-07-22T17:00:00Z",
        );
        let wildcard_match = document_input(DocumentKind::Quote, "MaixClient");
        persist_document(
            &mut connection,
            19,
            &wildcard_match,
            None,
            "2026-07-22T18:00:00Z",
        );
        assert_eq!(
            search_clients(&connection, "mai_")
                .expect("search literal prefix")
                .iter()
                .map(|client| client.name.as_str())
                .collect::<Vec<_>>(),
            ["Mai_Client"]
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
}
