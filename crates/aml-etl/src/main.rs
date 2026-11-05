//! aml-etl: loads the IBM HI-Small files into Chakshu's Postgres database.
//!
//!   cargo run --release -p aml-etl -- load --data-dir data/raw
//!   cargo run --release -p aml-etl -- load --data-dir data/raw --limit 100000   (quick test)
//!
//! Reads DATABASE_URL from the environment or from `.env`. Every run starts fresh:
//! it replaces the tables' contents with the three files, then builds indexes. The load
//! runs in one transaction, so a failed run leaves the previous data untouched.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Instant;

use aml_core::{parse_ibm_timestamp, to_usd};
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use sqlx::postgres::{PgConnection, PgPoolOptions};

const SCHEMA_SQL: &str = include_str!("../../../db/schema.sql");
const INDEXES_SQL: &str = include_str!("../../../db/indexes.sql");

const TRANSFERS_FILE: &str = "HI-Small_Trans.csv";
const ACCOUNTS_FILE: &str = "HI-Small_accounts.csv";
const PATTERNS_FILE: &str = "HI-Small_Patterns.txt";

/// Rows sent to Postgres per COPY chunk.
const CHUNK_ROWS: usize = 50_000;

#[derive(Parser)]
#[command(about = "Load the IBM AML data into Chakshu's database")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Empty the tables and load the HI-Small files.
    Load {
        /// Folder holding HI-Small_Trans.csv, HI-Small_accounts.csv and HI-Small_Patterns.txt
        #[arg(long, default_value = "data/raw")]
        data_dir: PathBuf,
        /// Load only the first N transfers (for a quick test)
        #[arg(long)]
        limit: Option<usize>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    match cli.command {
        Command::Load { data_dir, limit } => load(&data_dir, limit).await,
    }
}

async fn load(data_dir: &Path, limit: Option<usize>) -> Result<()> {
    let transfers_path = require_file(data_dir, TRANSFERS_FILE)?;
    let accounts_path = require_file(data_dir, ACCOUNTS_FILE)?;
    let patterns_path = require_file(data_dir, PATTERNS_FILE)?;

    let url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set; copy .env.example to .env and fill it in")?;
    let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.context("could not connect to the database; is `docker compose up -d` running?")?;
    let mut conn = pool.acquire().await?;

    println!("Applying schema");
    sqlx::raw_sql(SCHEMA_SQL).execute(&mut *conn).await?;

    // One transaction for the whole load: if anything fails, the previous data stays.
    sqlx::raw_sql("BEGIN").execute(&mut *conn).await?;
    sqlx::raw_sql("TRUNCATE transfers, accounts, pattern_transfers, pattern_attempts RESTART IDENTITY")
        .execute(&mut *conn)
        .await?;

    let started = Instant::now();
    let n = load_transfers(&mut conn, &transfers_path, limit).await?;
    println!("Loaded {n} transfers in {:.0?}", started.elapsed());

    let n = load_accounts(&mut conn, &accounts_path).await?;
    println!("Loaded {n} accounts");

    let (attempts, rows) = load_patterns(&mut conn, &patterns_path).await?;
    println!("Loaded {attempts} labelled laundering attempts ({rows} transfers)");

    sqlx::raw_sql("COMMIT").execute(&mut *conn).await?;

    println!("Building indexes");
    sqlx::raw_sql(INDEXES_SQL).execute(&mut *conn).await?;

    print_summary(&mut conn).await?;
    println!("Done in {:.0?}", started.elapsed());
    Ok(())
}

fn require_file(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    if !path.is_file() {
        bail!("{} not found. Download it from the IBM AML dataset on Kaggle (see data/README.md).", path.display());
    }
    Ok(path)
}

