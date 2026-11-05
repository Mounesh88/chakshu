# AML-01 Alert triage and escalation

Chakshu Demo Bank (fictional). Owner: BSA Officer.

1. Every alert from the monitoring rules is reviewed by an L1 analyst within 2 business days.
2. The analyst may close an alert as a false positive only with a written reason that names
   the evidence (for example: activity matches the customer's declared business).
3. The analyst escalates to the BSA Officer when any of these hold:
   - a money-flow pattern (fan-in, fan-out, pass-through or a loop) has no business explanation;
   - activity is more than 3 times the customer's expected monthly activity;
   - account-takeover signals are present (see AML-03);
   - there is a watchlist match (see AML-05);
   - two or more alerts on the same customer within 30 days.
4. When unsure, the analyst asks a specific question in the room before deciding.
5. Every decision is recorded in the room, one line, in this form:
   `DECISION #<alert id>: CLOSE | ESCALATE | FILE SAR | NO SAR | REFER FRAUD - <reason>`
6. AI agents investigate and recommend. They never make or record a decision.
