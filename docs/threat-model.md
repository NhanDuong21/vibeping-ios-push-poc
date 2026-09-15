# Threat Model

The PoC assumes a short-lived test session and a tunnel URL shared only with the tester.

Relevant risks include:

- an unintended visitor discovering the live tunnel URL;
- unauthorized calls to the unauthenticated test endpoint;
- disclosure of the VAPID private key or subscription;
- stale Home Screen installations pointing at abandoned origins.

Production use requires authentication, authorization, rate limiting, and durable secret storage.
