# Build runners and compute

## Prior research scope

The project has investigated the economics and architecture of build and agent runners across:

- GitHub-hosted runners;
- self-hosted runners;
- macOS capacity in particular;
- Linux x64 and ARM64;
- cloud providers such as AWS, Azure, DigitalOcean, Rackspace and Alibaba Cloud;
- specialist CI/runner providers such as Actuated.

## Factory implications

Compute scheduling should eventually use declared capabilities:

- OS: Windows / Linux / macOS;
- architecture: amd64 / arm64;
- CPU / memory / GPU;
- sandbox/isolation properties;
- installed toolchains;
- network/secrets policy;
- locality/cost/performance;
- warm-cache availability.

The control plane should not assume GitHub Actions is the execution substrate. GitHub Actions may be one provider, and self-hosted runners may avoid GitHub-hosted compute charges. Re-check provider pricing and terms before making implementation decisions.
