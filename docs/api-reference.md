# API Reference

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/health` | Confirm the server is ready |
| GET | `/api/status` | Inspect local subscription state |
| GET | `/api/vapid-public-key` | Return the browser-safe VAPID public key |
| POST | `/api/subscription` | Save the current device subscription |
| POST | `/api/test-push` | Send a test notification |

The PoC endpoints are intentionally unauthenticated and must not be exposed as a production API.
