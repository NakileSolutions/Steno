---
name: screenshots
description: Regenerate the documentation screenshots (docs/screenshots/) with the "captures" Playwright project, and check they only show made-up data. Use for "update the screenshots", "redo the README screenshots".
---

# Screenshots

The `captures` Playwright project (`app/ui/e2e/captures/`) runs `steno serve` on the
synthetic daily meeting (`fixtures/daily-45min.jsonl`) and **rewrites `docs/screenshots/`**.

1. `cd app/ui && npm run captures`.
2. `git status docs/screenshots/`: only the intended screenshots should change. Restore the
   others (`git restore docs/screenshots/<file>`), including deleted files.
3. **Open every changed screenshot** and check: no real names (company, customer, colleague,
   internal project or tool), no personal paths, no addresses. The data comes from
   `fixtures/` and `app/ui/e2e/fixtures/`: if a real name shows up, fix the fixture (or
   `fixtures/generate.py`, then `make fixtures`), not the screenshot.
4. Check the light and dark variants, and both languages if the screen is affected.
5. Commit the PNGs together with the UI change that justifies them.
