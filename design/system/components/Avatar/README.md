# Avatar

A person's picture, round: the image they picked, or the initials of their name on a tint while they have none. bae stores the picture the way it stores a cover (spec/bae.md §5.4), and other devices download it when it first shows.

The picture leads the person's rows in People and devices: the You section's Picture row, where they choose or remove it, and each person's row in the People list. A row draws it through its `avatar` (the person's name) and `avatarImage` (their picture): `MacRow` at 28 points, `AndroidRow` at 40.

**In the code:** macOS: `Avatar` (`bae-macos/bae/bae/Views/Components/Avatar.swift`) draws the picture, loaded through the bridge's `avatar(person)`, or the initials the bridge sends. The playlist header (`MacPlaylistHeader`) uses it for the playlist's owner, and People and devices uses it in `PictureRow` and in each person's row; `PictureRow` calls `setAvatar` with the file the person chose. Android: `Avatar` (`ui/components/Avatar.kt`) draws the picture, loaded through the bridge's `avatar(person)`, or the initials the bridge sends. `AndroidRow` leads with it through `avatar`, and People and devices calls `setAvatar` from `PictureRows` with the image the system's photo picker returns.
