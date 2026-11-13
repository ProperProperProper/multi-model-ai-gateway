# Subagent contract

The main conversation is the orchestrator. A subagent receives one bounded task and returns a concise report containing role, assumptions, evidence, files inspected, proposed changes, tests, and risks. Research gathers evidence, implementation proposes or applies scoped code changes, and code review checks correctness and regressions. Subagents do not recursively spawn agents and do not receive unrestricted tool access.
