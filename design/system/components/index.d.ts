import type * as React from 'react';

// ---- macOS ----

/** The Settings window: the pane list on the left, the pane on the right. title heads the pane; back names the pane a Back button returns to. A MacSheet with overlay inside covers the whole window. */
export interface MacSettingsWindowProps { pane: 'Appearance' | 'Library' | 'Playback' | 'Transfers' | 'Import' | 'Formats' | 'Casting' | 'Subsonic' | 'Automation' | 'About'; title?: string; back?: string; width?: number; height?: number; children?: React.ReactNode }
export declare function MacSettingsWindow(props: MacSettingsWindowProps): React.ReactElement;

/** A plain macOS window with traffic lights and no sidebar (the welcome window). A MacAlert inside covers it. */
export interface MacWindowProps { width?: number; height?: number; children?: React.ReactNode }
export declare function MacWindow(props: MacWindowProps): React.ReactElement;

/** A macOS alert: title, message, and its buttons stacked full width as children. */
export interface MacAlertProps { title: string; message?: string; children?: React.ReactNode }
export declare function MacAlert(props: MacAlertProps): React.ReactElement;

/** A segmented control; options is comma-separated, selected names the chosen one. */
export interface MacSegmentedProps { options: string; selected?: string }
export declare function MacSegmented(props: MacSegmentedProps): React.ReactElement;

/** A heading over what follows it, usually one MacBox. */
export interface MacSectionProps { title?: string; children?: React.ReactNode }
export declare function MacSection(props: MacSectionProps): React.ReactElement;

/** A rounded group of rows; draws a hairline between its children. empty is the sentence shown when it has none. */
export interface MacBoxProps { elevated?: boolean; empty?: string; children?: React.ReactNode }
export declare function MacBox(props: MacBoxProps): React.ReactElement;

/** One line of a MacBox: label on the left, then value (grey, or in tone with a dot), controls passed as children, and a chevron when it opens something. action draws the label as an accent action ("Add device…"). */
export interface MacRowProps { label: string; strong?: boolean; tag?: string; note?: string; noteTone?: 'danger' | 'warning' | 'success'; waiting?: boolean; value?: string; tone?: 'success' | 'warning' | 'danger' | 'activity' | 'accent'; chevron?: boolean; selected?: boolean; action?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function MacRow(props: MacRowProps): React.ReactElement;

/** kind: primary is the one action a screen is for; secondary is everything else; icon is a glyph-only button (label is its accessible name). destructive colours the words danger. */
export interface MacButtonProps { kind?: 'primary' | 'secondary' | 'icon'; label?: string; destructive?: boolean; off?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function MacButton(props: MacButtonProps): React.ReactElement;

/** A text field with its label on the left (labelWidth px), or the label as a placeholder (labelHidden). note is a grey line under it; error a red one, shown when state is error. row draws it as a row of a MacBox. Children sit after the field (a Create button). */
export interface MacTextFieldProps { label: string; value?: string; placeholder?: string; state?: 'normal' | 'focused' | 'off' | 'error'; secret?: boolean; note?: string; error?: string; labelHidden?: boolean; labelWidth?: number; row?: boolean; children?: React.ReactNode }
export declare function MacTextField(props: MacTextFieldProps): React.ReactElement;

/** One choice of several, as a row of a MacBox. */
export interface MacRadioProps { label: string; note?: string; checked?: boolean; onClick?: () => void }
export declare function MacRadio(props: MacRadioProps): React.ReactElement;

/** A pop-up menu showing its current choice. */
export interface MacPopUpProps { value?: string; width?: number; off?: boolean; children?: React.ReactNode }
export declare function MacPopUp(props: MacPopUpProps): React.ReactElement;

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

/** A line saying what failed and what to do. */
export interface MacErrorTextProps { children?: React.ReactNode }
export declare function MacErrorText(props: MacErrorTextProps): React.ReactElement;

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

/** A rounded group of rows; draws a hairline between its children, except under a card heading. */
export interface AndroidCardProps { children?: React.ReactNode }
export declare function AndroidCard(props: AndroidCardProps): React.ReactElement;

/** A small heading inside a card, over the rows it names. */
export interface AndroidCardHeadingProps { title?: string; children?: React.ReactNode }
export declare function AndroidCardHeading(props: AndroidCardHeadingProps): React.ReactElement;

/** One line of a card: title, a subtitle under it (in tone with a dot), controls as children, and an icon on the right. action draws the title as an accent action. */
export interface AndroidRowProps { title: string; subtitle?: string; tone?: 'success' | 'warning' | 'danger' | 'activity' | 'accent'; icon?: 'chevron' | 'edit' | 'expand' | 'more' | 'refresh'; action?: boolean; selected?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function AndroidRow(props: AndroidRowProps): React.ReactElement;

/** kind: filled for the action a screen is for, outlined beside it, text in dialogs and rows. large is the full-width size on the welcome screen. */
export interface AndroidButtonProps { kind?: 'filled' | 'outlined' | 'text'; label?: string; large?: boolean; destructive?: boolean; off?: boolean; onClick?: () => void; children?: React.ReactNode }
export declare function AndroidButton(props: AndroidButtonProps): React.ReactElement;

/** A 48 dp glyph button. label is its accessible name. */
export interface AndroidIconButtonProps { icon: 'back' | 'edit' | 'chevron' | 'expand' | 'more' | 'search' | 'refresh'; label?: string }
export declare function AndroidIconButton(props: AndroidIconButtonProps): React.ReactElement;

/** An on/off switch; label is its accessible name. */
export interface AndroidSwitchProps { on?: boolean; label: string }
export declare function AndroidSwitch(props: AndroidSwitchProps): React.ReactElement;

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

// ---- Both ----

export type StatusTone = 'neutral' | 'accent' | 'info' | 'success' | 'warning' | 'danger' | 'activity';
/** A short label on its tone's tint: a status, a count, a tag or a role. */
export interface StatusChipProps { tone?: StatusTone; children?: React.ReactNode }
export declare function StatusChip(props: StatusChipProps): React.ReactElement;

/** A message on its tone's tint. */
export interface NoticeProps { tone?: StatusTone; children?: React.ReactNode }
export declare function Notice(props: NoticeProps): React.ReactElement;
