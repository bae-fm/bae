import type * as React from 'react';

// ---- macOS ----

/** The Settings window: the pane list on the left, the pane on the right. title heads the pane; back names the pane a Back button returns to. status puts a short status beside statusPane in the pane list, in statusTone. A MacSheet with overlay inside covers the whole window. */
export interface MacSettingsWindowProps { pane: 'Appearance' | 'Library' | 'Playback' | 'Transfers' | 'Import' | 'Formats' | 'Casting' | 'Subsonic' | 'Automation' | 'About'; title?: string; back?: string; statusPane?: string; status?: string; statusTone?: 'warning' | 'danger' | 'success' | 'activity'; width?: number; height?: number; children?: React.ReactNode }
export declare function MacSettingsWindow(props: MacSettingsWindowProps): React.ReactElement;

/** A plain macOS window with traffic lights and no sidebar (the welcome window). A MacAlert inside covers it. */
export interface MacWindowProps { width?: number; height?: number; children?: React.ReactNode }
export declare function MacWindow(props: MacWindowProps): React.ReactElement;

/** A macOS alert: title, message, and its buttons stacked full width as children. */
export interface MacAlertProps { title: string; message?: string; details?: string; children?: React.ReactNode }
export declare function MacAlert(props: MacAlertProps): React.ReactElement;

/** A segmented control; options is comma-separated, selected names the chosen one. */
export interface MacSegmentedControlProps { options: string; selected?: string }
export declare function MacSegmentedControl(props: MacSegmentedControlProps): React.ReactElement;

/** A heading over what follows it, usually one MacBox. */
export interface MacSectionProps { title?: string; children?: React.ReactNode }
export declare function MacSection(props: MacSectionProps): React.ReactElement;

/** A rounded group of rows; draws a hairline between its children. empty is the sentence shown when it has none. */
export interface MacBoxProps { elevated?: boolean; empty?: string; children?: React.ReactNode }
export declare function MacBox(props: MacBoxProps): React.ReactElement;

