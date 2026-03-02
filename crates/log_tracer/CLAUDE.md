# log_tracer

Core analysis engine for parsing terminal logs, searching code, and generating call graphs. Inspired by Process Mining and OpenTelemetry concepts.

## Module Structure

```
src/
├── log_tracer.rs         # Library root, data models (Trace, Span, SpanKind)
├── parser/               # Log parsing
│   ├── mod.rs            # LogEntry, SpanExtractor, parse_timestamp
│   ├── rule_engine.rs    # LogParseRule, CompiledLogRule, LogParseRuleStore
│   └── keyword_matcher.rs # Aho-Corasick multi-pattern matching
├── code_server_config.rs # SSH Docker server configuration
├── code_search/          # Code source abstraction
│   ├── mod.rs            # CodeSource trait, FunctionLocation
│   ├── docker.rs         # DockerCodeSource (batch grep in containers)
│   ├── ssh_docker.rs     # SshDockerCodeSource (SSH + Docker via russh)
│   ├── local.rs          # LocalCodeSource (filesystem search)
│   └── file_cache.rs     # LRU file content cache
├── language/             # Language analyzers
│   ├── mod.rs            # LanguageAnalyzer trait, LanguageRegistry
│   ├── c_analyzer.rs     # C/C++ function extraction
│   └── lua_analyzer.rs   # Lua function extraction
├── call_graph/           # Graph structures
│   ├── mod.rs            # CallGraph, CallGraphNode, CallEdge, DFG
│   ├── builder.rs        # CallGraphBuilder, IncrementalBuilder
│   └── merger.rs         # merge_graphs for combining multiple graphs
├── pipeline/             # Analysis pipeline
│   ├── mod.rs            # AnalysisPipeline, AnalysisStep, AnalysisContext
│   ├── log_parse.rs      # LogParseStep (parallel parsing)
│   ├── function_search.rs # FunctionSearchStep (batch code search)
│   ├── graph_build.rs    # GraphBuildStep (span to graph)
│   └── branch_mark.rs    # BranchMarkStep (detect branches)
└── renderer/             # Graph output
    ├── mod.rs            # GraphRenderer trait, RenderOptions
    ├── dot.rs            # DotRenderer (Graphviz)
    └── mermaid.rs        # MermaidRenderer
```

## Key Types

| Type | Purpose |
|------|---------|
| `Trace` | Collection of spans representing a log analysis session |
| `Span` | Single function call record with timing and context |
| `SpanKind` | Entry, Internal, or Exit span classification |
| `LogParseRule` | User-configurable log parsing rule |
| `CompiledLogRule` | Pre-compiled regex version for fast matching |
| `CodeSource` | Async trait for code searching (Docker/Local/SshDocker) |
| `CodeServerConfig` | SSH + Docker server configuration |
| `SshDockerCodeSource` | Code search via SSH + Docker using russh |
| `CallGraph` | Petgraph-based directed graph of function calls |
| `CallGraphNode` | Node with name, location, call count, duration |
| `CallEdge` | Edge with call count, sequence numbers, branch flag |
| `AnalysisPipeline` | Pluggable step-based analysis pipeline |

## Data Model (OpenTelemetry-inspired)

```
Trace
├── trace_id: UUID
├── name: String
├── start_time, end_time: DateTime
├── spans: Vec<Span>
└── root_spans: Vec<SpanId>

Span
├── span_id: UUID
├── parent_span_id: Option<UUID>
├── operation_name: String (function name)
├── kind: SpanKind (Entry/Internal/Exit)
├── start_time, end_time: DateTime
├── code_location: Option<CodeLocation>
├── attributes: HashMap<String, AttributeValue>
└── status: SpanStatus (Ok/Error/Unset)
```

## Default Log Parse Rules

| Rule Name | Pattern | Use Case |
|-----------|---------|----------|
| Module Timestamp | `^(?P<timestamp>...) [(?P<module>...)] (?P<message>...)$` | Structured logs |
| Lua Trace | `^\[TRACE\] (?P<file>...):(?P<line>...) (?P<message>...)$` | Lua debug output |
| Printf Debug | `^(?P<message>.*)$` with `>>>` / `<<<` markers | Simple printf tracing |
| Standard Log | `^(?P<timestamp>...) (?P<level>...) (?P<message>...)$` | Generic logs |

## Pipeline Architecture

```
LogParseStep → FunctionSearchStep → GraphBuildStep → BranchMarkStep
     │                 │                  │               │
  Parse log      Search code for      Build call      Mark branch
  entries with   function defs in     graph from      edges where
  parallel       Docker/local         spans           node has >1
  processing                                          outgoing edge
```

## Usage Example

```rust
use log_tracer::{
    AnalysisContext, AnalysisPipeline, LogParseRuleStore,
    code_search::DockerCodeSource,
};

// Create context with log content
let mut ctx = AnalysisContext::new(log_content)
    .with_rule(rule)?
    .with_code_source(Arc::new(DockerCodeSource::new("container_id", "/usr1")));

// Run analysis pipeline
let pipeline = AnalysisPipeline::default_pipeline();
pipeline.run(&mut ctx)?;

// Get result
if let Some(graph) = ctx.graph {
    let dot = log_tracer::render_dot(&graph, &DotOptions::default());
    println!("{}", dot);
}
```

## Performance Optimizations

| Optimization | Description |
|--------------|-------------|
| Aho-Corasick | Multi-pattern string matching for keyword extraction |
| Batch grep | Single Docker exec for multiple function searches |
| File cache | LRU cache for file contents (default: 100 files) |
| Parallel parsing | Rayon-based parallel log line processing |
| Path compression | Merge repeated call edges |

## Persistence

- **Rules**: `~/.config/bspterm/log_parse_rules.json`
- **Code Server**: `~/.config/bspterm/code_server.json`
- **Format**: JSON with version field for migrations

## Dependencies

- `petgraph` - Graph data structures
- `aho-corasick` - Fast multi-pattern matching
- `rayon` - Parallel processing
- `chrono` - Timestamp handling
- `regex` - Pattern matching
- `async-trait` - Async trait support
- `tokio` - Async runtime (for code search)
- `russh` - Pure Rust SSH client (for SshDockerCodeSource)

## Testing

```sh
cargo test -p log_tracer
```

43 tests covering:
- Log parsing and keyword extraction
- Call graph building and merging
- DOT and Mermaid rendering
- C and Lua language analyzers
- Pipeline step execution
- Code server configuration
- SSH Docker code source

## Pitfalls

- `DockerCodeSource` requires `docker` CLI in PATH
- Batch grep patterns can get very long with many functions
- Lua analyzer counts block depth for `function`/`if`/`for`/`while`/`repeat` - not `then`/`do`
- C analyzer uses brace counting which can fail with macros
- `CompiledLogRule::compile()` can fail if regex patterns are invalid
- `SshDockerCodeSource` uses password authentication only (no SSH key support currently)
- SSH connection is reused across commands but may reconnect if disconnected
