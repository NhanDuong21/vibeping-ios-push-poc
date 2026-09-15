# Security Notes

The Quick Tunnel URL is temporary but should still be treated as public. Anyone with the URL can reach the PoC while it is running.

- Do not commit `data/vapid-private.key` or `data/subscription.json`.
- Do not publish the active tunnel URL in screenshots or issue logs.
- Stop the server and tunnel when testing is complete.
- Replace this unauthenticated test endpoint before adapting the PoC for production.