/** One line of a MacBox: label on the left, then value (grey, or in tone with a dot), controls passed as children, and a chevron when it opens something. action draws the label as an accent action ("Invite…"); destructive colors it danger, for an action that removes something. */
export interface MacRowProps { label?: string; destructive?: boolean; strong?: boolean; tag?: string; note?: string; noteTone?: 'danger' | 'warning' | 'success'; value?: string; tone?: 'success' | 'warning' | 'danger' | 'activity' | 'accent'; chevron?: boolean; selected?: boolean; action?: boolean; nested?: boolean; off?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function MacRow(props: MacRowProps): React.ReactElement;
export declare function MacItem(props: { lead?: React.ReactNode; trail?: React.ReactNode; children?: React.ReactNode }): React.ReactElement;

/** A MacBox row holding a field: the label in a column labelWidth px wide (90), the field filling the rest, then any controls. Leave out label for a field with its label inside. alignText moves the field left by its inner padding so its text lines up with the rows around it. */
export interface MacFieldRowProps { label?: string; labelWidth?: number; alignText?: boolean; children?: React.ReactNode }
export declare function MacFieldRow(props: MacFieldRowProps): React.ReactElement;

/** kind: primary is the one action a screen is for; secondary is everything else; borderless is words with no chrome, for an action at the end of a row; icon is a glyph-only button (label is its accessible name). destructive colors the words danger. */
export interface MacButtonProps { kind?: 'primary' | 'secondary' | 'borderless' | 'icon'; icon?: 'copy' | 'check'; size?: 'small' | 'regular' | 'large'; label?: string; destructive?: boolean; muted?: boolean; off?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function MacButton(props: MacButtonProps): React.ReactElement;

/** A text field with its label on the left (labelWidth px), or the label as a placeholder (labelHidden). note is a grey line under it; error a red one, shown when state is error. Leave out label when a MacFieldRow holds the field and gives the label. Children sit after the field. */
export interface MacTextFieldProps { label?: string; value?: string; placeholder?: string; state?: 'normal' | 'focused' | 'off' | 'error'; secret?: boolean; note?: string; error?: string; labelHidden?: boolean; labelWidth?: number; children?: React.ReactNode }
export declare function MacTextField(props: MacTextFieldProps): React.ReactElement;

/** One choice of several, as a row of a MacBox. */
export interface MacRadioProps { label: string; note?: string; checked?: boolean; onClick?: () => void }
export declare function MacRadio(props: MacRadioProps): React.ReactElement;

/** A pop-up menu showing its current choice. */
export interface MacPopUpButtonProps { value?: string; width?: number; off?: boolean; children?: React.ReactNode }
export declare function MacPopUpButton(props: MacPopUpButtonProps): React.ReactElement;

/** An on/off switch; label is its accessible name. */
export interface MacSwitchProps { on?: boolean; label: string; off?: boolean }
export declare function MacSwitch(props: MacSwitchProps): React.ReactElement;

/** The eight accent swatches in the order the appearance settings offer them; selected names the chosen one. */
export interface MacAccentPickerProps { selected?: 'blue' | 'orchid' | 'pink' | 'coral' | 'gold' | 'lime' | 'mint' | 'sky'; label?: string }
export declare function MacAccentPicker(props: MacAccentPickerProps): React.ReactElement;

/** A menu as it shows when open: MacMenuItems in a panel. */
export interface MacMenuProps { children?: React.ReactNode }
export declare function MacMenu(props: MacMenuProps): React.ReactElement;

/** One choice in a MacMenu: checked marks the current choice, highlighted the one under the pointer. */
export interface MacMenuItemProps { checked?: boolean; highlighted?: boolean; children?: React.ReactNode }
export declare function MacMenuItem(props: MacMenuItemProps): React.ReactElement;

/** A ▸ label that shows its children when opened. open sets how it starts. */
export interface MacDisclosureProps { label?: string; open?: boolean; children?: React.ReactNode }
export declare function MacDisclosure(props: MacDisclosureProps): React.ReactElement;

/** A sheet: title, divider, content, and a MacSheetFooter last. overlay dims what is under it and sits it at the top. */
export interface MacSheetProps { title?: string; overlay?: boolean; width?: number; children?: React.ReactNode }
export declare function MacSheet(props: MacSheetProps): React.ReactElement;

/** A sheet's buttons, on the right. split puts the first child on the left. */
export interface MacSheetFooterProps { split?: boolean; children?: React.ReactNode }
export declare function MacSheetFooter(props: MacSheetFooterProps): React.ReactElement;

/** A step that waits: its words over a bar. Without value the bar moves; with value (0 to 1) it fills. */
export interface MacProgressProps { label?: string; value?: number }
export declare function MacProgress(props: MacProgressProps): React.ReactElement;
export interface MacSpinnerProps { label?: string; size?: number }
export declare function MacSpinner(props: MacSpinnerProps): React.ReactElement;

/** A grey line under a box or control. */
export interface MacNoteProps { children?: React.ReactNode }
export declare function MacNote(props: MacNoteProps): React.ReactElement;

// ---- Android ----

/** A screen: the top bar with Back and the title, then the content. Without title, no top bar. A dialog inside covers the whole screen. */
export interface AndroidScreenProps { title?: string; back?: boolean; width?: number; height?: number; children?: React.ReactNode }
export declare function AndroidScreen(props: AndroidScreenProps): React.ReactElement;

/** A label in capitals over the card that follows. */
export interface AndroidSectionProps { title?: string; children?: React.ReactNode }
export declare function AndroidSection(props: AndroidSectionProps): React.ReactElement;

/** A rounded group of rows; draws a hairline between its children. */
export interface AndroidCardProps { children?: React.ReactNode }
export declare function AndroidCard(props: AndroidCardProps): React.ReactElement;

/** One line of a card: title (with a tag chip after it), a subtitle under it (in tone with a dot), controls as children, and an icon on the right. action draws the title as an accent action; destructive colors it danger, for an action that removes something. */
export interface AndroidRowProps { title: string; tag?: string; subtitle?: string; tone?: 'success' | 'warning' | 'danger' | 'activity' | 'accent'; icon?: 'chevron' | 'edit' | 'expand' | 'more' | 'refresh'; leadingIcon?: 'add'; action?: boolean; destructive?: boolean; selected?: boolean; nested?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function AndroidRow(props: AndroidRowProps): React.ReactElement;

/** kind: filled for the action a screen is for, outlined beside it, text in dialogs and rows. large is the full-width size on the welcome screen. */
export interface AndroidButtonProps { kind?: 'filled' | 'outlined' | 'text'; label?: string; large?: boolean; alignTo?: 'text' | 'field'; destructive?: boolean; off?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function AndroidButton(props: AndroidButtonProps): React.ReactElement;

/** A 48 dp glyph button: a Material Symbol by name (back, close, more, search, …). label is its accessible name. */
export interface AndroidIconButtonProps { icon: string; label?: string; size?: number; tonal?: boolean }
export declare function AndroidIconButton(props: AndroidIconButtonProps): React.ReactElement;

/** An on/off switch; label is its accessible name. */
export interface AndroidSwitchProps { on?: boolean; label: string }
export declare function AndroidSwitch(props: AndroidSwitchProps): React.ReactElement;

/** The eight accent swatches in the order the appearance settings offer them; selected names the chosen one. */
export interface AndroidAccentPickerProps { selected?: 'blue' | 'orchid' | 'pink' | 'coral' | 'gold' | 'lime' | 'mint' | 'sky'; label?: string }
export declare function AndroidAccentPicker(props: AndroidAccentPickerProps): React.ReactElement;

/** An outlined text field with its label on the border. helper is a grey line under it; error a red one, shown when state is error. */
export interface AndroidTextFieldProps { label: string; value?: string; state?: 'normal' | 'focused' | 'error'; helper?: string; error?: string }
export declare function AndroidTextField(props: AndroidTextFieldProps): React.ReactElement;

/** One choice of several, in a dialog. */
export interface AndroidRadioProps { label: string; checked?: boolean }
export declare function AndroidRadio(props: AndroidRadioProps): React.ReactElement;

/** A dialog over a dimmed screen: title, content, and AndroidDialogActions last. */
export interface AndroidDialogProps { title?: string; width?: number; children?: React.ReactNode }
export declare function AndroidDialog(props: AndroidDialogProps): React.ReactElement;

/** A dialog's buttons, on the right. */
export interface AndroidDialogActionsProps { children?: React.ReactNode }
export declare function AndroidDialogActions(props: AndroidDialogActionsProps): React.ReactElement;

/** A grey line under a card. */
export interface AndroidNoteProps { children?: React.ReactNode }
export declare function AndroidNote(props: AndroidNoteProps): React.ReactElement;

/** What is happening now, over a bar. value from 0 to 1 when the length is known. */
export interface AndroidProgressProps { label?: string; value?: number }
export declare function AndroidProgress(props: AndroidProgressProps): React.ReactElement;
export interface AndroidSpinnerProps { label?: string; size?: number }
export declare function AndroidSpinner(props: AndroidSpinnerProps): React.ReactElement;

// ---- Both ----

export type StatusTone = 'neutral' | 'accent' | 'info' | 'success' | 'warning' | 'danger' | 'activity';
/** A short label on its tone's tint: a status, a count, a tag or a role. */
export interface StatusChipProps { tone?: StatusTone; children?: React.ReactNode }
export declare function StatusChip(props: StatusChipProps): React.ReactElement;

/** A code to scan, centered: what to do, the QR code, the buttons that hand over the code (children), then when it ends. size is the QR code's width in points. */
export interface MacScanCodeProps { instruction?: string; note?: string; size?: number; children?: React.ReactNode }
export declare function MacScanCode(props: MacScanCodeProps): React.ReactElement;
export interface AndroidScanCodeProps { instruction?: string; note?: string; size?: number; children?: React.ReactNode }
export declare function AndroidScanCode(props: AndroidScanCodeProps): React.ReactElement;

/** A message on its tone's tint. */
/** A tinted box in a tone, led by the tone's glyph unless `icon` is false; `set: 'mac'` draws macOS's glyph, top-aligned. */
export interface NoticeProps { tone?: StatusTone; title?: string; subtitle?: string; action?: string; secondary?: string; layout?: 'paragraph' | 'centered'; buttons?: 'inline' | 'below'; dismiss?: boolean; quiet?: boolean; stage?: boolean; icon?: boolean; children?: React.ReactNode }
/** A tone's glyph in the tone's colour, a filled circle with its mark; `set: 'mac'` draws the macOS stand-in. Neutral, accent and activity draw nothing. */
/** Material's disclosure row: its own card, expand_more or expand_less at the end; the AndroidRows it shows join the card. */
export declare function AndroidDisclosure(props: { label: string; summary?: string; open?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function StatusGlyph(props: { tone: StatusTone; set?: 'mac'; outline?: boolean; muted?: boolean; size?: number; className?: string }): React.ReactElement | null;

/** A cover or picture. Boards give each title its own stand-in colors; `missing` is a plain tile. */
export interface ArtworkProps { title?: string; seed?: string; size?: number; radius?: number; round?: boolean; missing?: boolean; shadow?: 'tile' | 'bar'; label?: string; children?: React.ReactNode }
export declare function Artwork(props: ArtworkProps): React.ReactElement;
export interface NowPlayingProps { trackTitle?: string; trackArtist?: string; trackAlbum?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; repeat?: boolean; casting?: string }
export interface MacMainWindowProps extends NowPlayingProps { section?: 'Library' | 'Import'; search?: string; searchFocused?: boolean; sync?: 'moving' | 'attention' | 'paused'; shuffle?: boolean; queueOpen?: boolean; width?: number; height?: number; children?: React.ReactNode }
export declare function MacMainWindow(props: MacMainWindowProps): React.ReactElement;
export interface MacNowPlayingBarProps { title?: string; artist?: string; album?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; shuffle?: boolean; repeat?: boolean; casting?: string; queueOpen?: boolean; volume?: number; noCover?: boolean }
export declare function MacNowPlayingBar(props: MacNowPlayingBarProps): React.ReactElement;
export interface SeekBarProps { position?: number; held?: number | string; elapsed?: string; duration?: string; off?: boolean }
export declare function MacSeekBar(props: SeekBarProps): React.ReactElement;
export declare function AndroidSeekBar(props: SeekBarProps): React.ReactElement;
export interface MacPlayPauseProps { playing?: boolean; waiting?: boolean }
export declare function MacPlayPause(props: MacPlayPauseProps): React.ReactElement;
export interface MacIconButtonProps { icon: string; label?: string; active?: boolean; faint?: boolean; small?: boolean; boxed?: boolean; size?: number }
export declare function MacIconButton(props: MacIconButtonProps): React.ReactElement;
export interface MacLibraryHeadingProps { view?: 'Albums' | 'Artists' | 'Composers'; collapsed?: boolean; children?: React.ReactNode }
export declare function MacLibraryHeading(props: MacLibraryHeadingProps): React.ReactElement;
export interface MacAlbumGridProps { columns?: number; children?: React.ReactNode }
export declare function MacAlbumGrid(props: MacAlbumGridProps): React.ReactElement;
export interface MacAlbumTileProps { title: string; artist?: string; year?: string; noCover?: boolean; hover?: boolean; open?: boolean; selected?: boolean }
export declare function MacAlbumTile(props: MacAlbumTileProps): React.ReactElement;
export interface MacTrackRowProps { position?: string; title: string; artist?: string; duration?: string; current?: boolean; playing?: boolean; hover?: boolean }
export declare function MacTrackRow(props: MacTrackRowProps): React.ReactElement;
export declare function MacEyebrow(props: { children?: React.ReactNode }): React.ReactElement;
export interface MacBrowseRowProps { name?: string; count?: string; noImage?: boolean; hover?: boolean; selected?: boolean; loading?: boolean }
export declare function MacBrowseRow(props: MacBrowseRowProps): React.ReactElement;
export interface MacDetailHeaderProps { name: string; count?: string; noImage?: boolean }
export declare function MacDetailHeader(props: MacDetailHeaderProps): React.ReactElement;
export interface MacQueueRowProps { title: string; artist?: string; album?: string; duration?: string; hover?: boolean }
export declare function MacQueueRow(props: MacQueueRowProps): React.ReactElement;
export interface AndroidLibraryScreenProps extends NowPlayingProps { view?: 'Albums' | 'Artists' | 'Composers'; collapsed?: boolean; searching?: boolean; search?: string; sync?: 'moving' | 'attention' | 'paused'; width?: number; height?: number; children?: React.ReactNode }
export declare function AndroidLibraryScreen(props: AndroidLibraryScreenProps): React.ReactElement;
export interface AndroidDetailScreenProps extends NowPlayingProps { back?: string; menu?: boolean; flush?: boolean; width?: number; height?: number; children?: React.ReactNode }
export declare function AndroidDetailScreen(props: AndroidDetailScreenProps): React.ReactElement;
export interface AndroidNowPlayingBarProps { title?: string; artist?: string; album?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; repeat?: boolean; casting?: string; castOff?: boolean }
export declare function AndroidNowPlayingBar(props: AndroidNowPlayingBarProps): React.ReactElement;
export interface AndroidPlayPauseProps { playing?: boolean; waiting?: boolean; size?: number }
export declare function AndroidPlayPause(props: AndroidPlayPauseProps): React.ReactElement;
export declare function AndroidAlbumGrid(props: { children?: React.ReactNode }): React.ReactElement;
export interface AndroidAlbumTileProps { title: string; line?: string; noCover?: boolean }
export declare function AndroidAlbumTile(props: AndroidAlbumTileProps): React.ReactElement;
export interface AndroidArtistHeadingProps { name: string; count?: string; pinned?: boolean; noImage?: boolean }
export declare function AndroidArtistHeading(props: AndroidArtistHeadingProps): React.ReactElement;
export interface AndroidTrackRowProps { position?: string; title: string; artist?: string; duration?: string; current?: boolean; playing?: boolean }
export declare function AndroidTrackRow(props: AndroidTrackRowProps): React.ReactElement;
export declare function AndroidEyebrow(props: { children?: React.ReactNode }): React.ReactElement;
export interface AndroidBrowseRowProps { name: string; count?: string; noImage?: boolean }
export declare function AndroidBrowseRow(props: AndroidBrowseRowProps): React.ReactElement;
export interface AndroidAlbumHeaderProps { title: string; artist?: string; year?: string; pressing?: string; labels?: string; noCover?: boolean }
export declare function AndroidAlbumHeader(props: AndroidAlbumHeaderProps): React.ReactElement;
export interface AndroidDownloadControlProps { state?: 'available' | 'queued' | 'downloading' | 'failed' | 'downloaded' | 'copying' | 'elsewhere'; progress?: number; bytes?: string; where?: string }
export declare function AndroidDownloadControl(props: AndroidDownloadControlProps): React.ReactElement;
export declare function AndroidUploadLine(props: { phase?: string; missing?: string }): React.ReactElement;
export interface MacAlbumPanelProps { title: string; artist?: string; year?: string; releases?: string; release?: string; pressing?: string; labels?: string; unplayable?: string; noCover?: boolean; coverSize?: number; children?: React.ReactNode }
export declare function MacAlbumPanel(props: MacAlbumPanelProps): React.ReactElement;
export declare function MacTrackColumns(props: { children?: React.ReactNode }): React.ReactElement;
export declare function MacBrowseSplit(props: { children?: React.ReactNode }): React.ReactElement;
export interface MacArtistAlbumTileProps { title: string; year?: string; noCover?: boolean }
export declare function MacArtistAlbumTile(props: MacArtistAlbumTileProps): React.ReactElement;
export interface MacSortCriteriaProps { criteria?: string; full?: boolean }
export declare function MacSortCriteria(props: MacSortCriteriaProps): React.ReactElement;
export interface MacCheckboxProps { label: string; on?: boolean }
export declare function MacCheckbox(props: MacCheckboxProps): React.ReactElement;
export declare function MacArtistGroupHeading(props: { name: string }): React.ReactElement;
export interface AndroidMenuProps { top?: number; left?: number; right?: number; width?: number; children?: React.ReactNode }
export declare function AndroidMenu(props: AndroidMenuProps): React.ReactElement;
export interface AndroidMenuItemProps { selected?: boolean; check?: boolean; heading?: boolean; divider?: boolean; pressed?: boolean; children?: React.ReactNode }
export declare function AndroidMenuItem(props: AndroidMenuItemProps): React.ReactElement;
export interface MacSearchCardProps { top?: number; right?: number; empty?: string; children?: React.ReactNode }
export declare function MacSearchCard(props: MacSearchCardProps): React.ReactElement;
export declare function MacSearchSection(props: { title: string }): React.ReactElement;
/** part (Android) indents a work under its group's heading work; inset (Android) is the composer and work pages' rows today, whose text carries a second 16dp padding inside the row's own. */
export interface SearchRowProps { title: string; subtitle?: string; trailing?: string; art?: string; noArt?: boolean; hover?: boolean; part?: boolean; inset?: boolean }
export declare function MacSearchRow(props: SearchRowProps): React.ReactElement;
export declare function AndroidSearchSection(props: { title: string }): React.ReactElement;
export declare function AndroidSearchRow(props: SearchRowProps): React.ReactElement;
export interface MacQueuePanelProps { title?: string; artist?: string; album?: string; playing?: boolean; position?: number; elapsed?: string; empty?: boolean; children?: React.ReactNode }
export declare function MacQueuePanel(props: MacQueuePanelProps): React.ReactElement;
export interface MacQueueSectionProps { title: string; clear?: boolean; shuffle?: boolean }
export declare function MacQueueSection(props: MacQueueSectionProps): React.ReactElement;
export declare function MacQueuePlaceholderRow(props: {}): React.ReactElement;
export declare function MacQueueInsertLine(props: {}): React.ReactElement;
export interface SidePauseProps { kind?: 'side' | 'disc'; label?: string; countdown?: number; unchecked?: boolean }
export declare function MacSidePause(props: SidePauseProps): React.ReactElement;
export interface MacCastPopoverProps { devices?: string; casting?: string; connecting?: string; right?: number; bottom?: number }
export declare function MacCastPopover(props: MacCastPopoverProps): React.ReactElement;
export declare function AndroidSheet(props: { top?: number; title?: string; children?: React.ReactNode }): React.ReactElement;
export interface AndroidQueueLabelProps { title: string; clear?: boolean; shuffle?: boolean }
export declare function AndroidQueueLabel(props: AndroidQueueLabelProps): React.ReactElement;
export interface AndroidQueueRowProps { title: string; artist?: string; album?: string; duration?: string; current?: boolean; dragging?: boolean }
export declare function AndroidQueueRow(props: AndroidQueueRowProps): React.ReactElement;
export interface AndroidFullPlayerProps { title: string; artist?: string; album?: string; playing?: boolean; waiting?: boolean; position?: number; held?: number | string; elapsed?: string; duration?: string; repeat?: boolean }
export declare function AndroidFullPlayer(props: AndroidFullPlayerProps): React.ReactElement;
export interface AndroidCastRowProps { name?: string; kind?: 'cast' | 'airplay' | 'upnp'; active?: boolean; casting?: string; waiting?: string }
export declare function AndroidCastRow(props: AndroidCastRowProps): React.ReactElement;
export interface AndroidCheckboxProps { label: string; on?: boolean }
export declare function AndroidCheckbox(props: AndroidCheckboxProps): React.ReactElement;
/** A row of a composer's page or a work's: cover, title, second line. credit is the text-only row; part indents a work; top is the gap above it. */
export interface MacDetailRowProps { title: string; subtitle?: string; art?: string; noArt?: boolean; credit?: boolean; part?: boolean; top?: number; hover?: boolean }
export declare function MacDetailRow(props: MacDetailRowProps): React.ReactElement;
export interface MacSectionHeaderProps { title: string; top?: number; bottom?: number }
export declare function MacSectionHeader(props: MacSectionHeaderProps): React.ReactElement;
export declare function MacWorkDetail(props: { title: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacHairline(props: {}): React.ReactElement;
export declare function MacContentUnavailable(props: { title: string; description?: string }): React.ReactElement;
export declare function Sym(props: { name: string; size?: number; weight?: number }): React.ReactElement;
export declare function MacStorageWindow(props: { filter?: 'All' | 'Local' | 'Cloud'; search?: string; inspector?: boolean; width?: number; height?: number; children?: React.ReactNode }): React.ReactElement;
export declare function MacTable(props: { columns: string; empty?: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacTableRow(props: { selected?: boolean; placeholder?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function MacReleaseCell(props: { title: string }): React.ReactElement;
export declare function MacStorageChip(props: { state?: 'local' | 'pinned' | 'cloud' | 'uploading' | 'pinning' | 'detaching' | 'downloadFailed' | 'moveFailed' | 'makeLocalFailed' | 'queued' | 'uploadQueued' | 'uploadBlocked' | 'makeLocalQueued'; progress?: number; file?: string }): React.ReactElement;
export declare function MacDownloadRow(props: { kind?: 'pin' | 'makeLocal'; title?: string; detail?: string; ago?: string }): React.ReactElement;
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
export declare function MacActivity(props: { children?: React.ReactNode; attention?: string; record?: string; reason?: string; error?: string; syncing?: boolean; uploads?: string; uploadProgress?: number; uploadsWait?: string; pins?: string; pinProgress?: number; makingLocal?: string; makeLocalProgress?: number; downloadsWait?: string }): React.ReactElement;
export declare function AndroidNotice(props: NoticeProps): React.ReactElement;
export declare function AndroidActivity(props: { children?: React.ReactNode; attention?: string; record?: string; reason?: string; error?: string; syncing?: boolean; uploads?: string; uploadProgress?: number; uploadsWait?: string; pins?: string; pinProgress?: number; downloadsWait?: string }): React.ReactElement;
export declare function AndroidSegmentedButtons(props: { options: string; selected?: string }): React.ReactElement;
export declare function AndroidDownloadsBar(props: { failed?: boolean; paused?: boolean; idle?: boolean }): React.ReactElement;
export declare function AndroidDownloadRow(props: { title?: string; detail?: string; state?: 'queued' | 'active' | 'failed'; progress?: number; bytes?: string }): React.ReactElement;
export declare function AndroidSnackbar(props: { message: string; action?: string; close?: boolean }): React.ReactElement;
export declare function AndroidErrorBanner(props: { message: string }): React.ReactElement;
export declare function MacImportSidebar(props: { tab?: 'Found' | 'Imported' | 'Skipped'; found?: string; imported?: string; skipped?: string; filter?: string; narrowed?: string; identifying?: number; importing?: boolean; scanning?: boolean; filtered?: boolean; empty?: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacImportGroupHeader(props: { name: string; closed?: boolean }): React.ReactElement;
export declare function MacImportRow(props: { title?: string; artist?: string; folder?: string; cover?: string; noCover?: boolean; record?: boolean; badge?: string; badgeTone?: 'warning' | 'danger'; state?: 'identifyQueued' | 'identifying' | 'importQueued' | 'importing' | 'uploading'; step?: string; progress?: number; error?: string; grouped?: boolean; selected?: boolean }): React.ReactElement;
export declare function MacImportFolderLine(props: { tab?: string; folder: string }): React.ReactElement;
export declare function MacImportDraft(props: { title?: string; artist?: string; artistChip?: string; year?: string; audio?: string; cover?: string; fields?: string; records?: string; check?: string; cloud?: boolean; retry?: boolean; blank?: boolean; editor?: boolean; changed?: boolean }): React.ReactElement;
export declare function MacImportTable(props: { children?: React.ReactNode }): React.ReactElement;
export declare function MacImportTrackRow(props: { file: string; entry?: boolean; evidence?: string; position?: string; title?: string; artist?: string; inherited?: boolean; length?: string; mismatch?: boolean; playing?: boolean }): React.ReactElement;
export declare function MacImportSheetCaption(props: { disc?: string; sheet: string; bound?: string; unbound?: boolean }): React.ReactElement;
export declare function MacFindOnline(props: { open?: 'Automatic' | 'Search'; discogsBar?: boolean; autoStatus?: 'working' | 'failed'; dimmed?: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacIdentifierBand(props: { chips: string }): React.ReactElement;
export declare function MacReleaseCard(props: { title: string; artist?: string; label?: string; sources?: string; children?: React.ReactNode }): React.ReactElement;
export declare function MacPressingRow(props: { year?: string; labels?: string; catalog?: string; badges?: string; summary?: string; sources?: string; inLibrary?: boolean; selected?: boolean }): React.ReactElement;
export declare function MacImportBulk(props: { count?: number; ready?: number; running?: string }): React.ReactElement;
export declare function MacModal(props: { width?: number; height?: number; children?: React.ReactNode }): React.ReactElement;
export declare function MacModalHeader(props: { title: string; subtitle?: string; large?: boolean; children?: React.ReactNode }): React.ReactElement;
export declare function MacCoverGallery(props: { title?: string; selected?: string; source?: string; current?: string; remote?: 'unlinked'; remoteNames?: string; files?: string }): React.ReactElement;
export declare function MacLightbox(props: { title?: string; label?: string; source?: string; browse?: boolean; state?: 'loading' | 'failed' }): React.ReactElement;
export declare function MacStorageBand(props: { state?: 'local' | 'pinned' | 'cloud' | 'uploading' | 'waiting'; missing?: string; download?: 'queued' | 'downloading' | 'failed' | 'copying'; makeLocal?: 'queued' | 'making' | 'failed'; progress?: number; why?: string; actions?: string }): React.ReactElement;
export declare function Stars(props: { value?: number; size?: number; who?: string }): React.ReactElement;
export declare function Avatar(props: { name: string; image?: string; size?: number }): React.ReactElement;
export declare function MacListRow(props: { lead?: string; title: string; sub?: string; album?: string; who?: string; stars?: number; duration?: string; selected?: boolean }): React.ReactElement;
export declare function AndroidListRow(props: { title: string; sub?: string; album?: string; who?: string; trailing?: string; selected?: boolean }): React.ReactElement;

// ---- Logo and app icons ----

/** The logo in one of its three forms: pair is the sheep over the word, sheep the sheep alone, word the word alone, small the drawing for 32 pixels and under. size is the sheep's width. */
export declare function Logo(props: { kind?: 'pair' | 'inline' | 'sheep' | 'word' | 'small'; size?: number; bob?: boolean; glow?: boolean }): React.ReactElement;
/** An app icon as a platform shows it. macOS, iOS and Android put the sheep on the pale pink tile; Windows and Linux show it with no tile. mask is Android's launcher shape. */
export declare function AppIcon(props: { platform?: 'macos' | 'ios' | 'android' | 'windows' | 'linux'; size?: number; mask?: 'circle' | 'squircle' | 'rounded' }): React.ReactElement;

// ---- The sheep on the empty Import screen ----

/** The sheep on the empty Import screen. Every few seconds it plays one moment from a fixed sequence, then rests: it gobbles a line of releases from the left or the right, catches one falling from above, breathes one in, bops to its music, or rolls on the spot. size is the sheep's width. still holds the pose it shows when the person has asked for reduced motion. */
export declare function ImportSheep(props: { size?: number; still?: boolean }): React.ReactElement;
