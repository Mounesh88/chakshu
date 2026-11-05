-- Synthetic data that IBM's dataset does not include. Run by `aml-etl synth` after
-- `aml-etl alerts`. Everything here is made up, and deterministic: the same input data
-- always produces the same profiles, logins and watchlist.
--
-- Some signals are PLANTED on purpose, using IBM's labels, so the agents have something
-- real to find (this is stated in the README):
--   * accounts feeding labelled fan-in / gather-scatter attempts are often newly opened
--   * about 30% of accounts with a real (true-positive) alert show an account takeover
--     (password reset + new device from another country) shortly before the money moved
--   * a few innocent accounts show harmless travel logins, so not every odd login is bad
--   * a few laundering accounts have near-matches on a fictional watchlist

-- Deterministic pseudo-random number in [0, 1) from any text.
CREATE OR REPLACE FUNCTION chakshu_rand(seed TEXT) RETURNS DOUBLE PRECISION
LANGUAGE sql IMMUTABLE AS $$
    SELECT ('x' || '00000000' || substr(md5(seed), 1, 8))::bit(64)::bigint / 4294967296.0
$$;

-- Pick one element of an array, deterministically from a seed.
CREATE OR REPLACE FUNCTION chakshu_pick(seed TEXT, options TEXT[]) RETURNS TEXT
LANGUAGE sql IMMUTABLE AS $$
    SELECT options[1 + floor(chakshu_rand(seed) * cardinality(options))::int]
$$;

------------------------------------------------------------------------------------------
-- Customer profiles: what the bank recorded about each customer when the account opened
------------------------------------------------------------------------------------------
DROP TABLE IF EXISTS customer_profiles;
CREATE TABLE customer_profiles (
    account_number       TEXT PRIMARY KEY,
    display_name         TEXT             NOT NULL,
    entity_type          TEXT             NOT NULL,
    industry             TEXT             NOT NULL,
    home_country         TEXT             NOT NULL,
    expected_monthly_usd DOUBLE PRECISION NOT NULL,
    opened_on            DATE             NOT NULL,
    risk_rating          TEXT             NOT NULL
);

WITH legit AS (
    -- The customer's normal, declared business: their non-laundering activity.
    SELECT account, SUM(amount_usd) AS usd FROM (
        SELECT from_account AS account, amount_usd FROM transfers
        WHERE NOT is_laundering AND from_account <> to_account
        UNION ALL
        SELECT to_account, amount_usd FROM transfers
        WHERE NOT is_laundering AND from_account <> to_account
    ) m GROUP BY account
),
spokes AS (
    -- Accounts that fed money into a labelled fan-in style attempt.
    SELECT DISTINCT pt.from_account AS account
    FROM pattern_transfers pt JOIN pattern_attempts pa ON pa.id = pt.attempt_id
    WHERE pa.pattern IN ('FAN-IN', 'GATHER-SCATTER')
),
base AS (
    SELECT a.account_number,
           a.entity_name,
           CASE WHEN a.entity_name ~ ' #\d+$' THEN split_part(a.entity_name, ' #', 1) ELSE 'Other' END AS entity_type,
           COALESCE(l.usd, 0) AS legit_usd,
           (s.account IS NOT NULL) AS is_spoke
    FROM accounts a
    LEFT JOIN legit l ON l.account = a.account_number
    LEFT JOIN spokes s ON s.account = a.account_number
)
INSERT INTO customer_profiles
SELECT account_number,
       CASE WHEN entity_type = 'Individual' THEN
            chakshu_pick(account_number || 'first', ARRAY['Aisha','Leo','Maya','Chen','Priya','Sam','Elena','Omar','Grace','Ravi','Sofia','Daniel','Amara','Kenji','Lucia','Noah'])
            || ' ' ||
            chakshu_pick(account_number || 'last', ARRAY['Khan','Martin','Okafor','Wei','Sharma','Lopez','Novak','Haddad','Kim','Fischer','Rossi','Mensah','Tanaka','Silva','Patel','Brooks'])
       ELSE
            chakshu_pick(account_number || 'adj', ARRAY['Blue','Golden','Northern','Silver','Green','Summit','Harbor','Cedar','Bright','Coastal','Pioneer','Maple','Atlas','Riverside','Crescent','Evergreen'])
            || ' ' ||
            chakshu_pick(account_number || 'noun', ARRAY['River','Oak','Bridge','Valley','Stone','Field','Star','Peak','Lake','Forge','Garden','Harvest','Ridge','Anchor','Lantern','Meadow'])
            || ' ' ||
            chakshu_pick(account_number || 'suffix', ARRAY['Trading LLC','Holdings','& Co','Partners','Supplies Ltd','Group','Services LLC','Imports'])
       END,
       entity_type,
       CASE WHEN entity_type = 'Individual' THEN 'Personal account'
            ELSE chakshu_pick(account_number || 'industry', ARRAY['Restaurant','Grocery','Construction','Consulting','Import/Export','Auto repair','Software','Logistics','Clothing retail','Real estate','Medical clinic','Travel agency'])
       END,
       chakshu_pick(account_number || 'country', ARRAY['United States','United States','United States','United Kingdom','Germany','India','China','Japan','Brazil','Mexico','Canada','Australia']),
       -- declared expected activity: their normal 18-day activity scaled to a month, give or take
       GREATEST(1000, legit_usd * 30.0 / 18.0 * (0.7 + 0.7 * chakshu_rand(account_number || 'expected'))),
       CASE WHEN is_spoke AND chakshu_rand(account_number || 'new') < 0.6
            THEN DATE '2022-09-01' - (10 + floor(50 * chakshu_rand(account_number || 'age')))::int
            ELSE DATE '2022-09-01' - (365 + floor(14 * 365 * chakshu_rand(account_number || 'age')))::int
       END,
       chakshu_pick(account_number || 'risk', ARRAY['Low','Low','Low','Low','Medium','Medium','High'])
