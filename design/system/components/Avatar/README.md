# Avatar

A person's picture, round: the image they picked, or the initials of their name on a tint while they have none. bae stores the picture the way it stores a cover (spec/bae.md §5.4), and other devices download it when it first shows.

The picture leads the person's rows in People and devices: the You section's Picture row, where they choose or remove it, and each person's row in the People list. A row draws it through its `avatar` (the person's name) and `avatarImage` (their picture): `MacRow` at 28 points, `AndroidRow` at 40.

**In the code:** no app view draws a person's picture. BaeKit's ImageStore loads one through the bridge's `avatar(person)` (`ImageKey.avatar`, BaeKit/Sources/BaeKit/Services/ImageStore.swift:49), and no app calls `set_avatar`.
