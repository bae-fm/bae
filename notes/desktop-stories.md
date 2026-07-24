# Desktop stories

User stories describing the desktop app's behavior, one screen or action at a
time. They serve as the parity contract between the macOS and Windows apps and
as verification scripts: each story is checkable against a running app.

## 1. First run, empty machine

On first launch the welcome screen finds no library on this device and nothing
restorable. It shows the bae wordmark and the subtitle "Get started with your
music library." Three stacked actions appear: create new library (prominent,
the default action), join a library, restore from cloud.

## 2. Creating the first library

The user clicks create new library. The button swaps to a spinner and all
three actions disable. No input is asked for; the library is created with a
generated name. On success the welcome window closes and the library window
opens on the new empty library. On failure an error line appears in red and
the actions re-enable.

## 3. The empty library

Across the top: a Library/Import switcher, a search field, and a settings
gear. Below, a large bold "Albums" heading — itself a dropdown switching to
Composers or Artists — with sort controls opposite. The content area says "No
albums" and "Import some music to get started" — text only, no icon. An idle
now-playing bar rests along the bottom. Native window controls sit where each
platform puts them.
