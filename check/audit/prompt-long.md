Finish today's (@DAY@) security audit of Oath's packages. An earlier run of the
`oath-audit` skill (`.claude/skills/oath-audit/SKILL.md`) moved pins and left them
uncommitted in the tree, with a draft report at @REPORT@, because their rebuild lasts
longer than a command may. The runner has built them: `cargo make pkgs -j 8` exited
@BUILT@, and its output is in @LOG@.

If it exited 0, carry on from the skill's step 4. If not, read why, undo the change
that failed as step 4 says, carry on with the rest, and report what failed. Do not
start the long rebuild again. No one answers questions, so what needs Joshua goes in
the report.

Replace the draft at @REPORT@ with the finished report (its first line is the mail's
subject). If you published a release, write its output id, alone, to @PUBLISHED@. If
you published nothing, do not create that file.
