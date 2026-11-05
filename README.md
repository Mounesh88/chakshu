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

Four Claude Code agents, created and run by Switch Console from one template
([`switch/chakshu-room.template.yaml`](switch/chakshu-room.template.yaml)). They reach
bank data only through Chakshu's read-only MCP server, written in Rust.

| Agent | Runs on | Job |
|---|---|---|
| `@graph-analyst` | Claude Code (Sonnet) | Lead investigator: takes the next alert, traces the money (fan-in, fan-out, pass-through, loops), hands off to the specialists, then posts a risk assessment with sourced recommendations |
| `@kyc-profiler` | Claude Code (Sonnet) | Compares the account's activity with the customer's profile, checks how new the counterparties are, screens the watchlist |
| `@cyber-signals` | Claude Code (Sonnet) | Checks logins, devices and password resets before the money moved, to tell a takeover victim from a launderer |
| `@case-writer` | Claude Code (Sonnet) | Drafts the SAR narrative, only when a person asks |

## How the channel is set up

One private Mattermost channel, `#aml-alerts`, created by the template with the four agents
and you in it.

- **Room instructions** tell every agent the workflow and the rules: investigate and
  recommend, never decide; cite a room document for every recommendation; compute
  deadlines only with the `sar_deadline` tool; never suggest telling the customer
  about an investigation.
- **Room documents** hold the bank's rulebook: six policies of a fictional bank (AML-01
  to AML-06) and a summary of public US guidance (REF-01), from
  [`knowledge/policies/`](knowledge/policies/). Agents cite them by ID.
- **The flow:** a person asks `@graph-analyst` for the next alert. It traces the money and
  hands off to `@kyc-profiler` and `@cyber-signals`, who investigate **in parallel** and
  reply in the thread. `@graph-analyst` posts the assessment. **A person decides**, in one
  line: `DECISION #<alert id>: CLOSE | ESCALATE | FILE SAR | NO SAR | REFER FRAUD - <reason>`.
  The room's history is the case file and audit trail.
- Agents answer only their owner and each other (`addressing: owner-agents`).

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
| `sar_deadline` | The SAR filing deadline under 31 CFR 1020.320 (30 days, or 60 with no suspect) |

Every tool only reads. None can close a case, file a report or contact a customer, and
none returns IBM's ground-truth labels (a test fails the build if the server's code
mentions them).

### Create the room

1. Prepare the agents' folders (each gets the connection to the MCP server):
   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\setup-agent-folders.ps1
   ```
   It prints a directory path ending in `{agent}`.
2. In Switch Console, open **Templates**, import `switch/chakshu-room.template.yaml`, and
   in the advanced options set the agents' **directory** to the path the script printed.
   Run it: Switch creates the four agents and `#aml-alerts`.
3. With the MCP server running, type in `#aml-alerts`:
   ```
   @graph-analyst next alert
   ```

_(remaining steps to fill in as the build progresses)_

## Architecture

_(diagram and explanation to add)_

## Data

All data is synthetic. Transactions come from IBM's synthetic AML dataset (Altman et al.,
NeurIPS 2023), used under the CDLA-Sharing-1.0 license and not redistributed here.
See [`data/README.md`](data/README.md).

## License

MIT. See [`LICENSE`](LICENSE).
