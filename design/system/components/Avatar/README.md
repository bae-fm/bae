# Avatar

A person's picture, round: the image they picked, or the initials of their name on a tint while they have none. bae stores the picture the way it stores a cover (spec/bae.md §5.4), and other devices download it when it first shows.

The picture leads the person's rows in People and devices: the You section's Picture row, where they choose or remove it, and each person's row in the People list. A row draws it through its `avatar` (the person's name) and `avatarImage` (their picture): `MacRow` at 28 points, `AndroidRow` at 40.

**In the code:** none yet; the People and devices page needs a picture view taking the person's name and their avatar's bytes, which the bridge's `avatar(person)` reads and `set_avatar` writes.
