# Local POC: OpenCode session binding on Continue

OnTrack 0.2.0 can launch OpenCode before OpenCode publishes a native session ID.
The recorded launch terminal and lifecycle sequence exist, but the native session
field remains absent. Continue then rejects the original idle agent.

The POC correction reuses OnTrack’s existing late-session validation and binding.
Before binding, it verifies the recorded terminal, post-prompt lifecycle activity,
agent kind, pane, workspace and both checkout paths. Continue rechecks the bound
session before submission and retains its durable uncertain-delivery fence.
No registry field is repaired manually and no identity requirement is bypassed.

Regression tests cover successful late binding and refusal for a replaced
terminal, unchanged lifecycle, missing native session, different checkout and
working agent. Existing Continue tests still cover replaced known sessions and
uncertain delivery.

This is a local POC build based on upstream v0.2.0. It has not been published or
submitted upstream. The official binary and the earlier installed binary have
backups outside the project. Runtime evidence lives in the surrounding POC case.
