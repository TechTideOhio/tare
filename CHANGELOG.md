# Changelog

All notable changes to `tare` are documented in this file.

## [0.1.0] - 2026-11-04

### Added
- TideOS Eval Kit Tool 14: Cost and latency first-class metrology and Pareto release gate.
- Token per request metrology (uncached, cache read, cache write, output, reasoning tokens).
- Prompt cache hit ratio tracking and micro-dollar pricing calculations.
- Cost Per Successful Task (CPST) computation and Failure Waste accounting.
- Order-statistic latency quantile engine (p50, p90, p95, p99, TTFT, tail amplification ratio).
- Paired SplitMix64 bootstrap confidence intervals on delta CPST and delta p95 latency.
- Quality-flat 40% cost reduction shipping rule (QualityFlatEfficiencyWin).
- Vanity call cost trap detector (VanityCallCostTrapBlocked).
- Sequential SHA-256 Merkle task provenance chaining (tare-run-v1).
- Cryptographic lockfile engine (sha256-framed-v1).
- Soft Factory 8-gate repository hygiene auditor.
- Terminal scorecard, Markdown step summary, Mermaid diagram, and OpenTelemetry span exports.
