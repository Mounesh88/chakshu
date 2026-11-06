"""Train an alert risk model and write a score with its top reasons for every alert.

    python python/score_alerts.py            # reads DATABASE_URL from .env

What it does:
1. Builds one row of features per alert, in SQL (FEATURES_SQL below).
2. Trains XGBoost with 5-fold cross-validation. Every alert's score comes from the
   model that did NOT see that alert while training (out-of-fold), so the scores the
   agents quote are honest predictions, not memorised answers.
3. Explains each score with SHAP values (XGBoost's own pred_contribs) and keeps the
   top three reasons that push the risk up.
4. Writes the table alert_scores, and the evaluation to python/results/metrics.json.

The label is alerts.is_true_positive (IBM's ground truth). It is read here for training
and evaluation only. The MCP server never reads it; it reads alert_scores.

Two feature groups are evaluated separately, because the second group is synthetic data
we generated, with signals planted on true alerts (see data/README.md):
- transactions: from IBM's transfers and the alert rules only;
- customer: synthetic profiles, logins and counterparty ages.
"""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from pathlib import Path

import numpy as np
import pandas as pd
import psycopg
import xgboost as xgb
from dotenv import load_dotenv
from psycopg.types.json import Jsonb
from sklearn.metrics import average_precision_score, roc_auc_score
from sklearn.model_selection import StratifiedKFold

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "python" / "results"
MODEL_NAME = "xgb-v1"
SEED = 7

