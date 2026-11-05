# Data

Nothing in `data/raw/` is committed.

## IBM synthetic AML transactions

1. On Kaggle, search for "IBM Transactions for Anti Money Laundering (AML)".
2. Read and note the dataset's license on its Kaggle page.
3. Download the **HI-Small** transactions file into `data/raw/`.

Source paper: Altman et al., "Realistic Synthetic Financial Transactions for
Anti-Money Laundering Models" (IBM Research), https://arxiv.org/abs/2306.16424

`data/synth/` holds the customer profiles, login events and past cases we generate.
Everything here is synthetic. No real customer data is used.
