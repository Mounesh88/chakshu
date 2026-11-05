# Chakshu

**AI investigators for anti-money-laundering alerts, working inside your team's chat.**

Chakshu (चक्षु, "eye" in Sanskrit) is a Switch room where AI agents investigate each
money-laundering alert in parallel and hand a person an explained, sourced
recommendation. The agents investigate; the humans decide.

Built for the Flip the Switch hackathon (Track 01: Best Business Workflow).

> Prototype on synthetic data. Not legal or compliance advice.

## The problem

Banks must report suspicious activity, but their rule-based monitoring raises so many
false alarms that analysts spend most of the day closing noise, while real laundering
hides in the pile.

## Agents we connected

| Agent | Runs on | Job |
|---|---|---|
| `@graph-analyst` | _(to fill in)_ | Money-flow patterns, risk score with reasons, sourced recommendations |
| `@kyc-profiler` | _(to fill in)_ | Compares activity with the customer profile; sanctions screening |
| `@cyber-signals` | _(to fill in)_ | Login and device evidence of account takeover |
| `@case-writer` | _(to fill in)_ | Drafts the SAR narrative when a person escalates |
| `@alert-feed` | Rust, Switch agent protocol | Posts new alerts and SAR deadline reminders |

## How the channel is set up

_(to fill in: the room, its instructions, the policy documents attached to it, who is in it,
and how a person records a decision)_

## How to run it

### Prerequisites

- Docker Desktop
- Rust 1.85 or newer
- Python 3.11 or newer
- Switch Console, with a local server running
- At least one agent CLI: Claude Code, Codex or OpenCode

### Steps

On Windows, in PowerShell, from the repository folder:

```powershell
copy .env.example .env          # then set POSTGRES_PASSWORD, and the same password in DATABASE_URL
docker compose up -d            # starts Chakshu's database on localhost:5433
cargo build --release
```

Download the three IBM data files into `data/raw/` (see [`data/README.md`](data/README.md)),
then load them. The full load of about 5 million transfers takes a few minutes:

```powershell
cargo run --release -p aml-etl -- load --data-dir data/raw
```

Run the bank-style alert rules, then generate the synthetic customer data:

```powershell
cargo run --release -p aml-etl -- alerts
cargo run --release -p aml-etl -- synth
```

`alerts` prints how many alerts each rule raised and how many were real laundering.
Its thresholds can be changed, for example `-- alerts --fan-min 6 --min-usd 50000`
(see `-- alerts --help`).

### Start the MCP server

The agents reach the bank data only through Chakshu's read-only MCP server. Leave it
running in its own PowerShell window:

```powershell
cargo run --release -p aml-mcp      # serves http://127.0.0.1:8090/mcp
```

Connect Claude Code to it once (user scope, so every agent session Switch starts sees it):

```powershell
claude mcp add --transport http --scope user chakshu http://127.0.0.1:8090/mcp
claude mcp list
```

| Tool | What it returns |
|---|---|
| `list_alerts` | Alerts from the rules engine, newest first (filter by rule or day) |
| `get_alert` | One alert: account, day, rules fired, money in and out, the customer |
| `get_transactions` | The account's transfers that day, in original currency and USD |
| `get_counterparties` | Who it paid and was paid by, with each counterparty's account age |
| `get_customer_profile` | What the bank recorded at onboarding vs. what the account actually moved |
| `get_login_events` | Logins and password resets, flagging new devices and foreign countries |
| `screen_watchlist` | Fuzzy name screening against the fictional watchlist |
| `detect_patterns` | Fan-in, fan-out, pass-through, and short loops through the account |

Every tool only reads. None can close a case, file a report or contact a customer, and
none returns IBM's ground-truth labels (a test fails the build if the server's code
mentions them).

_(remaining steps to fill in as the build progresses)_

## Architecture

_(diagram and explanation to add)_

## Data

All data is synthetic. Transactions come from IBM's synthetic AML dataset (Altman et al.,
NeurIPS 2023), used under the CDLA-Sharing-1.0 license and not redistributed here.
See [`data/README.md`](data/README.md).

## License

MIT. See [`LICENSE`](LICENSE).
