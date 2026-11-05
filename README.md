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

```bash
cp .env.example .env            # then fill in the values
docker compose up -d            # starts Chakshu's database on port 5433
cargo build --release
```

_(remaining steps to fill in as the build progresses)_

## Architecture

_(diagram and explanation to add)_

## Data

All data is synthetic. See [`data/README.md`](data/README.md).

## License

MIT. See [`LICENSE`](LICENSE).
