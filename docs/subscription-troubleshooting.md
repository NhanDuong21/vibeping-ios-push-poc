# Subscription Troubleshooting

An active subscription belongs to a specific browser origin and VAPID identity.

- A new tunnel hostname requires a new Home Screen installation and subscription.
- Running `reset` creates a new VAPID identity and invalidates the previous subscription.
- Provider responses `404` and `410` normally indicate a stale subscription.
- Reopen the current installed app and subscribe again after either change.
