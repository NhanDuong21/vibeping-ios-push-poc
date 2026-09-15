# Architecture Decisions

## Local Rust server

Axum and Tokio keep the sender, diagnostics, and static PWA in one executable.

## Quick Tunnel

Cloudflare Quick Tunnel provides the HTTPS origin required by Service Workers without paid infrastructure.

## Plain frontend

Vanilla HTML, CSS, and JavaScript keep the feasibility test small and easy to inspect.
