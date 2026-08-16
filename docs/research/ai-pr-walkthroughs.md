# AI PR walkthroughs and human code understanding

## Prior research topic

The project has investigated human-friendly PR walkthroughs for understanding AI-generated code.

Higher agent throughput can create a human comprehension bottleneck even when the code is correct. Exception-only governance still requires people to understand significant changes, architectural consequences and the reasons behind a decision.

## Factory implications

A future delivery and review surface should present:

- intent/spec summary;
- architectural impact;
- dependency/data-flow changes;
- important decisions and rejected alternatives;
- verification evidence and gate results;
- risk/uncertainty hotspots;
- suggested review path through the change;
- traceability back to acceptance criteria/ADR.

This capability belongs near the human exception and review boundary. It need not be part of the kernel.
