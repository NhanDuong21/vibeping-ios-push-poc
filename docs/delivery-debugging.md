# Delivery Debugging

Debug from the sender toward the device:

1. Verify `/api/health` responds.
2. Check that `/api/status` reports a saved subscription.
3. Send a notification and record the provider response.
4. Confirm the phone is online and VibePing notifications are enabled.
5. Reopen the installed app to refresh a stale subscription.

Only a visible notification on the physical iPhone completes the test.
