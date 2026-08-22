# Providers: presets, any OpenAI-compatible endpoint, and OAuth where offered

Subsurface must work with whatever the user already pays for. Two paths, both in
the first build:

- **Key** — a base URL, a key, and a model name. Presets for OpenAI, Grok,
  OpenRouter, and OpenCode Zen fill the first two. Anything OpenAI-compatible,
  including a local Ollama, works with no code change on our side.
- **OAuth** — sign-in for the providers that offer it.

The key path alone would cover every provider on the list, so OAuth is bought
purely for the sign-in experience, and it is not cheap: each provider is its own
client registration, consent screen, and refresh-token handling, and a provider's
approval timeline can block a release. Accepted deliberately. If OAuth threatens
the milestone, the key path is the fallback that still ships a working product,
and OAuth lands provider by provider afterwards.

Subsurface ships no curated model list. The model is a text field with
suggestions, so a model launching or dying is not our release.