FEATURES_SQL = """
WITH day_tx AS (
    -- Two index-friendly joins instead of one OR join; a self-transfer counts once.
    SELECT a.id AS alert_id, t.*
    FROM alerts a
    JOIN transfers t
      ON t.from_account = a.account_number
     AND t.ts >= a.alert_day AND t.ts < a.alert_day + 1
    UNION ALL
    SELECT a.id AS alert_id, t.*
    FROM alerts a
    JOIN transfers t
      ON t.to_account = a.account_number AND t.from_account <> a.account_number
     AND t.ts >= a.alert_day AND t.ts < a.alert_day + 1
),
tx AS (
    SELECT alert_id,
           COUNT(*)                                                        AS n_transfers,
           MAX(amount_usd)                                                 AS max_amount_usd,
           AVG((payment_format = 'ACH')::int)                              AS share_ach,
           AVG((payment_format = 'Wire')::int)                             AS share_wire,
           AVG((payment_format = 'Cash')::int)                             AS share_cash,
           AVG((payment_format = 'Bitcoin')::int)                          AS share_bitcoin,
           AVG((payment_format = 'Cheque')::int)                           AS share_cheque,
           AVG((payment_format = 'Credit Card')::int)                      AS share_card,
           AVG((payment_currency <> receiving_currency)::int)              AS share_cross_currency,
           AVG((from_bank <> to_bank)::int)                                AS share_other_bank,
           AVG((amount_usd = round(amount_usd::numeric, -2))::int)         AS share_round_amounts
    FROM day_tx
    GROUP BY alert_id
),
cp AS (
    SELECT d.alert_id,
           COUNT(DISTINCT CASE WHEN d.from_account = a.account_number THEN d.to_account
                               ELSE d.from_account END)                    AS counterparties,
           COUNT(DISTINCT CASE WHEN p.opened_on > a.alert_day - 90
                               THEN p.account_number END)                  AS new_counterparties
    FROM day_tx d
    JOIN alerts a ON a.id = d.alert_id
    LEFT JOIN customer_profiles p
      ON p.account_number = CASE WHEN d.from_account = a.account_number THEN d.to_account
                                 ELSE d.from_account END
    GROUP BY d.alert_id
),
logins AS (
    SELECT a.id AS alert_id,
           COUNT(*) FILTER (WHERE l.event_type = 'password_reset')         AS password_resets_72h,
           COUNT(*) FILTER (WHERE l.is_new_device)                         AS new_device_events_72h,
           COUNT(*) FILTER (WHERE l.ip_country <> p.home_country)          AS foreign_events_72h
    FROM alerts a
    JOIN customer_profiles p ON p.account_number = a.account_number
    JOIN login_events l
      ON l.account_number = a.account_number
     AND l.ts <  a.first_ts
     AND l.ts >= a.first_ts - interval '72 hours'
    GROUP BY a.id
)
SELECT a.id                                                                AS alert_id,
       a.is_true_positive                                                  AS label,
       -- transactions group
       a.senders, a.receivers, a.total_in_usd, a.total_out_usd,
       a.near_threshold_count,
       cardinality(a.rules)                                                AS n_rules,
       ('R1_RAPID_INFLOW'  = ANY (a.rules))::int                           AS rule_inflow,
       ('R2_RAPID_OUTFLOW' = ANY (a.rules))::int                           AS rule_outflow,
       ('R3_PASS_THROUGH'  = ANY (a.rules))::int                           AS rule_pass_through,
       ('R4_STRUCTURING'   = ANY (a.rules))::int                           AS rule_structuring,
       EXTRACT(EPOCH FROM a.last_ts - a.first_ts) / 3600.0                 AS active_hours,
       CASE WHEN GREATEST(a.total_in_usd, a.total_out_usd) > 0
            THEN LEAST(a.total_in_usd, a.total_out_usd)
                 / GREATEST(a.total_in_usd, a.total_out_usd) ELSE 0 END    AS pass_ratio,
       (SELECT COUNT(*) FROM alerts b
         WHERE b.account_number = a.account_number
           AND b.alert_day >= a.alert_day - 30 AND b.alert_day < a.alert_day) AS prior_alerts_30d,
       tx.n_transfers, tx.max_amount_usd, tx.share_ach, tx.share_wire, tx.share_cash,
       tx.share_bitcoin, tx.share_cheque, tx.share_card, tx.share_cross_currency,
       tx.share_other_bank, tx.share_round_amounts,
       COALESCE(cp.counterparties, 0)                                      AS counterparties,
       -- customer group (synthetic)
       a.alert_day - p.opened_on                                           AS account_age_days,
       CASE p.risk_rating WHEN 'Low' THEN 0 WHEN 'Medium' THEN 1 ELSE 2 END AS risk_rating,
       (p.entity_type = 'Individual')::int                                 AS is_individual,
       (a.total_in_usd + a.total_out_usd) / NULLIF(p.expected_monthly_usd, 0) AS day_vs_expected_month,
       COALESCE(cp.new_counterparties, 0)                                  AS new_counterparties,
       COALESCE(lg.password_resets_72h, 0)                                 AS password_resets_72h,
       COALESCE(lg.new_device_events_72h, 0)                               AS new_device_events_72h,
       COALESCE(lg.foreign_events_72h, 0)                                  AS foreign_events_72h
FROM alerts a
LEFT JOIN customer_profiles p ON p.account_number = a.account_number
LEFT JOIN tx     ON tx.alert_id = a.id
LEFT JOIN cp     ON cp.alert_id = a.id
LEFT JOIN logins lg ON lg.alert_id = a.id
ORDER BY a.id
"""

TRANSACTION_FEATURES = [
    "senders", "receivers", "total_in_usd", "total_out_usd", "near_threshold_count",
    "n_rules", "rule_inflow", "rule_outflow", "rule_pass_through", "rule_structuring",
    "active_hours", "pass_ratio", "prior_alerts_30d", "n_transfers", "max_amount_usd",
    "share_ach", "share_wire", "share_cash", "share_bitcoin", "share_cheque", "share_card",
    "share_cross_currency", "share_other_bank", "share_round_amounts", "counterparties",
]
CUSTOMER_FEATURES = [
    "account_age_days", "risk_rating", "is_individual", "day_vs_expected_month",
    "new_counterparties", "password_resets_72h", "new_device_events_72h", "foreign_events_72h",
]

