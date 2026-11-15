# Grid numeric display auto fit

The owner chose automatic numeric shrinking in Grid after observing hash displays in a household
workbook. Implement this on `my-custom`, inline without subagents. This request authorizes the
source change, not a release or replacing the installed app.

## Approaches

Automatic display-only shrinking preserves the workbook layout and is the chosen approach.
Changing the workbook's Shrink to Fit flags would change the saved document; expanding columns
would change the layout. Numeric spill into adjacent cells makes column ownership ambiguous.

## Contract

- Add a persisted, enabled-by-default Grid preference `autoFitNumbers`, exposed in File > Options
  > General as an English/Korean checkbox and `options.autoFitNumbers` UI command.
- On-screen numeric cells that would otherwise exceed their usable width shrink to fit. Keep
  all formatted digits, separators, signs and number-format colours; expand accounting fill
  only after choosing the smaller font. Fit ordinary and merged cells, zoom and Page Layout.
- General numbers use their ordinary General formatted output before shrinking, avoiding a
  width-driven hash or rounding before layout. Explicit workbook Shrink to Fit also uses this.
- Preserve text overflow/wrap, Fill alignment and rotated-text behaviour. Wrapped or rotated
  numeric cells retain their explicit layout. Invalid dates keep their existing hashes.
- The preference is UI-only: no workbook mutation, dirty flag, undo item, column resize, stored
  font-size change or XLSX alignment flag. Turning it off restores width hashes and General
  narrowing; explicit Shrink to Fit still works, including accounting repeat-fill formats.
- Reuse the existing bounded font-shrink helper; only refine its measured fit if regressions
  demonstrate a rounding problem. Extreme widths cannot promise readable digits; never spill
  numbers into neighbours or silently drop leading digits to make them fit.

Verify with synthetic render regressions and the real workbook through private temporary files;
keep workbook values/screenshots outside git. Document the difference between Grid's display
preference and exported document formatting in the existing Grid maintenance wiki.
