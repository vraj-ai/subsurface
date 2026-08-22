# Agents reach Subsurface over MCP, and share its Field Notes

Subsurface must be useful alone and useful alongside the user's agent skills,
without either depending on the other. The seam is an MCP server the app
exposes: agents ask for evidence about a region of code and get structured
Findings back. Nothing about the app requires a connected agent, and the skills
keep working with no Subsurface installed.

Field Notes are the shared store on both sides of that seam. An agent's question
returns a saved Finding when one exists and triggers an Excavate when it does
not, so an agent's digging accumulates in the app's history and the app's
digging answers the agent's questions. Without the shared store the integration
is just a slower way to run git log.

Rejected: running skills inside Subsurface. That makes it a worse Claude Code
and inverts the dependency.
