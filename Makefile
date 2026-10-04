.PHONY: deploy

# Release the exact, committed dev tip. The workflow performs validation,
# tagging, release-note generation, promotion, building, publication and
# verification; this target only dispatches it.
deploy:
	@set -eu; \
	git fetch origin dev main --tags; \
	test "$$(git branch --show-current)" = dev || { echo "make deploy must run from dev"; exit 1; }; \
	test -z "$$(git status --porcelain)" || { echo "make deploy requires a clean working tree"; exit 1; }; \
	test "$$(git rev-parse HEAD)" = "$$(git rev-parse origin/dev)" || { echo "local dev is not the pushed dev tip"; exit 1; }; \
	git merge-base --is-ancestor origin/main HEAD || { echo "main is not an ancestor of dev"; exit 1; }; \
	gh workflow run Release --repo chrisle/rbxport --ref dev; \
	echo "Release workflow dispatched from $$(git rev-parse --short HEAD)."
