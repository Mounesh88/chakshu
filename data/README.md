# Data

Nothing in `data/raw/` is committed.

## IBM synthetic AML transactions

Source: **IBM Transactions for Anti Money Laundering (AML)** by Erik Altman, on Kaggle:
https://www.kaggle.com/datasets/ealtman2019/ibm-transactions-for-anti-money-laundering-aml

License: **Community Data License Agreement – Sharing, Version 1.0 (CDLA-Sharing-1.0)**.
We do not redistribute the data; download it yourself from the page above.

Download these three files (each with its own download icon, not the whole dataset)
into `data/raw/`:

| File | What it holds |
|---|---|
| `HI-Small_Trans.csv` | About 5 million transfers, Sep 1–10, 2022, each labelled laundering or not |
| `HI-Small_accounts.csv` | The bank accounts and the people or companies that own them |
| `HI-Small_Patterns.txt` | Laundering attempts labelled by pattern (fan-in, fan-out, cycle, ...) |

Then load them into the database:

```powershell
cargo run --release -p aml-etl -- load --data-dir data/raw
```

Citation: Altman et al., "Realistic Synthetic Financial Transactions for Anti-Money
Laundering Models", NeurIPS 2023 Datasets and Benchmarks. https://arxiv.org/abs/2306.16424

## Data we generate

IBM's data has transfers and account owners, but no customer profiles or login records.
`aml-etl synth` generates them inside the database, deterministically (same input, same
output):

| Table | What it holds |
|---|---|
| `customer_profiles` | Made-up business name, industry, home country, expected monthly activity, account opening date, risk rating |
| `login_events` | Logins and password resets for every account with an alert |
| `watchlist` | A **fictional** sanctions-style list. No real names |

Some signals are **planted on purpose**, using IBM's labels, so the agents have something
real to find:

- Accounts that fed labelled fan-in attempts are often newly opened.
- About 30% of accounts with a real alert show an account takeover (password reset and a
  new device from another country) a few hours before the money moved.
- About 4% of innocent alerted accounts show harmless travel logins.
- 12 customers who moved laundered money have near-matching names on the watchlist.

`aml-etl alerts` also stores the ground truth for each alert (`alerts.is_true_positive`)
for training and evaluating the model. It is never shown to the agents.

Everything here is synthetic. No real customer data is used.
