# Data Lifecycle

The server creates two local runtime files:

- `data/vapid-private.key` persists the application-server identity.
- `data/subscription.json` stores the latest browser subscription.

Both are ignored by Git. Running `reset` removes them, and the next server start creates a new VAPID identity. The iPhone must subscribe again after a reset.
