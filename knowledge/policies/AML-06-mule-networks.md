# AML-06 Money mule and network indicators

Chakshu Demo Bank (fictional). Owner: BSA Officer.

Indicators that an account is part of a laundering network:

1. Fan-in: many different accounts send money in within a short time, especially accounts
   opened in the last 90 days.
2. Fan-out: one account sends to many accounts soon after receiving.
3. Pass-through: most of the money that comes in leaves again the same day.
4. Loops: money returns to where it started through a few accounts.
5. Amounts just under USD 10,000, repeated (structuring).

When two or more indicators appear together, the analyst escalates (AML-01) and
recommends reviewing the connected accounts, starting with the newest ones. An account
used by a recruited money mule may belong to a real customer who was deceived; consider
AML-03 before assuming intent.
