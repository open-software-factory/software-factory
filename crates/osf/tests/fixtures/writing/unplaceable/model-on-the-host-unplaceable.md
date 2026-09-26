This fix is split across a Windows machine and a container. Run the build on the host, then copy the artefact into the container for the test stage. If the container step fails, rerun it on the host and diff the two logs.

<!-- osf-unplaceable
label: unplaceable
kind: model
target: on the host
reason: ambiguous between two machines; only a model can resolve which one "the host" means
-->