/// HI-Small_Trans.csv columns, by position (the header repeats "Account", so names are ambiguous):
/// Timestamp, From Bank, Account, To Bank, Account, Amount Received, Receiving Currency,
/// Amount Paid, Payment Currency, Payment Format, Is Laundering
async fn load_transfers(conn: &mut PgConnection, path: &Path, limit: Option<usize>) -> Result<usize> {
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_path(path)?;
    let header = reader.headers()?.clone();
    if header.len() != 11 || header.get(0) != Some("Timestamp") || header.get(10) != Some("Is Laundering") {
        bail!("{} has an unexpected header: {:?}", path.display(), header);
    }

    let mut copy = conn
        .copy_in_raw(
            "COPY transfers (ts, from_bank, from_account, to_bank, to_account, amount_received, \
             receiving_currency, amount_paid, payment_currency, payment_format, is_laundering, amount_usd) \
             FROM STDIN WITH (FORMAT csv)",
        )
        .await?;

    let mut chunk = csv::Writer::from_writer(Vec::new());
    let mut count = 0usize;
    for (line, record) in reader.records().enumerate() {
        if limit.is_some_and(|max| count >= max) {
            break;
        }
        let record = record.with_context(|| format!("{} line {}", path.display(), line + 2))?;
        let row = TransferRow::parse(&record).with_context(|| format!("{} line {}", path.display(), line + 2))?;
        row.write(&mut chunk)?;
        count += 1;

        if count % CHUNK_ROWS == 0 {
            let bytes = std::mem::replace(&mut chunk, csv::Writer::from_writer(Vec::new())).into_inner()?;
            copy.send(bytes).await?;
            if count % 500_000 == 0 {
                println!("  {count} transfers");
            }
        }
    }
    copy.send(chunk.into_inner()?).await?;
    copy.finish().await?;
    Ok(count)
}

struct TransferRow<'a> {
    ts: String,
    from_bank: &'a str,
    from_account: &'a str,
    to_bank: &'a str,
    to_account: &'a str,
    amount_received: f64,
    receiving_currency: &'a str,
    amount_paid: f64,
    payment_currency: &'a str,
    payment_format: &'a str,
    is_laundering: bool,
    amount_usd: f64,
}

impl<'a> TransferRow<'a> {
    fn parse(r: &'a csv::StringRecord) -> Result<Self> {
        let field = |i: usize| r.get(i).with_context(|| format!("missing column {i}"));
        let amount_paid: f64 = field(7)?.parse().context("Amount Paid is not a number")?;
        let payment_currency = field(8)?;
        let is_laundering = match field(10)? {
            "1" => true,
            "0" => false,
            other => bail!("Is Laundering must be 0 or 1, got {other:?}"),
        };
        Ok(Self {
            ts: parse_ibm_timestamp(field(0)?)?.format("%Y-%m-%d %H:%M:%S").to_string(),
            from_bank: field(1)?,
            from_account: field(2)?,
            to_bank: field(3)?,
            to_account: field(4)?,
            amount_received: field(5)?.parse().context("Amount Received is not a number")?,
            receiving_currency: field(6)?,
            amount_paid,
            payment_currency,
            payment_format: field(9)?,
            is_laundering,
            amount_usd: to_usd(amount_paid, payment_currency)?,
        })
    }

    fn write(&self, w: &mut csv::Writer<Vec<u8>>) -> Result<()> {
        w.write_record([
            self.ts.as_str(),
            self.from_bank,
            self.from_account,
            self.to_bank,
            self.to_account,
            &self.amount_received.to_string(),
            self.receiving_currency,
            &self.amount_paid.to_string(),
            self.payment_currency,
            self.payment_format,
            if self.is_laundering { "true" } else { "false" },
            &format!("{:.2}", self.amount_usd),
        ])?;
        Ok(())
    }
}

/// HI-Small_accounts.csv: columns are found by name, so their order does not matter.
async fn load_accounts(conn: &mut PgConnection, path: &Path) -> Result<usize> {
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_path(path)?;
    let header = reader.headers()?.clone();
    let col = |name: &str| -> Result<usize> {
        header
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name))
            .with_context(|| format!("{} has no {name:?} column; its header is {:?}", path.display(), header))
    };
    let (account, bank_id, bank_name, entity_id, entity_name) =
        (col("Account Number")?, col("Bank ID")?, col("Bank Name")?, col("Entity ID")?, col("Entity Name")?);

    // Load into a staging table first, then keep one row per account number.
    sqlx::raw_sql("CREATE TEMP TABLE accounts_stage (LIKE accounts) ON COMMIT PRESERVE ROWS")
        .execute(&mut *conn)
        .await?;
    sqlx::raw_sql("ALTER TABLE accounts_stage DROP CONSTRAINT IF EXISTS accounts_stage_pkey")
        .execute(&mut *conn)
        .await?;
    let mut copy = conn
        .copy_in_raw("COPY accounts_stage (account_number, bank_id, bank_name, entity_id, entity_name) FROM STDIN WITH (FORMAT csv)")
        .await?;
    let mut chunk = csv::Writer::from_writer(Vec::new());
    let mut count = 0usize;
    for record in reader.records() {
        let r = record?;
        chunk.write_record([&r[account], &r[bank_id], &r[bank_name], &r[entity_id], &r[entity_name]])?;
        count += 1;
        if count % CHUNK_ROWS == 0 {
            let bytes = std::mem::replace(&mut chunk, csv::Writer::from_writer(Vec::new())).into_inner()?;
            copy.send(bytes).await?;
        }
    }
    copy.send(chunk.into_inner()?).await?;
    copy.finish().await?;

    let kept = sqlx::query(
        "INSERT INTO accounts SELECT DISTINCT ON (account_number) * FROM accounts_stage ORDER BY account_number",
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    sqlx::raw_sql("DROP TABLE accounts_stage").execute(&mut *conn).await?;
    if kept as usize != count {
        println!("  note: {count} rows in the file, {kept} distinct account numbers kept");
    }
    Ok(kept as usize)
}

