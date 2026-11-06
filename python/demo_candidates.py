"""List good alerts for the demo, a few per kind of case.

    python python/demo_candidates.py

Run it after `aml-etl alerts`, `aml-etl synth` and `python/score_alerts.py`. It reads the
ground truth to pick clear examples. That is fine here: this is for us, to choose what to
show. The agents never see it.
"""

from __future__ import annotations

import psycopg

from score_alerts import database_url

# Shared columns for every kind of case. Small alerts first: easier to follow on screen.
BASE = """
WITH facts AS (
    SELECT a.id, a.account_number, a.alert_day, a.rules, a.senders, a.receivers,
           round((a.total_in_usd + a.total_out_usd)::numeric) AS moved_usd,
           a.is_true_positive AS real, s.band, round(s.score::numeric, 2) AS score,
           p.display_name, p.industry,
           (SELECT COUNT(*) FROM login_events l
             WHERE l.account_number = a.account_number
               AND l.ts < a.first_ts AND l.ts >= a.first_ts - interval '72 hours'
               AND l.event_type = 'password_reset')                          AS resets,
           (SELECT COUNT(*) FROM login_events l
             WHERE l.account_number = a.account_number
               AND l.ts < a.first_ts AND l.ts >= a.first_ts - interval '72 hours'
               AND l.ip_country <> p.home_country AND NOT l.is_new_device)    AS travel_logins,
           EXISTS (SELECT 1 FROM watchlist w
                    WHERE w.name = regexp_replace(p.display_name, '^(\\S+)', '\\1s')) AS watchlisted
    FROM alerts a
    JOIN alert_scores s ON s.alert_id = a.id
    JOIN customer_profiles p ON p.account_number = a.account_number
)
SELECT id, account_number, alert_day, rules, senders, receivers, moved_usd, band, score,
       display_name
FROM facts
"""

KINDS = {
    "Money mule (pass-through or fan-in, real, no takeover)":
        "WHERE real AND resets = 0 AND band = 'HIGH' "
        "AND ('R3_PASS_THROUGH' = ANY (rules) OR 'R1_RAPID_INFLOW' = ANY (rules)) "
        "ORDER BY senders + receivers BETWEEN 4 AND 12 DESC, score DESC LIMIT 3",
    "Account-takeover victim (real, password reset before)":
        "WHERE real AND resets > 0 AND band = 'HIGH' "
        "ORDER BY senders + receivers BETWEEN 4 AND 12 DESC, score DESC LIMIT 3",
    "Watchlist near-match (real)":
        "WHERE real AND watchlisted ORDER BY score DESC LIMIT 3",
    "Low risk, safe to close (not real, no login or watchlist signals)":
        "WHERE NOT real AND band = 'LOW' AND resets = 0 AND travel_logins = 0 "
        "AND NOT watchlisted ORDER BY senders + receivers BETWEEN 4 AND 8 DESC, score ASC LIMIT 3",
    "Unsure: travel logins, agent should ask a question (not real)":
        "WHERE NOT real AND travel_logins > 0 AND band IN ('UNSURE', 'HIGH') "
        "ORDER BY score DESC LIMIT 3",
}


def main() -> None:
    with psycopg.connect(database_url()) as conn:
        for kind, where in KINDS.items():
            print(f"\n== {kind}")
            rows = conn.execute(BASE + where).fetchall()
            if not rows:
                print("   (none found)")
            for r in rows:
                print(f"   #{r[0]}  {r[1]}  {r[2]}  {','.join(r[3])}  in:{r[4]} out:{r[5]}  "
                      f"${r[6]:,}  {r[7]} {r[8]}  {r[9]}")


if __name__ == "__main__":
    main()
