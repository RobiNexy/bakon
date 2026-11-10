# 2026-11-10-2032__add-reasoning-and-go-procedures

- Mode: Copilot
- Task: Save two user-supplied capability-activation instructions as on-demand procedures
- Did: Added `AGENT__procedure__formal-problem-solving` and `AGENT__procedure__go-engineering` under `30__PROCEDURES/`, registered both in the entry trigger table; kept them out of `10__ALWAYS_LOAD/` (scope-specific, and the always-load word budget must stay small)
- Verified: `lint.sh` → 0 errors, 0 warnings; entry plus `10__ALWAYS_LOAD/` = 2192 words (limit 3000)
- Touched: [[00__AGENT__START_HERE]], [[AGENT__procedure__formal-problem-solving]], [[AGENT__procedure__go-engineering]]
- Next: If these should be always-on rather than task-triggered, move them to `10__ALWAYS_LOAD/` and re-check the word budget