FROM base;

------------------------------------------------------------------------------------------
-- Login events, for every account that has an alert
------------------------------------------------------------------------------------------
DROP TABLE IF EXISTS login_events;
CREATE TABLE login_events (
    id             BIGSERIAL PRIMARY KEY,
    account_number TEXT      NOT NULL,
    ts             TIMESTAMP NOT NULL,
    event_type     TEXT      NOT NULL,   -- login | password_reset
    device_id      TEXT      NOT NULL,
    ip_country     TEXT      NOT NULL,
    is_new_device  BOOLEAN   NOT NULL
);

CREATE TEMP TABLE alerted_accounts AS
SELECT a.account_number,
       p.home_country,
       bool_or(a.is_true_positive) AS has_true_alert,
       MIN(a.first_ts) FILTER (WHERE a.is_true_positive) AS first_true_ts
FROM alerts a JOIN customer_profiles p USING (account_number)
GROUP BY a.account_number, p.home_country;

-- Normal logins: 3 to 8 over the period, from home, on the customer's usual device.
INSERT INTO login_events (account_number, ts, event_type, device_id, ip_country, is_new_device)
SELECT account_number,
       date_trunc('minute', TIMESTAMP '2022-09-01' + (chakshu_rand(account_number || 'login' || n) * INTERVAL '17 days')),
       'login',
       'dev-' || substr(md5(account_number || 'device'), 1, 6),
       home_country,
       FALSE
FROM alerted_accounts,
     generate_series(1, 3 + floor(6 * chakshu_rand(account_number || 'logins'))::int) AS n;

-- Planted account takeovers on ~30% of accounts with a real alert.
CREATE TEMP TABLE takeovers AS
SELECT account_number,
       first_true_ts,
       chakshu_pick(account_number || 'abroad',
           array_remove(ARRAY['United States','United Kingdom','Germany','India','China','Japan','Brazil','Mexico','Canada','Australia'], home_country)) AS foreign_country,
       'dev-' || substr(md5(account_number || 'intruder'), 1, 6) AS new_device,
       1 + floor(5 * chakshu_rand(account_number || 'lead')) AS lead_hours
FROM alerted_accounts
WHERE has_true_alert AND chakshu_rand(account_number || 'ato') < 0.30;

INSERT INTO login_events (account_number, ts, event_type, device_id, ip_country, is_new_device)
SELECT account_number, first_true_ts - make_interval(hours => lead_hours::int), 'password_reset', new_device, foreign_country, TRUE FROM takeovers
UNION ALL
SELECT account_number, first_true_ts - make_interval(hours => lead_hours::int) + INTERVAL '20 minutes', 'login', new_device, foreign_country, TRUE FROM takeovers;

-- Harmless travel on ~4% of innocent alerted accounts: usual device, another country.
INSERT INTO login_events (account_number, ts, event_type, device_id, ip_country, is_new_device)
SELECT account_number,
       date_trunc('minute', TIMESTAMP '2022-09-01' + (chakshu_rand(account_number || 'trip') * INTERVAL '17 days')),
       'login',
       'dev-' || substr(md5(account_number || 'device'), 1, 6),
       chakshu_pick(account_number || 'trip-country',
           array_remove(ARRAY['United States','United Kingdom','Germany','India','China','Japan','Brazil','Mexico','Canada','Australia'], home_country)),
       FALSE
FROM alerted_accounts
WHERE NOT has_true_alert AND chakshu_rand(account_number || 'travel') < 0.04;

CREATE INDEX login_events_account_ts ON login_events (account_number, ts);

------------------------------------------------------------------------------------------
-- A FICTIONAL watchlist, standing in for a sanctions list. No real names.
------------------------------------------------------------------------------------------
DROP TABLE IF EXISTS watchlist;
CREATE TABLE watchlist (
    id        SERIAL PRIMARY KEY,
    name      TEXT NOT NULL,
    list_name TEXT NOT NULL,
    reason    TEXT NOT NULL
);

-- Near-matches (one letter added) of 12 customers who moved laundered money.
INSERT INTO watchlist (name, list_name, reason)
SELECT regexp_replace(p.display_name, '^(\S+)', '\1s'),
       'Fictional Sanctions List (demo only)',
       'Listed entity (fictional)'
FROM customer_profiles p
WHERE p.account_number IN (SELECT account_number FROM alerted_accounts WHERE has_true_alert)
ORDER BY chakshu_rand(p.account_number || 'watch')
LIMIT 12;

-- Unrelated fictional names.
INSERT INTO watchlist (name, list_name, reason)
SELECT chakshu_pick('wl-adj' || n, ARRAY['Iron','Black','Red','Desert','Polar','Shadow','Granite','Copper'])
       || ' ' || chakshu_pick('wl-noun' || n, ARRAY['Falcon','Wolf','Tiger','Cobra','Raven','Bear','Lynx','Hawk'])
       || ' ' || chakshu_pick('wl-suffix' || n, ARRAY['Shipping','Capital','Ventures','Exports','Logistics']),
       'Fictional Sanctions List (demo only)',
       'Listed entity (fictional)'
FROM generate_series(1, 25) AS n;

ANALYZE customer_profiles;
ANALYZE login_events;
