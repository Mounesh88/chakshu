-- Risk scores written by python/score_alerts.py. Rebuilt on every run.
-- Each score is out-of-fold: it comes from a model that did not see that alert's label.
DROP TABLE IF EXISTS alert_scores;

CREATE TABLE alert_scores (
    alert_id  BIGINT PRIMARY KEY REFERENCES alerts (id),
    score     DOUBLE PRECISION NOT NULL,          -- 0 to 1
    band      TEXT             NOT NULL,          -- HIGH | UNSURE | LOW
    reasons   JSONB            NOT NULL,          -- top 3 SHAP reasons pushing risk up
    model     TEXT             NOT NULL,
    scored_at TIMESTAMPTZ      NOT NULL DEFAULT now()
);
