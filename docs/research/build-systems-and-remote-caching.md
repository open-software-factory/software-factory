# Build systems and remote caching

## Prior research topic

Moonrepo was discussed as a possible piece of the build/caching story, particularly because the factory will need to support heterogeneous repositories and avoid redundant work.

Expected repository languages include:

- Go;
- TypeScript/JavaScript;
- .NET;
- Flutter/Dart;
- Python;
- Java;
- potentially others.

## Key architectural question

The factory should not become a build system by default.

Keep these responsibilities separate where possible:

- build/test graph knowledge owned by repo-native tooling;
- caching owned by build-system/cache providers where possible;
- factory-level deduplication of semantically identical verification requests;
- scheduling based on inputs, outputs and execution capabilities.

## Research leads

When this becomes active work, compare current versions of Moonrepo, Bazel, Nx, Turborepo and language-native build/test tooling. Look specifically at remote caching, cross-language support, hermeticity, task-graph introspection and CI portability.
