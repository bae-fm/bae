# MacFactsGrid

Label and value pairs in two columns, the labels in secondary text: a file's facts in the Storage Manager's inspector (`MacFileDetails`), a release's fields, an album's credits.

| Prop | |
| --- | --- |
| `facts` | The pairs: a list of `[label, value]`, or text with one `Label: value` a pair, separated by semicolons. A pair with no value is left out. |
| `numeric` | The labels whose values are numbers, drawn in tabular figures: Size. |
| `wide` | A 140pt label column and body-sized values, as an album's credits draw them. |

**In the code:** macOS: `MacFactsGrid` (`Views/Components/MacFactsGrid.swift`), holding `MacFact` rows.
