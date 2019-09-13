# Reactor
---
While we can rely on K8s to spawn workers that is not true on bare metal
systems or on Windows. To replicate this Thorium has the reactor. The
Thorium reactor periodically polls the Thorium API for information on
its node and spawns/despawns workers to match. This allows us to share
the same agent logic across all systems without making the agent more
complex.