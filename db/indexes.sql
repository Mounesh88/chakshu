-- Indexes are built after the bulk load, which is much faster than loading into
-- indexed tables.

CREATE INDEX IF NOT EXISTS transfers_from_account_ts ON transfers (from_account, ts);
CREATE INDEX IF NOT EXISTS transfers_to_account_ts   ON transfers (to_account, ts);
CREATE INDEX IF NOT EXISTS transfers_ts              ON transfers (ts);
CREATE INDEX IF NOT EXISTS pattern_transfers_attempt ON pattern_transfers (attempt_id);
CREATE INDEX IF NOT EXISTS pattern_transfers_accounts ON pattern_transfers (from_account, to_account);

ANALYZE transfers;
ANALYZE accounts;
ANALYZE pattern_transfers;
