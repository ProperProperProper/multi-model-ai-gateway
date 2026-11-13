# Context engineering and local graph

Thomas separates reusable instructions from context interfaces. `manifest.json` lists small rules, conventions, specs, skills, and interfaces. `src/context.rs` selects only matching entries at request time, so routine requests do not receive the whole repository policy pack.

The local Graphify-style subsystem in `src/graph.rs` creates a deterministic code knowledge graph without a cloud service or vector database. It powers:

- `thomas-ai graph scan`
- `thomas-ai graph query --query TERM`
- `/api/graph`
- `/api/graph/query`
- `/api/graph/report`
- `/graph.html`

The graph is intended to give Thomas and its research/implementation/review subagents precise file and symbol relationships while preserving the typed ontology/security boundary around tools.
