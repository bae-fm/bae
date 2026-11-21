import type * as React from 'react';

// ---- macOS ----

/** The Settings window: the pane list on the left, the pane on the right. title heads the pane; back names the pane a Back button returns to. status puts a short status beside statusPane in the pane list, in statusTone. A MacSheet with overlay inside covers the whole window. The window is 600 pt tall; a pane taller than that runs under its bottom edge, and scroll (px) draws it scrolled that far, and the heading stays in place. */
export interface MacSettingsWindowProps { pane: 'Appearance' | 'Library' | 'Playback' | 'Transfers' | 'Storage' | 'Import' | 'Save as' | 'Subsonic' | 'Automation' | 'About'; title: string; back?: string; statusPane?: string; status?: string; statusTone?: 'warning' | 'danger' | 'success'; width?: number; height?: number; front?: boolean; children?: React.ReactNode }
export declare function MacSettingsWindow(props: MacSettingsWindowProps): React.ReactElement;

/** A plain macOS window with traffic lights and no sidebar (the welcome window). A MacAlert inside covers it. */
export interface MacWelcomeWindowProps { width?: number; height?: number; children?: React.ReactNode }
export declare function MacWelcomeWindow(props: MacWelcomeWindowProps): React.ReactElement;

/** A window that walks through one step, centred: a headline, an optional line under it, then the step's content and its MacStepActions. */
export interface MacStepProps { title: string; text?: string; children?: React.ReactNode }
export declare function MacStep(props: MacStepProps): React.ReactElement;
/** A step's buttons, side by side and centred, the default last. */
export declare function MacStepActions(props: { children?: React.ReactNode }): React.ReactElement;

/** A macOS alert: title, message, and its buttons stacked full width as children. */
export interface MacAlertProps { title: string; warning?: string; message?: string; details?: string; suppress?: string; suppressOn?: boolean; children?: React.ReactNode }
export declare function MacAlert(props: MacAlertProps): React.ReactElement;

/** A segmented control; options is comma-separated, selected names the chosen one. */
export interface MacSegmentedControlProps { options: string; selected?: string; off?: boolean }
export declare function MacSegmentedControl(props: MacSegmentedControlProps): React.ReactElement;

/** A heading over what follows it, usually one MacBox. */
export interface MacSectionProps { title?: string; children?: React.ReactNode }
export declare function MacSection(props: MacSectionProps): React.ReactElement;

/** A rounded group of rows; draws a hairline between its children. empty is the sentence shown when it has none. */
export interface MacBoxProps { empty?: string; children?: React.ReactNode }
export declare function MacBox(props: MacBoxProps): React.ReactElement;

