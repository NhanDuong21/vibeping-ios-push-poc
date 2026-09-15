# Test Matrix

Record each run with the iOS version, network, app state, and outcome.

| App state | Network | Expected result |
| --- | --- | --- |
| Foreground | Wi-Fi | Notification arrives |
| Background | Wi-Fi | Notification arrives |
| Screen locked | Wi-Fi | Notification arrives |
| Background | Cellular | Notification arrives |

A provider `201` or `202` is useful evidence, but the physical device result is authoritative.
