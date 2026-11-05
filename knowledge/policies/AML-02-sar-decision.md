# AML-02 Suspicious Activity Report (SAR) decision and filing

Chakshu Demo Bank (fictional). Owner: BSA Officer.

1. Only the BSA Officer decides whether to file a SAR.
2. The bank files a SAR for suspicious activity involving USD 5,000 or more
   (31 CFR 1020.320).
3. Deadline: 30 calendar days from initial detection. If no suspect is identified, up to
   30 more days to identify one, never later than 60 days. "Initial detection" is the day a
   person confirmed the facts look suspicious, not the day the alert fired. Deadlines are
   computed with the `sar_deadline` tool, never by hand or by an AI.
4. The SAR narrative answers who, what, when, where, why and how, cites the evidence, and
   uses the FinCEN advisory key term when one applies (see REF-01).
5. Confidentiality: never tell the customer, or anyone outside the compliance team, that a
   SAR is being considered or was filed ("no tipping off").
6. Keep the SAR and its supporting documents for 5 years.
7. AI agents may draft a narrative for the BSA Officer to review. They never file.
