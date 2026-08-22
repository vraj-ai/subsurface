# Findings must cite Evidence

Subsurface uses a language model to state why code exists in prose, rather than
only listing commits for the reader to interpret. The obvious failure of that
approach is confabulation: most repositories have commit messages like "fix" or
"wip", and a model fed those will produce a fluent, wrong history — dangerous
here, because the product's whole purpose is telling someone whether code is
safe to remove.

So a Finding may only assert a rationale it can tie to specific Evidence. With
thin Evidence, the correct output is "no recorded rationale, here is what
touched this code" and a Confidence of zero. Confidence is a safety mechanism,
not a UI ornament.
