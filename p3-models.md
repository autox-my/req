# p3 model configuration. One line per role. Delete a line to fall back to the skill default.
# `inherit-parent` as a value: the role runs on the parent thread's model (omit the delegate_task model). Entries in a panel list still count toward its fan-out.
# budget: medium (high)
# req: a unit that touches the Tier-1 path (src/canon.rs, the address) runs as `hardest tasks`, never `bug-fix`.

feature, refactoring: opencode/opencode-go/deepseek-v4.1-flash (high)
bug-fix: opencode/opencode-go/deepseek-v4.1-flash (high)
perf-issue: opencode/opencode-go/deepseek-v4.1-flash (high)
hillclimb: opencode/opencode-go/deepseek-v4.1-flash (high)
judgment and prose: claudeAgent/claude-sonnet-5-5 (high)
hardest tasks: claudeAgent/claude-sonnet-5-5 (high)
how explorer: opencode/opencode-go/deepseek-v4.1-flash (high)
how explainer: claudeAgent/claude-sonnet-5-5 (high)
why investigators: opencode/opencode-go/deepseek-v4.1-flash (high)
why synthesizer: claudeAgent/claude-sonnet-5-5 (high)
reflect tooling: opencode/opencode-go/deepseek-v4.1-flash (high)
reflect judgment, divergent, synthesizer: claudeAgent/claude-sonnet-5-5 (high)
arena runners: opencode/opencode-go/deepseek-v4.1-flash (high), opencode/opencode-go/glm-5.3-flash (high), claudeAgent/claude-sonnet-5-5 (high)
arena cross-judge pool: opencode/opencode-go/deepseek-v4.1-flash (high), opencode/opencode-go/glm-5.3-flash (high), claudeAgent/claude-sonnet-5-5 (high)
swarm workers: opencode/opencode-go/deepseek-v4.1-flash (high)
architect runners: claudeAgent/claude-sonnet-5-5 (high)
interrogate reviewers: opencode/opencode-go/glm-5.3-flash (high)