# How each reason reads in the room. {v} is the alert's value for that feature. A reason
# means this value pushed the score up for this alert, high or low as the value may be.
REASON_TEXT = {
    "senders": "senders that day: {v:.0f}",
    "receivers": "receivers that day: {v:.0f}",
    "total_in_usd": "received that day: ${v:,.0f}",
    "total_out_usd": "sent that day: ${v:,.0f}",
    "near_threshold_count": "transfers just under $10,000: {v:.0f}",
    "n_rules": "alert rules fired: {v:.0f}",
    "rule_inflow": "rapid-inflow rule fired: {v:.0f}",
    "rule_outflow": "rapid-outflow rule fired: {v:.0f}",
    "rule_pass_through": "pass-through rule fired: {v:.0f}",
    "rule_structuring": "structuring rule fired: {v:.0f}",
    "active_hours": "hours between first and last transfer: {v:.1f}",
    "pass_ratio": "money out vs in (1 = all passed through): {v:.2f}",
    "prior_alerts_30d": "earlier alerts on this account in 30 days: {v:.0f}",
    "n_transfers": "transfers that day: {v:.0f}",
    "max_amount_usd": "largest transfer: ${v:,.0f}",
    "share_ach": "share of transfers by ACH: {v:.0%}",
    "share_wire": "share of transfers by wire: {v:.0%}",
    "share_cash": "share of transfers in cash: {v:.0%}",
    "share_bitcoin": "share of transfers in Bitcoin: {v:.0%}",
    "share_cheque": "share of transfers by cheque: {v:.0%}",
    "share_card": "share of transfers by credit card: {v:.0%}",
    "share_cross_currency": "share of transfers that change currency: {v:.0%}",
    "share_other_bank": "share of transfers to or from other banks: {v:.0%}",
    "share_round_amounts": "share of amounts in round hundreds: {v:.0%}",
    "counterparties": "counterparties that day: {v:.0f}",
    "account_age_days": "account age in days: {v:.0f}",
    "risk_rating": "customer risk rating (0 Low, 1 Medium, 2 High): {v:.0f}",
    "is_individual": "customer is an individual (1 yes, 0 no): {v:.0f}",
    "day_vs_expected_month": "the day's volume vs the expected month: {v:.2f}x",
    "new_counterparties": "counterparties opened in the last 90 days: {v:.0f}",
    "password_resets_72h": "password resets in the 72h before: {v:.0f}",
    "new_device_events_72h": "new-device logins in the 72h before: {v:.0f}",
    "foreign_events_72h": "logins from abroad in the 72h before: {v:.0f}",
}


def database_url() -> str:
    load_dotenv(ROOT / ".env")
    url = os.environ.get("DATABASE_URL")
    if not url:
        raise SystemExit("DATABASE_URL is not set. Copy .env.example to .env and fill it in.")
    return url


def model() -> xgb.XGBClassifier:
    return xgb.XGBClassifier(
        n_estimators=300,
        max_depth=4,
        learning_rate=0.05,
        subsample=0.9,
        colsample_bytree=0.9,
        eval_metric="aucpr",
        random_state=SEED,
        n_jobs=4,
    )


