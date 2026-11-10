# Vendored skill library

These skills are copied from open-source repositories and embedded in the Retinue daemon at build time.
Each is pinned to the commit shown. Only Markdown files are kept: SKILL.md and the Markdown references
it links to in the same folder. Scripts, binaries, assets, tool configuration (such as `agents/*.yaml`)
and examples in other languages were left out, so a reference to a script a skill does not ship points at nothing.
Licence texts (and NOTICE files where a repository has one) are in `licenses/`. Skill text is unmodified, except that line endings are normalised to LF.

| Skill | Source repository | Path | Commit | Licence | Author | Licence file |
|---|---|---|---|---|---|---|
| accessibility-audit | https://github.com/NickCrew/claude-cortex | `skills/accessibility-audit` | `bb47af79ad3befe01ae01940fcf5f16e30a1b6df` | MIT | NickCrew | licenses/NickCrew-claude-cortex.txt (from `LICENSE`) |
| api-design | https://github.com/affaan-m/ECC | `skills/api-design` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| architect | https://github.com/cursor/plugins | `pstack/skills/architect` | `236e5481d05cbadc315b374a5de54971858c27bb` | MIT | Cursor (cursor/plugins, pstack) | licenses/cursor-plugins.txt (from `pstack/LICENSE`) |
| benchmark-checklist | https://github.com/cursor/plugins | `pstack/skills/benchmark-checklist` | `236e5481d05cbadc315b374a5de54971858c27bb` | MIT | Cursor (cursor/plugins, pstack) | licenses/cursor-plugins.txt (from `pstack/LICENSE`) |
| blast-radius | https://github.com/cursor/plugins | `pstack/skills/blast-radius` | `236e5481d05cbadc315b374a5de54971858c27bb` | MIT | Cursor (cursor/plugins, pstack) | licenses/cursor-plugins.txt (from `pstack/LICENSE`) |
| brand-guidelines | https://github.com/anthropics/skills | `skills/brand-guidelines` | `2c5958ecac7cee2b583967a2c4981d0f4a75c579` | Apache-2.0 | Anthropic | licenses/anthropics-skills--brand-guidelines.txt (from `skills/brand-guidelines/LICENSE.txt`) |
| caveman | https://github.com/JuliusBrussee/caveman | `skills/caveman` | `3dc497a7f858c774d6e15b25e8c6120ef3a0eda8` | Apache-2.0 | Julius Brussee (JuliusBrussee) | licenses/JuliusBrussee-caveman.txt, licenses/JuliusBrussee-caveman.NOTICE.txt (from `LICENSE`) |
| code-review | https://github.com/mattpocock/skills | `skills/engineering/code-review` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| copywriting | https://github.com/coreyhaines31/marketingskills | `skills/copywriting` | `e690ca89862734c4b8deea18f9e32fceee59ebf5` | MIT | Corey Haines (coreyhaines31) | licenses/coreyhaines31-marketingskills.txt (from `LICENSE`) |
| database-migrations | https://github.com/affaan-m/ECC | `skills/database-migrations` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| deployment-patterns | https://github.com/affaan-m/ECC | `skills/deployment-patterns` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| diagnosing-bugs | https://github.com/mattpocock/skills | `skills/engineering/diagnosing-bugs` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| domain-modeling | https://github.com/mattpocock/skills | `skills/engineering/domain-modeling` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| e2e-testing | https://github.com/affaan-m/ECC | `skills/e2e-testing` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| frontend-design | https://github.com/anthropics/skills | `skills/frontend-design` | `2c5958ecac7cee2b583967a2c4981d0f4a75c579` | Apache-2.0 | Anthropic | licenses/anthropics-skills--frontend-design.txt (from `skills/frontend-design/LICENSE.txt`) |
| frontend-patterns | https://github.com/affaan-m/ECC | `skills/frontend-patterns` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| grill-me | https://github.com/mattpocock/skills | `skills/productivity/grill-me` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| handoff | https://github.com/mattpocock/skills | `skills/productivity/handoff` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| i-have-adhd | https://github.com/ayghri/i-have-adhd | `skills/i-have-adhd` | `1ecea1993b7fce257ffbd686c070948b5e07eda6` | MIT | Ayoub Ghriss (ayghri) | licenses/ayghri-i-have-adhd.txt (from `LICENSE`) |
| internal-comms | https://github.com/anthropics/skills | `skills/internal-comms` | `2c5958ecac7cee2b583967a2c4981d0f4a75c579` | Apache-2.0 | Anthropic | licenses/anthropics-skills--internal-comms.txt (from `skills/internal-comms/LICENSE.txt`) |
| no-comments | https://github.com/cursor/plugins | `pstack/skills/no-comments` | `236e5481d05cbadc315b374a5de54971858c27bb` | MIT | Cursor (cursor/plugins, pstack) | licenses/cursor-plugins.txt (from `pstack/LICENSE`) |
| pandas-pro | https://github.com/jeffallan/claude-skills | `skills/pandas-pro` | `078a25cc5bfd8011459be7b91d004e900c1d02d3` | MIT | jeffallan | licenses/jeffallan-claude-skills.txt (from `LICENSE`) |
| postgres-patterns | https://github.com/affaan-m/ECC | `skills/postgres-patterns` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| pr | https://github.com/mattpocock/skills | `skills/engineering/pr` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| security-review | https://github.com/affaan-m/ECC | `skills/security-review` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| security-scan | https://github.com/affaan-m/ECC | `skills/security-scan` | `d2ef2680594f9a17526a6e1a2e18b3d242c5d17b` | MIT | affaan-m (ECC) | licenses/affaan-m-ECC.txt (from `LICENSE`) |
| systematic-debugging | https://github.com/obra/superpowers | `skills/systematic-debugging` | `22f0b463e7a5e579904dd6211a520321035526f7` | MIT | Jesse Vincent (obra), with Prime Radiant | licenses/obra-superpowers.txt (from `LICENSE`) |
| tdd | https://github.com/mattpocock/skills | `skills/engineering/tdd` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| technical-writing | https://github.com/cursor/plugins | `pstack/skills/technical-writing` | `236e5481d05cbadc315b374a5de54971858c27bb` | MIT | Cursor (cursor/plugins, pstack) | licenses/cursor-plugins.txt (from `pstack/LICENSE`) |
| triage | https://github.com/mattpocock/skills | `skills/engineering/triage` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| typescript-best-practices | https://github.com/cursor/plugins | `pstack/skills/typescript-best-practices` | `236e5481d05cbadc315b374a5de54971858c27bb` | MIT | Cursor (cursor/plugins, pstack) | licenses/cursor-plugins.txt (from `pstack/LICENSE`) |
| unslop | https://github.com/cursor/plugins | `pstack/skills/unslop` | `236e5481d05cbadc315b374a5de54971858c27bb` | MIT | Cursor (cursor/plugins, pstack) | licenses/cursor-plugins.txt (from `pstack/LICENSE`) |
| writing-for-agents | https://github.com/mattpocock/skills | `skills/productivity/writing-for-agents` | `4ae2317ddb22b046e26455e4f9c9b3aaaf62212d` | MIT | Matt Pocock (mattpocock) | licenses/mattpocock-skills.txt (from `LICENSE`) |
| writing-plans | https://github.com/obra/superpowers | `skills/writing-plans` | `22f0b463e7a5e579904dd6211a520321035526f7` | MIT | Jesse Vincent (obra), with Prime Radiant | licenses/obra-superpowers.txt (from `LICENSE`) |

## Not vendored

Skipped because the licence could not be confirmed as MIT or Apache-2.0 from a LICENSE file, because the skill
needs scripts or assets to work, or because it is not open source: Anthropic's docx, xlsx, pptx and pdf
(source-available), webapp-testing and mcp-builder (they ship scripts), skill-creator and doc-coauthoring (no
licence stated), impeccable (needs its `scripts/` programs), design-taste-frontend (needs 1,600 asset and script
files), tdd-workflow (calls ECC scripts), ComposioHQ's content-research-writer, invoice-organizer and
file-organizer (the repository has no LICENSE file).
