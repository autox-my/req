# Handoff brief — one step of adopting req, for another project's coding agent

Copy the block below into the task you give the agent, fill in the angle-bracket parts, and leave the
rest. It is a pointer, not a restatement: the step, its commands and its deciding line live in
`docs/adopting.md` § *For an agent* at the req rev you pin, and `req --help` is the command reference.
The agent also needs the block from `examples/agents-snippet.md` in its project's `AGENTS.md`.

```text
STEP        <the one step, by its title in docs/adopting.md § For an agent, e.g. "Bring your own spec">
REQ PIN     <sha>. Precheck, before any other command: `req --version` must show
            claim_schema req.claim/3, rules_v 9 and built from <sha>. If it does not, stop and
            report what it shows. Do not install, rebuild or work around it.
INPUT       <the file in your repo the step reads, e.g. docs/capabilities.md>
DONE WHEN   <the exit code and the named line the step ends on, copied from docs/adopting.md,
            e.g. `req canon` exit 0 and a last line beginning `claims: <n>`>
MAY NOT     the prohibitions in examples/agents-snippet.md. Never edit a pin to fit a parse, never
            relax a check, and an exit 3 is not a pass.
COMMANDS    `req --help` lists them; `req <command> --help` is the contract and its `exit:` line.
REPORT      every req command you ran, its exit code, and its first and last output line; any
            exit 3 or flagged pin verbatim; then stop. Do not start the next step.
```

Why it is shaped like this: a step is checkable from the report alone (commands, exit codes, deciding
lines), without reading the agent's transcript, and the precheck turns "the wrong req is installed"
from a confusing failure three commands later into the first thing the agent says.
