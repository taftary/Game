# Performance and Observability

**Status:** Target

## Summary

Performance work starts with a workload and a measurement. Do not trade API
clarity for speculative allocation or synchronization improvements.

## Key points

- Measure CPU, GPU, memory, loading, and mobile energy/thermal behavior separately.
- Use structured logs and tracing at lifecycle and resource boundaries.
- Avoid logging hot-path data in production configurations.
- Every performance claim links to a benchmark, profile, or target-device result.

## Measure separately

- CPU frame and system time.
- Allocations and memory footprint.
- GPU submission and pass timing.
- Synchronization and queue stalls.
- Asset loading and startup time.
- Mobile energy and thermal behavior.
