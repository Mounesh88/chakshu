# Switch setup

| File | What it is |
|---|---|
| `room-template.base.yaml` | The source: four agents with their instructions, and the private room `#aml-alerts` with its own instructions and kickoff |
| `chakshu-room.template.yaml` | The template you load into Switch Console. **Generated**: do not edit by hand |

The generated file is the base template plus the seven policy documents from
[`knowledge/policies/`](../knowledge/policies/), attached to the room as room documents.
After editing either source, rebuild it:

```powershell
python scripts/build_room_template.py
```

Before running the template, prepare the agents' folders with
`scripts/setup-agent-folders.ps1`, and give the folder it prints as **Agents folder**
(`agents_root`). The main [README](../README.md#create-the-room) has the steps.

Two things about how Switch works shaped these instructions:

- One agent's message wakes another agent only when it names it with `@`. So
  `@kyc-profiler` and `@cyber-signals` start every reply with `@graph-analyst`, and the
  lead waits until both have answered before it posts the assessment.
- An agent cannot change its own instructions. To update them, the owner edits the agent
  in Switch Console (or asks switch-buddy), then sends `!reset @<agent>` in the room.
