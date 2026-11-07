# Notes on using Switch

What was easy, confusing or broken while building Chakshu on Switch (local server
v0.31.0, Switch Console v0.40.1, on Windows). The submission form asks: "What did you
like about Switch, and what would you improve?"

## Liked

- **Templates.** One YAML file in our repo creates the whole room: four agents with their
  instructions, the private channel, its instructions, seven policy documents and a
  kickoff message. Switch's own linter checks it.
- **Room documents.** A natural home for a bank's policies. Every agent reads the same
  rulebook, and the agents cited it by section reliably (`!list-documents` shows what the
  room holds).
- **@-mention hand-offs.** The parallel investigation is visible to people in the chat:
  the lead tags two specialists, they tag the lead back.
- **Safety by default.** An agent cannot change its own instructions (it refused when we
  asked it to), and `owner-agents` addressing keeps the room controlled.

## Got in the way

- **Windows.** Console could not start agents itself ("SDK sessions require a POSIX SSH
  execution host"), so we created them through switch-buddy. One template run timed out
  halfway and left the room unfinished.
- **`{agent}` in a template input.** A `{agent}` placeholder typed into a template's input
  value is not expanded; it only works inside the template itself. Our template now takes
  the parent folder (`agents_root`) and builds `{agents_root}/{agent}`.
- **MCP servers.** Agents Switch starts do not inherit our own Claude Code MCP settings, so
  each agent folder carries its own `.mcp.json` and `.claude/settings.json`.
- **Waking agents.** Our first live run stalled: the specialists replied, but the lead
  never posted. One agent's message wakes another only when it names it; we found this in
  Switch's source (`delivery/conversation.py`).
- **Display names.** Typing "@Graph Analyst" looks like a mention but does not address the
  agent; the handle `@graph-analyst` does.
- **Restarts.** After changing an agent's instructions, or restarting our MCP server, the
  agent needed `!reset` to pick up the change.

## Wish Switch did next

- Native agent sessions on Windows.
- A template run that can resume after a timeout.
- A clear note in the docs on how agents wake each other.
- A warning when a message's mention does not match any agent in the room.
