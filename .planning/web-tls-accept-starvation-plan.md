# Web TLS Accept Starvation Plan

## Context

The backend is reachable locally on both `8787` and `8788`, and `/api/health`
and `/api/cockpit` return HTTP 200. I reproduced the intermittent
`backend unreachable` symptom by opening one idle raw TCP connection to the
HTTPS port and then requesting `/api/health`: the health request timed out until
the idle connection was closed.

Current `TlsListener::accept` accepts a TCP connection and awaits the TLS
handshake before returning to Axum. A client that connects but stalls before
TLS can therefore starve the accept loop and make later browser polls look like
the backend is unreachable.

## Task 1: Add a failing TLS listener starvation test

- Failing behavior test to write:
  - Add an async test in `crates/ajax-web/src/runtime.rs`.
  - Start the Axum web app over the real TLS listener on an ephemeral localhost
    port.
  - Open and hold one raw `tokio::net::TcpStream` to that port without starting
    TLS.
  - In parallel, make a proper TLS request to `/api/health`.
  - Assert the health request completes with HTTP 200 within a short timeout.
- Code to implement:
  - Test-only helper functions for starting the TLS Axum app and making a tiny
    TLS HTTP/1.1 GET request.
  - No production behavior change in this task.
- Verification:
  - Run
    `rtk cargo test -p ajax-web tls_listener_idle_tcp_connection_does_not_block_health_request`
    and show the timeout/failure before implementation.

## Task 2: Make TLS handshakes non-blocking for the accept loop

- Failing behavior test to write:
  - Reuse the failing test from Task 1.
- Code to implement:
  - Change the web TLS listener so raw TCP accept continues immediately while
    TLS handshakes run per connection.
  - Send completed TLS streams to Axum through an internal channel.
  - Add a bounded TLS handshake timeout so idle raw TCP clients are dropped
    instead of leaving unbounded pending tasks.
  - Preserve existing TLS error logging and `Listener` behavior.
- Verification:
  - Run
    `rtk cargo test -p ajax-web tls_listener_idle_tcp_connection_does_not_block_health_request`
    and show the pass.

## Final validation

- Run the strongest applicable checks after the fix:
  - `rtk cargo fmt --check`
  - `rtk cargo check --all-targets --all-features`
  - `rtk cargo clippy --all-targets --all-features -- -D warnings`
  - `rtk cargo nextest run --all-features`
  - `rtk npm run web:check`
  - `rtk npm run web:test -- --run`

Plan ready. Approve to proceed.
