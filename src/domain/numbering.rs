use rusqlite::{Connection, Transaction, params};

use crate::domain::models::DocumentKind;

pub fn next_number(connection: &Connection, kind: &DocumentKind) -> rusqlite::Result<i64> {
    connection.query_row(
        "SELECT next_number FROM counters WHERE name = ?1",
        params![kind.as_str()],
        |row| row.get(0),
    )
}

pub fn reserve_number(transaction: &Transaction<'_>, kind: &DocumentKind) -> rusqlite::Result<i64> {
    transaction.query_row(
        "UPDATE counters
         SET next_number = next_number + 1
         WHERE name = ?1
         RETURNING next_number - 1",
        params![kind.as_str()],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{next_number, reserve_number};
    use crate::domain::{db::migrate, models::DocumentKind};

    fn database() -> Connection {
        let connection = Connection::open_in_memory().expect("open database");
        migrate(&connection).expect("migrate database");
        connection
    }

    #[test]
    fn reading_does_not_reserve_number() {
        let connection = database();

        assert_eq!(
            next_number(&connection, &DocumentKind::Quote).expect("read quote number"),
            10
        );
        assert_eq!(
            next_number(&connection, &DocumentKind::Quote).expect("read quote number again"),
            10
        );
    }

    #[test]
    fn reserves_independent_increasing_sequences() {
        let mut connection = database();
        let transaction = connection.transaction().expect("begin transaction");

        assert_eq!(
            reserve_number(&transaction, &DocumentKind::Quote).expect("reserve quote 10"),
            10
        );
        assert_eq!(
            reserve_number(&transaction, &DocumentKind::Quote).expect("reserve quote 11"),
            11
        );
        assert_eq!(
            reserve_number(&transaction, &DocumentKind::Quote).expect("reserve quote 12"),
            12
        );
        assert_eq!(
            reserve_number(&transaction, &DocumentKind::Invoice).expect("reserve invoice 1"),
            1
        );
        assert_eq!(
            reserve_number(&transaction, &DocumentKind::Invoice).expect("reserve invoice 2"),
            2
        );
        transaction.commit().expect("commit reservations");

        assert_eq!(
            next_number(&connection, &DocumentKind::Quote).expect("read next quote"),
            13
        );
        assert_eq!(
            next_number(&connection, &DocumentKind::Invoice).expect("read next invoice"),
            3
        );
    }

    #[test]
    fn rolling_back_transaction_keeps_number_available() {
        let mut connection = database();
        let transaction = connection.transaction().expect("begin transaction");

        assert_eq!(
            reserve_number(&transaction, &DocumentKind::Quote).expect("reserve quote"),
            10
        );
        transaction.rollback().expect("roll back reservation");

        assert_eq!(
            next_number(&connection, &DocumentKind::Quote).expect("read rolled back quote"),
            10
        );
    }
}
