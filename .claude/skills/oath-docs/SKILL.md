---
name: oath-docs
description: Oath session start and the end-of-slice doc update. Use at the start of work in the Oath repo and before every commit that changes behavior, capability status, priority, or a machine's state.
---

# Oath docs

## Start of a session

1. Read `AGENTS.md`, then `CURRENT.md`.
2. Open `docs/decisions.md`. If the slice touches an **Open** item, ask
   Joshua before acting.
3. Read only the spec in `docs/specs/` for your domain and its row in
   `docs/capabilities.md`.

## End of a slice (same commit as the code)

1. `docs/capabilities.md`: update the row (status, works, main gaps).
   Stay under 800 words; shorten other rows rather than grow the file.
2. `CURRENT.md`: replace the Now item you worked on with its new
   state and next step. Keep ≤ 80 lines and ≤ 5 items. Remove what is
   done; git keeps history.
3. `docs/manual/`: only if shipped, operator-visible behavior changed.
4. `docs/specs/<domain>.md`: if the design changed. Label "is" vs
   "will be".
5. `docs/architecture.md`: if a process, path or tree changed.
6. `hosts/<name>.md`: overwrite facts about a machine you touched.
7. `docs/decisions.md`: record any decision Joshua made, and close or
   add Open items.
8. `docs/roadmap.md`: only when a whole phase changes status.

Do not create session diaries, handoff files, `STATUS.md` or plan
checklists. Write plain declarative sentences with absolute dates.
