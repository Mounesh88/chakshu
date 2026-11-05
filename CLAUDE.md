# CLAUDE.md

Guidance for Claude Code working in this repository.

## What this project is

Chakshu (चक्षु, Sanskrit for "eye") is an anti-money-laundering (AML) alert triage room
built on Switch for the Flip the Switch hackathon, Track 01 (Best Business Workflow).
AI agents investigate each alert in parallel in a Mattermost room run by Switch, and a
person makes every final decision. The agents investigate; the humans decide.

Deadline: Sat Nov 7, 11:59 PM IST (12:29 PM US Central). Submit by about 11 AM Central.

## This repository is public

Never commit secrets (.env, API keys, tokens), the raw IBM data, or personal data.
`.gitignore` already blocks `.env`, `data/raw/*`, `target/`, `*.parquet` and models.

## How work flows

- `main` always builds. Changes arrive as small pull requests with a plain-English summary.
- The repo owner reviews and merges each PR, then runs `git pull` locally.
- Code is written and tested in a Linux cloud workspace; this Windows machine runs Switch,
  Mattermost, Docker and the demo. Fix Windows-only issues here.

## Architecture

| Part | Language | Path | Job |
|---|---|---|---|
| Shared types | Rust | `crates/aml-core` | Alert, Transfer, Account, Case, Event |
| Data pipeline | Rust (csv + Postgres COPY) | `crates/aml-etl` | `load` IBM files, run `alerts` rules, `synth` profiles/logins/watchlist (SQL in `db/`) |
| Graph patterns | Rust (petgraph) | `crates/aml-graph` | Fan-in, fan-out, cycles |
| MCP server | Rust (rmcp 3) | `crates/aml-mcp` | Read-only tools every agent uses; streamable HTTP |
| Alert feed | Rust | `crates/aml-feed` | Posts alerts and SAR deadline reminders into the room via `POST /agents/{id}/message` |
| ML | Python | `python/` | XGBoost training, SHAP reasons written to Postgres |
| Room setup | YAML | `switch/` | Template: 4 agents + `#aml-alerts` room; built by `scripts/build_room_template.py` from `room-template.base.yaml` + `knowledge/policies/` |
| Policies | Markdown | `knowledge/policies/` | Synthetic policies of a fictional bank |

Agents in the room (Claude Code via Switch Console, on the owner's Claude Pro plan):
`@graph-analyst` (lead), `@kyc-profiler`, `@cyber-signals`, `@case-writer`. Policies are
room documents the agents cite. Switch does not pass a user's own Claude Code MCP settings
to the agents it starts, so each agent folder carries `.mcp.json` +
`.claude/settings.json` (`scripts/setup-agent-folders.ps1`).

## Rules the product must keep

- Agents only read and recommend. No tool closes a case, files a SAR or contacts a customer.
- A person records every decision in the room; the timeline is built from room events.
- SAR deadline = detection + 30 days (60 max if no suspect), computed in code, never by an LLM.
- Every recommendation cites a source (policy, FFIEC manual, FinCEN advisory, or past case).
- All data is synthetic. README states: prototype, not legal or compliance advice.
- `alerts.is_true_positive` and `transfers.is_laundering` are ground truth for training and
  evaluation only. No MCP tool or agent may ever see them.

## Local commands (Windows, PowerShell)

```powershell
docker compose up -d        # Chakshu's database: Postgres + pgvector on localhost:5433
cargo build                 # build all Rust crates
cargo run -p aml-mcp        # run the MCP server
```

Switch's own server and Mattermost are managed by Switch Console, not by this repo.
