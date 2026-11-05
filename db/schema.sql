-- Chakshu database schema. Applied by `aml-etl` on every run; every statement is
-- safe to run again.

-- One row per transfer from the IBM HI-Small file.
CREATE TABLE IF NOT EXISTS transfers (
    id                 BIGSERIAL PRIMARY KEY,
    ts                 TIMESTAMP        NOT NULL,
    from_bank          TEXT             NOT NULL,
    from_account       TEXT             NOT NULL,
    to_bank            TEXT             NOT NULL,
    to_account         TEXT             NOT NULL,
    amount_received    DOUBLE PRECISION NOT NULL,
    receiving_currency TEXT             NOT NULL,
    amount_paid        DOUBLE PRECISION NOT NULL,
    payment_currency   TEXT             NOT NULL,
    payment_format     TEXT             NOT NULL,
    is_laundering      BOOLEAN          NOT NULL,
    -- amount_paid converted with the fixed approximate rates in aml-etl
    amount_usd         DOUBLE PRECISION NOT NULL
);

-- One row per account from HI-Small_accounts.csv.
CREATE TABLE IF NOT EXISTS accounts (
    account_number TEXT PRIMARY KEY,
    bank_id        TEXT NOT NULL,
    bank_name      TEXT NOT NULL,
    entity_id      TEXT NOT NULL,
    entity_name    TEXT NOT NULL
);

-- The labelled laundering attempts from HI-Small_Patterns.txt.
CREATE TABLE IF NOT EXISTS pattern_attempts (
    id      INTEGER PRIMARY KEY,
    pattern TEXT NOT NULL,   -- FAN-IN, FAN-OUT, CYCLE, GATHER-SCATTER, ...
    detail  TEXT NOT NULL    -- e.g. "Max 16-degree Fan-Out"
);

CREATE TABLE IF NOT EXISTS pattern_transfers (
    attempt_id       INTEGER          NOT NULL REFERENCES pattern_attempts (id),
    ts               TIMESTAMP        NOT NULL,
    from_bank        TEXT             NOT NULL,
    from_account     TEXT             NOT NULL,
    to_bank          TEXT             NOT NULL,
    to_account       TEXT             NOT NULL,
    amount_paid      DOUBLE PRECISION NOT NULL,
    payment_currency TEXT             NOT NULL,
    payment_format   TEXT             NOT NULL
);
