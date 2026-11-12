//! Rows in the text format of `COPY … FROM STDIN`: the fastest way to load PostgreSQL.

use std::fmt::Write;

use sqlx::{AssertSqlSafe, PgConnection};
use time::{Date, OffsetDateTime};

/// Rows of one table, ready to send.
#[derive(Debug)]
pub struct CopyBuf {
    /// `COPY table (columns) FROM STDIN`.
    statement: &'static str,
    out: String,
    at_row_start: bool,
    rows: u64,
}

impl CopyBuf {
    pub fn new(statement: &'static str) -> Self {
        Self {
            statement,
            out: String::new(),
            at_row_start: true,
            rows: 0,
        }
    }

    pub fn rows(&self) -> u64 {
        self.rows
    }

    fn separate(&mut self) {
        if !self.at_row_start {
            self.out.push('\t');
        }
        self.at_row_start = false;
    }

    pub fn int(&mut self, value: i64) -> &mut Self {
        self.separate();
        let _ = write!(self.out, "{value}");
        self
    }

    pub fn opt_int(&mut self, value: Option<i64>) -> &mut Self {
        match value {
            Some(value) => self.int(value),
            None => self.null(),
        }
    }

    /// A `bigint[]`.
    pub fn ints(&mut self, values: &[i64]) -> &mut Self {
        self.separate();
        self.out.push('{');
        for (n, value) in values.iter().enumerate() {
            if n > 0 {
                self.out.push(',');
            }
            let _ = write!(self.out, "{value}");
        }
        self.out.push('}');
        self
    }

    pub fn float(&mut self, value: f64) -> &mut Self {
        self.separate();
        let _ = write!(self.out, "{value}");
        self
    }

    pub fn bool(&mut self, value: bool) -> &mut Self {
        self.separate();
        self.out.push(if value { 't' } else { 'f' });
        self
    }

    pub fn null(&mut self) -> &mut Self {
        self.separate();
        self.out.push_str("\\N");
        self
    }

    pub fn text(&mut self, value: &str) -> &mut Self {
        self.separate();
        for c in value.chars() {
            match c {
                '\\' => self.out.push_str("\\\\"),
                '\t' => self.out.push_str("\\t"),
                '\n' => self.out.push_str("\\n"),
                '\r' => self.out.push_str("\\r"),
                c => self.out.push(c),
            }
        }
        self
    }

    pub fn opt_text(&mut self, value: Option<&str>) -> &mut Self {
        match value {
            Some(value) => self.text(value),
            None => self.null(),
        }
    }

    /// A `timestamptz` from Unix seconds.
    pub fn time(&mut self, unix: i64) -> &mut Self {
        self.separate();
        let at = OffsetDateTime::from_unix_timestamp(unix).unwrap_or(OffsetDateTime::UNIX_EPOCH);
        let _ = write!(
            self.out,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}+00",
            at.year(),
            u8::from(at.month()),
            at.day(),
            at.hour(),
            at.minute(),
            at.second()
        );
        self
    }

    pub fn opt_time(&mut self, unix: Option<i64>) -> &mut Self {
        match unix {
            Some(unix) => self.time(unix),
            None => self.null(),
        }
    }

    /// A `date` from Unix seconds.
    pub fn date(&mut self, unix: i64) -> &mut Self {
        self.separate();
        let date: Date = OffsetDateTime::from_unix_timestamp(unix)
            .unwrap_or(OffsetDateTime::UNIX_EPOCH)
            .date();
        let _ = write!(
            self.out,
            "{:04}-{:02}-{:02}",
            date.year(),
            u8::from(date.month()),
            date.day()
        );
        self
    }

    pub fn end(&mut self) {
        self.out.push('\n');
        self.at_row_start = true;
        self.rows += 1;
    }

    /// Sends the rows over `conn` and empties the buffer.
    pub async fn send(&mut self, conn: &mut PgConnection) -> sqlx::Result<u64> {
        if self.rows == 0 {
            return Ok(0);
        }
        let mut copy = conn.copy_in_raw(self.statement).await?;
        copy.send(self.out.as_bytes()).await?;
        let rows = copy.finish().await?;
        self.out.clear();
        self.rows = 0;
        Ok(rows)
    }
}

/// Runs a fixed statement of the benchmark (not one built from input).
pub async fn execute(conn: &mut PgConnection, sql: &'static str) -> sqlx::Result<()> {
    sqlx::raw_sql(sql).execute(conn).await.map(|_| ())
}

/// Runs a statement built from the benchmark's own SQL files and numbers, never from input.
pub async fn execute_built(conn: &mut PgConnection, sql: String) -> sqlx::Result<()> {
    sqlx::raw_sql(AssertSqlSafe(sql))
        .execute(conn)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_tab_separated_and_escaped() {
        let mut buf = CopyBuf::new("copy t from stdin");
        buf.int(1).text("a\tb\\c\nd").null().bool(true).end();
        buf.time(0).date(86_400).float(1.5).end();
        buf.ints(&[]).ints(&[3, 4]).end();
        assert_eq!(
            buf.out,
            "1\ta\\tb\\\\c\\nd\t\\N\tt\n1970-01-01 00:00:00+00\t1970-01-02\t1.5\n{}\t{3,4}\n"
        );
        assert_eq!(buf.rows(), 3);
    }
}
