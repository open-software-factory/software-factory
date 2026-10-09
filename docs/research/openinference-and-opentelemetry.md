---
title: OpenInference and OpenTelemetry for factory telemetry
document_type: research
status: concluded
date: 2026-10-09
---
# OpenInference and OpenTelemetry for factory telemetry

## Summary

The OpenInference project defines a semantic-convention layer for tracing AI
applications. Arize maintains it under the Apache-2.0 licence. It is not a
telemetry transport of its own. Every instrumentation observed routes its spans
through OpenTelemetry.

The research reached three findings.

1. The transport to choose is OpenTelemetry. OpenInference, OpenLLMetry and the
   OpenTelemetry GenAI conventions all emit OpenTelemetry spans over OTLP.
2. The factory keeps its own record. Work items, runs, review attempts, gates and
   the evidence ledger stay in factory types. Traces join to them by trace and
   span identifiers.
3. A thin edge adapter is worth having later. OpenInference is richest today for
   retrieval and reranking. It is a poor fit as the core record.

The recommendation is to adopt part of it later. Use OTLP as the transport. Keep
factory work events authoritative. Add an optional edge mapping when a real seam
appears. Revisit when
[Arize-ai/openinference#2817 (the compatibility layer)](https://github.com/Arize-ai/openinference/issues/2817)
and [Arize-ai/openinference#2983 (the gen_ai emission)](https://github.com/Arize-ai/openinference/issues/2983)
land, or when the OpenTelemetry GenAI conventions leave development stability.

## The convention

The convention defines one required attribute, `openinference.span.kind`. The
allowed kinds are `LLM`, `EMBEDDING`, `CHAIN`, `RETRIEVER`, `RERANKER`, `TOOL`,
`AGENT`, `GUARDRAIL`, `EVALUATOR`, `PROMPT` and `DECISION`. It also defines
namespaced attributes such as `llm.*`, `retrieval.*`, `message.*` and `tool.*`.
The scope covers model calls, chains, retrieval, embeddings, rerankers, agents,
tools, guardrails, prompt rendering and evaluations.

Two facts matter for adoption. The spec is transport-agnostic in intent. In
practice every instrumentation observed uses OpenTelemetry. The spec itself is
not versioned. Only the per-language packages carry versions.

## The relationship to OpenTelemetry

The project describes itself as complementary to OpenTelemetry. Its
instrumentations depend on the OpenTelemetry API and SDK. They emit standard
OpenTelemetry spans. Any OTLP backend can read them.

The friction is in the attribute names. This convention does not reuse the
`gen_ai.*` namespaces. It uses its own names to avoid attribute conflicts.
[Arize-ai/openinference#2130 (the relation question)](https://github.com/Arize-ai/openinference/issues/2130)
asks how the two relate. That issue is still open. A maintainer reply records
that there is no documented mapping. Two further issues,
[Arize-ai/openinference#2817 (the compatibility layer)](https://github.com/Arize-ai/openinference/issues/2817)
and [Arize-ai/openinference#2983 (the gen_ai emission)](https://github.com/Arize-ai/openinference/issues/2983),
show a move toward `gen_ai.*`.

The convergence is in progress. It is not settled. A decision to depend on these
attributes should stay reversible. Do not put them in the factory core.

## Alternatives

| Convention | Maintainer | Attribute namespace | Stability |
| --- | --- | --- | --- |
| OpenInference | Arize | `openinference.*`, `llm.*` | No spec-level field |
| OpenTelemetry GenAI | OpenTelemetry | `gen_ai.*` | development |
| OpenLLMetry | Traceloop | `gen_ai.*` | Follows OpenTelemetry |

The OpenTelemetry GenAI conventions live in the
open-telemetry/semantic-conventions-genai repository. They are vendor-neutral.
They are younger and weaker on retrieval and reranking. The OpenLLMetry project
states that its conventions are now part of OpenTelemetry. That is a signal that
the ecosystem centre is the OpenTelemetry vocabulary.

## Practicalities

- The reference backend, Arize Phoenix, uses the Elastic License 2.0. That is not
  an open-source licence.
- Instrumentations ship for Python, JavaScript, Java and Go.
- No first-party Rust SDK exists. The only Rust crate found is third-party and
  single-maintainer. Do not build the Rust core on it.
- Lock-in is low, because the spans travel over OTLP.

## Fit for this factory

The repository has no OpenInference references. The substrate is an open
question. Three documents record that.

- [docs/architecture/open-questions.md](../architecture/open-questions.md) asks
  where the evidence ledger lives, and whether OpenTelemetry is the canonical
  substrate.
- [docs/product/ux/open-questions.md](../product/ux/open-questions.md) asks how
  OpenTelemetry-style traces relate to higher-level work events.
- [docs/research/ahp-acp-architecture-direction.md](ahp-acp-architecture-direction.md)
  treats OpenTelemetry signals as optional, and asks how they correlate with
  factory events.

Factory concepts are work items, runs, review lenses, gates and the evidence
ledger. No candidate convention models them. The relevant surface of
OpenInference is only the model and harness call layer.

## Recommendation

1. Use OTLP as the trace transport.
2. Keep the factory work-event and evidence schema native and authoritative.
   Correlate by trace and span identifiers.
3. Add a thin edge adapter when a real seam appears. Map `openinference.*` span
   kinds to factory run and review roles. Preserve the raw attributes.
4. Take no core dependency on the unofficial Rust crate.
5. Revisit when
   [Arize-ai/openinference#2817 (the compatibility layer)](https://github.com/Arize-ai/openinference/issues/2817)
   or [Arize-ai/openinference#2983 (the gen_ai emission)](https://github.com/Arize-ai/openinference/issues/2983)
   land, or when the OpenTelemetry GenAI conventions reach stable.

The follow-up work is tracked in
[open-software-factory/software-factory#258 (the evaluation issue)](https://github.com/open-software-factory/software-factory/issues/258).

## Limits

- No attribute-level mapping between OpenInference and `gen_ai.*` exists. This
  note does not enumerate the overlap.
- Whether Arize Phoenix gives OpenInference attributes precedence over `gen_ai.*`
  is unverified.
- The spec has no stability field. This note cannot state compatibility
  guarantees.
- The Rust crate was not tested.
- No OTLP round trip was tested.

## Sources

- [Arize-ai/openinference](https://github.com/Arize-ai/openinference) and its
  [semantic conventions](https://raw.githubusercontent.com/Arize-ai/openinference/main/spec/semantic_conventions.md)
- [open-telemetry/semantic-conventions-genai](https://github.com/open-telemetry/semantic-conventions-genai)
- [traceloop/openllmetry](https://github.com/traceloop/openllmetry)
- [Arize-ai/phoenix](https://github.com/Arize-ai/phoenix)
- [openinference-semantic-conventions on crates.io](https://crates.io/crates/openinference-semantic-conventions)
- [Arthur.ai comparison](https://www.arthur.ai/column/openinference-vs-opentelemetry-genai-conventions-agent-tracing)
