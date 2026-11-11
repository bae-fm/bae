# AndroidDialogActions

Cancel (or Dismiss) first, then the action, at the right: a dialog's last row.

One button in the row has colour, never two: the action in the accent, or, when the action deletes or wipes something that can't come back (Delete an album, Reset this device), that one in red. Red is for loss: ending an invite, declining a request or cancelling a wait is not, and stays in the accent or plain. Removing a library from this device isn't loss (it stays in the cloud and on other devices): its Remove is in the accent. Every other button is `neutral`, in the text colour: Cancel, Back, Not yet, Cancel invite beside Done, Decline beside Approve. Two pink buttons, or pink beside red, is never right.

**In the code:** Android: `BaeAlertDialog`'s `confirmButton` and `dismissButton` slots, holding `DestructiveTextButton`, `NeutralTextButton` or `TextButton`.
