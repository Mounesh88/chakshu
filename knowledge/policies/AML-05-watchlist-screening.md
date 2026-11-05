# AML-05 Sanctions and watchlist screening

Chakshu Demo Bank (fictional). Owner: BSA Officer.

1. Screen the customer, and any counterparty that matters to the case, against the
   watchlist with the `screen_watchlist` tool.
2. A fuzzy match with similarity of 0.85 or more is a potential match, not a confirmed one.
3. A potential match is escalated to the BSA Officer the same day, and no further outgoing
   transfers on the account are approved until a person has cleared or confirmed it.
4. Clearing a potential match needs a written reason (for example: different country and
   business type).
5. The watchlist in this demo is fictional. No real sanctions list is used.
