# Shared Trading Adapter Architecture

This private repository is the shared development base for the Thomas AI trading projects. It keeps the AI gateway from the public repository while adding reusable interfaces for market data, execution, strategies, research, and operations.

## Design rules

- Adapters hide provider-specific APIs behind stable interfaces.
- Paper trading is the default; live execution requires explicit configuration.
- Every adapter must be testable with deterministic fixtures.
- Market timestamps are timezone-aware and normalized to UTC internally.
- Credentials stay in the macOS Keychain or environment; never commit secrets.
- Backtests and live execution share signal and risk interfaces where possible.

## Modules

| Module | Responsibility |
| --- | --- |
| `data/` | Moomoo/OpenD, SQLite, CSV, Parquet, and live market data adapters |
| `execution/` | Paper/live order submission, fills, cancellations, and account state |
| `strategies/` | Donchian, EMA, momentum, DCA, and common signal contracts |
| `backtesting/` | Walk-forward evaluation, fees, spread, slippage, and fills |
| `risk/` | Position sizing, exposure, stops, profit locks, and circuit breakers |
| `persistence/` | SQLite/Parquet repositories for data, orders, positions, and results |
| `symbols/` | ASX/US symbol normalization, sessions, calendars, and timezones |
| `config/` | Typed configuration, environments, account modes, and Keychain access |
| `ai/` | Thomas AI, local MLX, Apple Foundation Models, Apple Private Cloud Compute, and free NVIDIA reasoning integrations |
| `notifications/` | Fill, risk, failure, and daily-summary notifications |
| `observability/` | Structured logs, metrics, traces, and strategy diagnostics |

Each directory contains the initial contract and implementation notes. Provider-specific code should remain behind the corresponding adapter boundary.