/** One line of a MacBox: label on the left, then value (grey, or in tone with a dot), controls passed as children, and a chevron when it opens something. action draws the label as an accent action ("Invite…"); destructive colors it danger, for an action that removes something. */
export interface MacRowProps { label?: string; info?: boolean; destructive?: boolean; strong?: boolean; tag?: string; note?: string; /** Text after the note, as MacCopyText: selectable, with a copy button on hover. A list of more than one ends the note in a colon and puts each in a bullet under it, with `after` words that aren't copied (“(Wi-Fi)”). */ copy?: string | { text: string; after?: string }[]; copyHover?: boolean; noteTone?: 'danger' | 'warning' | 'success'; value?: string; tone?: 'success' | 'warning' | 'danger' | 'accent'; chevron?: boolean; selected?: boolean; action?: boolean; nested?: boolean; off?: boolean; onClick?: () => void; /** A row the person clicks that neither `chevron`, `action` nor `destructive` marks: a Welcome library. It reads as a button named by its words. */ button?: boolean; children?: React.ReactNode }
/** What belongs to the row above it and is more than one setting (a field with its explanation): joined to that row with no line, text in secondary. */
export declare function MacNested(props: { children?: React.ReactNode }): React.ReactElement;
/** An ⓘ after a row's name that opens a popover saying what the name can't. */
export declare function MacInfoTip(props: { label?: string }): React.ReactElement;
/** How Save as… names each track's file, one pattern for every format: chips, pieces to add, a preview. */
export declare function MacFileNameField(props: { label: string; /** The pattern's text, each value in brackets: "[Artist] - [Album]". */ pattern?: string; add?: string; preview?: string; /** Why the typed text makes no name; shown under the field, in place of the preview. */ error?: string }): React.ReactElement;
export declare function MacRow(props: MacRowProps): React.ReactElement;
/** A port number: a field five digits wide, digits only, its label hidden. What's wrong with it goes on the row's second line. */
/** A PIN as one box per digit (digits, from core); value is what the person has typed so far. The field shows error under the boxes when state is error. */
export interface MacPinFieldProps { digits?: number; value?: string; label?: string; state?: 'normal' | 'focused' | 'error' | 'off'; error?: string }
export declare function MacPinField(props: MacPinFieldProps): React.ReactElement;
export declare function MacPortField(props: { value?: string; label?: string; state?: 'normal' | 'focused' | 'error' | 'off' }): React.ReactElement;
/** Text someone may paste elsewhere (an address): selectable, with a copy button after it while the pointer is over it. `hover` draws that state. */
export declare function MacCopyText(props: { text?: string; after?: string; label?: string; hover?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function MacItem(props: { lead?: React.ReactNode; trail?: React.ReactNode; centerLead?: boolean; centerTrail?: boolean; wide?: boolean; children?: React.ReactNode }): React.ReactElement;

/** A MacBox row holding a field: the label in a column labelWidth px wide (90), the field filling the rest, then any controls. Leave out label for a field with its label inside. alignText moves the field left by its inner padding so its text lines up with the rows around it. */
export interface MacFieldRowProps { label?: string; labelWidth?: number; alignText?: boolean; children?: React.ReactNode }
export declare function MacFieldRow(props: MacFieldRowProps): React.ReactElement;

/** size alert is the button of a MacAlert; tinted is a secondary button on a filled notice. kind: primary is the one action a screen is for; secondary is everything else; borderless is words with no chrome, for an action at the end of a row; icon is a glyph-only button (label is its accessible name); beside words, label names a button whose words do not say what it does. destructive colors the words danger. */
export interface MacButtonProps { kind?: 'primary' | 'secondary' | 'borderless' | 'icon'; icon?: 'copy' | 'check'; size?: 'small' | 'regular' | 'large' | 'alert'; label?: string; destructive?: boolean; muted?: boolean; neutral?: boolean; tinted?: boolean; off?: boolean; /** It opens a menu, as Add library… does: read as a pop-up button. */ menu?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function MacButton(props: MacButtonProps): React.ReactElement;

/** A text field with its label on the left (labelWidth px), or the label as a placeholder (labelHidden). note is a grey line under it; error a red one, shown when state is error. Leave out label when a MacFieldRow holds the field and gives the label. Children sit after the field. */
export interface MacTextFieldProps { label?: string; value?: string; placeholder?: string; code?: boolean; state?: 'normal' | 'focused' | 'off' | 'error'; secret?: boolean; note?: string; error?: string; labelHidden?: boolean; labelWidth?: number | 'auto'; children?: React.ReactNode }
export declare function MacTextField(props: MacTextFieldProps): React.ReactElement;
/** A labelled control in a sheet's form: label above the control (the child), note under it, all on one left edge. */
export declare function MacFormField(props: { label: string; note?: string; children?: React.ReactNode }): React.ReactElement;

/** One choice of several, as a row of a MacBox. */
export interface MacRadioProps { label: string; note?: string; checked?: boolean; onClick?: () => void }
export declare function MacRadio(props: MacRadioProps): React.ReactElement;

/** A pop-up menu showing its current choice. */
export interface MacPopUpButtonProps { /** Names the pop-up by what it chooses (Import to, Album). */ label?: string; value?: string; width?: number; off?: boolean; children?: React.ReactNode; /** Its menu drawn open, as macOS opens it: the chosen item over the control. */ open?: boolean; /** The choices, comma-separated ("-" for a line). */ items?: string; chosen?: string; highlighted?: string; /** Keeps the menu inside a board's window: its right edge at the control's. */ menuInside?: boolean; /** The menu's top against the control's, when macOS would move it to stay on screen. */ menuTop?: number }
export declare function MacPopUpButton(props: MacPopUpButtonProps): React.ReactElement;

/** An on/off switch; label is its accessible name. */
export interface MacSwitchProps { on?: boolean; label: string; off?: boolean }
export declare function MacSwitch(props: MacSwitchProps): React.ReactElement;

/** The eight accent swatches in the order the appearance settings offer them; selected names the chosen one. */
export interface MacAccentPickerProps { selected?: 'blue' | 'orchid' | 'pink' | 'lime' | 'mint' | 'sky'; label?: string }
export declare function MacAccentPicker(props: MacAccentPickerProps): React.ReactElement;

/** A menu as it shows when open: MacMenuItems in a panel. */
export interface MacMenuProps { /** The menu runs past the screen's bottom edge and ends in the scroll arrow. */ more?: boolean; children?: React.ReactNode }
export declare function MacMenu(props: MacMenuProps): React.ReactElement;
/** The system stepper: an up and a down arrow in a small rounded box, named by what it counts (label). */
export declare function MacStepper(props: { label: string }): React.ReactElement;

/** One choice in a MacMenu: checked marks the current choice, highlighted the one under the pointer. */
export interface MacMenuItemProps { checked?: boolean; highlighted?: boolean; divider?: boolean; children?: React.ReactNode; /** Can't be chosen now: greyed, as NSMenu disables an item. */ off?: boolean; /** Heads the group below. */ heading?: boolean; /** A › opens more items. */ submenu?: boolean; /** On a submenu, the glyph before the ›. */ accessory?: 'problem' | 'scanning' | 'network'; /** The accessory's hover text. */ help?: string }
export declare function MacMenuItem(props: MacMenuItemProps): React.ReactElement;

/** A ▸ label that shows its children when opened. open sets how it starts. */
export interface MacDisclosureProps { label?: string; open?: boolean; children?: React.ReactNode }
export declare function MacDisclosure(props: MacDisclosureProps): React.ReactElement;

/** A sheet: title, divider, content, and a MacSheetFooter last. overlay dims what is under it and sits it at the top. */
export interface MacSheetProps { title?: string; message?: string; overlay?: boolean; width?: number; children?: React.ReactNode }
export declare function MacSheet(props: MacSheetProps): React.ReactElement;

/** A sheet's buttons, on the right. split puts the first child on the left. */
export interface MacSheetFooterProps { split?: boolean | number; children?: React.ReactNode }
export declare function MacSheetFooter(props: MacSheetFooterProps): React.ReactElement;

/** A step that waits: its words over a bar. Without value the bar moves; with value (0 to 1) it fills. */
export interface MacProgressProps { label?: string; name?: string; value?: number | string }
export declare function MacProgress(props: MacProgressProps): React.ReactElement;
export interface MacSpinnerProps { label?: string; size?: number }
export declare function MacSpinner(props: MacSpinnerProps): React.ReactElement;

/** A grey line under a box or control. */
export interface MacNoteProps { children?: React.ReactNode }
export declare function MacNote(props: MacNoteProps): React.ReactElement;

// ---- Android ----

/** A screen: the top bar with Back and the title, then the content. A screen without a title has no top bar. The screen draws an AndroidDialog or AndroidSheet among its children last, over the whole screen. */
export interface AndroidScreenProps { title?: string; back?: boolean; keyboard?: boolean; width?: number; height?: number; children?: React.ReactNode }
export declare function AndroidScreen(props: AndroidScreenProps): React.ReactElement;

/** A label in capitals over the card that follows. */
export interface AndroidSectionProps { title?: string; children?: React.ReactNode }
export declare function AndroidSection(props: AndroidSectionProps): React.ReactElement;

/** A rounded group of rows; draws a hairline between its children. */
export interface AndroidCardProps { children?: React.ReactNode }
export declare function AndroidCard(props: AndroidCardProps): React.ReactElement;

/** A screen that walks through one step, centred: a headline, an optional line under it, then the step's content and buttons. */
/** The six digits two devices both show while one lets the other in, centred; `waiting` is what the device waits for, beside a spinner. */
export interface MatchCodeProps { code: string; waiting?: string; dialog?: boolean }
export declare function MacMatchCode(props: MatchCodeProps): React.ReactElement;
/** `dialog` is the smaller size for inside a dialog. */
export declare function AndroidMatchCode(props: MatchCodeProps): React.ReactElement;

export interface AndroidStepProps { title: string; text?: string; top?: boolean; children?: React.ReactNode }
export declare function AndroidStep(props: AndroidStepProps): React.ReactElement;

/** One line of a card: title (with a tag chip after it), a subtitle under it (in tone with a dot), controls as children, and an icon on the right. action draws the title as an accent action; destructive colors it danger, for an action that removes something. place and joinsNext are AndroidCard's: it passes each row its place among its rows (first, middle, last, alone), which sets the row's tile and corners, and whether the next row is nested under it. */
export interface AndroidRowProps { title: string; tag?: string; subtitle?: string; value?: string; tone?: 'success' | 'warning' | 'danger' | 'accent'; icon?: 'chevron' | 'edit' | 'expand' | 'more' | 'refresh'; leadingIcon?: 'add'; action?: boolean; destructive?: boolean; selected?: boolean; nested?: boolean; place?: 'first' | 'middle' | 'last' | 'alone'; joinsNext?: boolean; onClick?: () => void; /** A row the person taps that none of `icon`, `action`, `destructive` marks: a Settings page's entry, a row that opens a dialog. It reads as a button named by its words. */ button?: boolean; /** On a row that opens and closes rows under it: whether they show. */ expanded?: boolean; /** Dims a row that can't be used for now, such as the other libraries while one is being removed; a tappable one reads as a disabled button. */ off?: boolean; children?: React.ReactNode }
export declare function AndroidRow(props: AndroidRowProps): React.ReactElement;

/** kind: filled for the action a screen is for, outlined beside it, text in dialogs and rows. large is the full-width size on the welcome screen. */
export interface MacImportImagesProps { /** "artwork title|file name", comma-separated. */ images?: string }
export declare function MacImportImages(props: MacImportImagesProps): React.ReactElement;
export interface MacReleaseFilesProps { /** "name|size[|kind]", comma-separated; kind is `other` (opens nothing), `audio` (plays), `ignored:audio` (a CUE sheet set to Ignored, drawn as its caption) or `damaged:why`. */ files?: string }
export declare function MacReleaseFiles(props: MacReleaseFilesProps): React.ReactElement;
export interface AndroidKeyboardProps {}
export declare function AndroidKeyboard(props: AndroidKeyboardProps): React.ReactElement;
export interface AndroidButtonProps { kind?: 'filled' | 'tonal' | 'outlined' | 'text'; label?: string; large?: boolean; icon?: string; alignTo?: 'text' | 'field'; destructive?: boolean; neutral?: boolean; off?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function AndroidButton(props: AndroidButtonProps): React.ReactElement;

/** A 48 dp glyph button: a Material Symbol by name (back, close, more, search, …). label is its accessible name. tone colours the glyph (secondary text, the accent, or the warning); box sets the touch box's side when it isn't 48. */
export interface AndroidIconButtonProps { icon: string; label?: string; size?: number; tonal?: boolean; tone?: 'muted' | 'accent' | 'warning' | 'onfill'; box?: 40 | 56; className?: string; /** It opens a menu: read as a menu button. */ menu?: boolean; /** Dimmed and disabled while its action can't run. */ off?: boolean }
export declare function AndroidIconButton(props: AndroidIconButtonProps): React.ReactElement;

/** An on/off switch; label is its accessible name. */
/** An icon level with a line of text, its touch area overhanging the row's padding: a notice's ×, a download's Cancel. */
export declare function AndroidInlineIconButton(props: { icon?: string; label: string; tone?: 'muted' | 'accent' }): React.ReactElement;
export interface AndroidSwitchProps { on?: boolean; label: string }
export declare function AndroidSwitch(props: AndroidSwitchProps): React.ReactElement;

/** The eight accent swatches in the order the appearance settings offer them; selected names the chosen one. */
export interface AndroidAccentPickerProps { selected?: 'blue' | 'orchid' | 'pink' | 'lime' | 'mint' | 'sky'; label?: string }
export declare function AndroidAccentPicker(props: AndroidAccentPickerProps): React.ReactElement;

/** An outlined text field with its label on the border. helper is a grey line under it; error a red one, shown when state is error. */
/** A PIN as one box per digit (digits, from core); value is what the person has typed so far. The field shows error under the boxes when state is error. */
export interface AndroidPinFieldProps { digits?: number; value?: string; label?: string; state?: 'normal' | 'focused' | 'error'; error?: string }
export declare function AndroidPinField(props: AndroidPinFieldProps): React.ReactElement;
export interface AndroidTextFieldProps { label: string; value?: string; placeholder?: string; code?: boolean; secret?: boolean; state?: 'normal' | 'focused' | 'error'; helper?: string; error?: string }
export declare function AndroidTextField(props: AndroidTextFieldProps): React.ReactElement;

/** One choice of several, in a dialog. */
export interface AndroidRadioProps { label: string; checked?: boolean }
export declare function AndroidRadio(props: AndroidRadioProps): React.ReactElement;

/** A dialog over a dimmed screen: title, content, and AndroidDialogActions last. Put it among the children of the screen it opens over (AndroidScreen, AndroidLibraryScreen, AndroidDetailScreen); the screen draws it over everything, bars included. */
export interface AndroidDialogProps { title?: string; message?: string; width?: number; children?: React.ReactNode }
export declare function AndroidDialog(props: AndroidDialogProps): React.ReactElement;

/** A dialog's buttons, on the right. */
export interface AndroidDialogActionsProps { children?: React.ReactNode }
export declare function AndroidDialogActions(props: AndroidDialogActionsProps): React.ReactElement;

/** A grey line under a card. */
export interface AndroidNoteProps { children?: React.ReactNode }
export declare function AndroidNote(props: AndroidNoteProps): React.ReactElement;
/** An empty or unavailable screen's one sentence, centred in what's left of the screen, in secondary text, with no glyph above it. children are its actions, under it. */
export declare function AndroidContentUnavailable(props: { title: string; children?: React.ReactNode }): React.ReactElement;

/** What is happening now, over a bar. value from 0 to 1 when the length is known. */
export interface AndroidProgressProps { label?: string; value?: number }
export declare function AndroidProgress(props: AndroidProgressProps): React.ReactElement;
export interface AndroidSpinnerProps { label?: string; size?: number }
export declare function AndroidSpinner(props: AndroidSpinnerProps): React.ReactElement;

// ---- Both ----

export type StatusTone = 'neutral' | 'accent' | 'info' | 'success' | 'warning' | 'danger';
/** A short label on its tone's tint: a status, a count, a tag or a role. */
export interface StatusChipProps { tone?: StatusTone; children?: React.ReactNode }
export declare function StatusChip(props: StatusChipProps): React.ReactElement;

/** A code to scan, centered: what to do, the QR code, the buttons that hand over the code (children), then when it ends. size is the QR code's width in points. */
export interface MacScanCodeProps { instruction?: string; size?: number; children?: React.ReactNode }
export declare function MacScanCode(props: MacScanCodeProps): React.ReactElement;
export interface AndroidScanCodeProps { instruction?: string; size?: number; children?: React.ReactNode }
export declare function AndroidScanCode(props: AndroidScanCodeProps): React.ReactElement;

/** A message on its tone's tint. */
/** A tinted box in a tone, led by the tone's glyph unless `icon` is false; `set: 'mac'` draws macOS's glyph, top-aligned. */
export interface NoticeProps { tone?: StatusTone; title?: string; subtitle?: string; action?: string; secondary?: string; layout?: 'paragraph' | 'centered'; buttons?: 'inline' | 'below'; dismiss?: boolean; quiet?: boolean; stage?: boolean; icon?: boolean; children?: React.ReactNode }
/** A tone's glyph in the tone's colour, a filled circle with its mark; `set: 'mac'` draws the macOS stand-in. Neutral and accent draw nothing. */
/** Material's disclosure row: its own card, expand_more or expand_less at the end; the AndroidRows it shows join the card. */
export declare function AndroidDisclosure(props: { label: string; summary?: string; open?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function StatusGlyph(props: { tone: StatusTone; set?: 'mac'; muted?: boolean; inline?: boolean; size?: number; className?: string }): React.ReactElement | null;

/** A cover or picture. Boards give each title its own stand-in colors; `missing` is a plain tile. */
export interface ArtworkProps { title?: string; seed?: string; size?: number; radius?: number; round?: boolean; missing?: boolean; shadow?: 'tile' | 'bar'; ring?: boolean; /** The picture's name, for one that is itself what a screen shows (a gallery's image). Without it the picture is decorative, as beside its title. */ label?: string; children?: React.ReactNode }
export declare function Artwork(props: ArtworkProps): React.ReactElement;
export interface NowPlayingProps { trackTitle?: string; trackArtist?: string; trackAlbum?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; repeat?: boolean; casting?: string }
export interface MacMainWindowProps extends NowPlayingProps { section?: 'Library' | 'Import'; search?: string; searchFocused?: boolean; sync?: 'moving' | 'attention' | 'paused'; shuffle?: boolean; queueOpen?: boolean; width?: number; height?: number; children?: React.ReactNode }
export declare function MacMainWindow(props: MacMainWindowProps): React.ReactElement;
export interface MacNowPlayingBarProps { title?: string; artist?: string; album?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; shuffle?: boolean; repeat?: boolean; casting?: string; queueOpen?: boolean; volume?: number; noCover?: boolean }
export declare function MacNowPlayingBar(props: MacNowPlayingBarProps): React.ReactElement;
export interface SeekBarProps { position?: number; held?: number | string; elapsed?: string; duration?: string; off?: boolean; large?: boolean }
export declare function MacSeekBar(props: SeekBarProps): React.ReactElement;
export declare function AndroidSeekBar(props: SeekBarProps): React.ReactElement;
export interface MacPlayPauseProps { playing?: boolean; waiting?: boolean }
export declare function MacPlayPause(props: MacPlayPauseProps): React.ReactElement;
export interface MacIconButtonProps { icon: string; label?: string; active?: boolean; faint?: boolean; small?: boolean; boxed?: boolean; /** Round on a dark fill, for a place on a cover. */ overlay?: boolean; size?: number; /** It opens a menu: read as a menu button, `active` while the menu is open. */ menu?: boolean; /** Dimmed and disabled while it can't be used. */ off?: boolean }
export declare function MacIconButton(props: MacIconButtonProps): React.ReactElement;
export interface MacLibraryHeadingProps { view?: 'Albums' | 'Artists' | 'Composers'; collapsed?: boolean; children?: React.ReactNode }
export declare function MacLibraryHeading(props: MacLibraryHeadingProps): React.ReactElement;
export interface MacAlbumGridProps { columns?: number; children?: React.ReactNode }
export declare function MacAlbumGrid(props: MacAlbumGridProps): React.ReactElement;
export interface MacAlbumTileProps { /** Only another device holds the album's files: its cover and words show dimmed, and the tile still opens it. */ unplayable?: boolean; title: string; artist?: string; year?: string; noCover?: boolean; hover?: boolean; open?: boolean; selected?: boolean }
export declare function MacAlbumTile(props: MacAlbumTileProps): React.ReactElement;
export interface MacTrackRowProps { position?: string; title: string; artist?: string; duration?: string; current?: boolean; playing?: boolean; hover?: boolean; /** Among the tracks picked together. */ picked?: boolean; /** The stars you gave the track, 1 to 5; the row keeps their slot after it, empty without them. */ rating?: number | string; /** The library's system for tracks: `heart` puts a heart in the slot at three stars or more, `none` draws no slot. */ ratingSystem?: 'stars' | 'heart' | 'none'; }
export declare function MacTrackRow(props: MacTrackRowProps): React.ReactElement;
export declare function MacEyebrow(props: { children?: React.ReactNode }): React.ReactElement;
export interface MacDetailHeaderProps { name: string; count?: string; noImage?: boolean }
export declare function MacDetailHeader(props: MacDetailHeaderProps): React.ReactElement;
/** A playlist's page inside the side pane's Playlists tab (the pane's back button returns to the list), its header laid out as the album page's: the picture beside the name (two lines at most), the owner, the facts line (count, length), then Play and the ⋯ under the text; the ⋯ menu offers Shuffle, then the owner's options. sharing: on the person's own playlist, who sees it, as a lock or people glyph; none on a playlist someone else shared. Its tracks (MacListRow) are its children. */
export interface MacPlaylistPageProps { name: string; covers?: string; owner: string; count: string; length: string; sharing?: 'Only me' | 'Everyone'; options?: string; optionsOpen?: boolean; highlighted?: string; children?: React.ReactNode }
export declare function MacPlaylistPage(props: MacPlaylistPageProps): React.ReactElement;
/** The round back button before a page's heading, named Back to and where it goes (to). */
export declare function MacBackButton(props: { to: string }): React.ReactElement;
export interface MacQueueRowProps { title: string; artist?: string; album?: string; duration?: string; hover?: boolean }
export declare function MacQueueRow(props: MacQueueRowProps): React.ReactElement;
export interface AndroidLibraryScreenProps extends NowPlayingProps { view?: 'Albums' | 'Artists' | 'Composers'; collapsed?: boolean; searching?: boolean; search?: string; sync?: 'moving' | 'attention' | 'paused'; width?: number; height?: number; children?: React.ReactNode }
export declare function AndroidLibraryScreen(props: AndroidLibraryScreenProps): React.ReactElement;
export interface AndroidDetailScreenProps extends NowPlayingProps { back?: string; menu?: boolean; menuLabel?: string; flush?: boolean; width?: number; height?: number; children?: React.ReactNode }
export declare function AndroidDetailScreen(props: AndroidDetailScreenProps): React.ReactElement;
export interface AndroidNowPlayingBarProps { title?: string; artist?: string; album?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; repeat?: boolean; casting?: string; castOff?: boolean }
export declare function AndroidNowPlayingBar(props: AndroidNowPlayingBarProps): React.ReactElement;
export interface AndroidPlayPauseProps { playing?: boolean; waiting?: boolean; size?: number }
export declare function AndroidPlayPause(props: AndroidPlayPauseProps): React.ReactElement;
export declare function AndroidAlbumGrid(props: { children?: React.ReactNode }): React.ReactElement;
export interface AndroidAlbumTileProps { /** Only another device holds the album's files: its cover and words show dimmed, and the tile still opens it. */ unplayable?: boolean; title: string; line?: string; noCover?: boolean }
export declare function AndroidAlbumTile(props: AndroidAlbumTileProps): React.ReactElement;
export interface AndroidArtistGroupHeadingProps { name: string; count?: string; pinned?: boolean; noImage?: boolean }
export declare function AndroidArtistGroupHeading(props: AndroidArtistGroupHeadingProps): React.ReactElement;
export interface AndroidTrackRowProps { position?: string; title: string; artist?: string; duration?: string; current?: boolean; playing?: boolean; /** The work the track records and its composer, on a line under the row. */ work?: string }
export declare function AndroidTrackRow(props: AndroidTrackRowProps): React.ReactElement;
export declare function AndroidEyebrow(props: { children?: React.ReactNode }): React.ReactElement;
export interface AndroidAlbumHeaderProps { /** Only another device holds the files: Not on this device in place of Play, with an info button that opens a sheet naming the device. */ unplayable?: string; title: string; artist?: string; year?: string; rating?: number | string; edition?: string; copy?: string; noCover?: boolean; children?: React.ReactNode; /** The library's rating system for albums: `heart` shows one heart at three stars or more in place of the rating, `none` shows no rating. */ ratingSystem?: 'stars' | 'heart' | 'none'; }
export declare function AndroidAlbumHeader(props: AndroidAlbumHeaderProps): React.ReactElement;
export interface AndroidPinControlProps { state?: 'available' | 'queued' | 'downloading' | 'failed' | 'downloaded' | 'elsewhere'; progress?: number; bytes?: string; where?: string }
export declare function AndroidPinControl(props: AndroidPinControlProps): React.ReactElement;
export declare function AndroidUploadLine(props: { phase?: string; missing?: string }): React.ReactElement;
export interface MacAlbumPanelProps { title: string; artist?: string; year?: string; releases?: string; release?: string; edition?: string; copy?: string; /** The release's audio as bae read it, the panel's last line, after the tracks, credits and works. */ audio?: string; unplayable?: string; unplayableHover?: boolean; noCover?: boolean; options?: string; optionsOpen?: boolean; highlighted?: string; /** The album's stars, 0 while no one has rated it; leave it out to draw no rating. */ rating?: number | string; children?: React.ReactNode; /** The library's rating system for albums: `heart` shows one heart at three stars or more in place of the rating, `none` shows no rating. */ ratingSystem?: 'stars' | 'heart' | 'none'; }
/** A line of progress: the phase, a bar filling the rest of the line, an optional count. No `value` is a wait of no known length. */
export declare function MacProgressLine(props: { label: string; value?: number | string; detail?: string }): React.ReactElement;
/** Every catalog that describes a release, as a wrapped row of links (`records`: catalog names, comma-separated). */
export declare function MacRecordLinks(props: { records: string; /** Each catalog is a button, as on a found album's card. */ buttons?: boolean }): React.ReactElement;
/** A row's ⋯ / ⋮ button with the row's actions in its menu; `open` draws the menu, `items` its entries ("-" for a line). */
export interface MoreMenuProps { open?: boolean; items?: string; highlighted?: string; off?: boolean; label?: string; /** macOS: draws the button on a control's fill, beside a page's Play. */ boxed?: boolean; /** macOS: round on a dark fill, for a place on a cover. */ overlay?: boolean }
export declare function MacMoreMenu(props: MoreMenuProps): React.ReactElement;
export declare function AndroidMoreMenu(props: MoreMenuProps): React.ReactElement;
/** The close control of a docked pane and of an open album. */
export declare function MacCloseButton(props: { label?: string }): React.ReactElement;
export declare function MacAlbumPanel(props: MacAlbumPanelProps): React.ReactElement;
export declare function MacTrackColumns(props: { children?: React.ReactNode }): React.ReactElement;
export declare function MacBrowseSplit(props: { children?: React.ReactNode }): React.ReactElement;
export interface MacArtistAlbumTileProps { /** Only another device holds the album's files: its cover and words show dimmed, and the tile still opens it. */ unplayable?: boolean; title: string; year?: string; noCover?: boolean }
export declare function MacArtistAlbumTile(props: MacArtistAlbumTileProps): React.ReactElement;
export interface MacSortCriteriaProps { criteria?: string; fields?: string }
export declare function MacSortCriteria(props: MacSortCriteriaProps): React.ReactElement;
export interface MacCheckboxProps { label: string; on?: boolean }
export declare function MacCheckbox(props: MacCheckboxProps): React.ReactElement;
export declare function MacArtistGroupHeading(props: { name: string }): React.ReactElement;
export interface AndroidMenuProps { top?: number; left?: number; right?: number; width?: number; children?: React.ReactNode }
export declare function AndroidMenu(props: AndroidMenuProps): React.ReactElement;
export interface AndroidMenuItemProps { selected?: boolean; check?: boolean; heading?: boolean; divider?: boolean; pressed?: boolean; children?: React.ReactNode }
export declare function AndroidMenuItem(props: AndroidMenuItemProps): React.ReactElement;
export interface MacSearchCardProps { top?: number; right?: number; empty?: boolean; children?: React.ReactNode }
export declare function MacSearchCard(props: MacSearchCardProps): React.ReactElement;
export declare function MacSearchSection(props: { title: string }): React.ReactElement;
/** part (Android) indents a work under its group's heading work; inset (Android) is the composer and work pages' rows today, whose text carries a second 16dp padding inside the row's own. */
/** One row of a library list on the Mac: Search's results, the artist and composer lists, the composer and work pages' rows. */
export interface MacLibraryRowProps { title?: string; subtitle?: string; trailing?: string; art?: string; missing?: boolean; hover?: boolean; selected?: boolean; loading?: boolean; credit?: boolean; bleed?: boolean; part?: boolean; top?: number }
export declare function MacLibraryRow(props: MacLibraryRowProps): React.ReactElement;
export declare function AndroidSearchSection(props: { title: string }): React.ReactElement;
/** One row of a library list: Search's results, the artist and composer lists, the composer and work pages' rows. */
export interface AndroidLibraryRowProps { title: string; subtitle?: string; trailing?: string; art?: string; noArt?: boolean; missing?: boolean; part?: boolean; inset?: boolean }
export declare function AndroidLibraryRow(props: AndroidLibraryRowProps): React.ReactElement;
export interface MacQueuePanelProps { tab?: 'Queue' | 'Playlists' | 'History'; back?: string; hang?: number; title?: string; artist?: string; album?: string; playing?: boolean; position?: number; elapsed?: string; empty?: boolean; whose?: string; children?: React.ReactNode }
export declare function MacQueuePanel(props: MacQueuePanelProps): React.ReactElement;
export interface MacQueueSectionProps { title: string; clear?: boolean; shuffle?: boolean; menu?: string; wide?: boolean; hang?: boolean }
export declare function MacQueueSection(props: MacQueueSectionProps): React.ReactElement;
export declare function MacQueuePlaceholderRow(props: {}): React.ReactElement;
export declare function AndroidQueuePlaceholderRow(): React.ReactElement;
export declare function MacQueueInsertLine(props: {}): React.ReactElement;
export interface SidePauseProps { kind?: 'side' | 'disc'; label?: string; countdown?: number; unchecked?: boolean }
export declare function MacSidePause(props: SidePauseProps): React.ReactElement;
export interface MacCastPopoverProps { devices?: string; casting?: string; connecting?: string; ownerDeciding?: boolean; right?: number; bottom?: number }
export declare function MacCastPopover(props: MacCastPopoverProps): React.ReactElement;
export interface MacCastRowProps { name: string; kind?: 'cast' | 'airplay' | 'upnp'; active?: boolean; menu?: boolean; /** The words of a wait on this receiver, such as Connecting…: a spinner at the row's end and the words in place of its kind. */ waiting?: string }
export declare function MacCastRow(props: MacCastRowProps): React.ReactElement;
export declare function AndroidSheet(props: { top?: number; title?: string; /** One sentence of secondary text under the title. */ note?: string; children?: React.ReactNode }): React.ReactElement;
export interface AndroidQueueSectionProps { title: string; clear?: boolean; shuffle?: boolean }
export declare function AndroidQueueSection(props: AndroidQueueSectionProps): React.ReactElement;
export interface AndroidQueueRowProps { title: string; artist?: string; album?: string; duration?: string; current?: boolean; dragging?: boolean }
export declare function AndroidQueueRow(props: AndroidQueueRowProps): React.ReactElement;
export interface AndroidFullPlayerProps { title: string; artist?: string; album?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; repeat?: boolean; noCover?: boolean; noVolume?: string; volume?: number | string }
export declare function AndroidFullPlayer(props: AndroidFullPlayerProps): React.ReactElement;
export interface AndroidCastRowProps { name?: string; kind?: 'cast' | 'airplay' | 'upnp'; active?: boolean; menu?: boolean; menuOpen?: boolean; /** The words of a wait. On a receiver (`name`): a spinner at the row's end and the words in place of its kind, such as Connecting…. With no receiver: the list's own wait, such as Looking for devices…. */ waiting?: string }
export declare function AndroidCastRow(props: AndroidCastRowProps): React.ReactElement;
export interface AndroidCheckboxProps { label: string; on?: boolean }
export declare function AndroidCheckbox(props: AndroidCheckboxProps): React.ReactElement;
/** A row of a composer's page or a work's: cover, title, second line. credit is the text-only row; part indents a work; top is the gap above it. */
export interface MacSectionHeaderProps { title: string; top?: number; bottom?: number }
export declare function MacSectionHeader(props: MacSectionHeaderProps): React.ReactElement;
export declare function MacWorkDetail(props: { title: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacHairline(props: {}): React.ReactElement;
export declare function MacContentUnavailable(props: { title: string; description?: string; children?: React.ReactNode }): React.ReactElement;
export declare function Sym(props: { name: string; size?: number; weight?: number }): React.ReactElement;
export declare function MacStorageWindow(props: { filter?: 'All' | 'Local' | 'Cloud'; search?: string; inspector?: boolean; disabled?: boolean; empty?: boolean; width?: number; height?: number; children?: React.ReactNode }): React.ReactElement;
export declare function MacTable(props: { columns: string; rows?: string; selected?: number | string; loading?: number }): React.ReactElement;
export declare function MacStorageCell(props: { value?: string; numeric?: boolean; state?: string; progress?: number | string }): React.ReactElement;
export declare function MacReleaseCell(props: { title: string }): React.ReactElement;
export declare function MacStorageChip(props: { state?: 'local' | 'pinned' | 'cloud' | 'uploading' | 'pinning' | 'detaching' | 'downloadFailed' | 'moveFailed' | 'moveToThisDeviceFailed' | 'queued' | 'uploadQueued' | 'uploadBlocked' | 'moveToThisDeviceQueued'; progress?: number; file?: string; long?: boolean }): React.ReactElement;
export declare function MacFailedTransferRow(props: { kind?: 'pin' | 'moveToThisDevice'; title?: string; detail?: string; ago?: string }): React.ReactElement;
/** What bae keeps on this device, heading Settings → Storage: the total, one bar split by kind, a legend line per kind. parts: "Label:size:share,…" in the kinds' fixed order; each kind keeps its colour. */
export interface StorageUsageProps { total?: string; of?: string; parts?: string }
export declare function MacStorageUsage(props: StorageUsageProps): React.ReactElement;
export declare function AndroidStorageUsage(props: StorageUsageProps): React.ReactElement;
export declare function MacStorageFooter(props: { releases?: string; total?: string; uploads?: string; pins?: string; exports?: string; uploadsWait?: string; pinsWait?: string; exportsWait?: string }): React.ReactElement;
export declare function MacTransferSummary(props: { kind?: 'upload' | 'pin'; counts?: string; bytes?: string; rate?: string; progress?: number; cancel?: boolean }): React.ReactElement;
export declare function MacTransferReadout(props: { kind?: 'upload' | 'pin'; bytes?: string; rate?: string; progress?: number }): React.ReactElement;
export declare function MacFileRow(props: { name: string; size?: string; state?: 'rest' | 'queued' | 'sending' | 'pinning'; progress?: number; selected?: boolean }): React.ReactElement;
export declare function MacFileDetails(props: { name: string; folder?: string; kind?: string; format?: string; size?: string }): React.ReactElement;
export declare function MacNotice(props: NoticeProps): React.ReactElement;
/** An error's causes, one a line, in an inset panel with Copy in its corner (an alert's `details`). */
export declare function MacErrorDetails(props: { details: string }): React.ReactElement;
/** MacErrorDetails on Android: a copy icon button at the corner; in an AndroidDialog. */
export declare function AndroidErrorDetails(props: { details: string }): React.ReactElement;
export declare function MacPopover(props: { width?: number; top?: number; right?: number; children?: React.ReactNode }): React.ReactElement;
export declare function MacActivity(props: { children?: React.ReactNode; attention?: string; error?: string; syncing?: boolean; uploads?: string; uploadProgress?: number; uploadsWait?: string; pins?: string; pinProgress?: number; movingToThisDevice?: string; moveToThisDeviceProgress?: number; downloadsWait?: string }): React.ReactElement;
export declare function AndroidNotice(props: NoticeProps): React.ReactElement;
export declare function AndroidActivity(props: { children?: React.ReactNode; attention?: string; record?: string; reason?: string; error?: string; syncing?: boolean; uploads?: string; uploadProgress?: number; uploadsWait?: string; pins?: string; pinProgress?: number; downloadsWait?: string }): React.ReactElement;
export declare function AndroidSegmentedControl(props: { options: string; selected?: string; /** What the choice is, over the buttons. */ label?: string; /** What the chosen option means, under the buttons. */ helper?: string }): React.ReactElement;
export declare function AndroidPinningBar(props: { failed?: boolean; paused?: boolean; idle?: boolean }): React.ReactElement;
export declare function AndroidPinningRow(props: { title?: string; detail?: string; state?: 'queued' | 'active' | 'failed'; progress?: number; bytes?: string }): React.ReactElement;
export declare function AndroidSnackbar(props: { message: string; action?: string; close?: boolean }): React.ReactElement;
export declare function MacImportSidebar(props: { tab?: 'Found' | 'Imported' | 'Skipped'; found?: string; imported?: string; skipped?: string; filter?: string; narrowed?: string; identifying?: number; importing?: boolean; scanning?: boolean; filtered?: boolean; empty?: string; /** The list at its 410-point floor, as the window's smallest width leaves it. */ narrow?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function MacImportGroupHeader(props: { name: string; closed?: boolean }): React.ReactElement;
export declare function MacImportRow(props: { title?: string; artist?: string; folder?: string; cover?: string; noCover?: boolean; record?: boolean; badge?: string; badgeTone?: 'warning' | 'danger'; state?: 'identifyQueued' | 'identifying' | 'importQueued' | 'importing' | 'uploading'; step?: string; progress?: number; error?: string; grouped?: boolean; selected?: boolean }): React.ReactElement;
export declare function MacImportFolderLine(props: { tab?: string; folder: string }): React.ReactElement;
/** Where an import goes: This Mac or Cloud. One stored choice, wherever it shows. */
export declare function MacImportDestination(props: { to: 'This Mac' | 'Cloud'; off?: boolean }): React.ReactElement;
/** The release editor, in the import pane and in the album's Edit release sheet: Identify and the action row (`actions`: import with where it goes, or save with Cancel and Save); the identity and fields; the records with Unlink; the images; the track table, its children; the Files list. */
export declare function MacReleaseEditor(props: { title?: string; artist?: string; artistChip?: string; year?: string; audio?: string; album?: string; albumInLibrary?: boolean; albumPicked?: boolean; albumOpen?: boolean; cover?: string; pressingYear?: string; media?: string; labels?: string; country?: string; countryOpen?: string; barcodes?: string; status?: string; packaging?: string; details?: string; records?: string; images?: string; files?: string; reading?: string; children?: React.ReactNode } & ({ actions: 'import'; check?: string; cloud?: boolean; to?: 'This Mac' | 'Cloud'; retry?: boolean; blank?: boolean } | { actions: 'save'; changed?: boolean })): React.ReactElement;
/** A skipped folder's card, read-only: why bae can't import it, then its cover and what its tags call the release, or its folder's name. */
export declare function MacInvalidCandidateCard(props: { reason: string; cover?: string; title?: string; artist?: string; folder?: string }): React.ReactElement;
export declare function MacImportTable(props: { children?: React.ReactNode }): React.ReactElement;
export declare function MacImportTrackRow(props: { file?: string; start?: string; position?: string; title?: string; artist?: string; artistChip?: 'Library' | 'New'; inherited?: boolean; length?: string; listed?: string; playing?: boolean }): React.ReactElement;
export declare function MacImportSheetCaption(props: { disc?: string; sheet?: string; bound?: string; unbound?: boolean }): React.ReactElement;
export declare function MacFindOnline(props: { open?: 'Automatic' | 'Search'; discogsBar?: boolean; autoStatus?: 'working' | 'failed'; dimmed?: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacIdentifierBand(props: { chips: string }): React.ReactElement;
export declare function MacReleaseCard(props: { title: string; artist?: string; label?: string; sources?: string }): React.ReactElement;
/** A found album with its pressings on a rule under it. */
export declare function MacReleaseGroup(props: { title: string; artist?: string; label?: string; sources?: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacPressingRow(props: { edition?: string; copy?: string; barcode?: string; badges?: string; /** The notes of the pressing's records: the line under the copy line and the Notes badge, with the same notes shown on the badge's hover. */ notes?: string; sources?: string; inLibrary?: boolean; selected?: boolean }): React.ReactElement;
export declare function MacImportBulk(props: { count?: number; ready?: number; running?: string; cloud?: boolean; to?: 'This Mac' | 'Cloud' }): React.ReactElement;
export declare function MacModal(props: { width?: number; height?: number; children?: React.ReactNode }): React.ReactElement;
export declare function MacModalHeader(props: { title: string; subtitle?: string; large?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function MacCoverGallery(props: { title?: string; selected?: string; source?: string; current?: string; remote?: 'unlinked'; remoteNames?: string; files?: string }): React.ReactElement;
export declare function MacLightbox(props: { title?: string; label?: string; source?: string; images?: string; selected?: string; browse?: boolean; state?: 'loading' | 'failed' }): React.ReactElement;
export declare function MacStorageBand(props: { state?: 'local' | 'pinned' | 'cloud' | 'uploading' | 'waiting'; missing?: string; download?: 'queued' | 'downloading' | 'failed'; moveToThisDevice?: 'queued' | 'moving' | 'failed'; progress?: number; why?: string; actions?: string }): React.ReactElement;
export declare function Stars(props: { value?: number; size?: number; who?: string }): React.ReactElement;
/** A rating under the Heart system, read-only: filled when `on` (three stars or more). */
export declare function Heart(props: { on?: boolean; size?: number }): React.ReactElement;
export declare function Avatar(props: { name: string; image?: string; size?: number }): React.ReactElement;
export declare function MacListRow(props: { /** the cover's size, 32pt unless given */ cover?: number; lead?: string; title: string; sub?: string; album?: string; who?: string; duration?: string; wide?: boolean; hanging?: boolean; leadWidth?: number; hover?: boolean; remove?: string; lifted?: boolean; dragging?: boolean }): React.ReactElement;
export declare function AndroidListRow(props: { title: string; sub?: string; album?: string; who?: string; trailing?: string; /** The copy under the finger while the owner drags the row to a new place: raised over the rows. */ dragging?: boolean; /** Ends the row in ⋮; a text value labels the button. */ menu?: boolean | string }): React.ReactElement;

// ---- Logo and app icons ----

/** The logo in one of its three forms: pair is the sheep over the word, sheep the sheep alone, word the word alone, small the drawing for 32 pixels and under. size is the sheep's width. */
export declare function Logo(props: { kind?: 'pair' | 'inline' | 'sheep' | 'word' | 'small'; size?: number; bob?: boolean; glow?: boolean }): React.ReactElement;
/** An app icon as a platform shows it. macOS, iOS and Android put the sheep on the pale pink tile; Windows and Linux show it with no tile. mask is Android's launcher shape. */
export declare function AppIcon(props: { platform?: 'macos' | 'ios' | 'android' | 'windows' | 'linux'; size?: number; mask?: 'circle' | 'squircle' | 'rounded' }): React.ReactElement;

// ---- The sheep on the empty Import screen ----

/** The sheep on the empty Import screen. Every few seconds it plays one moment from a fixed sequence, then rests: it gobbles a line of releases from the left or the right, catches one falling from above, breathes one in, bops to its music, or rolls on the spot. size is the sheep's width. still holds the pose it shows when the person has asked for reduced motion. */
export declare function ImportSheep(props: { size?: number; still?: boolean }): React.ReactElement;
// ---- Pieces that several screens share ----

/** The welcome window's first choices when this Mac has no library: Create new library…, Scan code and Enter code. */
export declare function MacWelcomeActions(props: {}): React.ReactElement;
/** The welcome screen's first choices when this device has no library: Scan code and Enter code. */
export declare function AndroidWelcomeActions(props: {}): React.ReactElement;

/** The libraries on this Mac, as the welcome window lists them, with Add library… under them. state: '' (both open), removing (bae is leaving the first), open-failed (the first could not open), unreadable or damaged-join (a third row bae cannot read). */
export interface MacWelcomeLibrariesProps { state?: '' | 'removing' | 'open-failed' | 'unreadable' | 'damaged-join' }
export declare function MacWelcomeLibraries(props: MacWelcomeLibrariesProps): React.ReactElement;
/** The libraries on this device, as the welcome screen lists them, with Add library as the card's last row. */
export interface AndroidWelcomeLibrariesProps { state?: '' | 'removing' | 'unreadable' | 'damaged-join' }
export declare function AndroidWelcomeLibraries(props: AndroidWelcomeLibrariesProps): React.ReactElement;

/** The welcome window's content: the logo over "Your music, everywhere", then MacWelcomeLibraries (state) and children under them. empty: no library yet, so a larger logo sits over MacWelcomeActions. */
export interface MacWelcomeHomeProps { empty?: boolean; state?: MacWelcomeLibrariesProps['state']; children?: React.ReactNode }
export declare function MacWelcomeHome(props: MacWelcomeHomeProps): React.ReactElement;
/** The welcome screen's content, as MacWelcomeHome's; empty puts AndroidWelcomeActions at the bottom. */
export interface AndroidWelcomeHomeProps { empty?: boolean; state?: AndroidWelcomeLibrariesProps['state']; children?: React.ReactNode }
export declare function AndroidWelcomeHome(props: AndroidWelcomeHomeProps): React.ReactElement;

/** The library's restore code in Settings, hidden but for its ends until shown, with Show or Hide, Print… and Copy. s3 adds that an S3 library also needs its key. */
export interface RestoreCodeProps { shown?: boolean | 'true'; s3?: boolean | 'true' }
export declare function MacRestoreCode(props: RestoreCodeProps): React.ReactElement;
export declare function AndroidRestoreCode(props: RestoreCodeProps): React.ReactElement;

/** An S3 library's cloud home in Settings: provider, endpoint, region, bucket and the access key ID with Replace key. oldKey: the key just replaced, which a notice asks the person to delete. */
export interface CloudHomeProps { keyId?: string; oldKey?: string }
export declare function MacCloudHome(props: CloudHomeProps & { provider?: 's3' | 'dropbox' | 'onedrive' | 'icloud'; endpoint?: string; region?: string; bucket?: string; account?: string; folder?: string; bare?: boolean }): React.ReactElement;
export declare function AndroidCloudHome(props: CloudHomeProps): React.ReactElement;

/** The two fields of an S3 access key. focus names the focused field (secret by default); refused shows that S3 storage turned the key down. */
export interface S3KeyFieldsProps { keyId?: string; secret?: string; refused?: boolean | 'true'; focus?: 'id' | 'secret' }
export declare function MacS3KeyFields(props: S3KeyFieldsProps): React.ReactElement;
export declare function AndroidS3KeyFields(props: S3KeyFieldsProps): React.ReactElement;

/** The S3 storage form for a new library, with Cancel and Set up. error says what S3 storage refused, where it refused it. */
export interface MacS3FormProps { error?: 'none' | 'key' | 'denied' | 'region' | 'bucket' | 'occupied' }
export declare function MacS3Form(props: MacS3FormProps): React.ReactElement;

/** The folder picker for a Dropbox or OneDrive library, with New folder…, Look again, Cancel and Set up. */
export interface MacFolderPickerProps { provider?: 'dropbox' | 'onedrive'; state?: 'list' | 'naming' | 'made' | 'notEmpty' | 'empty' }
export declare function MacFolderPicker(props: MacFolderPickerProps): React.ReactElement;

/** The invite form's fields: the account email, or on S3 the key the person joins with, then their role and how long the invite lasts. */
export interface MacInviteFieldsProps { storage?: 'dropbox' | 'onedrive' | 'icloud' | 's3'; keyOwner?: 'theirs' | 'mine' }
export declare function MacInviteFields(props: MacInviteFieldsProps): React.ReactElement;
export interface AndroidInviteFieldsProps { storage?: 'dropbox' | 'onedrive' | 's3'; keyOwner?: 'theirs' | 'mine' }
export declare function AndroidInviteFields(props: AndroidInviteFieldsProps): React.ReactElement;

/** How the library syncs: the status in its tone, what caused it and what to do (cause, remedy), a button that does it (fix), and when this device last synced. */
export interface SyncStatusProps { status: string; tone?: '' | 'success' | 'warning' | 'danger'; cause?: string; remedy?: string; fix?: string; lastSynced?: string }
export declare function MacSyncStatus(props: Omit<SyncStatusProps, 'remedy'> & { countdown?: string }): React.ReactElement;
export declare function AndroidSyncStatus(props: SyncStatusProps): React.ReactElement;

/** The Save as panel's Format line: "Format:" and the format pop-up; value, open, items and chosen are MacPopUpButton's. */
export interface MacSavePanelFormatProps { value: string; open?: boolean; items?: string; chosen?: string }
export declare function MacSavePanelFormat(props: MacSavePanelFormatProps): React.ReactElement;
/** A search or filter field: the magnifier, then the text. kind search is the title bar's rounded field, filter a toolbar's 160pt field, inline a field with no box that fills the rest of its row. label names it and shows while it is empty. off dims a field that has nothing to filter. */
export declare function MacSearchField(props: { kind?: 'search' | 'filter' | 'inline'; label?: string; value?: string; focused?: boolean; off?: boolean }): React.ReactElement;
/** A ring that fills clockwise from the top with value, from 0 to 1, in the accent, for a file's transfer. label names it for VoiceOver. */
export declare function MacProgressRing(props: { value: number | string; label?: string }): React.ReactElement;
/** Found, Imported and Skipped, each a tab with its count; tab is the open one. */
export declare function MacImportTabs(props: { tab?: 'Found' | 'Imported' | 'Skipped'; found?: string; imported?: string; skipped?: string }): React.ReactElement;
/** The import list's filter row: the Filter field, the narrowed count, the identifying ring, importing, scanning and the ⋯ menu with its filter dot. */
export declare function MacImportFilterRow(props: { filter?: string; narrowed?: string; identifying?: number | string; importing?: boolean; scanning?: boolean; filtered?: boolean }): React.ReactElement;
/** The release a folder holds, beside its cover in the import pane: title (or the placeholder), artist, facts line, audio line. */
export declare function MacImportReleaseSummary(props: { title?: string; placeholder?: boolean; artist?: string; edition?: string; copy?: string; audio?: string }): React.ReactElement;
/** A way to answer a folder that waits on you: a small button over the line saying what it does. */
export declare function MacAnswerAction(props: { title: string; subtitle: string; off?: boolean }): React.ReactElement;
/** A catalog a typed search still waits on: searching, or failed with Retry. */
export declare function MacSearchSourceLine(props: { catalog?: string; failed?: boolean; reason?: string }): React.ReactElement;
/** The Import section before any folder is added: the sheep, the line, Import folder… and, without a Discogs key, Add Discogs key…. */
export declare function MacImportEmptyState(props: { discogs?: boolean }): React.ReactElement;
/** The Import window's content: the list column (the first child) beside the pane (the other children). noSelection draws the pane's Select a folder state instead. */
export declare function MacImportWindow(props: { noSelection?: boolean; children?: React.ReactNode }): React.ReactElement;
/** A field drawn as its value until the pointer is over it or it has focus. large sets it in the album title's size; quiet draws its value in the tertiary colour. */
export declare function MacInlineField(props: { label: string; value?: string; placeholder?: string; large?: boolean; quiet?: boolean; fit?: boolean; mono?: boolean }): React.ReactElement;
/** A ruled heading over a part of a form: Release, Images, Files, Credits. */
export declare function MacFormHeading(props: { children?: React.ReactNode }): React.ReactElement;
/** The release editor's top row: Identify with Automatic, Search and the Metadata menu, then the action row: where the import goes and Import or Retry import, or Cancel once something changed and Save. */
export declare function MacIdentifyBar(props: { reading?: string } & ({ actions?: 'import'; blank?: boolean; retry?: boolean; cloud?: boolean; to?: 'This Mac' | 'Cloud' } | { actions: 'save'; changed?: boolean })): React.ReactElement;
/** An artist field: a button showing the artists and their Library or New chip, drawn as its value until the pointer is over it. */
export declare function MacAlbumArtistField(props: { artist?: string; chip?: 'Library' | 'New'; placeholder?: string; inherited?: boolean; secondary?: boolean; column?: boolean }): React.ReactElement;
/** The Album pop-up showing the album as it stands; open draws its menu: that album, New album, Use the release's album, Choose another album. */
export declare function MacAlbumPicker(props: { album: string; inLibrary?: boolean; picked?: boolean; open?: boolean }): React.ReactElement;
/** The release's cover beside its album title, album artist and album year, then its children: the audio line and the album picker. */
export declare function MacReleaseIdentity(props: { cover?: string; title?: string; artist?: string; artistChip?: 'Library' | 'New'; year?: string; children?: React.ReactNode }): React.ReactElement;
/** The release's fields under a Release heading: Year, Media, Label with catalog number, Country, Barcode, Status, Packaging and Details. */
export declare function MacReleaseFields(props: { year?: string; media?: string; labels?: string; country?: string; countryOpen?: string; barcodes?: string; status?: string; packaging?: string; details?: string }): React.ReactElement;
/** One file of a release: name and size; kind opens (a button that opens it), audio (with Play) or other; damaged says why the file is damaged. */
export declare function MacReleaseFileRow(props: { name: string; size?: string; kind?: 'opens' | 'audio' | 'other'; damaged?: string }): React.ReactElement;
/** One identifier the files state: label and value, or a dash, with the catalogs searched for it ("MusicBrainz=2,Discogs=0"). */
export declare function MacIdentifierChip(props: { label: string; value?: string; state?: 'filled' | 'outlined' | 'struck' | 'dash'; sources?: string }): React.ReactElement;
/** A pressing fact, each record's value followed by its catalog when the records differ ("1992:MusicBrainz|1993:Discogs"). */
export declare function MacSourcedValue(props: { value: string }): React.ReactElement;
/** One bulk action with how many selected folders it applies to; a child sits before the count. */
export declare function MacBulkActionRow(props: { icon: string; label: string; count?: number; strong?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function MacTransport(props: { playing?: boolean; waiting?: boolean; shuffle?: boolean; repeat?: boolean }): React.ReactElement;
export declare function MacVolumeSlider(props: { volume?: number; /** The receiver that sets the volume, when it takes none from bae. */ noVolume?: string }): React.ReactElement;
export declare function MacSectionTabs(props: { section?: 'Library' | 'Import' }): React.ReactElement;
export declare function MacActivityButton(props: { state?: 'moving' | 'attention' | 'paused' }): React.ReactElement | null;
export declare function MacAvatarButton(props: { name?: string; image?: string }): React.ReactElement;
export interface MacTitleBarProps { section?: 'Library' | 'Import'; search?: string; searchFocused?: boolean; sync?: 'moving' | 'attention' | 'paused'; avatar?: string; avatarImage?: string }
export declare function MacTitleBar(props: MacTitleBarProps): React.ReactElement;
export declare function MacTrackLead(props: { position?: string; hover?: boolean; current?: boolean; playing?: boolean }): React.ReactElement;
export declare function MacReleasePicker(props: { releases?: string; selected?: string }): React.ReactElement | null;
export declare function MacAlbumRating(props: { value?: number | string; who?: string }): React.ReactElement;
/** A help tag: the short words macOS shows under a control the pointer rests on. */
export interface MacHelpTagProps { text?: string; children?: React.ReactNode }
export declare function MacHelpTag(props: MacHelpTagProps): React.ReactElement;
export interface MacAlbumHeaderProps { title: string; artist?: string; year?: string; releases?: string; release?: string; edition?: string; copy?: string; /** Where the files are when only another device holds them; Play gives way to an inert Not on this device. */ unplayable?: string; /** Draws the help tag that names the device. */ unplayableHover?: boolean; options?: string; optionsOpen?: boolean; highlighted?: string; rating?: number | string; ratingWho?: string; /** The library's rating system for albums: `heart` shows one heart at three stars or more in place of the rating, `none` shows no rating. */ ratingSystem?: 'stars' | 'heart' | 'none'; }
export declare function MacAlbumHeader(props: MacAlbumHeaderProps): React.ReactElement;
export declare function MacAlbumCredits(props: { /** "Role: name" pairs separated by semicolons. */ credits: string }): React.ReactElement;
export declare function MacAlbumWorks(props: { /** "number|title|composer" entries separated by semicolons. */ works: string }): React.ReactElement;
export declare function MacSortPill(props: { field: string; direction?: 'up' | 'down'; removable?: boolean }): React.ReactElement;
export declare function MacPersonTag(props: { name: string; /** A playlist header's owner line. */ strong?: boolean }): React.ReactElement;
export declare function MacPlaylistHeader(props: Omit<MacPlaylistPageProps, 'children'>): React.ReactElement;
export declare function MacArtistAlbumGrid(props: { children?: React.ReactNode }): React.ReactElement;
export declare function MacArtistOtherNames(props: { value?: string }): React.ReactElement;
export declare function MacCoverTile(props: { name: string; image?: string; selected?: boolean }): React.ReactElement;
export declare function MacCoverPreview(props: { name: string; image?: string; source?: string; position?: number; count: number }): React.ReactElement;
export declare function MacThumbnailStrip(props: { images: string; selected?: string }): React.ReactElement;
export declare function MacStorageState(props: { state?: 'local' | 'pinned' | 'cloud' }): React.ReactElement;
export interface AndroidLibraryHeaderProps { view?: string; collapsed?: boolean; searching?: boolean; search?: string; sync?: 'moving' | 'attention' | 'paused'; avatar?: string; avatarImage?: string }
export declare function AndroidLibraryHeader(props: AndroidLibraryHeaderProps): React.ReactElement;
export declare function AndroidSearchBar(props: { search?: string }): React.ReactElement;
export declare function AndroidActivityButton(props: { state?: 'moving' | 'attention' | 'paused' }): React.ReactElement | null;
export declare function AndroidTransport(props: { playing?: boolean; waiting?: boolean; /** The full player's. */ large?: boolean }): React.ReactElement;
export declare function AndroidNowPlayingTrack(props: { title: string; artist?: string; album?: string; noCover?: boolean }): React.ReactElement;
export declare function AndroidTrackLead(props: { position?: string; current?: boolean; playing?: boolean }): React.ReactElement;
/** A grey bar where text or a picture will be while it loads. */
export declare function MacPlaceholderLine(props: { width: number | string; height: number | string; radius?: number | string; fixed?: boolean }): React.ReactElement;
/** The Settings window's list of panes, with the traffic lights inside it. */
export declare function MacSettingsSidebar(props: { pane?: string; status?: string; statusPane?: string; statusTone?: string }): React.ReactElement;
/** A Settings pane's heading, with the back button on a page inside a pane. */
export declare function MacPageHeading(props: { title: string; back?: string }): React.ReactElement;
/** One value of a file name pattern, as macOS's token field draws it. */
export declare function MacToken(props: { children?: React.ReactNode }): React.ReactElement;
/** A red line under a field that says what is wrong. */
export declare function MacErrorLine(props: { children?: React.ReactNode }): React.ReactElement;
/** A transfer in the Activity panel: label, count or hold, Pause or Resume, bar. */
export declare function MacTransferLine(props: { label: string; count?: string; progress?: number | string; pausable?: boolean; paused?: boolean }): React.ReactElement;
/** A transfer in Android's Activity sheet. */
export declare function AndroidTransferLine(props: { label: string; count?: string; progress?: number | string; pausable?: boolean; paused?: boolean; opens?: boolean }): React.ReactElement;
/** A release's files in its Storage sheet: "name|audio|size|kind" per file, separated by semicolons. */
export declare function MacReleaseFileTable(props: { files: string; sort?: string }): React.ReactElement;
/** Label and value pairs in two columns. */
export declare function MacFactsGrid(props: { facts: string | [string, string | undefined][]; numeric?: string; wide?: boolean }): React.ReactElement;
/** One transfer queue's rate or hold under the Storage Manager's list, with Pause or Resume. */
export declare function MacQueueRate(props: { kind: 'uploads' | 'pins' | 'exports'; rate?: string; wait?: string }): React.ReactElement;
/** The Storage Manager's toolbar. */
export declare function MacStorageToolbar(props: { filter?: 'All' | 'Local' | 'Cloud'; search?: string; inspector?: boolean; disabled?: boolean }): React.ReactElement;
/** One kind in a storage legend: dot, label, size. */
export declare function MacUsageKey(props: { label: string; size: string }): React.ReactElement;
/** One kind in a storage legend on Android. */
export declare function AndroidUsageKey(props: { label: string; size: string }): React.ReactElement;
export interface AndroidTopBarProps { title?: string; back?: boolean; backLabel?: string; menu?: string; more?: string }
export declare function AndroidTopBar(props: AndroidTopBarProps): React.ReactElement;
export interface MacSidePaneHeaderProps { tab?: 'Queue' | 'Playlists' | 'History'; back?: string }
export declare function MacSidePaneHeader(props: MacSidePaneHeaderProps): React.ReactElement;
export interface AndroidVolumeSliderProps { value?: number | string }
export declare function AndroidVolumeSlider(props: AndroidVolumeSliderProps): React.ReactElement;
export interface AndroidPlaceholderLineProps { width: number | string; height: number | string; radius?: number | string; fixed?: boolean }
export declare function AndroidPlaceholderLine(props: AndroidPlaceholderLineProps): React.ReactElement;
export interface AndroidAlbumCreditsProps { credits: string }
export interface AndroidAlbumAudioProps { /** The release's audio as bae read it, such as FLAC · 44.1 kHz · 16-bit · Stereo. */ children?: React.ReactNode }
export declare function AndroidAlbumAudio(props: AndroidAlbumAudioProps): React.ReactElement;
export declare function AndroidAlbumCredits(props: AndroidAlbumCreditsProps): React.ReactElement;
export interface AndroidArtistOtherNamesProps { names: string }
export declare function AndroidArtistOtherNames(props: AndroidArtistOtherNamesProps): React.ReactElement;
export interface AndroidPlaylistHeaderProps { name: string; facts?: string; covers?: string }
export declare function AndroidPlaylistHeader(props: AndroidPlaylistHeaderProps): React.ReactElement;
export interface AndroidPinsAtOnceProps { selected?: number | string; limit?: number | string }
export declare function AndroidPinsAtOnce(props: AndroidPinsAtOnceProps): React.ReactElement;
export interface AndroidReleasePickerProps { releases: string; selected?: string }
export declare function AndroidReleasePicker(props: AndroidReleasePickerProps): React.ReactElement;
export interface AndroidLightboxProps { title?: string; label?: string; page?: number | string; count?: number | string; loading?: boolean }
export declare function AndroidLightbox(props: AndroidLightboxProps): React.ReactElement;
export interface AndroidGalleryCaptionProps { label?: string; page?: number | string; count?: number | string }
export declare function AndroidGalleryCaption(props: AndroidGalleryCaptionProps): React.ReactElement;
export interface AndroidSidePauseProps { kind?: 'side' | 'disc'; label?: string; countdown?: number | string; unchecked?: boolean }
export declare function AndroidSidePause(props: AndroidSidePauseProps): React.ReactElement;
export interface AndroidErrorLineProps { children?: React.ReactNode }
export declare function AndroidErrorLine(props: AndroidErrorLineProps): React.ReactElement;
export interface AndroidItemProps { lead?: React.ReactNode; trail?: React.ReactNode; children?: React.ReactNode }
export declare function AndroidItem(props: AndroidItemProps): React.ReactElement;
export interface AndroidRecordLinksProps { records: string }
export declare function AndroidRecordLinks(props: AndroidRecordLinksProps): React.ReactElement;
export interface AndroidUnplayableLineProps { children?: React.ReactNode }
export declare function AndroidUnplayableLine(props: AndroidUnplayableLineProps): React.ReactElement;
/** Material's HorizontalDivider: a 1dp line across the screen or list. */
export declare function AndroidDivider(): React.ReactElement;
/** The album page's Files: the files no track plays from, under the Files eyebrow (files: "name|kind" entries separated by semicolons). */
export declare function AndroidReleaseFiles(props: { files: string }): React.ReactElement;
/** The camera screen that scans a restore code or an invite: the camera's picture, and at its foot the instructions over Cancel. */
export declare function AndroidQRScanner(props: { instructions?: string; width?: number | string; height?: number | string }): React.ReactElement;
/** The camera's picture in the Scan a code sheet: starting, camera access off, no camera, or the picture. */
export declare function MacQRScanner(props: { state?: 'starting' | 'denied' | 'unavailable' | 'camera' }): React.ReactElement;
