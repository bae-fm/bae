# MacSheetFooter

Secondary buttons, then the one primary button, at the right: a sheet's last row, and the button under a list that adds to it (“Add library…”), as System Settings puts “Add Account…”. `split` puts the first button on the left, for an action beside the flow: "New folder…", "Use my key instead".

Only one button in the row has colour: the primary's fill. A refusal beside it (Decline beside Approve) is a plain secondary button, not red: red text beside the accent fill is never right. A button is red only when it deletes or wipes something that can't come back (Delete, Reset this device). Removing a library from this Mac isn't (it stays in the cloud and on other devices): its Remove is the primary button. Ending an invite or declining a request isn't that.

**In the code:** macOS: `MacSheetFooter` (`Views/Components/MacSheetFooter.swift`).
