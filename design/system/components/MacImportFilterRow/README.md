# MacImportFilterRow

The import list's filter row, between the tabs and the rows.

- The Filter field (`filter`, the text typed), then the narrowed count (`narrowed`, such as 2 of 14) while a filter hides rows.
- Its buttons, each opening what it stands for: the identifying ring (`identifying`, how much of the batch is identified, from 0 to 1, in the accent with round ends over a strong hairline: the row's own ring, not `MacProgressRing`), importing (`importing`) and scanning folders (`scanning`), each shown only while that work runs.
- The ⋯ button that opens sorting, filtering and watched folders, with a dot while a filter is on (`filtered`).

**In the code:** macOS: `MacImportFilterRow` (`bae-macos/bae/bae/Views/Import/Candidates/MacImportFilterRow.swift`), in `MacImportSidebar`.
