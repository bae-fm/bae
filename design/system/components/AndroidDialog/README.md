# AndroidDialog

For a decision: changing a value, confirming an action, a choice that has to be made, an error to acknowledge. Ends with `AndroidDialogActions`. Something to look at with nothing to decide, such as a library's details, is an `AndroidSheet`. **On iOS** (not built yet) a decision is an alert, or a confirmation dialog for a destructive choice, and changing a value is a sheet with Cancel and Done, since iOS puts small forms in sheets rather than dialogs.

`message` is the line under the title, 8 apart, the dialog's supporting text; the content after it sits a section-step lower. It's never typed loose as the dialog's first child, which put it as far from the title as from the content.

**In the code:** Android: `AndroidDialog` (`ui/components/AndroidDialog.kt`), Material's `AlertDialog` with `DialogHead` setting `message` under the title. No dialog calls Material's `AlertDialog` directly.