def out_of_fold(x: pd.DataFrame, y: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Scores and SHAP values for every alert, each from a model that never saw it."""
    scores = np.zeros(len(y))
    contribs = np.zeros((len(y), x.shape[1]))
    folds = StratifiedKFold(n_splits=5, shuffle=True, random_state=SEED)
    for train, test in folds.split(x, y):
        clf = model()
        clf.set_params(scale_pos_weight=(len(train) - y[train].sum()) / max(y[train].sum(), 1))
        clf.fit(x.iloc[train], y[train])
        scores[test] = clf.predict_proba(x.iloc[test])[:, 1]
        booster_input = xgb.DMatrix(x.iloc[test])
        contribs[test] = clf.get_booster().predict(booster_input, pred_contribs=True)[:, :-1]
    return scores, contribs


def evaluate(y: np.ndarray, scores: np.ndarray) -> dict:
    order = np.argsort(-scores)
    positives = y.sum()
    top_share = {}
    for share in (0.1, 0.2, 0.3):
        k = max(1, int(round(share * len(y))))
        top_share[f"top_{int(share * 100)}pct_catches"] = round(float(y[order[:k]].sum() / positives), 3)
    k100 = min(100, len(y))
    return {
        "roc_auc": round(float(roc_auc_score(y, scores)), 3),
        "pr_auc": round(float(average_precision_score(y, scores)), 3),
        "precision_top_100": round(float(y[order[:k100]].mean()), 3),
        **top_share,
    }


def reasons(row_values: pd.Series, row_contribs: np.ndarray, names: list[str]) -> list[dict]:
    top = [i for i in np.argsort(-row_contribs) if row_contribs[i] > 0][:3]
    return [
        {
            "feature": names[i],
            "reason": REASON_TEXT[names[i]].format(v=float(row_values[names[i]])),
            "contribution": round(float(row_contribs[i]), 3),
        }
        for i in top
    ]


def band(score: float, cut_high: float, cut_unsure: float) -> str:
    if score >= cut_high:
        return "HIGH"
    if score >= cut_unsure:
        return "UNSURE"
    return "LOW"


def main() -> None:
    url = database_url()
    with psycopg.connect(url) as conn:
        print("Building features for every alert...")
        frame = _read(conn)
    y = frame["label"].astype(int).to_numpy()
    print(f"{len(frame):,} alerts, {y.sum():,} real laundering ({y.mean():.1%})")
    if y.sum() < 10:
        raise SystemExit("Too few true alerts to train. Load more data first.")

    metrics: dict = {
        "alerts": int(len(frame)),
        "true_alerts": int(y.sum()),
        "rules_only_precision": round(float(y.mean()), 3),
        "validation": "5-fold stratified cross-validation, out-of-fold scores",
    }
    groups = {
        "transactions_only": TRANSACTION_FEATURES,
        "transactions_and_customer": TRANSACTION_FEATURES + CUSTOMER_FEATURES,
    }
    results = {}
    for name, cols in groups.items():
        x = frame[cols].astype(float).fillna(0.0)
        scores, contribs = out_of_fold(x, y)
        metrics[name] = evaluate(y, scores)
        results[name] = (x, scores, contribs)
        print(f"{name:28s} {metrics[name]}")

    x, scores, contribs = results["transactions_and_customer"]
    cols = list(x.columns)
    cut_high = float(np.quantile(scores, 0.9))
    cut_unsure = float(np.quantile(scores, 0.7))
    metrics["bands"] = {"HIGH": "top 10% of scores", "UNSURE": "next 20%", "LOW": "the rest"}
    metrics["model"] = MODEL_NAME
    metrics["scored_at"] = datetime.now(timezone.utc).isoformat(timespec="seconds")

    rows = [
        (
            int(alert_id),
            round(float(score), 4),
            band(float(score), cut_high, cut_unsure),
            Jsonb(reasons(x.iloc[i], contribs[i], cols)),
            MODEL_NAME,
        )
        for i, (alert_id, score) in enumerate(zip(frame["alert_id"], scores))
    ]
    with psycopg.connect(url) as conn:
        conn.execute((ROOT / "db" / "alert_scores.sql").read_text(encoding="utf-8"))
        with conn.cursor() as cur:
            cur.executemany(
                "INSERT INTO alert_scores (alert_id, score, band, reasons, model) "
                "VALUES (%s, %s, %s, %s, %s)",
                rows,
            )
    RESULTS.mkdir(parents=True, exist_ok=True)
    (RESULTS / "metrics.json").write_text(json.dumps(metrics, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote {len(rows):,} scores to alert_scores and python/results/metrics.json")


def _read(conn: psycopg.Connection) -> pd.DataFrame:
    with conn.cursor() as cur:
        cur.execute(FEATURES_SQL)
        names = [d.name for d in cur.description]
        return pd.DataFrame(cur.fetchall(), columns=names)


if __name__ == "__main__":
    main()
