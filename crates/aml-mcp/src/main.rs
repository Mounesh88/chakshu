//! aml-mcp: the read-only MCP server every Chakshu agent uses to reach bank data.
//!
//!   cargo run --release -p aml-mcp          serves http://127.0.0.1:8090/mcp
//!
//! Every tool only reads. None of them can close a case, file a report or contact a
//! customer, and none of them returns IBM's ground-truth labels (see `no_ground_truth`
//! test at the bottom).

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime};
use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    model::{ServerCapabilities, ServerConfig},
    schemars, tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Maximum rows any tool returns in one call.
const MAX_ROWS: i64 = 200;

#[derive(Clone)]
pub struct Bank {
    db: PgPool,
}

// ---------------------------------------------------------------------------------------
// Tool inputs
// ---------------------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAlertsRequest {
    #[schemars(description = "How many alerts to return, newest first (default 20, max 200)")]
    pub limit: Option<i64>,
    #[schemars(description = "Only alerts that fired this rule, e.g. R1_RAPID_INFLOW")]
    pub rule: Option<String>,
    #[schemars(description = "Only alerts for this day, YYYY-MM-DD")]
    pub day: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AlertRequest {
    #[schemars(description = "Alert id, e.g. 4471")]
    pub alert_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AccountRequest {
    #[schemars(description = "Account number, e.g. 8139F54E0")]
    pub account_number: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AccountDayRequest {
    #[schemars(description = "Account number, e.g. 8139F54E0")]
    pub account_number: String,
    #[schemars(description = "Day to look at, YYYY-MM-DD (usually the alert day)")]
    pub day: String,
    #[schemars(description = "Also include this many days before `day` (default 0, max 7)")]
    pub days_before: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LoginRequest {
    #[schemars(description = "Account number, e.g. 8139F54E0")]
    pub account_number: String,
    #[schemars(description = "Look at events before this moment, YYYY-MM-DD HH:MM (default: end of data)")]
    pub before: Option<String>,
    #[schemars(description = "How many hours back from `before` (default 72, max 720)")]
    pub hours: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenRequest {
    #[schemars(description = "A person or company name to screen. Give this or account_number.")]
    pub name: Option<String>,
    #[schemars(description = "Screen the customer who owns this account. Give this or name.")]
    pub account_number: Option<String>,
    #[schemars(description = "Minimum whole-name similarity 0..1 to report a match (default 0.85)")]
    pub min_score: Option<f64>,
}

// ---------------------------------------------------------------------------------------
// Rows returned (no ground-truth columns anywhere)
// ---------------------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct AlertRow {
    id: i64,
    account_number: String,
    alert_day: NaiveDate,
    rules: Vec<String>,
    total_in_usd: f64,
    total_out_usd: f64,
    senders: i32,
    receivers: i32,
    near_threshold_count: i32,
    first_ts: NaiveDateTime,
    last_ts: NaiveDateTime,
    alerted_at: NaiveDateTime,
    customer_name: Option<String>,
    entity_type: Option<String>,
    industry: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct TransferRow {
    ts: NaiveDateTime,
    direction: String,
    counterparty: String,
    counterparty_bank: String,
    amount: f64,
    currency: String,
    amount_usd: f64,
    payment_format: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct CounterpartyRow {
    counterparty: String,
    direction: String,
    transfers: i64,
    total_usd: f64,
    customer_name: Option<String>,
    entity_type: Option<String>,
    home_country: Option<String>,
    opened_on: Option<NaiveDate>,
    account_age_days: Option<i32>,
}

#[derive(Serialize, sqlx::FromRow)]
struct ProfileRow {
    account_number: String,
    display_name: String,
    entity_type: String,
    industry: String,
    home_country: String,
    expected_monthly_usd: f64,
    opened_on: NaiveDate,
    risk_rating: String,
    bank_name: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct LoginRow {
    ts: NaiveDateTime,
    event_type: String,
    device_id: String,
    ip_country: String,
    is_new_device: bool,
}

// ---------------------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------------------

/// The alert columns agents may see (no ground truth). A macro, so queries stay static
/// strings, which sqlx requires as protection against SQL injection.
macro_rules! alert_select {
    () => {
        "SELECT a.id, a.account_number, a.alert_day, a.rules, \
         round(a.total_in_usd::numeric, 2)::float8 AS total_in_usd, \
         round(a.total_out_usd::numeric, 2)::float8 AS total_out_usd, \
         a.senders, a.receivers, a.near_threshold_count, a.first_ts, a.last_ts, a.alerted_at, \
         p.display_name AS customer_name, p.entity_type, p.industry \
         FROM alerts a LEFT JOIN customer_profiles p USING (account_number) "
    };
}

#[tool_router]
impl Bank {
    #[tool(description = "List alerts raised by the bank's rules engine, newest first. Each alert is one account on one day, with the rules that fired and the money in and out.")]
    async fn list_alerts(&self, Parameters(req): Parameters<ListAlertsRequest>) -> Result<String, String> {
        let limit = req.limit.unwrap_or(20).clamp(1, MAX_ROWS);
        let day = req.day.as_deref().map(parse_day).transpose()?;
        let rows: Vec<AlertRow> = sqlx::query_as(concat!(
            alert_select!(),
            "WHERE ($1::text IS NULL OR $1 = ANY(a.rules)) AND ($2::date IS NULL OR a.alert_day = $2) \
             ORDER BY a.alerted_at DESC, a.id DESC LIMIT $3"
        ))
            .bind(&req.rule)
            .bind(day)
            .bind(limit)
            .fetch_all(&self.db)
            .await
            .map_err(db_error)?;
        to_json(&rows)
    }

    #[tool(description = "Get one alert: the account, the day, the rules that fired, money in and out, number of senders and receivers, and who the customer is.")]
    async fn get_alert(&self, Parameters(req): Parameters<AlertRequest>) -> Result<String, String> {
        let row: Option<AlertRow> = sqlx::query_as(concat!(alert_select!(), "WHERE a.id = $1")).bind(req.alert_id).fetch_optional(&self.db).await.map_err(db_error)?;
        match row {
            Some(r) => to_json(&r),
            None => Err(format!("no alert with id {}", req.alert_id)),
        }
    }

    #[tool(description = "List an account's transfers in and out on a day (optionally plus days before), oldest first, with amounts in the original currency and in USD.")]
    async fn get_transactions(&self, Parameters(req): Parameters<AccountDayRequest>) -> Result<String, String> {
        let (from, to) = day_window(&req.day, req.days_before)?;
        let rows: Vec<TransferRow> = sqlx::query_as(
            "SELECT ts, 'out' AS direction, to_account AS counterparty, to_bank AS counterparty_bank,
                    amount_paid AS amount, payment_currency AS currency, amount_usd, payment_format
             FROM transfers WHERE from_account = $1 AND ts >= $2 AND ts < $3 AND to_account <> from_account
             UNION ALL
             SELECT ts, 'in', from_account, from_bank, amount_paid, payment_currency, amount_usd, payment_format
             FROM transfers WHERE to_account = $1 AND ts >= $2 AND ts < $3 AND to_account <> from_account
             ORDER BY ts LIMIT $4",
        )
        .bind(&req.account_number)
        .bind(from)
        .bind(to)
        .bind(MAX_ROWS + 1)
        .fetch_all(&self.db)
        .await
        .map_err(db_error)?;
        let truncated = rows.len() as i64 > MAX_ROWS;
        let rows: Vec<_> = rows.into_iter().take(MAX_ROWS as usize).collect();
        to_json(&serde_json::json!({
            "account_number": req.account_number,
            "from": from, "to": to,
            "count": rows.len(), "truncated": truncated,
            "transfers": rows,
        }))
    }

    #[tool(description = "Who an account sent money to and received money from on a day: totals per counterparty, and each counterparty's customer type, country and account age (new accounts are a warning sign).")]
    async fn get_counterparties(&self, Parameters(req): Parameters<AccountDayRequest>) -> Result<String, String> {
        let (from, to) = day_window(&req.day, req.days_before)?;
        let rows: Vec<CounterpartyRow> = sqlx::query_as(
            "WITH m AS (
                SELECT to_account AS counterparty, 'out' AS direction, amount_usd FROM transfers
                WHERE from_account = $1 AND ts >= $2 AND ts < $3 AND to_account <> from_account
                UNION ALL
                SELECT from_account, 'in', amount_usd FROM transfers
                WHERE to_account = $1 AND ts >= $2 AND ts < $3 AND to_account <> from_account
             )
             SELECT m.counterparty, m.direction, COUNT(*) AS transfers, round(SUM(m.amount_usd)::numeric, 2)::float8 AS total_usd,
                    p.display_name AS customer_name, p.entity_type, p.home_country, p.opened_on,
                    ($2::date - p.opened_on) AS account_age_days
             FROM m LEFT JOIN customer_profiles p ON p.account_number = m.counterparty
             GROUP BY 1, 2, 5, 6, 7, 8, 9 ORDER BY total_usd DESC LIMIT $4",
        )
        .bind(&req.account_number)
        .bind(from)
        .bind(to)
        .bind(MAX_ROWS)
        .fetch_all(&self.db)
        .await
        .map_err(db_error)?;
        let new_accounts = rows.iter().filter(|r| r.account_age_days.is_some_and(|d| d <= 90)).count();
        to_json(&serde_json::json!({
            "account_number": req.account_number,
            "from": from, "to": to,
            "counterparties": rows.len(),
            "counterparties_opened_in_last_90_days": new_accounts,
            "rows": rows,
        }))
    }

    #[tool(description = "The customer behind an account (what the bank recorded at onboarding: name, type, industry, country, expected monthly activity, opening date, risk rating), compared with what the account actually moved.")]
    async fn get_customer_profile(&self, Parameters(req): Parameters<AccountRequest>) -> Result<String, String> {
        let profile: Option<ProfileRow> = sqlx::query_as(
            "SELECT p.account_number, p.display_name, p.entity_type, p.industry, p.home_country,
                    round(p.expected_monthly_usd::numeric, 2)::float8 AS expected_monthly_usd, p.opened_on, p.risk_rating, a.bank_name
             FROM customer_profiles p LEFT JOIN accounts a USING (account_number) WHERE p.account_number = $1",
        )
        .bind(&req.account_number)
        .fetch_optional(&self.db)
        .await
        .map_err(db_error)?;
        let Some(profile) = profile else {
            return Err(format!("no customer profile for account {}", req.account_number));
        };
        let (moved, first, last): (Option<f64>, Option<NaiveDateTime>, Option<NaiveDateTime>) = sqlx::query_as(
            "SELECT SUM(amount_usd), MIN(ts), MAX(ts) FROM transfers
             WHERE (from_account = $1 OR to_account = $1) AND from_account <> to_account",
        )
        .bind(&req.account_number)
        .fetch_one(&self.db)
        .await
        .map_err(db_error)?;
        let moved = moved.unwrap_or(0.0);
        // The data covers Sep 1-18, 2022: scale the observed total to a month to compare.
        let monthly_equivalent = moved * 30.0 / 18.0;
        to_json(&serde_json::json!({
            "profile": profile,
            "actual_activity": {
                "total_moved_usd": round2(moved),
                "first_transfer": first, "last_transfer": last,
                "monthly_equivalent_usd": round2(monthly_equivalent),
                "times_expected": round2(monthly_equivalent / profile.expected_monthly_usd),
            },
        }))
    }

    #[tool(description = "Logins and password resets on an account before a moment (usually the first suspicious transfer). Flags new devices and logins from outside the customer's home country.")]
    async fn get_login_events(&self, Parameters(req): Parameters<LoginRequest>) -> Result<String, String> {
        let before = match req.before.as_deref() {
            Some(text) => parse_moment(text)?,
            None => NaiveDate::from_ymd_opt(2022, 9, 19).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        };
        let hours = req.hours.unwrap_or(72).clamp(1, 720);
        let since = before - chrono::Duration::hours(hours);
        let home: Option<String> = sqlx::query_scalar("SELECT home_country FROM customer_profiles WHERE account_number = $1")
            .bind(&req.account_number)
            .fetch_optional(&self.db)
            .await
            .map_err(db_error)?;
        let rows: Vec<LoginRow> = sqlx::query_as(
            "SELECT ts, event_type, device_id, ip_country, is_new_device FROM login_events
             WHERE account_number = $1 AND ts >= $2 AND ts < $3 ORDER BY ts LIMIT $4",
        )
        .bind(&req.account_number)
        .bind(since)
        .bind(before)
        .bind(MAX_ROWS)
        .fetch_all(&self.db)
        .await
        .map_err(db_error)?;
        let foreign: HashSet<&str> = rows
            .iter()
            .filter(|r| home.as_deref().is_some_and(|h| h != r.ip_country))
            .map(|r| r.ip_country.as_str())
            .collect();
        to_json(&serde_json::json!({
            "account_number": req.account_number,
            "home_country": home,
            "from": since, "to": before,
            "events": rows.len(),
            "new_device_events": rows.iter().filter(|r| r.is_new_device).count(),
            "password_resets": rows.iter().filter(|r| r.event_type == "password_reset").count(),
            "countries_outside_home": foreign,
            "rows": rows,
        }))
    }

    #[tool(description = "Screen a name (or the customer behind an account) against the watchlist, with fuzzy matching so near-spellings are caught. The watchlist in this demo is fictional.")]
    async fn screen_watchlist(&self, Parameters(req): Parameters<ScreenRequest>) -> Result<String, String> {
        let name = match (&req.name, &req.account_number) {
            (Some(n), _) => n.clone(),
            (None, Some(acct)) => sqlx::query_scalar::<_, String>("SELECT display_name FROM customer_profiles WHERE account_number = $1")
                .bind(acct)
                .fetch_optional(&self.db)
                .await
                .map_err(db_error)?
                .ok_or_else(|| format!("no customer profile for account {acct}"))?,
            (None, None) => return Err("give a name or an account_number".into()),
        };
        let min_score = req.min_score.unwrap_or(0.85).clamp(0.5, 1.0);
        let list: Vec<(String, String, String)> = sqlx::query_as("SELECT name, list_name, reason FROM watchlist")
            .fetch_all(&self.db)
            .await
            .map_err(db_error)?;
        let wanted = normalize(&name);
        let mut matches: Vec<serde_json::Value> = list
            .iter()
            .map(|(n, l, r)| (strsim::normalized_levenshtein(&wanted, &normalize(n)), n, l, r))
            .filter(|(score, ..)| *score >= min_score)
            .map(|(score, n, l, r)| serde_json::json!({"listed_name": n, "list": l, "reason": r, "similarity": round2(score)}))
            .collect();
        matches.sort_by(|a, b| b["similarity"].as_f64().partial_cmp(&a["similarity"].as_f64()).unwrap());
        to_json(&serde_json::json!({
            "screened_name": name,
            "min_score": min_score,
            "matches": matches,
            "note": "A fuzzy match is a lead for a person to check, not a confirmed hit.",
        }))
    }

    #[tool(description = "Detect money-flow patterns around an account on a day: fan-in (many senders), fan-out (many receivers), pass-through (money in goes straight out) and cycles (money returns to where it started), using the account's 2-hop network.")]
    async fn detect_patterns(&self, Parameters(req): Parameters<AccountDayRequest>) -> Result<String, String> {
        let (from, to) = day_window(&req.day, req.days_before)?;
        // The account's network: its counterparties and theirs (capped), then every transfer
        // between accounts in that network during the window.
        let edges: Vec<(String, String, f64)> = sqlx::query_as(
            "WITH w AS (
                SELECT from_account, to_account, amount_usd FROM transfers
                WHERE ts >= $2 AND ts < $3 AND from_account <> to_account
             ),
             n1 AS (
                SELECT to_account AS a FROM w WHERE from_account = $1
                UNION SELECT from_account FROM w WHERE to_account = $1
                UNION SELECT $1
             ),
             n2 AS (
                SELECT a FROM n1
                UNION (SELECT to_account FROM w WHERE from_account IN (SELECT a FROM n1)
                       UNION SELECT from_account FROM w WHERE to_account IN (SELECT a FROM n1)
                       LIMIT 2000)
             )
             SELECT from_account, to_account, amount_usd FROM w
             WHERE from_account IN (SELECT a FROM n2) AND to_account IN (SELECT a FROM n2)",
        )
        .bind(&req.account_number)
        .bind(from)
        .bind(to)
        .fetch_all(&self.db)
        .await
        .map_err(db_error)?;
        let edges: Vec<aml_graph::Edge> =
            edges.into_iter().map(|(from, to, amount_usd)| aml_graph::Edge { from, to, amount_usd }).collect();
        let report = aml_graph::analyze(&req.account_number, &edges, &aml_graph::Thresholds::default());
        to_json(&serde_json::json!({
            "from": from, "to": to,
            "network_transfers_examined": edges.len(),
            "report": report,
        }))
    }
}

#[tool_handler]
impl ServerHandler for Bank {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Chakshu bank data, read-only. Use these tools to investigate anti-money-laundering \
             alerts: the alert, the account's transfers and counterparties, the customer profile, \
             login events, watchlist screening and money-flow patterns. All data is synthetic. \
             You can read and recommend; only people make decisions.",
        )
    }
}

// ---------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------

fn parse_day(text: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").map_err(|_| format!("day {text:?} must be YYYY-MM-DD"))
}

fn parse_moment(text: &str) -> Result<NaiveDateTime, String> {
    let t = text.trim();
    NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| format!("moment {text:?} must be YYYY-MM-DD HH:MM"))
}

fn day_window(day: &str, days_before: Option<i64>) -> Result<(NaiveDateTime, NaiveDateTime), String> {
    let day = parse_day(day)?;
    let back = days_before.unwrap_or(0).clamp(0, 7);
    let from = (day - chrono::Duration::days(back)).and_hms_opt(0, 0, 0).unwrap();
    let to = (day + chrono::Duration::days(1)).and_hms_opt(0, 0, 0).unwrap();
    Ok((from, to))
}

/// Lowercase, keep letters, digits and single spaces, so "Blue  River & Co." ~ "blue river co".
fn normalize(name: &str) -> String {
    let kept: String = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn to_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

fn db_error(e: sqlx::Error) -> String {
    format!("database error: {e}")
}

// ---------------------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set; copy .env.example to .env and fill it in")?;
    let bind = std::env::var("MCP_BIND").unwrap_or_else(|_| "127.0.0.1:8090".to_string());
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .context("could not connect to the database; is `docker compose up -d` running?")?;

    let service = StreamableHttpService::new(
        move || Ok(Bank { db: db.clone() }),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("could not listen on {bind}"))?;
    println!("Chakshu MCP server: http://{bind}/mcp  (read-only, 8 tools). Press Ctrl+C to stop.");
    axum::serve(listener, router).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Agents must never see IBM's ground truth. Fail the build if this crate's source
    /// mentions either label column outside this test.
    #[test]
    fn no_ground_truth() {
        let source = include_str!("main.rs");
        let body = source.split("#[cfg(test)]").next().unwrap();
        for column in [concat!("is_", "laundering"), concat!("is_", "true_positive")] {
            assert!(!body.contains(column), "aml-mcp must not read {column}");
        }
    }

    #[test]
    fn watchlist_similarity_catches_near_spellings_only() {
        let score = |a: &str, b: &str| strsim::normalized_levenshtein(&super::normalize(a), &super::normalize(b));
        assert!(score("Evergreen Bridge Holdings", "Evergreens Bridge Holdings") >= 0.85);
        assert!(score("Evergreen Harvest & Co", "Evergreens Bridge Holdings") < 0.85);
        assert_eq!(super::normalize("Blue  River & Co."), "blue river co");
    }

    #[test]
    fn parses_days_and_windows() {
        let (from, to) = super::day_window("2022-09-05", Some(2)).unwrap();
        assert_eq!(from.to_string(), "2022-09-03 00:00:00");
        assert_eq!(to.to_string(), "2022-09-06 00:00:00");
        assert!(super::parse_day("05/09/2022").is_err());
        assert!(super::parse_moment("2022-09-05 14:30").is_ok());
    }
}
