.PHONY: deploy

SKIP_TESTS ?= false
SKIP_VERSION_BUMP ?= false

# Release the exact, committed dev tip. The workflow performs validation,
# tagging, release-note generation, promotion, building, publication and
# verification; this target only dispatches it.
deploy:
	@set -eu; \
	case "$(SKIP_TESTS)" in true|false) ;; *) echo "SKIP_TESTS must be true or false"; exit 1;; esac; \
	case "$(SKIP_VERSION_BUMP)" in true|false) ;; *) echo "SKIP_VERSION_BUMP must be true or false"; exit 1;; esac; \
	git fetch origin dev main --tags; \
	test "$$(git branch --show-current)" = dev || { echo "make deploy must run from dev"; exit 1; }; \
	test -z "$$(git status --porcelain)" || { echo "make deploy requires a clean working tree"; exit 1; }; \
	test "$$(git rev-parse HEAD)" = "$$(git rev-parse origin/dev)" || { echo "local dev is not the pushed dev tip"; exit 1; }; \
	git merge-base --is-ancestor origin/main HEAD || { echo "main is not an ancestor of dev"; exit 1; }; \
	gh workflow run Release --repo chrisle/rbxport --ref dev -f skip_tests=$(SKIP_TESTS) -f skip_version_bump=$(SKIP_VERSION_BUMP); \
	echo "Release workflow dispatched from $$(git rev-parse --short HEAD)."