/// HI-Small_Patterns.txt groups transfers between
///   BEGIN LAUNDERING ATTEMPT - FAN-OUT:  Max 16-degree Fan-Out
///   ...transfer rows in the same CSV format as the transfers file, without a header...
///   END LAUNDERING ATTEMPT - FAN-OUT
async fn load_patterns(conn: &mut PgConnection, path: &Path) -> Result<(usize, usize)> {
    let reader = BufReader::new(File::open(path)?);
    let mut attempts: Vec<(i32, String, String)> = Vec::new();
    let mut rows = csv::Writer::from_writer(Vec::new());
    let mut row_count = 0usize;
    let mut current: Option<i32> = None;

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("BEGIN LAUNDERING ATTEMPT - ") {
            let (pattern, detail) = rest.split_once(':').unwrap_or((rest, ""));
            let id = attempts.len() as i32 + 1;
            attempts.push((id, pattern.trim().to_string(), detail.trim().to_string()));
            current = Some(id);
        } else if line.starts_with("END LAUNDERING ATTEMPT") {
            current = None;
        } else {
            let attempt = current.with_context(|| format!("{} line {}: transfer outside any attempt", path.display(), i + 1))?;
            let record = csv::StringRecord::from(line.split(',').collect::<Vec<_>>());
            let row = TransferRow::parse(&record).with_context(|| format!("{} line {}", path.display(), i + 1))?;
            rows.write_record([
                attempt.to_string().as_str(),
                row.ts.as_str(),
                row.from_bank,
                row.from_account,
                row.to_bank,
                row.to_account,
                &row.amount_paid.to_string(),
                row.payment_currency,
                row.payment_format,
            ])?;
            row_count += 1;
        }
    }

    for (id, pattern, detail) in &attempts {
        sqlx::query("INSERT INTO pattern_attempts (id, pattern, detail) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(pattern)
            .bind(detail)
            .execute(&mut *conn)
            .await?;
    }
    let mut copy = conn
        .copy_in_raw(
            "COPY pattern_transfers (attempt_id, ts, from_bank, from_account, to_bank, to_account, \
             amount_paid, payment_currency, payment_format) FROM STDIN WITH (FORMAT csv)",
        )
        .await?;
    copy.send(rows.into_inner()?).await?;
    copy.finish().await?;
    Ok((attempts.len(), row_count))
}

async fn print_summary(conn: &mut PgConnection) -> Result<()> {
    let (total, laundering, first, last): (i64, i64, Option<chrono::NaiveDateTime>, Option<chrono::NaiveDateTime>) =
        sqlx::query_as("SELECT COUNT(*), COUNT(*) FILTER (WHERE is_laundering), MIN(ts), MAX(ts) FROM transfers")
            .fetch_one(&mut *conn)
            .await?;
    println!("\nSummary");
    println!("  transfers:  {total} ({laundering} laundering, 1 in {})", if laundering > 0 { total / laundering } else { 0 });
    if let (Some(first), Some(last)) = (first, last) {
        println!("  time span:  {first} to {last}");
    }
    let patterns: Vec<(String, i64)> =
        sqlx::query_as("SELECT pattern, COUNT(*) FROM pattern_attempts GROUP BY pattern ORDER BY pattern")
            .fetch_all(&mut *conn)
            .await?;
    let list: Vec<String> = patterns.iter().map(|(p, n)| format!("{p} {n}")).collect();
    println!("  patterns:   {}", list.join(", "));
    Ok(())
}
