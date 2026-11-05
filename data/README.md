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

`data/synth/` holds the customer profiles, login events and past cases we generate.
Everything here is synthetic. No real customer data is used.
