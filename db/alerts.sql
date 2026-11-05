-- Bank-style alert rules. Run by `aml-etl alerts`, which sets the chakshu.* thresholds first.
--
-- Like a real bank's rules engine, these are simple and noisy on purpose: they look at one
-- account on one day and fire on behaviour that *can* indicate laundering. Most alerts they
-- raise are innocent. Sorting real from noise is the job of the room.
--
-- One alert per account per day, listing every rule that fired.

CREATE TABLE IF NOT EXISTS alerts (
    id                   BIGSERIAL PRIMARY KEY,
    account_number       TEXT             NOT NULL,
    alert_day            DATE             NOT NULL,
    rules                TEXT[]           NOT NULL,
    total_in_usd         DOUBLE PRECISION NOT NULL,
    total_out_usd        DOUBLE PRECISION NOT NULL,
    senders              INTEGER          NOT NULL,
    receivers            INTEGER          NOT NULL,
    near_threshold_count INTEGER          NOT NULL,
    first_ts             TIMESTAMP        NOT NULL,
    last_ts              TIMESTAMP        NOT NULL,
    alerted_at           TIMESTAMP        NOT NULL,
    -- Ground truth from IBM's labels: did this account move laundered money that day?
    -- Used only to train and evaluate the model. Never exposed to agents.
    is_true_positive     BOOLEAN          NOT NULL,
    UNIQUE (account_number, alert_day)
);

TRUNCATE alerts RESTART IDENTITY;

WITH moves AS (
    -- Self-transfers (an account paying itself) are bookkeeping, not money movement.
    SELECT ts, ts::date AS day, from_account, to_account, amount_usd
    FROM transfers
    WHERE from_account <> to_account
),
inflow AS (
    SELECT to_account AS account, day, COUNT(DISTINCT from_account) AS senders,
           SUM(amount_usd) AS total_in, MIN(ts) AS first_ts, MAX(ts) AS last_ts
    FROM moves GROUP BY 1, 2
),
outflow AS (
    SELECT from_account AS account, day, COUNT(DISTINCT to_account) AS receivers,
           SUM(amount_usd) AS total_out, MIN(ts) AS first_ts, MAX(ts) AS last_ts,
           COUNT(*) FILTER (WHERE amount_usd >= current_setting('chakshu.near_low')::float8
                              AND amount_usd <  current_setting('chakshu.near_high')::float8) AS near_threshold
    FROM moves GROUP BY 1, 2
),
daily AS (
    SELECT COALESCE(i.account, o.account) AS account,
           COALESCE(i.day, o.day)         AS day,
           COALESCE(i.senders, 0)         AS senders,
           COALESCE(o.receivers, 0)       AS receivers,
           COALESCE(i.total_in, 0)        AS total_in,
           COALESCE(o.total_out, 0)       AS total_out,
           COALESCE(o.near_threshold, 0)  AS near_threshold,
           LEAST(i.first_ts, o.first_ts)  AS first_ts,
           GREATEST(i.last_ts, o.last_ts) AS last_ts
    FROM inflow i
    FULL JOIN outflow o ON o.account = i.account AND o.day = i.day
),
fired AS (
    SELECT d.*,
        array_remove(ARRAY[
            CASE WHEN senders >= current_setting('chakshu.fan_min')::int
                  AND total_in >= current_setting('chakshu.min_usd')::float8
                 THEN 'R1_RAPID_INFLOW' END,
            CASE WHEN receivers >= current_setting('chakshu.fan_min')::int
                  AND total_out >= current_setting('chakshu.min_usd')::float8
                 THEN 'R2_RAPID_OUTFLOW' END,
            CASE WHEN total_in >= current_setting('chakshu.min_usd')::float8
                  AND total_out BETWEEN total_in * current_setting('chakshu.pass_ratio')::float8
                                    AND total_in * (2 - current_setting('chakshu.pass_ratio')::float8)
                 THEN 'R3_PASS_THROUGH' END,
            CASE WHEN near_threshold >= current_setting('chakshu.near_count')::int
                 THEN 'R4_STRUCTURING' END
        ], NULL) AS rules
    FROM daily d
),
laundering_days AS (
    SELECT from_account AS account, ts::date AS day FROM transfers WHERE is_laundering
    UNION
    SELECT to_account, ts::date FROM transfers WHERE is_laundering
)
INSERT INTO alerts (account_number, alert_day, rules, total_in_usd, total_out_usd, senders,
                    receivers, near_threshold_count, first_ts, last_ts, alerted_at, is_true_positive)
SELECT f.account, f.day, f.rules, f.total_in, f.total_out, f.senders, f.receivers,
       f.near_threshold, f.first_ts, f.last_ts,
       -- the rules engine runs overnight: alerts appear at 06:00 the next morning
       f.day + INTERVAL '1 day 6 hours',
       (l.account IS NOT NULL)
FROM fired f
LEFT JOIN laundering_days l ON l.account = f.account AND l.day = f.day
WHERE cardinality(f.rules) > 0;

CREATE INDEX IF NOT EXISTS alerts_account ON alerts (account_number);
ANALYZE alerts;
