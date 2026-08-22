# Inference is cloud-first, local-optional

Superseded direction: Subsurface originally defaulted to a local model so that
nothing about a Site left the machine. That bought a privacy guarantee at the
cost of answer quality and a hardware floor, and it forced local inference on
users who don't want it.

The default is now a hosted model, reached through whatever provider the user
signs in to. Ollama remains supported as a switch for code that genuinely cannot
leave the machine.

What we keep from the original decision is the part that matters: indexing,
blame, renames, Timelines, and evidence retrieval are always local. Only the
narrowed evidence set for a single Excavate is ever sent, and Subsurface shows
what that is before sending it. The privacy claim becomes "we send this, and
only this, and only when you dig" rather than "nothing leaves" — a weaker
promise, but a true one, and the local switch is there for anyone who needs the
stronger one.
